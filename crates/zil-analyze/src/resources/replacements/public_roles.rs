//! Public invocation presence selects actual callback bodies and captures.
//! The helper's private replacement rows never cross its public boundary.
use super::*;

pub(super) fn helper_effects(effect: &Effect, b: &mut Budget) -> CompileResult<()> {
    fee(
        b,
        effect
            .replacements
            .len()
            .saturating_add(effect.tagged_replacements.len())
            .saturating_add(effect.invalidated_stores.len()),
    )?;
    if effect
        .replacements
        .keys()
        .chain(effect.tagged_replacements.keys())
        .any(|target| {
            !matches!(
                target.root,
                origins::Root::Parameter(_) | origins::Root::ParameterCapture(..)
            )
        })
        || !effect.invalidated_stores.is_empty()
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "public callable helper has an unpublished replacement or Store effect",
            None,
        )]);
    }
    Ok(())
}

pub(in crate::resources) fn invoked(
    module: &TypedModule,
    f: &TypedFunction,
    contract: &CallableParameterContract,
    arguments: &[ValueId],
    b: &mut Budget,
) -> CompileResult<Vec<(usize, ValueId)>> {
    fee(b, contract.roles.len())?;
    let mut selected = Vec::new();
    for role in &contract.roles {
        let mut invoked = false;
        match &contract.traces {
            CallableTraces::Finite(paths) => {
                fee(b, paths.len())?;
                for path in paths {
                    fee(b, path.calls.len())?;
                    invoked |= path
                        .calls
                        .iter()
                        .any(|call| call.parameter == role.parameter);
                }
            }
            CallableTraces::Flow(flow) => {
                fee(b, flow.nodes.len())?;
                for node in &flow.nodes {
                    fee(b, node.calls.len())?;
                    invoked |= node
                        .calls
                        .iter()
                        .any(|call| call.parameter == role.parameter);
                }
            }
        }
        if !invoked {
            continue;
        }
        let value = *arguments
            .get(role.parameter as usize)
            .ok_or_else(|| error("public replacement role actual input absent"))?;
        let ty = &f.values[value.0 as usize];
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        if ty != &Type::Closure(role.closure) {
            return Err(error(
                "public replacement role differs from actual environment",
            ));
        }
        let environment = module
            .closures
            .get(role.closure.0 as usize)
            .ok_or_else(|| error("public replacement role environment absent"))?;
        fee(b, 1)?;
        selected.push((environment.function.0 as usize, value));
    }
    Ok(selected)
}

pub(super) fn capture_effects(effect: &Effect, b: &mut Budget) -> CompileResult<()> {
    fee(
        b,
        effect
            .replacements
            .len()
            .saturating_add(effect.tagged_replacements.len()),
    )?;
    for (target, birth) in &effect.replacements {
        fee(b, birth.len())?;
        if !matches!(target.root, origins::Root::Capture(_))
            || birth.iter().any(|term| matches!(term, Birth::Argument(_)))
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "callback replacement through an explicit callback argument needs its actual argument mapping",
                None,
            )]);
        }
    }
    for target in effect.tagged_replacements.keys() {
        if !matches!(target.root, origins::Root::Capture(_)) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "tagged callback replacement needs its actual captured destination",
                None,
            )]);
        }
    }
    fee(b, effect.invalidated_stores.len())?;
    if effect
        .invalidated_stores
        .iter()
        .any(|source| matches!(source, StoreInvalidation::Input(_)))
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "callback Store replacement needs its actual callback input mapping",
            None,
        )]);
    }
    Ok(())
}
