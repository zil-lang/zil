//! Affine loop induction is checked in the original declaration context.
//! A live back edge must restore every non-Copy outer occupant; a selected
//! runtime iteration count is never a premise for the original body.
use super::*;

pub(super) struct LoopFrame {
    owner: SemanticDefinitionId,
    prefix: usize,
    breaks: Vec<Vec<Local>>,
    continues: Vec<Vec<Local>>,
}
impl Solver<'_, '_> {
    pub(super) fn loop_jump(
        &mut self,
        frame: &mut Frame,
        is_break: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let index = self
            .loops
            .len()
            .checked_sub(1)
            .ok_or_else(|| fail("original jump has no enclosing loop"))?;
        let flow = &self.loops[index];
        if flow.owner != frame.definition || frame.locals.len() < flow.prefix {
            return Err(fail(
                "original jump crosses a function or lexical binding boundary",
            ));
        }
        if frame.alive {
            let prefix = flow.prefix;
            self.cleanup(&frame.locals[prefix..], b)?;
            let locals = self.copy_locals(&frame.locals[..prefix], b)?;
            let target = if is_break {
                &mut self.loops[index].breaks
            } else {
                &mut self.loops[index].continues
            };
            Self::push(target, locals, b)?;
        }
        frame.alive = false;
        Ok(())
    }

    pub(super) fn loop_statement(
        &mut self,
        frame: &mut Frame,
        condition: Option<&SemanticOriginalExpr>,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.loop_body(frame, condition, None, body, b, depth)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern_loop(
        &mut self,
        frame: &mut Frame,
        statement: NodeId,
        pattern: &SemanticOriginalPattern,
        value: &SemanticOriginalExpr,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.loop_body(
            frame,
            Some(value),
            Some((statement, pattern)),
            body,
            b,
            depth,
        )
    }
    fn loop_body(
        &mut self,
        frame: &mut Frame,
        condition: Option<&SemanticOriginalExpr>,
        pattern: Option<(NodeId, &SemanticOriginalPattern)>,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let entered = frame.alive;
        let before = self.copy_locals(&frame.locals, b)?;
        let mut subject = None;
        if let Some(condition) = condition {
            let term = self.expression(frame, condition, b, depth + 1)?;
            if pattern.is_none() {
                let boolean = self.atom(Type::Bool, b)?;
                self.unify(term, boolean, depth + 1, b)?;
            } else {
                subject = Some(term);
            }
        }
        let after_condition = self.copy_locals(&frame.locals, b)?;
        if let Some((statement, pattern)) = pattern {
            let subject =
                subject.ok_or_else(|| fail("original pattern loop has no condition value"))?;
            self.pattern_bind(
                frame,
                statement,
                &mut Vec::new(),
                pattern,
                subject,
                b,
                depth + 1,
            )?;
            Self::push(
                &mut self.needs,
                Need {
                    term: subject,
                    predicate: Predicate::SyncDrop,
                },
                b,
            )?;
        }
        Self::push(
            &mut self.loops,
            LoopFrame {
                owner: frame.definition,
                prefix: before.len(),
                breaks: Vec::new(),
                continues: Vec::new(),
            },
            b,
        )?;
        self.block(frame, body, b, depth + 1)?;
        let mut flow = self
            .loops
            .pop()
            .ok_or_else(|| fail("original loop loses its control frame"))?;
        if frame.alive {
            self.cleanup(&frame.locals[before.len()..], b)?;
            frame.locals.truncate(before.len());
            let locals = self.copy_locals(&frame.locals, b)?;
            Self::push(&mut flow.continues, locals, b)?;
        }
        // The first test/body was proved with the incoming initialization and
        // move state. Re-establish that same precondition on every back edge.
        for edge in &flow.continues {
            if edge.len() != before.len() {
                return Err(fail(
                    "original loop changes its enclosing binding inventory",
                ));
            }
            for (old, next) in before.iter().zip(edge) {
                b.spend(1, WORK)?;
                if old.initialized && !next.initialized {
                    return Err(fail(
                        "original loop back edge loses an initialized outer occupant",
                    ));
                }
                if old.initialized && !old.used && next.used {
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: old.ty,
                            predicate: Predicate::Copy,
                        },
                        b,
                    )?;
                }
            }
        }
        let mut exits = flow.breaks;
        if entered && condition.is_some() {
            // Zero iterations and each later false test are possible. Repeated
            // tests have the same type/affine preconditions just proved above.
            let zero_exit = self.copy_locals(&after_condition, b)?;
            Self::push(&mut exits, zero_exit, b)?;
            for edge in flow.continues {
                let mut tested = edge;
                for ((old, tested), condition) in
                    before.iter().zip(&mut tested).zip(&after_condition)
                {
                    b.spend(1, WORK)?;
                    tested.used |= condition.used && !old.used;
                    tested.initialized &= condition.initialized;
                }
                Self::push(&mut exits, tested, b)?;
            }
        }
        if !entered || exits.is_empty() {
            frame.locals = before;
            frame.alive = false;
            return Ok(());
        }
        let mut exits = exits.into_iter();
        frame.locals = exits
            .next()
            .ok_or_else(|| fail("original loop loses its first exit"))?;
        for exit in exits {
            if exit.len() != frame.locals.len() {
                return Err(fail("original loop exit changes its lexical inventory"));
            }
            for (local, other) in frame.locals.iter_mut().zip(exit) {
                b.spend(1, WORK)?;
                local.used |= other.used;
                local.initialized &= other.initialized;
            }
        }
        frame.alive = true;
        Ok(())
    }
}
