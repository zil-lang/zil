//! Component cache evidence, independent of executable resource/loan proofs.
use super::*;
use crate::place_replacements::fixture;
use zil_base::{Limits, ResultCategory, TargetProfile};
use zil_ir::{FunctionId, ResourceDefinition};

fn module() -> TypedModule {
    let mut module = fixture::module();
    module.target = TargetProfile::host_llvm();
    module.functions.clear();
    module.resources = vec![ResourceDefinition {
        name: "@builtin:OwnedStore<Key>".into(),
        payload: Type::OwnedStore {
            element: Box::new(Type::Record {
                name: "Key".into(),
                fields: vec![("a".into(), Type::I64), ("b".into(), Type::I32)],
            }),
        },
        teardown: None,
    }];
    module
}
fn element(module: &TypedModule) -> &Type {
    let Type::OwnedStore { element } = &module.resources[0].payload else {
        panic!("Store")
    };
    element
}
fn budget() -> Budget {
    Budget::new(&Limits::default())
}
fn preflight(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    crate::budget::preflight(module, &Limits::default(), b).map(|_| ())
}
#[test]
fn only_real_preflight_publishes_and_equal_temporaries_never_enter_the_table() {
    let m = module();
    let el = element(&m);
    let mut b = budget();
    carrier(&m, el, &mut b).unwrap();
    assert!(
        b.pod_store_carriers.is_none(),
        "a successful ordinary miss must not publish"
    );
    preflight(&m, &mut b).unwrap();
    assert!(cached_carrier(&m, el, &mut b).unwrap());
    assert_eq!(b.pod_store_carriers.as_ref().unwrap().complete.len(), 1);
    for _ in 0..3 {
        let temporary = el.clone();
        assert_ne!(el as *const Type, &temporary as *const Type);
        assert!(!cached_carrier(&m, &temporary, &mut b).unwrap());
        carrier(&m, &temporary, &mut b).unwrap();
        assert!(!cached_carrier(&m, &temporary, &mut b).unwrap());
        assert_eq!(b.pod_store_carriers.as_ref().unwrap().complete.len(), 1);
    }
    let mut fresh = budget();
    assert!(!cached_carrier(&m, el, &mut fresh).unwrap());
    carrier(&m, el, &mut fresh).unwrap();
    assert!(fresh.pod_store_carriers.is_none());
    assert!(!b.tasks && !b.registrations && !b.generic_mutex);
}
#[test]
fn module_identity_not_claimed_id_or_fingerprint_bounds_every_hit() {
    let a = module();
    let el = element(&a);
    let mut b = budget();
    preflight(&a, &mut b).unwrap();
    let identical = a.clone();
    assert_eq!(a.fingerprint(), identical.fingerprint());
    assert!(!cached_carrier(&identical, el, &mut b).unwrap());
    carrier(&identical, el, &mut b).unwrap();
    let mut other = a.clone();
    other.resources[0].name = "wrong-canonical-name".into();
    assert_eq!(a.module_id, other.module_id);
    assert_eq!(a.source_fingerprint, other.source_fingerprint);
    assert!(!cached_carrier(&other, el, &mut b).unwrap());
    // Deliberately pass A's already registered element reference to B. An
    // element-address-only or module-id/fingerprint key would wrongly hit.
    let actual = carrier(&other, el, &mut b).unwrap_err();
    let expected = carrier_uncached(&other, el, &mut budget()).unwrap_err();
    assert_eq!(actual, expected);
    assert_eq!(
        b.pod_store_carriers.as_ref().unwrap().module,
        &a as *const TypedModule as usize
    );
    assert_eq!(b.pod_store_carriers.as_ref().unwrap().complete.len(), 1);
}
#[test]
fn cold_metadata_rejections_match_the_original_complete_carrier() {
    let a = module();
    let el = element(&a);
    let mut bad_modules = Vec::new();
    let mut m = a.clone();
    m.resources.clear();
    bad_modules.push(m);
    let mut m = a.clone();
    m.resources[0].name = "unrelated".into();
    bad_modules.push(m);
    let mut m = a.clone();
    m.resources[0].teardown = Some(FunctionId(0));
    bad_modules.push(m);
    let mut m = a.clone();
    m.resources.push(m.resources[0].clone());
    bad_modules.push(m);
    let mut m = a.clone();
    let mut alias = m.resources[0].clone();
    alias.name = "alias".into();
    m.resources.push(alias);
    bad_modules.push(m);
    let mut m = a.clone();
    let Type::OwnedStore { element } = &mut m.resources[0].payload else {
        unreachable!()
    };
    let Type::Record { fields, .. } = element.as_mut() else {
        unreachable!()
    };
    fields[0].1 = Type::U8; // Same nominal element identity, different layout.
    bad_modules.push(m);
    let mut m = a.clone();
    m.target.pointer_width = 32;
    bad_modules.push(m);
    let mut m = a.clone();
    m.resources[0].payload = Type::I32;
    bad_modules.push(m);
    for m in bad_modules {
        let expected = carrier_uncached(&m, el, &mut budget()).unwrap_err();
        let mut fresh = budget();
        assert_eq!(carrier(&m, el, &mut fresh).unwrap_err(), expected);
        assert!(fresh.pod_store_carriers.is_none());
        let mut warm = budget();
        preflight(&a, &mut warm).unwrap();
        assert_eq!(carrier(&m, el, &mut warm).unwrap_err(), expected);
        assert_eq!(warm.pod_store_carriers.as_ref().unwrap().complete.len(), 1);
    }
}
#[test]
fn zero_layout_and_nested_non_pod_types_still_take_the_original_cold_rejection() {
    let m = module();
    let mut b = budget();
    preflight(&m, &mut b).unwrap();
    for bad in [
        Type::Tuple(vec![]),
        Type::Array {
            element: Box::new(Type::I64),
            length: 0,
        },
        Type::Record {
            name: "Key".into(),
            fields: vec![],
        },
        Type::Record {
            name: "Key".into(),
            fields: vec![("bad".into(), Type::TaskI32)],
        },
        Type::Record {
            name: "Key".into(),
            fields: vec![(
                "bad".into(),
                Type::Dyn {
                    interface: zil_ir::DynInterfaceId {
                        module: m.module_id,
                        index: 71,
                    },
                },
            )],
        },
        Type::Record {
            name: "Key".into(),
            fields: vec![("same".into(), Type::I32), ("same".into(), Type::I64)],
        },
    ] {
        assert_eq!(
            carrier(&m, &bad, &mut b).unwrap_err(),
            carrier_uncached(&m, &bad, &mut budget()).unwrap_err()
        );
        assert_eq!(b.pod_store_carriers.as_ref().unwrap().complete.len(), 1);
    }
}
#[test]
fn failed_first_preflight_and_failed_publication_never_record_a_success() {
    let mut bad = module();
    bad.resources[0].teardown = Some(FunctionId(0));
    let mut b = budget();
    assert!(preflight(&bad, &mut b).is_err());
    assert!(b.pod_store_carriers.is_none());
    let m = module();
    let el = element(&m);
    carrier_uncached(&m, el, &mut budget()).unwrap();
    // Isolated publication fee check, with a separately established cold
    // result; no test bypass is reachable from the real preflight caller.
    for n in 0..5 {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: n,
            ..Limits::default()
        });
        assert!(
            publish_carrier(&m, el, &mut b)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        assert!(b.pod_store_carriers.is_none());
    }
    let mut enough = Budget::new(&Limits {
        max_analysis_steps: 5,
        ..Limits::default()
    });
    publish_carrier(&m, el, &mut enough).unwrap();
    assert_eq!(
        enough.pod_store_carriers.as_ref().unwrap().complete.len(),
        1
    );
}
#[test]
fn successful_module_and_actual_type_nodes_still_recheck_all_dynamic_store_instructions() {
    let m = module();
    let mut b = budget();
    preflight(&m, &mut b).unwrap();
    let mut raw = fixture::module().functions.remove(0);
    raw.values = vec![Type::I32].into();
    let i = fixture::instruction(
        Some(ValueId(0)),
        InstructionKind::OwnedStoreLen { store: ValueId(0) },
    );
    assert!(
        instruction(&raw, &i, &mut b)
            .unwrap_err()
            .iter()
            .any(|e| e.message.contains("Store carrier"))
    );
}
#[test]
fn hot_cold_and_temporary_paths_have_exact_fresh_request_boundaries() {
    let m = module();
    let el = element(&m);
    let temporary = el.clone();
    let run = |mode, work| {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: work,
            ..Limits::default()
        });
        preflight(&m, &mut b)?;
        for _ in 0..64 {
            match mode {
                0 => carrier(&m, el, &mut b)?,
                1 => carrier_uncached(&m, el, &mut b)?,
                _ => carrier(&m, &temporary, &mut b)?,
            }
        }
        assert_eq!(b.pod_store_carriers.as_ref().unwrap().complete.len(), 1);
        Ok::<_, Vec<Diagnostic>>(())
    };
    let minimum = |mode| {
        let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if run(mode, mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        run(mode, lo).unwrap();
        assert!(
            run(mode, lo - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        run(mode, lo).unwrap();
        lo
    };
    let hot = minimum(0);
    let cold = minimum(1);
    let cloned = minimum(2);
    assert!(
        hot < cold && cold < cloned,
        "hot={hot} cold={cold} clone={cloned}"
    );
    println!(
        "POD static carrier: hot N={hot}, original cold N={cold}, unregistered temporary N={cloned}; N-1 rejects and fresh N passes"
    );
}

#[test]
fn nonempty_publication_prepays_both_searches_and_preserves_the_old_table_on_failure() {
    let mut two = module();
    two.resources.push(ResourceDefinition {
        name: "@builtin:OwnedStore<i64>".into(),
        payload: Type::OwnedStore {
            element: Box::new(Type::I64),
        },
        teardown: None,
    });
    let first = element(&two);
    let Type::OwnedStore { element: second } = &two.resources[1].payload else {
        unreachable!()
    };
    carrier_uncached(&two, first, &mut budget()).unwrap();
    carrier_uncached(&two, second, &mut budget()).unwrap();
    let old = BTreeSet::from([first as *const Type as usize]);
    for n in 0..6 {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: n,
            ..Limits::default()
        });
        b.pod_store_carriers = Some(CarrierCache {
            module: &two as *const TypedModule as usize,
            complete: old.clone(),
        });
        assert!(
            publish_carrier(&two, second, &mut b)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(b.pod_store_carriers.as_ref().unwrap().complete, old);
    }
    let mut b = Budget::new(&Limits {
        max_analysis_steps: 6,
        ..Limits::default()
    });
    b.pod_store_carriers = Some(CarrierCache {
        module: &two as *const TypedModule as usize,
        complete: old,
    });
    publish_carrier(&two, second, &mut b).unwrap();
    assert_eq!(b.pod_store_carriers.as_ref().unwrap().complete.len(), 2);
}
