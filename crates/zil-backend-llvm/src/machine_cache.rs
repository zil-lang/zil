//! One bounded raw model dictionary for class and partition products. Names and
//! linkage are reference fields; immutable bodies and unchanged encoding bytes
//! are shared. This dictionary contains no checker or source authority.
use crate::emit::raw_codec::{self, Reader, Writer};
use std::collections::HashMap;
use zil_base::Limits;
use zil_llvm_api::model::{Block, Function, Item, Linkage, Module};

type Owner = Option<*const Vec<Block>>;
fn owner(function: &Function) -> Owner {
    function
        .blocks
        .as_ref()
        .map(|blocks| std::ptr::from_ref(&**blocks))
}
fn same_model(a: &Function, b: &Function) -> bool {
    a.signature == b.signature && a.attrs == b.attrs && a.blocks == b.blocks
}
fn linkage(value: Linkage) -> usize {
    match value {
        Linkage::External => 0,
        Linkage::Internal => 1,
        Linkage::Private => 2,
        Linkage::AvailableExternally => 3,
    }
}

pub(crate) struct Encoder<'a> {
    pub(crate) out: Writer,
    functions: Vec<(Function, Option<&'a [u8]>)>,
    owners: HashMap<Owner, Vec<usize>>,
    retained: HashMap<Owner, Vec<(Function, &'a [u8])>>,
    previous: Option<&'a Decoder<'a>>,
}
impl<'a> Encoder<'a> {
    pub(crate) fn new() -> Self {
        Self {
            out: Writer::new(),
            functions: vec![],
            owners: HashMap::new(),
            retained: HashMap::new(),
            previous: None,
        }
    }
    pub(crate) fn with_previous(mut self, previous: &'a Decoder<'_>) -> Self {
        self.previous = Some(previous);
        for (index, function) in previous.functions.iter().enumerate() {
            if let Some(function) = function {
                self.retained
                    .entry(owner(function))
                    .or_default()
                    .push((function.clone(), previous.encoded[index]));
            }
        }
        self
    }
    pub(crate) fn function(&mut self, function: &Function) -> Option<()> {
        let body = owner(function);
        let index = self.owners.get(&body).and_then(|indices| {
            indices
                .iter()
                .copied()
                .find(|&i| same_model(&self.functions[i].0, function))
        });
        let index = if let Some(index) = index {
            index
        } else {
            let index = self.functions.len();
            if index >= 32_768 {
                return None;
            }
            // Both maps retain strong owners. A different allocation misses;
            // every header/attribute is compared before raw bytes are retained.
            let encoded = self
                .retained
                .get(&body)
                .and_then(|rows| {
                    rows.iter()
                        .find(|(old, _)| same_model(old, function))
                        .map(|(_, bytes)| *bytes)
                })
                .or_else(|| {
                    // COW transforms can rebuild an equal body under a new owner.
                    // Names only select candidates; full model Eq binds retained
                    // encoding bytes once per distinct current dictionary model.
                    let previous = self.previous?;
                    previous
                        .names
                        .get(function.name.as_str())?
                        .iter()
                        .find_map(|&index| {
                            let old = previous.functions[index].as_ref()?;
                            same_model(old, function).then_some(previous.encoded[index])
                        })
                });
            let mut value = function.clone();
            value.name.clear();
            value.linkage = Linkage::External;
            self.functions.push((value, encoded));
            self.owners.entry(body).or_default().push(index);
            index
        };
        self.out.word(index)?;
        self.out.text(&function.name)?;
        self.out.word(linkage(function.linkage))
    }
    pub(crate) fn module(&mut self, module: &Module) -> Option<()> {
        self.out.text(&module.triple)?;
        self.out.word(module.items.len())?;
        for item in &module.items {
            let Item::Function(function) = item else {
                return None;
            };
            self.function(function)?;
        }
        Some(())
    }
    #[cfg(test)]
    pub(crate) fn finish(self, magic: &[u8; 8]) -> Option<Vec<u8>> {
        self.finish_counted(magic).map(|(bytes, _, _)| bytes)
    }
    pub(crate) fn finish_counted(self, magic: &[u8; 8]) -> Option<(Vec<u8>, usize, usize)> {
        let mut out = Writer::new();
        out.put(magic)?;
        out.word(self.functions.len())?;
        let mut encoded = 0;
        let mut retained = 0;
        for (function, previous) in self.functions {
            if let Some(previous) = previous {
                out.blob(previous)?;
                retained += 1;
            } else {
                out.blob(&raw_codec::encode_function(&function)?)?;
                encoded += 1;
            }
        }
        out.blob(&self.out.0)?;
        Some((out.0, encoded, retained))
    }
}
pub(crate) struct Decoder<'a> {
    pub(crate) input: Reader<'a>,
    encoded: Vec<&'a [u8]>,
    functions: Vec<Option<Function>>,
    names: HashMap<String, Vec<usize>>,
    limits: Limits,
    remaining: usize,
    pub(crate) decoded: usize,
}
/// A bounded dictionary location. Reading an index never materializes its body
/// or grants permission to consume it; the requesting stage binds its inputs.
pub(crate) struct FunctionRef {
    index: usize,
    name: String,
    linkage: Linkage,
}
impl<'a> Decoder<'a> {
    pub(crate) fn new(bytes: &'a [u8], magic: &[u8; 8], limits: &Limits) -> Option<Self> {
        let mut input = Reader::new(bytes, limits.max_nodes.min(1_000_000))?;
        if input.take(8)? != magic {
            return None;
        }
        let count = input.count()?;
        if count > 32_768 {
            return None;
        }
        let mut encoded = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            encoded.push(input.blob()?);
        }
        let body = input.blob()?;
        if !input.finish() {
            return None;
        }
        Some(Self {
            input: Reader::new(body, limits.max_nodes.min(1_000_000))?,
            encoded,
            functions: vec![None; count],
            names: HashMap::new(),
            limits: limits.clone(),
            remaining: limits.max_nodes.min(1_000_000),
            decoded: 0,
        })
    }
    pub(crate) fn section(&mut self, bytes: &'a [u8]) -> Option<()> {
        self.input = Reader::new(bytes, self.limits.max_nodes.min(1_000_000))?;
        Some(())
    }
    pub(crate) fn function(&mut self) -> Option<Function> {
        let reference = self.function_ref()?;
        self.resolve(&reference)
    }
    pub(crate) fn function_ref(&mut self) -> Option<FunctionRef> {
        let index = self.input.word()?;
        self.encoded.get(index)?;
        let name = self.input.text()?;
        let linkage = match self.input.word()? {
            0 => Linkage::External,
            1 => Linkage::Internal,
            2 => Linkage::Private,
            3 => Linkage::AvailableExternally,
            _ => return None,
        };
        Some(FunctionRef {
            index,
            name,
            linkage,
        })
    }
    pub(crate) fn resolve(&mut self, reference: &FunctionRef) -> Option<Function> {
        let index = reference.index;
        let slot = self.functions.get_mut(index)?;
        if slot.is_none() {
            let function =
                raw_codec::decode_function(self.encoded[index], &self.limits, &mut self.remaining)?;
            if !function.name.is_empty() || function.linkage != Linkage::External {
                return None;
            }
            *slot = Some(function);
            self.decoded += 1;
        }
        let mut function = slot.as_ref()?.clone();
        function.name = reference.name.clone();
        function.linkage = reference.linkage;
        let indices = self.names.entry(function.name.clone()).or_default();
        if !indices.contains(&index) {
            indices.push(index);
        }
        Some(function)
    }
    pub(crate) fn module(&mut self) -> Option<Module> {
        let triple = self.input.text()?;
        let count = self.input.count()?;
        if count > 32_768 {
            return None;
        }
        let mut items = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            items.push(Item::Function(self.function()?));
        }
        Some(Module {
            triple,
            fingerprint: String::new(),
            items: items.into(),
        })
    }
}

const MAGIC: &[u8; 8] = b"ZILMCP02";
pub(crate) struct Recovery<'a> {
    pub(crate) decoder: Decoder<'a>,
    classes: &'a [u8],
    partition: &'a [u8],
}
impl<'a> Recovery<'a> {
    pub(crate) fn new(bytes: &'a [u8], limits: &Limits) -> Option<Self> {
        let mut decoder = Decoder::new(bytes, MAGIC, limits)?;
        let classes = decoder.input.blob()?;
        let partition = decoder.input.blob()?;
        decoder.input.finish().then_some(Self {
            decoder,
            classes,
            partition,
        })
    }
    #[cfg(test)]
    pub(crate) fn classes(
        &mut self,
        current: &Module,
    ) -> Option<crate::machine_call_classes::Cache> {
        self.decoder.section(self.classes)?;
        crate::machine_call_classes::Cache::decode_from(&mut self.decoder, current)
    }
    pub(crate) fn class_candidates(
        &mut self,
        current: &Module,
    ) -> Option<crate::machine_call_classes::Candidates<'a, '_>> {
        self.decoder.section(self.classes)?;
        crate::machine_call_classes::Candidates::read(&mut self.decoder, current)
    }
    pub(crate) fn alias_class_candidates(
        &mut self,
        current: &crate::machine_output::AliasModule,
    ) -> Option<crate::machine_call_classes::Candidates<'a, '_>> {
        self.decoder.section(self.classes)?;
        crate::machine_call_classes::Candidates::read_aliases(&mut self.decoder, current)
    }
    pub(crate) fn partition(
        &mut self,
        lir: &zil_ir::LirView<'_>,
        entry: crate::EntryPoint,
        count: usize,
        imports: crate::machine_cgu::CallImportPolicy,
    ) -> Option<crate::machine_cgu::Materialization> {
        self.decoder.section(self.partition)?;
        crate::machine_cgu::Materialization::decode_from(
            &mut self.decoder,
            lir,
            entry,
            count,
            imports,
        )
    }
}
pub(crate) fn encode_products(
    classes: &crate::machine_call_classes::Cache,
    partition: &crate::machine_cgu::Materialization,
    previous: Option<&Recovery<'_>>,
) -> Option<(Vec<u8>, usize, usize)> {
    let mut out = Encoder::new();
    if let Some(previous) = previous {
        out = out.with_previous(&previous.decoder);
    }
    classes.encode_into(&mut out)?;
    let class_bytes = std::mem::replace(&mut out.out, Writer::new()).0;
    partition.encode_into(&mut out)?;
    let partition_bytes = std::mem::replace(&mut out.out, Writer::new()).0;
    out.out.blob(&class_bytes)?;
    out.out.blob(&partition_bytes)?;
    out.finish_counted(MAGIC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Inst, Op, Signature, Ty, Val};
    fn model() -> Function {
        Function {
            name: "zil.class.example".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions: vec![Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(7)))),
                    )],
                }]
                .into(),
            ),
        }
    }
    #[test]
    fn roles_share_one_dictionary_model_and_equal_cow_bodies_retain_encoding() {
        let original = model();
        let mut imported = original.clone();
        imported.name = "zil.import.3.zil.class.example".into();
        imported.linkage = Linkage::Internal;
        let mut encoder = Encoder::new();
        encoder.function(&original).unwrap();
        encoder.function(&imported).unwrap();
        let (bytes, encoded, retained) = encoder.finish_counted(b"DICTTEST").unwrap();
        assert_eq!((encoded, retained), (1, 0));
        let mut decoder = Decoder::new(&bytes, b"DICTTEST", &Limits::default()).unwrap();
        let restored = decoder.function().unwrap();
        assert_eq!(restored, original);
        assert_eq!(decoder.function().unwrap(), imported);
        assert_eq!(decoder.decoded, 1);
        assert!(decoder.input.finish());
        let mut equal = restored.clone();
        equal.blocks = Some(restored.blocks.as_ref().unwrap().iter().cloned().collect());
        assert!(
            !equal
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(restored.blocks.as_ref().unwrap())
        );
        let mut encoder = Encoder::new().with_previous(&decoder);
        encoder.function(&equal).unwrap();
        let (_, encoded, retained) = encoder.finish_counted(b"DICTTEST").unwrap();
        assert_eq!((encoded, retained), (0, 1));
        equal.blocks.as_mut().unwrap()[0].instructions[0].op =
            Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(99))));
        let mut encoder = Encoder::new().with_previous(&decoder);
        encoder.function(&equal).unwrap();
        let (_, encoded, retained) = encoder.finish_counted(b"DICTTEST").unwrap();
        assert_eq!((encoded, retained), (1, 0));
    }
}
