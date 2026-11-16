//! Cold-request sharing and counterexamples at the actual class-pass boundary.
use super::*;
use std::sync::Arc;
use zil_llvm_api::model::{Attr, Global, Inst, Param, Signature};

fn body(name: &str, instructions: Vec<Inst>) -> Function {
    Function {
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
    }
}

fn ret(value: Val) -> Inst {
    Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), value))))
}

fn leaf(name: &str, value: i128) -> Function {
    body(name, vec![ret(Val::Int(value))])
}

fn call(name: &str, target: &str) -> Function {
    body(
        name,
        vec![
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
            ret(Val::Local("v".into())),
        ],
    )
}

fn renamed(function: &Function, name: &str) -> Function {
    let mut copy = function.clone();
    copy.name = name.into();
    assert!(same_body_owner(function, &copy));
    copy
}

fn module(functions: Vec<Function>) -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "diagnostic-selector-is-not-authority".into(),
        items: functions.into_iter().map(Item::Function).collect(),
    }
}

fn function<'a>(module: &'a Module, name: &str) -> &'a Function {
    module
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(function) if function.name == name => Some(function),
            _ => None,
        })
        .unwrap()
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

fn target(module: &Module, name: &str) -> String {
    function(module, name).blocks.as_ref().unwrap()[0]
        .instructions
        .iter()
        .find_map(|inst| match &inst.op {
            Op::Call {
                callee: Val::Global(name),
                ..
            } => Some(name.clone()),
            _ => None,
        })
        .unwrap()
}

// Preserve the complete current model while deliberately removing every shared
// body owner. Comparing full transformed models exercises both admission and
// staged rewriting with and without request-local pointer hits.
fn without_sharing(module: &Module) -> Module {
    let mut copy = module.clone();
    for item in &mut copy.items {
        if let Item::Function(function) = item {
            function.blocks = function
                .blocks
                .as_ref()
                .map(|blocks| blocks.iter().cloned().collect());
        }
    }
    assert_eq!(copy, *module);
    copy
}

fn compare_unshared(
    source: &Module,
    protected: &BTreeSet<String>,
    previous: Option<&Cache>,
) -> (Module, Stats, Cache) {
    let mut actual = source.clone();
    let (stats, symbols, cache) =
        canonicalize_stable_cached(&mut actual, protected, &|| false, previous).unwrap();
    let mut expected = without_sharing(source);
    let (expected_stats, expected_symbols, _) =
        canonicalize_stable_cached(&mut expected, protected, &|| false, None).unwrap();
    assert_eq!(stats, expected_stats);
    assert_eq!(symbols, expected_symbols);
    assert_eq!(actual, expected);
    (actual, stats, cache)
}

#[test]
fn first_request_shares_actual_facts_normalized_bodies_and_rewritten_bodies() {
    const COPIES: usize = 96;
    let template = leaf("zil.fn.0", 7);
    let caller = call("zil.fn.96", "zil.fn.0");
    let mut functions = (0..COPIES)
        .map(|i| renamed(&template, &format!("zil.fn.{i}")))
        .collect::<Vec<_>>();
    functions.extend((COPIES..2 * COPIES).map(|i| renamed(&caller, &format!("zil.fn.{i}"))));
    functions.push(call("main", "zil.fn.191"));
    let source = module(functions);
    let (actual, stats, cache) = compare_unshared(&source, &BTreeSet::new(), None);
    assert_eq!(
        stats,
        Stats {
            eligible_functions: 2 * COPIES,
            equivalence_classes: 2,
            rewritten_calls: COPIES + 1,
        }
    );
    assert_eq!(cache.body_owners, 3);
    assert_eq!(cache.address_scans, 3);
    assert_eq!(
        (cache.rebuilt_inputs, cache.shared_inputs),
        (3, 2 * COPIES - 2)
    );
    assert_eq!((cache.rebuilt_keys, cache.shared_keys), (2, 2 * COPIES - 2));
    assert_eq!(cache.reused_inputs, cache.shared_inputs);
    assert_eq!(cache.reused_keys, cache.shared_keys);
    for i in 1..COPIES {
        assert!(Arc::ptr_eq(
            &cache.functions[0].addresses,
            &cache.functions[i].addresses
        ));
        assert!(Arc::ptr_eq(
            &cache.functions[0].dependencies,
            &cache.functions[i].dependencies
        ));
    }
    let normalized = &cache.functions[COPIES]
        .normalized
        .as_ref()
        .unwrap()
        .function;
    for i in (COPIES + 1)..2 * COPIES {
        let other = &cache.functions[i].normalized.as_ref().unwrap().function;
        assert!(same_body_owner(normalized, other));
        assert!(same_body_owner(
            function(&actual, "zil.fn.96"),
            function(&actual, &format!("zil.fn.{i}"))
        ));
    }
    // Normalization really rewrote the call, making one new immutable owner.
    assert!(!same_body_owner(normalized, &caller));
    assert_eq!(target(&source, "zil.fn.96"), "zil.fn.0");
    assert!(target(&actual, "zil.fn.96").starts_with("zil.class."));
}

#[test]
fn cow_mutation_and_independently_equal_owners_miss_request_facts() {
    let template = leaf("zil.fn.0", 7);
    let mut mutated = renamed(&template, "zil.fn.1");
    mutated.blocks.as_mut().unwrap()[0].instructions[0] = ret(Val::Int(9));
    let independent = leaf("zil.fn.2", 7);
    assert!(!same_body_owner(&template, &mutated));
    assert!(!same_body_owner(&template, &independent));
    let source = module(vec![
        template.clone(),
        mutated,
        independent,
        renamed(&template, "zil.fn.3"),
        call("main", "zil.fn.3"),
    ]);
    let (_, stats, cache) = compare_unshared(&source, &BTreeSet::new(), None);
    assert_eq!(
        (stats.eligible_functions, stats.equivalence_classes),
        (4, 2)
    );
    assert_eq!((cache.body_owners, cache.address_scans), (4, 4));
    assert_eq!((cache.rebuilt_inputs, cache.shared_inputs), (4, 1));
    assert_eq!((cache.rebuilt_keys, cache.shared_keys), (3, 1));
    assert!(Arc::ptr_eq(
        &cache.functions[0].dependencies,
        &cache.functions[3].dependencies
    ));
    assert!(!Arc::ptr_eq(
        &cache.functions[0].dependencies,
        &cache.functions[2].dependencies
    ));
    let key = |i: usize| &cache.functions[i].normalized.as_ref().unwrap().function;
    assert_eq!(key(0), key(2));
    assert_ne!(key(0), key(1));
    assert!(!same_body_owner(key(0), key(2)));
    assert!(same_body_owner(key(0), key(3)));
}

#[test]
fn shared_owner_still_binds_complete_signature_linkage_and_attributes() {
    let template = leaf("zil.fn.0", 7);
    let mut result_attrs = renamed(&template, "zil.fn.1");
    result_attrs.signature.result_attrs.push(Attr::ZeroExt);
    let mut param_attrs = renamed(&template, "zil.fn.2");
    param_attrs
        .signature
        .params
        .push(Param::new(Ty::Int(32), "p"));
    param_attrs.signature.params[0].attrs.push(Attr::SignExt);
    let mut param_name = renamed(&param_attrs, "zil.fn.3");
    param_name.signature.params[0].name = Some("different".into());
    let mut attrs = renamed(&template, "zil.fn.4");
    attrs.attrs.push(Attr::NoInline);
    let mut variadic = renamed(&template, "zil.fn.5");
    variadic.signature.variadic = true;
    let mut available = renamed(&template, "zil.fn.6");
    available.linkage = Linkage::AvailableExternally;
    let source = module(vec![
        template.clone(),
        result_attrs,
        param_attrs,
        param_name,
        attrs,
        variadic,
        available,
        renamed(&template, "zil.fn.7"),
        call("main", "zil.fn.7"),
    ]);
    let (_, stats, cache) = compare_unshared(&source, &BTreeSet::new(), None);
    assert_eq!((cache.body_owners, cache.address_scans), (2, 2));
    assert_eq!((cache.rebuilt_inputs, cache.shared_inputs), (8, 1));
    assert_eq!(
        (stats.eligible_functions, stats.equivalence_classes),
        (6, 5)
    );
    assert_eq!((cache.rebuilt_keys, cache.shared_keys), (5, 1));
    assert!(cache.functions[5].dependencies.as_ref().is_none());
    assert!(cache.functions[6].dependencies.as_ref().is_none());
    assert!(!Arc::ptr_eq(
        &cache.functions[0].dependencies,
        &cache.functions[1].dependencies
    ));
    assert!(Arc::ptr_eq(
        &cache.functions[0].addresses,
        &cache.functions[6].addresses
    ));

    // Another linkage on this very owner must reach the stable-mode rejection.
    let mut internal = renamed(&template, "zil.fn.8");
    internal.linkage = Linkage::Internal;
    let mut invalid = module(vec![template, internal, call("main", "zil.fn.8")]);
    let before = invalid.clone();
    assert!(canonicalize_stable_cached(&mut invalid, &BTreeSet::new(), &|| false, None).is_none());
    assert_eq!(invalid, before);
}

#[test]
fn same_caller_owner_uses_current_callee_classes_after_an_edit() {
    let template = leaf("zil.fn.0", 7);
    let caller = call("zil.fn.2", "zil.fn.0");
    let original = module(vec![
        template.clone(),
        renamed(&template, "zil.fn.1"),
        caller.clone(),
        renamed(&caller, "zil.fn.3"),
        call("main", "zil.fn.3"),
    ]);
    let (first, _, previous) = compare_unshared(&original, &BTreeSet::new(), None);
    let mut edited = original.clone();
    function_mut(&mut edited, "zil.fn.0")
        .blocks
        .as_mut()
        .unwrap()[0]
        .instructions[0] = ret(Val::Int(9));
    assert!(same_body_owner(
        function(&original, "zil.fn.2"),
        function(&edited, "zil.fn.2")
    ));
    let (actual, _, cache) = compare_unshared(&edited, &BTreeSet::new(), Some(&previous));
    assert_ne!(target(&first, "main"), target(&actual, "main"));
    assert_eq!((cache.reused_inputs, cache.rebuilt_inputs), (4, 1));
    assert_eq!(
        (cache.reused_keys, cache.rebuilt_keys, cache.shared_keys),
        (2, 2, 1)
    );
    let key = cache.functions[2].normalized.as_ref().unwrap();
    assert_eq!(key.callees, vec![(0, target(&actual, "zil.fn.2"))]);
    assert!(same_body_owner(
        &key.function,
        &cache.functions[3].normalized.as_ref().unwrap().function
    ));
    assert!(!same_body_owner(
        &key.function,
        &previous.functions[2].normalized.as_ref().unwrap().function
    ));

    let protected = BTreeSet::from(["zil.fn.0".to_owned()]);
    let (excluded, stats, _) = compare_unshared(&edited, &protected, Some(&cache));
    assert_eq!(stats.eligible_functions, 1);
    assert_eq!(excluded, edited);
    assert_eq!(target(&excluded, "main"), "zil.fn.3");
}

#[test]
fn callee_header_change_rechecks_shared_body_dependencies() {
    let caller = call("zil.fn.1", "zil.fn.0");
    let original = module(vec![
        leaf("zil.fn.0", 7),
        caller.clone(),
        renamed(&caller, "zil.fn.2"),
        call("main", "zil.fn.2"),
    ]);
    assert!(zil_llvm_api::validate_module(&original).is_ok());
    let (_, _, previous) = compare_unshared(&original, &BTreeSet::new(), None);
    let mut edited = original;
    function_mut(&mut edited, "zil.fn.0")
        .signature
        .params
        .push(Param::new(Ty::Int(32), "added"));
    // The complete original-model preflight must reject this ABI mismatch in
    // production. Even this pass alone cannot reuse its earlier call admission.
    assert!(zil_llvm_api::validate_module(&edited).is_err());
    let (actual, stats, cache) = compare_unshared(&edited, &BTreeSet::new(), Some(&previous));
    assert_eq!(stats.eligible_functions, 1);
    // compare_unshared already checks complete transformed-module equality.
    // The callee itself remains eligible, so even excluded callers can have
    // their direct targets rewritten to its current class. This pass requires
    // original-model preflight: neither raw model is a valid production input.
    assert!(zil_llvm_api::validate_module(&actual).is_err());
    assert_eq!((cache.rebuilt_inputs, cache.shared_inputs), (3, 1));
    assert!(cache.functions[1].dependencies.as_ref().is_none());
    assert!(Arc::ptr_eq(
        &cache.functions[1].dependencies,
        &cache.functions[2].dependencies
    ));
}

#[test]
fn new_argument_global_and_protected_exclusions_apply_to_each_current_symbol() {
    let template = leaf("zil.fn.0", 7);
    let mut sink = call("sink", "foreign");
    let Op::Call { args, .. } = &mut sink.blocks.as_mut().unwrap()[0].instructions[0].op else {
        panic!()
    };
    args.push(Arg::new(Ty::Ptr, Val::Zero));
    let mut foreign = leaf("foreign", 0);
    foreign.blocks = None;
    foreign.signature.params.push(Param::unnamed(Ty::Ptr));
    let mut functions = (0..5)
        .map(|i| renamed(&template, &format!("zil.fn.{i}")))
        .collect::<Vec<_>>();
    functions.extend([
        call("main", "zil.fn.4"),
        sink.clone(),
        renamed(&sink, "sink_copy"),
        foreign,
    ]);
    let original = module(functions);
    let (_, _, previous) = compare_unshared(&original, &BTreeSet::new(), None);
    assert!(Arc::ptr_eq(
        &previous.functions[6].addresses,
        &previous.functions[7].addresses
    ));
    let mut edited = original;
    let Op::Call { args, .. } =
        &mut function_mut(&mut edited, "sink").blocks.as_mut().unwrap()[0].instructions[0].op
    else {
        panic!()
    };
    args[0].value = Val::Global("zil.fn.1".into());
    edited.items.push(Item::Global(Global {
        name: "nested_address".into(),
        linkage: Linkage::External,
        ty: Ty::array(1, Ty::array(1, Ty::Ptr)),
        initializer: Some(Val::Array(vec![Arg::new(
            Ty::array(1, Ty::Ptr),
            Val::Array(vec![Arg::new(Ty::Ptr, Val::Global("zil.fn.2".into()))]),
        )])),
        constant: true,
        unnamed_addr: false,
        alignment: 8,
    }));
    let protected = BTreeSet::from(["zil.fn.0".to_owned()]);
    let (actual, stats, cache) = compare_unshared(&edited, &protected, Some(&previous));
    assert_eq!(
        (stats.eligible_functions, stats.equivalence_classes),
        (2, 1)
    );
    assert!(cache.functions[6].addresses.contains("zil.fn.1"));
    assert!(cache.functions[7].addresses.is_empty());
    assert!(!Arc::ptr_eq(
        &cache.functions[6].addresses,
        &cache.functions[7].addresses
    ));
    for i in 0..3 {
        assert_eq!(
            function(&actual, &format!("zil.fn.{i}")),
            function(&edited, &format!("zil.fn.{i}"))
        );
    }
    assert_eq!(function(&actual, "sink"), function(&edited, "sink"));
    assert_eq!(target(&actual, "sink"), "foreign");
    assert!(target(&actual, "main").starts_with("zil.class."));
}
