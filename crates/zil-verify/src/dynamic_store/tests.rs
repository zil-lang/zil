//! Raw layout/qualification boundary controls. These do not claim source or
//! native coverage; the driver and non-author suites exercise that full path.
use super::*;
use zil_base::ModuleId;

fn zero() -> Type {
    Type::Record {
        name: "Zero".into(),
        fields: vec![],
    }
}

fn module(captures: Vec<Type>) -> TypedModule {
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"201 raw Store layout controls"),
        target: TargetProfile::host_llvm(),
        functions: vec![],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            kind: CallableKind::Fn,
            captures,
            parameters: vec![],
            result: Type::I32,
            lending_captures: vec![],
        }],
        resources: vec![
            ResourceDefinition {
                name: "Zero".into(),
                payload: zero(),
                teardown: None,
            },
            ResourceDefinition {
                name: byte_buffers::NAME.into(),
                payload: Type::ByteBuffer,
                teardown: None,
            },
        ],
        unsupported: vec![],
    }
}

fn element(callbacks: &[u32]) -> Type {
    let mut fields = vec![("token".into(), Type::Managed(Box::new(zero())))];
    fields.extend(
        callbacks
            .iter()
            .enumerate()
            .map(|(index, id)| (format!("callback{index}"), Type::Closure(ClosureId(*id)))),
    );
    Type::Package {
        name: "Item".into(),
        fields,
    }
}

fn admit(module: &TypedModule, element: &Type, limits: &Limits) -> Check<()> {
    check_element_with_limits(
        module,
        element,
        None,
        Some(limits),
        &mut Budget::new(limits),
    )
}

#[test]
fn concrete_capture_alignment_and_appended_live_flags_determine_stride() {
    let mut m = module(vec![
        Type::U8,
        Type::I64,
        Type::Managed(Box::new(Type::ByteBuffer)),
    ]);
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(1),
        captures: vec![Type::Closure(ClosureId(0)), Type::U16],
        ..m.closures[0].clone()
    });
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    let layouts = closure_layouts(&m, ClosureId(1), Some(&limits), &mut budget).unwrap();
    // The first environment ends at byte 40 before its owned-buffer flag;
    // the flag requires tail padding to 48. The containing environment has
    // a separate child flag and rounds 48 + 2 + 1 to 56, not 50 or 48.
    assert_eq!(layouts[0], Some((48, 8)));
    assert_eq!(layouts[1], Some((56, 8)));
    assert_eq!(
        layout(&element(&[1, 1]), Some(&layouts), &mut budget).unwrap(),
        (112, 8)
    );
    admit(&m, &element(&[1, 1]), &limits).unwrap();
}

#[test]
fn actual_zero_stride_rejects_after_the_manages_package_requirement() {
    let limits = Limits::default();
    let empty = module(vec![]);
    let ty = element(&[0]);
    packages::check_type_for(&empty, &ty, None, &mut Budget::new(&limits)).unwrap();
    let error = admit(&empty, &ty, &limits).unwrap_err();
    assert!(error.message.contains("nonzero actual inline layout"));
    // Same nominal shell and zero-sized Managed field, but a real i64
    // environment supplies a valid nonzero stride.
    admit(&module(vec![Type::I64]), &ty, &limits).unwrap();
    let unit = module(vec![Type::Unit]);
    let mut budget = Budget::new(&limits);
    let layouts = closure_layouts(&unit, ClosureId(0), Some(&limits), &mut budget).unwrap();
    assert_eq!(layouts[0], Some((1, 1)));
    admit(&unit, &ty, &limits).unwrap();
}

#[test]
fn zero_payload_package_capture_still_has_actual_environment_live_flags() {
    let captured = Type::Package {
        name: "Token".into(),
        fields: vec![("empty".into(), Type::Managed(Box::new(zero())))],
    };
    let m = module(vec![captured.clone()]);
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    assert_eq!(layout(&captured, None, &mut budget).unwrap(), (0, 1));
    let layouts = closure_layouts(&m, ClosureId(0), Some(&limits), &mut budget).unwrap();
    // Owning Package captures carry one shell flag and one field flag even
    // when their data is empty. Store's nonzero condition applies to the
    // whole element, not independently to every captured payload.
    assert_eq!(layouts[0], Some((2, 1)));
    assert_eq!(
        layout(&element(&[0]), Some(&layouts), &mut budget).unwrap(),
        (2, 1)
    );
    admit(&m, &element(&[0]), &limits).unwrap();
}

#[test]
fn one_or_two_type_bindings_keep_both_fields_and_reject_extra_fields() {
    let mut m = module(vec![Type::I64]);
    let limits = Limits::default();
    admit(&m, &element(&[0]), &limits).unwrap();
    admit(&m, &element(&[0, 0]), &limits).unwrap();
    assert!(admit(&m, &element(&[0, 0, 0]), &limits).is_err());
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(1),
        ..m.closures[0].clone()
    });
    // 203 explicitly promotes this original two-root input; three fields stay closed.
    admit(&m, &element(&[0, 1]), &limits).unwrap();
}

#[test]
fn direct_and_package_hidden_store_capture_do_not_enter_the_layout_graph() {
    let store = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element(&[0])),
    }));
    let limits = Limits::default();
    for capture in [
        store.clone(),
        Type::Package {
            name: "Bag".into(),
            fields: vec![("items".into(), store)],
        },
    ] {
        let error = admit(&module(vec![capture]), &element(&[0]), &limits).unwrap_err();
        assert!(error.message.contains("finite owning environment profile"));
    }
    let error = admit(
        &module(vec![Type::Desc(Box::new(Type::ByteBuffer))]),
        &element(&[0]),
        &limits,
    )
    .unwrap_err();
    assert!(error.message.contains("self-contained environment"));
}

#[test]
fn raw_cycles_and_missing_closure_ids_reject_before_recursive_layout() {
    let limits = Limits::default();
    let mut m = module(vec![Type::Closure(ClosureId(1))]);
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(1),
        captures: vec![Type::Closure(ClosureId(0))],
        ..m.closures[0].clone()
    });
    let error = admit(&m, &element(&[0]), &limits).unwrap_err();
    assert_eq!(error.code, "E-VERIFY-CLOSURE-LAYOUT");
    let error = admit(&m, &element(&[99]), &limits).unwrap_err();
    assert_eq!(error.code, "E-VERIFY-CLOSURE");
}

#[test]
fn early_resource_admission_bounds_capture_shape_and_layout_arithmetic() {
    let mut nested = Type::I64;
    for _ in 0..8 {
        nested = Type::Tuple(vec![nested]);
    }
    let limits = Limits {
        max_call_depth: 3,
        ..Limits::default()
    };
    let error = admit(&module(vec![nested]), &element(&[0]), &limits).unwrap_err();
    assert!(error.message.contains("aggregate type nesting"));

    let huge = Type::Array {
        element: Box::new(Type::Array {
            element: Box::new(Type::I64),
            length: u32::MAX,
        }),
        length: u32::MAX,
    };
    let limits = Limits {
        max_analysis_steps: usize::MAX,
        ..Limits::default()
    };
    let error = admit(&module(vec![huge]), &element(&[0]), &limits).unwrap_err();
    assert!(
        error
            .message
            .contains("layout exceeds target address range")
    );
}

fn two_roots() -> TypedModule {
    let mut m = module(vec![Type::Managed(Box::new(Type::ByteBuffer))]);
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(1),
        captures: vec![Type::U16, Type::Unit],
        ..m.closures[0].clone()
    });
    m
}

#[test]
fn impl203_two_actual_layout_roots_include_each_environment_and_flags() {
    let m = two_roots();
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    let layouts = closure_layouts_pair(
        &m,
        ClosureId(0),
        Some(ClosureId(1)),
        Some(&limits),
        &mut budget,
    )
    .unwrap();
    assert_eq!(layouts, vec![Some((32, 8)), Some((4, 2))]);
    assert_eq!(
        layout(&element(&[0, 1]), Some(&layouts), &mut budget).unwrap(),
        (40, 8)
    );
    admit(&m, &element(&[0, 1]), &limits).unwrap();
    admit(&m, &element(&[1, 0]), &limits).unwrap();
    // Independent environments can share an already sized child type. Both
    // root layouts still include their own child bytes and child live flag.
    let mut shared = module(vec![Type::Managed(Box::new(Type::ByteBuffer))]);
    for (id, tail) in [(1, Type::U8), (2, Type::I64)] {
        shared.closures.push(ClosureDefinition {
            id: ClosureId(id),
            function: FunctionId(id),
            captures: vec![Type::Closure(ClosureId(0)), tail],
            ..shared.closures[0].clone()
        });
    }
    let layouts = closure_layouts_pair(
        &shared,
        ClosureId(1),
        Some(ClosureId(2)),
        Some(&limits),
        &mut budget,
    )
    .unwrap();
    assert_eq!(layouts, vec![Some((32, 8)), Some((40, 8)), Some((48, 8))]);
    assert_eq!(
        layout(&element(&[1, 2]), Some(&layouts), &mut budget).unwrap(),
        (88, 8)
    );
    admit(&shared, &element(&[1, 2]), &limits).unwrap();
}

#[test]
fn impl203_bad_second_capture_root_never_hides_behind_a_valid_first() {
    let limits = Limits::default();
    let store = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element(&[0])),
    }));
    let cases = [
        (
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            "E-UNSUPPORTED",
        ),
        (store.clone(), "E-UNSUPPORTED"),
        (
            Type::Package {
                name: "Bag".into(),
                fields: vec![("items".into(), store)],
            },
            "E-UNSUPPORTED",
        ),
        (Type::Closure(ClosureId(1)), "E-VERIFY-CLOSURE-LAYOUT"),
        (Type::Closure(ClosureId(99)), "E-VERIFY-CLOSURE"),
    ];
    for (capture, code) in cases {
        let mut m = two_roots();
        m.closures[1].captures = vec![capture];
        assert_eq!(
            admit(&m, &element(&[0, 1]), &limits).unwrap_err().code,
            code
        );
    }
    let mut m = two_roots();
    m.closures[1].id = ClosureId(0);
    assert_eq!(
        admit(&m, &element(&[0, 1]), &limits).unwrap_err().code,
        "E-VERIFY-CLOSURE"
    );
}

#[test]
fn impl203_heterogeneous_results_are_copy_and_preflighted_before_recursion() {
    let limits = Limits::default();
    for changed in [0, 1] {
        let mut m = two_roots();
        m.closures[changed].result = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let error = admit(&m, &element(&[0, 1]), &limits).unwrap_err();
        assert!(error.message.contains("Copy results for both"));
        // Qualification of the original single/same-root lending profile is
        // unchanged. This raw control does not assert executable loan safety.
        admit(&m, &element(&[changed as u32, changed as u32]), &limits).unwrap();
    }
    let mut m = two_roots();
    m.closures[1].result = Type::Tuple(vec![Type::Tuple(vec![Type::Tuple(vec![Type::I32])])]);
    let error = admit(
        &m,
        &element(&[0, 1]),
        &Limits {
            max_call_depth: 1,
            ..limits
        },
    )
    .unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
}

#[test]
fn impl203_two_root_qualification_has_an_exact_fresh_work_boundary() {
    let m = two_roots();
    let ty = element(&[0, 1]);
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    check_element_with_limits(&m, &ty, None, Some(&limits), &mut budget).unwrap();
    let n = limits.max_analysis_steps - budget.remaining;
    let exact = Limits {
        max_analysis_steps: n,
        ..limits.clone()
    };
    for _ in 0..2 {
        admit(&m, &ty, &exact).unwrap();
    }
    let error = admit(
        &m,
        &ty,
        &Limits {
            max_analysis_steps: n - 1,
            ..limits
        },
    )
    .unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    println!("impl203 independent two-root qualification work={n}");
}

fn captured_leaf_store() -> (Type, ResourceDefinition) {
    let leaf = Type::Package {
        name: "Leaf204".into(),
        fields: vec![
            ("marker".into(), Type::I32),
            ("token".into(), Type::Managed(Box::new(zero()))),
        ],
    };
    let payload = Type::OwnedStore {
        element: Box::new(leaf.clone()),
    };
    (
        Type::Managed(Box::new(payload.clone())),
        ResourceDefinition {
            name: definition_name(&leaf),
            payload,
            teardown: None,
        },
    )
}

#[test]
fn impl204_direct_and_package_capture_layouts_keep_every_store_flag() {
    let (store, resource) = captured_leaf_store();
    let limits = Limits::default();
    let mut m = module(vec![store.clone(), store.clone()]);
    m.resources.push(resource);
    let layouts =
        closure_layouts(&m, ClosureId(0), Some(&limits), &mut Budget::new(&limits)).unwrap();
    // Two distinct 24-byte heads and two distinct live flags, never one head
    // deduplicated by its canonical element type.
    assert_eq!(layouts[0], Some((56, 8)));
    admit(&m, &element(&[0]), &limits).unwrap();
    m.closures[0].captures = vec![Type::Package {
        name: "Bag204".into(),
        fields: vec![
            ("tag".into(), Type::I32),
            ("items".into(), store),
            ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
        ],
    }];
    let layouts =
        closure_layouts(&m, ClosureId(0), Some(&limits), &mut Budget::new(&limits)).unwrap();
    // 56 payload bytes plus shell + two Managed flags, rounded to 64.
    assert_eq!(layouts[0], Some((64, 8)));
    admit(&m, &element(&[0]), &limits).unwrap();
}

#[test]
fn impl204_fixed_store_head_never_bypasses_canonical_or_leaf_checks() {
    let (store, resource) = captured_leaf_store();
    let limits = Limits::default();
    let mut m = module(vec![store.clone()]);
    assert_eq!(
        admit(&m, &element(&[0]), &limits).unwrap_err().code,
        "E-VERIFY-OWNED-STORE"
    );
    m.resources.push(resource);
    admit(&m, &element(&[0]), &limits).unwrap();
    for variant in 0..3 {
        let mut bad = m.clone();
        let definition = bad.resources.last_mut().unwrap();
        match variant {
            0 => definition.name.push_str("wrong"),
            1 => {
                let Type::OwnedStore { element } = &mut definition.payload else {
                    unreachable!()
                };
                let Type::Package { fields, .. } = element.as_mut() else {
                    unreachable!()
                };
                fields[0].1 = Type::I64;
            }
            _ => definition.teardown = Some(FunctionId(0)),
        }
        assert_eq!(
            admit(&bad, &element(&[0]), &limits).unwrap_err().code,
            "E-VERIFY-OWNED-STORE"
        );
    }
    let Type::Managed(payload) = store else {
        unreachable!()
    };
    m.closures[0].captures = vec![Type::Desc(payload)];
    assert!(
        admit(&m, &element(&[0]), &limits)
            .unwrap_err()
            .message
            .contains("actual owning carrier")
    );

    let (mut zero_store, _) = captured_leaf_store();
    let Type::Managed(payload) = &mut zero_store else {
        unreachable!()
    };
    let Type::OwnedStore { element: leaf } = payload.as_mut() else {
        unreachable!()
    };
    let Type::Package { fields, .. } = leaf.as_mut() else {
        unreachable!()
    };
    fields.remove(0);
    m.closures[0].captures = vec![zero_store];
    assert!(
        admit(&m, &element(&[0]), &limits)
            .unwrap_err()
            .message
            .contains("nonzero")
    );
}

#[test]
fn impl204_copy_result_restriction_follows_actual_owned_closure_children() {
    let (store, resource) = captured_leaf_store();
    let limits = Limits::default();
    let mut m = module(vec![store]);
    m.resources.push(resource);
    for id in 1..3 {
        m.closures.push(ClosureDefinition {
            id: ClosureId(id),
            function: FunctionId(id),
            captures: vec![Type::Closure(ClosureId(id - 1))],
            ..m.closures[0].clone()
        });
    }
    admit(&m, &element(&[2]), &limits).unwrap();
    for changed in 0..3 {
        let mut bad = m.clone();
        bad.closures[changed].result = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        assert!(
            admit(&bad, &element(&[2]), &limits)
                .unwrap_err()
                .message
                .contains("Copy or Unit result")
        );
    }
    // The historical single-root environment with no owned Store still has
    // its original qualification behavior. This is not a complete loan proof.
    m.closures[0].captures = vec![Type::I32];
    m.closures[2].result = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    admit(&m, &element(&[2]), &limits).unwrap();
}

#[test]
fn impl204_owning_store_qualification_has_an_exact_fresh_work_boundary() {
    let (store, resource) = captured_leaf_store();
    let mut m = module(vec![store.clone(), store]);
    m.resources.push(resource);
    let ty = element(&[0]);
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    check_element_with_limits(&m, &ty, None, Some(&limits), &mut budget).unwrap();
    let n = limits.max_analysis_steps - budget.remaining;
    let exact = Limits {
        max_analysis_steps: n,
        ..limits.clone()
    };
    admit(&m, &ty, &exact).unwrap();
    let below = Limits {
        max_analysis_steps: n - 1,
        ..limits
    };
    assert_eq!(
        admit(&m, &ty, &below).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    admit(&m, &ty, &exact).unwrap();
    println!("impl204 independent owning Store qualification work={n}");
}

#[test]
fn impl204_callable_types_cannot_hide_in_copy_leaf_fields_or_resource_heads() {
    let limits = Limits::default();
    for callback in [
        Type::Closure(ClosureId(0)),
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(Type::I32),
        },
        Type::ForeignCallback {
            parameters: vec![],
            result: Box::new(Type::I32),
        },
    ] {
        for inside_resource in [false, true] {
            let (mut store, _) = captured_leaf_store();
            let Type::Managed(payload) = &mut store else {
                unreachable!()
            };
            let Type::OwnedStore { element: leaf } = payload.as_mut() else {
                unreachable!()
            };
            let Type::Package { fields, .. } = leaf.as_mut() else {
                unreachable!()
            };
            let hidden = Type::Record {
                name: "Hidden204".into(),
                fields: vec![("callback".into(), Type::Tuple(vec![callback.clone()]))],
            };
            fields.push((
                "extra".into(),
                if inside_resource {
                    Type::Managed(Box::new(hidden))
                } else {
                    hidden
                },
            ));
            let m = module(vec![store]);
            let error = admit(&m, &element(&[0]), &limits).unwrap_err();
            assert!(error.message.contains("inner leaf types cannot hide"));
        }
    }
}
