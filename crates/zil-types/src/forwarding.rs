//! A Parameter return is a relation to a real input position, independent of
//! the concrete environment's structural Copy property and callable kind.
use super::*;

pub(super) fn bound(function: &CoreFunction) -> Option<&CoreCallableParameter> {
    let Type::Named(name) = &function.return_type else {
        return None;
    };
    function
        .callable_parameters
        .iter()
        .find(|bound| &bound.generic_name == name)
}
pub(super) fn signature(signature: &Signature) -> Option<&CoreCallableParameter> {
    let Type::Named(name) = &signature.result else {
        return None;
    };
    signature
        .callable
        .iter()
        .find(|bound| &bound.generic_name == name)
}
pub(super) fn validate(
    function: &CoreFunction,
    prepay: &mut impl FnMut(&Type, Span) -> R<()>,
) -> R<()> {
    let bound = bound(function).expect("forward parameter");
    let reject = || {
        unsupported(
            function.span,
            "callable forwarding requires one direct owned self-contained F, a nonlending existing signature and closed ordinary data companions",
        )
    };
    let parameter = function
        .parameters
        .get(bound.index as usize)
        .ok_or_else(reject)?;
    if function.callable_parameters.len() != 1
        || !function.state_parameters.is_empty()
        || !function.type_parameters.is_empty()
        || function.callable_return.is_some()
        || function.foreign.is_some()
        || function.receiver.is_some()
        || function.result_mode != PassingMode::Infer
        || bound.generic_index != 0
        || bound.generic_name.is_empty()
        || bound.kind == CallableKind::FunctionPointer
        || helpers::parameter_mode(&parameter.ty, &bound.generic_name) != Some(None)
        || !matches!(parameter.mode, PassingMode::Infer | PassingMode::Move)
    {
        return Err(reject());
    }
    for ty in bound
        .parameters
        .iter()
        .chain(std::iter::once(&bound.result))
    {
        prepay(ty, function.span)?;
    }
    if lending::supported_result(&bound.result)
        || !closures::value_signature(&bound.parameters, &bound.result)
    {
        return Err(reject());
    }
    for (index, parameter) in function.parameters.iter().enumerate() {
        if index == bound.index as usize {
            continue;
        }
        prepay(&parameter.ty, parameter.span)?;
        if !closures::owned_signature_value(&parameter.ty)
            || !matches!(
                parameter.mode,
                PassingMode::Infer | PassingMode::Copy | PassingMode::Move
            )
            || (parameter.mode == PassingMode::Copy && !parameter.ty.is_copy_value())
        {
            return Err(reject());
        }
    }
    Ok(())
}

impl Builder<'_, '_> {
    /// Lazily reconstruct preceding declared-mode moves only on a new forward
    /// call. Unaffected legacy helpers retain their original costs.
    pub(super) fn forward_modes(&mut self, span: Span) -> R<()> {
        if self.factory_modes.is_some() {
            return Ok(());
        }
        self.closures.snapshot(self.helper_bounds.len(), span)?;
        let mut modes = owning_factory::Modes::default();
        for bound in &self.helper_bounds {
            modes
                .locals
                .insert(self.function.parameters[bound.index as usize], bound.kind);
        }
        for block in &self.function.blocks {
            self.closures.snapshot(block.instructions.len(), span)?;
            for instruction in &block.instructions {
                modes.record(
                    &instruction.kind,
                    instruction.result,
                    self.closures,
                    instruction.span,
                )?;
            }
        }
        self.factory_modes = Some(modes);
        Ok(())
    }
}
