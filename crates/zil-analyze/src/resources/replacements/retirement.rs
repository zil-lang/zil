//! May-retire is a projection of actual Commit destinations, not generic writes
//! or a must-Fresh postcondition. The existing fixed point preserves these keys
//! across conditional/nested calls even when some paths do not Commit.
use super::*;

fn nonlocal(source: Source) -> bool {
    matches!(source.parent(), Source::Argument(_) | Source::BorrowedField(_))
}

fn family(flow: &Flow<'_>, target: &Target, b: &mut Budget) -> CompileResult<Option<Source>> {
    fee(b, target.path.len().saturating_add(1))?;
    Ok(Some(match target.root {
        origins::Root::Local(local) => {
            let ty = &flow.function.locals.get(local.0 as usize)
                .ok_or_else(|| error("retired actual local is absent"))?.ty;
            if let (Type::Package { .. }, Some(PlaceProjection::Field(index))) = (ty, target.path.first()) {
                // Nested paths retain the actual first inline child family.
                // Missing deeper source precision is conservative, never apart.
                flow.package_source(local, *index, b)?
            } else {
                Source::Head(local)
            }
        }
        origins::Root::Parameter(parameter) | origins::Root::ParameterCapture(parameter, _) => Source::Argument(parameter),
        origins::Root::Capture(capture) => Source::BorrowedField(capture),
        // Store's structural/element boundary already has its independent
        // actual parent instance invalidation consumer and exported effects.
        origins::Root::StoreElement(_) => return Ok(None),
    }))
}

pub(super) fn check(
    flow: &Flow<'_>,
    state: &State,
    target: &Target,
    point: CheckPoint<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    if !point.validate {
        return Ok(());
    }
    let live = point.live.ok_or_else(|| error("retirement has no actual liveness frontier"))?;
    let Some(retired) = family(flow, target, b)? else { return Ok(()); };
    fee(b, live.len())?;
    for place in live {
        if matches!(place, Place::Value(value) if Some(*value)==point.output) {
            // A call's new result is not a pre-existing caller description,
            // including when the same static SSA site is revisited by a loop.
            continue;
        }
        let ty = match place {
            Place::Local(local) => &flow.function.locals[local.0 as usize].ty,
            Place::Value(value) => &flow.function.values[value.0 as usize],
            _ => continue,
        };
        if !matches!(ty, Type::Desc(_) | Type::Closure(_))
            && !matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(),Type::Closure(_)))
        {
            // In particular the genuine parent address is still usable. It
            // is not an old-instance weak description merely by pointee type.
            continue;
        }
        let fact = copied(state, *place, b)?;
        fee(b, fact.sources.len())?;
        if fact.sources.iter().any(|source| {
            let actual = source.parent();
            actual == retired.parent()
                || (nonlocal(actual) && nonlocal(retired))
                || matches!((retired.parent(),actual),
                    (Source::Head(owner),Source::PackageField{local,..}|Source::OwnedPackageField{local,..}|Source::ArrayElement{local,..}) if owner==local)
        }) {
            return Err(error("actual replacement may retire a resource instance whose description is required later"));
        }
    }
    Ok(())
}
