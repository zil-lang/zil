#![forbid(unsafe_code)]

//! Lowering starts at the verifier's immutable capability. The public LIR is
//! data, not authority: the LLVM backend rebinds it to this same capability.

use zil_base::{CompileResult, Diagnostic, Phase, SCHEMA_VERSION};
use zil_ir::{LirModule, LirView, ScalarLayout, Type, TypedFunction};
use zil_verify::VerifiedModule;
mod retained_ir;

/// Retain the independently checked CFG and concrete closure definitions, and
/// attach the target's scalar layout. This retained view is not a complete
/// machine plan: the backend binds it and prepares every representation,
/// operation, ABI and cleanup dependency before machine emission.
/// RegisterDefer stays in the retained CFG
/// as a checked fact; its adjacent TakeLocal/CallClosureOnce carries the action.
/// No source AST, candidate plan, or raw
/// typed module is an entry point.
///
/// ```compile_fail
/// fn bypass(raw: zil_ir::TypedModule) {
///     let _ = zil_lower::lower(&raw);
/// }
/// ```
pub fn lower(verified: &VerifiedModule) -> CompileResult<LirModule> {
    let (view, mut budget) = lower_view_with_budget(verified)?;
    // Clone through the existing instruction walk, paying new variable-length
    // byte payloads before allocating them. Borrowed lowering has no such copy.
    let functions = view
        .functions
        .iter()
        .map(|function| clone_function(function, &mut budget))
        .collect::<CompileResult<_>>()?;
    Ok(LirModule {
        schema_version: view.schema_version,
        module_id: view.module_id,
        verified_fingerprint: view.verified_fingerprint,
        target: view.target.clone(),
        functions,
        closures: view.closures.to_vec(),
        resources: view.resources.to_vec(),
        layouts: view.layouts.into_owned(),
    })
}

/// Run the same target and retention checks, borrowing immutable verified CFG
/// storage. The returned public raw view must be rebound by the backend.
pub fn lower_view(verified: &VerifiedModule) -> CompileResult<LirView<'_>> {
    lower_view_with_budget(verified).map(|(view, _)| view)
}

fn lower_view_with_budget(
    verified: &VerifiedModule,
) -> CompileResult<(LirView<'_>, retained_ir::Budget<'_>)> {
    let module = verified.module();
    let target = &module.target;
    if module.schema_version != SCHEMA_VERSION {
        return Err(vec![Diagnostic::error(
            Phase::Lower,
            "E-LOWER-SCHEMA",
            None,
            "the verified module has an unsupported schema version",
        )]);
    }
    if !target.is_supported_x64_llvm() {
        return Err(vec![Diagnostic::unsupported(
            Phase::Lower,
            "lowering supports checked x86_64 Linux and Win64 LLVM targets",
            None,
        )]);
    }
    let mut retention_budget = retained_ir::Budget::new(verified.limits());
    // Charge retained relations and type trees without re-deriving permissions
    // or predicting the backend's representation of each use position.
    retention_budget.callable_metadata(module)?;
    retention_budget.dyn_tables(module)?;
    // This view preserves independently verified semantics; it does not certify
    // target-machine readiness. The backend must bind the exact view first,
    // then complete layout, ABI, helper and operation preparation together.
    // callable_metadata pays the complete retained type and contract inventory.
    // Do not rebuild a capture/parameter/result shape whitelist here.
    Ok((
        LirView {
            resources: &module.resources,
            schema_version: SCHEMA_VERSION,
            module_id: module.module_id,
            verified_fingerprint: verified.fingerprint(),
            target,
            functions: &module.functions,
            closures: &module.closures,
            layouts: std::borrow::Cow::Owned(scalar_layouts_for_width(target.pointer_width)),
        },
        retention_budget,
    ))
}

fn clone_function(
    function: &TypedFunction,
    budget: &mut retained_ir::Budget<'_>,
) -> CompileResult<TypedFunction> {
    // Exhaustive destructuring makes new retained fields explicit consumers.
    let TypedFunction {
        dyn_instance,
        retained_return_sources,
        storage_life,
        call_kind,
        resume_bound,
        reference_return_bound,
        callable_return_contract,
        callable_contract,
        protocol_contracts,
        id,
        name,
        foreign,
        environment,
        parameters,
        return_type,
        result_mode,
        locals,
        values,
        blocks,
        entry,
        span,
    } = function;
    let blocks = blocks
        .iter()
        .map(|block| {
            let instructions = block
                .instructions
                .iter()
                .map(|instruction| {
                    if let zil_ir::InstructionKind::StaticUtf8 { bytes } = &instruction.kind {
                        budget.static_bytes(bytes.len())?;
                    }
                    if let zil_ir::InstructionKind::CallDynSlot { arguments, .. } =
                        &instruction.kind
                    {
                        budget.static_bytes(
                            arguments
                                .len()
                                .saturating_mul(std::mem::size_of::<zil_ir::ValueId>()),
                        )?;
                    }
                    Ok(instruction.clone())
                })
                .collect::<CompileResult<_>>()?;
            Ok(zil_ir::BasicBlock {
                id: block.id,
                instructions,
                terminator: block.terminator.clone(),
            })
        })
        .collect::<CompileResult<_>>()?;
    Ok(TypedFunction {
        dyn_instance: dyn_instance.clone(),
        retained_return_sources: retained_return_sources.clone(),
        storage_life: storage_life.clone(),
        call_kind: *call_kind,
        resume_bound: *resume_bound,
        reference_return_bound: reference_return_bound.clone(),
        callable_return_contract: callable_return_contract.clone(),
        callable_contract: callable_contract.clone(),
        protocol_contracts: protocol_contracts.clone(),
        id: *id,
        name: name.clone(),
        foreign: foreign.clone(),
        environment: *environment,
        parameters: parameters.clone(),
        return_type: return_type.clone(),
        result_mode: *result_mode,
        locals: locals.clone(),
        values: values.clone(),
        blocks,
        entry: *entry,
        span: *span,
    })
}

fn scalar_layouts_for_width(pointer_width: u16) -> Vec<ScalarLayout> {
    vec![
        ScalarLayout {
            ty: Type::Unit,
            size: 0,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::Bool,
            size: 1,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::I32,
            size: 4,
            alignment: 4,
        },
        ScalarLayout {
            ty: Type::I64,
            size: 8,
            alignment: 8,
        },
        ScalarLayout {
            ty: Type::F32,
            size: 4,
            alignment: 4,
        },
        ScalarLayout {
            ty: Type::F64,
            size: 8,
            alignment: 8,
        },
        ScalarLayout {
            ty: Type::U8,
            size: 1,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::U16,
            size: 2,
            alignment: 2,
        },
        ScalarLayout {
            ty: Type::U32,
            size: 4,
            alignment: 4,
        },
        ScalarLayout {
            ty: Type::I8,
            size: 1,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::I16,
            size: 2,
            alignment: 2,
        },
        ScalarLayout {
            ty: Type::Isize,
            size: u32::from(pointer_width / 8),
            alignment: u32::from(pointer_width / 8),
        },
        ScalarLayout {
            ty: Type::ForeignHandle,
            size: 8,
            alignment: 8,
        },
        ScalarLayout {
            ty: Type::CWideString,
            size: 8,
            alignment: 8,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, Limits, ModuleId, ResultCategory, Span, TargetProfile};
    use zil_ir::{
        BasicBlock, BlockId, BlockPlan, CandidatePlan, ClosureDefinition, ClosureId, Constant,
        FunctionId, FunctionPlan, Instruction, InstructionKind, Terminator, TypedFunction,
        TypedModule, ValueId,
    };
    use zil_verify::verify_module;

    fn checked(target: TargetProfile) -> VerifiedModule {
        let module = TypedModule {
            semantic: None,
            dyn_declarations: Vec::new(),
            dyn_interfaces: Vec::new(),
            dyn_vtables: Vec::new(),
            life_declarations: Vec::new(),
            life_instances: Vec::new(),
            resources: Vec::new(),
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(1),
            source_fingerprint: Fingerprint::of(b"lower typed-IR fixture"),
            target: target.clone(),
            functions: vec![TypedFunction {
                dyn_instance: None,
                retained_return_sources: None,
                storage_life: None,
                call_kind: zil_ir::CallKind::Direct,
                resume_bound: zil_ir::TaskResumeBound::Local,
                reference_return_bound: None,
                callable_return_contract: None,
                callable_contract: None,
                protocol_contracts: Vec::new(),
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                id: FunctionId(0),
                name: "main".into(),
                environment: None,
                parameters: vec![],
                return_type: Type::I32,
                locals: vec![],
                values: vec![Type::I32],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions: vec![Instruction {
                        result: Some(ValueId(0)),
                        kind: InstructionKind::Constant(Constant::I32(0)),
                        span: Span::default(),
                    }],
                    terminator: Terminator::Return(Some(ValueId(0))),
                }],
                entry: BlockId(0),
                span: Span::default(),
            }],
            closures: vec![],
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
                initialized_out: vec![],
            }],
        });
        verify_module(module, plan, &target, &Limits::default()).unwrap()
    }

    #[test]
    fn owned_static_utf8_clone_prepays_exact_bytes_without_charging_old_clones() {
        // Component-level retained-IR copy check; source/native acceptance is
        // covered in the existing driver target by the parent implementation.
        let original = checked(TargetProfile::host_llvm());
        let scalar = &original.module().functions[0];
        let zero = zil_base::Limits {
            max_analysis_steps: 0,
            ..zil_base::Limits::default()
        };
        assert_eq!(
            clone_function(scalar, &mut retained_ir::Budget::new(&zero)).unwrap(),
            *scalar
        );
        for bytes in [b"".as_slice(), b"A\0Z".as_slice(), "é中😀".as_bytes()] {
            let mut function = scalar.clone();
            function.values[0] = Type::Address {
                mutable: false,
                inner: Box::new(Type::Slice {
                    element: Box::new(Type::U8),
                }),
            };
            function.return_type = function.values[0].clone();
            function.blocks[0].instructions[0].kind = InstructionKind::StaticUtf8 {
                bytes: bytes.to_vec(),
            };
            let needed = bytes.len() + 1;
            for _ in 0..2 {
                let exact = zil_base::Limits {
                    max_analysis_steps: needed,
                    ..zil_base::Limits::default()
                };
                assert_eq!(
                    clone_function(&function, &mut retained_ir::Budget::new(&exact)).unwrap(),
                    function
                );
                let short = zil_base::Limits {
                    max_analysis_steps: needed - 1,
                    ..zil_base::Limits::default()
                };
                let errors =
                    clone_function(&function, &mut retained_ir::Budget::new(&short)).unwrap_err();
                assert!(errors.iter().all(|error| error.phase == Phase::Lower
                    && error.category == zil_base::ResultCategory::ResourceLimit));
            }
        }
    }

    #[test]
    fn preserves_the_exact_checked_cfg_and_target_identity() {
        let verified = checked(TargetProfile::host_llvm());
        let lowered = lower(&verified).unwrap();
        assert_eq!(lowered.functions, verified.module().functions);
        assert_eq!(lowered.closures, verified.module().closures);
        assert_eq!(lowered.target, verified.module().target);
        assert_eq!(lowered.verified_fingerprint, verified.fingerprint());
        assert_eq!(
            lowered
                .layouts
                .iter()
                .map(|layout| (layout.size, layout.alignment))
                .collect::<Vec<_>>(),
            vec![
                (0, 1),
                (1, 1),
                (4, 4),
                (8, 8),
                (4, 4),
                (8, 8),
                (1, 1),
                (2, 2),
                (4, 4),
                (1, 1),
                (2, 2),
                (8, 8),
                (8, 8),
                (8, 8)
            ]
        );
    }

    #[test]
    fn verified_but_unimplemented_target_is_explicitly_unsupported() {
        let mut target = TargetProfile::host_llvm();
        target.triple = "aarch64-unknown-linux-gnu".into();
        let verified = checked(target);
        let diagnostics = lower(&verified).unwrap_err();
        assert_eq!(diagnostics[0].category, ResultCategory::Unsupported);
        assert_eq!(diagnostics[0].phase, Phase::Lower);
    }

    #[test]
    fn preserves_the_verified_environment_table_and_closure_operands_exactly() {
        let mut module = checked(TargetProfile::host_llvm()).module().clone();
        module.functions[0].values = vec![Type::I32, Type::Closure(ClosureId(0)), Type::I32];
        module.functions[0].blocks[0].instructions.extend([
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![ValueId(0)],
                },
                span: Span::default(),
            },
            Instruction {
                result: Some(ValueId(2)),
                kind: InstructionKind::CallClosure {
                    callee: ValueId(1),
                    arguments: vec![],
                },
                span: Span::default(),
            },
        ]);
        module.functions[0].blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
        module.functions.push(TypedFunction {
            dyn_instance: None,
            retained_return_sources: None,
            storage_life: None,
            call_kind: zil_ir::CallKind::Direct,
            resume_bound: zil_ir::TaskResumeBound::Local,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contracts: Vec::new(),
            result_mode: zil_ir::PassingMode::Infer,
            foreign: None,
            id: FunctionId(1),
            name: "$closure0".into(),
            environment: Some(ClosureId(0)),
            parameters: vec![],
            return_type: Type::I32,
            locals: vec![],
            values: vec![Type::I32],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::ReadCapture { index: 0 },
                    span: Span::default(),
                }],
                terminator: Terminator::Return(Some(ValueId(0))),
            }],
            entry: BlockId(0),
            span: Span::default(),
        });
        module.closures.push(ClosureDefinition {
            storage_life: None,
            lending_captures: Vec::new(),
            id: ClosureId(0),
            function: FunctionId(1),
            captures: vec![Type::I32],
            parameters: vec![],
            result: Type::I32,
            kind: zil_ir::CallableKind::Fn,
        });
        let mut plan = CandidatePlan::empty_for(&module);
        plan.functions = (0..2)
            .map(|id| FunctionPlan {
                function: FunctionId(id),
                holds: (0..module.functions[id as usize].values.len())
                    .map(|i| zil_ir::ValueHold {
                        value: ValueId(i as u32),
                        roots: vec![],
                    })
                    .collect(),
                blocks: vec![BlockPlan {
                    block: BlockId(0),
                    initialized_in: vec![],
                    initialized_out: vec![],
                }],
            })
            .collect();
        let target = module.target.clone();
        let verified = verify_module(module, plan, &target, &Limits::default()).unwrap();
        let lowered = lower(&verified).unwrap();
        assert_eq!(lowered.closures, verified.module().closures);
        assert_eq!(lowered.functions, verified.module().functions);
        assert_eq!(lowered.verified_fingerprint, verified.fingerprint());
        assert!(!Type::Closure(ClosureId(0)).is_b0_scalar());
    }
}
