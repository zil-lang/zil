//! Actual concrete owning registration construction and declared generic bound.
use super::*;
use std::collections::BTreeSet;
pub(super) fn primitive(e: &CoreExpr) -> Option<&str> {
    let CoreExprKind::Name(n) = &e.kind else {
        return None;
    };
    matches!(
        n.as_str(),
        "c_register" | "registration_valid" | "registration_close"
    )
    .then_some(n)
}
pub(super) fn environment(t: &Type) -> Option<&Type> {
    let (Type::Managed(p) | Type::Desc(p)) = t else {
        return None;
    };
    let Type::Registration { environment } = p.as_ref() else {
        return None;
    };
    Some(environment)
}
pub(super) fn resource(t: &Type) -> bool {
    environment(t).is_some()
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
            let mut pending = vec![root];
            while let Some(t) = pending.pop() {
                spend(left, 1)?;
                match t {
                    Type::Registration { environment } => {
                        let Type::Closure(id) = environment.as_ref() else {
                            return Err(unsupported(
                                f.span,
                                "registration must retain an actual concrete environment",
                            ));
                        };
                        spend(left, found.len().saturating_add(32))?;
                        found.insert(*id, t.clone());
                    }
                    Type::Managed(t)
                    | Type::Desc(t)
                    | Type::Address { inner: t, .. }
                    | Type::Opaque { hidden: t, .. }
                    | Type::Array { element: t, .. }
                    | Type::OwnedStore { element: t }
                    | Type::Slice { element: t } => {
                        spend(left, 1)?;
                        pending.push(t)
                    }
                    Type::Tuple(ts) => {
                        spend(left, ts.len())?;
                        pending.extend(ts)
                    }
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        spend(left, fields.len())?;
                        pending.extend(fields.iter().map(|(_, t)| t))
                    }
                    Type::Enum { variants, .. } => {
                        spend(left, variants.len())?;
                        for (_, ts) in variants {
                            spend(left, ts.len())?;
                            pending.extend(ts)
                        }
                    }
                    Type::ErrorUnion { error, result } => {
                        spend(left, 2)?;
                        pending.extend([error.as_ref(), result.as_ref()])
                    }
                    _ => {}
                }
            }
        }
    }
    spend(left, found.len().saturating_mul(32))?;
    for (id, payload) in found {
        definitions.push(ResourceDefinition {
            name: format!("@builtin:Registration:{}", id.0),
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
        let (kind, ty) = if name == "c_register" {
            let [register, close, callback] = arguments else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "c_register requires register, close and actual F",
                ));
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
            if register.scoped_callback != Some(ScopedCallbackRole::Register)
                || close.scoped_callback != Some(ScopedCallbackRole::Close)
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
            closed_environment(self.closures, id, callback.span)?;
            let ty = Type::Managed(Box::new(Type::Registration {
                environment: Box::new(Type::Closure(id)),
            }));
            (
                InstructionKind::RegistrationStart {
                    register: register.concrete_id(span)?,
                    close: close.concrete_id(span)?,
                    callback: callback_value,
                    region,
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
            if !resource(&ty)
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
                Type::Task { .. }
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
                Type::Managed(t)
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
                | Type::I32
                | Type::I64
                | Type::U64
                | Type::Usize
                | Type::F32
                | Type::F64
                | Type::ForeignHandle
                | Type::CWideString
                | Type::ByteBuffer
                | Type::TaskI32 => {}
            }
        }
        state.snapshot(before - work, span)?;
    }
    Ok(())
}
