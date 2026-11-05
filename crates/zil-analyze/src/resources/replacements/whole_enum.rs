//! A definite whole Enum Commit changes one actual local's tagged contents.
//! The local shell's debt and cleanup age stay in the ordinary resource state.
use super::*;

pub(super) fn admitted(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::Enum { variants, .. } = ty else {
        return Ok(false);
    };
    b.type_work(ty, true, "whole Enum replacement qualification")?;
    fee(b, variants.len())?;
    let mut owning = false;
    for (_, fields) in variants {
        fee(b, fields.len())?;
        let mut owners = 0;
        for field in fields {
            if let Type::Managed(payload) = field {
                if !matches!(payload.as_ref(), Type::Record { .. }) || !payload.is_copy_value() {
                    return Ok(false);
                }
                owners += 1;
            } else if !field.is_b0_scalar() {
                return Ok(false);
            }
        }
        owning |= owners != 0;
    }
    Ok(owning)
}

pub(super) fn local(f: &TypedFunction, target: &Target, b: &mut Budget) -> CompileResult<LocalId> {
    fee(b, f.parameters.len())?;
    match target.root {
        origins::Root::Local(local)
            if target.path.is_empty() && target.mutable && !f.parameters.contains(&local) =>
        {
            Ok(local)
        }
        _ => Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "complete Enum replacement requires this body's exact whole local owner",
            None,
        )]),
    }
}

pub(super) fn apply(
    flow: &Flow<'_>,
    state: &mut State,
    target: &Target,
    rhs: ValueId,
    point: CheckPoint<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    let local = local(flow.function, target, b)?;
    if point.validate {
        if !exact(&copied(state, Place::Local(local), b)?, Life::Available) {
            return Err(error("complete Enum replacement needs its live old shell"));
        }
        state.sums.require_place(local, &[], b)?;
        let live = point
            .live
            .ok_or_else(|| error("Enum replacement lacks old-instance liveness"))?;
        let mut descriptions = Live::new();
        fee(b, live.len())?;
        for place in live {
            let ty = match place {
                Place::Local(l) => &flow.function.locals[l.0 as usize].ty,
                Place::Value(v) => &flow.function.values[v.0 as usize],
                _ => continue,
            };
            if matches!(ty, Type::Desc(_) | Type::Closure(_))
                || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_)))
            {
                b.lookup(descriptions.len(), "Enum old description carrier insertion")?;
                descriptions.insert(*place);
            }
        }
        no_later_use(state, &descriptions, Source::Head(local), b)?;
    }
    state.sums.replace_enum(
        local,
        rhs,
        flow.sums
            .ok_or_else(|| error("whole Enum replacement has no actual Sum plan"))?,
        point.validate,
        b,
    )
}

/// Exact borrowed endpoint shape; actual address mode, parent loan and contents
/// are proved separately. No tagged must-Commit relation is exported.
pub(super) fn borrowed(
    module: &TypedModule,
    f: &TypedFunction,
    target: &Target,
    b: &mut Budget,
) -> CompileResult<()> {
    let unsupported = || {
        vec![Diagnostic::unsupported(
            Phase::Analyze,
            "complete nonlocal Enum replacement needs an exact exclusive input or capture",
            None,
        )]
    };
    if !target.path.is_empty()
        || !target.mutable
        || f.foreign.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.storage_life.is_some()
        || !package::copy_result(&f.return_type, b)?
    {
        return Err(unsupported());
    }
    let (parameter, ty) = match target.root {
        origins::Root::Parameter(p) => {
            let local = f.parameters.get(p as usize).ok_or_else(unsupported)?;
            (Some(p), &f.locals[local.0 as usize].ty)
        }
        origins::Root::Capture(c) => {
            let id = f.environment.ok_or_else(unsupported)?;
            let definition = module.closures.get(id.0 as usize).ok_or_else(unsupported)?;
            (
                None,
                definition
                    .captures
                    .get(c as usize)
                    .ok_or_else(unsupported)?,
            )
        }
        _ => return Err(unsupported()),
    };
    let Type::Address {
        mutable: true,
        inner,
    } = ty
    else {
        return Err(unsupported());
    };
    if !admitted(inner, b)? {
        return Err(unsupported());
    }
    fee(b, f.parameters.len())?;
    for (p, local) in f.parameters.iter().enumerate() {
        if parameter == Some(p as u32) {
            continue;
        }
        let other = &f.locals[local.0 as usize].ty;
        b.type_work(other, true, "whole Enum actual companion shape")?;
        let managed = matches!(other, Type::Managed(payload) if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value());
        let address = matches!(other, Type::Address { inner: endpoint, .. } if endpoint.as_ref() == inner.as_ref());
        if !(other.is_b0_scalar() || managed || other == inner.as_ref() || address) {
            return Err(unsupported());
        }
    }
    Ok(())
}

pub(super) fn add(
    rows: &mut TaggedEffects,
    target: &Target,
    incoming: &sums::Replacement,
    b: &mut Budget,
) -> CompileResult<bool> {
    b.lookup(rows.len(), "tagged replacement exact summary key")?;
    fee(
        b,
        rows.len()
            .saturating_mul(target.path.len().saturating_add(1)),
    )?;
    for old in rows.keys() {
        fee(b, old.path.len().saturating_add(1))?;
    }
    if let Some(old) = rows.get_mut(target) {
        old.merge(incoming, b)
    } else {
        fee(b, target.path.len().saturating_add(1))?;
        rows.insert(target.clone(), incoming.copied(b)?);
        Ok(true)
    }
}
pub(super) fn record(
    effect: &mut Effect,
    target: &Target,
    incoming: &sums::Replacement,
    b: &mut Budget,
) -> CompileResult<()> {
    match target.root {
        origins::Root::Parameter(p) => {
            b.lookup(
                effect.arguments.len(),
                "tagged replacement actual parameter write",
            )?;
            effect.arguments.insert(p);
        }
        origins::Root::Capture(c) => {
            b.lookup(
                effect.borrowed_fields.len(),
                "tagged replacement actual capture write",
            )?;
            effect.borrowed_fields.insert(c);
        }
        _ => {
            return Err(error(
                "tagged replacement summary has no exact nonlocal root",
            ));
        }
    }
    add(&mut effect.tagged_replacements, target, incoming, b)?;
    Ok(())
}
