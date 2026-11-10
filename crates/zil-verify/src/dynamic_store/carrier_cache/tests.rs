//! Complete frozen 97d carrier admission, independent of the registry.
use super::*;
use crate::dynamic_store::tests::{pod_both, pod_protocol_fixture, scoped_pod_fixture};
fn old_carrier(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    target(&module.target, span)?;
    let Some(element) = element(ty) else {
        return Err(reject(
            "E-VERIFY-OWNED-STORE",
            span,
            "expected a Store carrier",
        ));
    };
    check_store_element(module, element, span, budget)?;
    pod_name_work(element, budget)?;
    let name = definition_name(element);
    budget.spend(
        module
            .resources
            .len()
            .saturating_mul(name.len().saturating_add(1)),
    )?;
    if plain_element(element) {
        for resource in &module.resources {
            // Charge both actual trees before comparing full nominal types.
            pod_type_work(element, budget)?;
            pod_type_work(&resource.payload, budget)?;
        }
    }
    if !module.resources.iter().any(|r| r.name == name && r.teardown.is_none()
        && matches!(&r.payload, Type::OwnedStore { element: actual } if actual.as_ref() == element)) {
        return Err(reject("E-VERIFY-OWNED-STORE", span, "Store carrier lacks its canonical sealed definition"));
    }
    Ok(())
}
fn record() -> Type {
    Type::Record {
        name: "ActualPair".into(),
        fields: vec![
            ("left".into(), Type::I64),
            ("right".into(), Type::Tuple(vec![Type::U8, Type::Usize])),
        ],
    }
}
fn fixture() -> (TypedModule, CandidatePlan) {
    scoped_pod_fixture(record())
}
fn carrier(module: &TypedModule) -> &Type {
    &module.functions[0].locals[0].ty
}
fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn roots(module: &TypedModule, b: &mut Budget) -> Check<()> {
    for f in &module.functions {
        for ty in std::iter::once(&f.return_type)
            .chain(f.locals.iter().map(|l| &l.ty))
            .chain(&f.values)
        {
            check_type_shape(ty, &module.target, &b.limits.clone(), b)?;
            if b.pod_stores {
                register_actual_carrier_root(module, ty, b)?;
            }
        }
    }
    Ok(())
}
fn successes(b: &Budget) -> usize {
    b.pod_carriers
        .as_ref()
        .map_or(0, |c| c.entries.values().filter(|value| **value).count())
}
#[test]
fn pod_carrier_cache_requires_actual_registered_nodes_and_never_registers_temporaries() {
    let (m, _) = fixture();
    let ty = carrier(&m);
    let mut b = budget(1_000_000);
    b.pod_stores = true;
    check_carrier(&m, ty, None, &mut b).unwrap();
    assert!(b.pod_carriers.is_none());
    roots(&m, &mut b).unwrap();
    assert_eq!(query(&m, ty, &mut b).unwrap(), Some(false));
    check_carrier(&m, ty, None, &mut b).unwrap();
    assert_eq!(query(&m, ty, &mut b).unwrap(), Some(true));
    let before = b.pod_carriers.as_ref().unwrap().entries.clone();
    for _ in 0..3 {
        let temporary = ty.clone();
        assert_ne!(&temporary as *const Type, ty as *const Type);
        assert_eq!(query(&m, &temporary, &mut b).unwrap(), None);
        check_carrier(&m, &temporary, None, &mut b).unwrap();
        assert_eq!(query(&m, &temporary, &mut b).unwrap(), None);
    }
    assert_eq!(b.pod_carriers.as_ref().unwrap().entries, before);
    let mut fresh = budget(1_000_000);
    fresh.pod_stores = true;
    check_carrier(&m, ty, None, &mut fresh).unwrap();
    assert!(fresh.pod_carriers.is_none());
}
#[test]
fn pod_carrier_cache_scope_is_the_live_module_not_its_ids_or_fingerprint() {
    let (a, _) = fixture();
    let mut other = a.clone();
    other.resources.clear();
    assert_eq!(a.module_id, other.module_id);
    assert_eq!(a.source_fingerprint, other.source_fingerprint);
    let ty = carrier(&a);
    let mut b = budget(1_000_000);
    roots(&a, &mut b).unwrap();
    check_carrier(&a, ty, None, &mut b).unwrap();
    assert_eq!(query(&other, ty, &mut b).unwrap(), None);
    let expected = old_carrier(&other, ty, None, &mut budget(1_000_000)).unwrap_err();
    assert_eq!(
        check_carrier(&other, ty, None, &mut b).unwrap_err(),
        expected
    );
    assert_eq!(successes(&b), 1);
    let equal = a.clone();
    assert_eq!(a.fingerprint(), equal.fingerprint());
    assert_eq!(query(&equal, ty, &mut b).unwrap(), None);
    old_carrier(&equal, ty, None, &mut budget(1_000_000)).unwrap();
    check_carrier(&equal, ty, None, &mut b).unwrap();
    assert_eq!(successes(&b), 1);
}
#[test]
fn pod_carrier_cache_cold_errors_preserve_complete_old_diagnostics() {
    let (a, _) = fixture();
    let ty = carrier(&a);
    let mut cases = Vec::new();
    let mut m = a.clone();
    m.resources.clear();
    cases.push(m);
    let mut m = a.clone();
    m.resources[0].name = "wrong-sealed-name".into();
    cases.push(m);
    let mut m = a.clone();
    m.resources[0].teardown = Some(FunctionId(0));
    cases.push(m);
    let mut m = a.clone();
    let Type::OwnedStore { element } = &mut m.resources[0].payload else {
        panic!()
    };
    let Type::Record { fields, .. } = element.as_mut() else {
        panic!()
    };
    fields[0].1 = Type::U8;
    cases.push(m);
    let mut m = a.clone();
    m.target.pointer_width = 32;
    cases.push(m);
    let mut m = a.clone();
    m.resources.push(ResourceDefinition {
        name: "InvalidTaskContext".into(),
        payload: Type::Task {
            result: Box::new(Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }),
            retained_life: zil_ir::RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        },
        teardown: None,
    });
    cases.push(m);
    for m in cases {
        let span = Some(a.functions[0].span);
        let expected = old_carrier(&m, ty, span, &mut budget(1_000_000)).unwrap_err();
        assert_ne!(expected.category, zil_base::ResultCategory::ResourceLimit);
        let mut cold = budget(1_000_000);
        cold.pod_stores = true;
        assert_eq!(
            check_carrier(&m, ty, span, &mut cold).unwrap_err(),
            expected
        );
        assert!(cold.pod_carriers.is_none());
        let mut warm = budget(1_000_000);
        roots(&a, &mut warm).unwrap();
        check_carrier(&a, ty, None, &mut warm).unwrap();
        assert_eq!(
            check_carrier(&m, ty, span, &mut warm).unwrap_err(),
            expected
        );
        assert_eq!(successes(&warm), 1);
    }
    let mut m = a.clone();
    m.resources.clear();
    let actual = carrier(&m);
    let mut b = budget(1_000_000);
    roots(&m, &mut b).unwrap();
    assert_eq!(query(&m, actual, &mut b).unwrap(), Some(false));
    assert_eq!(
        check_carrier(&m, actual, None, &mut b),
        old_carrier(&m, actual, None, &mut budget(1_000_000))
    );
    assert_eq!(query(&m, actual, &mut b).unwrap(), Some(false));
    assert_eq!(successes(&b), 0);
}
#[test]
fn pod_carrier_cache_success_publication_and_registration_have_atomic_boundaries() {
    let (m, _) = fixture();
    let ty = carrier(&m);
    let run = |n| {
        let mut b = budget(n);
        let result = (|| {
            roots(&m, &mut b)?;
            check_carrier(&m, ty, None, &mut b)
        })();
        (result, successes(&b))
    };
    let (mut low, mut high) = (0, 1_000_000);
    assert!(run(high).0.is_ok());
    while low < high {
        let mid = (low + high) / 2;
        if run(mid).0.is_ok() {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    let n = low;
    assert_eq!(run(n).1, 1);
    assert!(run(n).0.is_ok());
    assert_eq!(
        run(n - 1).0.unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(run(n - 1).1, 0);
    let (fresh, count) = run(n);
    fresh.unwrap();
    assert_eq!(count, 1);
    eprintln!("V actual POD carrier registration+cold publication N={n}; N-1/fresh checked");
}
#[test]
fn pod_carrier_cache_full_cold_hot_and_temporary_requests_keep_exact_fresh_limits() {
    let (m, _) = fixture();
    let ty = carrier(&m);
    let run = |cached: bool, n| {
        let mut b = budget(n);
        if cached {
            roots(&m, &mut b)?;
        } else {
            for f in &m.functions {
                for t in std::iter::once(&f.return_type)
                    .chain(f.locals.iter().map(|l| &l.ty))
                    .chain(&f.values)
                {
                    check_type_shape(t, &m.target, &b.limits.clone(), &mut b)?;
                }
            }
        }
        for _ in 0..64 {
            if cached {
                check_carrier(&m, ty, None, &mut b)?;
            } else {
                old_carrier(&m, ty, None, &mut b)?;
            }
        }
        Ok::<_, Diagnostic>((
            b.pod_stores,
            b.head_descriptions,
            b.tasks,
            b.registrations,
            b.generic_mutex,
        ))
    };
    let boundary = |cached| {
        let (mut low, mut high) = (0, 1_000_000);
        assert!(run(cached, high).is_ok());
        while low < high {
            let mid = (low + high) / 2;
            if run(cached, mid).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        low
    };
    let n = boundary(true);
    let old_n = boundary(false);
    assert!(n < old_n, "cached N={n}, old N={old_n}");
    assert_eq!(run(true, n).unwrap(), run(false, old_n).unwrap());
    assert_eq!(
        run(true, n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(
        run(false, old_n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(run(true, n).unwrap(), run(true, n).unwrap());
    eprintln!("V full POD carrier x64 N={n}, uncached N={old_n}; N-1/fresh checked");
}
#[test]
fn pod_carrier_cache_real_raw_wire_requests_rebuild_and_reject_actual_metadata_attacks() {
    let limits = Limits::default();
    // The existing complete protocol consumes the SAME actual Desc SSA node
    // in Push/Push/Pop. New's constructed temporary Type must remain cold.
    let (protocol, mut protocol_plan) = pod_protocol_fixture();
    protocol_plan.module_fingerprint = protocol.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&protocol, &protocol_plan, &limits).unwrap();
    for wire in [false, true] {
        HITS.with(|hits| hits.set(0));
        if wire {
            verify_module_bytes(&bytes, &protocol.target, &limits).unwrap();
        } else {
            verify_module(
                protocol.clone(),
                protocol_plan.clone(),
                &protocol.target,
                &limits,
            )
            .unwrap();
        }
        assert!(
            HITS.with(|hits| hits.get()) > 0,
            "real raw reconstruction did not use the registered checker; wire={wire}"
        );
    }
    // Keep the original minimal owner/cleanup program and every metadata
    // attack. It has no repeated Desc use, so its admission can stay cold.
    let (m, plan) = fixture();
    for result in pod_both(&m, &plan, &limits) {
        result.unwrap();
    }
    let mut cases = Vec::new();
    let mut bad = m.clone();
    bad.resources.clear();
    cases.push(bad);
    let mut bad = m.clone();
    bad.resources[0].teardown = Some(FunctionId(0));
    cases.push(bad);
    let mut bad = m.clone();
    bad.resources.push(bad.resources[0].clone());
    cases.push(bad);
    let mut bad = m.clone();
    bad.resources[0].name = "not-canonical".into();
    cases.push(bad);
    let mut bad = m.clone();
    let Type::OwnedStore { element } = &mut bad.resources[0].payload else {
        panic!()
    };
    let Type::Record { fields, .. } = element.as_mut() else {
        panic!()
    };
    fields[0].1 = Type::U8;
    cases.push(bad);
    let mut bad = m.clone();
    bad.target.pointer_width = 32;
    cases.push(bad);
    let mut bad = m.clone();
    bad.resources.push(ResourceDefinition {
        name: "BadTaskContext".into(),
        payload: Type::Task {
            result: Box::new(Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }),
            retained_life: zil_ir::RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        },
        teardown: None,
    });
    cases.push(bad);
    let task_case = cases.len() - 1;
    for (index, bad) in cases.into_iter().enumerate() {
        for result in pod_both(&bad, &plan, &limits) {
            let errors = result.unwrap_err();
            assert!(!errors.is_empty());
            assert_ne!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
            assert!(!errors[0].message.contains("fingerprint"));
            if index == task_case {
                assert!(errors[0].message.to_ascii_lowercase().contains("task"));
            }
        }
    }
    for result in pod_both(&m, &plan, &limits) {
        result.unwrap();
    }
}
