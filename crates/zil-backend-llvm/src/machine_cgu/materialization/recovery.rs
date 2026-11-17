//! Cross-process raw layout/render products. Admission below rederives reads
//! from raw bodies, checks the actual rendering relation and then revalidates
//! all CURRENT roles/graph/copy limits before these items reach a CGU consumer.
use super::*;
use crate::machine_cache::{Decoder, Encoder};
use zil_llvm_api::model::{Function, Linkage, Op, Val};

impl Materialization {
    pub(crate) fn encode_into(&self, out: &mut Encoder<'_>) -> Option<()> {
        if !self.completed_roles {
            return None;
        }
        match self.imports {
            CallImportPolicy::Complete => out.out.word(0)?,
            CallImportPolicy::BoundedClasses(None) => out.out.word(1)?,
            CallImportPolicy::BoundedClasses(Some(cost)) => {
                out.out.word(2)?;
                out.out.word(cost)?;
            }
            _ => return None,
        }
        out.module(&self.source)?;
        out.out.blob(&self.layout.encode()?)?;
        out.out.word(self.units.len())?;
        for unit in &self.units {
            out.module(unit)?;
        }
        let input = self.grouping_input.as_ref()?;
        out.out.word(input.count)?;
        out.out.word(input.classes.len())?;
        for class in &input.classes {
            out.out.text(class)?;
        }
        self.grouping()?.encode_source(&mut out.out)?;
        Some(())
    }
    pub(crate) fn decode_from(
        input: &mut Decoder<'_>,
        lir: &zil_ir::LirView<'_>,
        entry: crate::EntryPoint,
        count: usize,
        imports: CallImportPolicy,
    ) -> Option<Self> {
        let old_imports = match input.input.word()? {
            0 => CallImportPolicy::Complete,
            1 => CallImportPolicy::BoundedClasses(None),
            2 => CallImportPolicy::BoundedClasses(Some(input.input.word()?)),
            _ => return None,
        };
        if old_imports != imports {
            return None;
        }
        let source = input.module()?;
        if source.triple != lir.target.triple {
            return None;
        }
        let layout = layout::Layout::decode(input.input.blob()?)?;
        let n = input.input.count()?;
        if n != layout.units.len() || n > count || n > 64 {
            return None;
        }
        let mut units = Vec::with_capacity(n);
        for _ in 0..n {
            units.push(input.module()?);
        }
        if input.input.word()? != count {
            return None;
        }
        let n = input.input.count()?;
        let mut classes = BTreeSet::new();
        for _ in 0..n {
            if !classes.insert(input.input.text()?) {
                return None;
            }
        }
        // Fresh raw graph derivation: no missing edge can be supplied by bytes.
        let graph = layout::CurrentGraph::read(&source, None)?;
        layout.valid_classes_observed_with_policy(
            &source, lir, entry, count, &classes, &graph, imports,
        )?;
        if !render_matches(&source, &layout, &units) {
            return None;
        }
        let mut grouping = layout::Grouping::read_with_policy(
            &source, lir, entry, count, &classes, &graph, imports,
        )?;
        grouping.decode_source(&mut input.input)?;
        if !input.input.finish() {
            return None;
        }
        Some(
            Self::completed_with_graph(source, &layout, &units, 0, graph, Some(grouping))
                .with_import_policy(imports),
        )
    }
    pub(in crate::machine_cgu) fn count_reused_units(&self, units: &[Module]) -> usize {
        units
            .iter()
            .filter(|unit| self.units.iter().any(|old| old.items.ptr_eq(&unit.items)))
            .count()
    }
}

// Check raw rendered functions without cloning/rebuilding their instruction
// arrays. Only direct global call atoms and imported own name/linkage change.
fn body_matches(
    source: &Function,
    output: &Function,
    name: &str,
    linkage: Linkage,
    imports: &BTreeMap<String, String>,
) -> bool {
    if output.name != name
        || output.linkage != linkage
        || output.signature != source.signature
        || output.attrs != source.attrs
    {
        return false;
    }
    match (&source.blocks, &output.blocks) {
        (None, None) => true,
        (Some(a), Some(b)) if a.len() == b.len() => a.iter().zip(b.iter()).all(|(a, b)| {
            a.name == b.name
                && a.instructions.len() == b.instructions.len()
                && a.instructions.iter().zip(&b.instructions).all(|(a, b)| {
                    if a.result != b.result {
                        return false;
                    }
                    match (&a.op, &b.op) {
                        (
                            Op::Call {
                                result: ar,
                                result_attrs: aa,
                                callee: Val::Global(an),
                                args: ax,
                                vararg: av,
                            },
                            Op::Call {
                                result: br,
                                result_attrs: ba,
                                callee: Val::Global(bn),
                                args: bx,
                                vararg: bv,
                            },
                        ) => {
                            imports.get(an).map_or(an, |n| n) == bn
                                && ar == br
                                && aa == ba
                                && ax == bx
                                && av == bv
                        }
                        _ => a.op == b.op,
                    }
                })
        }),
        _ => false,
    }
}
fn render_matches(source: &Module, layout: &layout::Layout, units: &[Module]) -> bool {
    for (roles, output) in layout.units.iter().zip(units) {
        if output.triple != source.triple {
            return false;
        }
        let imports = import_names(source, roles);
        let mut items = output.items.iter();
        for &index in &roles.prelude {
            if items.next() != source.items.get(index) {
                return false;
            }
        }
        for &index in &roles.declarations {
            let Some(Item::Function(source)) = source.items.get(index) else {
                return false;
            };
            if items.next() != Some(&Item::Function(source.declaration())) {
                return false;
            }
        }
        for &index in &roles.imports {
            let Some(Item::Function(source)) = source.items.get(index) else {
                return false;
            };
            if items.next() != Some(&Item::Function(source.declaration())) {
                return false;
            }
            let Some(Item::Function(output)) = items.next() else {
                return false;
            };
            let Some(name) = imports.get(&source.name) else {
                return false;
            };
            if !body_matches(source, output, name, Linkage::Internal, &imports) {
                return false;
            }
        }
        for &index in &roles.definitions {
            let Some(Item::Function(source)) = source.items.get(index) else {
                return false;
            };
            let Some(Item::Function(output)) = items.next() else {
                return false;
            };
            if !body_matches(source, output, &source.name, source.linkage, &imports) {
                return false;
            }
        }
        if items.next().is_some() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Inst, Signature, Ty};
    fn leaf(name: &str, value: i128) -> Item {
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions: vec![Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(value)))),
                    )],
                }]
                .into(),
            ),
        })
    }
    #[test]
    fn well_typed_cached_unit_with_changed_body_is_not_a_raw_render_product() {
        let source = Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: String::new(),
            items: vec![leaf("zil.fn.0", 7), leaf("zil.fn.1", 9)].into(),
        };
        let layout = layout::Layout {
            units: (0..2)
                .map(|owner| layout::Unit {
                    owner,
                    prelude: vec![],
                    declarations: vec![],
                    imports: vec![],
                    definitions: vec![owner],
                })
                .collect(),
        };
        let mut units = layout.replay(source.clone());
        assert!(render_matches(&source, &layout, &units));
        units[0].items[0] = leaf("zil.fn.0", 99);
        zil_llvm_api::validate_module(&units[0]).unwrap();
        assert!(!render_matches(&source, &layout, &units));
        units = layout.replay(source.clone());
        units[0].items.push(leaf("extra", 7));
        assert!(!render_matches(&source, &layout, &units));
    }
}
