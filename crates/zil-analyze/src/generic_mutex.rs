//! Producer-side reconstruction of generic mutex signatures and sealed heads.
//! Resource and loan passes independently check live origin and cleanup.
use super::*;
use zil_base::Span;

fn bad(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-MUTEX",
        Some(span),
        message,
    )]
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

fn payload(module: &TypedModule, ty: &Type, budget: &mut Budget, span: Span) -> CompileResult<()> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(2, "mutex payload shape")?;
        match ty {
            ty if ty.is_copy_value() => {}
            Type::Managed(inner) => {
                if matches!(
                    inner.as_ref(),
                    Type::Mutex { .. } | Type::Task { .. } | Type::AsyncRegistration { .. }
                ) {
                    return Err(bad(
                        span,
                        "mutex failure cannot synchronously clean nested or pollable owner",
                    ));
                }
                budget.spend(module.resources.len(), "mutex payload resource lookup")?;
                if module
                    .resources
                    .iter()
                    .filter(|r| r.payload == **inner)
                    .count()
                    != 1
                {
                    return Err(bad(
                        span,
                        "mutex payload owning resource is not uniquely defined",
                    ));
                }
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                budget.spend(fields.len(), "mutex payload fields")?;
                pending.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Array { element, length }
                if *length > 0 && matches!(element.as_ref(), Type::Package { .. }) =>
            {
                budget.spend(*length as usize, "mutex owning array cleanup slots")?;
                pending.push(element);
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len(), "mutex payload tuple")?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len(), "mutex payload variants")?;
                pending.extend(variants.iter().flat_map(|(_, fields)| fields));
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            _ => {
                return Err(bad(
                    span,
                    "mutex requires a complete ordinary Copy or owned payload",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if !budget.generic_mutex {
        return Ok(());
    }
    let mut has_mutex = false;
    for resource in &module.resources {
        budget.spend(1, "mutex resource inventory")?;
        if let Type::Mutex { payload: value } = &resource.payload {
            has_mutex = true;
            payload(module, value, budget, Span::default())?;
            let name = format!("@builtin:Mutex<{}>", value.fingerprint());
            budget.spend(
                name.len().saturating_add(resource.name.len()),
                "mutex resource identity",
            )?;
            budget.spend(module.resources.len(), "mutex sealed identity lookup")?;
            if resource.name != name
                || resource.teardown.is_some()
                || module.resources.iter().filter(|r| r.name == name).count() != 1
            {
                return Err(bad(
                    Span::default(),
                    "mutex resource has invalid sealed identity or teardown",
                ));
            }
        } else if resource.name.starts_with("@builtin:Mutex<") {
            return Err(bad(
                Span::default(),
                "mutex identity is bound to a different payload",
            ));
        }
    }
    for f in &module.functions {
        for block in &f.blocks {
            budget.spend(1, "mutex block inventory")?;
            for i in &block.instructions {
                budget.spend(1, "mutex instruction inventory")?;
                let value = |id: ValueId| {
                    f.values
                        .get(id.0 as usize)
                        .ok_or_else(|| bad(i.span, "mutex operand absent"))
                };
                let output = i.result.and_then(|id| f.values.get(id.0 as usize));
                let expected = match i.kind {
                    InstructionKind::MutexCreate { value: input } => {
                        let value_ty = value(input)?;
                        payload(module, value_ty, budget, i.span)?;
                        Some(Type::ErrorUnion {
                            error: Box::new(error_type()),
                            result: Box::new(Type::Managed(Box::new(Type::Mutex {
                                payload: Box::new(value_ty.clone()),
                            }))),
                        })
                    }
                    InstructionKind::MutexLock { mutex } => {
                        let Type::Desc(head) = value(mutex)? else {
                            return Err(bad(i.span, "lock requires an actual mutex descriptor"));
                        };
                        let Type::Mutex { payload: value_ty } = head.as_ref() else {
                            return Err(bad(i.span, "lock head is not a mutex"));
                        };
                        Some(guard_type(value_ty))
                    }
                    InstructionKind::MutexValue { guard } => {
                        let Type::Address {
                            mutable: true,
                            inner,
                        } = value(guard)?
                        else {
                            return Err(bad(i.span, "value needs mutable guard address"));
                        };
                        let Type::MutexGuard {
                            payload: value_ty, ..
                        } = inner.as_ref()
                        else {
                            return Err(bad(i.span, "value operand is not guard"));
                        };
                        if inner.as_ref() != &guard_type(value_ty) {
                            return Err(bad(
                                i.span,
                                "value guard has invalid retained mutex parent",
                            ));
                        }
                        Some(Type::Address {
                            mutable: true,
                            inner: value_ty.clone(),
                        })
                    }
                    InstructionKind::MutexUnlock { guard } => {
                        let Type::MutexGuard {
                            payload: value_ty, ..
                        } = value(guard)?
                        else {
                            return Err(bad(i.span, "unlock requires affine guard"));
                        };
                        if value(guard)? != &guard_type(value_ty) {
                            return Err(bad(
                                i.span,
                                "unlock guard has invalid retained mutex parent",
                            ));
                        }
                        Some(Type::Unit)
                    }
                    InstructionKind::DropMutexGuard { local } => {
                        let ty = f
                            .locals
                            .get(local.0 as usize)
                            .map(|l| &l.ty)
                            .ok_or_else(|| bad(i.span, "guard cleanup local absent"))?;
                        let Type::MutexGuard {
                            payload: value_ty, ..
                        } = ty
                        else {
                            return Err(bad(i.span, "guard cleanup local is not guard"));
                        };
                        if ty != &guard_type(value_ty) {
                            return Err(bad(i.span, "guard cleanup retained parent invalid"));
                        }
                        None
                    }
                    _ => continue,
                };
                has_mutex = true;
                if output != expected.as_ref() {
                    return Err(bad(i.span, "mutex result does not match actual operation"));
                }
                if f.call_kind != CallKind::Direct {
                    return Err(bad(
                        i.span,
                        "guard operations cannot enter a suspended frame",
                    ));
                }
            }
        }
    }
    if has_mutex
        && (module.target.triple != "x86_64-unknown-linux-gnu"
            || !module.target.is_supported_x64_llvm())
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "mutex requires Linux x64 pthread ABI",
            None,
        )]);
    }
    Ok(())
}
