//! Ordinary Verify is the positive control; transport alone grants no CT power.
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

fn raw(read: bool) -> (TypedModule, CandidatePlan) {
    let span = Span::new(FileId(0), 0, 1);
    let ty = Type::Enum {
        name: "@module0:CopyChoice239".into(),
        variants: vec![("Some".into(), vec![Type::I32])],
    };
    let op = if read {
        InstructionKind::ReadVariantLocal {
            local: LocalId(0),
            path: vec![VariantProjection {
                variant: 0,
                index: 0,
            }],
        }
    } else {
        InstructionKind::VariantTagLocal {
            local: LocalId(0),
            path: vec![],
        }
    };
    let instructions = [
        (None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        (None, InstructionKind::StorageLive(LocalId(0))),
        (
            Some(ValueId(0)),
            InstructionKind::Constant(Constant::I32(7)),
        ),
        (
            Some(ValueId(1)),
            InstructionKind::MakeVariant {
                variant: 0,
                fields: vec![ValueId(0)],
            },
        ),
        (
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(1),
            },
        ),
        (Some(ValueId(2)), op),
        (None, InstructionKind::StorageDead(LocalId(0))),
        (
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Fallthrough,
            },
        ),
    ]
    .into_iter()
    .map(|(result, kind)| Instruction { result, kind, span })
    .collect();
    let module = TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"raw CT239 runtime enum op"),
        target: TargetProfile::host_llvm(),
        resources: vec![],
        closures: vec![],
        unsupported: vec![],
        functions: vec![TypedFunction {
            id: FunctionId(0),
            name: "entry".into(),
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            environment: None,
            foreign: None,
            callable_contract: None,
            callable_return_contract: None,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "choice".into(),
                ty: ty.clone(),
                mutable: false,
                span,
            }],
            values: vec![Type::I32, ty, Type::I32],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator: Terminator::Return(Some(ValueId(2))),
            }],
            entry: BlockId(0),
            span,
        }],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![],
        }],
        holds: (0..3)
            .map(|i| ValueHold {
                value: ValueId(i),
                roots: vec![],
            })
            .collect(),
    });
    (module, plan)
}

#[test]
fn checked_copy_local_enum_observations_remain_runtime_only() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for read in [false, true] {
        let (module, plan) = raw(read);
        // This must reach ordinary executable authority before CT rejection
        // counts. A temporary unsupported Sum implementation cannot satisfy it.
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        let (module, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let errors = zil_verify::verify_ct_program(
            CtProgram {
                module,
                plan,
                entry: FunctionId(0),
                max_steps: limits.max_ct_steps,
                max_call_depth: limits.max_call_depth,
                max_live_slots: limits.max_values + limits.max_locals,
            },
            &target,
            &limits,
        )
        .unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert_eq!(errors[0].phase, Phase::Ct);
        assert_eq!(errors[0].category, ResultCategory::SafeReject);
        assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
        assert_eq!(
            errors[0].message,
            "instruction is not in the pure CT capability set"
        );
    }
}

#[test]
fn legacy_copy_enum_ssa_observations_still_execute_in_ct() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for read in [false, true] {
        let (mut module, mut plan) = raw(read);
        module.functions[0].blocks[0].instructions[5].kind = if read {
            InstructionKind::ReadVariant {
                value: ValueId(1),
                variant: 0,
                index: 0,
            }
        } else {
            InstructionKind::VariantTag { value: ValueId(1) }
        };
        plan.module_fingerprint = module.fingerprint();
        let verified = zil_verify::verify_ct_program(
            CtProgram {
                module,
                plan,
                entry: FunctionId(0),
                max_steps: limits.max_ct_steps,
                max_call_depth: limits.max_call_depth,
                max_live_slots: limits.max_values + limits.max_locals,
            },
            &target,
            &limits,
        )
        .unwrap();
        assert_eq!(
            zil_ct::evaluate_program(&verified).unwrap().value,
            zil_ct::CompileTimeValue::Scalar(Constant::I32(if read { 7 } else { 0 }))
        );
    }
}
