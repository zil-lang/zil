//! Admission of complete owned data at a concrete callback boundary. This
//! deliberately walks the raw signature; a producer's mode or helper claim
//! does not establish an owning value or permit a borrowed result.
use super::*;

pub(super) fn root(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
    ) && !ty.is_copy_value()
}

pub(super) fn value(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    if !root(ty) {
        return Err(unsupported(
            "owning callback data must be a complete Product, Enum or ErrorUnion",
            span,
        ));
    }
    resources::charge_payload(ty, budget)?;
    match ty {
        Type::Package { .. } if !sums::needs_shape(ty, budget)? => {
            packages::check_ordinary_type(module, ty, span, budget)
        }
        Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. } => {
            sums::check_type_for(module, ty, span, budget)
        }
        _ => unreachable!(),
    }
}

pub(super) fn signature(
    module: &TypedModule,
    definition: &ClosureDefinition,
    budget: &mut Budget,
) -> Check<bool> {
    let owning = definition.parameters.iter().any(root) || root(&definition.result);
    if !owning {
        return Ok(false);
    }
    budget.spend(definition.parameters.len().saturating_add(1))?;
    for ty in definition
        .parameters
        .iter()
        .chain(std::iter::once(&definition.result))
    {
        if root(ty) {
            value(module, ty, None, budget)?;
        } else {
            // A new owning signature does not acquire address, description,
            // opaque environment, array or lending permissions from an old
            // Copy/address callback profile.
            resources::charge_payload(ty, budget)?;
            check_type(ty, None)?;
        }
    }
    if !definition.lending_captures.is_empty() {
        return Err(unsupported(
            "owning callback signatures cannot lend from their environment",
            None,
        ));
    }
    Ok(true)
}
