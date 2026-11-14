use super::*;

#[derive(Default)]
pub(super) struct Outcome {
    pub(super) fall: Option<State>,
    pub(super) breaks: Option<State>,
    pub(super) continues: Option<State>,
}

impl<'a> Engine<'a> {
    fn collect(&mut self, to: &mut Outcome, from: Outcome, b: &mut Budget) -> Check<()> {
        self.merge(&mut to.fall, from.fall, b)?;
        self.merge(&mut to.breaks, from.breaks, b)?;
        self.merge(&mut to.continues, from.continues, b)
    }
    pub(super) fn block(
        &mut self,
        body: &'a SemanticOriginalBlock,
        state: State,
        unwind: &[&'a SemanticOriginalBlock],
        loop_start: Option<usize>,
        depth: usize,
        b: &mut Budget,
    ) -> Check<Outcome> {
        self.depth(depth, b)?;
        b.spend(unwind.len().saturating_add(state.values.len()))?;
        let before = state.values.keys().cloned().collect::<BTreeSet<_>>();
        let mut deferred = unwind.to_vec();
        let start = deferred.len();
        let mut output = Outcome {
            fall: Some(state),
            ..Outcome::default()
        };
        for statement in &body.statements {
            let Some(state) = output.fall.take() else {
                break;
            };
            if let CoreStatementKind::Defer(body) = &statement.kind {
                b.spend(1)?;
                deferred.push(body);
                output.fall = Some(state);
                continue;
            }
            b.spend(deferred.len())?;
            self.error_unwind = deferred.clone();
            let next = self.statement(statement, state, &deferred, loop_start, depth + 1, b)?;
            self.collect(&mut output, next, b)?;
        }
        if let Some(state) = output.fall.take() {
            let mut state = self.unwind(state, &deferred, start, depth + 1, b)?;
            self.task_scope_cleanup(&mut state, &before, b)?;
            output.fall = Some(state);
        }
        Ok(output)
    }
    pub(super) fn unwind(
        &mut self,
        mut state: State,
        deferred: &[&'a SemanticOriginalBlock],
        start: usize,
        depth: usize,
        b: &mut Budget,
    ) -> Check<State> {
        for body in deferred
            .get(start..)
            .ok_or_else(|| bad("original protocol cleanup scope is invalid"))?
            .iter()
            .rev()
        {
            let result = self.block(body, state, &[], None, depth + 1, b)?;
            if result.breaks.is_some() || result.continues.is_some() {
                return Err(bad(
                    "original protocol cleanup transfers control outside its body",
                ));
            }
            state = result.fall.ok_or_else(|| {
                bad("original protocol cleanup returns instead of completing its caller exit")
            })?;
        }
        Ok(state)
    }
    fn statement(
        &mut self,
        statement: &'a SemanticOriginalStatement,
        mut state: State,
        deferred: &[&'a SemanticOriginalBlock],
        loop_start: Option<usize>,
        depth: usize,
        b: &mut Budget,
    ) -> Check<Outcome> {
        self.depth(depth, b)?;
        use CoreStatementKind::*;
        match &statement.kind {
            Binding {
                name, initializer, ..
            } => {
                let value = match initializer {
                    Some(value) => self.expression(value, &mut state, depth + 1, b)?,
                    None => Value::Plain,
                };
                self.bind(
                    &mut state,
                    Binder::Local {
                        definition: self.current,
                        node: statement.id,
                        name: name.clone(),
                    },
                    value,
                    b,
                )?;
            }
            TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                let value = self.expression(initializer, &mut state, depth + 1, b)?;
                self.bind_pattern(statement.id, &[], pattern, &value, &mut state, b)?;
            }
            Expression(value) => {
                self.expression(value, &mut state, depth + 1, b)?;
            }
            Assign { target, value } => {
                let binder = self.binder(target, b)?.ok_or_else(|| {
                    unsupported(
                        "original protocol assignment needs its complete projected owner place",
                        Some(target.span),
                    )
                })?;
                let value = self.expression(value, &mut state, depth + 1, b)?;
                self.bind(&mut state, binder, value, b)?;
            }
            CompoundAssign { target, value, .. } => {
                self.expression(target, &mut state, depth + 1, b)?;
                self.expression(value, &mut state, depth + 1, b)?;
            }
            Return(value) => {
                let value = match value {
                    Some(value) => self.expression(value, &mut state, depth + 1, b)?,
                    None => Value::Plain,
                };
                if let Value::Owner {
                    root,
                    borrowed: false,
                } = &value
                {
                    self.update(&mut state, root, DEAD, b)?;
                }
                let state = self.unwind(state, deferred, 0, depth + 1, b)?;
                self.check_exit(state, value, b)?;
                return Ok(Outcome::default());
            }
            If {
                condition,
                then_body,
                else_body,
            } => {
                self.expression(condition, &mut state, depth + 1, b)?;
                let yes = self.block(
                    then_body,
                    state.copy(b)?,
                    deferred,
                    loop_start,
                    depth + 1,
                    b,
                )?;
                let no = match else_body {
                    Some(body) => self.block(body, state, deferred, loop_start, depth + 1, b)?,
                    None => Outcome {
                        fall: Some(state),
                        ..Outcome::default()
                    },
                };
                let mut result = yes;
                self.collect(&mut result, no, b)?;
                return Ok(result);
            }
            IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                let observed = self.expression(value, &mut state, depth + 1, b)?;
                let poll = self.node_poll(value, b)?;
                let mut result = Outcome::default();
                for selected in self.select(&state, &observed, pattern, poll, true, b)? {
                    let mut selected = selected;
                    self.bind_pattern(statement.id, &[], pattern, &observed, &mut selected, b)?;
                    let next =
                        self.block(then_body, selected, deferred, loop_start, depth + 1, b)?;
                    self.collect(&mut result, next, b)?;
                }
                for selected in self.select(&state, &observed, pattern, poll, false, b)? {
                    let next = match else_body {
                        Some(body) => {
                            self.block(body, selected, deferred, loop_start, depth + 1, b)?
                        }
                        None => Outcome {
                            fall: Some(selected),
                            ..Outcome::default()
                        },
                    };
                    self.collect(&mut result, next, b)?;
                }
                return Ok(result);
            }
            Match { value, arms } => {
                let observed = self.expression(value, &mut state, depth + 1, b)?;
                let poll = self.node_poll(value, b)?;
                let mut covered = 0u8;
                let mut result = Outcome::default();
                for (ordinal, arm) in arms.iter().enumerate() {
                    for mut selected in
                        self.select_uncovered(&state, &observed, &arm.pattern, poll, covered, b)?
                    {
                        self.bind_pattern(
                            statement.id,
                            &[ordinal as u32],
                            &arm.pattern,
                            &observed,
                            &mut selected,
                            b,
                        )?;
                        let next =
                            self.block(&arm.body, selected, deferred, loop_start, depth + 1, b)?;
                        self.collect(&mut result, next, b)?;
                    }
                    covered |= pattern_mask(&arm.pattern, poll);
                }
                if covered != 3 {
                    for selected in self.unmatched(&state, &observed, poll, covered, b)? {
                        self.merge(&mut result.fall, Some(selected), b)?;
                    }
                }
                return Ok(result);
            }
            Block(body) | Unsafe(body) => {
                return self.block(body, state, deferred, loop_start, depth + 1, b);
            }
            Break | Continue => {
                let start = loop_start
                    .ok_or_else(|| bad("original protocol control has no actual enclosing loop"))?;
                let mut state = self.unwind(state, deferred, start, depth + 1, b)?;
                let before = self
                    .task_loop_keys
                    .last()
                    .cloned()
                    .ok_or_else(|| bad("Task loop cleanup has no exact lexical boundary"))?;
                self.task_scope_cleanup(&mut state, &before, b)?;
                return Ok(if matches!(statement.kind, Break) {
                    Outcome {
                        breaks: Some(state),
                        ..Outcome::default()
                    }
                } else {
                    Outcome {
                        continues: Some(state),
                        ..Outcome::default()
                    }
                });
            }
            Loop(body) => return self.loop_body(body, None, None, state, deferred, depth + 1, b),
            While { condition, body } => {
                return self.loop_body(body, Some(condition), None, state, deferred, depth + 1, b);
            }
            WhileLet {
                pattern,
                value,
                body,
            } => {
                return self.loop_body(
                    body,
                    Some(value),
                    Some((statement.id, pattern)),
                    state,
                    deferred,
                    depth + 1,
                    b,
                );
            }
            ForRange {
                start, end, body, ..
            } => {
                self.expression(start, &mut state, depth + 1, b)?;
                self.expression(end, &mut state, depth + 1, b)?;
                return self.range_body(body, state, deferred, depth + 1, b);
            }
            _ => {
                return Err(unsupported(
                    "original protocol statement has no complete CFG/cleanup rule",
                    Some(statement.span),
                ));
            }
        }
        Ok(Outcome {
            fall: Some(state),
            ..Outcome::default()
        })
    }
    fn range_body(
        &mut self,
        body: &'a SemanticOriginalBlock,
        state: State,
        deferred: &[&'a SemanticOriginalBlock],
        depth: usize,
        b: &mut Budget,
    ) -> Check<Outcome> {
        // Range bounds are evaluated once. An arbitrary number of body
        // executions, including zero, uses the same finite loop solver.
        self.loop_fixed(body, None, None, state, deferred, true, depth, b)
    }
    fn loop_body(
        &mut self,
        body: &'a SemanticOriginalBlock,
        condition: Option<&'a SemanticOriginalExpr>,
        pattern: Option<(NodeId, &'a SemanticOriginalPattern)>,
        state: State,
        deferred: &[&'a SemanticOriginalBlock],
        depth: usize,
        b: &mut Budget,
    ) -> Check<Outcome> {
        self.loop_fixed(
            body,
            condition,
            pattern,
            state,
            deferred,
            condition.is_some(),
            depth,
            b,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn loop_fixed(
        &mut self,
        body: &'a SemanticOriginalBlock,
        condition: Option<&'a SemanticOriginalExpr>,
        pattern: Option<(NodeId, &'a SemanticOriginalPattern)>,
        mut entry: State,
        deferred: &[&'a SemanticOriginalBlock],
        zero_exit: bool,
        depth: usize,
        b: &mut Budget,
    ) -> Check<Outcome> {
        self.depth(depth, b)?;
        b.spend(entry.values.len())?;
        self.task_loop_keys
            .push(entry.values.keys().cloned().collect());
        let result = (|| {
            entry.strip_observations(b)?;
            let mut header = entry.copy(b)?;
            let mut exits = None;
            let function = self.function()?;
            let height = function
                .bindings
                .len()
                .saturating_add(function.parameters.len())
                .saturating_add(function.expressions.len())
                .saturating_add(1)
                .saturating_mul(16);
            for _ in 0..height {
                let mut state = header.copy(b)?;
                state.strip_observations(b)?;
                let observed = condition
                    .map(|value| self.expression(value, &mut state, depth + 1, b))
                    .transpose()?
                    .unwrap_or(Value::Plain);
                let poll = condition
                    .map(|value| self.node_poll(value, b))
                    .transpose()?
                    .unwrap_or(false);
                let mut incoming = Vec::new();
                if let Some((statement, pattern)) = pattern {
                    for mut branch in self.select(&state, &observed, pattern, poll, true, b)? {
                        self.bind_pattern(statement, &[], pattern, &observed, &mut branch, b)?;
                        incoming.push(branch);
                    }
                    for branch in self.select(&state, &observed, pattern, poll, false, b)? {
                        self.merge(&mut exits, Some(branch), b)?;
                    }
                } else {
                    if zero_exit {
                        self.merge(&mut exits, Some(state.copy(b)?), b)?;
                    }
                    incoming.push(state);
                }
                let mut back = None;
                for branch in incoming {
                    let result =
                        self.block(body, branch, deferred, Some(deferred.len()), depth + 1, b)?;
                    self.merge(&mut exits, result.breaks, b)?;
                    self.merge(&mut back, result.continues, b)?;
                    self.merge(&mut back, result.fall, b)?;
                }
                let Some(mut back) = back else {
                    return Ok(Outcome {
                        fall: exits,
                        ..Outcome::default()
                    });
                };
                back.strip_observations(b)?;
                let mut next = self.join(&entry, &back, b)?;
                next.strip_observations(b)?;
                if header.same_loop(&next, b)? {
                    return Ok(Outcome {
                        fall: exits,
                        ..Outcome::default()
                    });
                }
                header = next;
            }
            Err(limit(
                "original protocol loop finite lattice did not converge within its paid height",
            ))
        })();
        self.task_loop_keys.pop();
        result
    }
    fn node_poll(&self, node: &SemanticOriginalExpr, b: &mut Budget) -> Check<bool> {
        self.original
            .expressions
            .get(&(self.current, node.id))
            .map(|ty| self.poll_type(ty, b))
            .transpose()
            .map(|value| value.unwrap_or(false))
    }
    fn select(
        &mut self,
        state: &State,
        value: &Value,
        pattern: &SemanticOriginalPattern,
        poll: bool,
        yes: bool,
        b: &mut Budget,
    ) -> Check<Vec<State>> {
        let selected = pattern_mask(pattern, poll);
        if selected == 0 {
            return Ok(vec![state.copy(b)?]);
        }
        self.select_mask(
            state,
            value,
            poll,
            if yes { selected } else { 3 & !selected },
            b,
        )
    }
    fn select_uncovered(
        &mut self,
        state: &State,
        value: &Value,
        pattern: &SemanticOriginalPattern,
        poll: bool,
        covered: u8,
        b: &mut Budget,
    ) -> Check<Vec<State>> {
        let selected = pattern_mask(pattern, poll);
        if selected == 0 {
            return if covered == 3 {
                Ok(vec![])
            } else {
                Ok(vec![state.copy(b)?])
            };
        }
        self.select_mask(state, value, poll, selected & !covered, b)
    }
    fn unmatched(
        &mut self,
        state: &State,
        value: &Value,
        poll: bool,
        covered: u8,
        b: &mut Budget,
    ) -> Check<Vec<State>> {
        self.select_mask(state, value, poll, 3 & !covered, b)
    }
    fn select_mask(
        &mut self,
        state: &State,
        value: &Value,
        poll: bool,
        mask: u8,
        b: &mut Budget,
    ) -> Check<Vec<State>> {
        if mask == 0 {
            return Ok(vec![]);
        }
        if !poll || mask == 3 {
            return Ok(vec![state.copy(b)?]);
        }
        let exit = if mask == 1 {
            ProtocolExit::PollPending
        } else {
            ProtocolExit::PollReady
        };
        let mut branch = state.copy(b)?;
        if branch.refine(value, exit, b)? {
            Ok(vec![branch])
        } else {
            Ok(vec![])
        }
    }
    fn bind_pattern(
        &mut self,
        statement: NodeId,
        path: &[u32],
        pattern: &SemanticOriginalPattern,
        value: &Value,
        state: &mut State,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(path.len().saturating_add(1))?;
        match &pattern.kind {
            CorePatternKind::Name(_) => {
                let binder = self
                    .original
                    .pattern_bindings
                    .get(&(self.current, statement, path.to_vec()))
                    .ok_or_else(|| {
                        bad("original protocol pattern has no exact checked binder occurrence")
                    })?;
                self.bind(state, binder.clone(), value.clone(), b)
            }
            CorePatternKind::Wildcard => {
                if matches!(
                    value,
                    Value::Owner { .. } | Value::Fresh | Value::FreshTask | Value::FallibleTask
                ) {
                    Err(unsupported(
                        "original wildcard owner cleanup requires a complete owner exit relation",
                        Some(pattern.span),
                    ))
                } else {
                    Ok(())
                }
            }
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                if matches!(
                    value,
                    Value::Owner { .. } | Value::Fresh | Value::FreshTask | Value::FallibleTask
                ) {
                    return Err(unsupported(
                        "original owner pattern requires its projected generation ledger",
                        Some(pattern.span),
                    ));
                }
                for (index, field) in fields.iter().enumerate() {
                    b.spend(path.len().saturating_add(1))?;
                    let mut child = path.to_vec();
                    child.push(index as u32);
                    self.bind_pattern(statement, &child, field, &Value::Plain, state, b)?;
                }
                Ok(())
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                if matches!(
                    value,
                    Value::Owner { .. } | Value::Fresh | Value::FreshTask | Value::FallibleTask
                ) {
                    return Err(unsupported(
                        "original owner record pattern requires its projected generation ledger",
                        Some(pattern.span),
                    ));
                }
                for (index, (_, field)) in fields.iter().enumerate() {
                    let mut child = path.to_vec();
                    child.push(index as u32);
                    self.bind_pattern(statement, &child, field, &Value::Plain, state, b)?;
                }
                Ok(())
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                let mut child = path.to_vec();
                child.push(0);
                let payload = if !*is_error && *value == Value::FallibleTask {
                    Value::FreshTask
                } else if *value == Value::FallibleTask {
                    Value::Plain
                } else {
                    value.clone()
                };
                self.bind_pattern(statement, &child, pattern, &payload, state, b)
            }
            CorePatternKind::Reference { pattern, .. } => {
                let mut child = path.to_vec();
                child.push(0);
                self.bind_pattern(statement, &child, pattern, value, state, b)
            }
            _ => Ok(()),
        }
    }
}
fn pattern_mask(pattern: &SemanticOriginalPattern, poll: bool) -> u8 {
    match &pattern.kind {
        CorePatternKind::Name(_) | CorePatternKind::Wildcard => 3,
        CorePatternKind::Variant { variant, .. } | CorePatternKind::Constructor { variant, .. }
            if poll =>
        {
            match variant.as_str() {
                "Pending" => 1,
                "Ready" => 2,
                _ => 0,
            }
        }
        // Other typed patterns have no protocol discriminant. Both the matching
        // and nonmatching path must remain possible until their own control proof.
        _ => 0,
    }
}
