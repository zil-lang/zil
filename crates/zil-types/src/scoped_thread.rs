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
            && (self.parameters.as_slice() == [Type::I32]
                || matches!(self.parameters.as_slice(), [Type::Address { mutable: false, inner }] if **inner == Type::I32 || matches!(inner.as_ref(), Type::SyncCell(_))))
            && self.result == Type::I32
    }
}

pub(super) fn sync_primitive(name: &str) -> Option<(SyncCellKind, Option<SyncI32Op>)> {
    Some(match name {
        "mutex_scope" => (SyncCellKind::MutexI32, None),
        "atomic_scope" => (SyncCellKind::AtomicI32, None),
        "mutex_load" => (SyncCellKind::MutexI32, Some(SyncI32Op::Load)),
        "mutex_store" => (SyncCellKind::MutexI32, Some(SyncI32Op::Store)),
        "mutex_fetch_add" => (SyncCellKind::MutexI32, Some(SyncI32Op::FetchAdd)),
        "atomic_load" => (SyncCellKind::AtomicI32, Some(SyncI32Op::Load)),
        "atomic_store" => (SyncCellKind::AtomicI32, Some(SyncI32Op::Store)),
        "atomic_fetch_add" => (SyncCellKind::AtomicI32, Some(SyncI32Op::FetchAdd)),
        _ => return None,
    })
}

pub(super) fn sync_address(kind: SyncCellKind) -> Type {
    Type::Address {
        mutable: false,
        inner: Box::new(Type::SyncCell(kind)),
    }
}

impl Builder<'_, '_> {
    pub(super) fn sync_call(
        &mut self,
        kind: SyncCellKind,
        op: Option<SyncI32Op>,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        if let Some(op) = op {
            let arity = if op == SyncI32Op::Load { 1 } else { 2 };
            if arguments.len() != arity {
                return Err(error("E-CALL-ARITY", span, "incorrect synchronization operation arity"));
            }
            let address_ty = sync_address(kind);
            let (address, actual) = self.expr(&arguments[0], Some(&address_ty))?;
            self.same(&actual, &address_ty, arguments[0].span)?;
            let value = if let Some(argument) = arguments.get(1) {
                let (value, actual) = self.expr(argument, Some(&Type::I32))?;
                self.same(&actual, &Type::I32, argument.span)?;
                Some(value)
            } else {
                None
            };
            let ty = if op == SyncI32Op::Store { Type::Unit } else { Type::I32 };
            if let Some(expected) = expected { self.same(&ty, expected, span)?; }
            let value = self.value(InstructionKind::SyncI32 {
                op, address, value, ordering: AtomicOrdering::SeqCst,
            }, ty.clone(), span)?;
            return Ok((value, ty));
        }
        let [initial, body] = arguments else {
            return Err(error("E-CALL-ARITY", span, "cell scope requires initial i32 and body"));
        };
        let (initial_value, initial_ty) = self.expr(initial, Some(&Type::I32))?;
        self.same(&initial_ty, &Type::I32, initial.span)?;
        let (body_value, body_ty) = self.transfer_expr(body, None)?;
        let Type::Closure(id) = body_ty else {
            return Err(error("E-SYNC-BODY", body.span, "cell scope requires an actual concrete closure"));
        };
        self.closures.snapshot(1, span)?;
        if self.closures.pending[id.0 as usize].inference.arity() != 1 {
            return Err(error("E-SYNC-BODY", body.span, "cell scope body takes one shared sealed cell address"));
        }
        if self.closures.thread_parameter(id, 0)?.as_ref() != Some(&sync_address(kind)) {
            return Err(error("E-SYNC-BODY", body.span, "cell scope parameter is not the actual sealed cell kind"));
        }
        self.closures.thread_expect_result(id, &Type::I32, body.span)?;
        self.materialize_closure(id)?;
        let ty = Type::Tuple(vec![Type::Bool, Type::I32]);
        if let Some(expected) = expected { self.same(&ty, expected, span)?; }
        let value = self.value(InstructionKind::WithSyncCell {
            kind, initial: initial_value, body: body_value,
        }, ty.clone(), span)?;
        Ok((value, ty))
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
            if self.closures.thread_result(id)?.as_ref() != Some(&self.function.return_type) {
                return Err(error(
                    "E-THREAD-WORKER",
                    span,
                    "parent closure result differs from its actual entry",
                ));
            }
            for (index, local) in self.function.parameters.iter().enumerate() {
                if self.closures.thread_parameter(id, index)?.as_ref()
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
                "thread worker must be a concrete ordinary fn(i32)->i32 or fn(&i32)->i32",
            ));
        }
        let worker = signature.concrete_id(worker.span)?;
        let payload_type = &signature.parameters[0];
        let (argument_value, argument_type) = self.expr(argument, Some(payload_type))?;
        self.same(&argument_type, payload_type, argument.span)?;
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
        self.closures
            .thread_expect_result(id, &Type::I32, body.span)?;
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
