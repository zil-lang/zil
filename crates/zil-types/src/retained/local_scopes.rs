//! Complete proposals for local environments. None is never an empty domain.
//! Targets and region compatibility are reconstructed by the two kernels.
use super::*;

pub(super) fn closed(
    ty: &Type,
    module: &TypedModule,
    left: &mut usize,
    span: Span,
    depth: usize,
    limit: usize,
) -> R<bool> {
    spend(left, 1, span)?;
    if depth > limit {
        return Err(unsupported(
            span,
            "local callable type scope exceeds finite depth",
        ));
    }
    Ok(match ty {
        Type::Closure(id) => module
            .closures
            .get(id.0 as usize)
            .and_then(|d| d.storage_life.as_ref())
            .is_some_and(|life| life.fixed_binder_count == 0),
        Type::Managed(inner)
        | Type::OwnedStore { element: inner }
        | Type::Array { element: inner, .. } => {
            closed(inner, module, left, span, depth + 1, limit)?
        }
        Type::Tuple(fields) => {
            let mut all = true;
            for field in fields {
                all &= closed(field, module, left, span, depth + 1, limit)?;
            }
            all
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            let mut all = true;
            for (_, field) in fields {
                all &= closed(field, module, left, span, depth + 1, limit)?;
            }
            all
        }
        Type::Enum { variants, .. } => {
            let mut all = true;
            for (_, fields) in variants {
                for field in fields {
                    all &= closed(field, module, left, span, depth + 1, limit)?;
                }
            }
            all
        }
        Type::ErrorUnion { error, result } => {
            closed(error, module, left, span, depth + 1, limit)?
                && closed(result, module, left, span, depth + 1, limit)?
        }
        Type::BorrowedStorage { storage, life } => {
            life.binder_count == 0
                && life.slots.is_empty()
                && life.retained.is_empty()
                && life.nominal_arguments.is_empty()
                && closed(storage, module, left, span, depth + 1, limit)?
        }
        Type::ByteBuffer => true,
        ty => ty.is_b0_scalar(),
    })
}

fn add(
    ty: &Type,
    base: StorageBase,
    life: &mut StorageLifeContract,
    module: &TypedModule,
    left: &mut usize,
    span: Span,
    limits: &Limits,
) -> R<bool> {
    let capture = matches!(base, StorageBase::Capture(_));
    let mut binder = || -> R<StorageBinder> {
        let count = if capture {
            &mut life.fixed_binder_count
        } else {
            &mut life.invocation_binder_count
        };
        let index = *count;
        *count = count
            .checked_add(1)
            .ok_or_else(|| unsupported(span, "local callable class count overflow"))?;
        Ok(if capture {
            StorageBinder::Fixed(index)
        } else {
            StorageBinder::Invocation(index)
        })
    };
    let (target, pointed) = if let Type::Address { inner, .. } = ty {
        let bound = binder()?;
        life.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base,
                path: Vec::new(),
            },
            binder: bound,
        });
        (inner.as_ref(), true)
    } else {
        (ty, false)
    };
    let count = match target {
        Type::BorrowedStorage { life, .. } => Some(life.binder_count),
        Type::Closure(id) => module
            .closures
            .get(id.0 as usize)
            .and_then(|d| d.storage_life.as_ref())
            .map(|life| life.fixed_binder_count),
        Type::Slice { element } if pointed => {
            closed(element, module, left, span, 0, limits.max_call_depth)?.then_some(0)
        }
        ty => closed(ty, module, left, span, 0, limits.max_call_depth)?.then_some(0),
    };
    let Some(count) = count else {
        return Ok(false);
    };
    if count as usize > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "local callable class directory exceeds node quota",
            Some(span),
        ));
    }
    for class in 0..count {
        spend(left, 2, span)?;
        let bound = binder()?;
        life.classes.push(SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base,
                path: if pointed {
                    vec![LifeTypeProjection::Pointee]
                } else {
                    Vec::new()
                },
                class,
            },
            binder: bound,
        });
    }
    Ok(true)
}

pub(super) fn populate(
    module: &mut TypedModule,
    left: &mut usize,
    limits: &Limits,
    defer_static: bool,
) -> R<bool> {
    let mut changed = false;
    loop {
        let mut next = Vec::new();
        for definition in &module.closures {
            if definition.storage_life.is_some() || definition.kind == CallableKind::FunctionPointer
            {
                continue;
            }
            let span = module.functions[definition.function.0 as usize].span;
            // Owning results contribute no borrow occurrences only after the
            // complete actual type inventory proves their free domain empty.
            if !closed(
                &definition.result,
                module,
                left,
                span,
                0,
                limits.max_call_depth,
            )? {
                continue;
            }
            // Factory free classes use actual creator input equations below,
            // including a Static alternative weakened into an ordinary class.
            if module.functions.iter().any(|f| {
                f.callable_return_contract.as_ref().is_some_and(|returned| {
                    returned.closure == definition.id
                        && returned.source == CallableReturnSource::Fresh
                        && !returned.hold.is_empty()
                })
            }) {
                continue;
            }
            if defer_static && definition.captures.iter().any(|ty|matches!(ty,Type::Address{mutable:false,inner}if **inner==Type::U8 || matches!(inner.as_ref(),Type::Slice{element}if **element==Type::U8))) {continue;}
            let mut life = StorageLifeContract {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
                bindings: Vec::new(),
                static_bindings: Vec::new(),
                classes: Vec::new(),
            };
            let mut complete = true;
            for (index, ty) in definition.parameters.iter().enumerate() {
                inference::charge_type_snapshot(ty, span, limits, left, 1)?;
                complete &= add(
                    ty,
                    StorageBase::Parameter(index as u32),
                    &mut life,
                    module,
                    left,
                    span,
                    limits,
                )?;
            }
            for (index, ty) in definition.captures.iter().enumerate() {
                inference::charge_type_snapshot(ty, span, limits, left, 1)?;
                complete &= add(
                    ty,
                    StorageBase::Capture(index as u32),
                    &mut life,
                    module,
                    left,
                    span,
                    limits,
                )?;
            }
            if complete {
                borrowed_fp::charge_contract(&life, |work| spend(left, work, span), 2)?;
                next.push((definition.id, definition.function, life));
            }
        }
        if next.is_empty() {
            break;
        }
        changed = true;
        for (id, entry, life) in next {
            module.closures[id.0 as usize].storage_life = Some(life.clone());
            module.functions[entry.0 as usize].storage_life = Some(life);
        }
    }
    Ok(changed)
}
