//! Independent raw/wire type, SSA, target and CT-budget boundaries.
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, ResultCategory, SCHEMA_VERSION, Span, TargetProfile,
};
use zil_ir::*;
use zil_verify::{verify_ct, verify_ct_bytes, verify_module, verify_module_bytes};

fn module(left: Constant, right: Constant, op: BinaryOp) -> TypedModule {
    let span = Span::new(FileId(0), 0, 1);
    let ty = left.ty();
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"124 independent raw shift"),
        target: TargetProfile::host_llvm(),
        resources: vec![],
        closures: vec![],
        unsupported: vec![],
        functions: vec![TypedFunction {
            id: FunctionId(0),
            name: "value".into(),
            environment: None,
            callable_contract: None,
            callable_return_contract: None,
            result_mode: PassingMode::Infer,
            foreign: None,
            parameters: vec![],
            return_type: ty.clone(),
            locals: vec![],
            values: vec![left.ty(), right.ty(), ty],
            entry: BlockId(0),
            span,
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction {
                        result: Some(ValueId(0)),
                        kind: InstructionKind::Constant(left),
                        span,
                    },
                    Instruction {
                        result: Some(ValueId(1)),
                        kind: InstructionKind::Constant(right),
                        span,
                    },
                    Instruction {
                        result: Some(ValueId(2)),
                        kind: InstructionKind::Binary {
                            op,
                            left: ValueId(0),
                            right: ValueId(1),
                        },
                        span,
                    },
                ],
                terminator: Terminator::Return(Some(ValueId(2))),
            }],
        }],
    }
}
fn plan(module: &TypedModule) -> CandidatePlan {
    let mut plan = CandidatePlan::empty_for(module);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        holds: (0..3)
            .map(|i| ValueHold {
                value: ValueId(i),
                roots: vec![],
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
fn runtime(module: TypedModule, expected: Option<&str>) {
    let limits = Limits::default();
    let plan = plan(&module); // Rebind genuine content identity after every mutation.
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let a = verify_module(module.clone(), plan, &module.target, &limits);
    let b = verify_module_bytes(&wire, &module.target, &limits);
    for result in [a, b] {
        match expected {
            Some(code) => assert_eq!(result.unwrap_err()[0].code, code),
            None => {
                result.unwrap();
            }
        }
    }
}
fn ct(module: &TypedModule) -> CtModule {
    let f = &module.functions[0];
    CtModule {
        schema_version: SCHEMA_VERSION,
        input_fingerprint: module.source_fingerprint,
        target: module.target.clone(),
        result: ValueId(2),
        max_steps: 3,
        dependencies: vec![],
        instructions: f.blocks[0]
            .instructions
            .iter()
            .map(|i| {
                let value = i.result.unwrap();
                CtInstruction {
                    result: value,
                    ty: f.values[value.0 as usize].clone(),
                    kind: match &i.kind {
                        InstructionKind::Constant(c) => CtInstructionKind::Constant(c.clone()),
                        InstructionKind::Binary { op, left, right } => CtInstructionKind::Binary {
                            op: *op,
                            left: *left,
                            right: *right,
                        },
                        _ => unreachable!(),
                    },
                }
            })
            .collect(),
    }
}

#[test]
fn raw_and_wire_accept_signed_shapes_but_do_not_pretend_to_prove_count_values() {
    for op in [BinaryOp::ShiftLeft, BinaryOp::ShiftRight] {
        for count in [-1, 0, 31, 32, 33] {
            let raw = module(Constant::I32(-3), Constant::I32(count), op);
            runtime(raw.clone(), None);
            let ct = ct(&raw);
            let limits = Limits::default();
            verify_ct(ct.clone(), &ct.target, &limits).unwrap();
            let bytes = wire::encode_ct(&ct, &limits).unwrap();
            verify_ct_bytes(&bytes, &ct.target, &limits).unwrap();
        }
        for count in [-1, 0, 63, 64, 65] {
            runtime(
                module(Constant::I64(i64::MIN), Constant::I64(count), op),
                None,
            );
        }
    }
    // Admission is structural. CT evaluation / checked native lowering must
    // reject invalid counts; those actual executions are exercised by Driver.
}

#[test]
fn forged_noninteger_mixed_types_results_and_future_operands_still_reject() {
    for op in [BinaryOp::ShiftLeft, BinaryOp::ShiftRight] {
        for (left, right) in [
            (Constant::Bool(true), Constant::Bool(false)),
            (Constant::I32(1), Constant::I64(1)),
            (Constant::I64(1), Constant::U8(1)),
        ] {
            let raw = module(left, right, op);
            runtime(raw.clone(), Some("E-UNSUPPORTED"));
            let ct = ct(&raw);
            let limits = Limits::default();
            let bytes = wire::encode_ct(&ct, &limits).unwrap();
            assert_eq!(
                verify_ct(ct.clone(), &ct.target, &limits).unwrap_err()[0].code,
                "E-UNSUPPORTED"
            );
            assert_eq!(
                verify_ct_bytes(&bytes, &ct.target, &limits).unwrap_err()[0].code,
                "E-UNSUPPORTED"
            );
        }
        let mut result = module(Constant::I32(1), Constant::I32(1), op);
        result.functions[0].values[2] = Type::I64;
        result.functions[0].return_type = Type::I64;
        runtime(result, Some("E-VERIFY-TYPE"));
        let mut future = module(Constant::I64(1), Constant::I64(1), op);
        if let InstructionKind::Binary { right, .. } =
            &mut future.functions[0].blocks[0].instructions[2].kind
        {
            *right = ValueId(2);
        }
        runtime(future, Some("E-VERIFY-DOMINANCE"));
    }
}

#[test]
fn signed_shift_ct_retains_closed_capabilities_and_existing_step_budget() {
    let limits = Limits::default();
    let mut ct = ct(&module(
        Constant::I64(-3),
        Constant::I64(1),
        BinaryOp::ShiftRight,
    ));
    ct.max_steps = 2;
    let bytes = wire::encode_ct(&ct, &limits).unwrap();
    for result in [
        verify_ct(ct.clone(), &ct.target, &limits),
        verify_ct_bytes(&bytes, &ct.target, &limits),
    ] {
        assert_eq!(
            result.unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
    }
    ct.max_steps = 3;
    ct.instructions[2].kind = CtInstructionKind::Forbidden("foreign call".into());
    let bytes = wire::encode_ct(&ct, &limits).unwrap();
    for result in [
        verify_ct(ct.clone(), &ct.target, &limits),
        verify_ct_bytes(&bytes, &ct.target, &limits),
    ] {
        assert_eq!(result.unwrap_err()[0].code, "E-CT-FORBIDDEN");
    }
}
