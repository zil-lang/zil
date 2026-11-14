//! Finite protocol fixed points over the original control graph. Source tags
//! select post-state relations only after the complete original Poll identity
//! was checked; a spelling or a selected runtime branch is never a premise.
use super::*;
#[derive(Clone, Copy)]
enum Carrier {
    Poll(SemanticDefinitionId),
    Error,
}

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
        self.loop_flow(state, condition, body, condition.is_some(), b, depth)
    }
    pub(super) fn loop_flow(
        &self,
        state: &mut State,
        condition: Option<&SemanticOriginalExpr>,
        body: &SemanticOriginalBlock,
        may_finish: bool,
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
            }
            if may_finish {
                Solver::push(&mut exits, iteration.copy(b)?, b)?;
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
    pub(super) fn bind_pattern(
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
                let payload = Data {
                    alias: data.alias,
                    exposed: data.exposed,
                    outcomes: Vec::new(),
                };
                self.bind_pattern(
                    state,
                    pattern,
                    if *is_error { &empty } else { &payload },
                    b,
                    depth + 1,
                )
            }
            CorePatternKind::Reference { pattern, .. } => {
                self.bind_pattern(state, pattern, data, b, depth + 1)
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                if data.alias.is_some() {
                    return Err(unknown(
                        "original protocol product pattern needs its complete nested owner-place mapping",
                    ));
                }
                for (_, field) in fields {
                    self.bind_pattern(
                        state,
                        field,
                        &Data {
                            alias: None,
                            exposed: data.exposed,
                            outcomes: Vec::new(),
                        },
                        b,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Constructor { fields, .. }
            | CorePatternKind::Variant { fields, .. } => {
                if data.alias.is_some() {
                    return Err(unknown(
                        "original protocol sum/tuple pattern needs its complete nested owner-place mapping",
                    ));
                }
                for field in fields {
                    self.bind_pattern(
                        state,
                        field,
                        &Data {
                            alias: None,
                            exposed: data.exposed,
                            outcomes: Vec::new(),
                        },
                        b,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            _ => Err(unknown(
                "original protocol pattern needs its exact projected owning-head identity",
            )),
        }
    }
    fn outcome_carrier(&self, node: NodeId, b: &mut Budget) -> CompileResult<Carrier> {
        let term = self.expression_term(node, b)?;
        if let Some(definition) = self.term_poll(term, b)? {
            return Ok(Carrier::Poll(definition));
        }
        if matches!(self.checked.arena[term], Kind::ErrorUnion { .. }) {
            return Ok(Carrier::Error);
        }
        Err(fail(
            "original protocol outcomes have no matching complete Poll or ErrorUnion carrier",
        ))
    }
    fn arm_exit(
        &self,
        carrier: Carrier,
        pattern: &SemanticOriginalPattern,
        b: &mut Budget,
    ) -> CompileResult<Option<ProtocolExit>> {
        match (&pattern.kind, carrier) {
            (CorePatternKind::Wildcard | CorePatternKind::Name(_), _) => Ok(None),
            (CorePatternKind::ErrorArm { is_error, .. }, Carrier::Error) => {
                Ok(Some(if *is_error {
                    ProtocolExit::ErrorErr
                } else {
                    ProtocolExit::ErrorOk
                }))
            }
            (
                CorePatternKind::Constructor {
                    definition,
                    variant,
                    ..
                },
                Carrier::Poll(poll),
            ) => {
                if definition.is_some_and(|definition| definition != poll) {
                    return Err(fail(
                        "original protocol pattern names another canonical Poll definition",
                    ));
                }
                match variant.as_str() {
                    "Pending" => Ok(Some(ProtocolExit::PollPending)),
                    "Ready" => Ok(Some(ProtocolExit::PollReady)),
                    _ => Err(fail(
                        "original protocol pattern selects no complete Poll alternative",
                    )),
                }
            }
            (CorePatternKind::Variant { ty, variant, .. }, Carrier::Poll(poll)) => {
                let SemanticTypeRecipe::Nominal { definition, .. } = ty else {
                    return Err(fail(
                        "original explicit Poll pattern has no nominal declaration",
                    ));
                };
                if *definition != poll || self.inventory.protocol_poll_recipe(ty, b)?.is_none() {
                    return Err(fail(
                        "original explicit Poll pattern changes its complete canonical owner",
                    ));
                }
                match variant.as_str() {
                    "Pending" => Ok(Some(ProtocolExit::PollPending)),
                    "Ready" => Ok(Some(ProtocolExit::PollReady)),
                    _ => Err(fail(
                        "original protocol pattern selects no complete Poll alternative",
                    )),
                }
            }
            _ => Err(unknown(
                "original protocol outcome needs its exact complete carrier pattern",
            )),
        }
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
        let carrier = if data.outcomes.is_empty() {
            None
        } else {
            Some(self.outcome_carrier(value.id, b)?)
        };
        let base = state.copy(b)?;
        let scope = base.locals.len();
        let mut joined = base.copy(b)?;
        joined.alive = false;
        let mut covered = BTreeSet::new();
        for arm in arms {
            let selected = if let Some(carrier) = carrier {
                self.arm_exit(carrier, &arm.pattern, b)?
            } else {
                None
            };
            let mut branches = Vec::new();
            if carrier.is_some() {
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
                let payload = Data {
                    alias: data.alias,
                    exposed: data.exposed,
                    outcomes: Vec::new(),
                };
                self.bind_pattern(
                    &mut branch,
                    &arm.pattern,
                    if carrier.is_none() || matches!(selected, Some(ProtocolExit::ErrorOk)) {
                        &payload
                    } else {
                        &empty
                    },
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
                    "original protocol match loses a possible actual carrier outcome",
                ));
            }
        }
        *state = joined;
        Ok(())
    }
}

impl Proof<'_, '_> {
    fn pattern_states(
        &self,
        state: &State,
        data: &Data,
        node: NodeId,
        pattern: &SemanticOriginalPattern,
        b: &mut Budget,
    ) -> CompileResult<(Vec<State>, Vec<State>)> {
        let mut yes = Vec::new();
        let mut no = Vec::new();
        let total = matches!(
            pattern.kind,
            CorePatternKind::Wildcard | CorePatternKind::Name(_)
        );
        if !data.outcomes.is_empty() {
            let carrier = self.outcome_carrier(node, b)?;
            let selected = self.arm_exit(carrier, pattern, b)?;
            for (exit, phases) in &data.outcomes {
                let mut branch = state.copy(b)?;
                b.spend(phases.len(), WORK)?;
                branch.phases = phases.clone();
                Solver::push(
                    if selected.is_none_or(|selected| selected == *exit) {
                        &mut yes
                    } else {
                        &mut no
                    },
                    branch,
                    b,
                )?;
            }
        } else {
            let mut matched = state.copy(b)?;
            let mut unmatched = state.copy(b)?;
            if let CorePatternKind::ErrorArm { is_error, .. } = pattern.kind
                && let Some(role) = data.alias
            {
                if is_error {
                    matched.phases.insert(role, 0);
                } else {
                    unmatched.phases.insert(role, 0);
                }
            }
            Solver::push(&mut yes, matched, b)?;
            if !total {
                Solver::push(&mut no, unmatched, b)?;
            }
        }
        Ok((yes, no))
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn if_let_block(
        &self,
        state: &mut State,
        pattern: &SemanticOriginalPattern,
        value: &SemanticOriginalExpr,
        then_body: &SemanticOriginalBlock,
        else_body: Option<&SemanticOriginalBlock>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        let data = self.expression(state, value, b, depth + 1)?;
        let (yes, no) = self.pattern_states(state, &data, value.id, pattern, b)?;
        let scope = state.locals.len();
        let mut joined = state.copy(b)?;
        joined.alive = false;
        for mut branch in yes {
            let payload = Data {
                alias: data.alias,
                exposed: data.exposed,
                outcomes: Vec::new(),
            };
            self.bind_pattern(&mut branch, pattern, &payload, b, depth + 1)?;
            self.block(&mut branch, then_body, b, depth + 1)?;
            branch.locals.truncate(scope);
            joined.join(&branch, b)?;
        }
        for mut branch in no {
            if let Some(body) = else_body {
                self.block(&mut branch, body, b, depth + 1)?;
            }
            joined.join(&branch, b)?;
        }
        *state = joined;
        Ok(())
    }
    pub(super) fn while_let_block(
        &self,
        state: &mut State,
        pattern: &SemanticOriginalPattern,
        value: &SemanticOriginalExpr,
        body: &SemanticOriginalBlock,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(b, depth)?;
        let mut header = state.copy(b)?;
        loop {
            let old = header.copy(b)?;
            let mut iteration = header.copy(b)?;
            let data = self.expression(&mut iteration, value, b, depth + 1)?;
            let (yes, mut exits) = self.pattern_states(&iteration, &data, value.id, pattern, b)?;
            let scope = iteration.locals.len();
            for mut branch in yes {
                let payload = Data {
                    alias: data.alias,
                    exposed: data.exposed,
                    outcomes: Vec::new(),
                };
                self.bind_pattern(&mut branch, pattern, &payload, b, depth + 1)?;
                Solver::push(
                    &mut self.loops.borrow_mut(),
                    LoopFlow {
                        scope,
                        breaks: Vec::new(),
                        continues: Vec::new(),
                    },
                    b,
                )?;
                self.block(&mut branch, body, b, depth + 1)?;
                let mut flow = self.loops.borrow_mut().pop().ok_or_else(|| {
                    fail("original protocol pattern loop loses its exact control frame")
                })?;
                branch.locals.truncate(scope);
                if branch.alive {
                    Solver::push(&mut flow.continues, branch, b)?;
                }
                for back in flow.continues {
                    header.join(&back, b)?;
                }
                for exit in flow.breaks {
                    Solver::push(&mut exits, exit, b)?;
                }
            }
            b.spend(
                header.phases.len().saturating_add(header.locals.len() + 1),
                WORK,
            )?;
            if header == old {
                let mut all = exits.into_iter();
                if let Some(first) = all.next() {
                    *state = first;
                    for exit in all {
                        state.join(&exit, b)?;
                    }
                } else {
                    state.alive = false;
                }
                return Ok(());
            }
        }
    }
}
