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

#[derive(Clone)]
pub(in crate::semantic::original) enum Shape {
    Plain,
    Task { borrowed: bool },
    Product(Vec<(String, Shape)>),
}
impl Shape {
    pub(in crate::semantic::original) fn has_task(&self) -> bool {
        match self {
            Self::Plain => false,
            Self::Task { .. } => true,
            Self::Product(fields) => fields.iter().any(|(_, field)| field.has_task()),
        }
    }
}
impl CheckedOriginal {
    pub(in crate::semantic::original) fn task_shape(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        ty: &Ty,
        borrowed: bool,
        active: &mut ActiveQueries,
        b: &mut Budget,
        depth: usize,
    ) -> Check<Shape> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("Task projected original owner shape exceeded"));
        }
        let ty = self.resolved(ty, b)?;
        if is_task(&ty) {
            return Ok(Shape::Task {
                borrowed: borrowed || owner_type(&ty) == Some(true),
            });
        }
        Ok(match ty {
            Ty::Tuple(fields) => {
                let mut product = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    product.push((
                        index.to_string(),
                        self.task_shape(module, context, field, borrowed, active, b, depth + 1)?,
                    ));
                }
                Shape::Product(product)
            }
            Ty::Nominal(id, arguments) => {
                let definition = context.definition(module, id, b)?;
                if let SemanticDefinitionBody::Nominal {
                    body: SemanticNominalBody::Product { fields },
                    ..
                } = &definition.body
                {
                    let mut env = self.environment(module, context, id, active, b)?;
                    env.lexical[0].actuals = arguments;
                    let mut product = Vec::new();
                    for field in fields {
                        let ty = self.ty(module, context, &field.ty, &env, active, b)?;
                        product.push((
                            field.name.clone(),
                            self.task_shape(module, context, &ty, borrowed, active, b, depth + 1)?,
                        ));
                    }
                    Shape::Product(product)
                } else {
                    Shape::Plain
                }
            }
            Ty::Address { inner, .. } | Ty::Desc { inner, .. } => {
                self.task_shape(module, context, &inner, true, active, b, depth + 1)?
            }
            Ty::Managed(inner) => {
                self.task_shape(module, context, &inner, borrowed, active, b, depth + 1)?
            }
            _ => Shape::Plain,
        })
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
        let destination = root_place(binder);
        if let Some(previous) = state.values.get(binder).cloned() {
            self.task_cleanup_value(state, &previous, Some(&value), b)?;
        }
        self.task_bind_place(state, &destination, value, b)
    }
    fn task_bind_place(
        &mut self,
        state: &mut State,
        destination: &Place,
        value: Value,
        b: &mut Budget,
    ) -> Check<Value> {
        b.spend(
            destination
                .fields
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(1),
        )?;
        match value {
            Value::Product(fields) => {
                let mut result = Vec::new();
                for (name, value) in fields {
                    let mut child = destination.clone();
                    child.fields.push(name.clone());
                    result.push((name, self.task_bind_place(state, &child, value, b)?));
                }
                Ok(Value::Product(result))
            }
            Value::FreshTask => {
                if state
                    .owners
                    .get(&destination.clone())
                    .is_some_and(|owner| owner.states != DEAD)
                {
                    return Err(bad("original Task birth reuses a live owner generation"));
                }
                let generation = self.generation(b)?;
                state.owners.insert(
                    destination.clone(),
                    Owner {
                        states: OPEN,
                        generation,
                        task: true,
                        owned: true,
                    },
                );
                Ok(Value::Owner {
                    root: destination.clone(),
                    borrowed: false,
                })
            }
            Value::Owner {
                root,
                borrowed: false,
            } if root != destination.clone()
                && state.owners.get(&root).is_some_and(|owner| owner.task) =>
            {
                let owner = state.owner(&root, b)?.clone();
                if !owner.owned {
                    return Err(bad(
                        "borrowed Task generation cannot become an owning transfer",
                    ));
                }
                if state
                    .owners
                    .get(&destination.clone())
                    .is_some_and(|owner| owner.states != DEAD)
                {
                    return Err(bad("Task transfer overwrites another live generation"));
                }
                self.update(state, &root, DEAD, b)?;
                let generation = self.generation(b)?;
                state.owners.insert(
                    destination.clone(),
                    Owner {
                        states: owner.states,
                        generation,
                        task: true,
                        owned: true,
                    },
                );
                Ok(Value::Owner {
                    root: destination.clone(),
                    borrowed: false,
                })
            }
            value => Ok(value),
        }
    }
    pub(super) fn task_entry(
        &mut self,
        state: &mut State,
        place: &Place,
        shape: &Shape,
        b: &mut Budget,
    ) -> Check<Value> {
        b.spend(1)?;
        Ok(match shape {
            Shape::Plain => Value::Plain,
            Shape::Task { borrowed } => {
                let generation = self.generation(b)?;
                state.owners.insert(
                    place.clone(),
                    Owner {
                        states: LIVE,
                        generation,
                        task: true,
                        owned: !*borrowed,
                    },
                );
                Value::Owner {
                    root: place.clone(),
                    borrowed: *borrowed,
                }
            }
            Shape::Product(fields) => {
                let mut result = Vec::new();
                for (name, shape) in fields {
                    let mut child = place.clone();
                    child.fields.push(name.clone());
                    result.push((name.clone(), self.task_entry(state, &child, shape, b)?));
                }
                Value::Product(result)
            }
        })
    }
    pub(super) fn task_cleanup_value(
        &mut self,
        state: &mut State,
        value: &Value,
        replacement: Option<&Value>,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?;
        match value {
            Value::Owner {
                root,
                borrowed: false,
            } => {
                if !replacement.is_some_and(|other| task_contains_root(other, root)) {
                    self.task_cleanup_root(state, root, b)?;
                }
            }
            Value::Product(fields) => {
                for (_, value) in fields {
                    self.task_cleanup_value(state, value, replacement, b)?;
                }
            }
            Value::FallibleTaskOwned(_) | Value::FallibleTask | Value::FreshTask => {
                return Err(unsupported(
                    "Task conditional birth cleanup still needs both exact owning outcome edges",
                    None,
                ));
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn task_transfer_value(
        &mut self,
        state: &mut State,
        value: &Value,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?;
        match value {
            Value::Owner {
                root,
                borrowed: false,
            } => {
                if state.owners.get(root).is_some_and(|owner| owner.task) {
                    self.update(state, root, DEAD, b)?;
                }
            }
            Value::Product(fields) => {
                for (_, value) in fields {
                    self.task_transfer_value(state, value, b)?;
                }
            }
            Value::FallibleTaskOwned(inputs) => {
                for value in inputs {
                    self.task_transfer_value(state, value, b)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn task_cleanup_root(&mut self, state: &mut State, root: &Place, b: &mut Budget) -> Check<()> {
        b.spend(state.width())?;
        let Some(owner) = state.owners.get(root).cloned() else {
            return Ok(());
        };
        if !owner.task || !owner.owned || owner.states == DEAD {
            return Ok(());
        }
        if owner.states == 0 {
            return Err(bad(
                "original Task cleanup has no possible current generation",
            ));
        }
        root.binder.charge_clone(b)?;
        b.spend(root.fields.iter().fold(1usize, |n, field| {
            n.saturating_add(field.len()).saturating_add(1)
        }))?;
        if owner.states & !(CLOSED | DEAD) == 0 {
            if !self.async_tasks.contains(root) {
                self.closed_tasks.insert(root.clone());
            }
        } else {
            let prototype = self
                .original
                .prototypes
                .get(&self.current)
                .ok_or_else(|| bad("Task cleanup has no fixed original effect upper"))?;
            if prototype.call_kind != CallKind::Suspend {
                return Err(bad(
                    "original Task cleanup edge lacks Closed and cannot suspend",
                ));
            }
            // This is the sealed asynchronous cleanup edge under an explicit
            // Suspend ABI. It supplies no Closed or global SyncDrop fact. The
            // actual continuation/frame/loans are mandatory later consumers.
            self.closed_tasks.remove(root);
            self.async_tasks.insert(root.clone());
        }
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
            if let Some(value) = state.values.get(&binder).cloned() {
                self.task_cleanup_value(state, &value, None, b)?;
            }
            state.values.remove(&binder);
        }
        // A join that lost the value selector cannot silently erase an owned
        // Task. All such orphaned generations must already be retired.
        let orphaned = state
            .owners
            .iter()
            .filter(|(root, owner)| {
                owner.task
                    && owner.owned
                    && owner.states != DEAD
                    && !before.contains(&root.binder)
                    && !state
                        .values
                        .values()
                        .any(|value| task_contains_root(value, root))
            })
            .map(|(root, _)| root.clone())
            .collect::<Vec<_>>();
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
            b.spend(values.len())?;
            let inputs = values.iter().skip(1).cloned().collect::<Vec<_>>();
            if kind == Intrinsic::TaskCreateOwned {
                return Ok(Some(Value::FallibleTaskOwned(inputs)));
            }
            for value in &inputs {
                self.task_constructor_cleanup(state, value, b)?;
                self.task_transfer_value(state, value, b)?;
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
    fn task_constructor_cleanup(&self, state: &State, value: &Value, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        match value {
            Value::Owner {
                root,
                borrowed: false,
            } => {
                let owner = state.owner(root, b)?;
                if owner.task && owner.states != CLOSED {
                    return Err(bad(
                        "Task creation failure cannot synchronously clean a non-Closed input generation; the owning failure form must retain that input",
                    ));
                }
            }
            Value::Product(fields) => {
                for (_, value) in fields {
                    self.task_constructor_cleanup(state, value, b)?;
                }
            }
            Value::FallibleTaskOwned(_) | Value::FreshTask | Value::FallibleTask => {
                return Err(unsupported(
                    "Task input still has an unresolved conditional owner generation",
                    None,
                ));
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn task_failure_payload(
        &mut self,
        state: &mut State,
        value: &Value,
        is_error: bool,
        b: &mut Budget,
    ) -> Check<Value> {
        match value {
            Value::FallibleTaskOwned(inputs) => {
                b.spend(inputs.len())?;
                if is_error {
                    Ok(Value::Product(vec![
                        ("reason".into(), Value::Plain),
                        (
                            "inputs".into(),
                            Value::Product(
                                inputs
                                    .iter()
                                    .enumerate()
                                    .map(|(index, value)| (index.to_string(), value.clone()))
                                    .collect(),
                            ),
                        ),
                    ]))
                } else {
                    for input in inputs {
                        self.task_transfer_value(state, input, b)?;
                    }
                    Ok(Value::FreshTask)
                }
            }
            Value::FallibleTask => Ok(if is_error {
                Value::Plain
            } else {
                Value::FreshTask
            }),
            _ => Ok(value.clone()),
        }
    }
    pub(super) fn task_place(
        &self,
        node: &SemanticOriginalExpr,
        b: &mut Budget,
    ) -> Check<Option<Place>> {
        b.spend(1)?;
        if let Some(place) = self.original.value_places.get(&(self.current, node.id)) {
            return Ok(Some(place.clone()));
        }
        Ok(self.binder(node, b)?.map(|binder| root_place(&binder)))
    }
    pub(super) fn task_take_place(
        &self,
        state: &mut State,
        place: &Place,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(
            place
                .fields
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(1),
        )?;
        if place.fields.is_empty() {
            state.values.remove(&place.binder);
            return Ok(());
        }
        let mut value = state
            .values
            .get_mut(&place.binder)
            .ok_or_else(|| bad("Task projected move has no real original containing binding"))?;
        for name in &place.fields {
            let Value::Product(fields) = value else {
                return Err(unsupported(
                    "Task projected move crosses an unsupported original storage constructor",
                    None,
                ));
            };
            value = &mut fields
                .iter_mut()
                .find(|(field, _)| field == name)
                .ok_or_else(|| bad("Task projected move selects another original field"))?
                .1;
        }
        *value = Value::Plain;
        Ok(())
    }
    pub(super) fn task_assign_place(
        &mut self,
        state: &mut State,
        place: &Place,
        value: Value,
        b: &mut Budget,
    ) -> Check<()> {
        if place.fields.is_empty() {
            return self.bind(state, place.binder.clone(), value, b);
        }
        b.spend(state.width())?;
        let mut whole = state
            .values
            .get(&place.binder)
            .cloned()
            .ok_or_else(|| bad("Task projected assignment has no real containing binding"))?;
        let mut destination = &mut whole;
        for name in &place.fields {
            let Value::Product(fields) = destination else {
                return Err(unsupported(
                    "Task projected assignment crosses an unsupported original storage constructor",
                    None,
                ));
            };
            destination = &mut fields
                .iter_mut()
                .find(|(field, _)| field == name)
                .ok_or_else(|| bad("Task projected assignment selects another original field"))?
                .1;
        }
        self.task_cleanup_value(state, destination, Some(&value), b)?;
        *destination = self.task_bind_place(state, place, value, b)?;
        state.values.insert(place.binder.clone(), whole);
        Ok(())
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
                let Some(value) = state.values.get(&binding.identity).cloned() else {
                    continue;
                };
                self.task_transfer_value(state, &value, b)?;
                state.values.remove(&binding.identity);
            }
        }
        Ok(())
    }
}

pub(super) fn task_contains_root(value: &Value, root: &Place) -> bool {
    match value {
        Value::Owner {
            root: actual,
            borrowed: false,
        } => actual == root,
        Value::Product(fields) => fields
            .iter()
            .any(|(_, value)| task_contains_root(value, root)),
        Value::FallibleTaskOwned(inputs) => {
            inputs.iter().any(|value| task_contains_root(value, root))
        }
        _ => false,
    }
}

pub(super) fn task_borrow_value(value: Value) -> Value {
    match value {
        Value::Owner { root, .. } => Value::Owner {
            root,
            borrowed: true,
        },
        Value::Product(fields) => Value::Product(
            fields
                .into_iter()
                .map(|(name, value)| (name, task_borrow_value(value)))
                .collect(),
        ),
        value => value,
    }
}

pub(super) fn owner_value(value: &Value) -> bool {
    match value {
        Value::Owner { .. }
        | Value::Fresh
        | Value::FreshTask
        | Value::FallibleTask
        | Value::FallibleTaskOwned(_) => true,
        Value::Product(fields) => fields.iter().any(|(_, value)| owner_value(value)),
        _ => false,
    }
}
pub(super) fn task_roots(value: &Value, roots: &mut Vec<Place>, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    match value {
        Value::Owner { root, .. } => {
            b.spend(
                root.fields
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(1),
            )?;
            roots.push(root.clone());
        }
        Value::Product(fields) => {
            for (_, value) in fields {
                task_roots(value, roots, b)?;
            }
        }
        _ => {}
    }
    Ok(())
}
