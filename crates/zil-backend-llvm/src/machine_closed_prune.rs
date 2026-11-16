//! Structured global reachability after closed-executable internalization.
//!
//! Remove unreachable local definitions and unused current class declarations.
//! Globals, types, ordinary declarations and nonlocal definitions survive. A nonempty plan must pass
//! complete ORIGINAL unit validation before any deletion; callers still retain
//! their production budgets and all actual-unit preflight/Builder/O2 checks.
use std::collections::{BTreeSet, HashMap};
use zil_llvm_api::model::{Block, Item, Module, SharedVec, Val};

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
#[cfg(test)]
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
#[cfg(test)]
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

/// The class producer owns these exact current implementation names. After
/// local dead-definition pruning, an unused prototype for an imported class
/// must not spread a changed member's class spelling to unrelated objects.
/// Ordinary declarations are untouched. All original unit fields are validated
/// before any deletion, including otherwise unused malformed prototypes.
#[cfg(test)]
pub(crate) fn prune_class_declarations(
    module: &mut Module,
    classes: &std::collections::BTreeSet<String>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Option<usize>, zil_llvm_api::Error> {
    if cancelled() {
        return Ok(None);
    }
    if classes.is_empty() {
        return Ok(Some(0));
    }
    let mut referenced = std::collections::BTreeSet::new();
    let mut pending = Vec::new();
    let mut remaining = 1_000_000usize;
    for item in &module.items {
        if cancelled() {
            return Ok(None);
        }
        match item {
            Item::Function(function) => {
                for block in function.blocks.iter().flatten() {
                    for inst in &block.instructions {
                        pending.extend(inst.op.values());
                    }
                }
            }
            Item::Global(global) => pending.extend(global.initializer.as_ref()),
            Item::NamedType(..) => {}
        }
        while let Some(value) = pending.pop() {
            if cancelled() {
                return Ok(None);
            }
            let Some(next) = remaining.checked_sub(1) else {
                return Ok(Some(0));
            };
            remaining = next;
            match value {
                Val::Global(name) => {
                    referenced.insert(name.clone());
                }
                Val::Array(args) => {
                    if args.len() > remaining.saturating_sub(pending.len()) {
                        return Ok(Some(0));
                    }
                    pending.extend(args.iter().map(|arg| &arg.value));
                }
                Val::Gep { ptr, indices, .. } => {
                    if indices.len().saturating_add(1) > remaining.saturating_sub(pending.len()) {
                        return Ok(Some(0));
                    }
                    pending.push(ptr);
                    pending.extend(indices.iter().map(|arg| &arg.value));
                }
                Val::Invalid(_) => return Ok(Some(0)),
                _ => {}
            }
            if pending.len() > remaining {
                return Ok(Some(0));
            }
        }
    }
    let remove = module
        .items
        .iter()
        .map(|item| {
            matches!(item, Item::Function(f) if f.blocks.is_none()
            && classes.contains(&f.name) && !referenced.contains(&f.name))
        })
        .collect::<Vec<_>>();
    let count = remove.iter().filter(|&&remove| remove).count();
    if count == 0 {
        return Ok(Some(0));
    }
    zil_llvm_api::validate_module(module)?;
    let mut retained = Vec::with_capacity(module.items.len() - count);
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
    Ok(Some(count))
}

#[derive(Default)]
struct Reads {
    functions: Vec<usize>,
    // Charge every logical use, even when actual immutable bodies share this
    // observation. Saturation only disables optional declaration pruning.
    operands: usize,
}

fn observe(
    value: &Val,
    names: &HashMap<&str, usize>,
    reads: &mut Reads,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Option<bool> {
    let mut values = Vec::new();
    let mut first = Some(value);
    while let Some(value) = first.take().or_else(|| values.pop()) {
        if cancelled() {
            return None;
        }
        reads.operands = reads.operands.saturating_add(1).min(1_000_001);
        match value {
            Val::Global(name) => {
                if let Some(&index) = names.get(name.as_str()) {
                    reads.functions.push(index);
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

/// Stage both old pruning results against one COMPLETE original unit. Live
/// declarations are discovered by the same actual global-reference walk as
/// definitions, so there is no second graph, intermediate clone or validation.
/// Unknown values refuse both deletions; cancellation never publishes either.
pub(crate) fn prune_closed_unit(
    module: &mut Module,
    classes: &BTreeSet<String>,
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
        if let Item::Function(f) = item {
            if names.insert(f.name.as_str(), index).is_some() {
                zil_llvm_api::validate_module(module)?;
                return Ok(Some(Stats::default()));
            }
            if f.blocks.is_some() && !f.linkage.is_local() {
                enqueue(index, &mut live, &mut pending);
            }
        }
    }
    // The old declaration pass admits at most one million operand values over
    // the retained unit, including nested constants and repeated body uses.
    // For known values its pending-stack bound is implied by this same total.
    let mut declaration_budget = Some(1_000_000usize);
    for item in &module.items {
        if cancelled() {
            return Ok(None);
        }
        if let Item::Global(g) = item
            && let Some(value) = &g.initializer
        {
            let mut reads = Reads::default();
            match observe(value, &names, &mut reads, cancelled) {
                None => return Ok(None),
                Some(false) => return Ok(Some(Stats::default())),
                Some(true) => {}
            }
            declaration_budget = declaration_budget.and_then(|n| n.checked_sub(reads.operands));
            for index in reads.functions {
                enqueue(index, &mut live, &mut pending);
            }
        }
    }
    let mut bodies = HashMap::<usize, (&SharedVec<Block>, Reads)>::new();
    while let Some(index) = pending.pop() {
        if cancelled() {
            return Ok(None);
        }
        let Item::Function(f) = &module.items[index] else {
            unreachable!("only function items are queued")
        };
        let Some(blocks) = &f.blocks else {
            continue;
        };
        let key = blocks.as_ptr() as usize;
        if !bodies.get(&key).is_some_and(|(old, _)| old.ptr_eq(blocks)) {
            let mut reads = Reads::default();
            for block in blocks {
                for inst in &block.instructions {
                    if cancelled() {
                        return Ok(None);
                    }
                    for value in inst.op.values() {
                        match observe(value, &names, &mut reads, cancelled) {
                            None => return Ok(None),
                            Some(false) => return Ok(Some(Stats::default())),
                            Some(true) => {}
                        }
                    }
                }
            }
            bodies.insert(key, (blocks, reads));
        }
        let reads = &bodies[&key].1;
        declaration_budget = declaration_budget.and_then(|n| n.checked_sub(reads.operands));
        for &index in &reads.functions {
            if cancelled() {
                return Ok(None);
            }
            enqueue(index, &mut live, &mut pending);
        }
    }
    let mut remove = vec![false; module.items.len()];
    let mut stats = Stats::default();
    let mut count = 0;
    for (index, item) in module.items.iter().enumerate() {
        if cancelled() {
            return Ok(None);
        }
        if let Item::Function(f) = item {
            if f.blocks.is_some() && f.linkage.is_local() && !live[index] {
                remove[index] = true;
                stats.removed_names.push(f.name.clone());
            } else if f.blocks.is_none()
                && declaration_budget.is_some()
                && classes.contains(&f.name)
                && !live[index]
            {
                remove[index] = true;
            }
            count += usize::from(remove[index]);
        }
    }
    stats.removed_count = stats.removed_names.len();
    if count == 0 {
        return Ok(Some(stats));
    }
    if cancelled() {
        return Ok(None);
    }
    // Includes every dead body and unused prototype, before either deletion.
    zil_llvm_api::validate_module(module)?;
    let mut retained = Vec::with_capacity(module.items.len() - count);
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
    #[test]
    fn only_unreferenced_current_class_prototypes_are_removed() {
        let mut class = leaf("zil.class.test", Linkage::External);
        class.blocks = None;
        let mut foreign = class.clone();
        foreign.name = "ordinary.foreign".into();
        let mut m = module(vec![
            class.clone(),
            foreign.clone(),
            leaf("main", Linkage::External),
        ]);
        let classes = std::collections::BTreeSet::from([class.name.clone()]);
        assert_eq!(
            prune_class_declarations(&mut m, &classes, &|| false).unwrap(),
            Some(1)
        );
        assert!(
            m.items
                .iter()
                .any(|item| matches!(item,Item::Function(f)if f.name==foreign.name))
        );
        let mut referenced = module(vec![
            class.clone(),
            caller("main", Linkage::External, &class.name),
        ]);
        let before = referenced.clone();
        assert_eq!(
            prune_class_declarations(&mut referenced, &classes, &|| false).unwrap(),
            Some(0)
        );
        assert_eq!(referenced, before);
        class.signature.result = Ty::Int(0);
        let mut malformed = module(vec![class, leaf("main", Linkage::External)]);
        let before = malformed.clone();
        assert!(prune_class_declarations(&mut malformed, &classes, &|| false).is_err());
        assert_eq!(malformed, before);
    }

    fn fused_fixture() -> (Module, BTreeSet<String>) {
        let classes: BTreeSet<String> = [
            "zil.class.live",
            "zil.class.dead_only",
            "zil.class.unused",
            "zil.class.global",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let mut functions: Vec<_> = classes
            .iter()
            .map(|name| {
                let mut f = leaf(name, Linkage::External);
                f.blocks = None;
                f
            })
            .collect();
        let root = caller("main", Linkage::External, "zil.class.live");
        let mut same_body = root.clone();
        same_body.name = "export_same_body".into();
        functions.extend([
            root,
            same_body,
            caller("dead", Linkage::Internal, "zil.class.dead_only"),
        ]);
        let mut foreign = leaf("ordinary.foreign", Linkage::External);
        foreign.blocks = None;
        functions.push(foreign);
        let mut m = module(functions);
        m.items.push(Item::Global(Global {
            name: "private_table".into(),
            linkage: Linkage::Private,
            ty: Ty::array(1, Ty::Ptr),
            initializer: Some(Val::Array(vec![Arg::new(
                Ty::Ptr,
                Val::Global("zil.class.global".into()),
            )])),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        }));
        (m, classes)
    }

    #[test]
    fn fused_pruning_matches_two_pass_outputs_and_definition_statistics() {
        let (original, classes) = fused_fixture();
        for classes in [classes, BTreeSet::new()] {
            let mut expected = original.clone();
            let stats = prune(&mut expected, &|| false).unwrap().unwrap();
            prune_class_declarations(&mut expected, &classes, &|| false)
                .unwrap()
                .unwrap();
            let mut actual = original.clone();
            assert_eq!(
                prune_closed_unit(&mut actual, &classes, &|| false).unwrap(),
                Some(stats)
            );
            assert_eq!(actual, expected);
            zil_llvm_api::validate_module(&actual).unwrap();
            let again = actual.clone();
            assert_eq!(
                prune_closed_unit(&mut actual, &classes, &|| false).unwrap(),
                Some(Stats::default())
            );
            assert_eq!(actual, again);
        }
    }

    #[test]
    fn fused_pruning_checks_dead_bodies_and_unused_prototypes_before_either_deletion() {
        let (original, classes) = fused_fixture();
        for name in ["dead", "zil.class.unused"] {
            let mut m = original.clone();
            for item in &mut m.items {
                if let Item::Function(f) = item
                    && f.name == name
                {
                    if let Some(blocks) = f.blocks.as_mut() {
                        blocks[0].instructions = vec![Inst::new(
                            None,
                            Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("missing".into())))),
                        )];
                    } else {
                        f.signature.result = Ty::Int(0);
                    }
                }
            }
            let before = m.clone();
            let expected = zil_llvm_api::validate_module(&m).unwrap_err().to_string();
            assert_eq!(
                prune_closed_unit(&mut m, &classes, &|| false)
                    .unwrap_err()
                    .to_string(),
                expected
            );
            assert_eq!(m, before);
        }
        let mut unknown = original.clone();
        for item in &mut unknown.items {
            if let Item::Function(f) = item
                && f.name == "main"
            {
                f.blocks.as_mut().unwrap()[0].instructions = vec![Inst::new(
                    None,
                    Op::Return(Some(Arg::new(Ty::Int(32), Val::Invalid("unknown".into())))),
                )];
            }
        }
        let before = unknown.clone();
        assert_eq!(
            prune_closed_unit(&mut unknown, &classes, &|| false).unwrap(),
            Some(Stats::default())
        );
        assert_eq!(unknown, before);
    }

    #[test]
    fn fused_pruning_cancellation_keeps_both_deletions_atomic() {
        let (original, classes) = fused_fixture();
        let polls = AtomicUsize::new(0);
        let mut measured = original.clone();
        prune_closed_unit(&mut measured, &classes, &|| {
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
                prune_closed_unit(&mut m, &classes, &|| polls.fetch_add(1, Ordering::Relaxed)
                    >= stop)
                .unwrap()
                .is_none()
            );
            assert_eq!(m, original);
        }
    }
}
