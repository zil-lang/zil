//! Thin code values are Copy data, not data addresses or environments. Every
//! producer independently checks the real Direct target; indirect calls keep
//! exact complete input/result types and introduce no stored class or root.
use super::*;

const WORK: &str = "thin function pointer complete ABI";

pub(crate) fn data(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "thin function pointer data exceeds its finite type depth",
            None,
        )]);
    }
    if scalar(ty) {
        return Ok(true);
    }
    match ty {
        Type::FunctionPointer { parameters, result } => {
            b.spend(parameters.len(), WORK)?;
            for parameter in parameters {
                if !data(parameter, b, depth + 1)? {
                    return Ok(false);
                }
            }
            data(result, b, depth + 1)
        }
        Type::Tuple(fields) => {
            b.spend(fields.len(), WORK)?;
            for field in fields {
                if !data(field, b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Array { element, .. } => data(element, b, depth + 1),
        Type::Record { name, fields } => {
            b.spend(name.len().saturating_add(fields.len()), WORK)?;
            for (name, field) in fields {
                b.spend(name.len(), WORK)?;
                if !data(field, b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Enum { name, variants } => {
            b.spend(name.len().saturating_add(variants.len()), WORK)?;
            for (name, fields) in variants {
                b.spend(name.len().saturating_add(fields.len()), WORK)?;
                for field in fields {
                    if !data(field, b, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        Type::ErrorUnion { error, result } => {
            Ok(data(error, b, depth + 1)? && data(result, b, depth + 1)?)
        }
        // A nominal wrapper may carry phantom classes without any address
        // leaf. Thin signatures never erase it or reinterpret a rich pointer.
        _ => Ok(false),
    }
}

pub(crate) fn instruction(f: &TypedFunction, i: &Instruction) -> bool {
    let ty = match i.kind {
        InstructionKind::FunctionRef { .. } => i.result.and_then(|v| f.values.get(v.0 as usize)),
        InstructionKind::CallIndirect { callee, .. } => f.values.get(callee.0 as usize),
        _ => None,
    };
    matches!(ty, Some(Type::FunctionPointer { .. }))
}

pub(crate) fn types(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<()> {
    let value = match i.kind {
        InstructionKind::FunctionRef { .. } => i.result,
        InstructionKind::CallIndirect { callee, .. } => Some(callee),
        _ => return Err(bad("thin pointer checker received another operation")),
    };
    let ty = value
        .and_then(|v| f.values.get(v.0 as usize))
        .ok_or_else(|| bad("thin pointer operation lacks its exact code type"))?;
    let Type::FunctionPointer { parameters, result } = ty else {
        return Err(bad("thin pointer operation changes its code family"));
    };
    if !data(ty, b, 0)? {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "thin pointer needs complete Copy input/result data without a retained domain",
            Some(i.span),
        )]);
    }
    match &i.kind {
        InstructionKind::FunctionRef { function } => {
            let target = module
                .functions
                .get(function.0 as usize)
                .filter(|target| target.id == *function)
                .ok_or_else(|| bad("thin pointer target has no exact function identity"))?;
            b.spend(
                target
                    .parameters
                    .len()
                    .saturating_add(module.resources.len() + 1),
                WORK,
            )?;
            b.type_work(&target.return_type, false, WORK)?;
            if target.foreign.is_some()
                || target.environment.is_some()
                || target.call_kind != CallKind::Direct
                || target.callable_contract.is_some()
                || target.callable_return_contract.is_some()
                || !target.protocol_contracts.is_empty()
                || !matches!(target.result_mode, PassingMode::Infer | PassingMode::Copy)
                || target.parameters.len() != parameters.len()
                || target.return_type != **result
                || module
                    .resources
                    .iter()
                    .any(|resource| resource.teardown == Some(*function))
            {
                return Err(bad(
                    "thin pointer target changes its complete Direct code ABI",
                ));
            }
            for (local, expected) in target.parameters.iter().zip(parameters) {
                let actual = target
                    .locals
                    .get(local.0 as usize)
                    .filter(|actual| actual.id == *local)
                    .ok_or_else(|| bad("thin pointer parameter has no exact actual local"))?;
                b.type_work(&actual.ty, false, WORK)?;
                if actual.ty != *expected {
                    return Err(bad("thin pointer target changes a complete parameter type"));
                }
            }
            if let Some(life) = &target.storage_life {
                crate::signature_life::charge(life, b)?;
                if life.fixed_binder_count != 0
                    || life.invocation_binder_count != 0
                    || !life.bindings.is_empty()
                    || !life.static_bindings.is_empty()
                    || !life.classes.is_empty()
                {
                    return Err(bad(
                        "thin pointer erases the target's complete lifetime context",
                    ));
                }
            }
            if let Some(sources) = &target.retained_return_sources {
                b.spend(sources.len(), WORK)?;
                if !sources.is_empty() {
                    return Err(bad("thin pointer erases retained result sources"));
                }
            }
            if let Some(bound) = &target.reference_return_bound {
                b.spend(bound.len(), WORK)?;
                if !bound.is_empty() {
                    return Err(bad("thin pointer erases an address result bound"));
                }
            }
        }
        InstructionKind::CallIndirect { arguments, .. } => {
            b.spend(arguments.len(), WORK)?;
            if arguments.len() != parameters.len() {
                return Err(bad("thin indirect call changes its full arity"));
            }
            for (argument, expected) in arguments.iter().zip(parameters) {
                let actual = f
                    .values
                    .get(argument.0 as usize)
                    .ok_or_else(|| bad("thin indirect argument is absent"))?;
                b.type_work(actual, false, WORK)?;
                if actual != expected {
                    return Err(bad("thin indirect argument changes its complete type"));
                }
            }
            if **result == Type::Unit {
                if i.result.is_some() {
                    return Err(bad("Unit thin indirect call fabricates a result"));
                }
            } else {
                let actual = i
                    .result
                    .and_then(|v| f.values.get(v.0 as usize))
                    .ok_or_else(|| bad("thin indirect call omits its actual result"))?;
                b.type_work(actual, false, WORK)?;
                if actual != result.as_ref() {
                    return Err(bad("thin indirect result changes its complete type"));
                }
            }
        }
        _ => unreachable!("checked thin code operation"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};

    fn module() -> TypedModule {
        let pointer = Type::FunctionPointer {
            parameters: vec![Type::I32],
            result: Box::new(Type::I32),
        };
        let target = TypedFunction {
            dyn_instance: None,
            retained_return_sources: None,
            storage_life: None,
            call_kind: CallKind::Direct,
            resume_bound: TaskResumeBound::Local,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contracts: vec![],
            id: FunctionId(0),
            name: "target".into(),
            foreign: None,
            environment: None,
            parameters: vec![LocalId(0)],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "x".into(),
                ty: Type::I32,
                mutable: false,
                span: Span::default(),
            }],
            values: vec![pointer, Type::I32],
            blocks: vec![],
            entry: BlockId(0),
            span: Span::default(),
        };
        TypedModule {
            semantic: None,
            dyn_declarations: vec![],
            dyn_interfaces: vec![],
            dyn_vtables: vec![],
            life_declarations: vec![],
            life_instances: vec![],
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::host_llvm(),
            functions: vec![target],
            closures: vec![],
            resources: vec![],
            unsupported: vec![],
        }
    }

    #[test]
    fn actual_target_abi_and_phantom_context_cannot_be_erased() {
        let original = module();
        let instruction = Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::FunctionRef {
                function: FunctionId(0),
            },
            span: Span::default(),
        };
        let check = |module: &TypedModule| {
            types(
                module,
                &original.functions[0],
                &instruction,
                &mut Budget::new(&Limits::default()),
            )
        };
        assert!(check(&original).is_ok());
        for mutation in 0..6 {
            let mut changed = original.clone();
            let target = &mut changed.functions[0];
            match mutation {
                0 => target.call_kind = CallKind::Suspend,
                1 => target.environment = Some(ClosureId(0)),
                2 => target.locals[0].ty = Type::U32,
                3 => target.return_type = Type::U32,
                4 => target.result_mode = PassingMode::Move,
                _ => {
                    target.storage_life = Some(StorageLifeContract {
                        fixed_binder_count: 1,
                        invocation_binder_count: 0,
                        bindings: vec![],
                        static_bindings: vec![],
                        classes: vec![],
                    })
                }
            }
            assert!(check(&changed).is_err());
        }
    }

    #[test]
    fn copy_code_arrays_keep_full_signatures_and_inactive_variants() {
        let pointer = module().functions[0].values[0].clone();
        let array = Type::Array {
            element: Box::new(pointer),
            length: 2,
        };
        assert!(data(&array, &mut Budget::new(&Limits::default()), 0).unwrap());
        let hidden = Type::FunctionPointer {
            parameters: vec![],
            result: Box::new(Type::Enum {
                name: "Hidden".into(),
                variants: vec![
                    ("Empty".into(), vec![]),
                    (
                        "Borrowed".into(),
                        vec![Type::Address {
                            mutable: false,
                            inner: Box::new(Type::I32),
                        }],
                    ),
                ],
            }),
        };
        assert!(!data(&hidden, &mut Budget::new(&Limits::default()), 0).unwrap());
        let mut actual = module();
        let call = Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::CallIndirect {
                callee: ValueId(0),
                arguments: vec![ValueId(1)],
            },
            span: Span::default(),
        };
        assert!(
            types(
                &actual,
                &actual.functions[0],
                &call,
                &mut Budget::new(&Limits::default())
            )
            .is_ok()
        );
        actual.functions[0].values[1] = Type::U32;
        assert!(
            types(
                &actual,
                &actual.functions[0],
                &call,
                &mut Budget::new(&Limits::default())
            )
            .is_err()
        );
    }
}
