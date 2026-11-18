//! A consuming call's remaining environment belongs to that actual invocation.
//! Direct callers require synchronous settlement; a real Task continuation can
//! retain it across Pending. Neither case grants SyncDrop to the callable Type.
use super::*;
use tasks::Requirement;

impl Engine<'_, '_> {
    pub(super) fn consumption_parameter(
        &mut self,
        owner: SemanticDefinitionId,
        index: u32,
        phase: Option<u8>,
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let root = Root::Parameter(owner, index);
        let value = if self.query_async(owner, Query::Parameter(index), depth + 1)? {
            state.roots.insert(root, phase.unwrap_or(UNKNOWN));
            Value::Root(
                root,
                true,
                self.query_ownership(owner, Query::Parameter(index), depth + 1)?
                    .unwrap_or(false),
                None,
            )
        } else {
            Value::Plain
        };
        let ty = self
            .query_type(owner, Query::Parameter(index), depth + 1)?
            .ok_or_else(|| failure("original consuming parameter loses its complete Gamma type"))?;
        let value = self
            .task_input(owner, &ty, root, state, depth + 1)?
            .unwrap_or(value);
        let mut value = self.typed(owner, &ty, value, depth + 1)?;
        self.input_sources(&mut value, root, &mut Vec::new(), depth + 1)?;
        Ok(value)
    }

    fn consumption_inputs(
        &mut self,
        owner: SemanticDefinitionId,
        depth: usize,
    ) -> R<(Vec<Value>, BTreeMap<String, Value>, State)> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { parameters, .. } = &definition.body else {
            return Err(failure(
                "complete consuming operation has no original function inputs",
            ));
        };
        let mut state = State {
            owner,
            roots: BTreeMap::new(),
            tasks: BTreeSet::new(),
            task_sync: BTreeMap::new(),
            task_sources: None,
            reachable: true,
            abrupt: Vec::new(),
            names: BTreeMap::new(),
            control: Control::Next,
            returned: Value::Plain,
            temporaries: Vec::new(),
        };
        let mut values = Vec::new();
        for index in 0..parameters.len() {
            self.pay(1, depth)?;
            values.push(
                self.consumption_parameter(
                    owner,
                    u32::try_from(index)
                        .map_err(|_| failure("original consuming parameter ordinal overflows"))?,
                    None,
                    &mut state,
                    depth + 1,
                )?,
            );
        }
        let mut captures = BTreeMap::new();
        if definition.kind == SemanticDefinitionKind::Closure {
            let (parent, creator) = self.closure_creator(definition, depth + 1)?;
            let names = self
                .proof
                .capture_names
                .get(&owner)
                .ok_or_else(|| failure("consuming Gamma loses its full capture directory"))?;
            self.pay(
                names
                    .iter()
                    .fold(names.len(), |n, name| n.saturating_add(name.len())),
                depth,
            )?;
            let names = self.proof.capture_names[&owner].clone();
            for (index, name) in names.into_iter().enumerate() {
                let ty = self.capture_type(parent, creator, &name, definition, depth + 1)?;
                let mut value = self.typed(parent, &ty, Value::Plain, depth + 1)?;
                self.input_sources(
                    &mut value,
                    Root::Capture(owner, index as u32),
                    &mut Vec::new(),
                    depth + 1,
                )?;
                captures.insert(name, value);
            }
        }
        Ok((values, captures, state))
    }

    fn discharge_consuming_requirements(
        &mut self,
        owner: SemanticDefinitionId,
        needed: &BTreeSet<Requirement>,
        depth: usize,
    ) -> R<()> {
        if needed.is_empty() {
            return Ok(());
        }
        let (arguments, captures, _) = self.consumption_inputs(owner, depth + 1)?;
        for requirement in needed {
            self.pay(1, depth)?;
            let (Requirement::Drop(root) | Requirement::Call(root) | Requirement::Consume(root)) =
                requirement;
            let Some(value) =
                self.original_input(*root, owner, &arguments, Some(&captures), depth + 1)?
            else {
                return Err(self.consumption_failure());
            };
            let proven = match (&value, requirement) {
                (Value::Cleanup { owner, ty, .. }, Requirement::Drop(_)) => {
                    self.can_drop(*owner, ty, depth + 1)?
                }
                (
                    Value::Cleanup { owner, ty, .. },
                    Requirement::Call(_) | Requirement::Consume(_),
                ) => cleanup::direct_callable(
                    self.program,
                    self.proof,
                    *owner,
                    ty,
                    matches!(requirement, Requirement::Consume(_)),
                    self.limits,
                    self.left,
                )?,
                (Value::Plain | Value::Bool(_), Requirement::Drop(_)) => true,
                _ => false,
            };
            if !proven {
                return Err(self.consumption_failure());
            }
        }
        Ok(())
    }

    fn complete_direct_consumption(&mut self, value: &Value, state: &State, depth: usize) -> R<()> {
        self.pay(value.work(), depth)?;
        match value {
            Value::Closure {
                definition,
                captures,
            } => {
                if !matches!(
                    &self.definition(*definition)?.body,
                    SemanticDefinitionBody::FunctionWithBody {
                        signature: SemanticSignatureRecipe {
                            call_kind: CallKind::Direct,
                            ..
                        },
                        ..
                    }
                ) {
                    return Err(self.consumption_failure());
                }
                self.consumption_body(*definition, depth + 1)?;
                let Some(Some(exits)) = self.consuming_exits.get(definition) else {
                    return Err(self.consumption_failure());
                };
                let work = exits.len().saturating_add(
                    self.consuming_requirements
                        .get(definition)
                        .map_or(0, BTreeSet::len),
                );
                self.pay(work, depth)?;
                let mut needed = self.consuming_exits[definition]
                    .as_ref()
                    .expect("checked exits")
                    .clone();
                if let Some(body) = self.consuming_requirements.get(definition) {
                    needed.extend(body.iter().copied());
                }
                let mut parameters = BTreeSet::new();
                let mut environment = BTreeSet::new();
                for requirement in needed {
                    let (Requirement::Drop(root)
                    | Requirement::Call(root)
                    | Requirement::Consume(root)) = requirement;
                    match self.input_source_root(root, depth + 1)? {
                        Root::Parameter(owner, _) if owner == *definition => {
                            parameters.insert(requirement);
                        }
                        Root::Capture(owner, _) if owner == *definition => {
                            environment.insert(requirement);
                        }
                        _ => return Err(self.consumption_failure()),
                    }
                }
                // Invocation inputs are arbitrary in the closure's own Gamma.
                // No concrete caller or borrowed Suspend frame can prove them.
                self.discharge_consuming_requirements(*definition, &parameters, depth + 1)?;
                // This subset has been proved to contain only Capture roots.
                let Some(remaining) = self.instantiate_graph_requirements(
                    *definition,
                    &environment,
                    &[],
                    Some(captures),
                    state,
                    &mut BTreeSet::new(),
                    depth + 1,
                )?
                else {
                    return Err(self.consumption_failure());
                };
                self.discharge_consuming_requirements(state.owner, &remaining, depth + 1)
            }
            Value::Cleanup { owner, ty, .. } => {
                if cleanup::direct_callable(
                    self.program,
                    self.proof,
                    *owner,
                    ty,
                    true,
                    self.limits,
                    self.left,
                )? {
                    Ok(())
                } else {
                    Err(self.consumption_failure())
                }
            }
            Value::Choice(values) => {
                for value in values {
                    self.complete_direct_consumption(value, state, depth + 1)?;
                }
                Ok(())
            }
            Value::Plain => Ok(()),
            _ => Err(self.consumption_failure()),
        }
    }

    pub(super) fn check_consuming_result(
        &mut self,
        owner: SemanticDefinitionId,
        state: &State,
        depth: usize,
    ) -> R<()> {
        let result = SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Result,
        };
        if cleanup::direct_consuming_bound(
            self.program,
            self.proof,
            owner,
            &result,
            self.limits,
            self.left,
        )? {
            self.complete_direct_consumption(&state.returned, state, depth + 1)?;
        }
        Ok(())
    }

    fn check_consuming_inputs(
        &mut self,
        target: SemanticDefinitionId,
        arguments: &[Value],
        state: &State,
        depth: usize,
    ) -> R<()> {
        let required = cleanup::direct_consuming_inputs(
            self.program,
            self.proof,
            target,
            self.limits,
            self.left,
        )?;
        if required.is_empty() {
            return Ok(());
        }
        let definition = self.definition(target)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else {
            return Ok(());
        };
        if signature.parameters.len() != arguments.len() {
            return Err(failure("consuming contract loses complete original inputs"));
        }
        for required in required {
            let mut found = false;
            for (parameter, value) in signature.parameters.iter().zip(arguments) {
                found |= self.check_consuming_input(
                    target,
                    &parameter.ty,
                    &required,
                    value,
                    state,
                    depth + 1,
                )?;
            }
            if !found {
                return Err(pending(
                    "Direct consuming requirement lacks its complete actual owning input occurrence",
                ));
            }
        }
        Ok(())
    }
    fn check_consuming_input(
        &mut self,
        target: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        required: &SemanticTypeRecipe,
        value: &Value,
        state: &State,
        depth: usize,
    ) -> R<bool> {
        self.pay(value.work(), depth)?;
        crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(required, self.limits, self.left, 1)?;
        if ty == required {
            self.complete_direct_consumption(value, state, depth + 1)?;
            return Ok(true);
        }
        if let Value::Choice(values) = value {
            let mut found = !values.is_empty();
            for value in values {
                found &=
                    self.check_consuming_input(target, ty, required, value, state, depth + 1)?;
            }
            return Ok(found);
        }
        if let Some(fields) =
            cleanup::fields(self.program, self.proof, target, ty, self.limits, self.left)?
        {
            let mut found = false;
            for (name, ty) in fields {
                let field = match value {
                    Value::Fields(values) => values.get(&name),
                    Value::Tuple(values) => name
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| values.get(index)),
                    _ => None,
                };
                if field.is_none() && matches!(value, Value::Plain | Value::Bool(_)) {
                    continue;
                }
                let value = field.ok_or_else(|| {
                    failure("Direct input contract loses its actual complete field correspondence")
                })?;
                found |=
                    self.check_consuming_input(target, &ty, required, value, state, depth + 1)?;
            }
            return Ok(found);
        }
        if let Value::Error { is_error, payload } = value {
            if let Some((error, result)) =
                cleanup::error_arms(self.program, self.proof, target, ty, self.limits, self.left)?
            {
                return self.check_consuming_input(
                    target,
                    if *is_error { &error } else { &result },
                    required,
                    payload,
                    state,
                    depth + 1,
                );
            }
        } else if let Value::Variant { name, fields } = value {
            let types = cleanup::variant(
                self.program,
                self.proof,
                target,
                ty,
                name,
                self.limits,
                self.left,
            )?
            .ok_or_else(|| failure("Direct input contract loses its original variant"))?;
            if types.len() != fields.len() {
                return Err(failure(
                    "Direct input contract changes its complete variant arity",
                ));
            }
            let mut found = false;
            for (ty, value) in types.iter().zip(fields) {
                found |=
                    self.check_consuming_input(target, ty, required, value, state, depth + 1)?;
            }
            return Ok(found);
        } else if let (SemanticTypeRecipe::Managed(inner), Value::Boxed(value)) = (ty, value) {
            return self.check_consuming_input(target, inner, required, value, state, depth + 1);
        } else if let SemanticTypeRecipe::Qualified { storage, .. } = ty {
            return self.check_consuming_input(target, storage, required, value, state, depth + 1);
        }
        Ok(false)
    }
    pub(super) fn suspend_root(&mut self, owner: SemanticDefinitionId, depth: usize) -> R<bool> {
        self.pay(1, depth)?;
        let original = self.definition(owner)?;
        Ok(
            matches!(&original.body, SemanticDefinitionBody::FunctionWithBody {
            signature, foreign: None, ..
        } if signature.call_kind == CallKind::Suspend),
        )
    }

    pub(super) fn consumption_state(&mut self, state: &State, depth: usize) -> R<State> {
        self.pay(
            state
                .roots
                .len()
                .saturating_add(state.tasks.len())
                .saturating_add(
                    state
                        .task_sync
                        .values()
                        .fold(1usize, |n, r| n.saturating_add(r.len()).saturating_add(1)),
                ),
            depth,
        )?;
        Ok(State {
            owner: state.owner,
            roots: state.roots.clone(),
            tasks: state.tasks.clone(),
            task_sync: state.task_sync.clone(),
            task_sources: None,
            reachable: state.reachable,
            abrupt: Vec::new(),
            names: BTreeMap::new(),
            control: Control::Next,
            returned: Value::Plain,
            temporaries: Vec::new(),
        })
    }

    fn consumption_failure(&self) -> Diagnostic {
        error(
            "E-ORIGINAL-CLEANUP",
            Span::default(),
            "consuming call has remaining environment cleanup without a synchronous proof or a Suspend execution root",
        )
    }

    fn consumption_body(&mut self, target: SemanticDefinitionId, depth: usize) -> R<()> {
        self.pay(lookup_work(self.local_complete.len()), depth)?;
        if self.local_complete.contains(&target) {
            return Ok(());
        }
        if self.active.contains(&target) {
            return Err(pending(
                "recursive consuming cleanup requires its complete original invocation relation",
            ));
        }
        // Frozen original type proofs do not contain this Engine's actual
        // per-exit summaries. Reconstruct from the immutable original body.
        self.proof.protocol_complete.remove(&target);
        self.proof.cleanup_complete.remove(&target);
        self.function(target, depth + 1)
    }

    pub(super) fn record_consuming_exit(
        &mut self,
        owner: SemanticDefinitionId,
        remaining: Option<BTreeSet<Requirement>>,
        depth: usize,
    ) -> R<()> {
        self.pay(
            lookup_work(self.consuming_exits.len())
                .saturating_add(remaining.as_ref().map_or(1, BTreeSet::len)),
            depth,
        )?;
        if let Some(previous) = self.consuming_exits.get_mut(&owner) {
            match (previous.as_mut(), remaining) {
                (Some(previous), Some(remaining)) => previous.extend(remaining),
                _ => *previous = None,
            }
        } else {
            self.consuming_exits.insert(owner, remaining);
        }
        Ok(())
    }

    fn remaining_generation(&mut self, value: &Value, state: &State, depth: usize) -> R<()> {
        self.pay(value.work(), depth)?;
        match value {
            Value::Root(_, _, false, _) => {}
            Value::Root(root, true, true, None)
                if state
                    .roots
                    .get(root)
                    .is_some_and(|phase| *phase != 0 && *phase & !UNKNOWN == 0) => {}
            Value::Root(_, false, true, Some(CLOSED)) => {}
            Value::Root(..) => {
                return Err(failure(
                    "consuming exit loses the exact live generation of a remaining captured owner",
                ));
            }
            Value::Cleanup {
                owner,
                ty,
                source: None,
            } if !self.can_drop(*owner, ty, depth + 1)? => {
                return Err(pending(
                    "remaining consuming environment lacks its complete original owner occurrence",
                ));
            }
            Value::Error { payload, .. } | Value::Boxed(payload) => {
                self.remaining_generation(payload, state, depth + 1)?
            }
            Value::Tuple(values)
            | Value::Choice(values)
            | Value::Variant { fields: values, .. } => {
                for value in values {
                    self.remaining_generation(value, state, depth + 1)?;
                }
            }
            Value::Fields(values)
            | Value::Closure {
                captures: values, ..
            } => {
                for value in values.values() {
                    self.remaining_generation(value, state, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn remaining_consumption(
        &mut self,
        value: &Value,
        state: &State,
        depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        self.remaining_generation(value, state, depth + 1)?;
        self.pay(value.work(), depth)?;
        match value {
            Value::Valid { .. } | Value::Poll { .. } => Ok(Some(BTreeSet::new())),
            Value::Root(_, _, false, _) => Ok(Some(BTreeSet::new())),
            Value::Root(root, true, true, None)
                if !state.tasks.contains(root) && state.roots.get(root) == Some(&CLOSED) =>
            {
                Ok(Some(BTreeSet::new()))
            }
            Value::Root(_, false, true, Some(CLOSED)) => Ok(Some(BTreeSet::new())),
            Value::Cleanup {
                owner,
                ty,
                source: Some(root),
            } => {
                if self.can_drop(*owner, ty, depth + 1)? {
                    return Ok(Some(BTreeSet::new()));
                }
                Ok(Some(BTreeSet::from([Requirement::Drop(*root)])))
            }
            Value::Error { payload, .. } | Value::Boxed(payload) => {
                self.remaining_consumption(payload, state, depth + 1)
            }
            Value::Tuple(values)
            | Value::Choice(values)
            | Value::Variant { fields: values, .. } => {
                let mut all = BTreeSet::new();
                for value in values {
                    let Some(needed) = self.remaining_consumption(value, state, depth + 1)? else {
                        return Ok(None);
                    };
                    self.pay(needed.len(), depth)?;
                    all.extend(needed);
                }
                Ok(Some(all))
            }
            Value::Fields(fields)
            | Value::Closure {
                captures: fields, ..
            } => {
                let mut all = BTreeSet::new();
                for value in fields.values() {
                    let Some(needed) = self.remaining_consumption(value, state, depth + 1)? else {
                        return Ok(None);
                    };
                    self.pay(needed.len(), depth)?;
                    all.extend(needed);
                }
                Ok(Some(all))
            }
            _ => self.synchronous_task_value(value, state, depth + 1),
        }
    }

    fn add_consuming_requirements(
        &mut self,
        owner: SemanticDefinitionId,
        needed: BTreeSet<Requirement>,
        depth: usize,
    ) -> R<()> {
        self.pay(
            lookup_work(self.consuming_requirements.len()).saturating_add(needed.len()),
            depth,
        )?;
        for requirement in &needed {
            let (Requirement::Drop(root) | Requirement::Call(root) | Requirement::Consume(root)) =
                requirement;
            if !matches!(root, Root::Parameter(domain, _) | Root::Capture(domain, _) | Root::Projection(domain, _) if *domain == owner)
            {
                return Err(self.consumption_failure());
            }
        }
        let requirements = self.consuming_requirements.entry(owner).or_default();
        if requirements.len().saturating_add(needed.len()) > self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "consuming cleanup obligations exceeded",
                None,
            ));
        }
        requirements.extend(needed);
        Ok(())
    }

    pub(super) fn synchronous_consumption(
        &mut self,
        value: &Value,
        arguments: &[Value],
        state: &State,
        active: &mut BTreeSet<SemanticDefinitionId>,
        depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        self.pay(value.work().saturating_add(arguments.len()), depth)?;
        match value {
            Value::Cleanup { owner, ty, source } => {
                // This is the complete invocation promise, including every
                // checked owning parameter. It never proves that an uncalled
                // value may be dropped, and needs no fabricated empty P.
                if cleanup::direct_callable(self.program, self.proof, *owner, ty, true, self.limits, self.left)? {
                    return Ok(Some(BTreeSet::new()));
                }
                if self.can_drop(*owner, ty, depth + 1)? {
                    return Ok(Some(BTreeSet::new()));
                }
                // A deferred application with owning arguments needs its full
                // invocation environment; never discard those dependencies.
                if !arguments
                    .iter()
                    .all(|value| matches!(value, Value::Plain | Value::Bool(_)))
                {
                    return Err(pending(
                        "symbolic consuming cleanup requires its complete owning argument environment",
                    ));
                }
                Ok(source.map(|root| BTreeSet::from([Requirement::Consume(root)])))
            }
            Value::Closure {
                definition,
                captures,
            } => {
                if !active.insert(*definition) {
                    return Ok(None);
                }
                let result = (|| {
                    self.consumption_body(*definition, depth + 1)?;
                    let Some(Some(remaining)) = self.consuming_exits.get(definition) else {
                        return Ok(None);
                    };
                    let work = remaining.len().saturating_add(
                        self.consuming_requirements
                            .get(definition)
                            .map_or(0, BTreeSet::len),
                    );
                    self.pay(work, depth)?;
                    let mut needed = self.consuming_exits[definition]
                        .as_ref()
                        .expect("checked exit")
                        .clone();
                    if let Some(body) = self.consuming_requirements.get(definition) {
                        needed.extend(body.iter().copied());
                    }
                    self.instantiate_graph_requirements(
                        *definition,
                        &needed,
                        arguments,
                        Some(captures),
                        state,
                        active,
                        depth + 1,
                    )
                })();
                active.remove(definition);
                result
            }
            Value::Choice(values) => {
                let mut all = BTreeSet::new();
                for value in values {
                    let Some(needed) =
                        self.synchronous_consumption(value, arguments, state, active, depth + 1)?
                    else {
                        return Ok(None);
                    };
                    self.pay(needed.len(), depth)?;
                    all.extend(needed);
                }
                Ok(Some(all))
            }
            // Stateless code has no environment responsibility. The original
            // callable call directory independently checks its signature.
            Value::Plain => Ok(Some(BTreeSet::new())),
            _ => Ok(None),
        }
    }

    pub(super) fn check_direct_consumption(
        &mut self,
        callee: &Value,
        arguments: &[Value],
        state: &State,
        depth: usize,
    ) -> R<()> {
        let Some(needed) = self.synchronous_consumption(
            callee,
            arguments,
            state,
            &mut BTreeSet::new(),
            depth + 1,
        )?
        else {
            return Err(self.consumption_failure());
        };
        self.add_consuming_requirements(state.owner, needed, depth + 1)
    }

    pub(super) fn check_scoped_body_consumption(
        &mut self, body: &Value, state: &State, depth: usize,
    ) -> R<()> {
        // Failed setup drops the prepared body without invocation. Successful
        // setup consumes it once inside the same synchronous scope.
        let Some(needed) = self.remaining_consumption(body, state, depth + 1)? else {
            return Err(self.consumption_failure());
        };
        self.add_consuming_requirements(state.owner, needed, depth + 1)?;
        self.check_direct_consumption(body, &[], state, depth + 1)
    }

    pub(super) fn check_consuming_invocation(
        &mut self,
        callee: &Value,
        arguments: &[Value],
        operation: CallKind,
        state: &State,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if operation == CallKind::Suspend && self.suspend_root(state.owner, depth + 1)? {
            // Still check every local body obligation. Only remaining capture
            // cleanup is attached to this invocation's stable Task root.
            match callee {
                Value::Closure {
                    definition,
                    captures,
                } => {
                    self.consumption_body(*definition, depth + 1)?;
                    self.check_consuming_target(
                        *definition,
                        arguments,
                        Some(captures),
                        state,
                        depth + 1,
                    )?;
                }
                Value::Cleanup { owner, ty, .. } => {
                    if !self.can_drop(*owner, ty, depth + 1)?
                        && !cleanup::resumable_callable(
                            self.program,
                            self.proof,
                            *owner,
                            ty,
                            self.limits,
                            self.left,
                        )?
                    {
                        return Err(self.consumption_failure());
                    }
                }
                Value::Choice(values) => {
                    for value in values {
                        self.check_consuming_invocation(value, arguments, operation, state, depth + 1)?;
                    }
                }
                Value::Plain => {}
                _ => return Err(self.consumption_failure()),
            }
            return Ok(());
        }
        self.check_direct_consumption(callee, arguments, state, depth + 1)
    }

    pub(super) fn check_consuming_target(
        &mut self,
        target: SemanticDefinitionId,
        arguments: &[Value],
        captures: Option<&BTreeMap<String, Value>>,
        state: &State,
        depth: usize,
    ) -> R<()> {
        self.check_consuming_inputs(target, arguments, state, depth + 1)?;
        self.pay(lookup_work(self.consuming_relevant.len()), depth)?;
        if !self.consuming_relevant.contains(&target) {
            return Ok(());
        }
        self.consumption_body(target, depth + 1)?;
        let Some(needed) = self.consuming_requirements.get(&target) else {
            return Ok(());
        };
        let work = needed.len();
        self.pay(work, depth)?;
        let needed = self.consuming_requirements[&target].clone();
        // These obligations came from Direct instructions inside the callee.
        // Even a Suspend caller cannot lend its frame to that Direct stack.
        let Some(needed) = self.instantiate_graph_requirements(
            target,
            &needed,
            arguments,
            captures,
            state,
            &mut BTreeSet::new(),
            depth + 1,
        )?
        else {
            return Err(self.consumption_failure());
        };
        self.add_consuming_requirements(state.owner, needed, depth + 1)
    }
}
