use super::*;

/// Only the original checked scrutinee's exact sum kind supplies coverage.
/// ErrorUnion arm coverage never refines the unrelated Poll phase relation.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum Discriminant{Other,Poll,ErrorUnion}

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
                let actual=self.expression(value, &mut state, depth + 1, b)?;
                if tasks::owner_value(&actual){
                    let binder=Binder::Local{definition:self.current,node:value.id,name:"$original_overwrite".into()};
                    self.bind(&mut state,binder.clone(),actual,b)?;
                    let actual=state.values.get(&binder).cloned().ok_or_else(||bad("discarded Task value loses its exact temporary responsibility"))?;
                    self.task_cleanup_value(&mut state,&actual,None,b)?;state.values.remove(&binder);
                }
            }
            Assign { target, value } => {
                if let Some(place) = self.task_place(target, b)? {
                    let value = self.expression(value, &mut state, depth + 1, b)?;
                    self.task_assign_place(&mut state, &place, value, b)?;
                } else {
                    // The original place checker resolved this exact target,
                    // including dereferences and field selection. A scalar
                    // write cannot create, replace or retire a Task owner.
                    b.spend(self.original.place_types.len().checked_ilog2().map_or(1, |n| n as usize + 1))?;
                    if !matches!(self.original.place_types.get(&(self.current, target.id)),
                        Some(Ty::Atom(ty)) if ty.is_b0_scalar()) {
                        return Err(unsupported(
                            "original protocol assignment needs its complete projected owner place",
                            Some(target.span),
                        ));
                    }
                    let destination = self.expression(target, &mut state, depth + 1, b)?;
                    let source = self.expression(value, &mut state, depth + 1, b)?;
                    if !matches!((destination, source), (Value::Plain, Value::Plain)) {
                        return Err(bad("scalar assignment changes an original protocol responsibility"));
                    }
                }
            }
            CompoundAssign { target, value, .. } => {
                self.expression(target, &mut state, depth + 1, b)?;
                self.expression(value, &mut state, depth + 1, b)?;
            }
            Return(value) => {
                let source=value.as_ref();
                let value = match value {
                    Some(value) => self.expression(value, &mut state, depth + 1, b)?,
                    None => Value::Plain,
                };
                self.record_result_sources(&state,&value,source,b)?;
                self.task_transfer_value(&mut state, &value, b)?;
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
                let poll = self.node_discriminant(value, b)?;
                let mut result = Outcome::default();
                for selected in self.select(&state, &observed, pattern, poll, true, b)? {
                    let mut selected = selected;
                    self.bind_pattern(statement.id, &[], pattern, &observed, &mut selected, b)?;
                    self.consume_fallible_task_arm(value, poll, pattern, &observed, &mut selected, b)?;
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
                let poll = self.node_discriminant(value, b)?;
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
                        self.consume_fallible_task_arm(value, poll, &arm.pattern, &observed, &mut selected, b)?;
                        let next =
                            self.block(&arm.body, selected, deferred, loop_start, depth + 1, b)?;
                        self.collect(&mut result, next, b)?;
                    }
                    covered |= value_pattern_mask(&observed, &arm.pattern, poll, b)?;
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
            ForEach{iterator,body,..}=>{
                let mut range=iterator;while let CoreExprKind::Group(inner)=&range.kind{b.spend(1)?;range=inner;}
                let CoreExprKind::Range{start,end}=&range.kind else{return Err(unsupported("original iteration protocol lacks its complete range/iterator body",Some(iterator.span)));};
                self.expression(start,&mut state,depth+1,b)?;
                self.expression(end,&mut state,depth+1,b)?;
                return self.range_body(body,state,deferred,depth+1,b);
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
            condition.is_some_and(|expr| !matches!(expr.kind,
                CoreExprKind::Bool(true) | CoreExprKind::Literal(Constant::Bool(true)))),
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
                    .map(|value| self.node_discriminant(value, b))
                    .transpose()?
                    .unwrap_or(Discriminant::Other);
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
    // On allocation failure there is no child frame. A successful by-value
    // binding already owns the newly instantiated generation. Remove the
    // conditional responsibility only from this actual matched source place.
    fn consume_fallible_task_arm(&self, source: &SemanticOriginalExpr, sum: Discriminant,
        pattern: &SemanticOriginalPattern, observed: &Value, state: &mut State, b: &mut Budget) -> Check<()> {
        if matches!(observed, Value::Variant { .. }) {
            let mut roots = Vec::new();
            tasks::task_roots(observed, &mut roots, b)?;
            if roots.iter().any(|(_, borrowed)| !borrowed)
                && let Some(place) = self.task_place(source, b)?
            {
                self.task_take_place(state, &place, b)?;
            }
            return Ok(());
        }
        if sum != Discriminant::ErrorUnion { return Ok(()); }
        let CorePatternKind::ErrorArm { is_error, pattern: payload } = &pattern.kind else { return Ok(()); };
        let bound = matches!(payload.kind, CorePatternKind::Name(_));
        let consumed = match observed {
            Value::FallibleTask(_) => *is_error || bound,
            Value::FallibleTaskOwned(_, _) => bound,
            _ => false,
        };
        if !consumed { return Ok(()); }
        let Some(place) = self.task_place(source, b)? else { return Ok(()); };
        if !place.fields.is_empty() { return Ok(()); }
        b.spend(observed.width().saturating_add(state.values.len().saturating_add(1).ilog2() as usize + 1))?;
        if state.values.get(&place.binder) != Some(observed) {
            return Err(bad("conditional Task arm changes its actual source binding"));
        }
        self.task_take_place(state, &place, b)
    }

    fn node_discriminant(&self,node:&SemanticOriginalExpr,b:&mut Budget)->Check<Discriminant>{
        let Some(ty)=self.original.expressions.get(&(self.current,node.id))else{return Ok(Discriminant::Other);};
        if matches!(self.original.resolved(ty,b)?,Ty::ErrorUnion(..)){return Ok(Discriminant::ErrorUnion);}
        Ok(if self.poll_type(ty,b)?{Discriminant::Poll}else{Discriminant::Other})
    }
    fn select(
        &mut self,
        state: &State,
        value: &Value,
        pattern: &SemanticOriginalPattern,
        poll: Discriminant,
        yes: bool,
        b: &mut Budget,
    ) -> Check<Vec<State>> {
        if selected_owner_variant(value, pattern, b)? == Some(false) {
            return if yes { Ok(vec![]) } else { Ok(vec![state.copy(b)?]) };
        }
        let selected = value_pattern_mask(value, pattern, poll, b)?;
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
        poll: Discriminant,
        covered: u8,
        b: &mut Budget,
    ) -> Check<Vec<State>> {
        if selected_owner_variant(value, pattern, b)? == Some(false) {
            return Ok(vec![]);
        }
        let selected = value_pattern_mask(value, pattern, poll, b)?;
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
        poll: Discriminant,
        covered: u8,
        b: &mut Budget,
    ) -> Check<Vec<State>> {
        self.select_mask(state, value, poll, 3 & !covered, b)
    }
    fn select_mask(
        &mut self,
        state: &State,
        value: &Value,
        poll: Discriminant,
        mask: u8,
        b: &mut Budget,
    ) -> Check<Vec<State>> {
        if mask == 0 {
            return Ok(vec![]);
        }
        if poll!=Discriminant::Poll || mask == 3 {
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
                if matches!(value, Value::Product(_) | Value::Variant { .. }) {
                    return self.task_cleanup_value(state, value, None, b);
                }
                if matches!(
                    value,
                    Value::Owner { .. }
                        | Value::Fresh
                        | Value::FreshTask(_)
                        | Value::FallibleTask(_)
                        | Value::FallibleTaskOwned(_,_)
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
                    Value::Owner { .. }
                        | Value::Fresh
                        | Value::FreshTask(_)
                        | Value::FallibleTask(_)
                        | Value::FallibleTaskOwned(_,_)
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
                    let actual = match value {
                        Value::Product(values) => values.get(index).map(|(_, value)| value).ok_or_else(|| {
                            bad("Task product pattern changes its complete original arity")
                        })?,
                        Value::Variant { fields: values, .. } => {
                            if !selected_owner_variant(value, pattern, b)?.unwrap_or(false) || values.len() != fields.len() {
                                return Err(bad("original owner pattern does not select its exact active variant and fields"));
                            }
                            &values[index].1
                        }
                        _ => &Value::Plain,
                    };
                    self.bind_pattern(statement, &child, field, actual, state, b)?;
                }
                Ok(())
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                if matches!(
                    value,
                    Value::Owner { .. }
                        | Value::Fresh
                        | Value::FreshTask(_)
                        | Value::FallibleTask(_)
                        | Value::FallibleTaskOwned(_,_)
                ) {
                    return Err(unsupported(
                        "original owner record pattern requires its projected generation ledger",
                        Some(pattern.span),
                    ));
                }
                for (index, (name, field)) in fields.iter().enumerate() {
                    let mut child = path.to_vec();
                    child.push(index as u32);
                    let actual = if let Value::Product(values) = value {
                        values
                            .iter()
                            .find(|(key, _)| key == name)
                            .map(|(_, value)| value)
                            .ok_or_else(|| {
                                bad("Task record pattern selects another original field")
                            })?
                    } else {
                        &Value::Plain
                    };
                    self.bind_pattern(statement, &child, field, actual, state, b)?;
                }
                Ok(())
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                let mut child = path.to_vec();
                child.push(0);
                let payload = self.task_failure_payload(state, value, *is_error, b)?;
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
fn irrefutable_payload(pattern:&SemanticOriginalPattern,b:&mut Budget)->Check<bool>{
    let mut pending=vec![(pattern,0usize)];
    while let Some((pattern,depth))=pending.pop(){
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original sum pattern depth exceeded"));}
        match &pattern.kind{
            CorePatternKind::Name(_)|CorePatternKind::Wildcard=>{},
            CorePatternKind::Tuple(fields)=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|field|(field,depth+1)));},
            CorePatternKind::Record{fields,..}|CorePatternKind::RecordConstructor{fields,..}=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|(_,field)|(field,depth+1)));},
            CorePatternKind::Reference{pattern,..}=>pending.push((pattern,depth+1)),
            // Literal, variant and nested ErrorUnion selectors can fail. Their
            // unmatched route remains until a separate complete proof covers it.
            _=>return Ok(false),
        }
    }Ok(true)
}
fn selected_owner_variant(value: &Value, pattern: &SemanticOriginalPattern, b: &mut Budget) -> Check<Option<bool>> {
    let Value::Variant { definition, name, .. } = value else { return Ok(None); };
    let (declared, variant) = match &pattern.kind {
        CorePatternKind::Constructor { definition, variant, .. } => (*definition, variant),
        CorePatternKind::Variant { ty: SemanticTypeRecipe::Nominal { definition, .. }, variant, .. } => (Some(*definition), variant),
        _ => return Ok(None),
    };
    b.spend(variant.len().saturating_add(name.len()).saturating_add(1))?;
    if declared.is_some_and(|declared| declared != *definition) {
        return Err(bad("original protocol pattern changes its owning enum declaration"));
    }
    Ok(Some(name == variant))
}

fn value_pattern_mask(value: &Value, pattern: &SemanticOriginalPattern, discriminant: Discriminant, b: &mut Budget) -> Check<u8> {
    if let Some(active) = selected_owner_variant(value, pattern, b)? {
        let fields = match &pattern.kind {
            CorePatternKind::Variant { fields, .. } | CorePatternKind::Constructor { fields, .. } => fields,
            _ => unreachable!("selected owner variant implies a variant pattern"),
        };
        if active {
            for field in fields {
                if !irrefutable_payload(field, b)? {
                    return Ok(0);
                }
            }
            return Ok(3);
        }
        return Ok(0);
    }
    pattern_mask(pattern, discriminant, b)
}

fn pattern_mask(pattern:&SemanticOriginalPattern,discriminant:Discriminant,b:&mut Budget)->Check<u8>{
    b.spend(1)?;
    Ok(match &pattern.kind{
        CorePatternKind::Name(_)|CorePatternKind::Wildcard=>3,
        CorePatternKind::ErrorArm{is_error,pattern}if discriminant==Discriminant::ErrorUnion&&irrefutable_payload(pattern,b)?=>if *is_error{1}else{2},
        CorePatternKind::Variant{variant,fields,..}|CorePatternKind::Constructor{variant,fields,..}if discriminant==Discriminant::Poll=>{
            for field in fields{if !irrefutable_payload(field,b)?{return Ok(0);}}
            match variant.as_str(){"Pending"=>1,"Ready"=>2,_=>0}
        },
        // This pattern has no proved full discriminant/payload coverage. Its
        // matching and nonmatching routes both remain possible.
        _=>0,
    })
}

#[cfg(test)]
mod error_arm_coverage_tests{
    use super::*;
    #[test]
    fn actual_original_match_keeps_only_uncovered_error_paths_for_return_and_cleanup(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let index=index::build(&module,&mut b).unwrap();let definition=*index.definitions.keys().next().unwrap();let span=module.functions[0].span;
        for(typed_sum,complete,refutable)in [(true,true,false),(true,false,false),(false,true,false),(true,true,true)]{
            let mut original=CheckedOriginal::default();
            if typed_sum{original.expressions.insert((definition,NodeId(900)),Ty::ErrorUnion(Box::new(Ty::Atom(Type::I32)),Box::new(Ty::Atom(Type::Unit))));}
            let mut arms=Vec::new();
            for is_error in [true,false]{
                if !complete&&!is_error{continue;}
                let payload=zil_ir::CorePattern{kind:if refutable&&is_error{CorePatternKind::Integer("7".into())}else{CorePatternKind::Wildcard},span};
                arms.push(zil_ir::CoreMatchArm{pattern:zil_ir::CorePattern{kind:CorePatternKind::ErrorArm{is_error,pattern:Box::new(payload)},span},body:zil_ir::CoreBlock{statements:vec![zil_ir::CoreStatement{id:NodeId(if is_error{902}else{903}),kind:CoreStatementKind::Return(None),span}],span},span});
            }
            let body:SemanticOriginalBlock=zil_ir::CoreBlock{statements:vec![zil_ir::CoreStatement{id:NodeId(901),kind:CoreStatementKind::Match{value:zil_ir::CoreExpr{id:NodeId(900),kind:CoreExprKind::Name("sum".into()),span},arms},span}],span};
            let mut births=tasks::SynchronousBirths::default();let summaries=BTreeMap::new();
            let mut engine=Engine{original:&original,module:&module,index:&index,current:definition,contracts:vec![],inputs:BTreeMap::new(),poll:false,next:0,closed_tasks:BTreeSet::new(),async_tasks:BTreeSet::new(),retiring_captures:BTreeSet::new(),capture_exits:vec![],capture_inputs:BTreeMap::new(),synchronous_births:&mut births,result_sources_by_function:&summaries,returned_sources:None,fallible_task_result:false,error_unwind:vec![],task_loop_keys:vec![],result_shapes:BTreeMap::new(),callable_roles:BTreeMap::new(),callable_targets:BTreeMap::new(),creation_shapes:BTreeMap::new()};
            let outcome=engine.block(&body,State::default(),&[],None,0,&mut b).unwrap();
            assert_eq!(outcome.fall.is_none(),typed_sum&&complete&&!refutable,"a real exhaustive two-arm return has no third cleanup/source route; missing type/arm or refutable payload must retain it");
        }
        let error=zil_ir::CorePattern{kind:CorePatternKind::ErrorArm{is_error:true,pattern:Box::new(zil_ir::CorePattern{kind:CorePatternKind::Wildcard,span})},span};
        assert_eq!(pattern_mask(&error,Discriminant::Poll,&mut b).unwrap(),0,"ErrorUnion tags cannot refine the unrelated Poll generation relation");
        let tuple=zil_ir::CorePattern{kind:CorePatternKind::Tuple(vec![zil_ir::CorePattern{kind:CorePatternKind::Wildcard,span},zil_ir::CorePattern{kind:CorePatternKind::Name("field".into()),span}]),span};
        assert!(irrefutable_payload(&tuple,&mut b).unwrap());
        let mut literal=tuple;let CorePatternKind::Tuple(fields)=&mut literal.kind else{unreachable!()};fields[1].kind=CorePatternKind::Integer("7".into());assert!(!irrefutable_payload(&literal,&mut b).unwrap());
    }
}
