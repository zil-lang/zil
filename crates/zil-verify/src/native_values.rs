//! Independent admission of runtime-only foreign values. No raw pointer or
//! front-end provenance claim can manufacture a static string or callback.
use super::*;

pub(super) fn target(target: &TargetProfile, span: Option<Span>) -> Check<()> {
    if !target.is_supported_x64_llvm() {
        return Err(unsupported(
            "static foreign values and C records require the supported x64 Linux GNU or Win64 MSVC target tuple",
            span,
        ));
    }
    Ok(())
}

pub(super) fn callback_parameter(ty: &Type) -> bool {
    matches!(ty, Type::I32 | Type::I64 | Type::ForeignHandle)
}

pub(super) fn callback_signature(
    parameters: &[Type],
    result: &Type,
    span: Option<Span>,
) -> Check<()> {
    if !parameters.iter().all(callback_parameter)
        || !(result == &Type::Unit || callback_parameter(result))
    {
        return Err(reject(
            "E-VERIFY-FOREIGN-CALLBACK",
            span,
            "C callback parameters require i32/i64/handle and its result additionally allows unit",
        ));
    }
    Ok(())
}

pub(super) fn constant(
    constant: &Constant,
    target_profile: &TargetProfile,
    span: Option<Span>,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    match constant {
        Constant::WideString(units) => {
            if units.len() > limits.max_nodes {
                return Err(limit(
                    "static wide string unit count exceeds the raw/wire bound",
                ));
            }
            // Charge before walking, cloning or fingerprinting this raw vector.
            budget.spend(units.len())?;
            target(target_profile, span)?;
            let valid = units.last() == Some(&0)
                && !units[..units.len() - 1].contains(&0)
                && char::decode_utf16(units[..units.len() - 1].iter().copied())
                    .all(|unit| unit.is_ok());
            if !valid {
                return Err(reject(
                    "E-VERIFY-FOREIGN-STRING",
                    span,
                    "a static wide string requires valid UTF16 and exactly one trailing NUL",
                ));
            }
        }
        Constant::NullHandle | Constant::NullWideString => target(target_profile, span)?,
        _ => {}
    }
    Ok(())
}

pub(super) fn function_ref(
    module: &TypedModule,
    id: FunctionId,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<Type> {
    target(&module.target, span)?;
    let bad = |message| reject("E-VERIFY-FOREIGN-CALLBACK", span, message);
    let function = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| bad("C callback target is absent"))?;
    budget.spend(module.closures.len())?;
    if function.foreign.is_some()
        || function.environment.is_some()
        || function.blocks.is_empty()
        || function.callable_contract.is_some()
        || function.callable_return_contract.is_some()
        || module.closures.iter().any(|closure| closure.function == id)
        || resources::is_teardown(module, id, budget)?
    {
        return Err(bad(
            "C callback requires a defined ordinary static function without environment, teardown or callable contracts",
        ));
    }
    budget.spend(function.parameters.len().saturating_add(1))?;
    let mut parameters = Vec::with_capacity(function.parameters.len());
    for local in &function.parameters {
        let ty = local_type(function, *local, span)?;
        if !callback_parameter(ty) {
            return Err(bad("C callback parameter is outside its scalar profile"));
        }
        parameters.push(ty.clone());
    }
    callback_signature(&parameters, &function.return_type, span)?;
    Ok(Type::ForeignCallback {
        parameters,
        result: Box::new(function.return_type.clone()),
    })
}
