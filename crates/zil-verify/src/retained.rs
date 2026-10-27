//! Opaque callable results are untrusted claims until the concrete environment,
//! its public boundary and every ordinary return have been checked here.
use super::*;

fn bad(f: &TypedFunction, message: impl Into<String>) -> Diagnostic {
    reject("E-VERIFY-CALLABLE-CONTRACT", Some(f.span), message)
}

/// Runs before fingerprinting. Raw callers cannot hide huge index vectors in a
/// forged contract and force unbounded hashing or equality work.
pub(super) fn check_claim(
    module: &TypedModule,
    f: &TypedFunction,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    if let Some(c) = &f.callable_return_contract {
        for indices in [&c.hold, &c.call, &c.writes, &c.cleanup] {
            if indices.len() > limits.max_locals {
                return Err(limit("callable result index vector exceeds local bound"));
            }
            budget.spend(indices.len().saturating_add(1))?;
        }
    }
    let Type::Closure(id) = f.return_type else {
        return if f.callable_return_contract.is_none() {
            Ok(())
        } else {
            Err(bad(
                f,
                "a non-closure result cannot claim a callable return contract",
            ))
        };
    };
    let c = f.callable_return_contract.as_ref().ok_or_else(|| {
        bad(
            f,
            "a callable result requires its independently checkable return contract",
        )
    })?;
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.callable_contract.is_some()
        || f.result_mode != PassingMode::Infer
    {
        return Err(bad(
            f,
            "a callable factory must be an ordinary function with Value result mode",
        ));
    }
    let d = closures::definition(module, id, Some(f.span))?;
    if c.closure != id || c.kind != d.kind {
        return Err(bad(
            f,
            "returned environment identity or exact callable kind disagrees",
        ));
    }
    if !c.hold.is_empty() || !c.call.is_empty() || !c.writes.is_empty() {
        return Err(bad(
            f,
            "self-contained factory results cannot advertise external dependencies",
        ));
    }
    // The empty external envelope is valid only when a caller cannot lend an
    // external address/description to the factory. Inspect raw parameters here;
    // a source producer's restriction grants no authority to a forged module.
    budget.spend(f.parameters.len())?;
    for parameter in &f.parameters {
        let local = f
            .locals
            .get(parameter.0 as usize)
            .ok_or_else(|| bad(f, "callable factory parameter is outside its local table"))?;
        if matches!(local.ty, Type::Package { .. }) {
            packages::check_type_for(module, &local.ty, Some(local.span), budget)?;
        } else if !(local.ty.is_copy_value() || matches!(local.ty, Type::Managed(_))) {
            return Err(unsupported(
                "self-contained callable factories require Copy, Managed or finite Package inputs",
                Some(local.span),
            ));
        }
    }
    let mut cleanup = Vec::new();
    for (index, ty) in d.captures.iter().enumerate() {
        budget.spend(1)?;
        if matches!(ty, Type::Package { .. }) {
            packages::check_type_for(module, ty, Some(f.span), budget)?;
            cleanup.push(index as u32);
        } else if matches!(ty, Type::Managed(_)) {
            cleanup.push(index as u32);
        } else if let Type::Closure(id) = ty {
            closures::self_contained(module, *id, budget)?;
            cleanup.push(index as u32);
        } else if !ty.is_copy_value() {
            return Err(unsupported(
                "returned callable captures must be self-contained finite Copy, Managed or Package values",
                Some(f.span),
            ));
        }
    }
    if c.cleanup != cleanup {
        return Err(bad(
            f,
            "callable result cleanup must name every owned capture exactly in field order",
        ));
    }
    Ok(())
}

/// General SSA typing and responsibility checking still establish exact value
/// types and one transfer at Return. This only checks the finite factory scope;
/// it does not replace those independent checks with a producer summary.
pub(super) fn check_body(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<()> {
    let Some(contract) = &f.callable_return_contract else {
        return Ok(());
    };
    check_dependencies(module, f, budget)?;
    let mut creations = 0usize;
    let mut returns = 0usize;
    for (b, block) in f.blocks.iter().enumerate() {
        budget.spend(1)?;
        if !graph.reachable[b] {
            continue;
        }
        for instruction in &block.instructions {
            budget.spend(1)?;
            match instruction.kind {
                InstructionKind::MakeClosure { closure, .. } if closure == contract.closure => {
                    creations += 1;
                }
                InstructionKind::Call { function, .. } => {
                    let target = module.functions.get(function.0 as usize).ok_or_else(|| {
                        bad(f, "factory call target is outside the function table")
                    })?;
                    if target
                        .callable_return_contract
                        .as_ref()
                        .is_some_and(|c| c.closure == contract.closure)
                    {
                        creations += 1;
                    }
                }
                _ => {}
            }
        }
        if matches!(block.terminator, Terminator::Return(Some(_))) {
            returns += 1;
        }
    }
    if creations == 0 || returns == 0 {
        return Err(bad(
            f,
            "factory requires a concrete creation or grounded forwarding site and a reachable same-type return",
        ));
    }
    Ok(())
}

/// A raw producer cannot claim a finite factory identity by making factories
/// endorse each other. This independent graph walk does not consume a Types or
/// Analyze summary and does not recurse on the host stack.
fn check_dependencies(
    module: &TypedModule,
    root: &TypedFunction,
    budget: &mut Budget,
) -> Check<()> {
    let mut active = std::collections::BTreeSet::new();
    let mut complete = std::collections::BTreeSet::new();
    let mut pending = vec![(root.id, false)];
    while let Some((id, exiting)) = pending.pop() {
        budget.spend(1)?;
        if exiting {
            active.remove(&id);
            complete.insert(id);
            continue;
        }
        if complete.contains(&id) {
            continue;
        }
        if !active.insert(id) {
            return Err(bad(
                root,
                "recursive callable factory dependencies cannot establish a finite identity",
            ));
        }
        pending.push((id, true));
        let function = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| bad(root, "factory dependency is outside the function table"))?;
        let mut dependencies = std::collections::BTreeSet::new();
        for block in &function.blocks {
            budget.spend(1)?;
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                {
                    let target = module.functions.get(callee.0 as usize).ok_or_else(|| {
                        bad(root, "factory call target is outside the function table")
                    })?;
                    if target.callable_return_contract.is_some() {
                        dependencies.insert(callee);
                    }
                }
            }
        }
        for dependency in dependencies {
            pending.push((dependency, false));
        }
    }
    Ok(())
}
