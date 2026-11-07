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
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"201 raw Store layout controls"),
        target: TargetProfile::host_llvm(),
        functions: vec![],
        closures: vec![ClosureDefinition {
            storage_life: None,
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

fn pod(fields: Vec<(&str, Type)>) -> Type {
    Type::Record {
        name: "Pod932".into(),
        fields: fields.into_iter().map(|(n, ty)| (n.into(), ty)).collect(),
    }
}

fn pod_resource(element: &Type) -> ResourceDefinition {
    ResourceDefinition {
        name: definition_name(element),
        payload: Type::OwnedStore {
            element: Box::new(element.clone()),
        },
        teardown: None,
    }
}

// A complete public raw candidate, without Analyze or a frontend dependency.
fn pod_fixture(element: Type) -> (TypedModule, CandidatePlan) {
    let span = Span::new(zil_base::FileId(0), 0, 1);
    let store = Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element.clone()),
    }));
    let mut m = module(vec![]);
    m.closures.clear();
    m.resources = vec![pod_resource(&element)];
    m.functions.push(TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        result_mode: PassingMode::Infer,
        foreign: None,
        environment: None,
        id: FunctionId(0),
        name: "main".into(),
        parameters: vec![],
        return_type: Type::I32,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "items".into(),
            ty: store.clone(),
            mutable: true,
            span,
        }],
        values: vec![store, Type::I32],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::OwnedStoreNew { element },
                    span,
                },
                Instruction {
                    result: None,
                    kind: InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(0),
                    },
                    span,
                },
                Instruction {
                    result: None,
                    kind: InstructionKind::DropManaged { local: LocalId(0) },
                    span,
                },
                Instruction {
                    result: Some(ValueId(1)),
                    kind: InstructionKind::Constant(Constant::I32(42)),
                    span,
                },
            ],
            terminator: Terminator::Return(Some(ValueId(1))),
        }],
        entry: BlockId(0),
        span,
    });
    let plan = pod_plan(&m, vec![vec![], vec![]]);
    (m, plan)
}

// Legal ordinary owning metadata uses the same explicit root lifetime as the
// existing Store push/pop fixture. The extra owner types below may be unused,
// but their function still has a real managed local and cleanup obligation.
fn scoped_pod_fixture(element: Type) -> (TypedModule, CandidatePlan) {
    let (mut module, plan) = pod_fixture(element);
    let function = &mut module.functions[0];
    let span = function.span;
    let op = |kind| Instruction {
        result: None,
        kind,
        span,
    };
    let original = std::mem::take(&mut function.blocks[0].instructions);
    let mut instructions = vec![
        op(InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        op(InstructionKind::StorageLive(LocalId(0))),
    ];
    for instruction in original {
        let dropped = matches!(instruction.kind, InstructionKind::DropManaged { .. });
        instructions.push(instruction);
        if dropped {
            instructions.push(op(InstructionKind::StorageDead(LocalId(0))));
        }
    }
    instructions.push(op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    }));
    function.blocks[0].instructions = instructions;
    (module, plan)
}

fn pod_plan(m: &TypedModule, roots: Vec<Vec<HoldRoot>>) -> CandidatePlan {
    let mut plan = CandidatePlan::empty_for(m);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        holds: roots
            .into_iter()
            .enumerate()
            .map(|(i, roots)| ValueHold {
                value: ValueId(i as u32),
                roots,
            })
            .collect(),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![],
        }],
    });
    plan
}

fn pod_both(
    m: &TypedModule,
    plan: &CandidatePlan,
    limits: &Limits,
) -> [CompileResult<VerifiedModule>; 2] {
    let mut plan = plan.clone();
    plan.module_fingerprint = m.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(m, &plan, &Limits::default()).unwrap();
    [
        verify_module(m.clone(), plan, &m.target, limits),
        verify_module_bytes(&bytes, &m.target, limits),
    ]
}

#[test]
fn pod932_complete_numeric_tree_and_actual_target_layout() {
    let ty = pod(vec![
        ("head", Type::U8),
        (
            "nested",
            Type::Tuple(vec![
                Type::Bool,
                Type::U16,
                Type::I32,
                Type::I64,
                Type::U64,
                Type::Usize,
                Type::F32,
                Type::F64,
                zero(),
                Type::Array {
                    element: Box::new(Type::U64),
                    length: 2,
                },
            ]),
        ),
    ]);
    let m = module(vec![]);
    let limits = Limits::default();
    let mut b = Budget::new(&limits);
    assert_eq!(
        pod_layout(&ty, &m.target, None, &mut b).unwrap(),
        (72, 8, true)
    );
    admit(&m, &ty, &limits).unwrap();
    let (m, plan) = pod_fixture(ty);
    for result in pod_both(&m, &plan, &limits) {
        result.unwrap();
    }
    let mut wrong = m.target.clone();
    wrong.pointer_width = 32;
    assert!(
        check_pod_element(
            &pod(vec![("n", Type::Usize)]),
            &wrong,
            None,
            &mut Budget::new(&limits)
        )
        .is_err()
    );
}

#[test]
fn pod932_forbidden_leaves_cannot_hide_in_zero_length_arrays() {
    let forbidden = vec![
        Type::Enum {
            name: "E".into(),
            variants: vec![("A".into(), vec![])],
        },
        Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(Type::I32),
        },
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Desc(Box::new(Type::ByteBuffer)),
        Type::Managed(Box::new(Type::ByteBuffer)),
        Type::ForeignHandle,
        Type::Closure(ClosureId(0)),
        Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(Type::Unit),
        },
        Type::ForeignCallback {
            parameters: vec![],
            result: Box::new(Type::Unit),
        },
        Type::OwnedStore {
            element: Box::new(pod(vec![("n", Type::I32)])),
        },
        Type::Package {
            name: "Owner".into(),
            fields: vec![],
        },
        Type::Slice {
            element: Box::new(Type::U8),
        },
        Type::ByteBuffer,
    ];
    let limits = Limits::default();
    for leaf in forbidden {
        let ty = pod(vec![
            ("n", Type::U64),
            (
                "hidden",
                Type::Array {
                    element: Box::new(Type::Tuple(vec![leaf])),
                    length: 0,
                },
            ),
        ]);
        let (m, plan) = pod_fixture(ty);
        for result in pod_both(&m, &plan, &limits) {
            assert!(result.is_err());
        }
    }
}

#[test]
fn pod932_unused_definitions_local_and_ssa_types_are_admitted_independently() {
    let limits = Limits::default();
    let valid = pod(vec![("n", Type::U64)]);
    for bad in [
        zero(),
        pod(vec![("same", Type::I32), ("same", Type::U8)]),
        pod(vec![("", Type::I32)]),
        Type::Record {
            name: "".into(),
            fields: vec![("n".into(), Type::U8)],
        },
        pod(vec![(
            "hidden",
            Type::Tuple(vec![Type::Managed(Box::new(Type::ByteBuffer))]),
        )]),
    ] {
        for position in 0..3 {
            let (mut m, mut plan) = pod_fixture(valid.clone());
            let carrier = Type::Managed(Box::new(Type::OwnedStore {
                element: Box::new(bad.clone()),
            }));
            let span = m.functions[0].span;
            match position {
                0 => m.resources.push(pod_resource(&bad)),
                1 => m.functions[0].locals.push(LocalDecl {
                    id: LocalId(1),
                    name: "unused".into(),
                    ty: carrier,
                    mutable: false,
                    span,
                }),
                _ => {
                    m.functions[0].values.push(carrier);
                    plan.functions[0].holds.push(ValueHold {
                        value: ValueId(2),
                        roots: vec![],
                    });
                }
            }
            for result in pod_both(&m, &plan, &limits) {
                assert!(result.is_err());
            }
        }
    }
    let huge = pod(vec![(
        "a",
        Type::Array {
            element: Box::new(Type::Array {
                element: Box::new(Type::U64),
                length: u32::MAX,
            }),
            length: u32::MAX,
        },
    )]);
    let error = admit(
        &module(vec![]),
        &huge,
        &Limits {
            max_analysis_steps: usize::MAX,
            ..limits
        },
    )
    .unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    assert!(
        error
            .message
            .contains("layout exceeds target address range")
    );
}

#[test]
fn pod932_canonical_name_and_complete_nominal_identity_are_both_required() {
    let limits = Limits::default();
    let ty = pod(vec![("a", Type::I64), ("b", Type::U8)]);
    for variant in 0..5 {
        let (mut m, plan) = pod_fixture(ty.clone());
        match variant {
            0 => m.resources[0].name.push('x'),
            1 => m.resources[0].teardown = Some(FunctionId(0)),
            2 => m.resources.push(m.resources[0].clone()),
            _ => {
                let Type::OwnedStore { element } = &mut m.resources[0].payload else {
                    unreachable!()
                };
                let Type::Record { fields, .. } = element.as_mut() else {
                    unreachable!()
                };
                if variant == 3 {
                    fields[0].1 = Type::U64;
                } else {
                    fields.reverse();
                }
            }
        }
        for result in pod_both(&m, &plan, &limits) {
            assert!(result.is_err(), "variant={variant}");
        }
    }
}

#[test]
fn pod932_new_metadata_and_push_result_protocol_cannot_forge_same_name() {
    let limits = Limits::default();
    let ty = pod(vec![("n", Type::I64)]);
    let mut m = module(vec![]);
    m.resources = vec![pod_resource(&ty)];
    let (mut fixture, _) = pod_fixture(ty.clone());
    let f = &mut fixture.functions[0];
    let desc = Type::Desc(Box::new(m.resources[0].payload.clone()));
    f.values = vec![
        desc,
        ty.clone(),
        Type::ErrorUnion {
            error: Box::new(ty.clone()),
            result: Box::new(Type::Unit),
        },
        Type::I64,
    ];
    let span = f.span;
    let push = Instruction {
        result: Some(ValueId(2)),
        kind: InstructionKind::OwnedStorePush {
            store: ValueId(0),
            value: ValueId(1),
        },
        span,
    };
    super::super::check_instruction(&m, f, &push, &mut Budget::new(&limits)).unwrap();
    f.values[1] = pod(vec![("n", Type::U64)]);
    assert!(super::super::check_instruction(&m, f, &push, &mut Budget::new(&limits)).is_err());
    f.values[1] = ty.clone();
    f.values[2] = Type::ErrorUnion {
        error: Box::new(pod(vec![("n", Type::U64)])),
        result: Box::new(Type::Unit),
    };
    assert!(super::super::check_instruction(&m, f, &push, &mut Budget::new(&limits)).is_err());
    f.values[2] = Type::Address {
        mutable: true,
        inner: Box::new(ty.clone()),
    };
    let mutable = Instruction {
        result: Some(ValueId(2)),
        kind: InstructionKind::OwnedStoreBorrowMut {
            store: ValueId(0),
            index: ValueId(3),
        },
        span,
    };
    assert_eq!(
        super::check_instruction(&m, f, &mutable, &mut Budget::new(&limits)).unwrap(),
        Some(Type::Address {
            mutable: true,
            inner: Box::new(ty.clone()),
        })
    );
    super::super::check_instruction(&m, f, &mutable, &mut Budget::new(&limits)).unwrap();
    f.locals[0].ty = ty.clone();
    let ordinary = Instruction {
        result: Some(ValueId(2)),
        kind: InstructionKind::BorrowLocalMut { local: LocalId(0) },
        span,
    };
    super::super::check_instruction(&m, f, &ordinary, &mut Budget::new(&limits)).unwrap();
    let (mut m, plan) = pod_fixture(ty);
    let InstructionKind::OwnedStoreNew { element } =
        &mut m.functions[0].blocks[0].instructions[0].kind
    else {
        unreachable!()
    };
    *element = pod(vec![("n", Type::U64)]);
    for result in pod_both(&m, &plan, &limits) {
        assert!(result.is_err());
    }
}

#[test]
fn pod932_paid_qualification_has_exact_and_fresh_work_controls() {
    let ty = pod(vec![
        ("n", Type::U64),
        (
            "tail",
            Type::Array {
                element: Box::new(Type::U8),
                length: 4080,
            },
        ),
    ]);
    let m = module(vec![]);
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    check_element_with_limits(&m, &ty, None, Some(&limits), &mut budget).unwrap();
    let n = limits.max_analysis_steps - budget.remaining;
    let exact = Limits {
        max_analysis_steps: n,
        ..limits.clone()
    };
    admit(&m, &ty, &exact).unwrap();
    assert_eq!(
        admit(
            &m,
            &ty,
            &Limits {
                max_analysis_steps: n - 1,
                ..limits
            }
        )
        .unwrap_err()
        .category,
        zil_base::ResultCategory::ResourceLimit
    );
    admit(&m, &ty, &exact).unwrap();
    println!("G9.3.2.3 independent POD qualification N={n}");
}

#[test]
fn pod932_logical_payload_and_stored_unit_stride_are_separate() {
    let limits = Limits::default();
    let m = module(vec![]);
    for ty in [
        pod(vec![("unit", Type::Unit)]),
        pod(vec![(
            "units",
            Type::Array {
                element: Box::new(Type::Unit),
                length: 4,
            },
        )]),
        pod(vec![(
            "empty",
            Type::Array {
                element: Box::new(Type::U64),
                length: 0,
            },
        )]),
    ] {
        assert!(admit(&m, &ty, &limits).is_err());
    }
    let mixed = pod(vec![
        ("n", Type::U8),
        (
            "units",
            Type::Array {
                element: Box::new(Type::Unit),
                length: 7,
            },
        ),
    ]);
    assert_eq!(
        pod_layout(&mixed, &m.target, None, &mut Budget::new(&limits)).unwrap(),
        (1, 1, true)
    );
    admit(&m, &mixed, &limits).unwrap();
    let unit_array = pod(vec![
        ("n", Type::U8),
        (
            "units",
            Type::Array {
                element: Box::new(Type::Array {
                    element: Box::new(Type::Unit),
                    length: u32::MAX,
                }),
                length: u32::MAX,
            },
        ),
    ]);
    admit(&m, &unit_array, &limits).unwrap();
}

fn pod_window_fixture(distinct: bool, read_first: bool) -> (TypedModule, CandidatePlan) {
    plain_window_fixture(pod(vec![("n", Type::I64)]), distinct, read_first)
}

fn plain_window_fixture(
    ty: Type,
    distinct: bool,
    read_first: bool,
) -> (TypedModule, CandidatePlan) {
    let (mut m, _) = pod_fixture(ty.clone());
    let payload = m.resources[0].payload.clone();
    let f = &mut m.functions[0];
    let span = f.span;
    let op = |result: Option<u32>, kind| Instruction {
        result: result.map(ValueId),
        kind,
        span,
    };
    f.values.extend([
        Type::Desc(Box::new(payload.clone())),
        Type::I64,
        Type::Address {
            mutable: false,
            inner: Box::new(ty.clone()),
        },
        Type::Bool,
        ty,
    ]);
    let original = f.blocks[0].instructions.clone();
    let mut instructions = original[..2].to_vec();
    let (access_desc, mut holds) = if distinct {
        f.locals.push(LocalDecl {
            id: LocalId(1),
            name: "other".into(),
            ty: f.locals[0].ty.clone(),
            mutable: true,
            span,
        });
        f.values
            .extend([f.locals[0].ty.clone(), Type::Desc(Box::new(payload))]);
        instructions.extend([
            op(Some(7), original[0].kind.clone()),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(7),
                },
            ),
            op(
                Some(8),
                InstructionKind::DescribeLocal { local: LocalId(1) },
            ),
        ]);
        (
            8,
            vec![
                vec![],
                vec![],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
                vec![],
                vec![],
                vec![HoldRoot::Local(LocalId(1))],
            ],
        )
    } else {
        (
            2,
            vec![
                vec![],
                vec![],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
                vec![],
            ],
        )
    };
    instructions.extend([
        op(
            Some(2),
            InstructionKind::DescribeLocal { local: LocalId(0) },
        ),
        op(Some(3), InstructionKind::Constant(Constant::I64(0))),
        op(
            Some(4),
            InstructionKind::OwnedStoreBorrow {
                store: ValueId(2),
                index: ValueId(3),
            },
        ),
    ]);
    let read = op(
        Some(6),
        InstructionKind::ReadAddress {
            address: ValueId(4),
        },
    );
    let reserve = op(
        Some(5),
        InstructionKind::OwnedStoreReserve {
            store: ValueId(access_desc),
            capacity: ValueId(3),
        },
    );
    if read_first {
        instructions.extend([read, reserve]);
    } else {
        instructions.extend([reserve, read]);
    }
    if distinct {
        instructions.push(op(None, InstructionKind::DropManaged { local: LocalId(1) }));
    }
    instructions.extend_from_slice(&original[2..]);
    f.blocks[0].instructions = instructions;
    let plan = pod_plan(&m, std::mem::take(&mut holds));
    (m, plan)
}

fn pod_mut_window_fixture(distinct: bool, read_first: bool) -> (TypedModule, CandidatePlan) {
    mutable_window_fixture(pod_window_fixture(distinct, read_first))
}

fn mutable_window_fixture(
    (mut module, plan): (TypedModule, CandidatePlan),
) -> (TypedModule, CandidatePlan) {
    let function = &mut module.functions[0];
    let Type::Address { mutable, .. } = &mut function.values[4] else {
        unreachable!("shared Store window fixture has an address result")
    };
    *mutable = true;
    let borrow = function.blocks[0]
        .instructions
        .iter_mut()
        .find(|instruction| matches!(instruction.kind, InstructionKind::OwnedStoreBorrow { .. }))
        .expect("shared Store window fixture borrows one element");
    let InstructionKind::OwnedStoreBorrow { store, index } = borrow.kind else {
        unreachable!()
    };
    borrow.kind = InstructionKind::OwnedStoreBorrowMut { store, index };
    (module, plan)
}

#[test]
fn pod933_mutable_window_keeps_actual_whole_store_root_until_last_use() {
    let limits = Limits::default();
    for (distinct, read_first, accept) in [
        (false, false, false),
        (false, true, true),
        (true, false, true),
    ] {
        let (module, plan) = pod_mut_window_fixture(distinct, read_first);
        for result in pod_both(&module, &plan, &limits) {
            if accept {
                result.unwrap();
            } else {
                assert_eq!(result.unwrap_err()[0].code, "E-BORROW-CONFLICT");
            }
        }
    }
    let (module, mut plan) = pod_mut_window_fixture(true, false);
    plan.functions[0].holds[4].roots = vec![HoldRoot::Local(LocalId(1))];
    for result in pod_both(&module, &plan, &limits) {
        assert_eq!(result.unwrap_err()[0].code, "E-VERIFY-HOLD");
    }
}

#[test]
fn pod933_mutable_window_requires_exact_complete_result_even_when_unused() {
    let limits = Limits::default();
    let (module, plan) = pod_mut_window_fixture(false, true);
    for wrong in [
        Type::Address {
            mutable: false,
            inner: Box::new(pod(vec![("n", Type::I64)])),
        },
        Type::Address {
            mutable: true,
            inner: Box::new(pod(vec![("n", Type::U64)])),
        },
    ] {
        let mut malformed = module.clone();
        malformed.functions[0].values[4] = wrong;
        for result in pod_both(&malformed, &plan, &limits) {
            assert!(result.is_err());
        }
    }
    let mut malformed = module.clone();
    let function = &mut malformed.functions[0];
    let extra = ValueId(function.values.len() as u32);
    function.values.push(Type::I32);
    let span = function.span;
    function.blocks[0].instructions.insert(
        5,
        Instruction {
            result: Some(extra),
            kind: InstructionKind::OwnedStoreBorrowMut {
                store: ValueId(2),
                index: ValueId(3),
            },
            span,
        },
    );
    let mut plan = plan;
    plan.functions[0].holds.push(ValueHold {
        value: extra,
        roots: vec![],
    });
    for result in pod_both(&malformed, &plan, &limits) {
        assert!(result.is_err());
    }
}

#[test]
fn pod933_mutable_window_conflicts_with_every_same_store_index_and_view_mode() {
    let limits = Limits::default();
    for second_mutable in [false, true] {
        for read_first in [false, true] {
            let (mut module, mut plan) = pod_mut_window_fixture(false, read_first);
            let function = &mut module.functions[0];
            let Type::Address { inner, .. } = &function.values[4] else {
                unreachable!()
            };
            function.values[5] = Type::Address {
                mutable: second_mutable,
                inner: inner.clone(),
            };
            let other_index = ValueId(function.values.len() as u32);
            function.values.push(Type::I64);
            let instructions = &mut function.blocks[0].instructions;
            let second = instructions
                .iter()
                .position(|instruction| {
                    matches!(instruction.kind, InstructionKind::OwnedStoreReserve { .. })
                })
                .expect("fixture reserves the Store");
            instructions.insert(
                second,
                Instruction {
                    result: Some(other_index),
                    kind: InstructionKind::Constant(Constant::I64(1)),
                    span: function.span,
                },
            );
            instructions[second + 1].kind = if second_mutable {
                InstructionKind::OwnedStoreBorrowMut {
                    store: ValueId(2),
                    index: other_index,
                }
            } else {
                InstructionKind::OwnedStoreBorrow {
                    store: ValueId(2),
                    index: other_index,
                }
            };
            plan.functions[0].holds[5].roots = vec![HoldRoot::Local(LocalId(0))];
            plan.functions[0].holds.push(ValueHold {
                value: other_index,
                roots: vec![],
            });
            for result in pod_both(&module, &plan, &limits) {
                if read_first {
                    result.unwrap();
                } else {
                    assert_eq!(result.unwrap_err()[0].code, "E-BORROW-CONFLICT");
                }
            }
        }
    }
}

#[test]
fn pod933_mutable_raw_and_wire_requests_have_exact_fresh_work_boundaries() {
    let (module, plan) = pod_mut_window_fixture(false, true);
    let limits = Limits::default();
    for wire in [false, true] {
        let check = |n| {
            let selected = Limits {
                max_analysis_steps: n,
                ..limits.clone()
            };
            if wire {
                let mut plan = plan.clone();
                plan.module_fingerprint = module.fingerprint();
                let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
                verify_module_bytes(&bytes, &module.target, &selected)
            } else {
                let mut plan = plan.clone();
                plan.module_fingerprint = module.fingerprint();
                verify_module(module.clone(), plan, &module.target, &selected)
            }
        };
        let (mut lo, mut hi) = (0, limits.max_analysis_steps);
        check(hi).unwrap();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if check(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        check(lo).unwrap();
        assert_eq!(
            check(lo - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        check(lo).unwrap();
        println!("G9.3.3.3 mutable request wire={wire} N={lo}");
    }
}

#[test]
fn scalar934_nine_direct_elements_and_canonical_carriers_are_independent() {
    let limits = Limits::default();
    for scalar in [
        Type::Bool,
        Type::U8,
        Type::U16,
        Type::I32,
        Type::I64,
        Type::U64,
        Type::Usize,
        Type::F32,
        Type::F64,
    ] {
        assert!(direct_scalar(&scalar));
        let (module, plan) = pod_fixture(scalar.clone());
        assert_eq!(
            module.resources[0].name,
            format!("@builtin:OwnedStore<{}>", scalar.name())
        );
        for result in pod_both(&module, &plan, &limits) {
            result.unwrap();
        }
        let (module, plan) = mutable_window_fixture(plain_window_fixture(scalar, false, true));
        for result in pod_both(&module, &plan, &limits) {
            result.unwrap();
        }
    }
    for forbidden in [
        Type::Unit,
        Type::Tuple(vec![Type::I32]),
        Type::Array {
            element: Box::new(Type::I32),
            length: 1,
        },
        Type::ForeignHandle,
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
    ] {
        assert!(!direct_scalar(&forbidden));
        assert!(
            check_pod_element(
                &forbidden,
                &TargetProfile::host_llvm(),
                None,
                &mut Budget::new(&limits)
            )
            .is_err()
        );
    }
    let (mut module, plan) = pod_fixture(Type::U64);
    module.resources[0].name = definition_name(&Type::Usize);
    for result in pod_both(&module, &plan, &limits) {
        assert!(result.is_err());
    }
}

#[test]
fn scalar934_raw_wire_checks_complete_mutable_result_and_unused_producer() {
    let limits = Limits::default();
    let (module, plan) = mutable_window_fixture(plain_window_fixture(Type::I32, false, true));
    for wrong in [
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Address {
            mutable: true,
            inner: Box::new(Type::I64),
        },
    ] {
        let mut malformed = module.clone();
        malformed.functions[0].values[4] = wrong;
        for result in pod_both(&malformed, &plan, &limits) {
            assert!(result.is_err());
        }
    }
    let mut malformed = module.clone();
    let function = &mut malformed.functions[0];
    let extra = ValueId(function.values.len() as u32);
    function.values.push(Type::I32);
    function.blocks[0].instructions.insert(
        5,
        Instruction {
            result: Some(extra),
            kind: InstructionKind::OwnedStoreBorrowMut {
                store: ValueId(2),
                index: ValueId(3),
            },
            span: function.span,
        },
    );
    let mut plan = plan;
    plan.functions[0].holds.push(ValueHold {
        value: extra,
        roots: vec![],
    });
    for result in pod_both(&malformed, &plan, &limits) {
        assert!(result.is_err());
    }
}

#[test]
fn scalar934_actual_whole_store_window_rejects_same_root_and_forged_hold() {
    let limits = Limits::default();
    for (distinct, read_first, accept) in [
        (false, false, false),
        (false, true, true),
        (true, false, true),
    ] {
        let (module, plan) =
            mutable_window_fixture(plain_window_fixture(Type::I32, distinct, read_first));
        for result in pod_both(&module, &plan, &limits) {
            if accept {
                result.unwrap();
            } else {
                assert_eq!(result.unwrap_err()[0].code, "E-BORROW-CONFLICT");
            }
        }
    }
    let (module, mut plan) = mutable_window_fixture(plain_window_fixture(Type::I32, true, false));
    plan.functions[0].holds[4].roots = vec![HoldRoot::Local(LocalId(1))];
    for result in pod_both(&module, &plan, &limits) {
        assert_eq!(result.unwrap_err()[0].code, "E-VERIFY-HOLD");
    }
    for second_mutable in [false, true] {
        let (mut module, mut plan) =
            mutable_window_fixture(plain_window_fixture(Type::I32, false, false));
        let function = &mut module.functions[0];
        function.values[5] = Type::Address {
            mutable: second_mutable,
            inner: Box::new(Type::I32),
        };
        let index = ValueId(function.values.len() as u32);
        function.values.push(Type::I64);
        let instructions = &mut function.blocks[0].instructions;
        let second = instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.kind, InstructionKind::OwnedStoreReserve { .. })
            })
            .unwrap();
        instructions.insert(
            second,
            Instruction {
                result: Some(index),
                kind: InstructionKind::Constant(Constant::I64(1)),
                span: function.span,
            },
        );
        instructions[second + 1].kind = if second_mutable {
            InstructionKind::OwnedStoreBorrowMut {
                store: ValueId(2),
                index,
            }
        } else {
            InstructionKind::OwnedStoreBorrow {
                store: ValueId(2),
                index,
            }
        };
        plan.functions[0].holds[5].roots = vec![HoldRoot::Local(LocalId(0))];
        plan.functions[0].holds.push(ValueHold {
            value: index,
            roots: vec![],
        });
        for result in pod_both(&module, &plan, &limits) {
            assert_eq!(result.unwrap_err()[0].code, "E-BORROW-CONFLICT");
        }
    }
}

#[test]
fn scalar934_raw_wire_exact_fresh_request_budget() {
    let (module, plan) = mutable_window_fixture(plain_window_fixture(Type::I32, false, true));
    let limits = Limits::default();
    for wire in [false, true] {
        let check = |n| {
            let selected = Limits {
                max_analysis_steps: n,
                ..limits.clone()
            };
            let mut plan = plan.clone();
            plan.module_fingerprint = module.fingerprint();
            if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
                verify_module_bytes(&bytes, &module.target, &selected)
            } else {
                verify_module(module.clone(), plan, &module.target, &selected)
            }
        };
        let (mut lo, mut hi) = (0, limits.max_analysis_steps);
        check(hi).unwrap();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if check(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        check(lo).unwrap();
        assert_eq!(
            check(lo - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        check(lo).unwrap();
        println!("G9.3.4.3 scalar request wire={wire} N={lo}");
    }
}

#[test]
fn store935_unused_ordinary_owners_qualify_but_record_and_array_do_not() {
    let limits = Limits::default();
    let (module, _) = pod_fixture(Type::I32);
    let store = module.functions[0].locals[0].ty.clone();
    for owner in [
        Type::Package {
            name: "ScalarBag934".into(),
            fields: vec![("items".into(), store.clone())],
        },
        Type::Tuple(vec![store.clone()]),
        Type::Enum {
            name: "ScalarEnum934".into(),
            variants: vec![("Own".into(), vec![store.clone()])],
        },
        Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(store.clone()),
        },
        Type::ErrorUnion {
            error: Box::new(store.clone()),
            result: Box::new(Type::Unit),
        },
        Type::ErrorUnion {
            error: Box::new(store.clone()),
            result: Box::new(store.clone()),
        },
    ] {
        let (mut malformed, plan) = scoped_pod_fixture(Type::I32);
        let span = malformed.functions[0].span;
        malformed.functions[0].locals.push(LocalDecl {
            id: LocalId(1),
            name: "unused_owner".into(),
            ty: owner,
            mutable: false,
            span,
        });
        for result in pod_both(&malformed, &plan, &limits) {
            result.unwrap();
        }
    }
    for owner in [
        Type::Record {
            name: "ScalarRecord934".into(),
            fields: vec![("items".into(), store.clone())],
        },
        Type::Array {
            element: Box::new(store),
            length: 1,
        },
    ] {
        let (mut malformed, plan) = pod_fixture(Type::I32);
        let span = malformed.functions[0].span;
        malformed.functions[0].locals.push(LocalDecl {
            id: LocalId(1),
            name: "unused_owner".into(),
            ty: owner,
            mutable: false,
            span,
        });
        for result in pod_both(&malformed, &plan, &limits) {
            assert_eq!(result.unwrap_err()[0].code, "E-UNSUPPORTED");
        }
    }
}

#[test]
fn store935_capture_requires_the_actual_entry_even_after_carrier_admission() {
    let limits = Limits::default();
    let (mut module, plan) = pod_fixture(Type::I32);
    let store = module.functions[0].locals[0].ty.clone();
    check_owned_capture(&module, &store, Some(&limits), &mut Budget::new(&limits)).unwrap();
    module.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(0),
        function: FunctionId(0),
        kind: CallableKind::FnOnce,
        captures: vec![store],
        parameters: vec![],
        result: Type::I32,
        lending_captures: vec![],
    });
    for result in pod_both(&module, &plan, &limits) {
        let errors = result.unwrap_err();
        assert_eq!(errors[0].code, "E-VERIFY-CLOSURE-ENTRY", "{errors:?}");
    }
}

#[test]
fn store935_nested_product_capture_checks_every_actual_store_definition() {
    let limits = Limits::default();
    let (mut module, _) = pod_fixture(Type::I32);
    let store = module.functions[0].locals[0].ty.clone();
    let outer = Type::Package {
        name: "Outer935".into(),
        fields: vec![(
            "inner".into(),
            Type::Package {
                name: "Inner935".into(),
                fields: vec![("items".into(), store)],
            },
        )],
    };
    assert!(package_inline_store(&outer, &mut Budget::new(&limits)).unwrap());
    let mut budget = Budget::new(&limits);
    check_type_shape(&outer, &module.target, &limits, &mut budget).unwrap();
    check_owned_capture(&module, &outer, Some(&limits), &mut budget).unwrap();
    module.resources[0].payload = Type::OwnedStore {
        element: Box::new(Type::F32),
    };
    assert!(check_owned_capture(&module, &outer, Some(&limits), &mut budget).is_err());
}

#[test]
fn store935_product_capture_rechecks_store_behind_each_owned_sum_constructor() {
    let limits = Limits::default();
    let (module, _) = pod_fixture(Type::I32);
    let store = module.functions[0].locals[0].ty.clone();
    for child in [
        Type::Tuple(vec![store.clone()]),
        Type::Enum {
            name: "Choice935".into(),
            variants: vec![("Own".into(), vec![store.clone()])],
        },
        Type::ErrorUnion {
            error: Box::new(store.clone()),
            result: Box::new(Type::Unit),
        },
    ] {
        let parent = Type::Package {
            name: "Capture935".into(),
            fields: vec![("child".into(), child)],
        };
        let mut budget = Budget::new(&limits);
        check_type_shape(&parent, &module.target, &limits, &mut budget).unwrap();
        assert!(owned_capture_store(&parent, &mut budget).unwrap());
        check_owned_capture(&module, &parent, Some(&limits), &mut budget).unwrap();
    }
}

#[test]
fn store935_mutable_product_parent_address_qualifies_without_shared_address_permission() {
    let limits = Limits::default();
    let (module, _) = pod_fixture(Type::I32);
    let store = module.functions[0].locals[0].ty.clone();
    for parent in [
        Type::Package {
            name: "Direct935".into(),
            fields: vec![("items".into(), store.clone())],
        },
        Type::Package {
            name: "Nested935".into(),
            fields: vec![(
                "part".into(),
                Type::Package {
                    name: "Part935".into(),
                    fields: vec![("items".into(), store.clone())],
                },
            )],
        },
    ] {
        for mutable in [true, false] {
            let (mut candidate, plan) = scoped_pod_fixture(Type::I32);
            let span = candidate.functions[0].span;
            candidate.functions[0].locals.push(LocalDecl {
                id: LocalId(1),
                name: "unused_address".into(),
                ty: Type::Address {
                    mutable,
                    inner: Box::new(parent.clone()),
                },
                mutable: false,
                span,
            });
            for result in pod_both(&candidate, &plan, &limits) {
                if mutable {
                    result.unwrap();
                } else {
                    assert_eq!(result.unwrap_err()[0].code, "E-UNSUPPORTED");
                }
            }
        }
    }
}

#[test]
fn store935_raw_wire_unused_owner_has_exact_fresh_budget_and_same_size_forgery_rejects() {
    let (mut module, plan) = scoped_pod_fixture(Type::I32);
    let store = module.functions[0].locals[0].ty.clone();
    let span = module.functions[0].span;
    module.functions[0].locals.push(LocalDecl {
        id: LocalId(1),
        name: "unused_parent".into(),
        ty: Type::Package {
            name: "Model935".into(),
            fields: vec![("values".into(), store)],
        },
        mutable: false,
        span,
    });
    let limits = Limits::default();
    for wire in [false, true] {
        let check = |steps| {
            let mut selected = limits.clone();
            selected.max_analysis_steps = steps;
            if wire {
                let mut candidate = plan.clone();
                candidate.module_fingerprint = module.fingerprint();
                let bytes =
                    zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
                verify_module_bytes(&bytes, &module.target, &selected)
            } else {
                let mut candidate = plan.clone();
                candidate.module_fingerprint = module.fingerprint();
                verify_module(module.clone(), candidate, &module.target, &selected)
            }
        };
        let (mut lo, mut hi) = (0, limits.max_analysis_steps);
        check(hi).unwrap();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if check(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        check(lo).unwrap();
        assert_eq!(
            check(lo - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        check(lo).unwrap();
    }
    module.resources[0].payload = Type::OwnedStore {
        element: Box::new(Type::F32),
    };
    for result in pod_both(&module, &plan, &limits) {
        assert!(result.is_err());
    }
}

#[test]
fn pod932_actual_shared_window_blocks_mutation_and_tracks_last_use_and_roots() {
    let limits = Limits::default();
    for (distinct, read_first, accept) in [
        (false, false, false),
        (false, true, true),
        (true, false, true),
    ] {
        let (m, plan) = pod_window_fixture(distinct, read_first);
        for result in pod_both(&m, &plan, &limits) {
            if accept {
                result.unwrap();
            } else {
                let errors = result.unwrap_err();
                assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
            }
        }
    }
    let (mut m, plan) = pod_window_fixture(false, false);
    let instructions = &mut m.functions[0].blocks[0].instructions;
    let reserve = instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::OwnedStoreReserve { .. }))
        .unwrap();
    reserve.result = None;
    reserve.kind = InstructionKind::DropManaged { local: LocalId(0) };
    for result in pod_both(&m, &plan, &limits) {
        assert!(result.is_err());
    }
    // Candidate claims cannot reassign an actual window to a separate Store.
    let (m, mut plan) = pod_window_fixture(true, false);
    plan.functions[0].holds[4].roots = vec![HoldRoot::Local(LocalId(1))];
    for result in pod_both(&m, &plan, &limits) {
        assert_eq!(result.unwrap_err()[0].code, "E-VERIFY-HOLD");
    }
}

#[test]
fn pod932_complete_raw_and_wire_request_has_exact_fresh_boundaries() {
    let (m, plan) = pod_window_fixture(false, true);
    let limits = Limits::default();
    for wire in [false, true] {
        let check = |n| {
            let selected = Limits {
                max_analysis_steps: n,
                ..limits.clone()
            };
            if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&m, &plan, &limits).unwrap();
                verify_module_bytes(&bytes, &m.target, &selected)
            } else {
                verify_module(m.clone(), plan.clone(), &m.target, &selected)
            }
        };
        let (mut lo, mut hi) = (0, limits.max_analysis_steps);
        check(hi).unwrap();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if check(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        check(lo).unwrap();
        assert_eq!(
            check(lo - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        check(lo).unwrap();
        println!("G9.3.2.3 complete request wire={wire} N={lo}");
    }
}

fn pod_protocol_fixture() -> (TypedModule, CandidatePlan) {
    let element = pod(vec![("n", Type::I64)]);
    let (mut m, _) = pod_fixture(element.clone());
    let payload = m.resources[0].payload.clone();
    let f = &mut m.functions[0];
    let span = f.span;
    let push = Type::ErrorUnion {
        error: Box::new(element.clone()),
        result: Box::new(Type::Unit),
    };
    let pop = Type::ErrorUnion {
        error: Box::new(Type::Unit),
        result: Box::new(element.clone()),
    };
    assert!(push.is_copy_value() && pop.is_copy_value());
    f.locals.push(LocalDecl {
        id: LocalId(1),
        name: "protocol".into(),
        ty: push.clone(),
        mutable: false,
        span,
    });
    f.values.extend([
        Type::Desc(Box::new(payload)),
        Type::I64,
        element,
        push.clone(),
        pop,
        push.clone(),
        push,
    ]);
    let op = |result: Option<u32>, kind| Instruction {
        result: result.map(ValueId),
        kind,
        span,
    };
    let original = f.blocks[0].instructions.clone();
    let mut instructions = vec![
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        op(None, InstructionKind::StorageLive(LocalId(0))),
        op(None, InstructionKind::StorageLive(LocalId(1))),
    ];
    instructions.extend_from_slice(&original[..2]);
    instructions.extend([
        op(
            Some(2),
            InstructionKind::DescribeLocal { local: LocalId(0) },
        ),
        op(Some(3), InstructionKind::Constant(Constant::I64(42))),
        op(
            Some(4),
            InstructionKind::MakeAggregate {
                fields: vec![ValueId(3)],
            },
        ),
        op(
            Some(5),
            InstructionKind::OwnedStorePush {
                store: ValueId(2),
                value: ValueId(4),
            },
        ),
        // A POD input is Copy: a second push of the same SSA value creates no
        // element debt. The unique head remains managed and is freed once.
        op(
            Some(8),
            InstructionKind::OwnedStorePush {
                store: ValueId(2),
                value: ValueId(4),
            },
        ),
        op(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(5),
            },
        ),
        op(Some(7), InstructionKind::Load(LocalId(1))),
        op(
            Some(6),
            InstructionKind::OwnedStorePop { store: ValueId(2) },
        ),
    ]);
    instructions.extend_from_slice(&original[2..]);
    instructions.extend([
        op(None, InstructionKind::StorageDead(LocalId(1))),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]);
    f.blocks[0].instructions = instructions;
    let mut holds = vec![vec![]; f.values.len()];
    holds[2] = vec![HoldRoot::Local(LocalId(0))];
    let plan = pod_plan(&m, holds);
    (m, plan)
}

#[test]
fn pod932_complete_copy_push_pop_protocols_and_no_element_debt_are_raw_and_wire_checked() {
    let limits = Limits::default();
    let (m, plan) = pod_protocol_fixture();
    for result in pod_both(&m, &plan, &limits) {
        result.unwrap();
    }
    for variant in 0..3 {
        let mut bad = m.clone();
        match variant {
            0 => {
                bad.functions[0].values[5] = Type::ErrorUnion {
                    error: Box::new(pod(vec![("n", Type::U64)])),
                    result: Box::new(Type::Unit),
                }
            }
            1 => {
                bad.functions[0].values[6] = Type::ErrorUnion {
                    error: Box::new(Type::Unit),
                    result: Box::new(pod(vec![("n", Type::U64)])),
                }
            }
            _ => {
                let drop = bad.functions[0].blocks[0]
                    .instructions
                    .iter()
                    .find(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                    .unwrap()
                    .clone();
                bad.functions[0].blocks[0].instructions.insert(10, drop);
            }
        }
        for result in pod_both(&bad, &plan, &limits) {
            assert!(result.is_err(), "variant={variant}");
        }
    }
}

#[test]
fn store935_unused_package_and_recursive_product_gain_qualified_pod_store_ownership() {
    let limits = Limits::default();
    for nested in [false, true] {
        let (mut m, plan) = scoped_pod_fixture(pod(vec![("n", Type::I64)]));
        let leaf = Type::Package {
            name: "Bag932".into(),
            fields: vec![("items".into(), m.functions[0].locals[0].ty.clone())],
        };
        let owner = if nested {
            Type::Package {
                name: "Outer932".into(),
                fields: vec![("child".into(), leaf)],
            }
        } else {
            leaf
        };
        packages::check_ordinary_type(&m, &owner, None, &mut Budget::new(&limits)).unwrap();
        let span = m.functions[0].span;
        m.functions[0].locals.push(LocalDecl {
            id: LocalId(1),
            name: "unused_bag".into(),
            ty: owner,
            mutable: false,
            span,
        });
        for result in pod_both(&m, &plan, &limits) {
            result.unwrap();
        }
    }
    // The previous complete owning Package Store field remains admitted.
    let (old_store, resource) = captured_leaf_store();
    let mut m = module(vec![]);
    m.resources.push(resource);
    for nested in [false, true] {
        let leaf = Type::Package {
            name: "OldBag932".into(),
            fields: vec![("items".into(), old_store.clone())],
        };
        let owner = if nested {
            Type::Package {
                name: "OldOuter932".into(),
                fields: vec![("child".into(), leaf)],
            }
        } else {
            leaf
        };
        packages::check_ordinary_type(&m, &owner, None, &mut Budget::new(&limits)).unwrap();
    }
    let (m, _) = pod_fixture(pod(vec![("n", Type::I64)]));
    check_owned_capture(
        &m,
        &m.functions[0].locals[0].ty,
        Some(&limits),
        &mut Budget::new(&limits),
    )
    .unwrap();
}

#[test]
fn store935_new_head_family_enters_qualified_owning_sum_containers() {
    let limits = Limits::default();
    let (old_store, old_resource) = captured_leaf_store();
    let wrap = |store: Type| {
        [
            Type::Tuple(vec![store.clone()]),
            Type::Enum {
                name: "HeadEnum932".into(),
                variants: vec![("Own".into(), vec![store.clone()])],
            },
            Type::ErrorUnion {
                error: Box::new(Type::Unit),
                result: Box::new(store),
            },
        ]
    };
    let mut old_module = module(vec![]);
    old_module.resources.push(old_resource);
    for old_container in wrap(old_store) {
        sums::check_type_for(&old_module, &old_container, None, &mut Budget::new(&limits)).unwrap();
    }
    let (m, _) = pod_fixture(pod(vec![("n", Type::I64)]));
    let store = m.functions[0].locals[0].ty.clone();
    resources::check_type_for(&m, &store, None, &mut Budget::new(&limits)).unwrap();
    sums::check_type_for(&m, &store, None, &mut Budget::new(&limits)).unwrap();
    for container in wrap(store) {
        sums::check_type_for(&m, &container, None, &mut Budget::new(&limits)).unwrap();
        let (mut candidate, plan) = scoped_pod_fixture(pod(vec![("n", Type::I64)]));
        let span = candidate.functions[0].span;
        candidate.functions[0].locals.push(LocalDecl {
            id: LocalId(1),
            name: "unused_sum".into(),
            ty: container,
            mutable: false,
            span,
        });
        for result in pod_both(&candidate, &plan, &limits) {
            result.unwrap();
        }
    }
}
