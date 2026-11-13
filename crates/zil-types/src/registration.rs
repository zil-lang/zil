//! Actual concrete owning registration construction and declared generic bound.
use super::*;
use std::collections::BTreeSet;
pub(super) mod definition;
pub(super) mod threaded;
pub(super) fn charge_contract(
    c: &FunctionProtocolContract,
    mut pay: impl FnMut(usize) -> R<()>,
) -> R<()> {
    pay(c
        .entry
        .len()
        .saturating_add(c.exits.len())
        .saturating_add(1))?;
    for row in &c.exits {
        pay(row.output.len().saturating_add(1))?;
    }
    Ok(())
}
pub(super) fn primitive(e: &CoreExpr) -> Option<&str> {
    let CoreExprKind::Name(n) = &e.kind else {
        return None;
    };
    matches!(
        n.as_str(),
        "c_register"
            | "registration_valid"
            | "registration_close"
            | "c_register_async"
            | "async_registration_valid"
            | "async_registration_request_close"
            | "async_registration_poll_close"
            | "async_registration_finish_destroy"
            | "async_registration_finish_destroy_checked"
    )
    .then_some(n)
}
pub(super) fn environment(t: &Type) -> Option<&Type> {
    let (Type::Managed(p) | Type::Desc(p)) = t else {
        return None;
    };
    let (Type::Registration { environment } | Type::AsyncRegistration { environment }) = p.as_ref()
    else {
        return None;
    };
    Some(environment)
}
pub(super) fn asynchronous(t: &Type) -> bool {
    matches!(t,Type::Managed(p)|Type::Desc(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
}
pub(super) fn resource(t: &Type) -> bool {
    environment(t).is_some()
}
pub(super) fn borrowed_environment(t: &Type) -> Option<&Type> {
    let Type::Address {
        mutable: true,
        inner,
    } = t
    else {
        return None;
    };
    let Type::Managed(payload) = inner.as_ref() else {
        return None;
    };
    let Type::AsyncRegistration { environment } = payload.as_ref() else {
        return None;
    };
    Some(environment)
}
pub(super) fn owned_template(f: &CoreFunction) -> bool {
    !f.callable_parameters.is_empty() && f.callable_parameters.iter().all(|b| f.parameters.get(b.index as usize).is_some_and(|p|
        p.mode==PassingMode::Give && matches!(&p.ty,Type::Managed(t) if matches!(t.as_ref(),Type::AsyncRegistration{environment} if matches!(environment.as_ref(),Type::Named(n) if n==&b.generic_name)))))
}
pub(super) fn borrowed_template(f: &CoreFunction) -> bool {
    !f.callable_parameters.is_empty()
        && f.callable_parameters.iter().all(|b| {
            f.parameters
                .get(b.index as usize)
                .and_then(|p| borrowed_environment(&p.ty))
                .is_some_and(|t| matches!(t,Type::Named(n)if n==&b.generic_name))
        })
}
pub(super) fn validate_borrowed_template(
    f: &CoreFunction,
    prepay: &mut impl FnMut(&Type, Span) -> R<()>,
) -> R<()> {
    let owned = owned_template(f);
    if (!borrowed_template(f) && !owned)
        || f.foreign.is_some()
        || f.receiver.is_some()
        || !f.state_parameters.is_empty()
        || (!owned && f.call_kind.is_some_and(|kind| kind != CallKind::Direct))
        || !f.type_parameters.is_empty()
        || f.callable_return.is_some()
        || !f.return_type.is_copy_value()
    {
        return Err(unsupported(
            f.span,
            "registration helper requires actual closed FnMut carriers, Copy result and a state contract for every role",
        ));
    }
    for b in &f.callable_parameters {
        if !b.closed_environment
            || b.kind != CallableKind::FnMut
            || b.parameters != [Type::I32]
            || b.result != Type::I32
            || b.life.is_some()
            || b.source_life.is_some()
            || !f.protocol_contracts.iter().any(|c| c.parameter == b.index)
        {
            return Err(unsupported(
                f.span,
                "registration helper omits a complete actual callable/protocol role",
            ));
        }
    }
    for parameter in &f.parameters {
        prepay(&parameter.ty, f.span)?;
    }
    prepay(&f.return_type, f.span)?;
    Ok(())
}
pub(super) fn template(f: &CoreFunction) -> bool {
    matches!(f.callable_parameters.as_slice(),[b] if environment(&f.return_type).is_some_and(|t|matches!(t,Type::Named(n)if n==&b.generic_name)))
}
pub(super) fn validate_template(
    f: &CoreFunction,
    prepay: &mut impl FnMut(&Type, Span) -> R<()>,
) -> R<()> {
    let [b] = f.callable_parameters.as_slice() else {
        return Err(unsupported(
            f.span,
            "Registration return requires one callable binder",
        ));
    };
    if !b.closed_environment
        || b.kind != CallableKind::FnMut
        || b.parameters != [Type::I32]
        || b.result != Type::I32
        || !matches!(f.result_mode, PassingMode::Create | PassingMode::Give)
        || f.parameters
            .get(b.index as usize)
            .is_none_or(|p| helpers::parameter_mode(&p.ty, &b.generic_name) != Some(None))
    {
        return Err(error(
            "E-REGISTRATION-BOUND",
            f.span,
            "Registration<F> requires explicit FnMut(i32)->i32 + ClosedEnvironment, owning F input and result(create/give)",
        ));
    }
    for t in f
        .parameters
        .iter()
        .map(|p| &p.ty)
        .chain(std::iter::once(&f.return_type))
    {
        prepay(t, f.span)?
    }
    Ok(())
}
pub(super) fn add_definitions(
    functions: &[TypedFunction],
    definitions: &mut Vec<ResourceDefinition>,
    left: &mut usize,
) -> R<()> {
    let mut found = BTreeMap::new();
    let spend = |left: &mut usize, n| -> R<()> {
        *left = left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "registration definition inventory exceeded budget",
                None,
            )
        })?;
        Ok(())
    };
    for f in functions {
        spend(
            left,
            f.values
                .len()
                .saturating_add(f.locals.len())
                .saturating_add(1),
        )?;
        for root in f
            .values
            .iter()
            .chain(f.locals.iter().map(|l| &l.ty))
            .chain(std::iter::once(&f.return_type))
        {
            let mut pending = vec![(root, false)];
            while let Some((t, blocked)) = pending.pop() {
                spend(left, 1)?;
                tasks::require_closed_context(t, f.span)?;
                if blocked && matches!(t, Type::AsyncRegistration { .. }) {
                    return Err(unsupported(
                        f.span,
                        "pollable registration roots in ordinary aggregate storage require an asynchronous ownership consumer",
                    ));
                }
                match t {
                    Type::Registration { environment }
                    | Type::AsyncRegistration { environment } => {
                        let Type::Closure(id) = environment.as_ref() else {
                            return Err(unsupported(
                                f.span,
                                "registration must retain an actual concrete environment",
                            ));
                        };
                        spend(left, found.len().saturating_add(32))?;
                        found.insert(
                            (matches!(t, Type::AsyncRegistration { .. }), *id),
                            t.clone(),
                        );
                    }
                    Type::BorrowedStorage { storage: t, .. }
                    | Type::Task { result: t, .. }
                    | Type::Managed(t)
                    | Type::Desc(t)
                    | Type::Address { inner: t, .. }
                    | Type::Opaque { hidden: t, .. }
                    | Type::Array { element: t, .. }
                    | Type::Slice { element: t } => {
                        spend(left, 1)?;
                        pending.push((t, blocked))
                    }
                    Type::OwnedStore { element } => {
                        spend(left, 1)?;
                        pending.push((element, true));
                    }
                    Type::Tuple(ts) => {
                        spend(left, ts.len())?;
                        pending.extend(ts.iter().map(|t| (t, blocked)))
                    }
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        spend(left, fields.len())?;
                        let blocked = blocked
                            || matches!(t,Type::Package{name,..} if !name.starts_with("@std.task:v1:TaskCreateFailure:"));
                        pending.extend(fields.iter().map(|(_, t)| (t, blocked)))
                    }
                    Type::Enum { variants, .. } => {
                        spend(left, variants.len())?;
                        for (_, ts) in variants {
                            spend(left, ts.len())?;
                            pending.extend(ts.iter().map(|t| (t, blocked)))
                        }
                    }
                    Type::ErrorUnion { error, result } => {
                        spend(left, 2)?;
                        let admitted = (matches!(error.as_ref(),Type::Package{name,..}|Type::Record{name,..} if name.starts_with("@std.task:v1:TaskCreateFailure:"))
                            || tasks::failure::is_carrier(error))
                            && matches!(result.as_ref(),Type::Managed(t) if matches!(t.as_ref(),Type::Task{..}));
                        pending.extend([
                            (error.as_ref(), blocked || !admitted),
                            (result.as_ref(), blocked || !admitted),
                        ])
                    }
                    _ => {}
                }
            }
        }
    }
    spend(left, found.len().saturating_mul(32))?;
    for ((asynchronous, id), payload) in found {
        definitions.push(ResourceDefinition {
            name: format!(
                "@builtin:{}:{}",
                if asynchronous {
                    "AsyncRegistration"
                } else {
                    "Registration"
                },
                id.0
            ),
            payload,
            teardown: None,
        })
    }
    Ok(())
}
impl Builder<'_, '_> {
    pub(super) fn registration_call(
        &mut self,
        name: &str,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let asynchronous = name.starts_with("async_registration_") || name == "c_register_async";
        let (kind, ty) = if matches!(name, "c_register" | "c_register_async") {
            let (register, close, drain, callback) = match arguments {
                [register, close, callback] if !asynchronous => (register, close, None, callback),
                [register, close, drain, callback] if asynchronous => {
                    (register, close, Some(drain), callback)
                }
                _ => {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "registration constructor requires ordered actual providers and F",
                    ));
                }
            };
            let region = self.unsafe_regions.last().copied().ok_or_else(|| {
                error(
                    "E-FFI-UNSAFE",
                    span,
                    "c_register requires an explicit unsafe region",
                )
            })?;
            let register = self.resolve_callee(register)?.clone();
            let close = self.resolve_callee(close)?.clone();
            let drain = drain.map(|d| self.resolve_callee(d).cloned()).transpose()?;
            let threaded = asynchronous
                && register.scoped_callback == Some(ScopedCallbackRole::ThreadAsyncRegister);
            if register.scoped_callback
                != Some(if threaded {
                    ScopedCallbackRole::ThreadAsyncRegister
                } else if asynchronous {
                    ScopedCallbackRole::AsyncRegister
                } else {
                    ScopedCallbackRole::Register
                })
                || close.scoped_callback
                    != Some(if threaded {
                        ScopedCallbackRole::ThreadAsyncRequestClose
                    } else if asynchronous {
                        ScopedCallbackRole::AsyncRequestClose
                    } else {
                        ScopedCallbackRole::Close
                    })
                || drain.as_ref().is_some_and(|d| {
                    d.scoped_callback
                        != Some(if threaded {
                            ScopedCallbackRole::ThreadAsyncPollDrain
                        } else {
                            ScopedCallbackRole::AsyncPollDrain
                        })
                })
            {
                return Err(error(
                    "E-FFI-REGISTRATION",
                    span,
                    "c_register requires actual static register/close roles",
                ));
            }
            if !self.helper_bounds.is_empty() {
                self.forward_modes(span)?;
            }
            let (callback_value, callback_type) = self.transfer_expr(callback, None)?;
            let Type::Closure(id) = callback_type else {
                return Err(error(
                    "E-REGISTRATION-CALLBACK",
                    callback.span,
                    "registration requires an actual concrete closure value",
                ));
            };
            if self.declared_state_kind(callback, None, id)? == Some(CallableKind::FnOnce) {
                return Err(error(
                    "E-REGISTRATION-CALLBACK",
                    callback.span,
                    "declared FnOnce cannot gain repeated callback authority",
                ));
            }
            self.closures.snapshot(1, span)?;
            let pending = &mut self.closures.pending[id.0 as usize];
            if pending.inference.arity() != 1 {
                return Err(error(
                    "E-REGISTRATION-CALLBACK",
                    callback.span,
                    "callback signature is (i32)->i32",
                ));
            }
            pending.inference.argument(0, &Type::I32, span)?;
            pending.inference.expect_result(&Type::I32, span)?;
            self.materialize_closure(id)?;
            if !matches!(
                self.closures.pending[id.0 as usize].callable_kind(),
                CallableKind::Fn | CallableKind::FnMut
            ) {
                return Err(error(
                    "E-REGISTRATION-CALLBACK",
                    callback.span,
                    "callback must support repeated FnMut invocation",
                ));
            }
            self.closures.factory_self_contained(id, callback.span)?;
            closed_environment_for(self.closures, id, callback.span, asynchronous)?;
            if threaded {
                self.closures.snapshot(
                    self.closures
                        .thread_entries
                        .len()
                        .saturating_add(self.closures.thread_transfers.len())
                        .saturating_add(2),
                    span,
                )?;
                self.closures.thread_entries.insert(id);
                self.closures.thread_transfers.insert(id);
            }
            let environment = Box::new(Type::Closure(id));
            let ty = Type::Managed(Box::new(if asynchronous {
                Type::AsyncRegistration { environment }
            } else {
                Type::Registration { environment }
            }));
            (
                if let Some(drain) = drain {
                    InstructionKind::AsyncRegistrationStart {
                        register: register.concrete_id(span)?,
                        request_close: close.concrete_id(span)?,
                        poll_drain: drain.concrete_id(span)?,
                        callback: callback_value,
                        region,
                    }
                } else {
                    InstructionKind::RegistrationStart {
                        register: register.concrete_id(span)?,
                        close: close.concrete_id(span)?,
                        callback: callback_value,
                        region,
                    }
                },
                ty,
            )
        } else {
            let [argument] = arguments else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "registration operation requires one actual carrier",
                ));
            };
            let (registration, ty) = self.expr(argument, None)?;
            if asynchronous {
                let result = match name {
                    "async_registration_valid" if matches!(&ty,Type::Desc(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})) => {
                        (
                            InstructionKind::AsyncRegistrationValid { registration },
                            Type::Bool,
                        )
                    }
                    "async_registration_finish_destroy"
                    | "async_registration_finish_destroy_checked"
                        if matches!(&ty,Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})) =>
                    {
                        (
                            if name == "async_registration_finish_destroy_checked" {
                                InstructionKind::AsyncRegistrationFinishDestroyChecked {
                                    registration,
                                }
                            } else {
                                InstructionKind::AsyncRegistrationFinishDestroy { registration }
                            },
                            Type::Unit,
                        )
                    }
                    "async_registration_request_close" | "async_registration_poll_close" if matches!(&ty,Type::Address{mutable:true,inner} if matches!(inner.as_ref(),Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))) => {
                        if name == "async_registration_request_close" {
                            (
                                InstructionKind::AsyncRegistrationRequestClose { registration },
                                Type::Unit,
                            )
                        } else {
                            (
                                InstructionKind::AsyncRegistrationPollClose { registration },
                                tasks::poll_type(&Type::Unit),
                            )
                        }
                    }
                    _ => {
                        return Err(error(
                            "E-REGISTRATION-CARRIER",
                            span,
                            "async validity reads actual descriptor; request/poll require exclusive owning head address; finish consumes give owner",
                        ));
                    }
                };
                if let Some(wanted) = expected {
                    self.same(&result.1, wanted, span)?;
                }
                let value = self.value(result.0, result.1.clone(), span)?;
                return Ok((value, result.1));
            }
            if matches!(&ty,Type::Managed(p)|Type::Desc(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
                || !resource(&ty)
                || !matches!(
                    (&ty, name),
                    (Type::Desc(_), "registration_valid")
                        | (Type::Managed(_), "registration_close")
                )
            {
                return Err(error(
                    "E-REGISTRATION-CARRIER",
                    span,
                    "registration_valid reads actual descriptor; registration_close consumes give owner",
                ));
            }
            if name == "registration_valid" {
                (
                    InstructionKind::RegistrationValid { registration },
                    Type::Bool,
                )
            } else {
                (
                    InstructionKind::RegistrationClose { registration },
                    Type::Unit,
                )
            }
        };
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        let value = self.value(kind, ty.clone(), span)?;
        Ok((value, ty))
    }
}

pub(super) fn closed_environment(
    state: &mut closures::State,
    root: ClosureId,
    span: Span,
) -> R<()> {
    closed_environment_for(state, root, span, false)
}
fn closed_environment_for(
    state: &mut closures::State,
    root: ClosureId,
    span: Span,
    asynchronous: bool,
) -> R<()> {
    let mut deferred_static = false;
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        state.snapshot(seen.len().saturating_add(1), span)?;
        if !seen.insert(id) {
            continue;
        }
        let captures = &state
            .pending
            .get(id.0 as usize)
            .ok_or_else(|| unsupported(span, "registration actual environment absent"))?
            .captures;
        // Borrow each complete shape while charging the live State budget; no clone.
        let mut work = state.remaining_work();
        let before = work;
        let pay = |work: &mut usize, n| -> R<()> {
            *work = work.checked_sub(n).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "ClosedEnvironment budget exceeded",
                    Some(span),
                )
            })?;
            Ok(())
        };
        pay(&mut work, captures.len())?;
        let mut types = Vec::new();
        for capture in captures {
            if capture.borrowed || matches!(capture.ty, Type::Managed(_)) && !capture.moved {
                return Err(error(
                    "E-CLOSED-ENVIRONMENT",
                    span,
                    "actual captured position is borrowed",
                ));
            }
            types.push(&capture.ty)
        }
        while let Some(t) = types.pop() {
            pay(&mut work, 1)?;
            match t {
                Type::AsyncRegistration { .. } => {
                    return Err(unsupported(
                        span,
                        "ClosedEnvironment cannot retain a pollable registration with asynchronous teardown",
                    ));
                }
                Type::TaskI32 if asynchronous => {
                    return Err(error(
                        "E-CLOSED-ENVIRONMENT",
                        span,
                        "AsyncRegistration environment requires an actual synchronous legacy Task frame cleanup proof",
                    ));
                }
                Type::Address {
                    mutable: false,
                    inner,
                } if **inner == Type::U8
                    || matches!(inner.as_ref(),Type::Slice{element}if **element==Type::U8) =>
                {
                    deferred_static = true;
                }
                Type::Dyn { .. }
                | Type::Task { .. }
                | Type::Address { .. }
                | Type::Desc(_)
                | Type::Slice { .. }
                | Type::Named(_)
                | Type::Unsupported(_)
                | Type::SyncCell(_) => {
                    return Err(error(
                        "E-CLOSED-ENVIRONMENT",
                        span,
                        "actual stored environment retains an external loan",
                    ));
                }
                Type::Closure(id) => {
                    pay(&mut work, 1)?;
                    pending.push(*id)
                }
                Type::BorrowedStorage { storage: t, .. }
                | Type::Managed(t)
                | Type::Registration { environment: t }
                | Type::Opaque { hidden: t, .. }
                | Type::OwnedStore { element: t }
                | Type::Array { element: t, .. } => {
                    pay(&mut work, 1)?;
                    types.push(t)
                }
                Type::Tuple(ts) => {
                    pay(&mut work, ts.len())?;
                    types.extend(ts)
                }
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    pay(&mut work, fields.len())?;
                    types.extend(fields.iter().map(|(_, t)| t))
                }
                Type::Enum { variants, .. } => {
                    pay(&mut work, variants.len())?;
                    for (_, ts) in variants {
                        pay(&mut work, ts.len())?;
                        types.extend(ts)
                    }
                }
                Type::ErrorUnion { error, result } => {
                    pay(&mut work, 2)?;
                    types.extend([error.as_ref(), result.as_ref()])
                }
                Type::BorrowedFunctionPointer { .. }
                | Type::FunctionPointer { .. }
                | Type::ForeignCallback { .. }
                | Type::Unit
                | Type::Bool
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::I32
                | Type::I8
                | Type::I16
                | Type::Isize
                | Type::I64
                | Type::U64
                | Type::Usize
                | Type::F32
                | Type::F64
                | Type::ForeignHandle
                | Type::CWideString
                | Type::ByteBuffer
                | Type::TaskI32 => {}
                Type::Mutex { .. } | Type::MutexGuard { .. } | Type::OwnedDyn { .. } => {
                    return Err(unsupported(
                        span,
                        "mutex capture needs a complete thread and cleanup contract",
                    ));
                }
            }
        }
        state.snapshot(before - work, span)?;
    }
    if deferred_static {
        state.record_static_closed(root, span)?;
    }
    Ok(())
}
