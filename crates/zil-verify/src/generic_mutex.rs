//! Independent type and nominal admission for generic mutex candidates.
//! Lock origin, exclusive payload loans and exactly-once cleanup are checked
//! separately by the resource and borrow passes.
use super::*;

fn bad(span: Option<Span>, message: &str) -> Diagnostic {
    reject("E-VERIFY-MUTEX", span, message)
}

fn error_type() -> Type {
    Type::Enum {
        name: "@std.sync:v1:MutexCreateError".into(),
        variants: vec![("Allocation".into(), vec![])],
    }
}

fn guard_type(payload: &Type) -> Type {
    Type::MutexGuard {
        payload: Box::new(payload.clone()),
        retained_life: RetainedLifeShape {
            binder_count: 1,
            ports: vec![RetainedBorrowPort {
                leaf: Type::Mutex {
                    payload: Box::new(payload.clone()),
                },
                mode: StoredBorrowMode::SharedAddress,
                binder: 0,
            }],
            retained: vec![],
            nominal_arguments: vec![],
        },
    }
}

pub(super) fn guard_shape(
    payload: &Type,
    actual: &RetainedLifeShape,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(
        actual
            .ports
            .len()
            .saturating_add(actual.retained.len())
            .saturating_add(actual.nominal_arguments.len())
            .saturating_add(4),
    )?;
    let Type::MutexGuard {
        retained_life: expected,
        ..
    } = guard_type(payload)
    else {
        unreachable!()
    };
    if actual != &expected {
        return Err(bad(
            None,
            "guard retained class differs from its actual mutex parent",
        ));
    }
    Ok(())
}

fn supported_payload(module: &TypedModule, payload: &Type, budget: &mut Budget) -> Check<()> {
    let mut pending = vec![payload];
    while let Some(ty) = pending.pop() {
        budget.spend(2)?;
        match ty {
            ty if ty.is_copy_value() => {}
            Type::Managed(inner) => {
                resources::check_type_for(module, ty, None, budget)?;
                if matches!(
                    inner.as_ref(),
                    Type::Mutex { .. } | Type::AsyncRegistration { .. } | Type::Task { .. }
                ) {
                    return Err(bad(
                        None,
                        "nested mutex or asynchronous payload lacks synchronous failure cleanup",
                    ));
                }
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                pending.extend(variants.iter().flat_map(|(_, fields)| fields));
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            _ => {
                return Err(bad(
                    None,
                    "mutex payload requires a complete ordinary owned value with synchronous cleanup",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn definition(
    module: &TypedModule,
    resource: &ResourceDefinition,
    budget: &mut Budget,
) -> Check<()> {
    let Type::Mutex { payload } = &resource.payload else {
        return Err(bad(None, "mutex definition has no sealed payload"));
    };
    resources::charge_payload(payload, budget)?;
    supported_payload(module, payload, budget)?;
    let expected = format!("@builtin:Mutex<{}>", payload.fingerprint());
    budget.spend(expected.len().saturating_add(resource.name.len()))?;
    if resource.name != expected || resource.teardown.is_some() {
        return Err(bad(
            None,
            "mutex definition has wrong identity or user teardown",
        ));
    }
    Ok(())
}

pub(super) fn instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    if module.target.triple != "x86_64-unknown-linux-gnu" || !module.target.is_supported_x64_llvm()
    {
        return Err(unsupported(
            "mutex requires the supported Linux x64 pthread ABI",
            span,
        ));
    }
    budget.spend(5)?;
    match instruction.kind {
        InstructionKind::MutexCreate { value } => {
            let payload = value_type(function, value, span)?;
            resources::charge_payload(payload, budget)?;
            supported_payload(module, payload, budget)?;
            Ok(Some(Type::ErrorUnion {
                error: Box::new(error_type()),
                result: Box::new(Type::Managed(Box::new(Type::Mutex {
                    payload: Box::new(payload.clone()),
                }))),
            }))
        }
        InstructionKind::MutexLock { mutex } => {
            let Type::Desc(inner) = value_type(function, mutex, span)? else {
                return Err(bad(span, "lock requires an actual mutex description"));
            };
            let Type::Mutex { payload } = inner.as_ref() else {
                return Err(bad(span, "lock requires a sealed mutex head"));
            };
            budget.spend(module.resources.len())?;
            resources::check_type_for(module, &Type::Desc(inner.clone()), span, budget)?;
            Ok(Some(guard_type(payload)))
        }
        InstructionKind::MutexValue { guard } => {
            let Type::Address {
                mutable: true,
                inner,
            } = value_type(function, guard, span)?
            else {
                return Err(bad(
                    span,
                    "payload projection requires a mutable guard borrow",
                ));
            };
            let Type::MutexGuard {
                payload,
                retained_life,
            } = inner.as_ref()
            else {
                return Err(bad(span, "payload projection requires a real guard"));
            };
            guard_shape(payload, retained_life, budget)?;
            Ok(Some(Type::Address {
                mutable: true,
                inner: payload.clone(),
            }))
        }
        InstructionKind::MutexUnlock { guard } => {
            let Type::MutexGuard {
                payload,
                retained_life,
            } = value_type(function, guard, span)?
            else {
                return Err(bad(span, "unlock consumes one actual guard"));
            };
            guard_shape(payload, retained_life, budget)?;
            Ok(Some(Type::Unit))
        }
        InstructionKind::DropMutexGuard { local } => {
            let Type::MutexGuard {
                payload,
                retained_life,
            } = local_type(function, local, span)?
            else {
                return Err(bad(span, "guard cleanup requires an actual guard local"));
            };
            guard_shape(payload, retained_life, budget)?;
            Ok(None)
        }
        _ => Err(bad(span, "instruction is not a mutex operation")),
    }
}
