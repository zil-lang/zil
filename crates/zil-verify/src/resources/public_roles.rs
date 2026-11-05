//! This kernel selects only publicly declared possible callback invocations.
//! Actual callback bodies and actual argument captures supply their effects.
use super::*;

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
