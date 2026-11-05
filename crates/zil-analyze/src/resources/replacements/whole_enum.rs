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
        if owners > 1 {
            return Ok(false);
        }
        owning |= owners == 1;
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
