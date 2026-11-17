//! Bounded raw scalar work-product codec. The trusted semantic-cache boundary
//! is the same as raw emission; none of these values contain checker receipts.
//! Shared bodies are encoded/decoded once, with each actual symbol bound below.
use crate::emit::raw_codec::{self, Reader, Writer};
use std::collections::HashMap;
use zil_base::Limits;
use zil_llvm_api::model::{Block, Function, Item, Module};

pub(crate) struct Encoder {
    pub(crate) out: Writer,
    functions: Vec<Function>,
    owners: HashMap<Option<*const Vec<Block>>, Vec<usize>>,
}
impl Encoder {
    pub(crate) fn new() -> Self {
        Self {
            out: Writer::new(),
            functions: vec![],
            owners: HashMap::new(),
        }
    }
    pub(crate) fn function(&mut self, function: &Function) -> Option<()> {
        let owner = function
            .blocks
            .as_ref()
            .map(|blocks| std::ptr::from_ref(&**blocks));
        let index = self.owners.get(&owner).and_then(|indices| {
            indices.iter().copied().find(|&i| {
                let old = &self.functions[i];
                old.linkage == function.linkage
                    && old.signature == function.signature
                    && old.attrs == function.attrs
                    && old.blocks == function.blocks
            })
        });
        let index = if let Some(index) = index {
            index
        } else {
            let index = self.functions.len();
            if index >= 32_768 {
                return None;
            }
            let mut value = function.clone();
            value.name.clear();
            self.functions.push(value);
            self.owners.entry(owner).or_default().push(index);
            index
        };
        self.out.word(index)?;
        self.out.text(&function.name)
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
    pub(crate) fn finish(self, magic: &[u8; 8]) -> Option<Vec<u8>> {
        let mut out = Writer::new();
        out.put(magic)?;
        out.word(self.functions.len())?;
        for function in self.functions {
            out.blob(&raw_codec::encode_function(&function)?)?;
        }
        out.blob(&self.out.0)?;
        Some(out.0)
    }
}
pub(crate) struct Decoder<'a> {
    pub(crate) input: Reader<'a>,
    functions: Vec<Function>,
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
        let mut functions = Vec::with_capacity(count.min(4096));
        let mut remaining = limits.max_nodes.min(1_000_000);
        for _ in 0..count {
            functions.push(raw_codec::decode_function(
                input.blob()?,
                limits,
                &mut remaining,
            )?);
        }
        let body = input.blob()?;
        if !input.finish() {
            return None;
        }
        Some(Self {
            input: Reader::new(body, remaining)?,
            functions,
        })
    }
    pub(crate) fn function(&mut self) -> Option<Function> {
        let mut function = self.functions.get(self.input.word()?)?.clone();
        function.name = self.input.text()?;
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
