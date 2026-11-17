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
    // The complete actual domain qualifies the value structurally. Its
    // sources, affine occupancy and actual cleanup remain independent passes.
    dynamic_witness::intrinsic_domain(module,ty,budget)?;
    dynamic_witness::structural_actual(module,ty,DynStructuralBound::Value,budget)?;
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
    if root(inner){value(module,inner,None,budget)}else{
        dynamic_witness::intrinsic_domain(module,inner,budget)?;
        dynamic_witness::structural_actual(module,inner,DynStructuralBound::Value,budget)
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
        if let Type::Address{..}=ty{input(module,ty,budget)?;}
        else if root(ty){value(module,ty,None,budget)?;}
        else{
            resources::charge_payload(ty,budget)?;
            dynamic_witness::intrinsic_domain(module,ty,budget)?;
            dynamic_witness::structural_actual(module,ty,DynStructuralBound::Value,budget)?;
        }
    }
    if root(&definition.result){value(module,&definition.result,None,budget)?;}else{
        resources::charge_payload(&definition.result,budget)?;
        dynamic_witness::intrinsic_domain(module,&definition.result,budget)?;
        dynamic_witness::structural_actual(module,&definition.result,DynStructuralBound::Value,budget)?;
    }
    if !definition.lending_captures.is_empty() {
        return Err(unsupported(
            "owning callback signatures cannot lend from their environment",
            None,
        ));
    }
    Ok(true)
}
