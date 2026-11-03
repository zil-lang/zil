//! Source preparation only. The independent kernels reconstruct worker purity.
use super::*;

impl Signature {
    pub(super) fn scalar_thread_worker(&self) -> bool {
        !self.foreign
            && !self.cleanup
            && self.callable.is_empty()
            && self.callable_return.is_none()
            && self.type_parameter.is_none()
            && self.template.is_none()
            && self.receiver.is_none()
            && self.parameters.as_slice() == [Type::I32]
            && self.result == Type::I32
    }
}

impl Builder<'_, '_> {
    pub(super) fn with_scoped_thread(
        &mut self,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let [worker, argument, body] = arguments else {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "thread_scope requires a static worker, i32 payload and body",
            ));
        };
        if self.function.environment.is_some() || !self.helper_bounds.is_empty() {
            return Err(unsupported(
                span,
                "thread_scope is implemented only in ordinary Direct functions; closure and callable helper entries are not implemented",
            ));
        }
        if !matches!(
            worker.kind,
            CoreExprKind::Name(_) | CoreExprKind::Member { .. }
        ) {
            return Err(error(
                "E-THREAD-WORKER",
                worker.span,
                "thread worker requires an actual static ordinary function symbol",
            ));
        }
        let signature = self.resolve_callee(worker)?.clone();
        if !signature.scalar_thread_worker() {
            return Err(error(
                "E-THREAD-WORKER",
                worker.span,
                "thread worker must be a concrete ordinary fn(i32)->i32",
            ));
        }
        let worker = signature.concrete_id(worker.span)?;
        let (argument_value, argument_type) = self.expr(argument, Some(&Type::I32))?;
        self.same(&argument_type, &Type::I32, argument.span)?;
        let (body_value, body_type) = self.transfer_expr(body, None)?;
        let Type::Closure(id) = body_type else {
            return Err(error(
                "E-THREAD-BODY",
                body.span,
                "thread_scope body must be a concrete zero-argument closure",
            ));
        };
        self.closures.snapshot(1, span)?;
        let pending = &mut self.closures.pending[id.0 as usize];
        if pending.inference.arity() != 0 {
            return Err(error(
                "E-THREAD-BODY",
                body.span,
                "thread_scope body signature is ()->i32",
            ));
        }
        pending.inference.expect_result(&Type::I32, body.span)?;
        self.materialize_closure(id)?;
        let ty = Type::Tuple(vec![Type::Bool, Type::I32, Type::I32]);
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        let value = self.value(
            InstructionKind::WithScopedThread {
                worker,
                argument: argument_value,
                body: body_value,
            },
            ty.clone(),
            span,
        )?;
        Ok((value, ty))
    }
}
