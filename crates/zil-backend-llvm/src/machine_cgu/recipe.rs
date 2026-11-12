//! In-memory ordinary partition recipes, never native validation capabilities.
//! Only complete identical topology/ABI dependencies admit body replacement.
use super::{Plan, direct_calls, layout};
use std::collections::BTreeSet;
use std::sync::Arc;
use zil_llvm_api::model::{Function, Item, Module};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Recipe {
    source: Module,
    layout: Arc<layout::Layout>,
}
impl Recipe {
    pub(super) fn new(source: Module, layout: layout::Layout) -> Option<Self> {
        if layout.units.is_empty() || layout.units.iter().any(|unit| !unit.imports.is_empty()) {
            return None;
        }
        Some(Self {
            source,
            layout: Arc::new(layout),
        })
    }
    /// Caller additionally binds the current independently admitted source
    /// inventory, ABI/limits/target/options and absence of post-partition edits.
    /// This method checks every changed native input's complete relevant graph.
    pub(crate) fn replay(
        &self,
        module: &Module,
        previous: &[Module],
    ) -> Option<(Plan, Self, usize)> {
        if self.source.triple != module.triple
            || self.source.items.len() != module.items.len()
            || previous.len() != self.layout.units.len()
        {
            return None;
        }
        let mut changed = BTreeSet::new();
        for (index, (old, current)) in self.source.items.iter().zip(&module.items).enumerate() {
            if old == current {
                continue;
            }
            let (Item::Function(old), Item::Function(current)) = (old, current) else {
                return None;
            };
            if !same_function_partition_inputs(old, current) {
                return None;
            }
            changed.insert(index);
        }
        let mut units = Vec::with_capacity(previous.len());
        let mut replaced = 0usize;
        let mut updated_units = 0usize;
        for (layout, previous) in self.layout.units.iter().zip(previous) {
            if !layout.imports.is_empty() {
                return None;
            }
            let prefix = layout
                .prelude
                .len()
                .checked_add(layout.declarations.len())?;
            if previous.items.len() != prefix.checked_add(layout.definitions.len())? {
                return None;
            }
            let mut unit = previous.clone();
            unit.fingerprint = module.fingerprint.clone();
            let mut updated = false;
            for (position, &index) in layout.definitions.iter().enumerate() {
                if changed.contains(&index) {
                    *unit.items.get_mut(prefix.checked_add(position)?)? =
                        module.items.get(index)?.clone();
                    replaced += 1;
                    updated = true;
                }
            }
            updated_units += usize::from(updated);
            units.push(unit);
        }
        // Every changed definition must have one actual canonical consumer.
        // A declaration-only or missing definition cannot disappear via replay.
        if replaced != changed.len() {
            return None;
        }
        Some((
            Plan {
                units,
                fallback: None,
            },
            Self {
                source: module.clone(),
                layout: self.layout.clone(),
            },
            updated_units,
        ))
    }
}
fn same_function_partition_inputs(old: &Function, current: &Function) -> bool {
    // Exhaustive projection: a future machine field must be accounted for.
    let Function {
        name,
        linkage,
        signature,
        attrs,
        blocks,
    } = current;
    old.name == *name && old.linkage == *linkage && old.signature == *signature && old.attrs == *attrs
        && old.blocks.is_some() && blocks.is_some()
        // Conservative budget guard, including complete-call copy accounting if
        // this recipe's scope is ever extended beyond ordinary code generation.
        && old.cost() == current.cost()
        && old.references() == current.references()
        && direct_calls(old) == direct_calls(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, ModuleId, Span, TargetProfile};
    use zil_ir::{BlockId, FunctionId, LirModule, PassingMode, Type, TypedFunction};
    use zil_llvm_api::model::{Arg, Block, Global, Inst, Linkage, Op, Signature, Ty, Val};
    fn fixture() -> (Module, LirModule) {
        let source = LirModule {
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: ModuleId(0),
            verified_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::host_llvm(),
            functions: (0..4)
                .map(|id| TypedFunction {
                    storage_life: None,
                    call_kind: zil_ir::CallKind::Direct,
                    reference_return_bound: None,
                    callable_return_contract: None,
                    callable_contract: None,
                    protocol_contract: None,
                    id: FunctionId(id),
                    name: format!("f{id}"),
                    foreign: None,
                    environment: None,
                    parameters: vec![],
                    return_type: Type::I32,
                    result_mode: PassingMode::Infer,
                    locals: vec![].into(),
                    values: vec![].into(),
                    blocks: vec![].into(),
                    entry: BlockId(0),
                    span: Span {
                        file: FileId(id % 2),
                        ..Span::default()
                    },
                })
                .collect(),
            resources: vec![],
            closures: vec![],
            layouts: vec![],
        };
        let mut items = vec![
            Item::Global(Global {
                name: "config".into(),
                linkage: Linkage::Private,
                ty: Ty::Int(32),
                initializer: Some(Val::Int(7)),
                constant: true,
                unnamed_addr: false,
                alignment: 4,
            }),
            Item::Function(Function {
                name: "llvm.trap".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Void, vec![]),
                attrs: vec![],
                blocks: None,
            }),
        ];
        for id in 0..4 {
            let mut instructions = Vec::new();
            let result = if id == 0 {
                instructions.push(Inst::new(
                    Some("result".into()),
                    Op::Call {
                        result: Ty::Int(32),
                        result_attrs: vec![],
                        callee: Val::Global("zil.fn.1".into()),
                        args: vec![],
                        vararg: None,
                    },
                ));
                Val::Local("result".into())
            } else {
                Val::Int(id)
            };
            instructions.push(Inst::new(
                None,
                Op::Return(Some(Arg::new(Ty::Int(32), result))),
            ));
            items.push(Item::Function(Function {
                name: format!("zil.fn.{id}"),
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
            }));
        }
        (
            Module {
                triple: source.target.triple.clone(),
                fingerprint: "before".into(),
                items: items.into(),
            },
            source,
        )
    }
    fn function_mut<'a>(module: &'a mut Module, name: &str) -> &'a mut Function {
        module
            .items
            .iter_mut()
            .find_map(|item| match item {
                Item::Function(function) if function.name == name => Some(function),
                _ => None,
            })
            .unwrap()
    }
    fn initial(module: &Module, source: &LirModule) -> (Plan, Recipe) {
        let (plan, _, recipe) = crate::machine_cgu::partition_view_cached_with_recipe(
            module.clone(),
            &source.as_view(),
            crate::EntryPoint::None,
            2,
            false,
            crate::machine_cgu::CallImportPolicy::Disabled,
            None,
        );
        assert_eq!(plan.units.len(), 2);
        assert_eq!(plan.fallback, None);
        (plan, recipe.unwrap())
    }
    #[test]
    fn changed_body_updates_only_its_unit_and_matches_the_current_complete_planner() {
        let (module, source) = fixture();
        let (old, recipe) = initial(&module, &source);
        let mut current = module.clone();
        current.fingerprint = "current".into();
        function_mut(&mut current, "zil.fn.1")
            .blocks
            .as_mut()
            .unwrap()[0]
            .instructions[0]
            .op = Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(9))));
        let (updated, next, rebuilt) = recipe.replay(&current, &old.units).unwrap();
        let fresh = crate::machine_cgu::partition_view(
            current.clone(),
            &source.as_view(),
            crate::EntryPoint::None,
            2,
            false,
            false,
        );
        assert_eq!(rebuilt, 1);
        assert_eq!(updated.units, fresh.units);
        assert_eq!(
            updated
                .units
                .iter()
                .zip(&old.units)
                .filter(|(new, old)| new.items.ptr_eq(&old.items))
                .count(),
            1
        );
        for unit in &updated.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
        // Reassembled equal current results stop again, without another payload rebuild.
        let (equal, _, rebuilt) = next.replay(&current, &updated.units).unwrap();
        assert_eq!(rebuilt, 0);
        assert!(
            equal
                .units
                .iter()
                .zip(&updated.units)
                .all(|(a, b)| a.items.ptr_eq(&b.items))
        );
    }
    #[test]
    fn topology_abi_visibility_cost_globals_and_item_order_changes_use_the_complete_planner() {
        let (module, source) = fixture();
        let (old, recipe) = initial(&module, &source);
        let mut changes = Vec::new();
        let mut changed = module.clone();
        if let Op::Call { callee, .. } = &mut function_mut(&mut changed, "zil.fn.0")
            .blocks
            .as_mut()
            .unwrap()[0]
            .instructions[0]
            .op
        {
            *callee = Val::Global("zil.fn.3".into());
        }
        changes.push(changed);
        let mut changed = module.clone();
        function_mut(&mut changed, "zil.fn.0")
            .blocks
            .as_mut()
            .unwrap()[0]
            .instructions[0]
            .op = Op::Freeze(Arg::new(Ty::Ptr, Val::Global("zil.fn.1".into())));
        changes.push(changed);
        let mut changed = module.clone();
        function_mut(&mut changed, "zil.fn.1").signature.result = Ty::Int(64);
        changes.push(changed);
        let mut changed = module.clone();
        function_mut(&mut changed, "zil.fn.1").linkage = Linkage::Internal;
        changes.push(changed);
        let mut changed = module.clone();
        function_mut(&mut changed, "zil.fn.1")
            .blocks
            .as_mut()
            .unwrap()[0]
            .instructions
            .insert(
                0,
                Inst::new(
                    Some("extra".into()),
                    Op::Freeze(Arg::new(Ty::Int(32), Val::Int(0))),
                ),
            );
        changes.push(changed);
        let mut changed = module.clone();
        if let Item::Global(global) = &mut changed.items[0] {
            global.initializer = Some(Val::Int(9));
        }
        changes.push(changed);
        let mut changed = module.clone();
        changed.items.swap(2, 3);
        changes.push(changed);
        let mut changed = module.clone();
        function_mut(&mut changed, "llvm.trap").signature.result = Ty::Int(32);
        changes.push(changed);
        let mut changed = module.clone();
        changed.triple = "x86_64-pc-windows-msvc".into();
        changes.push(changed);
        for changed in changes {
            assert!(recipe.replay(&changed, &old.units).is_none());
        }
    }
}
