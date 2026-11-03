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
        if !self.helper_bounds.is_empty() {
            return Err(unsupported(
                span,
                "thread_scope in callable parameter/helper entries is not implemented",
            ));
        }
        if let Some(id) = self.function.environment {
            // This is a real materialized concrete entry, not an arbitrary
            // environment bit granting permission to enter the thread root.
            let pending = self.closures.pending.get(id.0 as usize).ok_or_else(|| {
                error("E-THREAD-WORKER", span, "parent closure identity is absent")
            })?;
            if pending.id != id
                || pending.function != self.function.id
                || pending.inference.arity() != self.function.parameters.len()
                || self.function.foreign.is_some()
                || self.function.result_mode != zil_ir::PassingMode::Infer
            {
                return Err(error(
                    "E-THREAD-WORKER",
                    span,
                    "parent closure actual entry differs from its definition",
                ));
            }
            let mut work =
                inference::type_snapshot_work(&self.function.return_type, span, self.limits)?;
            for local in &self.function.parameters {
                work = work.saturating_add(inference::type_snapshot_work(
                    &self.function.locals[local.0 as usize].ty,
                    span,
                    self.limits,
                )?);
            }
            self.closures.snapshot(work.saturating_add(1), span)?;
            let pending = &self.closures.pending[id.0 as usize];
            if pending.inference.result()?.as_ref() != Some(&self.function.return_type) {
                return Err(error(
                    "E-THREAD-WORKER",
                    span,
                    "parent closure result differs from its actual entry",
                ));
            }
            for (index, local) in self.function.parameters.iter().enumerate() {
                if pending.inference.parameter(index)?.as_ref()
                    != Some(&self.function.locals[local.0 as usize].ty)
                {
                    return Err(error(
                        "E-THREAD-WORKER",
                        span,
                        "parent closure parameter differs from its actual entry",
                    ));
                }
            }
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
