use super::tests::{function, source};
use super::*;
use crate::machine_output::AliasItem;
use crate::machine_preflight::CurrentPreflight;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use zil_ir::FunctionId;
use zil_llvm_api::model::{Arg, Global, Inst, Linkage, Op, Ty, Val};

fn alias(name: &str, value: Arc<Item>) -> AliasItem {
    AliasItem {
        name: name.into(),
        value,
    }
}

fn owned(item: Item) -> AliasItem {
    let name = match &item {
        Item::Function(f) => &f.name,
        Item::Global(g) => &g.name,
        Item::NamedType(name, _) => name,
    };
    let name = name.clone();
    alias(&name, Arc::new(item))
}

fn fixture() -> (AliasModule, BTreeSet<String>, BTreeSet<String>) {
    // All these logical functions have the SAME stored outer name and owner.
    // Only alias names may supply roots, callee bindings, stats or witness rows.
    let leaf = Arc::new(function("template.leaf", None));
    let mut declaration = function("template.declaration", None);
    let Item::Function(f) = &mut declaration else {
        unreachable!()
    };
    f.blocks = None;
    let declaration = Arc::new(declaration);
    let mut address = function("export_address", None);
    let Item::Function(f) = &mut address else {
        unreachable!()
    };
    f.signature.result = Ty::Ptr;
    f.blocks.as_mut().unwrap()[0].instructions = vec![Inst::new(
        None,
        Op::Return(Some(Arg::new(
            Ty::Ptr,
            Val::Global("zil.class.callback".into()),
        ))),
    )];
    let items = vec![
        owned(function("main", Some("zil.fn.0"))),
        alias(
            "zil.fn.0",
            Arc::new(function("template.caller", Some("zil.class.live"))),
        ),
        alias("zil.fn.1", leaf.clone()),
        alias("zil.fn.2", leaf.clone()),
        alias("zil.fn.3", leaf.clone()),
        alias("zil.fn.3.lc1", leaf.clone()),
        alias("zil.class.dead", leaf.clone()),
        alias(
            "zil.class.live",
            Arc::new(function("template.live", Some("zil.fn.1"))),
        ),
        alias("zil.class.unused", declaration.clone()),
        alias("ordinary.foreign", declaration),
        alias("native.export", leaf.clone()),
        alias(
            "zil.class.protected",
            Arc::new(function("template.protected", Some("zil.class.helper"))),
        ),
        alias("zil.class.helper", leaf.clone()),
        owned(address),
        alias(
            "zil.class.callback",
            Arc::new(function(
                "template.callback",
                Some("zil.class.address_helper"),
            )),
        ),
        alias("zil.class.address_helper", leaf),
        owned(Item::NamedType("kept.type".into(), vec![Ty::Int(32)])),
        owned(Item::Global(Global {
            name: "private.table".into(),
            linkage: Linkage::Private,
            ty: Ty::array(1, Ty::array(1, Ty::Ptr)),
            initializer: Some(Val::Array(vec![Arg::new(
                Ty::array(1, Ty::Ptr),
                Val::Array(vec![Arg::new(Ty::Ptr, Val::Global("zil.fn.2".into()))]),
            )])),
            constant: true,
            unnamed_addr: false,
            alignment: 8,
        })),
    ];
    let classes = items
        .iter()
        .filter(|item| item.name.starts_with("zil.class."))
        .map(|item| item.name.clone())
        .collect();
    (
        AliasModule {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "current.aliases".into(),
            items,
        },
        classes,
        BTreeSet::from(["main".into(), "zil.class.protected".into()]),
    )
}

fn assert_unchanged(actual: &AliasModule, original: &AliasModule) {
    assert_eq!(actual.triple, original.triple);
    assert_eq!(actual.fingerprint, original.fingerprint);
    assert_eq!(actual.items.len(), original.items.len());
    for (actual, original) in actual.items.iter().zip(&original.items) {
        assert_eq!(actual.name, original.name);
        assert!(Arc::ptr_eq(&actual.value, &original.value));
    }
}

#[test]
fn flat_and_alias_full_output_witness_cost_and_source_projection_match() {
    let source = source();
    let (mut aliases, mut classes, protected) = fixture();
    let original = aliases.clone();
    let mut flat = aliases.materialize();
    let mut flat_classes = classes.clone();
    let expected_witness = witness(&flat.views(), 11).unwrap();
    let mut flat_checks = CurrentPreflight::default();
    let expected = prune_with_preflight(
        &mut flat,
        &source.as_view(),
        &mut flat_classes,
        &protected,
        &|| false,
        &mut flat_checks,
    )
    .unwrap()
    .unwrap();
    let mut checks = CurrentPreflight::default();
    let actual = prune_aliases_with_preflight(
        &mut aliases,
        &source.as_view(),
        &mut classes,
        &protected,
        &|| false,
        &mut checks,
    )
    .unwrap()
    .unwrap();
    assert_eq!(actual, expected);
    // Four leaf source aliases (including lc1) cost two each; source0 costs three.
    assert_eq!(actual.copy_allowance, 11);
    assert_eq!(*actual.witness, expected_witness);
    assert_eq!(
        actual.removed,
        BTreeSet::from([
            "zil.fn.3".into(),
            "zil.fn.3.lc1".into(),
            "zil.class.dead".into(),
        ])
    );
    // The witness includes deleted definitions AND the unused prototype.
    for name in [
        "zil.fn.3",
        "zil.fn.3.lc1",
        "zil.class.dead",
        "zil.class.unused",
    ] {
        assert!(actual.witness.contains(&format!("[\"{name}\",")));
    }
    assert!(!actual.witness.contains("template.leaf"));
    assert!(actual.witness.contains("__zil_witness_outer__"));
    assert_eq!(classes, flat_classes);
    assert!(!classes.contains("zil.class.unused"));
    assert_eq!(checks.executions, 1);
    assert_eq!(flat_checks.executions, 1);
    // No retained alias acquired a fresh Function/header owner during reduction.
    for item in &aliases.items {
        let old = original
            .items
            .iter()
            .find(|old| old.name == item.name)
            .unwrap();
        assert!(Arc::ptr_eq(&item.value, &old.value));
    }
    let materialized = aliases.materialize();
    assert_eq!(materialized, flat);
    assert_eq!(materialized.to_text(), flat.to_text());
    zil_llvm_api::validate_module(&materialized).unwrap();
    let functions = actual
        .source_functions(&materialized, &source.as_view())
        .unwrap();
    assert_eq!(
        functions.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![FunctionId(0), FunctionId(1), FunctionId(2)]
    );
    assert_eq!(
        functions,
        expected.source_functions(&flat, &source.as_view()).unwrap()
    );
}

#[test]
fn callee_addresses_nested_globals_and_protected_roots_use_alias_names() {
    let (mut aliases, mut classes, protected) = fixture();
    prune_aliases_with_preflight(
        &mut aliases,
        &source().as_view(),
        &mut classes,
        &protected,
        &|| false,
        &mut CurrentPreflight::default(),
    )
    .unwrap()
    .unwrap();
    let names: BTreeSet<_> = aliases
        .items
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    for name in [
        "zil.fn.0",
        "zil.fn.1",
        "zil.fn.2",
        "zil.class.live",
        "zil.class.callback",
        "zil.class.address_helper",
        "zil.class.protected",
        "zil.class.helper",
        "ordinary.foreign",
        "native.export",
        "private.table",
        "kept.type",
    ] {
        assert!(names.contains(name), "lost {name}");
    }
    assert!(!names.contains("zil.fn.3"));
    assert!(!names.contains("zil.class.unused"));
}

#[test]
fn malformed_dead_alias_and_unused_prototype_are_checked_before_any_deletion() {
    let source = source();
    for name in ["zil.fn.3", "zil.class.dead", "zil.class.unused"] {
        let (mut aliases, mut classes, protected) = fixture();
        let entry = aliases
            .items
            .iter_mut()
            .find(|entry| entry.name == name)
            .unwrap();
        let Item::Function(f) = Arc::make_mut(&mut entry.value) else {
            unreachable!()
        };
        if let Some(blocks) = &mut f.blocks {
            blocks[0].instructions = vec![Inst::new(
                None,
                Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("missing".into())))),
            )];
        } else {
            f.signature.result = Ty::Int(0);
        }
        let before = aliases.clone();
        let before_classes = classes.clone();
        let mut flat = aliases.materialize();
        let expected = zil_llvm_api::validate_module(&flat)
            .unwrap_err()
            .to_string();
        let flat_error = prune_with_preflight(
            &mut flat,
            &source.as_view(),
            &mut classes.clone(),
            &protected,
            &|| false,
            &mut CurrentPreflight::default(),
        )
        .unwrap_err()
        .to_string();
        let mut checks = CurrentPreflight::default();
        let actual = prune_aliases_with_preflight(
            &mut aliases,
            &source.as_view(),
            &mut classes,
            &protected,
            &|| false,
            &mut checks,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(actual, expected);
        assert_eq!(actual, flat_error);
        assert_eq!(checks.executions, 1);
        assert_unchanged(&aliases, &before);
        assert_eq!(classes, before_classes);
        assert_eq!(flat, before.materialize());
    }
}

#[test]
fn every_cancellation_poll_including_after_preflight_preserves_aliases_and_classes() {
    let source = source();
    let (original, original_classes, protected) = fixture();
    let polls = AtomicUsize::new(0);
    let mut measured = original.clone();
    let mut flat = original.materialize();
    let mut checks = CurrentPreflight::default();
    prune_aliases_with_preflight(
        &mut measured,
        &source.as_view(),
        &mut original_classes.clone(),
        &protected,
        &|| {
            polls.fetch_add(1, Ordering::Relaxed);
            false
        },
        &mut checks,
    )
    .unwrap()
    .unwrap();
    let total = polls.load(Ordering::Relaxed);
    assert_eq!(checks.executions, 1);
    polls.store(0, Ordering::Relaxed);
    prune_with_preflight(
        &mut flat,
        &source.as_view(),
        &mut original_classes.clone(),
        &protected,
        &|| {
            polls.fetch_add(1, Ordering::Relaxed);
            false
        },
        &mut CurrentPreflight::default(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(polls.load(Ordering::Relaxed), total);
    let mut cancelled_after_check = false;
    for stop in 0..total {
        let polls = AtomicUsize::new(0);
        let mut aliases = original.clone();
        let mut classes = original_classes.clone();
        let mut checks = CurrentPreflight::default();
        assert!(
            prune_aliases_with_preflight(
                &mut aliases,
                &source.as_view(),
                &mut classes,
                &protected,
                &|| polls.fetch_add(1, Ordering::Relaxed) >= stop,
                &mut checks
            )
            .unwrap()
            .is_none()
        );
        cancelled_after_check |= checks.executions > 0;
        assert_unchanged(&aliases, &original);
        assert_eq!(classes, original_classes);
    }
    assert!(cancelled_after_check);
}

#[test]
fn duplicate_actual_aliases_validate_and_unknown_live_values_refuse_pruning() {
    for duplicate in [false, true] {
        let (mut aliases, mut classes, protected) = fixture();
        if duplicate {
            aliases.items[4].name = "zil.fn.2".into();
        } else {
            let Item::Function(f) = Arc::make_mut(&mut aliases.items[0].value) else {
                unreachable!()
            };
            f.blocks.as_mut().unwrap()[0].instructions[0].op =
                Op::Return(Some(Arg::new(Ty::Int(32), Val::Invalid("unknown".into()))));
        }
        let original = aliases.clone();
        let original_classes = classes.clone();
        let mut flat = aliases.materialize();
        let expected = prune_with_preflight(
            &mut flat,
            &source().as_view(),
            &mut classes.clone(),
            &protected,
            &|| false,
            &mut CurrentPreflight::default(),
        );
        let actual = prune_aliases_with_preflight(
            &mut aliases,
            &source().as_view(),
            &mut classes,
            &protected,
            &|| false,
            &mut CurrentPreflight::default(),
        );
        if duplicate {
            assert_eq!(
                actual.unwrap_err().to_string(),
                expected.unwrap_err().to_string()
            );
        } else {
            assert_eq!(actual.unwrap(), expected.unwrap());
            assert_eq!(classes, original_classes);
        }
        assert_unchanged(&aliases, &original);
    }
}
