//! Independent closure proof boundary. Frontend/analyzer claims never
//! substitute for checked signatures, executable operands, or reconstructed Hold.
use super::*;

pub(super) fn definition(
    module: &TypedModule,
    id: ClosureId,
    span: Option<Span>,
) -> Check<&ClosureDefinition> {
    module.closures.get(id.0 as usize).ok_or_else(|| {
        reject(
            "E-VERIFY-CLOSURE",
            span,
            "closure identity is outside the definition table",
        )
    })
}

pub(super) fn check_definitions(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    if module.closures.len() > limits.max_functions
        || module.closures.len() > module.functions.len()
    {
        return Err(limit("closure count exceeds the function bound"));
    }
    budget.spend(module.functions.len())?;
    let mut entries = vec![None; module.functions.len()];
    for (index, closure) in module.closures.iter().enumerate() {
        budget.spend(1)?;
        if closure.id.0 as usize != index {
            return Err(reject(
                "E-VERIFY-CLOSURE",
                None,
                "ClosureId must equal its vector index",
            ));
        }
        if !matches!(
            closure.kind,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) {
            return Err(unsupported(
                "this slice verifies concrete Fn, FnMut and FnOnce environments",
                None,
            ));
        }
        if closure.captures.len() > limits.max_locals
            || closure.parameters.len() > limits.max_locals
        {
            return Err(limit(
                "closure field or parameter count exceeds the selected bound",
            ));
        }
        budget.spend(closure.captures.len())?;
        budget.spend(closure.parameters.len().saturating_add(1))?;
        // Capture fields include independently checked finite owning Packages.
        // Hold, view binding and authority are reconstructed from the raw CFG.
        for ty in &closure.captures {
            if errors::owned(ty) {
                return Err(unsupported(
                    "owning error-union captures are outside the finite closure profile",
                    None,
                ));
            }
            if byte_buffers::is_desc(ty) {
                return Err(unsupported(
                    "borrowed byte storage captures are outside the finite profile",
                    None,
                ));
            }
            if slices::is_window(ty) {
                return Err(unsupported(
                    "borrowed slice captures are outside the finite slice profile",
                    None,
                ));
            }
            check_function_signature_type(module, ty, None, budget)?;
        }
        if closure.kind == CallableKind::Fn
            && closure
                .captures
                .iter()
                .any(|t| matches!(t, Type::Address { mutable: true, .. }))
        {
            return Err(reject(
                "E-VERIFY-CLOSURE-CALL",
                None,
                "exclusive capture requires an exclusive environment entry",
            ));
        }
        for ty in closure
            .parameters
            .iter()
            .chain(std::iter::once(&closure.result))
        {
            check_type(ty, None)?;
        }
        let function = module
            .functions
            .get(closure.function.0 as usize)
            .ok_or_else(|| {
                reject(
                    "E-VERIFY-CLOSURE-ENTRY",
                    None,
                    "closure entry function is absent",
                )
            })?;
        let entry = &mut entries[closure.function.0 as usize];
        if entry.replace(closure.id).is_some() || function.environment != Some(closure.id) {
            return Err(reject(
                "E-VERIFY-CLOSURE-ENTRY",
                Some(function.span),
                "closure definition and entry environment must be one-to-one",
            ));
        }
        if function.parameters.len() != closure.parameters.len()
            || function.return_type != closure.result
        {
            return Err(reject(
                "E-VERIFY-CLOSURE-ENTRY",
                Some(function.span),
                "closure entry signature differs from its definition",
            ));
        }
        for (local, ty) in function.parameters.iter().zip(&closure.parameters) {
            if local_type(function, *local, Some(function.span))? != ty {
                return Err(reject(
                    "E-VERIFY-CLOSURE-ENTRY",
                    Some(function.span),
                    "closure entry parameter differs from its definition",
                ));
            }
        }
    }
    for (index, function) in module.functions.iter().enumerate() {
        budget.spend(1)?;
        if function.environment != entries[index] {
            return Err(reject(
                "E-VERIFY-CLOSURE-ENTRY",
                Some(function.span),
                "function has an orphaned or mismatched environment identity",
            ));
        }
    }
    Ok(())
}

/// A unique carrier is required for owned resource fields and for a consuming
/// entry. This is a representation classification, not proof of responsibility.
pub(super) fn owned_type(module: &TypedModule, ty: &Type, budget: &mut Budget) -> Check<bool> {
    let Type::Closure(id) = ty else {
        return Ok(false);
    };
    let definition = definition(module, *id, None)?;
    budget.spend(definition.captures.len().saturating_add(1))?;
    Ok(definition.kind == CallableKind::FnOnce
        || definition
            .captures
            .iter()
            .any(|ty| matches!(ty, Type::Managed(_) | Type::Package { .. })))
}

pub(super) fn check_resource_instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    match &instruction.kind {
        InstructionKind::DescribeCapture { index } | InstructionKind::TakeCapture { index } => {
            let environment = function.environment.ok_or_else(|| {
                reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "resource capture operation requires an entry environment",
                )
            })?;
            let definition = definition(module, environment, span)?;
            let Some(Type::Managed(inner)) = definition.captures.get(*index as usize) else {
                return Err(reject(
                    "E-VERIFY-CAPTURE",
                    span,
                    "resource capture operation requires an actual Managed field",
                ));
            };
            if matches!(instruction.kind, InstructionKind::TakeCapture { .. }) {
                if definition.kind != CallableKind::FnOnce {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-CALL",
                        span,
                        "moving an owned capture requires a Consume entry",
                    ));
                }
                Ok(Some(Type::Managed(inner.clone())))
            } else {
                Ok(Some(Type::Desc(inner.clone())))
            }
        }
        InstructionKind::DropClosure { local } => {
            let ty = local_type(function, *local, span)?;
            if !owned_type(module, ty, budget)? {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "DropClosure requires an owned or consuming environment position",
                ));
            }
            Ok(None)
        }
        InstructionKind::CallClosureBorrowed {
            callee,
            kind,
            arguments,
        } => {
            let Type::Address { mutable, inner } = value_type(function, *callee, span)? else {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "borrowed closure call requires an actual environment address",
                ));
            };
            let Type::Closure(id) = inner.as_ref() else {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "borrowed closure call pointee must be a concrete environment",
                ));
            };
            let definition = definition(module, *id, span)?;
            if !matches!(kind, CallableKind::Fn | CallableKind::FnMut)
                || !supports(definition.kind, *kind)
                || (*kind == CallableKind::FnMut && !mutable)
            {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "borrowed closure call mode lacks its concrete entry or address permission",
                ));
            }
            if arguments.len() != definition.parameters.len() {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "borrowed closure call arity mismatch",
                ));
            }
            for (value, ty) in arguments.iter().zip(&definition.parameters) {
                if value_type(function, *value, span)? != ty {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-CALL",
                        span,
                        "borrowed closure argument mismatch",
                    ));
                }
            }
            Ok((definition.result != Type::Unit).then(|| definition.result.clone()))
        }
        InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureOnce { arguments, .. } => {
            let (ty, expected_kind) = match instruction.kind {
                InstructionKind::CallClosureShared { callee, .. } => {
                    (local_type(function, callee, span)?, CallableKind::Fn)
                }
                InstructionKind::CallClosureOnce { callee, .. } => {
                    (value_type(function, callee, span)?, CallableKind::FnOnce)
                }
                _ => unreachable!(),
            };
            let Type::Closure(id) = ty else {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call requires a concrete environment",
                ));
            };
            let definition = definition(module, *id, span)?;
            if !supports(definition.kind, expected_kind) {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call mode does not match its concrete entry",
                ));
            }
            if arguments.len() != definition.parameters.len() {
                return Err(reject(
                    "E-VERIFY-CLOSURE-CALL",
                    span,
                    "closure call arity mismatch",
                ));
            }
            for (value, ty) in arguments.iter().zip(&definition.parameters) {
                if value_type(function, *value, span)? != ty {
                    return Err(reject(
                        "E-VERIFY-CLOSURE-CALL",
                        span,
                        "closure call argument mismatch",
                    ));
                }
            }
            Ok((definition.result != Type::Unit).then(|| definition.result.clone()))
        }
        _ => unreachable!(),
    }
}

/// The environment carries address dependencies even when a helper body has no
/// explicit Address-typed temporary of its own. Desc dependencies are tracked
/// separately by the resource checker; they are not address lifetime claims.
pub(super) fn holds_addresses(module: &TypedModule, ty: &Type) -> bool {
    match ty {
        Type::Address { .. } => true,
        Type::Closure(id) => module.closures[id.0 as usize]
            .captures
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. })),
        _ => false,
    }
}

/// The requested mode is an invocation capability, not a change to the actual
/// entry ABI. Consuming adapters retain their explicit CallClosureOnce cleanup.
pub(super) fn supports(actual: CallableKind, requested: CallableKind) -> bool {
    matches!(
        (actual, requested),
        (
            CallableKind::Fn,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) | (
            CallableKind::FnMut,
            CallableKind::FnMut | CallableKind::FnOnce
        ) | (CallableKind::FnOnce, CallableKind::FnOnce)
    )
}
