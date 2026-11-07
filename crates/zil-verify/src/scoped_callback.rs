//! Independent checks for the synchronous foreign registration operation.
//! Roles are unsafe external promises, never a source of local loan authority.
use super::*;
fn bad(span: Option<Span>, message: &str) -> Diagnostic {
    reject("E-VERIFY-SCOPED-CALLBACK", span, message)
}
pub(super) fn declaration(function: &TypedFunction, budget: &mut Budget) -> Check<()> {
    let Some(foreign) = &function.foreign else {
        return Ok(());
    };
    let Some(role) = foreign.scoped_callback else {
        return Ok(());
    };
    let span = Some(function.span);
    let invalid = || {
        reject(
            "E-VERIFY-FOREIGN",
            span,
            "scoped callback role requires its exact static C signature and canonical declaration",
        )
    };
    budget.spend(4)?;
    if function.call_kind != CallKind::Direct
        || !foreign.static_callbacks
        || !foreign.noescape_parameters.is_empty()
        || function.result_mode != PassingMode::Infer
        || function.callable_contract.is_some()
        || function.callable_return_contract.is_some()
    {
        return Err(invalid());
    }
    let valid = match role {
        ScopedCallbackRole::AsyncPollDrain | ScopedCallbackRole::ThreadAsyncPollDrain => {
            function.parameters.len() == 1
                && function.return_type == Type::I32
                && local_type(function, function.parameters[0], span)? == &Type::ForeignHandle
        }
        ScopedCallbackRole::Register
        | ScopedCallbackRole::AsyncRegister
        | ScopedCallbackRole::ThreadAsyncRegister => {
            if function.parameters.len() != 2 || function.return_type != Type::ForeignHandle {
                false
            } else {
                let callback = local_type(function, function.parameters[0], span)?;
                let context = local_type(function, function.parameters[1], span)?;
                matches!(callback,Type::ForeignCallback{parameters,result} if parameters.as_slice()==[Type::ForeignHandle,Type::I32] && result.as_ref()==&Type::I64)
                    && context == &Type::ForeignHandle
            }
        }
        ScopedCallbackRole::Close
        | ScopedCallbackRole::AsyncRequestClose
        | ScopedCallbackRole::ThreadAsyncRequestClose => {
            function.parameters.len() == 1
                && function.return_type == Type::Unit
                && local_type(function, function.parameters[0], span)? == &Type::ForeignHandle
        }
    };
    if !valid {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn instruction(
    module: &TypedModule,
    caller: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let InstructionKind::WithForeignCallback {
        register,
        close,
        callback,
        body,
        ..
    } = instruction.kind
    else {
        unreachable!()
    };
    let span = Some(instruction.span);
    budget.spend(5)?;
    tasks::require_synchronous_value(module, caller, callback, budget, span)?;
    for (id, role) in [
        (register, ScopedCallbackRole::Register),
        (close, ScopedCallbackRole::Close),
    ] {
        let function = module.functions.get(id.0 as usize).ok_or_else(|| {
            reject(
                "E-VERIFY-FOREIGN",
                span,
                "scoped callback static declaration is absent",
            )
        })?;
        if function
            .foreign
            .as_ref()
            .and_then(|foreign| foreign.scoped_callback)
            != Some(role)
        {
            return Err(reject(
                "E-VERIFY-FOREIGN",
                span,
                "scoped callback static declaration has the wrong role",
            ));
        }
        declaration(function, budget)?;
    }
    let Type::Address {
        mutable: true,
        inner,
    } = value_type(caller, callback, span)?
    else {
        return Err(bad(
            span,
            "callback requires a mutable address of its actual concrete closure",
        ));
    };
    let Type::Closure(id) = inner.as_ref() else {
        return Err(bad(
            span,
            "callback address must point to a concrete closure",
        ));
    };
    let f = closures::definition(module, *id, span)?;
    budget.spend(f.parameters.len().saturating_add(1))?;
    if !matches!(f.kind, CallableKind::Fn | CallableKind::FnMut)
        || f.parameters.as_slice() != [Type::I32]
        || f.result != Type::I32
    {
        return Err(bad(
            span,
            "callback requires an actual Fn or FnMut(i32)->i32 entry",
        ));
    }
    closures::self_contained(module, *id, budget).map_err(|error| {
        if error.category == zil_base::ResultCategory::ResourceLimit {
            error
        } else {
            bad(
                span,
                "callback environment must be recursively self-contained",
            )
        }
    })?;
    let Type::Closure(body_id) = value_type(caller, body, span)? else {
        return Err(bad(span, "body requires a concrete closure value"));
    };
    let body = closures::definition(module, *body_id, span)?;
    budget.spend(
        body.parameters
            .len()
            .saturating_add(body.lending_captures.len())
            .saturating_add(1),
    )?;
    if !body.parameters.is_empty()
        || body.result != Type::I32
        || !closures::supports(body.kind, CallableKind::FnOnce)
        || !body.lending_captures.is_empty()
    {
        return Err(bad(
            span,
            "body must admit one consuming zero-argument i32 invocation",
        ));
    }
    budget.spend(3)?;
    Ok(Some(Type::Tuple(vec![Type::Bool, Type::I32])))
}
