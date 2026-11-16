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
            Ty::Opaque(owner,arguments)=>{
                let hidden=self.opaque_hidden(module,context,owner,&arguments,active,b)?;
                self.task_shape(module,context,&hidden,borrowed,active,b,depth+1)?
            },
            Ty::Closure(owner,arguments,sealed)=>{
                let moves=self.creator_moves.get(&owner).copied().ok_or_else(||bad("Task closure shape lacks its original capture mode"))?;
                let mut product=Vec::new();
                for(name,ty)in self.closure_capture_fields(module,context,owner,&arguments,&sealed,b)?{
                    product.push((name,self.task_shape(module,context,&ty,borrowed||!moves,active,b,depth+1)?));
                }
                Shape::Product(product)
            },
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
            Value::FreshTask(sources) => {
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
                        sources,
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
                b.spend(source_width(&state.owner(&root,b)?.sources))?;
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
                        sources:owner.sources,
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
                if state.owners.get(place).is_some_and(|owner|owner.states!=DEAD){
                    return Err(bad("Task input/result entry reuses an unsettled owner generation"));
                }
                let generation = self.generation(b)?;
                state.owners.insert(
                    place.clone(),
                    Owner {
                        states: LIVE,
                        generation,
                        task: true,
                        owned: !*borrowed,
                        sources:match &place.binder{Binder::Parameter{definition,ordinal}=>BTreeSet::from([TaskSource::Input{definition:*definition,ordinal:*ordinal,path:place.fields.clone()}]),_=>unknown_sources()},
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
            Value::FallibleTaskOwned(_,_) | Value::FallibleTask(_) | Value::FreshTask(_) => {
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
            Value::FallibleTaskOwned(inputs,_) => {
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
                b.spend(source_width(&owner.sources))?;
                if owner.sources.is_empty()||!owner.sources.iter().all(|source|matches!(source,TaskSource::Birth{definition,node}if self.synchronous_births.contains(&(*definition,*node)))){
                    return Err(bad("original Task cleanup edge lacks Closed, a complete synchronous birth source, and cannot suspend"));
                }
                self.synchronous_tasks.insert(root.clone());
            }else{
            // This is the sealed asynchronous cleanup edge under an explicit
            // Suspend ABI. It supplies no Closed or global SyncDrop fact. The
            // actual continuation/frame/loans are mandatory later consumers.
            self.closed_tasks.remove(root);
            self.async_tasks.insert(root.clone());
            }
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
            .filter(|(root, owner)| owner.task && owner.owned && owner.states != DEAD
                // Nonconsuming activations retain the persistent environment.
                // Its owning slots still require real cleanup on replacement;
                // only invocation exit leaves those same occupants in place.
                && !(matches!(root.binder,Binder::Capture{definition,..}if definition==self.current)
                    && self.original.prototypes.get(&self.current).is_some_and(|prototype|matches!(prototype.callable_kind,Some(CallableKind::Fn|CallableKind::FnMut)))))
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
        node:NodeId,
        values: &[Value],
        state: &mut State,
        b: &mut Budget,
    ) -> Check<Option<Value>> {
        if matches!(kind, Intrinsic::TaskCreate | Intrinsic::TaskCreateOwned) {
            b.spend(values.len())?;
            let inputs = values.iter().skip(1).cloned().collect::<Vec<_>>();
            let sources=BTreeSet::from([TaskSource::Birth{definition:self.current,node}]);
            if kind == Intrinsic::TaskCreateOwned {
                return Ok(Some(Value::FallibleTaskOwned(inputs,sources)));
            }
            for value in &inputs {
                self.task_constructor_cleanup(state, value, b)?;
                self.task_transfer_value(state, value, b)?;
            }
            return Ok(Some(Value::FallibleTask(sources)));
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
        b.spend(source_width(&state.owner(root,b)?.sources))?;
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
            Value::FallibleTaskOwned(_,_) | Value::FreshTask(_) | Value::FallibleTask(_) => {
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
            Value::FallibleTaskOwned(inputs,sources) => {
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
                    Ok(Value::FreshTask(sources.clone()))
                }
            }
            Value::FallibleTask(sources) => Ok(if is_error {
                Value::Plain
            } else {
                Value::FreshTask(sources.clone())
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
    ) -> Check<Value> {
        let moves=self
            .original
            .creator_moves
            .get(&child)
            .copied()
            .ok_or_else(||bad("Task capture lacks its exact original capture mode"))?;
        let mut fields=Vec::new();
        if let Some(free) = self.original.free_values.get(&child) {
            for(name,binding)in free {
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
                let value=state.values.get(&binding.identity).cloned().unwrap_or(Value::Plain);
                if moves{state.values.remove(&binding.identity);}
                let value=if moves{value}else{task_borrow_value(value)};
                fields.push((CheckedOriginal::capture_field(name,b)?,value));
            }
        }
        // Binding, Return or consuming invocation transfers these exact live
        // generations once. Do not kill them and manufacture fresh Open ones.
        Ok(Value::Product(fields))
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
        Value::FallibleTaskOwned(inputs,_) => {
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
        | Value::FreshTask(_)
        | Value::FallibleTask(_)
        | Value::FallibleTaskOwned(_,_) => true,
        Value::Product(fields) => fields.iter().any(|(_, value)| owner_value(value)),
        _ => false,
    }
}
pub(super) fn task_roots(value: &Value, roots: &mut Vec<(Place,bool)>, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    match value {
        Value::Owner { root, borrowed } => {
            b.spend(
                root.fields
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(1),
            )?;
            roots.push((root.clone(),*borrowed));
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

impl Engine<'_>{
    pub(super) fn invalidate_task_source(&self,state:&mut State,root:&Place,b:&mut Budget)->Check<()>{
        let owner=state.owner(root,b)?;b.spend(source_width(&owner.sources))?;
        if owner.task{state.owners.get_mut(root).ok_or_else(||bad("source invalidation lost its exact Task owner"))?.sources=unknown_sources();}
        Ok(())
    }
    pub(super) fn result_sources(&self,state:&State,value:&Value,b:&mut Budget)->Check<ResultSources>{
        fn walk(state:&State,value:&Value,path:&mut Vec<String>,out:&mut ResultSources,b:&mut Budget)->Check<()>{
            b.spend(path.iter().fold(1usize,|n,s|n.saturating_add(s.len())))?;
            match value{
                Value::Owner{root,..}=>{let owner=state.owner(root,b)?;if owner.task{b.spend(source_width(&owner.sources))?;out.insert(path.clone(),owner.sources.clone());}},
                Value::FreshTask(sources)=>{b.spend(source_width(sources))?;out.insert(path.clone(),sources.clone());},
                Value::Product(fields)=>for(name,value)in fields{path.push(name.clone());walk(state,value,path,out,b)?;path.pop();},
                _=>{},
            }
            Ok(())
        }
        let mut out=BTreeMap::new();walk(state,value,&mut vec![],&mut out,b)?;Ok(out)
    }
    pub(super) fn record_result_sources(&mut self,state:&State,value:&Value,b:&mut Budget)->Check<()>{
        let incoming=self.result_sources(state,value,b)?;
        if let Some(previous)=&mut self.returned_sources{
            b.spend(previous.len().saturating_add(incoming.len()))?;
            for(path,sources)in previous.iter_mut(){b.spend(source_width(sources))?;if let Some(other)=incoming.get(path){b.spend(source_width(other))?;sources.extend(other.iter().cloned());}else{sources.insert(TaskSource::Unknown);}}
            for(path,mut sources)in incoming{if !previous.contains_key(&path){sources.insert(TaskSource::Unknown);previous.insert(path,sources);}}
        }else{self.returned_sources=Some(incoming);}
        Ok(())
    }
    pub(super) fn apply_result_sources(&self,state:&mut State,value:&Value,summary:&ResultSources,inputs:&[ResultSources],target:SemanticDefinitionId,b:&mut Budget)->Check<()>{
        fn walk(state:&mut State,value:&Value,path:&mut Vec<String>,summary:&ResultSources,inputs:&[ResultSources],target:SemanticDefinitionId,b:&mut Budget)->Check<()>{
            b.spend(path.iter().fold(1usize,|n,s|n.saturating_add(s.len())))?;
            match value{
                Value::Owner{root,..}=>{
                    let mut sources=Sources::new();
                    if let Some(rows)=summary.get(path){for source in rows{b.spend(1)?;match source{
                        TaskSource::Input{definition,ordinal,path}if *definition==target=>{
                            if let Some(actual)=inputs.get(*ordinal as usize).and_then(|input|input.get(path)){b.spend(source_width(actual))?;sources.extend(actual.iter().cloned());}else{sources.insert(TaskSource::Unknown);}
                        },
                        TaskSource::Input{..}|TaskSource::Unknown=>{sources.insert(TaskSource::Unknown);},
                        birth@TaskSource::Birth{..}=>{sources.insert(birth.clone());},
                    }}}else{sources.insert(TaskSource::Unknown);}
                    if sources.is_empty(){sources.insert(TaskSource::Unknown);}
                    let owner=state.owners.get_mut(root).ok_or_else(||bad("helper Return source lost its exact newly initialized owner"))?;
                    if owner.task{owner.sources=sources;}
                },
                Value::Product(fields)=>for(name,value)in fields{path.push(name.clone());walk(state,value,path,summary,inputs,target,b)?;path.pop();},
                _=>{},
            }
            Ok(())
        }
        // The caller supplies the callee identity explicitly, never infers it
        // from a compatible type, source name, or current concrete occupant.
        walk(state,value,&mut vec![],summary,inputs,target,b)
    }
}

impl CheckedOriginal{
    /// The restricted default-close route requires a closed universal source
    /// worker graph. It never specializes an abstract incoming Task from its
    /// concrete caller. More general retained/unknown input frames keep their
    /// existing Closed or Suspend obligation.
    pub(super) fn synchronous_task_births(&mut self,module:&TypedModule,context:&CheckedContext,index:&index::Index<'_>,active:&mut ActiveQueries,b:&mut Budget)->Check<BTreeSet<(SemanticDefinitionId,NodeId)>>{
        let mut out=BTreeSet::new();let mut workers=BTreeMap::new();
        let sites=self.intrinsic_calls.iter().filter(|(_,use_)|matches!(use_.kind,Intrinsic::TaskCreate|Intrinsic::TaskCreateOwned)).map(|(key,use_)|(*key,*use_)).collect::<Vec<_>>();b.spend(sites.len())?;
        for((owner,site),use_)in sites{
            let Some(worker)=use_.worker else{return Err(bad("original Task birth loses its exact worker source"));};
            let function=index.definitions.get(&owner).ok_or_else(||bad("original Task birth is outside the complete source inventory"))?;
            let Some(index::Node::Expression(node))=function.nodes.get(&site)else{return Err(bad("original Task birth has no exact source Call"));};
            let CoreExprKind::Call{arguments,..}=&node.kind else{return Err(bad("original Task birth selects a non-Call source occurrence"));};
            let prototype=self.prototypes.get(&worker).ok_or_else(||bad("original Task birth has no complete worker prototype"))?;
            if arguments.len()!=prototype.parameters.len().saturating_add(1){return Err(bad("original Task birth changes its complete source inputs"));}
            // Every source input and every physical field must be closed here;
            // an abstract Task/registration or unknown callback is not a fact
            // about its caller. Creation's failure/SyncDrop obligations remain.
            let proved=if let Some(proved)=workers.get(&worker){*proved}else{let proved=self.closed_source_worker(module,context,index,worker,active,b)?;workers.insert(worker,proved);proved};
            if proved{out.insert((owner,node.id));}
        }
        Ok(out)
    }
    fn closed_source_type(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,pending:&mut Vec<SemanticDefinitionId>,active:&mut ActiveQueries,b:&mut Budget,depth:usize)->Check<bool>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original synchronous worker type depth exceeded"));}
        let ty=self.resolved(ty,b)?;
        Ok(match ty{
            Ty::Atom(Type::Unit|Type::Bool|Type::I8|Type::I16|Type::I32|Type::I64|Type::Isize|Type::U8|Type::U16|Type::U32|Type::U64|Type::Usize|Type::F32|Type::F64|Type::ByteBuffer)|Ty::SealedError(_)=>true,
            Ty::Function(id)=>{pending.push(id);true},
            Ty::Tuple(fields)=>{for field in fields{if !self.closed_source_type(module,context,&field,pending,active,b,depth+1)?{return Ok(false);}}true},
            Ty::Address{inner,..}|Ty::Desc{inner,..}|Ty::Managed(inner)|Ty::Array(inner,_)|Ty::Slice(inner)|Ty::Store(inner)=>self.closed_source_type(module,context,&inner,pending,active,b,depth+1)?,
            Ty::ErrorUnion(error,result)=>self.closed_source_type(module,context,&error,pending,active,b,depth+1)?&&self.closed_source_type(module,context,&result,pending,active,b,depth+1)?,
            Ty::Code{parameters,result,..}=>{for parameter in parameters{if !self.closed_source_type(module,context,&parameter,pending,active,b,depth+1)?{return Ok(false);}}self.closed_source_type(module,context,&result,pending,active,b,depth+1)?},
            Ty::Nominal(id,args)=>{
                let nominal=context.definition(module,id,b)?;
                let SemanticDefinitionBody::Nominal{body,..}=&nominal.body else{return Err(bad("synchronous source nominal has no original complete declaration"));};
                let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=args;
                match body{
                    SemanticNominalBody::Product{fields}=>for field in fields{let ty=self.ty(module,context,&field.ty,&selected,active,b)?;if !self.closed_source_type(module,context,&ty,pending,active,b,depth+1)?{return Ok(false);}},
                    SemanticNominalBody::Enum{variants}=>for variant in variants{for field in &variant.fields{let ty=self.ty(module,context,field,&selected,active,b)?;if !self.closed_source_type(module,context,&ty,pending,active,b,depth+1)?{return Ok(false);}}},
                }
                // A custom destructor's entire original dependency graph is
                // part of default close, including scalar observer leaves.
                let program=module.semantic.as_ref().ok_or_else(||bad("synchronous source Drop lacks its complete original directory"))?;
                for scope in &program.definitions{b.spend(1)?;let SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{self_type,trait_kind:SemanticBuiltinTrait::Drop})=&scope.body else{continue;};let mut ty=self_type;while let SemanticTypeRecipe::Qualified{storage,..}=ty{b.spend(1)?;ty=storage;}
                    if matches!(ty,SemanticTypeRecipe::Nominal{definition,..}if *definition==id){for method in &program.definitions{b.spend(1)?;if method.parent==Some(scope.id)&&method.kind==SemanticDefinitionKind::Method{pending.push(method.id);}}}
                }
                true
            },
            Ty::Opaque(owner,args)=>{let hidden=self.opaque_hidden(module,context,owner,&args,active,b)?;self.closed_source_type(module,context,&hidden,pending,active,b,depth+1)?},
            Ty::Closure(owner,args,sealed)=>{
                pending.push(owner);
                for(_,capture)in self.closure_capture_fields(module,context,owner,&args,&sealed,b)?{if !self.closed_source_type(module,context,&capture,pending,active,b,depth+1)?{return Ok(false);}}true
            },
            // Abstract variables, retained Tasks/registrations and erased
            // callbacks do not supply a universally closed graph premise.
            _=>false,
        })
    }
    fn closed_source_worker(&mut self,module:&TypedModule,context:&CheckedContext,index:&index::Index<'_>,root:SemanticDefinitionId,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        fn scalar(ty:&Ty)->bool{matches!(ty,Ty::Atom(Type::Unit|Type::Bool|Type::I8|Type::I16|Type::I32|Type::I64|Type::Isize|Type::U8|Type::U16|Type::U32|Type::U64|Type::Usize|Type::F32|Type::F64))}
        let mut pending=vec![root];let mut seen=BTreeSet::new();
        while let Some(id)=pending.pop(){
            b.spend(seen.len().saturating_add(1))?;if !seen.insert(id){continue;}
            let Some(function)=index.definitions.get(&id)else{return Ok(false);};
            let prototype=self.prototypes.get(&id).ok_or_else(||bad("synchronous worker dependency lacks its original signature"))?;
            let mut types=Vec::new();for ty in prototype.parameters.iter().chain(std::iter::once(&prototype.result)){types.push(self.resolved(ty,b)?);}
            if !matches!(function.body,SemanticOriginalFunctionBody::Defined(_)){
                // A scalar C observer cannot receive a Task/registration head,
                // pointer, callback or environment. General foreign authority
                // is not inferred from this finite leaf profile.
                if !matches!(function.body,SemanticOriginalFunctionBody::External)||function.definition.kind!=SemanticDefinitionKind::Function||function.definition.parent.is_some()||prototype.call_kind!=CallKind::Direct||!types.iter().all(scalar){return Ok(false);}
                continue;
            }
            for ty in &types{if !self.closed_source_type(module,context,ty,&mut pending,active,b,0)?{return Ok(false);}}
            let captures=self.free_values.get(&id).map(|values|values.values().map(|value|self.resolved(&value.ty,b)).collect::<Check<Vec<_>>>()).transpose()?.unwrap_or_default();
            for ty in captures{if !self.closed_source_type(module,context,&ty,&mut pending,active,b,0)?{return Ok(false);}}
            for node in function.expressions.values(){
                b.spend(1)?;
                if let Some(ty)=self.expressions.get(&(id,node.id)){let ty=self.resolved(ty,b)?;if !self.closed_source_type(module,context,&ty,&mut pending,active,b,0)?{return Ok(false);}}
                if !matches!(node.kind,CoreExprKind::Call{..}){continue;}
                let(_,site)=self.original_call_site(id,node.id,b)?;
                if let Some(use_)=self.intrinsic_calls.get(&(id,site)){
                    if matches!(use_.kind,Intrinsic::AsyncStart|Intrinsic::AsyncValid|Intrinsic::AsyncRequest|Intrinsic::AsyncPoll|Intrinsic::AsyncFinish|Intrinsic::AsyncFinishChecked|Intrinsic::ThreadScope|Intrinsic::OwnedDyn|Intrinsic::CCallback){return Ok(false);}
                    if let Some(worker)=use_.worker{pending.push(worker);}
                }else if let Some(callee)=self.call_targets.get(&(id,site)){pending.push(*callee);}else{return Ok(false);}
            }
        }
        Ok(true)
    }
}
