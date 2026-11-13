//! In-memory ordinary partition recipes, never native validation capabilities.
//! Only complete identical topology/ABI dependencies admit body replacement.
use super::{Plan, direct_calls, layout};
use std::collections::BTreeSet;
use std::sync::Arc;
use zil_llvm_api::model::{Function, Item, Module};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Recipe {
    source: crate::machine_output::MachineOutput,
    layout: Arc<layout::Layout>,
    consumers: Arc<Vec<Vec<(usize, usize)>>>,
}
impl Recipe {
    pub(super) fn new(source: Module, layout: layout::Layout) -> Option<Self> {
        if layout.units.is_empty() || layout.units.iter().any(|unit| !unit.imports.is_empty()) {
            return None;
        }
        // All canonical occurrences, not a single last-writer slot. Imported
        // available_externally bodies are outside this ordinary recipe domain
        // (the imports guard above rejects them before retaining any index).
        let mut consumers = vec![Vec::new(); source.items.len()];
        for (unit_index, unit) in layout.units.iter().enumerate() {
            let prefix = unit.prelude.len().checked_add(unit.declarations.len())?;
            for (position, &index) in unit.definitions.iter().enumerate() {
                consumers
                    .get_mut(index)?
                    .push((unit_index, prefix.checked_add(position)?));
            }
        }
        Some(Self {
            source: source.into(),
            layout: Arc::new(layout),
            consumers: Arc::new(consumers),
        })
    }
    /// Caller additionally binds the current independently admitted source
    /// inventory, ABI/limits/target/options and absence of post-partition edits.
    /// This method checks every changed native input's complete relevant graph.
    #[cfg(test)]
    pub(crate) fn replay(
        &self,
        module: &Module,
        previous: &[Module],
    ) -> Option<(Plan, Self, usize)> {
        self.replay_output(&module.clone().into(), previous)
    }
    /// Bind only the actual complete cold planner input. This one-time full Eq
    /// prevents a foreign output tree from acquiring this recipe's layout.
    pub(crate) fn bind_output(&mut self, output: &crate::machine_output::MachineOutput) -> bool {
        if self.source.to_module() != output.to_module() {
            return false;
        }
        self.source = output.clone();
        true
    }
    pub(crate) fn replay_output(
        &self,
        module: &crate::machine_output::MachineOutput,
        previous: &[Module],
    ) -> Option<(Plan, Self, usize)> {
        if self.source.triple != module.triple
            || self.source.len() != module.len()
            || previous.len() != self.layout.units.len()
        {
            return None;
        }
        let candidates = module
            .changed_items(&self.source)
            .unwrap_or_else(|| (0..module.len()).collect());
        let mut changed = BTreeSet::new();
        for index in candidates {
            let old = self.source.item(index)?;
            let current = module.item(index)?;
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
            units.push(unit);
        }
        let mut updated = BTreeSet::new();
        // Push only actual changed definitions to their canonical consumers.
        // Prelude/declaration-only changes have no owner and fail closed.
        for index in changed {
            let consumers = self.consumers.get(index)?;
            if consumers.is_empty() {
                return None;
            }
            for &(unit, slot) in consumers {
                *units.get_mut(unit)?.items.get_mut(slot)? = module.item(index)?.clone();
                updated.insert(unit);
            }
        }
        let updated_units = updated.len();
        Some((
            Plan {
                units,
                fallback: None,
            },
            Self {
                source: module.clone(),
                layout: self.layout.clone(),
                consumers: self.consumers.clone(),
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
    #[test]
    fn push_consumers_update_all_occurrences_and_reject_unowned_or_imported_bodies() {
        let (module, source) = fixture();
        let (_, initial) = initial(&module, &source);
        let changed_index = module
            .items
            .iter()
            .position(|item| matches!(item, Item::Function(f) if f.name == "zil.fn.1"))
            .unwrap();
        let mut layout = initial.layout.as_ref().clone();
        // Repeated canonical consumers must all receive the real new body.
        layout.units[0].definitions.push(changed_index);
        layout.units[1].definitions.push(changed_index);
        let old = layout.replay(module.clone());
        let recipe = Recipe::new(module.clone(), layout.clone()).unwrap();
        let mut current = module.clone();
        function_mut(&mut current, "zil.fn.1")
            .blocks
            .as_mut()
            .unwrap()[0]
            .instructions[0]
            .op = Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(19))));
        let (updated, next, count) = recipe.replay(&current, &old).unwrap();
        assert_eq!(count, 2);
        assert_eq!(updated.units, layout.replay(current.clone()));
        assert!(Arc::ptr_eq(&recipe.consumers, &next.consumers));
        assert_eq!(
            recipe.source.to_module(),
            module,
            "old successful parent stays immutable"
        );
        let mut imported = layout.clone();
        imported.units[0].imports.push(changed_index);
        assert!(Recipe::new(module.clone(), imported).is_none());
        for unit in &mut layout.units {
            unit.definitions.retain(|&id| id != changed_index);
        }
        let previous = layout.replay(module.clone());
        let absent = Recipe::new(module.clone(), layout).unwrap();
        assert!(absent.replay(&current, &previous).is_none());
        current.items.remove(changed_index);
        assert!(recipe.replay(&current, &old).is_none());
    }

    #[test]
    fn multiple_changed_results_push_to_units_and_match_complete_partition() {
        let (module, source) = fixture();
        let (old, recipe) = initial(&module, &source);
        let mut current = module.clone();
        for (name, value) in [("zil.fn.1", 31), ("zil.fn.2", 32)] {
            function_mut(&mut current, name).blocks.as_mut().unwrap()[0].instructions[0].op =
                Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(value))));
        }
        let (updated, _, count) = recipe.replay(&current, &old.units).unwrap();
        let fresh = crate::machine_cgu::partition_view(
            current,
            &source.as_view(),
            crate::EntryPoint::None,
            2,
            false,
            false,
        );
        assert_eq!(updated.units, fresh.units);
        assert_eq!(count, 2);
    }
}
