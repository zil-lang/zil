//! Ordinary owned callback data is separate from environment admission. These
//! checks reconstruct actual signatures; no Types or Verify fact is consumed.
use super::*;

pub(super) fn data(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<()> {
    match ty {
        Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. } => {
            resources::sums::admit(module, ty, budget).map(|_| ())
        }
        _ => {
            budget.type_work(ty, true, "owned callback finite Copy companion")?;
            if ty.is_copy_value() {
                Ok(())
            } else {
                Err(bad(
                    "owned callback data requires ordinary Product/Enum/ErrorUnion or finite Copy",
                ))
            }
        }
    }
}

pub(super) fn signature(
    module: &TypedModule,
    definition: &ClosureDefinition,
    budget: &mut Budget,
) -> CompileResult<()> {
    let entry = module
        .functions
        .get(definition.function.0 as usize)
        .ok_or_else(|| bad("owned callback entry is absent"))?;
    budget.spend(
        definition.parameters.len().saturating_add(1),
        "owned callback actual entry",
    )?;
    budget.type_work(
        &definition.result,
        false,
        "owned callback entry result comparison",
    )?;
    if module.closures.get(definition.id.0 as usize) != Some(definition)
        || definition.kind == CallableKind::FunctionPointer
        || !definition.lending_captures.is_empty()
        || entry.environment != Some(definition.id)
        || entry.foreign.is_some()
        || entry.callable_return_contract.is_some()
        || entry.callable_contract.is_some()
        || entry.parameters.len() != definition.parameters.len()
        || entry.return_type != definition.result
        || entry.result_mode != PassingMode::Infer
    {
        return Err(bad(
            "owned callback definition differs from its actual ordinary entry",
        ));
    }
    for (local, ty) in entry.parameters.iter().zip(&definition.parameters) {
        budget.type_work(ty, false, "owned callback entry parameter comparison")?;
        if entry.locals.get(local.0 as usize).map(|l| &l.ty) != Some(ty) {
            return Err(bad("owned callback actual parameter type differs"));
        }
    }
    Ok(())
}

pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if budget.owned_callbacks.is_empty() && budget.owned_callback_helpers.is_empty() {
        return Ok(());
    }
    budget.spend(
        module.functions.len(),
        "owned callback window function inventory",
    )?;
    let mut windows = vec![false; module.functions.len()];
    let mut edges = Vec::new();
    budget.spend(
        budget
            .owned_callbacks
            .len()
            .saturating_add(budget.owned_callback_helpers.len()),
        "owned callback window roots",
    )?;
    for id in &budget.owned_callbacks {
        windows[module.closures[id.0 as usize].function.0 as usize] = true;
    }
    for id in &budget.owned_callback_helpers {
        windows[id.0 as usize] = true;
    }
    budget.spend(module.functions.len(), "owned callback call functions")?;
    for f in &module.functions {
        budget.spend(f.blocks.len(), "owned callback call blocks")?;
        for b in &f.blocks {
            budget.spend(b.instructions.len(), "owned callback call instructions")?;
            for i in &b.instructions {
                if let InstructionKind::Call {
                    function,
                    arguments,
                } = &i.kind
                {
                    if function.0 as usize >= windows.len() {
                        return Err(bad("owned callback call graph target is absent"));
                    }
                    budget.spend(1, "owned callback ordinary call edge")?;
                    edges.push((f.id.0 as usize, function.0 as usize));
                    budget.lookup(
                        budget.owned_callback_helpers.len(),
                        "owned helper actual signature lookup",
                    )?;
                    if budget.owned_callback_helpers.contains(function) {
                        let target = &module.functions[function.0 as usize];
                        budget.spend(arguments.len(), "owned helper actual arguments")?;
                        budget.type_work(
                            &target.return_type,
                            false,
                            "owned helper actual result",
                        )?;
                        if arguments.len() != target.parameters.len()
                            || !(i.result.and_then(|v| f.values.get(v.0 as usize))
                                == Some(&target.return_type)
                                || (i.result.is_none() && target.return_type == Type::Unit))
                        {
                            return Err(bad("owned callback helper call signature differs"));
                        }
                        for (value, local) in arguments.iter().zip(&target.parameters) {
                            let ty = &target.locals[local.0 as usize].ty;
                            budget.type_work(ty, false, "owned helper actual parameter")?;
                            if f.values.get(value.0 as usize) != Some(ty) {
                                return Err(bad(
                                    "owned callback helper actual argument type differs",
                                ));
                            }
                        }
                    }
                    continue;
                }
                let (ty, arguments) = match &i.kind {
                    InstructionKind::CallClosureShared { callee, arguments }
                    | InstructionKind::CallClosureMut { callee, arguments } => {
                        (f.locals.get(callee.0 as usize).map(|l| &l.ty), arguments)
                    }
                    InstructionKind::CallClosure { callee, arguments }
                    | InstructionKind::CallClosureOnce { callee, arguments }
                    | InstructionKind::CallClosureBorrowed {
                        callee, arguments, ..
                    } => (f.values.get(callee.0 as usize), arguments),
                    _ => continue,
                };
                let ty = match ty {
                    Some(Type::Address { inner, .. }) => inner.as_ref(),
                    Some(ty) => ty,
                    None => return Err(bad("callback callee is absent")),
                };
                let Type::Closure(id) = ty else {
                    return Err(bad("callback callee has no environment"));
                };
                let c = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("callback environment is absent"))?;
                budget.spend(1, "owned callback concrete call edge")?;
                edges.push((f.id.0 as usize, c.function.0 as usize));
                budget.lookup(
                    budget.owned_callbacks.len(),
                    "owned callback signature lookup",
                )?;
                if !budget.owned_callbacks.contains(id) {
                    continue;
                }
                budget.spend(arguments.len(), "owned callback actual arguments")?;
                budget.type_work(&c.result, false, "owned callback actual result")?;
                if arguments.len() != c.parameters.len()
                    || !(i.result.and_then(|v| f.values.get(v.0 as usize)) == Some(&c.result)
                        || (i.result.is_none() && c.result == Type::Unit))
                {
                    return Err(bad("owned callback call signature differs"));
                }
                for (value, ty) in arguments.iter().zip(&c.parameters) {
                    budget.type_work(ty, false, "owned callback actual parameter")?;
                    if f.values.get(value.0 as usize) != Some(ty) {
                        return Err(bad("owned callback actual argument type differs"));
                    }
                }
            }
        }
    }
    loop {
        budget.spend(edges.len(), "owned callback connected window functions")?;
        let mut changed = false;
        for &(caller, callee) in &edges {
            if windows[caller] || windows[callee] {
                changed |= !windows[caller] || !windows[callee];
                windows[caller] = true;
                windows[callee] = true;
            }
        }
        if !changed {
            break;
        }
    }
    budget.owned_callback_window_functions = windows;
    Ok(())
}
