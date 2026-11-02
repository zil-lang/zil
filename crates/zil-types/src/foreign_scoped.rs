//! Producer preparation for one structured external registration.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn with_foreign_callback(
        &mut self,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let [register, close, callback, body] = arguments else {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "c_with_callback requires register, close, exclusive callback address and body",
            ));
        };
        let region = self.unsafe_regions.last().copied().ok_or_else(|| {
            error(
                "E-FFI-UNSAFE",
                span,
                "c_with_callback requires an explicit lexical unsafe region",
            )
        })?;
        // A symbolic callable bound grants invocation, never Copy. Preserve
        // its declared source even when this actual concrete environment is Copy.
        if !self.helper_bounds.is_empty() {
            self.forward_modes(span)?;
        }
        let register = self.resolve_callee(register)?.clone();
        let close = self.resolve_callee(close)?.clone();
        if register.scoped_callback != Some(ScopedCallbackRole::Register)
            || close.scoped_callback != Some(ScopedCallbackRole::Close)
        {
            return Err(error(
                "E-FFI-SCOPED-ROLE",
                span,
                "c_with_callback requires actual static register and close role declarations",
            ));
        }
        let (callback_value, callback_type) = self.expr(callback, None)?;
        let Type::Address {
            mutable: true,
            inner,
        } = callback_type
        else {
            return Err(error(
                "E-FFI-SCOPED-CALLBACK",
                callback.span,
                "scoped callback requires an actual live exclusive closure address",
            ));
        };
        let Type::Closure(callback_id) = *inner else {
            return Err(error(
                "E-FFI-SCOPED-CALLBACK",
                callback.span,
                "scoped callback address must point to one actual concrete closure",
            ));
        };
        let declared_expression = match &callback.kind {
            CoreExprKind::Unary {
                op: UnaryOp::BorrowMut,
                operand,
            } => operand.as_ref(),
            _ => callback,
        };
        if self.declared_state_kind(declared_expression, None, callback_id)?
            == Some(CallableKind::FnOnce)
        {
            return Err(error(
                "E-FFI-SCOPED-CALLBACK",
                callback.span,
                "a declared FnOnce callback cannot gain FnMut from its concrete environment",
            ));
        }
        self.closures.snapshot(1, span)?;
        let pending = &mut self.closures.pending[callback_id.0 as usize];
        if pending.inference.arity() != 1 {
            return Err(error(
                "E-FFI-SCOPED-CALLBACK",
                callback.span,
                "scoped callback signature must be (i32)->i32",
            ));
        }
        pending.inference.argument(0, &Type::I32, span)?;
        pending.inference.expect_result(&Type::I32, span)?;
        self.materialize_closure(callback_id)?;
        if !matches!(
            self.closures.pending[callback_id.0 as usize].callable_kind(),
            CallableKind::Fn | CallableKind::FnMut
        ) {
            return Err(error(
                "E-FFI-SCOPED-CALLBACK",
                callback.span,
                "scoped callback must support FnMut invocation and cannot be FnOnce",
            ));
        }
        self.closures.factory_self_contained(callback_id, callback.span).map_err(|diagnostic| {
            if diagnostic.category==zil_base::ResultCategory::ResourceLimit {diagnostic}
            else {error("E-FFI-SCOPED-CALLBACK",callback.span,"scoped callback must be self-contained and cannot capture borrowed positions or descriptions")}
        })?;
        // An ordinary shared scalar capture makes this concrete environment
        // non-Copy too. Keep Copy locals on their existing copying path.
        let local_body = if !self.helper_bounds.is_empty()
            && let CoreExprKind::Name(name) = &body.kind
            && let Some(local) = self.lookup(name)
            && !self.closure_slots.contains_key(&local)
            && !self.borrowed_captures.contains_key(&local)
            && !self.closure_captures.contains_key(&local)
            && let Type::Closure(id) = self.function.locals[local.0 as usize].ty
            && self.declared_state_kind(body, None, id)?.is_none()
        {
            self.closures.snapshot(
                self.closures.pending[id.0 as usize].captures.len(),
                body.span,
            )?;
            if self.closures.pending[id.0 as usize]
                .captures
                .iter()
                .any(|capture| capture.borrowed || matches!(capture.ty, Type::Address { .. }))
            {
                Some((local, Type::Closure(id)))
            } else {
                None
            }
        } else {
            None
        };
        let (body_value, body_type) = if let Some((local, ty)) = local_body {
            (
                self.value(InstructionKind::TakeLocal { local }, ty.clone(), body.span)?,
                ty,
            )
        } else {
            self.transfer_expr(body, None)?
        };
        let Type::Closure(body_id) = body_type else {
            return Err(error(
                "E-FFI-SCOPED-BODY",
                body.span,
                "scoped callback body must be an actual concrete zero-argument closure",
            ));
        };
        self.closures.snapshot(1, span)?;
        let pending = &mut self.closures.pending[body_id.0 as usize];
        if pending.inference.arity() != 0 {
            return Err(error(
                "E-FFI-SCOPED-BODY",
                body.span,
                "scoped callback body signature must be ()->i32",
            ));
        }
        pending.inference.expect_result(&Type::I32, span)?;
        self.materialize_closure(body_id)?;
        let ty = Type::Tuple(vec![Type::Bool, Type::I32]);
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        let value = self.value(
            InstructionKind::WithForeignCallback {
                register: register.concrete_id(span)?,
                close: close.concrete_id(span)?,
                callback: callback_value,
                body: body_value,
                region,
            },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }
}
