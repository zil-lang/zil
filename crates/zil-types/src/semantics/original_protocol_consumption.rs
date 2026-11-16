//! A consuming call's remaining environment belongs to that actual invocation.
//! Direct callers require synchronous settlement; a real Task continuation can
//! retain it across Pending. Neither case grants SyncDrop to the callable Type.
use super::*;
use tasks::Requirement;

impl Engine<'_, '_> {
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
            if !matches!(root, Root::Parameter(domain, _) | Root::Capture(domain, _) if *domain == owner)
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

    pub(super) fn check_thread_consumption(
        &mut self,
        values: &[Value],
        state: &State,
        depth: usize,
    ) -> R<()> {
        let [Value::Plain, _, body] = values else {
            return Err(failure(
                "thread body cleanup loses its complete actual arguments",
            ));
        };
        // Thread creation failure never invokes the body. Its prepared owner
        // must also be synchronously destructible in that distinct branch.
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
        state: &State,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if self.suspend_root(state.owner, depth + 1)? {
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
                        self.check_consuming_invocation(value, arguments, state, depth + 1)?;
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
