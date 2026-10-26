//! Hand-constructed IR tests exercise the trust boundary. They are not source,
//! compiler-pipeline, native-execution, or language-conformance evidence.

use super::*;
use zil_base::{FileId, ModuleId, ResultCategory};

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn inst(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(),
    }
}
fn baseline() -> (TypedModule, CandidatePlan) {
    let function = TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        result_mode: zil_ir::PassingMode::Infer,
        foreign: None,
        environment: None,
        id: FunctionId(0),
        name: "main".into(),
        parameters: vec![],
        return_type: Type::I32,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "answer".into(),
            ty: Type::I32,
            mutable: false,
            span: span(),
        }],
        values: vec![Type::I32, Type::I32, Type::I32, Type::I32],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                inst(Some(0), InstructionKind::Constant(Constant::I32(40))),
                inst(Some(1), InstructionKind::Constant(Constant::I32(2))),
                inst(
                    Some(2),
                    InstructionKind::Binary {
                        op: BinaryOp::Add,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                ),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(2),
                    },
                ),
                inst(Some(3), InstructionKind::Load(LocalId(0))),
            ],
            terminator: Terminator::Return(Some(ValueId(3))),
        }],
        entry: BlockId(0),
        span: span(),
    };
    let module = TypedModule {
        resources: Vec::new(),
        closures: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"synthetic verifier test; not source evidence"),
        target: TargetProfile::host_llvm(),
        functions: vec![function],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        holds: (0..module.functions[0].values.len())
            .map(|i| zil_ir::ValueHold {
                value: ValueId(i as u32),
                roots: vec![],
            })
            .collect(),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![LocalId(0)],
        }],
    });
    (module, plan)
}
fn reject_code(module: TypedModule, mut plan: CandidatePlan) -> String {
    // Rebind the identity so mutations reach the independent structural checks.
    plan.module_fingerprint = module.fingerprint();
    verify_module(
        module,
        plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap_err()[0]
        .code
        .clone()
}

#[test]
fn accepts_checked_arithmetic_initialization_and_readonly_snapshot() {
    let (module, plan) = baseline();
    let expected = module.fingerprint();
    let verified = verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(verified.module(), &module);
    assert_eq!(verified.plan(), &plan);
    assert_eq!(verified.fingerprint(), expected);
}
#[test]
fn rejects_false_initialization_claim() {
    let (module, mut plan) = baseline();
    plan.functions[0].blocks[0].initialized_in.push(LocalId(0));
    assert_eq!(reject_code(module, plan), "E-VERIFY-INITIALIZATION-CLAIM");
}
#[test]
fn rejects_uninitialized_read_even_with_matching_boundary_sets() {
    let (mut module, plan) = baseline();
    module.functions[0].blocks[0].instructions.swap(3, 4);
    assert_eq!(reject_code(module, plan), "E-VERIFY-INITIALIZATION");
}
#[test]
fn rejects_wrong_return_type() {
    let (mut module, plan) = baseline();
    module.functions[0].return_type = Type::Bool;
    assert_eq!(reject_code(module, plan), "E-VERIFY-RETURN");
}
#[test]
fn rejects_operand_before_its_definition() {
    let (mut module, plan) = baseline();
    module.functions[0].blocks[0].instructions.swap(0, 2);
    assert_eq!(reject_code(module, plan), "E-VERIFY-DOMINANCE");
}
#[test]
fn rejects_duplicate_and_missing_value_definitions() {
    let (mut module, plan) = baseline();
    module.functions[0].blocks[0].instructions[1].result = Some(ValueId(0));
    assert_eq!(reject_code(module, plan), "E-VERIFY-SSA");
    let (mut module, plan) = baseline();
    module.functions[0].values.push(Type::I32);
    assert_eq!(reject_code(module, plan), "E-VERIFY-SSA");
}
#[test]
fn rejects_forged_target_and_stale_plan() {
    let (mut module, mut plan) = baseline();
    module.target.triple = "fake-target".into();
    plan.module_fingerprint = module.fingerprint();
    assert_eq!(reject_code(module, plan), "E-VERIFY-TARGET");
    let (module, mut plan) = baseline();
    plan.module_fingerprint = Fingerprint::ZERO;
    assert_eq!(
        verify_module(
            module,
            plan,
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .unwrap_err()[0]
            .code,
        "E-VERIFY-PLAN-IDENTITY"
    );
}
#[test]
fn rejects_unchecked_arithmetic_profile() {
    let (mut module, mut plan) = baseline();
    module.target.checked_arithmetic = false;
    plan.module_fingerprint = module.fingerprint();
    plan.target_fingerprint = module.target.fingerprint();
    let target = module.target.clone();
    assert_eq!(
        verify_module(module, plan, &target, &Limits::default()).unwrap_err()[0].code,
        "E-VERIFY-ARITHMETIC"
    );
}
#[test]
fn rejects_immutable_reassignment() {
    let (mut module, plan) = baseline();
    module.functions[0].blocks[0].instructions.push(inst(
        None,
        InstructionKind::Store {
            local: LocalId(0),
            value: ValueId(0),
        },
    ));
    assert_eq!(reject_code(module, plan), "E-VERIFY-MUTABILITY");
}
#[test]
fn unknown_semantics_are_unsupported() {
    let (mut module, plan) = baseline();
    module.functions[0].locals[0].ty = Type::Desc(Box::new(Type::Named("Resource".into())));
    let diagnostics = verify_module(
        module,
        plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(diagnostics[0].category, ResultCategory::Unsupported);
    let (mut module, plan) = baseline();
    module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Unsupported("acquire capability".into());
    assert_eq!(reject_code(module, plan), "E-UNSUPPORTED");
}
#[test]
fn refuses_nonempty_responsibility_certificates() {
    let (module, mut plan) = baseline();
    plan.responsibilities.push(ResponsibilityClaim {
        function: FunctionId(0),
        carrier: LocalId(0),
        origin: OriginId(0),
        state: ResponsibilityState::Live,
    });
    assert_eq!(
        verify_module(
            module,
            plan,
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .unwrap_err()[0]
            .category,
        ResultCategory::Unsupported
    );
}

fn branching() -> (TypedModule, CandidatePlan) {
    let (mut module, mut plan) = baseline();
    let f = &mut module.functions[0];
    f.values = vec![Type::Bool, Type::I32, Type::I32, Type::I32];
    f.locals[0].mutable = true;
    f.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![inst(
                Some(0),
                InstructionKind::Constant(Constant::Bool(true)),
            )],
            terminator: Terminator::Branch {
                condition: ValueId(0),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![
                inst(Some(1), InstructionKind::Constant(Constant::I32(42))),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(1),
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![
                inst(Some(2), InstructionKind::Constant(Constant::I32(7))),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(2),
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![inst(Some(3), InstructionKind::Load(LocalId(0)))],
            terminator: Terminator::Return(Some(ValueId(3))),
        },
    ];
    plan.module_fingerprint = module.fingerprint();
    plan.functions[0].blocks = vec![
        BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![],
        },
        BlockPlan {
            block: BlockId(1),
            initialized_in: vec![],
            initialized_out: vec![LocalId(0)],
        },
        BlockPlan {
            block: BlockId(2),
            initialized_in: vec![],
            initialized_out: vec![LocalId(0)],
        },
        BlockPlan {
            block: BlockId(3),
            initialized_in: vec![LocalId(0)],
            initialized_out: vec![LocalId(0)],
        },
    ];
    (module, plan)
}
#[test]
fn accepts_diamond_with_all_paths_initialized() {
    let (module, plan) = branching();
    assert!(
        verify_module(
            module,
            plan,
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .is_ok()
    );
}
#[test]
fn branch_without_store_cannot_fake_join_initialization() {
    let (mut module, mut plan) = branching();
    module.functions[0].blocks[2].instructions.pop();
    plan.functions[0].blocks[2].initialized_out.clear();
    assert_eq!(reject_code(module, plan), "E-VERIFY-INITIALIZATION-CLAIM");
}
#[test]
fn branch_without_store_is_rejected_even_with_honest_plan() {
    let (mut module, mut plan) = branching();
    module.functions[0].blocks[2].instructions.pop();
    plan.functions[0].blocks[2].initialized_out.clear();
    plan.functions[0].blocks[3].initialized_in.clear();
    plan.functions[0].blocks[3].initialized_out.clear();
    assert_eq!(reject_code(module, plan), "E-VERIFY-INITIALIZATION");
}
#[test]
fn rejects_nondominating_branch_value_at_merge() {
    let (mut module, plan) = branching();
    module.functions[0].blocks[3].terminator = Terminator::Return(Some(ValueId(1)));
    assert_eq!(reject_code(module, plan), "E-VERIFY-DOMINANCE");
}
#[test]
fn rejects_non_boolean_branch_and_missing_target() {
    let (mut module, plan) = branching();
    module.functions[0].blocks[0].terminator = Terminator::Branch {
        condition: ValueId(1),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    assert_eq!(reject_code(module, plan), "E-VERIFY-BRANCH");
    let (mut module, plan) = branching();
    module.functions[0].blocks[0].terminator = Terminator::Jump(BlockId(999));
    assert_eq!(reject_code(module, plan), "E-VERIFY-CFG");
}
#[test]
fn accepts_loop_fixed_point_and_rejects_budget_exhaustion() {
    let (mut module, mut plan) = baseline();
    let f = &mut module.functions[0];
    f.locals[0].mutable = true;
    f.values = vec![
        Type::I32,
        Type::I32,
        Type::I32,
        Type::Bool,
        Type::I32,
        Type::I32,
        Type::I32,
    ];
    f.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![
                inst(Some(0), InstructionKind::Constant(Constant::I32(0))),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(0),
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(1)),
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![
                inst(Some(1), InstructionKind::Load(LocalId(0))),
                inst(Some(2), InstructionKind::Constant(Constant::I32(3))),
                inst(
                    Some(3),
                    InstructionKind::Binary {
                        op: BinaryOp::Less,
                        left: ValueId(1),
                        right: ValueId(2),
                    },
                ),
            ],
            terminator: Terminator::Branch {
                condition: ValueId(3),
                then_block: BlockId(2),
                else_block: BlockId(3),
            },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![
                inst(Some(4), InstructionKind::Constant(Constant::I32(1))),
                inst(
                    Some(5),
                    InstructionKind::Binary {
                        op: BinaryOp::Add,
                        left: ValueId(1),
                        right: ValueId(4),
                    },
                ),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(5),
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(1)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![inst(Some(6), InstructionKind::Load(LocalId(0)))],
            terminator: Terminator::Return(Some(ValueId(6))),
        },
    ];
    plan.module_fingerprint = module.fingerprint();
    plan.functions[0].holds = (0..module.functions[0].values.len())
        .map(|i| ValueHold {
            value: ValueId(i as u32),
            roots: vec![],
        })
        .collect();
    plan.functions[0].blocks = (0..4)
        .map(|i| BlockPlan {
            block: BlockId(i),
            initialized_in: if i == 0 { vec![] } else { vec![LocalId(0)] },
            initialized_out: vec![LocalId(0)],
        })
        .collect();
    assert!(
        verify_module(
            module.clone(),
            plan.clone(),
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .is_ok()
    );
    let limits = Limits {
        max_analysis_steps: 1,
        ..Limits::default()
    };
    assert_eq!(
        verify_module(module, plan, &TargetProfile::host_llvm(), &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}
#[test]
fn checks_direct_call_signature_and_parameter_initialization() {
    let (mut module, mut plan) = baseline();
    module.functions.push(TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        result_mode: zil_ir::PassingMode::Infer,
        foreign: None,
        environment: None,
        id: FunctionId(1),
        name: "identity".into(),
        parameters: vec![LocalId(0)],
        return_type: Type::I32,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "x".into(),
            ty: Type::I32,
            mutable: false,
            span: span(),
        }],
        values: vec![Type::I32],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![inst(Some(0), InstructionKind::Load(LocalId(0)))],
            terminator: Terminator::Return(Some(ValueId(0))),
        }],
        entry: BlockId(0),
        span: span(),
    });
    module.functions[0].blocks[0].instructions[2].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: vec![ValueId(0)],
    };
    plan.functions.push(FunctionPlan {
        function: FunctionId(1),
        holds: (0..module.functions[1].values.len())
            .map(|i| zil_ir::ValueHold {
                value: ValueId(i as u32),
                roots: vec![],
            })
            .collect(),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0)],
            initialized_out: vec![LocalId(0)],
        }],
    });
    plan.module_fingerprint = module.fingerprint();
    assert!(
        verify_module(
            module.clone(),
            plan.clone(),
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .is_ok()
    );
    module.functions[0].blocks[0].instructions[2].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: vec![],
    };
    assert_eq!(reject_code(module, plan), "E-VERIFY-CALL");
}
#[test]
fn unreachable_cannot_be_used_as_a_runtime_escape() {
    let (mut module, plan) = baseline();
    module.functions[0].blocks[0].terminator = Terminator::Unreachable;
    assert_eq!(reject_code(module, plan), "E-VERIFY-UNREACHABLE");
}
#[test]
fn malformed_parameter_id_is_diagnostic_not_panic() {
    let (mut module, plan) = baseline();
    module.functions[0].parameters.push(LocalId(u32::MAX));
    assert_eq!(reject_code(module, plan), "E-VERIFY-ID");
}
#[test]
fn metadata_cost_is_bounded_before_module_fingerprinting() {
    let (mut module, plan) = baseline();
    module.functions[0].name = "x".repeat(4096);
    let limits = Limits {
        max_analysis_steps: 1024,
        ..Limits::default()
    };
    let diagnostics =
        verify_module(module, plan, &TargetProfile::host_llvm(), &limits).unwrap_err();
    assert_eq!(diagnostics[0].category, ResultCategory::ResourceLimit);
}

fn ct() -> CtModule {
    CtModule {
        schema_version: SCHEMA_VERSION,
        input_fingerprint: Fingerprint::of(b"synthetic CT input"),
        target: TargetProfile::host_llvm(),
        instructions: vec![
            CtInstruction {
                result: ValueId(0),
                ty: Type::I64,
                kind: CtInstructionKind::Constant(Constant::I64(20)),
            },
            CtInstruction {
                result: ValueId(1),
                ty: Type::I64,
                kind: CtInstructionKind::Constant(Constant::I64(22)),
            },
            CtInstruction {
                result: ValueId(2),
                ty: Type::I64,
                kind: CtInstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(0),
                    right: ValueId(1),
                },
            },
        ],
        result: ValueId(2),
        max_steps: 3,
        dependencies: vec![],
    }
}
#[test]
fn ct_checks_types_order_and_budget() {
    let module = ct();
    assert_eq!(
        verify_ct(module.clone(), &module.target, &Limits::default())
            .unwrap()
            .module(),
        &module
    );
    let mut wrong = module.clone();
    wrong.instructions[2].ty = Type::Bool;
    assert_eq!(
        verify_ct(wrong, &module.target, &Limits::default()).unwrap_err()[0].code,
        "E-CT-TYPE"
    );
    let mut future = module.clone();
    future.instructions[0].kind = CtInstructionKind::Unary {
        op: UnaryOp::Negate,
        operand: ValueId(2),
    };
    assert_eq!(
        verify_ct(future, &module.target, &Limits::default()).unwrap_err()[0].code,
        "E-CT-SSA"
    );
    let mut exhausted = module.clone();
    exhausted.max_steps = 2;
    assert_eq!(
        verify_ct(exhausted, &module.target, &Limits::default()).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}
#[test]
fn ct_forbidden_operations_and_raw_dependency_flags_fail_closed() {
    let mut module = ct();
    module.instructions[2].kind =
        CtInstructionKind::Forbidden("host filesystem/network/ffi".into());
    assert_eq!(
        verify_ct(module, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err()[0].code,
        "E-CT-FORBIDDEN"
    );
    let mut module = ct();
    module.dependencies.push(CtDependency {
        name: "pretend-completed".into(),
        fingerprint: Fingerprint::ZERO,
        completed: true,
    });
    assert_eq!(
        verify_ct(module, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err()[0].category,
        ResultCategory::Unsupported
    );
}

#[test]
fn decoded_type_claims_never_bypass_ct_verification() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = ct();
    let bytes = zil_ir::wire::encode_ct(&good, &limits).unwrap();
    assert_eq!(
        verify_ct_bytes(&bytes, &target, &limits).unwrap().module(),
        &good
    );
    let mut bad = good;
    bad.instructions[2].ty = Type::Bool;
    let bytes = zil_ir::wire::encode_ct(&bad, &limits).unwrap();
    assert!(zil_ir::wire::decode_ct(&bytes, &limits).is_ok());
    assert_eq!(
        verify_ct_bytes(&bytes, &target, &limits).unwrap_err()[0].code,
        "E-CT-TYPE"
    );
}

#[test]
fn raw_ct_cannot_evade_value_or_check_work_quotas() {
    let target = TargetProfile::host_llvm();
    for limits in [
        Limits {
            max_values: 2,
            ..Limits::default()
        },
        Limits {
            max_analysis_steps: 1,
            ..Limits::default()
        },
    ] {
        let errors = verify_ct(ct(), &target, &limits).unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    }
}

#[test]
fn decoded_candidate_facts_are_rechecked_even_with_correct_identity() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let (module, mut plan) = baseline();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &module
    );
    plan.functions[0].blocks[0].initialized_in.push(LocalId(0));
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert!(zil_ir::wire::decode_module_candidate(&bytes, &limits).is_ok());
    assert_eq!(
        verify_module_bytes(&bytes, &target, &limits).unwrap_err()[0].code,
        "E-VERIFY-INITIALIZATION-CLAIM"
    );
}
