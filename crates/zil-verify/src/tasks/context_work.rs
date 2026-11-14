//! Complete private fees for shared-result context admission and selection.
//! Ordinary payload walkers intentionally do not traverse opaque Task ports.
use super::*;

pub(crate) fn context(ty: &Type, b: &mut Budget) -> Check<()> {
    let Type::Task {
        result,
        retained_life,
    } = ty
    else {
        return Ok(());
    };
    dynamic_metadata::ty(result, b)?;
    dynamic_metadata::retained_shape(retained_life, b)
}

pub(crate) fn selection(mut ty: &Type, b: &mut Budget) -> Check<()> {
    loop {
        match ty {
            Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => {
                b.spend(1)?;
                ty = inner;
            }
            Type::ErrorUnion { result, .. } => {
                b.spend(1)?;
                ty = result;
            }
            _ => return context(ty, b),
        }
    }
}
