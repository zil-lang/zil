//! Synthetic hostile candidate tests. These are independent verifier evidence,
//! not source or native execution evidence.
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
fn candidate() -> (TypedModule, CandidatePlan) {
    let closure = ClosureId(0);
    let main = TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        result_mode: zil_ir::PassingMode::Infer,
        foreign: None,
        id: FunctionId(0),
        name: "main".into(),
        environment: None,
        parameters: vec![],
        return_type: Type::I32,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "f".into(),
            ty: Type::Closure(closure),
            mutable: false,
            span: span(),
        }],
        values: vec![
            Type::I32,
            Type::Closure(closure),
            Type::Closure(closure),
            Type::I32,
            Type::I32,
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                inst(Some(0), InstructionKind::Constant(Constant::I32(40))),
                inst(
                    Some(1),
                    InstructionKind::MakeClosure {
                        closure,
                        captures: vec![ValueId(0)],
                    },
                ),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(1),
                    },
                ),
                inst(Some(2), InstructionKind::Load(LocalId(0))),
                inst(Some(3), InstructionKind::Constant(Constant::I32(2))),
                inst(
                    Some(4),
                    InstructionKind::CallClosure {
                        callee: ValueId(2),
                        arguments: vec![ValueId(3)],
                    },
                ),
            ],
            terminator: Terminator::Return(Some(ValueId(4))),
        }],
        entry: BlockId(0),
        span: span(),
    };
    let entry = TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        result_mode: zil_ir::PassingMode::Infer,
        foreign: None,
        id: FunctionId(1),
        name: "$closure0".into(),
        environment: Some(closure),
        parameters: vec![LocalId(0)],
        return_type: Type::I32,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "x".into(),
            ty: Type::I32,
            mutable: false,
            span: span(),
        }],
        values: vec![Type::I32, Type::I32, Type::I32],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                inst(Some(0), InstructionKind::ReadCapture { index: 0 }),
                inst(Some(1), InstructionKind::Load(LocalId(0))),
                inst(
                    Some(2),
                    InstructionKind::Binary {
                        op: BinaryOp::Add,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                ),
            ],
            terminator: Terminator::Return(Some(ValueId(2))),
        }],
        entry: BlockId(0),
        span: span(),
    };
    let module = TypedModule {
        resources: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"synthetic closure candidate"),
        target: TargetProfile::host_llvm(),
        functions: vec![main, entry],
        unsupported: vec![],
        closures: vec![ClosureDefinition {
            id: closure,
            function: FunctionId(1),
            captures: vec![Type::I32],
            parameters: vec![Type::I32],
            result: Type::I32,
            kind: CallableKind::Fn,
        }],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = vec![
        FunctionPlan {
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
        },
        FunctionPlan {
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
        },
    ];
    (module, plan)
}
fn check(module: TypedModule, mut plan: CandidatePlan) -> CompileResult<VerifiedModule> {
    plan.module_fingerprint = module.fingerprint();
    verify_module(
        module,
        plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
}
fn rejected(mutator: impl FnOnce(&mut TypedModule), code: &str) {
    let (mut module, plan) = candidate();
    mutator(&mut module);
    assert_eq!(check(module, plan).unwrap_err()[0].code, code);
}

#[test]
fn concrete_copy_environment_and_wire_reverification_succeed() {
    let (module, plan) = candidate();
    assert!(check(module.clone(), plan.clone()).is_ok());
    let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let verified = verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap();
    assert_eq!(verified.module(), &module);
    for length in 0..bytes.len() {
        assert!(
            wire::decode_module_candidate(&bytes[..length], &Limits::default()).is_err(),
            "truncated prefix {length}"
        );
    }
}
#[test]
fn old_wire_version_is_rejected_even_for_scalar_bytes() {
    let (module, plan) = candidate();
    let mut bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err()[0].code,
        "E-WIRE-VERSION"
    );
}
#[test]
fn ordinary_call_cannot_skip_hidden_environment() {
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[5].kind = InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(3)],
            }
        },
        "E-VERIFY-CLOSURE-ENTRY",
    );
}
#[test]
fn read_capture_requires_the_current_entry_environment() {
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[0].kind =
                InstructionKind::ReadCapture { index: 0 }
        },
        "E-VERIFY-CAPTURE",
    );
    rejected(
        |m| {
            m.functions[1].blocks[0].instructions[0].kind =
                InstructionKind::ReadCapture { index: 1 }
        },
        "E-VERIFY-CAPTURE",
    );
    rejected(|m| m.functions[1].values[0] = Type::Bool, "E-VERIFY-TYPE");
}
#[test]
fn orphaned_and_crossed_entry_identities_are_rejected() {
    rejected(
        |m| m.functions[1].environment = None,
        "E-VERIFY-CLOSURE-ENTRY",
    );
    rejected(
        |m| m.functions[0].environment = Some(ClosureId(0)),
        "E-VERIFY-CLOSURE-ENTRY",
    );
    rejected(
        |m| m.closures[0].function = FunctionId(0),
        "E-VERIFY-CLOSURE-ENTRY",
    );
    rejected(
        |m| m.closures[0].function = FunctionId(99),
        "E-VERIFY-CLOSURE-ENTRY",
    );
    rejected(|m| m.closures[0].id = ClosureId(1), "E-VERIFY-CLOSURE");
    rejected(
        |m| {
            m.closures.push(ClosureDefinition {
                id: ClosureId(1),
                ..m.closures[0].clone()
            })
        },
        "E-VERIFY-CLOSURE-ENTRY",
    );
}
#[test]
fn signature_claims_are_compared_to_the_real_entry() {
    rejected(
        |m| m.closures[0].parameters.clear(),
        "E-VERIFY-CLOSURE-ENTRY",
    );
    rejected(
        |m| m.closures[0].parameters[0] = Type::I64,
        "E-VERIFY-CLOSURE-ENTRY",
    );
    rejected(
        |m| m.closures[0].result = Type::Bool,
        "E-VERIFY-CLOSURE-ENTRY",
    );
}
#[test]
fn capture_fields_require_exact_count_order_and_types() {
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[1].kind = InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![],
            }
        },
        "E-VERIFY-CAPTURE",
    );
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[1].kind = InstructionKind::MakeClosure {
                closure: ClosureId(99),
                captures: vec![ValueId(0)],
            }
        },
        "E-VERIFY-CLOSURE",
    );
    rejected(
        |m| m.closures[0].captures[0] = Type::Bool,
        "E-VERIFY-CAPTURE",
    );
}
#[test]
fn closure_calls_check_the_actual_callee_and_arguments() {
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[5].kind = InstructionKind::CallClosure {
                callee: ValueId(0),
                arguments: vec![ValueId(3)],
            }
        },
        "E-VERIFY-CLOSURE-CALL",
    );
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[5].kind = InstructionKind::CallClosure {
                callee: ValueId(2),
                arguments: vec![],
            }
        },
        "E-VERIFY-CLOSURE-CALL",
    );
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[5].kind = InstructionKind::CallClosure {
                callee: ValueId(2),
                arguments: vec![ValueId(1)],
            }
        },
        "E-VERIFY-CLOSURE-CALL",
    );
}
#[test]
fn capture_and_callee_ssa_operands_cannot_escape_bounds_or_dominance() {
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[1].kind = InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![ValueId(99)],
            }
        },
        "E-VERIFY-ID",
    );
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[1].kind = InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![ValueId(3)],
            }
        },
        "E-VERIFY-DOMINANCE",
    );
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[5].kind = InstructionKind::CallClosure {
                callee: ValueId(99),
                arguments: vec![ValueId(3)],
            }
        },
        "E-VERIFY-ID",
    );
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[5].kind = InstructionKind::CallClosure {
                callee: ValueId(1),
                arguments: vec![ValueId(0)],
            };
            m.functions[0].blocks[0].instructions.swap(1, 5);
        },
        "E-VERIFY-DOMINANCE",
    );
}
#[test]
fn closures_never_become_scalars_for_arithmetic_comparison_or_ct() {
    rejected(
        |m| {
            m.functions[0].blocks[0].instructions[5].kind = InstructionKind::Binary {
                op: BinaryOp::Equal,
                left: ValueId(2),
                right: ValueId(2),
            }
        },
        "E-VERIFY-TYPE",
    );
    let ct = CtModule {
        schema_version: SCHEMA_VERSION,
        input_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        instructions: vec![CtInstruction {
            result: ValueId(0),
            kind: CtInstructionKind::Constant(Constant::I32(1)),
            ty: Type::Closure(ClosureId(0)),
        }],
        result: ValueId(0),
        max_steps: 1,
        dependencies: vec![],
    };
    assert!(verify_ct(ct, &TargetProfile::host_llvm(), &Limits::default()).is_err());
}
#[test]
fn closure_proof_rejects_false_call_permissions_resources_and_unsupported_kinds() {
    for ty in [
        Type::Managed(Box::new(Type::I32)),
        Type::Desc(Box::new(Type::I32)),
        Type::Closure(ClosureId(0)),
    ] {
        let (mut module, plan) = candidate();
        module.closures[0].captures[0] = ty;
        assert_eq!(
            check(module, plan).unwrap_err()[0].category,
            ResultCategory::Unsupported
        );
    }
    rejected(
        |module| {
            module.closures[0].captures[0] = Type::Address {
                mutable: true,
                inner: Box::new(Type::I32),
            };
        },
        "E-VERIFY-CLOSURE-CALL",
    );
    // FnMut is admitted only through its real-place calling instruction.
    let (mut mutable, plan) = candidate();
    mutable.closures[0].kind = CallableKind::FnMut;
    assert_eq!(
        check(mutable, plan).unwrap_err()[0].code,
        "E-VERIFY-CLOSURE-CALL"
    );
    // A consuming entry is supported, but this fixture copies its environment.
    rejected(
        |module| module.closures[0].kind = CallableKind::FnOnce,
        "E-VERIFY-RESOURCE",
    );
    {
        let (mut module, plan) = candidate();
        module.closures[0].kind = CallableKind::FunctionPointer;
        assert_eq!(
            check(module, plan).unwrap_err()[0].category,
            ResultCategory::Unsupported
        );
    }
}
#[test]
fn closure_table_and_operands_participate_in_hashes_and_work_limits() {
    let (module, plan) = candidate();
    let mut changed = module.clone();
    changed.closures[0].captures[0] = Type::I64;
    assert_ne!(module.fingerprint(), changed.fingerprint());
    changed = module.clone();
    changed.functions[1].environment = None;
    assert_ne!(module.fingerprint(), changed.fingerprint());
    changed = module.clone();
    changed.functions[0].blocks[0].instructions[5].kind = InstructionKind::CallClosure {
        callee: ValueId(1),
        arguments: vec![ValueId(3)],
    };
    assert_ne!(module.fingerprint(), changed.fingerprint());
    let limits = Limits {
        max_analysis_steps: 1,
        ..Limits::default()
    };
    assert_eq!(
        verify_module(module.clone(), plan, &module.target, &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}
