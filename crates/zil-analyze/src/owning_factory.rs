//! The producer's finite owning-factory boundary. No classification from Types
//! is consumed and none of this traversal is exported as verifier authority.
use super::*;

fn signature(definition: &ClosureDefinition, budget: &mut Budget) -> CompileResult<()> {
    budget.type_work(&definition.result, true, "owning factory root result")?;
    budget.spend(
        definition.parameters.len().saturating_add(1),
        "owning factory root signature",
    )?;
    if definition.kind == CallableKind::FunctionPointer
        || !definition.result.is_copy_value()
        || !definition.lending_captures.is_empty()
    {
        return Err(bad(
            "owning factory roots require non-lending Copy callable results",
        ));
    }
    for parameter in &definition.parameters {
        budget.type_work(parameter, true, "owning factory root parameter")?;
        let supported = parameter.is_copy_value()
            || matches!(parameter, Type::Address { inner, .. }
                if inner.is_copy_value()
                    || matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value()));
        if !supported {
            return Err(bad(
                "owning factory callable parameter is outside the finite synchronous profile",
            ));
        }
    }
    Ok(())
}

fn ordinary_capture(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<()> {
    budget.type_work(ty, true, "owning factory complete capture shape")?;
    if ty.is_copy_value()
        || matches!(ty, Type::Managed(payload)
            if **payload == Type::ByteBuffer || payload.is_copy_value())
    {
        return Ok(());
    }
    if dynamic_store::is_owned_carrier(ty) {
        return dynamic_store::owned_capture(module, ty, budget);
    }
    if let Type::Package { fields, .. } = ty {
        budget.spend(fields.len(), "owning factory Package profile")?;
        if fields
            .iter()
            .any(|(_, field)| dynamic_store::is_owned_carrier(field))
        {
            // This checks every field and each Store's exact canonical leaf
            // resource. A State or owned-array field cannot hide beside it.
            return dynamic_store::captured_package(module, ty, budget);
        }
        // Unlike ordinary Package value admission, the complete Product
        // profile excludes callback fields, owned arrays and borrowed leaves.
        return resources::product::admit(ty, budget);
    }
    Err(bad(
        "owning factory requires complete self-contained captures and companion parameters",
    ))
}

pub(super) fn check(
    module: &TypedModule,
    function: &TypedFunction,
    input: ClosureId,
    output: &ClosureDefinition,
    budget: &mut Budget,
) -> CompileResult<()> {
    if input == output.id {
        return Err(bad(
            "owning factory must produce a different closure identity from its input",
        ));
    }
    let incoming = module
        .closures
        .get(input.0 as usize)
        .ok_or_else(|| bad("owning factory input environment is absent"))?;
    signature(incoming, budget)?;
    signature(output, budget)?;
    budget.spend(
        function.parameters.len(),
        "owning factory companion parameter scan",
    )?;
    for parameter in &function.parameters {
        let ty = &function.locals[parameter.0 as usize].ty;
        if !matches!(ty, Type::Closure(_)) {
            ordinary_capture(module, ty, budget)?;
        }
    }

    // Gray/black DFS distinguishes a cycle from repeated references to one
    // type. A shared ClosureId does not merge runtime values or ownership.
    // Reserve storage before allocating; this path is never entered by old
    // factories with no direct owning callable input.
    budget.spend(
        module.closures.len().saturating_add(2),
        "owning factory capture graph tables",
    )?;
    let mut colors = vec![0u8; module.closures.len()];
    let mut pending = vec![(input, false), (output.id, false)];
    while let Some((id, leaving)) = pending.pop() {
        budget.spend(1, "owning factory capture graph visit")?;
        let color = colors
            .get_mut(id.0 as usize)
            .ok_or_else(|| bad("owning factory child environment is absent"))?;
        if leaving {
            *color = 2;
            continue;
        }
        if *color == 2 {
            continue;
        }
        if *color == 1 {
            return Err(bad("owning factory capture graph contains a cycle"));
        }
        *color = 1;
        let definition = &module.closures[id.0 as usize];
        let entry = module
            .functions
            .get(definition.function.0 as usize)
            .ok_or_else(|| bad("owning factory capture entry is absent"))?;
        budget.type_work(
            &definition.result,
            false,
            "owning factory entry result identity",
        )?;
        budget.spend(
            definition.parameters.len().saturating_add(1),
            "owning factory entry signature identity",
        )?;
        if definition.id != id
            || entry.environment != Some(id)
            || entry.foreign.is_some()
            || entry.return_type != definition.result
            || entry.parameters.len() != definition.parameters.len()
            || definition.kind == CallableKind::FunctionPointer
        {
            return Err(bad(
                "owning factory environment differs from its actual entry identity",
            ));
        }
        for (parameter, expected) in entry.parameters.iter().zip(&definition.parameters) {
            budget.type_work(expected, false, "owning factory entry parameter identity")?;
            if entry.locals[parameter.0 as usize].ty != *expected {
                return Err(bad(
                    "owning factory environment differs from its actual entry signature",
                ));
            }
        }
        budget.spend(
            definition.captures.len().saturating_add(1),
            "owning factory capture graph edges",
        )?;
        pending.push((id, true));
        for capture in &definition.captures {
            if let Type::Closure(child) = capture {
                pending.push((*child, false));
            } else {
                ordinary_capture(module, capture, budget)?;
            }
        }
    }
    Ok(())
}
