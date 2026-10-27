#![forbid(unsafe_code)]

//! Lowering starts at the verifier's immutable capability. The public LIR is
//! data, not authority: the LLVM backend rebinds it to this same capability.

use zil_base::{CompileResult, Diagnostic, Phase, SCHEMA_VERSION};
use zil_ir::{CallableKind, LirModule, ScalarLayout, Type};
use zil_verify::VerifiedModule;

/// Retain the independently checked CFG and concrete closure definitions, and
/// attach the supported target layout. No source AST, candidate plan, or raw
/// typed module is an entry point.
///
/// ```compile_fail
/// fn bypass(raw: zil_ir::TypedModule) {
///     let _ = zil_lower::lower(&raw);
/// }
/// ```
pub fn lower(verified: &VerifiedModule) -> CompileResult<LirModule> {
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
    for closure in &module.closures {
        if !matches!(
            closure.kind,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) || !closure.result.is_copy_value()
            || !closure.captures.iter().all(|ty| {
                ty.is_copy_value()
                    || callable_parameter_type(ty, &module.closures)
                    || matches!(ty, Type::Address { inner, .. } if inner.is_copy_value())
                    || (resource_type(ty, &module.resources)
                        && !matches!(ty, Type::Desc(p) if p.as_ref() == &Type::ByteBuffer))
                    || (package_type(ty, &module.resources) && !package_array_fields(ty))
            })
            || !closure.parameters.iter().all(Type::is_copy_value)
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Lower,
                "lowering requires a finite closure environment and Copy signature",
                None,
            )]);
        }
    }
    for function in &module.functions {
        if !(function.return_type.is_copy_value()
            || function.return_type.is_scalar_or_address()
            || resource_type(&function.return_type, &module.resources)
            || package_type(&function.return_type, &module.resources)
            || error_type(&function.return_type, &module.resources)
            || owned_array_type(&function.return_type, &module.resources)
            || (matches!(function.return_type, Type::Closure(_))
                && function.callable_return_contract.is_some()))
            || function.parameters.iter().any(|id| {
                let ty = &function.locals[id.0 as usize].ty;
                !(ty.is_copy_value()
                    || ty.is_scalar_or_address()
                    || resource_type(ty, &module.resources)
                    || package_type(ty, &module.resources)
                    || error_type(ty, &module.resources)
                    || owned_array_type(ty, &module.resources)
                    || callable_parameter_type(ty, &module.closures))
            })
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Lower,
                "lowering requires a scalar function signature",
                Some(function.span),
            )]);
        }
        for ty in function
            .locals
            .iter()
            .map(|local| &local.ty)
            .chain(function.values.iter())
        {
            let concrete_closure = callable_parameter_type(ty, &module.closures);
            if !ty.is_copy_value()
                && !concrete_closure
                && !matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee())
                && !resource_type(ty, &module.resources)
                && !package_type(ty, &module.resources)
                && !error_type(ty, &module.resources)
                && !owned_array_type(ty, &module.resources)
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Lower,
                    format!("lowering type {}", ty.name()),
                    Some(function.span),
                )]);
            }
        }
    }
    Ok(LirModule {
        resources: module.resources.clone(),
        schema_version: SCHEMA_VERSION,
        module_id: module.module_id,
        verified_fingerprint: verified.fingerprint(),
        target: target.clone(),
        functions: module.functions.clone(),
        closures: module.closures.clone(),
        layouts: scalar_layouts(),
    })
}

fn resource_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    if matches!(ty, Type::Managed(payload) | Type::Desc(payload)
        if payload.as_ref() == &Type::ByteBuffer
        && resources.iter().any(|r| r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
            && r.payload == Type::ByteBuffer && r.teardown.is_none()))
    {
        return true;
    }
    match ty {
        Type::Managed(payload) | Type::Desc(payload) => {
            matches!(payload.as_ref(), Type::Record { name, .. }
                if payload.is_copy_value() && resources.iter().any(|r| &r.name == name && &r.payload == payload.as_ref()))
        }
        _ => false,
    }
}

fn package_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    let Type::Package { fields, .. } = ty else {
        return false;
    };
    fields.iter().any(|(_, ty)| matches!(ty, Type::Managed(_)))
        && fields.iter().all(|(_, ty)| {
            ty.is_copy_value() || matches!(ty, Type::Managed(_) if resource_type(ty, resources))
                || matches!(ty, Type::Array { element, length } if *length > 0 && package_type(element, resources) && !package_array_fields(element))
        })
}

fn package_array_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. } if fields.iter().any(|(_, ty)| matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. }))))
}

fn error_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if (error.is_copy_value() || package_type(error, resources))
            && (result.is_copy_value() || package_type(result, resources)))
}

fn owned_array_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    matches!(ty, Type::Array { element, length }
        if *length > 0 && (package_type(element, resources) || matches!(element.as_ref(), Type::Closure(_))))
}

/// The B0 ABI uses no storage for unit and natural scalar size/alignment. This
/// table describes storage; Boolean arithmetic still uses LLVM's `i1` values.
pub fn scalar_layouts() -> Vec<ScalarLayout> {
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

fn callable_parameter_type(ty: &Type, closures: &[zil_ir::ClosureDefinition]) -> bool {
    let id = match ty {
        Type::Closure(id) => id,
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => id,
            _ => return false,
        },
        _ => return false,
    };
    closures
        .get(id.0 as usize)
        .is_some_and(|definition| definition.id == *id)
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
            resources: Vec::new(),
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(1),
            source_fingerprint: Fingerprint::of(b"lower typed-IR fixture"),
            target: target.clone(),
            functions: vec![TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
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
                (1, 1),
                (2, 2),
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
            callable_return_contract: None,
            callable_contract: None,
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
            id: ClosureId(0),
            function: FunctionId(1),
            captures: vec![Type::I32],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::Fn,
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
