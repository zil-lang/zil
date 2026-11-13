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
    // Ordinary StoreElement may contain a concrete callback field. That
    // container is independently supported elsewhere, but a new callback
    // data signature cannot use it to carry F through a result or argument.
    // Walk every embedded type, including Store elements and inactive Sum
    // alternatives; actual operation checks still prove its live branches.
    budget.spend(1)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        match ty {
            Type::Closure(_) => {
                return Err(unsupported(
                    "owning callback data cannot contain a concrete or opaque callback",
                    span,
                ));
            }
            Type::Array { element, .. }
            | Type::OwnedStore { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element) => {
                budget.spend(1)?;
                pending.push(element);
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                pending.push(error);
                pending.push(result);
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Enum { variants, .. } => {
                let width = variants
                    .iter()
                    .map(|(_, fields)| fields.len())
                    .sum::<usize>();
                budget.spend(width)?;
                pending.extend(variants.iter().flat_map(|(_, fields)| fields));
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                budget.spend(parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
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

pub(super) fn input(module: &TypedModule, ty: &Type, budget: &mut Budget) -> Check<()> {
    let Type::Address { inner, .. } = ty else {
        return Err(unsupported(
            "borrowed callback input must be an actual address",
            None,
        ));
    };
    resources::charge_payload(ty, budget)?;
    if root(inner) {
        value(module, inner, None, budget)
    } else {
        slices::check_pointee(inner, None)
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
    for ty in &definition.parameters {
        if let Type::Address { .. } = ty {
            resources::charge_payload(&definition.result, budget)?;
            if !root(&definition.result) {
                return Err(unsupported(
                    "borrowed inputs in an owning callback require a closed owned result",
                    None,
                ));
            }
            // Independently inspect the complete actual pointee. The ordinary
            // call/borrow checker still reconstructs its active input window.
            input(module, ty, budget)?;
        } else if root(ty) {
            value(module, ty, None, budget)?;
        } else {
            resources::charge_payload(ty, budget)?;
            check_type(ty, None)?;
        }
    }
    if root(&definition.result) {
        value(module, &definition.result, None, budget)?;
    } else {
        resources::charge_payload(&definition.result, budget)?;
        check_type(&definition.result, None)
            .map_err(|error| error.with_note("while checking the actual owning callback result"))?;
    }
    if !definition.lending_captures.is_empty() {
        return Err(unsupported(
            "owning callback signatures cannot lend from their environment",
            None,
        ));
    }
    Ok(true)
}
