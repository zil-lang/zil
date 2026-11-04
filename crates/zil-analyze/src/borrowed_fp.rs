//! Producer checks for invocation-local code pointer relations. Actual return
//! roots are checked only after the whole-module provenance fixed point.
use super::*;

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    life: &StorageLifeContract,
) -> CompileResult<()> {
    if life.fixed_binder_count != 0 {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "borrowed pointer or role Fixed classes require genuine enclosing semantic context",
            None,
        )]);
    }
    super::signature_life::relation(parameters, result, life)
}
pub(super) fn allowed(ty: &Type, budget: &mut Budget) -> CompileResult<Vec<usize>> {
    let Type::BorrowedFunctionPointer {
        parameters,
        result,
        life,
    } = ty
    else {
        return Ok(Vec::new());
    };
    crate::signature_life::charge(life, budget)?;
    let mutable = matches!(result.as_ref(), Type::Address { mutable: true, .. });
    budget.spend(
        parameters
            .len()
            .saturating_mul(life.bindings.len().saturating_add(1)),
        "borrowed pointer positional upper lookup",
    )?;
    let upper = super::signature_life::outer(life, StorageBase::Result);
    Ok(parameters
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            (upper.is_some()
                && super::signature_life::outer(life, StorageBase::Parameter(i as u32)) == upper
                && matches!(p,Type::Address{mutable:m,..}if !mutable||*m))
            .then_some(i)
        })
        .collect())
}

pub(super) fn targets(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    facts: &[Vec<ValueHold>],
    budget: &mut Budget,
) -> CompileResult<()> {
    if !budget.borrowed_function_pointers {
        return Ok(());
    }
    if budget.invocation_callbacks {
        callbacks(module, graphs, facts, budget)?;
    }
    budget.spend(
        module
            .functions
            .len()
            .saturating_add(module.resources.len()),
        "borrowed pointer target inventory",
    )?;
    for f in &module.functions {
        budget.spend(f.blocks.len(), "borrowed pointer source blocks")?;
        for block in &f.blocks {
            budget.spend(block.instructions.len(), "borrowed pointer target sites")?;
            for instruction in &block.instructions {
                let InstructionKind::FunctionRef { function } = instruction.kind else {
                    continue;
                };
                let Some(pointer) = instruction
                    .result
                    .and_then(|id| f.values.get(id.0 as usize))
                else {
                    continue;
                };
                let Type::BorrowedFunctionPointer {
                    parameters, result, ..
                } = pointer
                else {
                    continue;
                };
                let target = module
                    .functions
                    .get(function.0 as usize)
                    .ok_or_else(|| bad("borrowed pointer target absent"))?;
                budget.spend(
                    target
                        .parameters
                        .len()
                        .saturating_add(module.resources.len()),
                    "borrowed pointer actual signature",
                )?;
                budget.type_work(result, true, "borrowed pointer exact result mode")?;
                if target.foreign.is_some()
                    || target.environment.is_some()
                    || module
                        .resources
                        .iter()
                        .any(|r| r.teardown == Some(function))
                    || target.return_type != **result
                    || target.parameters.len() != parameters.len()
                {
                    return Err(bad(
                        "borrowed pointer target has a different complete signature or requires an environment",
                    ));
                }
                for (local, parameter) in target.parameters.iter().zip(parameters) {
                    budget.type_work(parameter, true, "borrowed pointer exact parameter mode")?;
                    if target.locals.get(local.0 as usize).map(|l| &l.ty) != Some(parameter) {
                        return Err(bad("borrowed pointer actual parameter mode differs"));
                    }
                }
                if !matches!(result.as_ref(), Type::Address { .. }) {
                    continue;
                }
                let allowed = allowed(pointer, budget)?;
                budget.spend(target.blocks.len(), "borrowed pointer actual return blocks")?;
                for (block, live) in target.blocks.iter().zip(&graphs[function.0 as usize].1) {
                    if *live && let Terminator::Return(Some(value)) = block.terminator {
                        let roots = &facts[function.0 as usize][value.0 as usize].roots;
                        budget.product(
                            roots.len(),
                            allowed.len().saturating_add(1),
                            1,
                            "borrowed pointer actual return inclusion",
                        )?;
                        if roots.is_empty() || roots.iter().any(|r| !matches!(r, HoldRoot::Parameter(i) if allowed.contains(&(*i as usize)))) { return Err(bad("actual target returns outside the pointer relation")); }
                    }
                }
            }
        }
    }
    Ok(())
}

/// A concrete call returns only invocation input storage in this profile.
/// This classifier triggers a full actual-return proof; it grants no origins.
pub(super) fn invocation(parameters: &[Type], result: &Type) -> bool {
    matches!(result, Type::Address { .. })
        && parameters.iter().any(|p| matches!(p, Type::Address { .. }))
}

pub(super) fn invocation_call<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    kind: &'a InstructionKind,
) -> Option<(&'a ClosureDefinition, &'a [ValueId])> {
    let (ty, args) = match kind {
        InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallClosureBorrowed {
            callee, arguments, ..
        } => (f.values.get(callee.0 as usize)?, arguments),
        InstructionKind::CallClosureShared { callee, arguments }
        | InstructionKind::CallClosureMut { callee, arguments } => {
            (&f.locals.get(callee.0 as usize)?.ty, arguments)
        }
        _ => return None,
    };
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else { return None };
    let d = module.closures.get(id.0 as usize)?;
    (d.lending_captures.is_empty() && matches!(d.result, Type::Address { .. })).then_some((d, args))
}

pub(super) fn signature_work(
    parameters: &[Type],
    result: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    for ty in parameters.iter().chain(std::iter::once(result)) {
        budget.type_work(ty, true, "callback relation and exact actual signature")?;
    }
    Ok(())
}

fn callback_returns(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    facts: &[Vec<ValueHold>],
    closure: &ClosureDefinition,
    life: Option<&StorageLifeContract>,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(
        closure.parameters.len().saturating_mul(3).saturating_add(1),
        "callback invocation return source set",
    )?;
    signature_work(&closure.parameters, &closure.result, budget)?;
    if let Some(life) = life {
        crate::signature_life::charge(life, budget)?;
        relation(&closure.parameters, &closure.result, life)?;
    }
    if !matches!(closure.result, Type::Address { .. }) {
        return Ok(());
    }
    let mutable = matches!(closure.result, Type::Address { mutable: true, .. });
    let allowed: Vec<_> = closure
        .parameters
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            (matches!(p,Type::Address{mutable:m,..} if !mutable||*m)
                && life.is_none_or(|life| {
                    super::signature_life::outer(life, StorageBase::Parameter(i as u32))
                        == super::signature_life::outer(life, StorageBase::Result)
                }))
            .then_some(i as u32)
        })
        .collect();
    let f = module
        .functions
        .get(closure.function.0 as usize)
        .ok_or_else(|| bad("callback entry absent"))?;
    if f.environment != Some(closure.id)
        || f.foreign.is_some()
        || f.parameters.len() != closure.parameters.len()
        || f.return_type != closure.result
    {
        return Err(bad("invocation callback actual complete signature differs"));
    }
    for (p, ty) in f.parameters.iter().zip(&closure.parameters) {
        if f.locals.get(p.0 as usize).map(|l| &l.ty) != Some(ty) {
            return Err(bad("invocation callback actual parameter differs"));
        }
    }
    budget.spend(f.blocks.len(), "callback actual reachable return blocks")?;
    for (block, live) in f.blocks.iter().zip(&graphs[f.id.0 as usize].1) {
        if *live && let Terminator::Return(Some(value)) = block.terminator {
            let roots = &facts[f.id.0 as usize][value.0 as usize].roots;
            budget.spend(
                roots.len().saturating_mul(allowed.len().saturating_add(1)),
                "callback actual invocation return inclusion",
            )?;
            if roots.is_empty()
                || roots
                    .iter()
                    .any(|root| !matches!(root,HoldRoot::Parameter(i) if allowed.contains(i)))
            {
                return Err(bad(
                    "callback returns outside this invocation's declared input storage",
                ));
            }
        }
    }
    Ok(())
}
fn callbacks(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    facts: &[Vec<ValueHold>],
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(
        module.closures.len().saturating_add(module.functions.len()),
        "actual callback invocation inventory",
    )?;
    for closure in &module.closures {
        budget.spend(
            closure.parameters.len(),
            "actual invocation callback signature scan",
        )?;
        if invocation(&closure.parameters, &closure.result) && closure.lending_captures.is_empty() {
            callback_returns(module, graphs, facts, closure, None, budget)?;
        }
    }
    for f in &module.functions {
        if let Some(contract) = &f.callable_contract {
            budget.spend(contract.roles.len(), "higher-rank callback role scan")?;
            for role in &contract.roles {
                if let Some(life) = &role.life {
                    let closure = module
                        .closures
                        .get(role.closure.0 as usize)
                        .ok_or_else(|| bad("higher-rank callback definition absent"))?;
                    callback_returns(module, graphs, facts, closure, Some(life), budget)?;
                }
            }
        }
    }
    Ok(())
}
