//! Structured global reachability after closed-executable internalization.
//!
//! Only unreachable local function definitions may be removed. Globals, types,
//! declarations and nonlocal definitions survive. A nonempty plan must pass
//! complete ORIGINAL unit validation before any deletion; callers still retain
//! their production budgets and all actual-unit preflight/Builder/O2 checks.
use std::collections::HashMap;
use zil_llvm_api::model::{Item, Module, Val};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stats {
    pub(crate) removed_count: usize,
    /// Original item order, not hash iteration order.
    pub(crate) removed_names: Vec<String>,
}

fn enqueue(index: usize, live: &mut [bool], pending: &mut Vec<usize>) {
    if !live[index] {
        live[index] = true;
        pending.push(index);
    }
}

// Traverse aggregate constants iteratively. Direct calls and address operands
// have identical reachability significance; neither can authorize an alias.
// None means cancellation; false means an unknown value forbids pruning.
fn enqueue_symbols(
    value: &Val,
    names: &HashMap<&str, usize>,
    live: &mut [bool],
    pending: &mut Vec<usize>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<bool> {
    // Ordinary scalar operands need no heap allocation. Only nested constants
    // grow the explicit traversal stack.
    let mut values = Vec::new();
    let mut first = Some(value);
    while let Some(value) = first.take().or_else(|| values.pop()) {
        if cancelled() {
            return None;
        }
        match value {
            Val::Global(name) => {
                if let Some(&index) = names.get(name.as_str()) {
                    enqueue(index, live, pending);
                }
            }
            Val::Array(args) => values.extend(args.iter().map(|arg| &arg.value)),
            Val::Gep { ptr, indices, .. } => {
                values.push(ptr);
                values.extend(indices.iter().map(|arg| &arg.value));
            }
            Val::Invalid(_) => return Some(false),
            Val::Local(_)
            | Val::Int(_)
            | Val::F32(_)
            | Val::F64(_)
            | Val::Bool(_)
            | Val::Null
            | Val::Zero
            | Val::Undef
            | Val::Poison
            | Val::Bytes(_) => {}
        }
    }
    Some(true)
}

/// Apply only to an already-internalized, complete physical codegen unit.
/// Ok(None) is cancellation and leaves the module untouched. Validation errors
/// also leave it untouched, including errors in otherwise dead definitions.
pub(crate) fn prune(
    module: &mut Module,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Option<Stats>, zil_llvm_api::Error> {
    if cancelled() {
        return Ok(None);
    }
    let mut names = HashMap::new();
    let mut live = vec![false; module.items.len()];
    let mut pending = Vec::new();
    for (index, item) in module.items.iter().enumerate() {
        if cancelled() {
            return Ok(None);
        }
        if let Item::Function(function) = item {
            if names.insert(function.name.as_str(), index).is_some() {
                // Ambiguous symbols cannot supply reachability authority.
                zil_llvm_api::validate_module(module)?;
                return Ok(Some(Stats::default()));
            }
            if function.blocks.is_some() && !function.linkage.is_local() {
                enqueue(index, &mut live, &mut pending);
            }
        }
    }
    for item in &module.items {
        if cancelled() {
            return Ok(None);
        }
        // Even an otherwise unused PRIVATE global is retained, so every one of
        // its initializer symbols must keep its definition alive transitively.
        if let Item::Global(global) = item
            && let Some(value) = &global.initializer
        {
            match enqueue_symbols(value, &names, &mut live, &mut pending, cancelled) {
                None => return Ok(None),
                Some(false) => return Ok(Some(Stats::default())),
                Some(true) => {}
            }
        }
    }
    while let Some(index) = pending.pop() {
        let Item::Function(function) = &module.items[index] else {
            unreachable!("only function items are queued")
        };
        for block in function.blocks.iter().flatten() {
            for inst in &block.instructions {
                if cancelled() {
                    return Ok(None);
                }
                // Model::Op owns the exhaustive structured operand walk,
                // including call arguments, indirect callees, select and phi.
                for value in inst.op.values() {
                    match enqueue_symbols(value, &names, &mut live, &mut pending, cancelled) {
                        None => return Ok(None),
                        Some(false) => return Ok(Some(Stats::default())),
                        Some(true) => {}
                    }
                }
            }
        }
    }
    let mut remove = vec![false; module.items.len()];
    let mut stats = Stats::default();
    for (index, item) in module.items.iter().enumerate() {
        if cancelled() {
            return Ok(None);
        }
        if let Item::Function(function) = item
            && function.blocks.is_some()
            && function.linkage.is_local()
            && !live[index]
        {
            remove[index] = true;
            stats.removed_names.push(function.name.clone());
        }
    }
    stats.removed_count = stats.removed_names.len();
    if stats.removed_count == 0 {
        return Ok(Some(stats));
    }
    if cancelled() {
        return Ok(None);
    }
    // Do not let unreachability hide malformed unused bodies or bypass their
    // complete original-machine admission. No model mutation has happened yet.
    zil_llvm_api::validate_module(module)?;
    let mut retained = Vec::with_capacity(module.items.len() - stats.removed_count);
    for (index, item) in module.items.iter().enumerate() {
        if cancelled() {
            return Ok(None);
        }
        if !remove[index] {
            retained.push(item.clone());
        }
    }
    if cancelled() {
        return Ok(None);
    }
    module.items = retained.into();
    Ok(Some(stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zil_llvm_api::model::{Arg, Block, Function, Global, Inst, Linkage, Op, Signature, Ty};

    fn leaf(name: &str, linkage: Linkage) -> Function {
        Function {
            name: name.into(),
            linkage,
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

    fn caller(name: &str, linkage: Linkage, target: &str) -> Function {
        let mut f = leaf(name, linkage);
        f.blocks.as_mut().unwrap()[0].instructions.insert(
            0,
            Inst::new(
                Some("v".into()),
                Op::Call {
                    result: Ty::Int(32),
                    result_attrs: vec![],
                    callee: Val::Global(target.into()),
                    args: vec![],
                    vararg: None,
                },
            ),
        );
        f
    }

    fn module(functions: Vec<Function>) -> Module {
        Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "unchanged".into(),
            items: functions.into_iter().map(Item::Function).collect(),
        }
    }

    fn run(m: &mut Module) -> Stats {
        prune(m, &|| false).unwrap().unwrap()
    }

    #[test]
    fn only_unreachable_local_definitions_are_deleted_in_original_order() {
        let mut declaration = leaf("foreign", Linkage::External);
        declaration.blocks = None;
        let mut m = module(vec![
            leaf("dead_private", Linkage::Private),
            caller("main", Linkage::External, "first"),
            caller("first", Linkage::Internal, "second"),
            leaf("second", Linkage::Private),
            leaf("dead_internal", Linkage::Internal),
            leaf("export", Linkage::External),
            leaf("import", Linkage::AvailableExternally),
            declaration,
        ]);
        m.items
            .push(Item::NamedType("kept_type".into(), vec![Ty::Int(32)]));
        let original = m.clone();
        let stats = run(&mut m);
        assert_eq!(
            stats,
            Stats {
                removed_count: 2,
                removed_names: vec!["dead_private".into(), "dead_internal".into()]
            }
        );
        let mut expected = original;
        expected.items = expected
            .items
            .iter()
            .enumerate()
            .filter(|(i, _)| ![0, 4].contains(i))
            .map(|(_, item)| item.clone())
            .collect();
        assert_eq!(m, expected);
        zil_llvm_api::validate_module(&m).unwrap();
        assert_eq!(run(&mut m), Stats::default());
        assert_eq!(m, expected);
    }

    #[test]
    fn every_global_initializer_is_a_root_including_nested_private_tables() {
        let mut m = module(vec![
            caller("callback", Linkage::Private, "helper"),
            leaf("helper", Linkage::Internal),
            leaf("dead", Linkage::Private),
        ]);
        m.items.push(Item::Global(Global {
            name: "table".into(),
            linkage: Linkage::Private,
            ty: Ty::array(1, Ty::array(1, Ty::Ptr)),
            initializer: Some(Val::Array(vec![Arg::new(
                Ty::array(1, Ty::Ptr),
                Val::Array(vec![Arg::new(Ty::Ptr, Val::Global("callback".into()))]),
            )])),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        }));
        let original = m.clone();
        assert_eq!(run(&mut m).removed_names, vec!["dead"]);
        assert_eq!(
            m.items.as_slice(),
            &[
                original.items[0].clone(),
                original.items[1].clone(),
                original.items[3].clone()
            ]
        );
        zil_llvm_api::validate_module(&m).unwrap();
    }

    #[test]
    fn reachable_function_addresses_and_their_callees_survive() {
        let mut root = leaf("export_address", Linkage::External);
        root.signature.result = Ty::Ptr;
        root.blocks.as_mut().unwrap()[0].instructions = vec![Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Ptr, Val::Global("callback".into())))),
        )];
        let mut m = module(vec![
            root,
            caller("callback", Linkage::Internal, "helper"),
            leaf("helper", Linkage::Private),
            leaf("dead", Linkage::Private),
        ]);
        let before = m.clone();
        assert_eq!(run(&mut m).removed_names, vec!["dead"]);
        assert_eq!(m.items.as_slice(), &before.items[..3]);
    }

    #[test]
    fn rooted_cycles_survive_and_unrooted_cycles_are_removed() {
        let mut m = module(vec![
            caller("main", Linkage::External, "live_a"),
            caller("live_a", Linkage::Internal, "live_b"),
            caller("live_b", Linkage::Private, "live_a"),
            caller("dead_a", Linkage::Internal, "dead_b"),
            caller("dead_b", Linkage::Private, "dead_a"),
        ]);
        let before = m.clone();
        assert_eq!(run(&mut m).removed_names, vec!["dead_a", "dead_b"]);
        assert_eq!(m.items.as_slice(), &before.items[..3]);
        zil_llvm_api::validate_module(&m).unwrap();
    }

    #[test]
    fn malformed_dead_body_is_rejected_before_any_mutation() {
        let mut bad = leaf("dead_bad", Linkage::Private);
        bad.blocks.as_mut().unwrap()[0].instructions = vec![Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("missing".into())))),
        )];
        let mut m = module(vec![leaf("main", Linkage::External), bad]);
        let before = m.clone();
        let expected_error = zil_llvm_api::validate_module(&m).unwrap_err().to_string();
        assert_eq!(
            prune(&mut m, &|| false).unwrap_err().to_string(),
            expected_error
        );
        assert_eq!(m, before);
    }

    #[test]
    fn unknown_reachable_value_suppresses_pruning_and_remains_for_production_validation() {
        let mut root = leaf("main", Linkage::External);
        root.blocks.as_mut().unwrap()[0].instructions = vec![Inst::new(
            None,
            Op::Return(Some(Arg::new(Ty::Int(32), Val::Invalid("unknown".into())))),
        )];
        let mut m = module(vec![root, leaf("dead", Linkage::Private)]);
        let before = m.clone();
        assert_eq!(run(&mut m), Stats::default());
        assert_eq!(m, before);
        assert!(zil_llvm_api::validate_module(&m).is_err());
    }

    #[test]
    fn cancellation_including_after_original_validation_never_publishes_partial_deletion() {
        let original = module(vec![
            leaf("dead_a", Linkage::Internal),
            leaf("main", Linkage::External),
            leaf("dead_b", Linkage::Private),
        ]);
        let polls = AtomicUsize::new(0);
        let mut measured = original.clone();
        prune(&mut measured, &|| {
            polls.fetch_add(1, Ordering::Relaxed);
            false
        })
        .unwrap()
        .unwrap();
        let total = polls.load(Ordering::Relaxed);
        for stop in [0, total / 2, total - 2, total - 1] {
            let polls = AtomicUsize::new(0);
            let mut m = original.clone();
            assert!(
                prune(&mut m, &|| polls.fetch_add(1, Ordering::Relaxed) >= stop)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(m, original);
        }
    }
}
