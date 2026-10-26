//! Pattern conditions reuse checked projections and lexical local lifetimes.
use super::*;

impl Builder<'_, '_> {
    fn pattern_scope(&mut self, bindings: Vec<(String, ValueId, Type, Span)>, span: Span) -> R<()> {
        self.push_scope(span)?;
        for (name, value, ty, span) in bindings {
            let local = self.local(&name, ty, false, span)?;
            self.store(local, value, span)?;
        }
        Ok(())
    }
    pub(super) fn if_let(
        &mut self,
        pattern: &CorePattern,
        expression: &CoreExpr,
        then_body: &CoreBlock,
        else_body: Option<&CoreBlock>,
        span: Span,
    ) -> R<()> {
        let (value, ty) = self.expr(expression, None)?;
        let irrefutable = self.conditional_pattern(pattern, &ty)?;
        self.finish_resource_temps(expression.span)?;
        let fail = self.block_id()?;
        let mut bindings = Vec::new();
        self.pattern_test(pattern, value, &ty, fail, &mut bindings)?;
        self.pattern_scope(bindings, span)?;
        self.block(then_body)?;
        if self.current.is_some() {
            self.end_scopes(self.scopes.len() - 1, span)?;
        }
        let then_end = self.current.take();
        self.pop_scope();
        self.current = Some(fail);
        if let Some(body) = else_body {
            self.block(body)?;
        }
        let else_end = if irrefutable {
            // A defensive tag failure is a trap, never a reachable LLVM undef exit.
            if self.current.is_some() {
                self.terminate(Terminator::Trap);
            }
            None
        } else {
            self.current.take()
        };
        if then_end.is_some() || else_end.is_some() {
            let join = self.block_id()?;
            for end in [then_end, else_end].into_iter().flatten() {
                self.function.blocks[end.0 as usize].terminator = Terminator::Jump(join);
            }
            self.current = Some(join);
        }
        Ok(())
    }
    pub(super) fn while_let(
        &mut self,
        pattern: &CorePattern,
        expression: &CoreExpr,
        body: &CoreBlock,
        span: Span,
    ) -> R<()> {
        let head = self.block_id()?;
        let after = self.block_id()?;
        self.terminate(Terminator::Jump(head));
        self.current = Some(head);
        let (value, ty) = self.expr(expression, None)?;
        let irrefutable = self.conditional_pattern(pattern, &ty)?;
        self.finish_resource_temps(expression.span)?;
        let mut bindings = Vec::new();
        self.pattern_test(pattern, value, &ty, after, &mut bindings)?;
        self.loops.push(LoopContext {
            continue_target: head,
            break_target: after,
            has_break: false,
            has_continue: false,
            scope_depth: self.scopes.len(),
            unsafe_depth: self.unsafe_regions.len(),
        });
        self.pattern_scope(bindings, span)?;
        self.block(body)?;
        if self.current.is_some() {
            self.end_scopes(self.scopes.len() - 1, span)?;
            self.terminate(Terminator::Jump(head));
        }
        self.pop_scope();
        let context = self.loops.pop().expect("pattern loop context");
        self.current = Some(after);
        if irrefutable && !context.has_break {
            self.terminate(Terminator::Trap);
        }
        Ok(())
    }
}
