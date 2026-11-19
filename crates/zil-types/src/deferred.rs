//! Source cleanup actions use real closure capture and executable invocation.
use super::*;

impl Builder<'_, '_> {
    pub(super) fn register_defer(&mut self, body: &CoreBlock, span: Span) -> R<()> {
        if self.defer_body {
            return Err(error(
                "E-DEFER-NESTED",
                span,
                "nested defer is not supported",
            ));
        }
        let (action, result, body) = body.deferred_action().ok_or_else(||
            error("E-DEFER-ACTION", span, "defer requires its actual zero-argument cleanup closure"))?;
        self.same(result, &Type::Unit, action.span)?;
        let (value, ty) = self.emit_expression(action, |builder| builder.closure_expression(
            closures::Input {
                defer_action: true,
                move_capture: false,
                parameters: &[],
                result: Some(result),
                body,
                span: action.span,
            },
            None,
            Some(action.id),
        ))?;
        // This is ordinary lexical storage, not a full-expression temporary.
        // Reserving a temporary would move registration storage before earlier
        // exits and would let finish_resource_temps clean the action too soon.
        let name = format!("$defer{}", self.function.locals.len());
        let local = self.local(&name, ty, false, span)?;
        self.store(local, value, span)?;
        self.emit(
            InstructionKind::RegisterDefer {
                scope: *self.scope_ids.last().expect("active cleanup scope"),
                local,
            },
            None,
            span,
        )?;
        self.closures
            .snapshot(self.defer_locals.len().saturating_add(1), span)?;
        self.defer_locals.insert(local, action.id);
        Ok(())
    }

    pub(super) fn invoke_defer(&mut self, local: LocalId, span: Span) -> R<()> {
        let ty = self.function.locals[local.0 as usize].ty.clone();
        let callee = self.value(InstructionKind::TakeLocal { local }, ty, span)?;
        let block = self.current.ok_or_else(|| error("E-DEFER-CONTROL", span,
            "scope exit has no active block for its registered action"))?;
        self.emit(
            InstructionKind::CallClosureOnce {
                callee,
                arguments: vec![],
            },
            None,
            span,
        )?;
        let before = self.closures.remaining_work();
        let mut left = before;
        let origin = *self.defer_locals.get(&local).ok_or_else(||
            error("E-DEFER-ACTION", span, "scope exit has no registered lexical cleanup operand"))?;
        self.closures.semantic_emission.cleanup_call(self.function.id, origin, block,
            local, callee, &mut left, span)?;
        self.closures.snapshot(before - left, span)?;
        Ok(())
    }

}
