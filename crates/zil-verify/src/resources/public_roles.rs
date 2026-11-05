//! This kernel selects only publicly declared possible callback invocations.
//! Actual callback bodies and actual argument captures supply their effects.
use super::*;

pub(crate) fn mutable_capture(
    module: &TypedModule,
    f: &TypedFunction,
    parameter: u32,
    capture: u32,
    b: &mut Budget,
) -> Check<bool> {
    let Some(contract) = &f.callable_contract else {
        return Ok(false);
    };
    let local = *f
        .parameters
        .get(parameter as usize)
        .ok_or_else(|| bad(Span::default(), "public role parameter absent"))?;
    let ty = &f.locals[local.0 as usize].ty;
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else {
        return Ok(false);
    };
    let definition = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| bad(Span::default(), "public role actual environment absent"))?;
    if !matches!(
        definition.captures.get(capture as usize),
        Some(Type::Address { mutable: true, .. })
    ) {
        return Ok(false);
    }
    b.spend(contract.roles.len())?;
    for role in &contract.roles {
        b.spend(role.writes.len())?;
        if role.parameter == parameter
            && role.closure == *id
            && role.kind != CallableKind::Fn
            && role.writes.contains(&capture)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn invoked(
    module: &TypedModule,
    f: &TypedFunction,
    contract: &CallableParameterContract,
    arguments: &[ValueId],
    b: &mut Budget,
) -> Check<Vec<(usize, ValueId)>> {
    b.spend(contract.roles.len())?;
    let mut selected = Vec::new();
    for role in &contract.roles {
        let mut may_call = false;
        match &contract.traces {
            CallableTraces::Finite(paths) => {
                b.spend(paths.len())?;
                for path in paths {
                    b.spend(path.calls.len())?;
                    may_call |= path
                        .calls
                        .iter()
                        .any(|call| call.parameter == role.parameter);
                }
            }
            CallableTraces::Flow(flow) => {
                b.spend(flow.nodes.len())?;
                for node in &flow.nodes {
                    b.spend(node.calls.len())?;
                    may_call |= node
                        .calls
                        .iter()
                        .any(|call| call.parameter == role.parameter);
                }
            }
        }
        if !may_call {
            continue;
        }
        let value = *arguments
            .get(role.parameter as usize)
            .ok_or_else(|| bad(Span::default(), "public replacement role input is absent"))?;
        let ty = &f.values[value.0 as usize];
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        if ty != &Type::Closure(role.closure) {
            return Err(bad(
                Span::default(),
                "public replacement role actual environment differs",
            ));
        }
        let definition = module
            .closures
            .get(role.closure.0 as usize)
            .ok_or_else(|| bad(Span::default(), "public replacement environment absent"))?;
        b.spend(1)?;
        selected.push((definition.function.0 as usize, value));
    }
    Ok(selected)
}
