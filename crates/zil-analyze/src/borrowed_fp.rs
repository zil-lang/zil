//! Producer checks for invocation-local code pointer relations. Actual return
//! roots are checked only after the whole-module provenance fixed point.
use super::*;

pub(super) fn work(life: &FunctionLifeContract) -> usize {
    (life.binder_count as usize)
        .saturating_add(life.parameter_lives.len().saturating_mul(2))
        .saturating_add(2)
}
pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    life: &FunctionLifeContract,
) -> CompileResult<()> {
    let invalid = || bad("invalid invocation-local borrowed function pointer relation");
    if life.binder_count == 0 || life.parameter_lives.len() != parameters.len() {
        return Err(invalid());
    }
    let mut next = 0;
    for (parameter, tag) in parameters.iter().zip(&life.parameter_lives) {
        match parameter {
            Type::Address { inner, .. } if inner.is_address_pointee() => {
                let Some(tag) = tag.filter(|tag| *tag < life.binder_count && *tag <= next) else {
                    return Err(invalid());
                };
                if tag == next {
                    next += 1;
                }
            }
            ty if ty.is_copy_value() && tag.is_none() => {}
            _ => return Err(invalid()),
        }
    }
    if next != life.binder_count {
        return Err(invalid());
    }
    match result {
        Type::Address { inner, mutable } if inner.is_address_pointee() => {
            let Some(tag) = life.result_life.filter(|tag| *tag < life.binder_count) else {
                return Err(invalid());
            };
            if !parameters.iter().zip(&life.parameter_lives).any(|(p, t)| {
                *t == Some(tag) && matches!(p, Type::Address { mutable: m, .. } if !mutable || *m)
            }) {
                return Err(invalid());
            }
        }
        ty if ty.is_copy_value() && life.result_life.is_none() => {}
        _ => return Err(invalid()),
    }
    Ok(())
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
    budget.spend(work(life), "borrowed pointer input substitution")?;
    let mutable = matches!(result.as_ref(), Type::Address { mutable: true, .. });
    Ok(parameters
        .iter()
        .zip(&life.parameter_lives)
        .enumerate()
        .filter_map(|(i, (p, t))| {
            (life.result_life.is_some()
                && *t == life.result_life
                && matches!(p, Type::Address { mutable: m, .. } if !mutable || *m))
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
