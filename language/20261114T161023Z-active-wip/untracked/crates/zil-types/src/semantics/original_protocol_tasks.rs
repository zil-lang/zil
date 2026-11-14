//! Task ownership is attached to real input, capture, constructor and checked
//! return occurrences. A Task type alone never creates a close-state fact.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Exit {
    pub value: Value,
    pub phases: BTreeMap<Root, u8>,
}
impl Engine<'_, '_> {
    pub(super) fn task_head(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(1, depth)?;
        cleanup::task_head(self.program, self.proof, owner, ty, self.limits, self.left)
    }
    pub(super) fn task_input(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        root: Root,
        state: &mut State,
        depth: usize,
    ) -> R<Option<Value>> {
        let Some(owning) = self.task_head(owner, ty, depth + 1)? else {
            return Ok(None);
        };
        self.pay(2, depth)?;
        state.roots.insert(root, UNKNOWN);
        state.tasks.insert(root);
        Ok(Some(Value::Root(root, true, owning, None)))
    }
    pub(super) fn task_drop(
        &mut self,
        root: Root,
        state: &State,
        prior: Option<u8>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        let phase = prior.unwrap_or_else(|| state.roots.get(&root).copied().unwrap_or(0));
        if phase == CLOSED {
            return Ok(());
        }
        if prior.is_some() {
            return Err(failure(
                "original Task cleanup loses an earlier live task generation",
            ));
        }
        let original = self.definition(state.owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            foreign: None,
            ..
        } = &original.body
        else {
            return Err(failure(
                "original Task cleanup has no language execution root",
            ));
        };
        if signature.call_kind != CallKind::Suspend {
            return Err(error(
                "E-ORIGINAL-CLEANUP",
                Span::default(),
                "live original Task cleanup may suspend and lacks a Closed fact or a Suspend execution root",
            ));
        }
        // A genuine initialized/incoming owner preserves its frame's cleanup
        // invariant. This validates a resumable cleanup edge only: it does not
        // assert Closed at a subsequent source operation or SyncDrop(Task/F).
        Ok(())
    }
    pub(super) fn task_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        site: SemanticSiteId,
        kind: &IntrinsicKind,
        values: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Option<Value>> {
        self.pay(1, depth)?;
        match kind {
            IntrinsicKind::TaskCreate {
                worker,
                owned,
                await_result,
            } => {
                if !self.active.contains(worker) {
                    self.function(*worker, depth + 1)?;
                }
                if !self.proof.cleanup_complete.contains(worker) {
                    return Err(pending(
                        "Task construction requires its complete original worker cleanup invariant",
                    ));
                }
                let arity = self
                    .proof
                    .intrinsics
                    .get(&(owner, site))
                    .ok_or_else(|| failure("Task worker scheme disappeared"))?
                    .parameters
                    .len();
                if values.len() != arity.saturating_add(1)
                    || !matches!(values.first(), Some(Value::Plain))
                {
                    return Err(failure(
                        "Task worker occurrence changes its static provider or complete runtime input arity",
                    ));
                }
                let values = &values[1..];
                if *await_result {
                    return self.task_return_call(owner, node, *worker, values, state, depth + 1);
                }
                let root = Root::Created(owner, node);
                self.change(state, root, OPEN, true, depth + 1)?;
                self.pay(1, depth)?;
                state.tasks.insert(root);
                let failure = if *owned {
                    // The canonical failure carrier physically owns exactly the
                    // prepared inputs; its frame type argument is phantom.
                    let mut inputs = Vec::with_capacity(values.len());
                    for value in values {
                        inputs.push(self.value_copy(value, depth + 1)?)
                    }
                    self.pay(14, depth)?;
                    Value::Fields(BTreeMap::from([
                        ("reason".into(), Value::Plain),
                        ("inputs".into(), Value::Tuple(inputs)),
                    ]))
                } else {
                    for value in values {
                        self.drop_value(value, state, depth + 1)?;
                    }
                    Value::Plain
                };
                self.choice(
                    vec![
                        Value::Error {
                            is_error: true,
                            payload: Box::new(failure),
                        },
                        Value::Error {
                            is_error: false,
                            payload: Box::new(Value::Root(root, true, true, None)),
                        },
                    ],
                    depth + 1,
                )
                .map(Some)
            }
            IntrinsicKind::TaskOperation(op) => {
                let [value] = values else {
                    return Err(failure(
                        "Task operation changes its original one-owner arity",
                    ));
                };
                let root = self.root(value, state, depth + 1)?;
                self.pay(
                    state.tasks.len().saturating_add(1).ilog2() as usize + 1,
                    depth,
                )?;
                if !state.tasks.contains(&root) {
                    return Err(failure(
                        "Task operation has no real initialized Task owner occurrence",
                    ));
                }
                let phase = state.roots[&root];
                match op {
                    TaskOp::Valid => Ok(Some(Value::Plain)),
                    TaskOp::FinishDestroy => {
                        if !matches!(value, Value::Root(_, true, true, _)) || phase != CLOSED {
                            return Err(failure(
                                "Task destruction lacks its unique owning occurrence and Closed fact",
                            ));
                        }
                        self.change(state, root, CLOSED, true, depth + 1)?;
                        Ok(Some(Value::Plain))
                    }
                    TaskOp::RequestCancel => {
                        let closing = if phase & (OPEN | CLOSING) != 0 {
                            CLOSING
                        } else {
                            0
                        };
                        self.change(state, root, closing | (phase & CLOSED), false, depth + 1)?;
                        Ok(Some(Value::Plain))
                    }
                    TaskOp::PollClose => {
                        let closing = if phase & (OPEN | CLOSING) != 0 {
                            CLOSING
                        } else {
                            0
                        };
                        self.change(state, root, closing | CLOSED, false, depth + 1)?;
                        let mut outcomes = Vec::new();
                        if closing != 0 {
                            outcomes.push(Outcome {
                                exit: ProtocolExit::PollPending,
                                phases: BTreeMap::from([(root, CLOSING)]),
                            })
                        }
                        outcomes.push(Outcome {
                            exit: ProtocolExit::PollReady,
                            phases: BTreeMap::from([(root, CLOSED)]),
                        });
                        self.pay(outcomes.len().saturating_mul(3), depth)?;
                        Ok(Some(Value::Poll {
                            fresh: true,
                            outcomes,
                        }))
                    }
                    TaskOp::Poll => {
                        let result = {
                            let call = self
                                .proof
                                .intrinsics
                                .get(&(owner, site))
                                .ok_or_else(|| failure("Task poll scheme disappeared"))?;
                            copy_recipe(&call.result, self.limits, self.left)?
                        };
                        let Some((error, poll)) = cleanup::error_arms(
                            self.program,
                            self.proof,
                            owner,
                            &result,
                            self.limits,
                            self.left,
                        )?
                        else {
                            return Err(failure(
                                "Task poll result changes its original ErrorUnion scheme",
                            ));
                        };
                        let error = self.typed(owner, &error, Value::Plain, depth + 1)?;
                        let mut outcomes = vec![Value::Error {
                            is_error: true,
                            payload: Box::new(error),
                        }];
                        if phase & (OPEN | CLOSING) != 0 {
                            let fields = cleanup::variant(
                                self.program,
                                self.proof,
                                owner,
                                &poll,
                                "Ready",
                                self.limits,
                                self.left,
                            )?
                            .ok_or_else(|| {
                                failure("Task poll has no canonical complete Ready payload")
                            })?;
                            let [payload] = fields.as_slice() else {
                                return Err(failure(
                                    "Task Ready changes its complete payload arity",
                                ));
                            };
                            let ready = self.typed(owner, payload, Value::Plain, depth + 1)?;
                            let poll = self.choice(
                                vec![
                                    Value::Variant {
                                        name: "Pending".into(),
                                        fields: Vec::new(),
                                    },
                                    Value::Variant {
                                        name: "Ready".into(),
                                        fields: vec![ready],
                                    },
                                ],
                                depth + 1,
                            )?;
                            outcomes.push(Value::Error {
                                is_error: false,
                                payload: Box::new(poll),
                            });
                        }
                        self.pay(outcomes.len().saturating_add(14), depth)?;
                        // Ready moves the payload once. It does not prove that
                        // a borrowed result, child, waker or external user has
                        // drained; only the separate close operation does that.
                        self.choice(outcomes, depth + 1).map(Some)
                    }
                }
            }
            _ => Ok(None),
        }
    }
    pub(super) fn task_record_exit(
        &mut self,
        owner: SemanticDefinitionId,
        state: &State,
        depth: usize,
    ) -> R<()> {
        let mut roots = BTreeSet::new();
        self.pay(state.returned.work(), depth)?;
        state.returned.roots(&mut roots);
        let mut phases: BTreeMap<Root, u8> = BTreeMap::new();
        for root in roots {
            if state.tasks.contains(&root) {
                self.pay(2, depth)?;
                phases.insert(root, state.roots[&root]);
            }
        }
        let value = self.value_copy(&state.returned, depth + 1)?;
        let exit = Exit { value, phases };
        self.pay(
            self.task_exits.len().saturating_add(1).ilog2() as usize + 1,
            depth,
        )?;
        if let Some(previous) = self.task_exits.get(&owner) {
            let work = previous.iter().fold(0usize, |work, old| {
                work.saturating_add(old.value.work())
                    .saturating_add(exit.value.work())
                    .saturating_add(old.phases.len())
                    .saturating_add(exit.phases.len())
            });
            self.pay(work, depth)?;
            if self.task_exits[&owner].iter().any(|old| old == &exit) {
                return Ok(());
            }
        }
        if self.task_exit_count >= self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original Task return inventory limit exceeded",
                None,
            ));
        }
        self.pay(1, depth)?;
        self.task_exits.entry(owner).or_default().push(exit);
        self.task_exit_count += 1;
        Ok(())
    }
    pub(super) fn task_return_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        target: SemanticDefinitionId,
        arguments: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Option<Value>> {
        if self.active.contains(&target) {
            return Ok(None);
        }
        self.function(target, depth + 1)?;
        let Some(exits) = self.task_exits.get(&target) else {
            return Ok(None);
        };
        if !exits.iter().any(|exit| !exit.phases.is_empty()) {
            return Ok(None);
        }
        let work = exits.iter().fold(exits.len(), |work, exit| {
            work.saturating_add(exit.value.work())
                .saturating_add(exit.phases.len())
        });
        self.pay(work, depth)?;
        let exits = self.task_exits[&target].clone();
        for exit in &exits {
            self.charge_value(&exit.value, depth + 1)?;
        }
        let mut roots = BTreeMap::new();
        let mut phases: BTreeMap<Root, u8> = BTreeMap::new();
        for exit in &exits {
            for (root, phase) in &exit.phases {
                let mapped = if let Root::Parameter(definition, ordinal) = root {
                    if *definition != target {
                        return Err(failure("Task return changes its original parameter domain"));
                    }
                    let value = arguments
                        .get(*ordinal as usize)
                        .ok_or_else(|| failure("Task return omits its real input owner"))?;
                    let mapped = self.root(value, state, depth + 1)?;
                    if !matches!(value, Value::Root(_, true, true, _)) {
                        return Err(pending(
                            "Task owning return through a borrowed head requires its exact storage take/refill relation",
                        ));
                    }
                    if !state.tasks.contains(&mapped) {
                        return Err(failure(
                            "Task return input has no initialized owner invariant",
                        ));
                    }
                    mapped
                } else {
                    if matches!(root, Root::Capture(_, _))
                        && self.proof.kinds.get(&target) != Some(&CallableKind::FnOnce)
                    {
                        return Err(pending(
                            "Task return from reusable capture requires its complete take/refill correspondence",
                        ));
                    }
                    let slot = u32::try_from(roots.len())
                        .map_err(|_| failure("Task return root index overflow"))?;
                    *roots
                        .entry(*root)
                        .or_insert(Root::Result(owner, node, slot))
                };
                phases
                    .entry(mapped)
                    .and_modify(|joined| *joined |= *phase)
                    .or_insert(*phase);
                roots.insert(*root, mapped);
            }
        }
        for (root, phase) in phases {
            let fresh = matches!(root,Root::Result(definition,at,_) if definition==owner&&at==node);
            self.change(state, root, phase, fresh, depth + 1)?;
            state.tasks.insert(root);
        }
        let mut values = Vec::with_capacity(exits.len());
        for exit in exits {
            values.push(self.task_rehome(exit.value, &roots, depth + 1)?)
        }
        self.choice(values, depth + 1).map(Some)
    }
    fn task_rehome(
        &mut self,
        value: Value,
        roots: &BTreeMap<Root, Root>,
        depth: usize,
    ) -> R<Value> {
        self.pay(1, depth)?;
        Ok(match value {
            Value::Root(root, live, owning, prior) => Value::Root(
                *roots.get(&root).ok_or_else(|| {
                    pending("Task return has an unmapped original owning projection")
                })?,
                live,
                owning,
                prior,
            ),
            Value::Error { is_error, payload } => Value::Error {
                is_error,
                payload: Box::new(self.task_rehome(*payload, roots, depth + 1)?),
            },
            Value::Boxed(value) => {
                Value::Boxed(Box::new(self.task_rehome(*value, roots, depth + 1)?))
            }
            Value::Tuple(values) => Value::Tuple(
                values
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, depth + 1))
                    .collect::<R<_>>()?,
            ),
            Value::Choice(values) => Value::Choice(
                values
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, depth + 1))
                    .collect::<R<_>>()?,
            ),
            Value::Variant { name, fields } => Value::Variant {
                name,
                fields: fields
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, depth + 1))
                    .collect::<R<_>>()?,
            },
            Value::Fields(values) => Value::Fields(
                values
                    .into_iter()
                    .map(|(name, value)| Ok((name, self.task_rehome(value, roots, depth + 1)?)))
                    .collect::<R<_>>()?,
            ),
            Value::Closure {
                definition,
                captures,
            } => Value::Closure {
                definition,
                captures: captures
                    .into_iter()
                    .map(|(name, value)| Ok((name, self.task_rehome(value, roots, depth + 1)?)))
                    .collect::<R<_>>()?,
            },
            Value::Poll { .. } | Value::Valid { .. } => {
                return Err(pending(
                    "Task return also carries an independently correlated protocol discriminator",
                ));
            }
            other => other,
        })
    }
}
