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
    FallibleTask,
    Product(Vec<(String, Shape)>),
}
impl Shape {
    pub(in crate::semantic::original) fn has_task(&self) -> bool {
        match self {
            Self::Plain => false,
            Self::Task { .. } | Self::FallibleTask => true,
            Self::Product(fields) => fields.iter().any(|(_, field)| field.has_task()),
        }
    }
}
type Birth=(SemanticDefinitionId,NodeId);
#[derive(Default)]
pub(super) struct SynchronousBirths{
    known:BTreeSet<Birth>,
    conditional:BTreeSet<Birth>,
    observed:BTreeMap<Birth,Sources>,
    pending:Vec<(SemanticDefinitionId,Place,u64,u8,Sources)>,
}
impl SynchronousBirths{
    fn observe(&mut self,birth:Birth,shapes:&[Shape],values:&[Value],state:&State,b:&mut Budget)->Check<()>{
        fn collect(shape:&Shape,value:&Value,state:&State,out:&mut Sources,b:&mut Budget)->Check<()>{
            b.spend(1)?;
            fn add(task:&PendingTask,out:&mut Sources,b:&mut Budget)->Check<()>{b.spend(task.width())?;if task.states&!(CLOSED|DEAD)!=0{if task.sources.is_empty(){out.insert(TaskSource::Unknown);}else{out.extend(task.sources.iter().cloned());}}Ok(())}
            match(shape,value){
                (Shape::Task{borrowed:expected_borrow},Value::Owner{root,borrowed})=>{
                    let owner=state.owner(root,b)?;
                    // A weak description and a strong borrow share this shape.
                    // Neither carries a stable-generation lease here; even a
                    // currently Closed borrowed occupant is not a transferred
                    // frame owner whose future source is fixed by this proof.
                    if !owner.task||*expected_borrow||*borrowed||!owner.owned{out.insert(TaskSource::Unknown);}
                    else if owner.states&!(CLOSED|DEAD)!=0{b.spend(source_width(&owner.sources))?;if owner.sources.is_empty(){out.insert(TaskSource::Unknown);}else{out.extend(owner.sources.iter().cloned());}}
                },
                (Shape::Task{borrowed:false},Value::FreshTask(task))|(Shape::FallibleTask,Value::FallibleTask(task))=>add(task,out,b)?,
                (Shape::Product(expected),Value::Product(actual))=>{
                    b.spend(expected.len().saturating_mul(actual.len().saturating_add(1)))?;
                    if expected.len()!=actual.len(){out.insert(TaskSource::Unknown);return Ok(());}
                    for(name,shape)in expected{let mut rows=actual.iter().filter(|(field,_)|field==name);let Some((_,value))=rows.next()else{out.insert(TaskSource::Unknown);continue;};if rows.next().is_some(){out.insert(TaskSource::Unknown);continue;}collect(shape,value,state,out,b)?;}
                },
                (Shape::Plain,_)if !owner_value(value)=>{},
                _=>{out.insert(TaskSource::Unknown);},
            }Ok(())
        }
        if !self.conditional.contains(&birth){return Ok(());}
        let mut incoming=Sources::new();
        if shapes.len()!=values.len(){incoming.insert(TaskSource::Unknown);}else{for(shape,value)in shapes.iter().zip(values){collect(shape,value,state,&mut incoming,b)?;}}
        b.spend(source_width(&incoming).saturating_add(self.observed.len().checked_ilog2().map_or(1,|n|n as usize+1)))?;
        if self.observed.len()>=b.limits.max_nodes&&!self.observed.contains_key(&birth){return Err(limit("original Task creation-source inventory exceeded"));}
        let previous=self.observed.entry(birth).or_default();b.spend(source_width(previous))?;previous.extend(incoming);Ok(())
    }
    fn request(&mut self,definition:SemanticDefinitionId,root:&Place,owner:&Owner,b:&mut Budget)->Check<()>{
        b.spend(source_width(&owner.sources))?;
        if owner.sources.is_empty()||owner.sources.iter().any(|source|!matches!(source,TaskSource::Birth{definition,node}if self.known.contains(&(*definition,*node))||self.conditional.contains(&(*definition,*node)))){
            return Err(bad("original Task cleanup edge lacks Closed, a complete synchronous birth source, and cannot suspend").with_note(format!("original cleanup owner {definition:?}; root {root:?}; phase {}; universal sources {:?}",owner.states,owner.sources)));
        }
        if self.pending.len()>=b.limits.max_nodes{return Err(limit("original pending Task cleanup inventory exceeded"));}
        root.binder.charge_clone(b)?;b.spend(root.fields.iter().map(String::len).sum::<usize>().saturating_add(source_width(&owner.sources)).saturating_add(2))?;
        self.pending.push((definition,root.clone(),owner.generation,owner.states,owner.sources.clone()));Ok(())
    }
    pub(super) fn finish(mut self,b:&mut Budget)->Check<Vec<(SemanticDefinitionId,Place)>>{
        // Least fixed point of all-source obligations. Original Input/Unknown,
        // absent creation observations and source-only cycles are not anchors.
        let mut changed=true;
        while changed{changed=false;for(birth,sources)in &self.observed{
            b.spend(source_width(sources).saturating_add(1))?;
            if self.known.contains(birth){continue;}
            if self.conditional.contains(birth)&&sources.iter().all(|source|matches!(source,TaskSource::Birth{definition,node}if self.known.contains(&(*definition,*node)))){self.known.insert(*birth);changed=true;}
        }}
        let mut ready=Vec::new();
        for(definition,root,generation,phase,sources)in self.pending{
            b.spend(source_width(&sources).saturating_add(1))?;
            if !sources.iter().all(|source|matches!(source,TaskSource::Birth{definition,node}if self.known.contains(&(*definition,*node)))){
                return Err(bad("original Task cleanup edge lacks Closed, a complete synchronous birth source, and cannot suspend").with_note(format!("original cleanup {definition:?} {root:?} generation {generation}, phase {phase}; incomplete all-source creation dependencies {sources:?}")));
            }
            ready.push((definition,root));
        }Ok(ready)
    }
}

impl CheckedOriginal {
    pub(super) fn creation_dependency_shapes(&mut self,module:&TypedModule,context:&CheckedContext,owner:SemanticDefinitionId,site:SemanticSiteId,arguments:&[SemanticOriginalExpr],active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<Shape>>{
        let worker=self.intrinsic_calls.get(&(owner,site)).and_then(|use_|use_.worker).ok_or_else(||bad("Task dependency shapes have no original worker"))?;
        let caller=self.environment(module,context,owner,active,b)?;
        let actuals=self.calls.get(&(owner,site)).ok_or_else(||bad("Task dependency shapes omit the original full mixed worker arguments"))?;
        let actuals=self.apply_arguments(actuals,&caller,0,b)?;
        let selected=self.synchronous_environment(module,context,worker,&actuals,&[caller],active,b)?;
        let mut shapes=Vec::new();
        for(ordinal,argument)in arguments.iter().enumerate(){
            if ordinal==0{
                // task_worker resolves the code selector's base, not an
                // executable value at its written generic-application shell.
                // Reconfirm that exact static role; this proves no capture
                // dependency and does not erase any runtime input Closure.
                let base=match &argument.kind{
                    CoreExprKind::TypeApply{callee,..}|CoreExprKind::GenericApply{callee,..}=>callee.as_ref(),
                    _=>argument,
                };
                let source=self.expressions.get(&(owner,base.id)).ok_or_else(||bad("Task worker selector has no complete original static base type"))?;
                if !matches!(self.resolved(source,b)?,Ty::Function(id)if id==worker)
                    ||self.call_targets.get(&(owner,site))!=Some(&worker)
                    ||!context.definition(module,worker,b)?.environment.is_empty(){
                    return Err(bad("Task worker source differs from its independently selected static code role"));
                }
                shapes.push(Shape::Plain);continue;
            }
            let actual=self.expressions.get(&(owner,argument.id)).ok_or_else(||bad("Task creation has no complete original worker/input expression type"))?.clone();
            let mut shape=self.task_shape(module,context,&actual,false,active,b,0)?;
            if let Some(selected)=&selected{
                    let parameter=self.prototypes.get(&worker).and_then(|prototype|prototype.parameters.get(ordinal-1)).ok_or_else(||bad("Task dependency shape changes its original worker arity"))?;
                    let parameter=self.apply_type(parameter,selected,b)?;
                    let required=self.task_shape(module,context,&parameter,false,active,b,0)?;
                    // A source Type variable/equality cannot erase a Task slot
                    // required by the independently selected worker prototype.
                    if required.has_task(){shape=required;}
            }
            shapes.push(shape);
        }Ok(shapes)
    }
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
            Ty::ErrorUnion(error,result) if !borrowed => {
                let success=self.task_shape(module,context,&result,false,active,b,depth+1)?;
                let env=self.environment(module,context,self.current.ok_or_else(||bad("conditional Task shape has no original declaration Gamma"))?,active,b)?;
                if matches!(success,Shape::Task{borrowed:false})&&self.is_copy(module,context,&error,&env,active,b)? {
                    // Copy is a universal no-owning-error premise, not proof
                    // that the success arm is freshly created or already closed.
                    Shape::FallibleTask
                }else{Shape::Plain}
            },
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
            Value::FreshTask(task) => {
                if task.states==0||task.states&!LIVE!=0||task.sources.is_empty(){return Err(bad("pending Task omits a live phase or complete universal source"));}
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
                        states: task.states,
                        generation,
                        task: true,
                        owned: true,
                        sources:task.sources,
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
                let source_width_work = source_width(&state.owner(&root,b)?.sources);
                b.spend(source_width_work)?;
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
            Shape::FallibleTask => Value::FallibleTask(PendingTask{
                states:LIVE,
                sources:match &place.binder{Binder::Parameter{definition,ordinal}=>BTreeSet::from([TaskSource::Input{definition:*definition,ordinal:*ordinal,path:place.fields.clone()}]),_=>unknown_sources()},
            }),
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
                self.synchronous_births.request(self.current,root,&owner,b)?;
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
            if matches!(root.binder,Binder::Capture{definition,..}if definition==self.current)
                &&self.original.prototypes.get(&self.current).is_some_and(|prototype|prototype.callable_kind==Some(CallableKind::FnOnce)){
                // Final environment retirement belongs to the complete consume
                // operation. It is not a Direct user-body cleanup or a new
                // Closed/NoExternal fact about this incoming capture.
                root.binder.charge_clone(b)?;b.spend(root.fields.iter().map(String::len).sum::<usize>().saturating_add(1))?;
                self.retiring_captures.insert(root.clone());self.update(state,&root,DEAD,b)?;
            }else{self.task_cleanup_root(state, &root, b)?;}
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
            let shapes=self.creation_shapes.get(&node).ok_or_else(||bad("Task creation omits its complete original input/capture shape directory"))?;
            self.synchronous_births.observe((self.current,node),shapes,values,state,b)?;
            b.spend(values.len())?;
            let inputs = values.iter().skip(1).cloned().collect::<Vec<_>>();
            let sources=PendingTask{states:OPEN,sources:BTreeSet::from([TaskSource::Birth{definition:self.current,node}])};
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
        let source_width_work = source_width(&state.owner(root,b)?.sources);
        b.spend(source_width_work)?;
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

pub(super) fn pending_value(value:&Value)->bool{
    match value{Value::FreshTask(_)|Value::FallibleTask(_)|Value::FallibleTaskOwned(_,_)=>true,Value::Product(fields)=>fields.iter().any(|(_,value)|pending_value(value)),_=>false}
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
                Value::FreshTask(task)|Value::FallibleTask(task)=>{b.spend(task.width())?;out.insert(path.clone(),task.sources.clone());},
                Value::Product(fields)=>for(name,value)in fields{path.push(name.clone());walk(state,value,path,out,b)?;path.pop();},
                _=>{},
            }
            Ok(())
        }
        let mut out=BTreeMap::new();walk(state,value,&mut vec![],&mut out,b)?;Ok(out)
    }
    pub(super) fn record_result_sources(&mut self,state:&State,value:&Value,node:Option<&SemanticOriginalExpr>,b:&mut Budget)->Check<()>{
        b.spend(self.original.injections.len().checked_ilog2().map_or(1,|width|width as usize+1))?;
        if self.fallible_task_result&&node.is_some_and(|node|self.original.injections.get(&(self.current,node.id)).is_some_and(|(failed,_)|*failed)){
            // This exact original injection returns the independently proved
            // nonowning error arm. It is not a missing success-source path.
            return Ok(());
        }
        let incoming=self.result_sources(state,value,b)?;
        if let Some(previous)=&mut self.returned_sources{
            b.spend(previous.len().saturating_add(incoming.len()))?;
            for(path,sources)in previous.iter_mut(){b.spend(source_width(sources))?;if let Some(other)=incoming.get(path){b.spend(source_width(other))?;sources.extend(other.iter().cloned());}else{sources.insert(TaskSource::Unknown);}}
            for(path,mut sources)in incoming{if !previous.contains_key(&path){sources.insert(TaskSource::Unknown);previous.insert(path,sources);}}
        }else{self.returned_sources=Some(incoming);}
        Ok(())
    }
    pub(super) fn apply_result_sources(&self,state:&mut State,value:&mut Value,summary:&ResultSources,inputs:&[ResultSources],target:SemanticDefinitionId,b:&mut Budget)->Check<()>{
        fn sources(path:&[String],summary:&ResultSources,inputs:&[ResultSources],target:SemanticDefinitionId,b:&mut Budget)->Check<Sources>{
            let mut sources=Sources::new();
                    if let Some(rows)=summary.get(path){for source in rows{b.spend(1)?;match source{
                        TaskSource::Input{definition,ordinal,path}if *definition==target=>{
                            if let Some(actual)=inputs.get(*ordinal as usize).and_then(|input|input.get(path)){b.spend(source_width(actual))?;sources.extend(actual.iter().cloned());}else{sources.insert(TaskSource::Unknown);}
                        },
                        TaskSource::Input{..}|TaskSource::Unknown=>{sources.insert(TaskSource::Unknown);},
                        birth@TaskSource::Birth{..}=>{sources.insert(birth.clone());},
                    }}}else{sources.insert(TaskSource::Unknown);}
                    if sources.is_empty(){sources.insert(TaskSource::Unknown);}
            Ok(sources)
        }
        fn walk(state:&mut State,value:&mut Value,path:&mut Vec<String>,summary:&ResultSources,inputs:&[ResultSources],target:SemanticDefinitionId,b:&mut Budget)->Check<()>{
            b.spend(path.iter().fold(1usize,|n,s|n.saturating_add(s.len())))?;
            match value{
                Value::Owner{root,..}=>{
                    let sources=sources(path,summary,inputs,target,b)?;
                    let owner=state.owners.get_mut(root).ok_or_else(||bad("helper Return source lost its exact newly initialized owner"))?;
                    if owner.task{owner.sources=sources;}
                },
                Value::FreshTask(task)|Value::FallibleTask(task)=>task.sources=sources(path,summary,inputs,target,b)?,
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

// One original execution world and the exact incoming call-boundary source
// worlds which can explain a transported Closure's declared parent. They carry
// no runtime roots or cleanup capability; ambiguous creator domains stay unknown.
#[derive(Clone,PartialEq,Eq)]
struct SynchronousWorld {world:Env,sources:Vec<Env>}

#[cfg(test)]
mod source_world_tests{
    use super::*;
    fn birth(index:u32)->Birth{(SemanticDefinitionId{module:zil_base::ModuleId(0),index:0},NodeId(index))}
    fn source(value:Birth)->TaskSource{TaskSource::Birth{definition:value.0,node:value.1}}
    fn pending(sources:Sources)->Value{Value::FreshTask(PendingTask{states:OPEN,sources})}
    fn request(registry:&mut SynchronousBirths,value:Birth,generation:u64,b:&mut Budget){
        let root=root_place(&Binder::Parameter{definition:value.0,ordinal:0});
        registry.request(value.0,&root,&Owner{states:OPEN,generation,task:true,owned:true,sources:BTreeSet::from([source(value)])},b).unwrap();
    }
    #[test]
    fn creation_dependencies_require_all_universal_sources_and_keep_each_cleanup_generation(){
        let limits=Limits::default();let mut b=Budget::new(&limits);let state=State::default();let base=birth(1);let a=birth(2);let z=birth(3);
        let mut safe=SynchronousBirths{known:BTreeSet::from([base]),conditional:BTreeSet::from([a]),..SynchronousBirths::default()};
        safe.observe(a,&[Shape::Task{borrowed:false}],&[pending(BTreeSet::from([source(base)]))],&state,&mut b).unwrap();request(&mut safe,a,1,&mut b);assert_eq!(safe.finish(&mut b).unwrap().len(),1);
        for hostile in 0..4{
            let mut proof=SynchronousBirths{known:BTreeSet::from([base]),conditional:BTreeSet::from([a,z]),..SynchronousBirths::default()};
            let unsafe_source=if hostile==0{TaskSource::Input{definition:a.0,ordinal:0,path:vec![]}}else{TaskSource::Unknown};
            match hostile{
                0=>proof.observe(a,&[Shape::Task{borrowed:false}],&[pending(BTreeSet::from([unsafe_source]))],&state,&mut b).unwrap(),
                1=>{proof.observe(a,&[Shape::Task{borrowed:false}],&[pending(BTreeSet::from([source(base)]))],&state,&mut b).unwrap();proof.observe(a,&[Shape::Task{borrowed:false}],&[pending(BTreeSet::from([unsafe_source]))],&state,&mut b).unwrap();},
                2=>{proof.observe(a,&[Shape::Task{borrowed:false}],&[pending(BTreeSet::from([source(z)]))],&state,&mut b).unwrap();proof.observe(z,&[Shape::Task{borrowed:false}],&[pending(BTreeSet::from([source(a)]))],&state,&mut b).unwrap();},
                _=>{}, // A candidate with no original creation observation has no proof.
            }
            request(&mut proof,a,1,&mut b);
            // A later safe occupant at the same Place cannot overwrite the
            // earlier cleanup's exact source/phase/generation obligation.
            request(&mut proof,base,2,&mut b);
            assert!(proof.finish(&mut b).is_err(),"unsafe creation dependency {hostile}");
        }
    }
    #[test]
    fn creation_dependencies_include_worker_capture_zero_and_reject_missing_task_slots(){
        let mut b=Budget::new(&Limits::default());let state=State::default();let base=birth(1);let target=birth(2);
        let shape=Shape::Product(vec![("$capture:child".into(),Shape::Task{borrowed:false})]);
        for hostile in 0..4{
            let mut proof=SynchronousBirths{known:BTreeSet::from([base]),conditional:BTreeSet::from([target]),..SynchronousBirths::default()};
            let value=match hostile{0=>Value::Product(vec![("$capture:child".into(),pending(BTreeSet::from([source(base)])))]),1=>Value::Product(vec![("$capture:child".into(),pending(unknown_sources()))]),2=>Value::Plain,_=>Value::FallibleTaskOwned(vec![pending(unknown_sources())],PendingTask{states:OPEN,sources:BTreeSet::from([source(base)])})};
            proof.observe(target,std::slice::from_ref(&shape),&[value],&state,&mut b).unwrap();request(&mut proof,target,1,&mut b);
            assert_eq!(proof.finish(&mut b).is_ok(),hostile==0,"worker selector/capture omission {hostile}");
        }
        let mut borrowed_fresh=SynchronousBirths{known:BTreeSet::from([base]),conditional:BTreeSet::from([target]),..SynchronousBirths::default()};
        borrowed_fresh.observe(target,&[Shape::Task{borrowed:true}],&[pending(BTreeSet::from([source(base)]))],&state,&mut b).unwrap();request(&mut borrowed_fresh,target,1,&mut b);assert!(borrowed_fresh.finish(&mut b).is_err(),"fresh owning expression cannot erase required borrowed activation shape");
        let root=root_place(&Binder::Parameter{definition:target.0,ordinal:0});let mut state=State::default();state.owners.insert(root.clone(),Owner{states:CLOSED,generation:5,task:true,owned:true,sources:unknown_sources()});
        for(expected_borrow,borrowed,owned)in [(false,false,true),(true,true,true),(true,false,true),(false,true,true),(false,false,false)]{
            state.owners.get_mut(&root).unwrap().owned=owned;
            let mut proof=SynchronousBirths{conditional:BTreeSet::from([target]),..SynchronousBirths::default()};
            proof.observe(target,&[Shape::Task{borrowed:expected_borrow}],&[Value::Owner{root:root.clone(),borrowed}],&state,&mut b).unwrap();request(&mut proof,target,1,&mut b);
            assert_eq!(proof.finish(&mut b).is_ok(),!expected_borrow&&!borrowed&&owned,"only the exact already Closed owning transfer fixes this generation; borrowed/weak views carry no stable future occupant proof");
        }
    }
    #[test]
    fn synchronous_creator_never_selects_between_distinct_full_parent_domains(){
        let original=CheckedOriginal::default();let mut b=Budget::new(&Limits::default());
        let parent=SemanticDefinitionId{module:zil_base::ModuleId(0),index:7};
        let world=Env{lexical:vec![Frame{definition:parent,actuals:vec![Arg::Type(Ty::Atom(Type::I32)),Arg::Life(Life::Formal(parent,0))],environment:vec![Arg::Type(Ty::Atom(Type::I32))]}],bounds:vec![],invocation:vec![],self_type:None};
        assert!(original.synchronous_parent(parent,&[world.clone(),world.clone()],&mut b).unwrap().is_some());
        for part in 0..3{
            let mut other=world.clone();
            match part{0=>other.lexical[0].actuals[0]=Arg::Type(Ty::Atom(Type::Bool)),1=>other.lexical[0].actuals[1]=Arg::Life(Life::Formal(parent,1)),_=>other.lexical[0].environment[0]=Arg::Type(Ty::Atom(Type::Bool))}
            assert!(original.synchronous_parent(parent,&[world.clone(),other.clone()],&mut b).unwrap().is_none());
            assert!(original.synchronous_parent(parent,&[other,world.clone()],&mut b).unwrap().is_none(),"a first matching parent or sealed image cannot choose an instance");
        }
    }
}

impl CheckedOriginal{
    /// A birth is proved in the original creation site's universal Gamma. A
    /// concrete caller never specializes an incoming Task into a safe birth.
    pub(super) fn synchronous_task_births(&mut self,module:&TypedModule,context:&CheckedContext,index:&index::Index<'_>,active:&mut ActiveQueries,b:&mut Budget)->Check<SynchronousBirths>{
        let previous=self.current;
        let result=(||{
            let mut out=SynchronousBirths::default();
            let sites=self.intrinsic_calls.iter().filter(|(_,use_)|matches!(use_.kind,Intrinsic::TaskCreate|Intrinsic::TaskCreateOwned)).map(|(key,use_)|(*key,*use_)).collect::<Vec<_>>();b.spend(sites.len())?;
            for((owner,site),use_)in sites{
                let Some(worker)=use_.worker else{return Err(bad("original Task birth loses its exact worker source"));};
                let function=index.definitions.get(&owner).ok_or_else(||bad("original Task birth is outside the complete source inventory"))?;
                let Some(index::Node::Expression(node))=function.nodes.get(&site)else{return Err(bad("original Task birth has no exact source Call"));};
                let CoreExprKind::Call{arguments,..}=&node.kind else{return Err(bad("original Task birth selects a non-Call source occurrence"));};
                let prototype=self.prototypes.get(&worker).ok_or_else(||bad("original Task birth has no complete worker prototype"))?;
                if arguments.len()!=prototype.parameters.len().saturating_add(1){return Err(bad("original Task birth changes its complete source inputs"));}
                self.current=Some(owner);
                let caller=self.environment(module,context,owner,active,b)?;
                let actuals=self.calls.get(&(owner,site)).ok_or_else(||bad("original Task birth lacks its complete worker mixed actuals"))?;
                let actuals=self.apply_arguments(actuals,&caller,0,b)?;
                let sources=vec![self.copy_environment(&caller,b)?];
                let Some(world)=self.synchronous_environment(module,context,worker,&actuals,&sources,active,b)?else{continue;};
                let conditional=SynchronousWorld{world:self.copy_environment(&world,b)?,sources:sources.iter().map(|source|self.copy_environment(source,b)).collect::<Check<Vec<_>>>()?};
                if self.closed_source_worker(module,context,index,SynchronousWorld{world,sources},false,active,b)?{out.known.insert((owner,node.id));}
                else if self.closed_source_worker(module,context,index,conditional,true,active,b)?{out.conditional.insert((owner,node.id));}
            }
            Ok(out)
        })();
        self.current=previous;result
    }

    /// Select a declared parent only from actual original call boundaries.
    /// Sealed shapes or a desired result never choose between different full
    /// parent instantiations of the same original declaration.
    fn synchronous_parent(&self,parent:SemanticDefinitionId,sources:&[Env],b:&mut Budget)->Check<Option<Env>>{
        let mut selected:Option<Env>=None;
        for source in sources{
            b.spend(source.lexical.len())?;
            let Some(at)=source.lexical.iter().position(|frame|frame.definition==parent)else{continue;};
            let mut suffix=self.copy_environment(source,b)?;suffix.lexical.drain(..at);
            if let Some(old)=&selected{
                self.copy_environment(old,b)?;
                if old!=&suffix{return Ok(None);}
            }else{selected=Some(suffix);}
        }
        Ok(selected)
    }
    fn synchronous_environment(&mut self,module:&TypedModule,context:&CheckedContext,target:SemanticDefinitionId,actuals:&[Arg],sources:&[Env],active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Env>>{
        let definition=context.definition(module,target,b)?;
        if !definition.environment.is_empty(){return Ok(None);}
        if actuals.len()!=definition.parameters.len(){return Err(bad("synchronous dependency changes its complete mixed argument arity"));}
        for(formal,actual)in definition.parameters.iter().zip(actuals){b.spend(1)?;if !matches!((formal,actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("synchronous dependency changes a full-ordinal actual kind"));}}
        let mut world=self.environment(module,context,target,active,b)?;
        if let Some(parent)=definition.parent{
            let Some(source)=self.synchronous_parent(parent,sources,b)?else{return Ok(None);};
            if world.lexical.is_empty(){return Err(bad("synchronous dependency has no own original frame"));}
            let own=world.lexical.remove(0);
            world.lexical=source.lexical;world.lexical.insert(0,own);
        }
        world.lexical[0].actuals=self.resolve_arguments(actuals,0,b)?;
        if let Some((owner,ty))=&world.self_type{world.self_type=Some((*owner,self.apply_type(ty,&world,b)?));}
        Ok(Some(world))
    }
    fn synchronous_sources(&self,item:&SynchronousWorld,b:&mut Budget)->Check<Vec<Env>>{
        let mut sources=Vec::new();
        for source in item.sources.iter().chain(std::iter::once(&item.world)){
            let source=self.copy_environment(source,b)?;
            b.spend(sources.len().saturating_add(1))?;
            for old in &sources{self.copy_environment(old,b)?;}
            if !sources.contains(&source){sources.push(source);}
        }Ok(sources)
    }

    #[allow(clippy::too_many_arguments)]
    fn closed_source_type(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,item:&SynchronousWorld,pending:&mut Vec<SynchronousWorld>,allow_tasks:bool,active:&mut ActiveQueries,b:&mut Budget,depth:usize)->Check<bool>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original synchronous worker type depth exceeded"));}
        let ty=self.resolved(ty,b)?;
        Ok(match ty{
            Ty::Atom(Type::Unit|Type::Bool|Type::I8|Type::I16|Type::I32|Type::I64|Type::Isize|Type::U8|Type::U16|Type::U32|Type::U64|Type::Usize|Type::F32|Type::F64|Type::ByteBuffer)|Ty::SealedError(_)=>true,
            // A reference to code is not execution. Every actual Call below
            // still requires a complete target, including indirect callables.
            Ty::Function(_)=>true,
            Ty::Tuple(fields)=>{for field in fields{if !self.closed_source_type(module,context,&field,item,pending,allow_tasks,active,b,depth+1)?{return Ok(false);}}true},
            Ty::Address{inner,..}|Ty::Desc{inner,..}|Ty::Managed(inner)=>self.closed_source_type(module,context,&inner,item,pending,allow_tasks,active,b,depth+1)?,
            Ty::Array(inner,_)|Ty::Slice(inner)|Ty::Store(inner)=>self.closed_source_type(module,context,&inner,item,pending,false,active,b,depth+1)?,
            Ty::Carrier{kind:CarrierKind::Task,child,..}if allow_tasks=>self.closed_source_type(module,context,&child,item,pending,false,active,b,depth+1)?,
            Ty::ErrorUnion(error,result)=>{
                let shape=self.task_shape(module,context,&Ty::ErrorUnion(error.clone(),result.clone()),false,active,b,depth+1)?;
                let conditional=allow_tasks&&matches!(shape,Shape::FallibleTask);
                self.closed_source_type(module,context,&error,item,pending,false,active,b,depth+1)?&&self.closed_source_type(module,context,&result,item,pending,conditional,active,b,depth+1)?
            },
            Ty::Code{parameters,result,..}=>{for parameter in parameters{if !self.closed_source_type(module,context,&parameter,item,pending,allow_tasks,active,b,depth+1)?{return Ok(false);}}self.closed_source_type(module,context,&result,item,pending,allow_tasks,active,b,depth+1)?},
            Ty::Nominal(id,args)=>{
                let nominal=context.definition(module,id,b)?;
                let SemanticDefinitionBody::Nominal{body,..}=&nominal.body else{return Err(bad("synchronous source nominal has no original complete declaration"));};
                let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=args;
                match body{
                    SemanticNominalBody::Product{fields}=>for field in fields{let ty=self.ty(module,context,&field.ty,&selected,active,b)?;if !self.closed_source_type(module,context,&ty,item,pending,allow_tasks,active,b,depth+1)?{return Ok(false);}},
                    SemanticNominalBody::Enum{variants}=>for variant in variants{for field in &variant.fields{let ty=self.ty(module,context,field,&selected,active,b)?;if !self.closed_source_type(module,context,&ty,item,pending,false,active,b,depth+1)?{return Ok(false);}}},
                }
                let program=module.semantic.as_ref().ok_or_else(||bad("synchronous source Drop lacks its complete original directory"))?;
                for scope in &program.definitions{b.spend(1)?;let SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{self_type,trait_kind:SemanticBuiltinTrait::Drop})=&scope.body else{continue;};let mut ty=self_type;while let SemanticTypeRecipe::Qualified{storage,..}=ty{b.spend(1)?;ty=storage;}
                    if matches!(ty,SemanticTypeRecipe::Nominal{definition,..}if *definition==id){for method in &program.definitions{b.spend(1)?;if method.parent==Some(scope.id)&&method.kind==SemanticDefinitionKind::Method{
                        let raw=self.environment(module,context,method.id,active,b)?;
                        let mut sources=self.synchronous_sources(item,b)?;for source in &sources{self.copy_environment(source,b)?;}if !sources.contains(&raw){sources.push(self.copy_environment(&raw,b)?);}
                        let actuals=self.resolve_arguments(&raw.lexical[0].actuals,0,b)?;
                        let Some(world)=self.synchronous_environment(module,context,method.id,&actuals,&sources,active,b)?else{return Ok(false);};
                        pending.push(SynchronousWorld{world,sources});
                    }}}
                }true
            },
            Ty::Opaque(owner,args)=>{
                let sources=self.synchronous_sources(item,b)?;
                let Some(world)=self.synchronous_environment(module,context,owner,&args,&sources,active,b)?else{return Ok(false);};
                let hidden=self.opaque_hidden(module,context,owner,&args,active,b)?;
                self.closed_source_type(module,context,&hidden,&SynchronousWorld{world,sources},pending,allow_tasks,active,b,depth+1)?
            },
            Ty::Closure(owner,args,sealed)=>{
                let parent=context.definition(module,owner,b)?.parent.ok_or_else(||bad("synchronous Closure has no exact original creator"))?;
                let sources=self.synchronous_sources(item,b)?;
                let Some(creator)=self.synchronous_parent(parent,&sources,b)?else{return Ok(false);};
                let world=self.closure_creator_environment(module,context,owner,&args,&sealed,&creator,active,b)?;
                let closure=SynchronousWorld{world,sources};
                for(_,capture)in self.closure_capture_fields(module,context,owner,&args,&sealed,b)?{if !self.closed_source_type(module,context,&capture,&closure,pending,allow_tasks,active,b,depth+1)?{return Ok(false);}}
                pending.push(closure);true
            },
            // Input Tasks/registrations, erased targets and abstract variables
            // never acquire a known birth from a concrete calling programme.
            _=>false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn closed_source_callable(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,item:&SynchronousWorld,pending:&mut Vec<SynchronousWorld>,allow_tasks:bool,active:&mut ActiveQueries,b:&mut Budget,depth:usize)->Check<bool>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("synchronous callable source depth exceeded"));}
        match self.resolved(ty,b)?{
            ref ty@Ty::Closure(..)=>self.closed_source_type(module,context,ty,item,pending,allow_tasks,active,b,depth+1),
            Ty::Opaque(owner,args)=>{
                let sources=self.synchronous_sources(item,b)?;
                let Some(world)=self.synchronous_environment(module,context,owner,&args,&sources,active,b)?else{return Ok(false);};
                let hidden=self.opaque_hidden(module,context,owner,&args,active,b)?;
                self.closed_source_callable(module,context,&hidden,&SynchronousWorld{world,sources},pending,allow_tasks,active,b,depth+1)
            },
            Ty::Address{inner,..}=>self.closed_source_callable(module,context,&inner,item,pending,allow_tasks,active,b,depth+1),
            _=>Ok(false),
        }
    }
    fn closed_source_worker(&mut self,module:&TypedModule,context:&CheckedContext,index:&index::Index<'_>,root:SynchronousWorld,allow_tasks:bool,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        fn scalar(ty:&Ty)->bool{matches!(ty,Ty::Atom(Type::Unit|Type::Bool|Type::I8|Type::I16|Type::I32|Type::I64|Type::Isize|Type::U8|Type::U16|Type::U32|Type::U64|Type::Usize|Type::F32|Type::F64))}
        let mut pending=vec![root];let mut seen=Vec::<SynchronousWorld>::new();
        while let Some(item)=pending.pop(){
            self.copy_environment(&item.world,b)?;
            for source in &item.sources{self.copy_environment(source,b)?;}
            b.spend(seen.len().saturating_add(1))?;
            let mut duplicate=false;
            for old in &seen{self.copy_environment(&old.world,b)?;for source in &old.sources{self.copy_environment(source,b)?;}if old==&item{duplicate=true;break;}}
            if duplicate{continue;}seen.push(item.clone());
            let id=item.world.current()?;self.current=Some(id);
            let Some(function)=index.definitions.get(&id)else{return Ok(false);};
            if !self.checked.contains(&id){return Err(bad("synchronous dependency has no complete original body proof"));}
            let prototype=self.prototypes.get(&id).ok_or_else(||bad("synchronous worker dependency lacks its original signature"))?;
            let call_kind=prototype.call_kind;
            let mut types=Vec::new();for ty in prototype.parameters.iter().chain(std::iter::once(&prototype.result)){types.push(self.apply_type(ty,&item.world,b)?);}
            if !matches!(function.body,SemanticOriginalFunctionBody::Defined(_)){
                if !matches!(function.body,SemanticOriginalFunctionBody::External)||function.definition.kind!=SemanticDefinitionKind::Function||function.definition.parent.is_some()||call_kind!=CallKind::Direct||!types.iter().all(scalar){return Ok(false);}
                continue;
            }
            for ty in &types{if !self.closed_source_type(module,context,ty,&item,&mut pending,allow_tasks,active,b,0)?{return Ok(false);}}
            let captures=self.free_values.get(&id).map(|values|values.values().map(|value|self.apply_type(&value.ty,&item.world,b)).collect::<Check<Vec<_>>>()).transpose()?.unwrap_or_default();
            for ty in captures{if !self.closed_source_type(module,context,&ty,&item,&mut pending,allow_tasks,active,b,0)?{return Ok(false);}}
            b.spend(self.bindings.len())?;
            let bindings=self.bindings.iter().filter(|((owner,_),_)|*owner==id).map(|(_,ty)|self.apply_type(ty,&item.world,b)).collect::<Check<Vec<_>>>()?;
            for ty in bindings{if !self.closed_source_type(module,context,&ty,&item,&mut pending,allow_tasks,active,b,0)?{return Ok(false);}}
            for node in function.expressions.values(){
                b.spend(1)?;
                if let Some(ty)=self.expressions.get(&(id,node.id)){let ty=self.apply_type(ty,&item.world,b)?;if !self.closed_source_type(module,context,&ty,&item,&mut pending,allow_tasks,active,b,0)?{return Ok(false);}}
                if !matches!(node.kind,CoreExprKind::Call{..}){continue;}
                let(_,site)=self.original_call_site(id,node.id,b)?;
                let target=if let Some(use_)=self.intrinsic_calls.get(&(id,site)){
                    if matches!(use_.kind,Intrinsic::AsyncStart|Intrinsic::AsyncValid|Intrinsic::AsyncRequest|Intrinsic::AsyncPoll|Intrinsic::AsyncFinish|Intrinsic::AsyncFinishChecked|Intrinsic::ThreadScope|Intrinsic::OwnedDyn|Intrinsic::CCallback){return Ok(false);}
                    use_.worker
                }else{self.call_targets.get(&(id,site)).copied()};
                if let Some(target)=target{
                    let actuals=self.calls.get(&(id,site)).ok_or_else(||bad("synchronous call lacks its complete original mixed actuals"))?;
                    let actuals=self.apply_arguments(actuals,&item.world,0,b)?;
                    let sources=self.synchronous_sources(&item,b)?;
                    let Some(world)=self.synchronous_environment(module,context,target,&actuals,&sources,active,b)?else{return Ok(false);};
                    pending.push(SynchronousWorld{world,sources});
                }else if !self.intrinsic_calls.contains_key(&(id,site)){
                    let Some(source)=self.callable_nodes.get(&(id,node.id))else{return Ok(false);};
                    let Some(ty)=self.expressions.get(&(id,*source))else{return Ok(false);};
                    let ty=self.apply_type(ty,&item.world,b)?;
                    if !self.closed_source_callable(module,context,&ty,&item,&mut pending,allow_tasks,active,b,0)?{return Ok(false);}
                }
            }
        }Ok(true)
    }
}
