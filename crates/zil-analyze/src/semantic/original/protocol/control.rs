//! Finite protocol fixed points over the original control graph. Source tags
//! select post-state relations only after the complete original Poll identity
//! was checked; a spelling or a selected runtime branch is never a premise.
use super::*;

pub(super) struct LoopFlow {
    scope: usize,
    breaks: Vec<State>,
    continues: Vec<State>,
}
impl Proof<'_, '_> {
    pub(super) fn jump(
        &self,
        state: &mut State,
        is_break: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let scope = self
            .loops
            .borrow()
            .last()
            .map(|flow| flow.scope)
            .ok_or_else(|| fail("original protocol jump has no actual enclosing loop"))?;
        let mut exit = state.copy(b)?;
        if exit.locals.len() < scope {
            return Err(fail("original protocol jump crosses a lexical boundary"));
        }
        exit.locals.truncate(scope);
        let mut loops = self.loops.borrow_mut();
        let flow = loops
            .last_mut()
            .ok_or_else(|| fail("original protocol jump loses its loop frame"))?;
        Solver::push(
            if is_break {
                &mut flow.breaks
            } else {
                &mut flow.continues
            },
            exit,
            b,
        )?;
        state.alive = false;
        Ok(())
    }
    pub(super) fn loop_block(
        &self,
        state: &mut State,
        condition: Option<&SemanticOriginalExpr>,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        let mut header = state.copy(b)?;
        loop {
            self.step(b, depth + 1)?;
            let old = header.copy(b)?;
            let mut iteration = header.copy(b)?;
            let mut exits = Vec::new();
            if let Some(condition) = condition {
                let result = self.expression(&mut iteration, condition, b, depth + 1)?;
                self.plain(&result)?;
                let exit = iteration.copy(b)?;
                Solver::push(&mut exits, exit, b)?;
            }
            let scope = iteration.locals.len();
            Solver::push(
                &mut self.loops.borrow_mut(),
                LoopFlow {
                    scope,
                    breaks: Vec::new(),
                    continues: Vec::new(),
                },
                b,
            )?;
            self.block(&mut iteration, body, b, depth + 1)?;
            let mut flow = self
                .loops
                .borrow_mut()
                .pop()
                .ok_or_else(|| fail("original protocol loop loses its control frame"))?;
            if iteration.alive {
                Solver::push(&mut flow.continues, iteration, b)?;
            }
            for back in flow.continues {
                header.join(&back, b)?;
            }
            for exit in flow.breaks {
                Solver::push(&mut exits, exit, b)?;
            }
            // The complete state was prepaid by copy above. Increasing union
            // reaches the least finite invariant or consumes the caller budget.
            b.spend(
                header.phases.len().saturating_add(header.locals.len() + 1),
                WORK,
            )?;
            if header == old {
                let mut exits = exits.into_iter();
                if let Some(first) = exits.next() {
                    *state = first;
                    for exit in exits {
                        state.join(&exit, b)?;
                    }
                } else {
                    state.alive = false;
                }
                return Ok(());
            }
        }
    }
    fn bind_pattern(
        &self,
        state: &mut State,
        pattern: &SemanticOriginalPattern,
        data: &Data,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                b.spend(name.len().saturating_add(data.outcomes.len()), WORK)?;
                Solver::push(&mut state.locals, (name.clone(), data.clone()), b)
            }
            CorePatternKind::Wildcard | CorePatternKind::Bool(_) | CorePatternKind::Integer(_) => {
                Ok(())
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                let empty = Data::default();
                self.bind_pattern(
                    state,
                    pattern,
                    if *is_error { &empty } else { data },
                    b,
                    depth + 1,
                )
            }
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Constructor { fields, .. }
            | CorePatternKind::Variant { fields, .. } => {
                for field in fields {
                    self.bind_pattern(state, field, &Data::default(), b, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(unknown(
                "original protocol pattern needs its exact projected owning-head identity",
            )),
        }
    }
    fn poll_identity(&self, node: NodeId, b: &mut Budget) -> CompileResult<SemanticDefinitionId> {
        b.lookup(self.checked.frames.len(), WORK)?;
        let frame = self
            .checked
            .frames
            .get(&self.definition.id)
            .ok_or_else(|| fail("original protocol match has no checked type frame"))?;
        b.lookup(frame.expressions.len(), WORK)?;
        let mut term = *frame
            .expressions
            .get(&node)
            .ok_or_else(|| fail("original protocol match loses its exact expression type"))?;
        for _ in 0..=b.partial_depth {
            term = self.checked.root(term, b)?;
            if let Kind::CallResult { hidden, .. } = self.checked.arena[term] {
                term = hidden;
                continue;
            }
            let Kind::Nominal { definition, .. } = self.checked.arena[term] else {
                return Err(fail(
                    "original protocol outcome has no actual Poll nominal type",
                ));
            };
            let declaration = self.inventory.definition(definition, b)?;
            if !matches!(declaration.body, SemanticDefinitionBody::Nominal { declaration, kind: LifeNominalKind::Enum, .. } if declaration == zil_ir::builtin_life::POLL)
            {
                return Err(fail(
                    "original protocol outcome selects another nominal declaration",
                ));
            }
            return Ok(definition);
        }
        Err(resource(
            "original protocol result identity exceeds its finite depth",
        ))
    }
    pub(super) fn match_block(
        &self,
        state: &mut State,
        value: &SemanticOriginalExpr,
        arms: &[SemanticOriginalMatchArm],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        let data = self.expression(state, value, b, depth + 1)?;
        let poll = if data.outcomes.is_empty() {
            None
        } else {
            Some(self.poll_identity(value.id, b)?)
        };
        let base = state.copy(b)?;
        let scope = base.locals.len();
        let mut joined = base.copy(b)?;
        joined.alive = false;
        let mut covered = BTreeSet::new();
        for arm in arms {
            let selected = if let Some(poll) = poll {
                match &arm.pattern.kind {
                    CorePatternKind::Wildcard | CorePatternKind::Name(_) => None,
                    CorePatternKind::Constructor {
                        definition,
                        variant,
                        ..
                    } => {
                        if definition.is_some_and(|definition| definition != poll) {
                            return Err(fail(
                                "original protocol pattern names another actual Poll definition",
                            ));
                        }
                        Some(match variant.as_str() {
                            "Pending" => ProtocolExit::PollPending,
                            "Ready" => ProtocolExit::PollReady,
                            _ => {
                                return Err(fail(
                                    "original protocol pattern selects no actual Poll variant",
                                ));
                            }
                        })
                    }
                    CorePatternKind::Variant { variant, .. } => Some(match variant.as_str() {
                        "Pending" => ProtocolExit::PollPending,
                        "Ready" => ProtocolExit::PollReady,
                        _ => {
                            return Err(fail(
                                "original protocol pattern selects no actual Poll variant",
                            ));
                        }
                    }),
                    _ => {
                        return Err(unknown(
                            "original protocol outcome needs its exact full Poll pattern",
                        ));
                    }
                }
            } else {
                None
            };
            let mut branches = Vec::new();
            if poll.is_some() {
                for (exit, phases) in &data.outcomes {
                    b.spend(phases.len().saturating_add(1), WORK)?;
                    if selected.is_some_and(|selected| selected != *exit) {
                        continue;
                    }
                    let mut branch = base.copy(b)?;
                    branch.phases = phases.clone();
                    b.lookup(covered.len(), WORK)?;
                    covered.insert(*exit);
                    Solver::push(&mut branches, branch, b)?;
                }
            } else {
                let mut branch = base.copy(b)?;
                if matches!(
                    arm.pattern.kind,
                    CorePatternKind::ErrorArm { is_error: true, .. }
                ) && let Some(role) = data.alias
                {
                    branch.phases.insert(role, 0);
                }
                Solver::push(&mut branches, branch, b)?;
            }
            for mut branch in branches {
                let empty = Data::default();
                self.bind_pattern(
                    &mut branch,
                    &arm.pattern,
                    if poll.is_none() { &data } else { &empty },
                    b,
                    depth + 1,
                )?;
                self.block(&mut branch, &arm.body, b, depth + 1)?;
                branch.locals.truncate(scope);
                joined.join(&branch, b)?;
            }
        }
        for (exit, _) in &data.outcomes {
            b.lookup(covered.len(), WORK)?;
            if !covered.contains(exit) {
                return Err(fail(
                    "original protocol match loses a possible actual Poll outcome",
                ));
            }
        }
        *state = joined;
        Ok(())
    }
}
