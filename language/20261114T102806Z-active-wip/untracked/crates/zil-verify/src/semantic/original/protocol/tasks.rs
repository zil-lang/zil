//! Task phases reuse the original finite CFG solver. Only a canonical current
//! poll-close Ready can supply Closed; construction and ordinary Ready cannot.
use super::*;

pub(super) fn is_task(ty: &Ty) -> bool {
    match ty {
        Ty::Carrier {
            kind: CarrierKind::Task,
            ..
        } => true,
        Ty::Managed(inner) | Ty::Desc { inner, .. } | Ty::Address { inner, .. } => is_task(inner),
        _ => false,
    }
}

impl<'a> Engine<'a> {
    pub(super) fn task_bind(
        &mut self,
        state: &mut State,
        binder: &Binder,
        value: Value,
        b: &mut Budget,
    ) -> Check<Value> {
        if let Some(Value::Owner {
            root,
            borrowed: false,
        }) = state.values.get(binder).cloned()
        {
            if state.owners.get(&root).is_some_and(|owner| owner.task)
                && !matches!(&value,Value::Owner{root:other,..}if *other==root)
            {
                self.task_cleanup_root(state, &root, b)?;
                state.values.remove(binder);
            }
        }
        match value {
            Value::FreshTask => {
                if state
                    .owners
                    .get(binder)
                    .is_some_and(|owner| owner.states != DEAD)
                {
                    return Err(bad("original Task birth reuses a live owner generation"));
                }
                let generation = self.generation(b)?;
                state.owners.insert(
                    binder.clone(),
                    Owner {
                        states: OPEN,
                        generation,
                        task: true,
                        owned: true,
                    },
                );
                Ok(Value::Owner {
                    root: binder.clone(),
                    borrowed: false,
                })
            }
            Value::Owner {
                root,
                borrowed: false,
            } if root != *binder && state.owners.get(&root).is_some_and(|owner| owner.task) => {
                let owner = state.owner(&root, b)?.clone();
                if !owner.owned{return Err(bad("borrowed Task generation cannot become an owning transfer"));}
                if state
                    .owners
                    .get(binder)
                    .is_some_and(|owner| owner.states != DEAD)
                {
                    return Err(bad("Task transfer overwrites another live generation"));
                }
                self.update(state, &root, DEAD, b)?;
                let generation = self.generation(b)?;
                state.owners.insert(
                    binder.clone(),
                    Owner {
                        states: owner.states,
                        generation,
                        task: true,
                        owned: true,
                    },
                );
                Ok(Value::Owner {
                    root: binder.clone(),
                    borrowed: false,
                })
            }
            value => Ok(value),
        }
    }
    fn task_cleanup_root(&mut self, state: &mut State, root: &Binder, b: &mut Budget) -> Check<()> {
        b.spend(state.width())?;
        let Some(owner) = state.owners.get(root).cloned() else {
            return Ok(());
        };
        if !owner.task || !owner.owned || owner.states == DEAD {
            return Ok(());
        }
        if owner.states == 0 || owner.states & !(CLOSED | DEAD) != 0 {
            return Err(bad(
                "original Task cleanup edge lacks Closed for every live current generation",
            ));
        }
        root.charge_clone(b)?;
        self.closed_tasks.insert(root.clone());
        state.invalidate(root, b)?;
        let generation = self.generation(b)?;
        let owner = state
            .owners
            .get_mut(root)
            .ok_or_else(|| bad("Task cleanup lost its actual owner"))?;
        owner.states = DEAD;
        owner.generation = generation;
        Ok(())
    }
    pub(super) fn task_scope_cleanup(
        &mut self,
        state: &mut State,
        before: &BTreeSet<Binder>,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(state.width())?;
        let leaving = state
            .values
            .keys()
            .filter(|key| !before.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for binder in leaving {
            if let Some(Value::Owner {
                root,
                borrowed: false,
            }) = state.values.get(&binder).cloned()
            {
                self.task_cleanup_root(state, &root, b)?;
            }
            state.values.remove(&binder);
        }
        // A join that lost the value selector cannot silently erase an owned
        // Task. All such orphaned generations must already be retired.
        let orphaned=state.owners.iter().filter(|(root,owner)|owner.task&&owner.owned&&owner.states!=DEAD&&!before.contains(*root)&&!state.values.values().any(|value|matches!(value,Value::Owner{root:actual,borrowed:false}if actual==*root))).map(|(root,_)|root.clone()).collect::<Vec<_>>();
        for root in orphaned {
            self.task_cleanup_root(state, &root, b)?;
        }
        Ok(())
    }
    pub(super) fn task_exit(&mut self, state: &mut State, b: &mut Budget) -> Check<()> {
        b.spend(state.width())?;
        let roots = state
            .owners
            .iter()
            .filter(|(_, owner)| owner.task && owner.owned && owner.states != DEAD)
            .map(|(root, _)| root.clone())
            .collect::<Vec<_>>();
        for root in roots {
            self.task_cleanup_root(state, &root, b)?;
        }
        Ok(())
    }
    pub(super) fn task_call(
        &mut self,
        kind: Intrinsic,
        values: &[Value],
        state: &mut State,
        b: &mut Budget,
    ) -> Check<Option<Value>> {
        if matches!(kind, Intrinsic::TaskCreate | Intrinsic::TaskCreateOwned) {
            for value in values {
                if let Value::Owner {
                    root,
                    borrowed: false,
                } = value
                {
                    if state.owners.get(root).is_some_and(|owner| owner.task) {
                        return Err(unsupported(
                            "owning Task frame input needs its complete returned-failure owner transport",
                            None,
                        ));
                    }
                }
            }
            return Ok(Some(Value::FallibleTask));
        }
        if kind == Intrinsic::TaskYield {
            return Ok(Some(Value::Plain));
        }
        if !matches!(
            kind,
            Intrinsic::TaskValid
                | Intrinsic::TaskPoll
                | Intrinsic::TaskRequest
                | Intrinsic::TaskPollClose
                | Intrinsic::TaskFinish
        ) {
            return Ok(None);
        }
        let [Value::Owner { root, borrowed }] = values else {
            return Err(bad("Task operation has no exact original owner slot"));
        };
        let owner = state.owner(root, b)?.clone();
        if !owner.task {
            return Err(bad(
                "Task operation substitutes a registration owner generation",
            ));
        }
        Ok(Some(match kind {
            Intrinsic::TaskValid => Value::Plain,
            Intrinsic::TaskPoll => {
                self.update(state, root, owner.states, b)?;
                Value::Plain
            }
            Intrinsic::TaskRequest => {
                let next = if owner.states & CLOSED != 0 {
                    CLOSED
                } else {
                    0
                } | if owner.states & (OPEN | CLOSING) != 0 {
                    CLOSING
                } else {
                    0
                };
                self.update(state, root, next, b)?;
                Value::Plain
            }
            Intrinsic::TaskPollClose => {
                let pending = if owner.states & (OPEN | CLOSING) != 0 {
                    CLOSING
                } else {
                    0
                };
                let ready = CLOSED;
                let generation = self.update(state, root, pending | ready, b)?;
                Value::Poll {
                    relations: vec![Conditional {
                        root: root.clone(),
                        generation,
                        pending,
                        ready,
                    }],
                    tag: None,
                }
            }
            Intrinsic::TaskFinish => {
                if *borrowed || !owner.owned || owner.states != CLOSED {
                    return Err(bad(
                        "Task destruction lacks its owning current-generation canonical Closed proof",
                    ));
                }
                self.update(state, root, DEAD, b)?;
                Value::Plain
            }
            _ => unreachable!(),
        }))
    }
    pub(super) fn task_capture(
        &mut self,
        child: SemanticDefinitionId,
        state: &mut State,
        b: &mut Budget,
    ) -> Check<()> {
        if !self
            .original
            .creator_moves
            .get(&child)
            .copied()
            .unwrap_or(false)
        {
            return Ok(());
        }
        if let Some(free) = self.original.free_values.get(&child) {
            for binding in free.values() {
                let captured = Binder::Capture {
                    definition: child,
                    source: Box::new(binding.identity.clone()),
                };
                if !self
                    .original
                    .used
                    .get(&child)
                    .is_some_and(|used| used.contains(&captured))
                {
                    continue;
                }
                if !is_task(&self.original.resolved(&binding.ty, b)?) {
                    continue;
                }
                let Some(Value::Owner {
                    root,
                    borrowed: false,
                }) = state.values.get(&binding.identity).cloned()
                else {
                    return Err(bad("Task capture has no exact owning source generation"));
                };
                self.update(state, &root, DEAD, b)?;
                state.values.remove(&binding.identity);
            }
        }
        Ok(())
    }
}
