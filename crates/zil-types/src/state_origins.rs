//! Private source declarations for the finite two-binding owning State profile.
//! A declaration restricts a use; it never supplies a runtime value or loan.
use super::*;
use std::borrow::Cow;

pub(super) fn bounds(function: &CoreFunction) -> Cow<'_, [CoreCallableParameter]> {
    let Some(state) = function.state_parameters.first() else {
        return Cow::Borrowed(&function.callable_parameters);
    };
    Cow::Owned(
        state
            .bindings
            .iter()
            .map(|binding| CoreCallableParameter {
                index: state.index,
                generic_index: binding.generic_index,
                generic_name: binding.generic_name.clone(),
                kind: binding.kind,
                parameters: binding.parameters.clone(),
                result: binding.result.clone(),
            })
            .collect(),
    )
}

pub(super) fn validate_header(function: &CoreFunction) -> R<()> {
    if function.state_parameters.is_empty() {
        return Ok(());
    }
    let reject = || {
        error(
            "E-STATE-PARAMETERS",
            function.span,
            "State declarations require one owned parameter with two distinct ordered callable bindings",
        )
    };
    let [state] = function.state_parameters.as_slice() else {
        return Err(reject());
    };
    if !function.callable_parameters.is_empty()
        || !function.type_parameters.is_empty()
        || function.callable_return.is_some()
        || function.foreign.is_some()
        || function.receiver.is_some()
        || state.bindings.len() != 2
        || state.index as usize >= function.parameters.len()
        || function.parameters[state.index as usize].mode != PassingMode::Infer
    {
        return Err(reject());
    }
    for (index, binding) in state.bindings.iter().enumerate() {
        if binding.generic_index != index as u32
            || binding.generic_name.is_empty()
            || binding.kind == CallableKind::FunctionPointer
            || !binding.result.is_copy_value()
            || !closures::supported_signature(&binding.parameters, &binding.result)
            || binding.fields.len() != 1
        {
            return Err(reject());
        }
    }
    if state.bindings[0].generic_name == state.bindings[1].generic_name
        || state.bindings[0].fields == state.bindings[1].fields
    {
        return Err(reject());
    }
    Ok(())
}

/// Called only after aggregate resolution and the input's bounded metadata walk.
pub(super) fn validate_shape(function: &CoreFunction) -> R<()> {
    validate_header(function)?;
    let Some(state) = function.state_parameters.first() else {
        return Ok(());
    };
    let reject = || {
        error(
            "E-STATE-PARAMETERS",
            function.span,
            "State selectors must cover exactly the two direct fields of their declared binders",
        )
    };
    let Type::Package { fields, .. } = &function.parameters[state.index as usize].ty else {
        return Err(reject());
    };
    let mut covered = 0;
    for (index, (_, ty)) in fields.iter().enumerate() {
        if let Some(binding) = state
            .bindings
            .iter()
            .find(|binding| matches!(ty, Type::Named(name) if name == &binding.generic_name))
        {
            if binding.fields.as_slice() != [index as u32] {
                return Err(reject());
            }
            covered += 1;
        } else if !(ty.is_copy_value()
            || matches!(ty, Type::Managed(_) if resources::supported(ty)))
        {
            return Err(reject());
        }
    }
    if covered != 2 {
        return Err(reject());
    }
    Ok(())
}
