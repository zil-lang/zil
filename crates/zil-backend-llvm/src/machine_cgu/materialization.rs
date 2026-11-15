//! Reuse actual partition items after the CURRENT planner chose their roles.
//! This is process-owned data, not a layout decision or a verification receipt.
use super::{direct_calls, layout};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use zil_llvm_api::model::{Item, Module};

pub(super) const PRELUDE: u8 = 0;
pub(super) const DECLARATION: u8 = 1;
pub(super) const IMPORT_DECLARATION: u8 = 2;
pub(super) const IMPORT_BODY: u8 = 3;
pub(super) const DEFINITION: u8 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Materialization {
    source: Module,
    pub(crate) reused_items: usize,
    pub(crate) constructed_items: usize,
    units: Vec<Module>,
    inputs: Vec<UnitInputs>,
    calls: Vec<Arc<BTreeSet<String>>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct UnitInputs {
    owner: usize,
    slots: BTreeMap<(usize, u8), usize>,
    names: BTreeMap<String, String>,
}
pub(super) struct Reuse<'a> {
    previous: &'a Materialization,
    reused_items: std::cell::Cell<usize>,
    same: Vec<bool>,
    changed_names: Vec<BTreeSet<String>>,
}
impl Materialization {
    pub(super) fn new(
        source: Module,
        layout: &layout::Layout,
        units: &[Module],
        reused_items: usize,
    ) -> Self {
        let inputs = layout
            .units
            .iter()
            .map(|unit| {
                let mut slots = BTreeMap::new();
                let mut next = 0;
                for (indices, role) in [(&unit.prelude, PRELUDE), (&unit.declarations, DECLARATION)]
                {
                    for &index in indices {
                        slots.insert((index, role), next);
                        next += 1;
                    }
                }
                for &index in &unit.imports {
                    slots.insert((index, IMPORT_DECLARATION), next);
                    slots.insert((index, IMPORT_BODY), next + 1);
                    next += 2;
                }
                for &index in &unit.definitions {
                    slots.insert((index, DEFINITION), next);
                    next += 1;
                }
                UnitInputs {
                    owner: unit.owner,
                    slots,
                    names: import_names(&source, unit),
                }
            })
            .collect();
        let calls = source
            .items
            .iter()
            .map(|item| {
                Arc::new(match item {
                    Item::Function(function) => direct_calls(function),
                    _ => BTreeSet::new(),
                })
            })
            .collect();
        Self {
            reused_items,
            constructed_items: units
                .iter()
                .map(|unit| unit.items.len())
                .sum::<usize>()
                .saturating_sub(reused_items),
            source,
            units: units.to_vec(),
            inputs,
            calls,
        }
    }
    pub(super) fn bind<'a>(
        &'a self,
        current: &Module,
        layout: &layout::Layout,
    ) -> Option<Reuse<'a>> {
        if self.source.triple != current.triple || self.inputs.len() != layout.units.len() {
            return None;
        }
        // Full values authorize raw item reuse; position and hashes alone do not.
        let same = current
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| self.source.items.get(index) == Some(item))
            .collect();
        let changed_names = layout
            .units
            .iter()
            .zip(&self.inputs)
            .map(|(unit, old)| {
                let names = import_names(current, unit);
                names
                    .keys()
                    .chain(old.names.keys())
                    .filter(|name| names.get(*name) != old.names.get(*name))
                    .cloned()
                    .collect()
            })
            .collect();
        Some(Reuse {
            previous: self,
            reused_items: std::cell::Cell::new(0),
            same,
            changed_names,
        })
    }
}
impl Reuse<'_> {
    pub(super) fn reused_items(&self) -> usize {
        self.reused_items.get()
    }
    pub(super) fn item(&self, unit: usize, owner: usize, index: usize, role: u8) -> Option<Item> {
        if !self.same.get(index).copied()? {
            return None;
        }
        let input = self.previous.inputs.get(unit)?;
        if input.owner != owner {
            return None;
        }
        let changed = &self.changed_names[unit];
        if matches!(role, IMPORT_BODY | DEFINITION)
            && self
                .previous
                .calls
                .get(index)?
                .iter()
                .any(|name| changed.contains(name))
        {
            return None;
        }
        // An imported body's own private symbol also depends on its current role.
        if role == IMPORT_BODY
            && let Item::Function(function) = self.previous.source.items.get(index)?
            && changed.contains(&function.name)
        {
            return None;
        }
        let item = self
            .previous
            .units
            .get(unit)?
            .items
            .get(*input.slots.get(&(index, role))?)?
            .clone();
        self.reused_items.set(self.reused_items.get() + 1);
        Some(item)
    }
}
fn import_names(module: &Module, unit: &layout::Unit) -> BTreeMap<String, String> {
    unit.imports
        .iter()
        .filter_map(|&index| {
            let Item::Function(function) = module.items.get(index)? else {
                return None;
            };
            Some((
                function.name.clone(),
                format!("zil.import.{}.{}", unit.owner, function.name),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::{Arg, Block, Function, Inst, Linkage, Op, Signature, Ty, Val};

    fn function(name: &str, callee: Option<&str>) -> Item {
        let mut instructions = Vec::new();
        let value = if let Some(callee) = callee {
            instructions.push(Inst::new(
                Some("value".into()),
                Op::Call {
                    result: Ty::Int(32),
                    result_attrs: vec![],
                    callee: Val::Global(callee.into()),
                    args: vec![],
                    vararg: None,
                },
            ));
            Val::Local("value".into())
        } else {
            Val::Int(7)
        };
        instructions.push(Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(32), value))),
        ));
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions,
                }]
                .into(),
            ),
        })
    }
    fn fixture() -> (Module, layout::Layout) {
        let module = Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "old".into(),
            items: vec![
                function("zil.fn.0", Some("zil.fn.1")),
                function("zil.fn.1", None),
                function("main", Some("zil.fn.0")),
            ]
            .into(),
        };
        let layout = layout::Layout {
            units: vec![
                layout::Unit {
                    owner: 0,
                    prelude: vec![],
                    declarations: vec![],
                    imports: vec![1],
                    definitions: vec![0, 2],
                },
                layout::Unit {
                    owner: 1,
                    prelude: vec![],
                    declarations: vec![],
                    imports: vec![],
                    definitions: vec![1],
                },
            ],
        };
        (module, layout)
    }
    fn named<'a>(module: &'a Module, name: &str) -> &'a Function {
        module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(function) if function.name == name => Some(function),
                _ => None,
            })
            .unwrap()
    }
    #[test]
    fn changed_import_body_matches_fresh_and_keeps_equal_callers_shared() {
        let (source, layout) = fixture();
        let previous = layout.replay(source.clone());
        let cache = Materialization::new(source.clone(), &layout, &previous, 0);
        let mut current = source;
        current.fingerprint = "current".into();
        let Item::Function(function) = &mut current.items[1] else {
            unreachable!()
        };
        function.blocks.as_mut().unwrap()[0].instructions[0].op =
            Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(9))));
        let actual = layout.replay_reusing(current.clone(), Some(&cache));
        assert_eq!(actual, layout.replay(current));
        assert!(
            named(&actual[0], "zil.fn.0")
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(named(&previous[0], "zil.fn.0").blocks.as_ref().unwrap())
        );
        assert!(
            !named(&actual[0], "zil.import.0.zil.fn.1")
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(
                    named(&previous[0], "zil.import.0.zil.fn.1")
                        .blocks
                        .as_ref()
                        .unwrap()
                )
        );
    }
    #[test]
    fn changed_import_binding_rewrites_an_unchanged_caller() {
        let (source, mut layout) = fixture();
        let previous = layout.replay(source.clone());
        let cache = Materialization::new(source.clone(), &layout, &previous, 0);
        layout.units[0].imports.clear();
        layout.units[0].declarations.push(1);
        let actual = layout.replay_reusing(source.clone(), Some(&cache));
        assert_eq!(actual, layout.replay(source));
        assert!(
            !named(&actual[0], "zil.fn.0")
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(named(&previous[0], "zil.fn.0").blocks.as_ref().unwrap())
        );
        assert_eq!(
            direct_calls(named(&actual[0], "zil.fn.0")),
            BTreeSet::from(["zil.fn.1".into()])
        );
    }
    #[test]
    fn changed_owner_rebuilds_private_import_names_even_for_leaf_bodies() {
        let (source, mut layout) = fixture();
        let previous = layout.replay(source.clone());
        let cache = Materialization::new(source.clone(), &layout, &previous, 0);
        layout.units[0].owner = 7;
        let actual = layout.replay_reusing(source.clone(), Some(&cache));
        assert_eq!(actual, layout.replay(source));
        assert_eq!(
            named(&actual[0], "zil.import.7.zil.fn.1").name,
            "zil.import.7.zil.fn.1"
        );
        assert_eq!(
            direct_calls(named(&actual[0], "zil.fn.0")),
            BTreeSet::from(["zil.import.7.zil.fn.1".into()])
        );
    }
}
