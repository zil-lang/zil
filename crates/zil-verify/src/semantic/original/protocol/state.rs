//! Original owner slots and observations are separate namespaces. An
//! observation is useful only for the exact owner generation it observed.
use super::*;

pub(super) const OPEN: u8 = 1;
pub(super) const CLOSING: u8 = 2;
pub(super) const CLOSED: u8 = 4;
pub(super) const DEAD: u8 = 8;
pub(super) const LIVE: u8 = OPEN | CLOSING | CLOSED;

pub(super) fn bit(state: ProtocolState) -> u8 {
    match state {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
pub(super) fn mask(states: &[ProtocolState]) -> u8 {
    states.iter().fold(0, |value, state| value | bit(*state))
}

/// Universal source identities, not concrete caller evidence. A missing or
/// abstract input is never a locally proved synchronous constructor.
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum TaskSource{
    Unknown,
    Input{definition:SemanticDefinitionId,ordinal:u32,path:Vec<String>},
    Birth{definition:SemanticDefinitionId,node:NodeId},
    Capture{definition:SemanticDefinitionId,source:Box<Binder>,path:Vec<String>},
}
pub(super) type Sources=BTreeSet<TaskSource>;
pub(super) type ResultSources=BTreeMap<Vec<String>,Sources>;
pub(super) fn unknown_sources()->Sources{BTreeSet::from([TaskSource::Unknown])}
pub(super) fn source_width(sources:&Sources)->usize{sources.iter().fold(1usize,|n,source|n.saturating_add(match source{TaskSource::Input{path,..}=>path.iter().fold(2usize,|n,s|n.saturating_add(s.len())),TaskSource::Capture{source,path,..}=>path.iter().fold(binder_width(source).saturating_add(2),|n,s|n.saturating_add(s.len())),_=>1}))}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Owner {
    pub(super) states: u8,
    pub(super) generation: u64,
    pub(super) task: bool,
    pub(super) owned: bool,
    pub(super) sources: Sources,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Conditional {
    pub(super) root: Place,
    pub(super) generation: u64,
    pub(super) pending: u8,
    pub(super) ready: u8,
}
/// A pending value carries its phase uncertainty as well as its universal
/// source. Only the real Task constructor supplies OPEN; a helper boundary
/// supplies LIVE until an original protocol edge refines that exact owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingTask {
    pub(super) sources: Sources,
    pub(super) states: u8,
}
impl PendingTask {
    pub(super) fn width(&self)->usize{source_width(&self.sources).saturating_add(1)}
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Value {
    Plain,
    Owner {
        root: Place,
        borrowed: bool,
    },
    Fresh,
    FreshTask(PendingTask),
    FallibleTask(PendingTask),
    FallibleTaskOwned(Vec<Value>,PendingTask),
    Product(Vec<(String, Value)>),
    Poll {
        relations: Vec<Conditional>,
        tag: Option<ProtocolExit>,
    },
}
impl Value {
    // Tags are immutable enum data. Only phase/generation observations are
    // forgotten, including observations nested in a real product value.
    fn forget_observations(&mut self,root:Option<&Place>){
        match self{
            Self::Poll{relations,..}=>relations.retain(|row|root.is_some_and(|root|&row.root!=root)),
            Self::Product(fields)=>for(_,value)in fields{value.forget_observations(root);},
            Self::FallibleTaskOwned(inputs,_)=>for value in inputs{value.forget_observations(root);},
            _=>{},
        }
    }
    fn width(&self) -> usize {
        match self {
            Self::Owner { root, .. } => place_width(root),
            Self::Product(fields) => fields.iter().fold(1usize, |n, (name, value)| {
                n.saturating_add(name.len()).saturating_add(value.width())
            }),
            Self::FallibleTaskOwned(fields,task) => fields
                .iter()
                .fold(task.width(), |n, value| n.saturating_add(value.width())),
            Self::FreshTask(task)|Self::FallibleTask(task)=>task.width(),
            Self::Poll { relations, .. } => relations.iter().fold(1usize, |n, row| {
                n.saturating_add(place_width(&row.root)).saturating_add(3)
            }),
            _ => 1,
        }
    }
}
fn place_width(place: &Place) -> usize {
    binder_width(&place.binder).saturating_add(place.fields.iter().map(String::len).sum::<usize>())
}
pub(super) fn root_place(binder: &Binder) -> Place {
    Place {
        binder: binder.clone(),
        fields: vec![],
    }
}
fn binder_width(binder: &Binder) -> usize {
    match binder {
        Binder::Local { name, .. } => name.len().saturating_add(3),
        Binder::Capture { source, .. } => binder_width(source).saturating_add(1),
        _ => 3,
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub(super) owners: BTreeMap<Place, Owner>,
    pub(super) values: BTreeMap<Binder, Value>,
}
impl State {
    pub(super) fn width(&self) -> usize {
        self.owners
            .iter()
            .fold(1usize, |n, (key,owner)| {
                n.saturating_add(place_width(key)).saturating_add(source_width(&owner.sources)).saturating_add(2)
            })
            .saturating_add(self.values.iter().fold(0usize, |n, (key, value)| {
                n.saturating_add(binder_width(key))
                    .saturating_add(value.width())
            }))
    }
    pub(super) fn invalidate_pending_tasks(&mut self,b:&mut Budget)->Check<()>{
        fn walk(value:&mut Value,b:&mut Budget)->Check<()>{
            b.spend(1)?;
            match value{
                Value::FreshTask(task)|Value::FallibleTask(task)=>{b.spend(task.width())?;task.sources=unknown_sources();task.states=LIVE;},
                Value::FallibleTaskOwned(inputs,task)=>{b.spend(task.width())?;task.sources=unknown_sources();task.states=LIVE;for input in inputs{walk(input,b)?;}},
                Value::Product(fields)=>for(_,value)in fields{walk(value,b)?;},
                _=>{},
            }Ok(())
        }
        b.spend(self.values.len())?;for value in self.values.values_mut(){walk(value,b)?;}Ok(())
    }
    pub(super) fn copy(&self, b: &mut Budget) -> Check<Self> {
        b.spend(self.width())?;
        Ok(self.clone())
    }
    pub(super) fn invalidate(&mut self, root: &Place, b: &mut Budget) -> Check<()> {
        b.spend(
            self.width()
                .saturating_mul(place_width(root).saturating_add(1)),
        )?;
        for value in self.values.values_mut() { value.forget_observations(Some(root)); }
        Ok(())
    }
    pub(super) fn strip_observations(&mut self, b: &mut Budget) -> Check<()> {
        b.spend(self.width())?;
        for value in self.values.values_mut() { value.forget_observations(None); }
        Ok(())
    }
    pub(super) fn owner(&self, root: &Place, b: &mut Budget) -> Check<&Owner> {
        b.spend(place_width(root).saturating_add(1))?;
        let owner = self
            .owners
            .get(root)
            .ok_or_else(|| bad("original protocol operation has no initialized owner identity"))?;
        if owner.states == 0 || owner.states & DEAD != 0 {
            return Err(bad(
                "original protocol owner is consumed or only conditionally live",
            ));
        }
        Ok(owner)
    }
    pub(super) fn refine(
        &mut self,
        value: &Value,
        exit: ProtocolExit,
        b: &mut Budget,
    ) -> Check<bool> {
        let Value::Poll { relations, tag } = value else {
            return Ok(true);
        };
        if tag.is_some_and(|tag| tag != exit) {
            return Ok(false);
        }
        for row in relations {
            b.spend(place_width(&row.root).saturating_add(2))?;
            let owner = self
                .owners
                .get_mut(&row.root)
                .ok_or_else(|| bad("original Poll observation lost its actual owner"))?;
            if owner.generation != row.generation {
                return Err(bad(
                    "original Poll observation belongs to an obsolete owner generation",
                ));
            }
            let selected = match exit {
                ProtocolExit::PollPending => row.pending,
                ProtocolExit::PollReady => row.ready,
                _ => {
                    return Err(bad(
                        "original Poll relation used as another return category",
                    ));
                }
            };
            owner.states &= selected;
            if owner.states == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }
    // Loop heads deliberately discard observations. Their convergence key is
    // the finite state/slot graph, never a recycled poll or a numeric epoch.
    pub(super) fn same_loop(&self, other: &Self, b: &mut Budget) -> Check<bool> {
        b.spend(self.width().saturating_add(other.width()))?;
        Ok(self.values == other.values
            && self.owners.len() == other.owners.len()
            && self.owners.iter().all(|(root, owner)| {
                other
                    .owners
                    .get(root)
                    .is_some_and(|other| owner.states == other.states&&owner.sources==other.sources)
            }))
    }
}

pub(super) fn union_value(left: &Value, right: &Value, b: &mut Budget) -> Check<Value> {
    b.spend(left.width().saturating_add(right.width()))?;
    if left==right{return Ok(left.clone());}
    fn pending(left:&PendingTask,right:&PendingTask,b:&mut Budget)->Check<PendingTask>{
        b.spend(left.width().saturating_add(right.width()))?;
        let mut sources=left.sources.clone();sources.extend(right.sources.iter().cloned());
        Ok(PendingTask{sources,states:left.states|right.states})
    }
    match(left,right){
        (Value::FreshTask(left),Value::FreshTask(right))=>Ok(Value::FreshTask(pending(left,right,b)?)),
        (Value::FallibleTask(left),Value::FallibleTask(right))=>Ok(Value::FallibleTask(pending(left,right,b)?)),
        (Value::FallibleTaskOwned(left,a),Value::FallibleTaskOwned(right,z))if left.len()==right.len()=>{
            let mut fields=Vec::new();for(left,right)in left.iter().zip(right){fields.push(union_value(left,right,b)?);}
            Ok(Value::FallibleTaskOwned(fields,pending(a,z,b)?))
        },
        (Value::Product(left),Value::Product(right))if left.len()==right.len()=>{
            let mut fields=Vec::new();for((a,left),(z,right))in left.iter().zip(right){if a!=z{return Err(bad("original owner join changes its complete field identity"));}fields.push((a.clone(),union_value(left,right,b)?));}Ok(Value::Product(fields))
        },
        _ if tasks::owner_value(left)||tasks::owner_value(right)=>Err(unsupported("original owner join has no complete common conditional value/source",None)),
        _=>Ok(Value::Plain),
    }
}

impl Engine<'_> {
    pub(super) fn generation(&mut self, b: &mut Budget) -> Check<u64> {
        b.spend(1)?;
        let result = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| limit("original protocol generation identity exhausted"))?;
        Ok(result)
    }
    pub(super) fn update(
        &mut self,
        state: &mut State,
        root: &Place,
        states: u8,
        b: &mut Budget,
    ) -> Check<u64> {
        state.owner(root, b)?;
        state.invalidate(root, b)?;
        let generation = self.generation(b)?;
        let owner = state
            .owners
            .get_mut(root)
            .ok_or_else(|| bad("original owner disappeared during phase update"))?;
        owner.states = states;
        owner.generation = generation;
        Ok(generation)
    }
    /// The same tag/state join is used for expression results and stored Poll
    /// locals. Each feasible tag retains only its corresponding owner phases.
    pub(super) fn join_poll_values(&self,left:&State,a:&Value,right:&State,z:&Value,merged:&State,b:&mut Budget)->Check<Value>{
        b.spend(1)?;
        if let(Value::Product(lhs),Value::Product(rhs))=(a,z){
            if lhs.len()!=rhs.len(){return union_value(a,z,b);}
            let mut fields=Vec::new();for((name,a),(other,z))in lhs.iter().zip(rhs){b.spend(name.len().saturating_add(other.len()).saturating_add(1))?;if name!=other{return Err(bad("Poll product join changes a complete field identity"));}fields.push((name.clone(),self.join_poll_values(left,a,right,z,merged,b)?));}return Ok(Value::Product(fields));
        }
        if !matches!((a,z),(Value::Poll{..},Value::Poll{..})){return union_value(a,z,b);}
        let mut pending=Vec::new();let mut ready=Vec::new();
        for(state,value)in [(left,a),(right,z)]{
            for(exit,out)in [(ProtocolExit::PollPending,&mut pending),(ProtocolExit::PollReady,&mut ready)]{
                let mut branch=state.copy(b)?;if branch.refine(value,exit,b)?{out.push(branch);}
            }
        }
        let tag=match(pending.is_empty(),ready.is_empty()){(false,true)=>Some(ProtocolExit::PollPending),(true,false)=>Some(ProtocolExit::PollReady),(false,false)=>None,_=>return Err(bad("Poll join has no feasible original returned tag"))};
        b.spend(merged.width())?;let mut relations=Vec::new();
        for(root,owner)in &merged.owners{
            let mut low=0;let mut high=0;
            for(branches,out)in [(&pending,&mut low),(&ready,&mut high)]{b.spend(branches.len())?;for branch in branches{*out|=branch.owners.get(root).map_or(DEAD,|owner|owner.states);}}
            if low!=owner.states||high!=owner.states{b.spend(place_width(root).saturating_add(3))?;relations.push(Conditional{root:root.clone(),generation:owner.generation,pending:low,ready:high});}
        }
        Ok(Value::Poll{relations,tag})
    }
    pub(super) fn join(&mut self, left: &State, right: &State, b: &mut Budget) -> Check<State> {
        b.spend(left.width().saturating_add(right.width()).saturating_mul(3))?;
        let mut result = left.copy(b)?;
        for (root, other) in &right.owners {
            match result.owners.get_mut(root) {
                Some(owner) => {
                    if owner.task != other.task || owner.owned != other.owned {
                        return Err(bad(
                            "original owner join changes its Task/registration or responsibility role",
                        ));
                    }
                    owner.states |= other.states;
                    owner.sources.extend(other.sources.iter().cloned());
                    if owner.generation != other.generation {
                        owner.generation = self.generation(b)?;
                    }
                }
                None => {
                    result.owners.insert(
                        root.clone(),
                        Owner {
                            states: other.states | DEAD,
                            generation: self.generation(b)?,
                            task: other.task,
                            owned: other.owned,
                            sources:other.sources.clone(),
                        },
                    );
                }
            }
        }
        for (root, owner) in &mut result.owners {
            if !right.owners.contains_key(root) {
                owner.states |= DEAD;
                owner.generation = self.generation(b)?;
            }
        }
        let mut values=BTreeMap::new();
        let count=left.values.len().saturating_add(right.values.len());b.spend(count.saturating_mul(count.checked_ilog2().map_or(1,|n|n as usize+1)))?;
        for(key,value)in &left.values{
            let joined=match right.values.get(key){Some(other)=>self.join_poll_values(left,value,right,other,&result,b)?,None=>union_value(value,&Value::Plain,b)?};
            key.charge_clone(b)?;values.insert(key.clone(),joined);
        }
        for(key,value)in &right.values{
            if !values.contains_key(key){let joined=union_value(&Value::Plain,value,b)?;key.charge_clone(b)?;values.insert(key.clone(),joined);}
        }
        result.values=values;
        // Invalidate only stale phase relations. The immutable actual enum
        // tag is data, never a replacement occupant's Closed witness.
        for value in result.values.values_mut(){if let Value::Poll{relations,..}=value{
            relations.retain(|row|result.owners.get(&row.root).is_some_and(|owner|owner.generation==row.generation));
        }}
        Ok(result)
    }
    pub(super) fn merge(
        &mut self,
        destination: &mut Option<State>,
        source: Option<State>,
        b: &mut Budget,
    ) -> Check<()> {
        if let Some(source) = source {
            *destination = Some(match destination.take() {
                Some(old) => self.join(&old, &source, b)?,
                None => source,
            });
        }
        Ok(())
    }
    pub(super) fn bind(
        &mut self,
        state: &mut State,
        binder: Binder,
        value: Value,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(state.width().saturating_add(binder_width(&binder)))?;
        let value = self.task_bind(state, &binder, value, b)?;
        let value = if value == Value::Fresh {
            if state
                .owners
                .get(&root_place(&binder))
                .is_some_and(|owner| owner.states != DEAD)
            {
                return Err(unsupported(
                    "original replacement reuses an owner birth while an earlier generation is live",
                    None,
                ));
            }
            let generation = self.generation(b)?;
            state.owners.insert(
                root_place(&binder),
                Owner {
                    states: LIVE,
                    generation,
                    task: false,
                    owned: true,
                    sources:unknown_sources(),
                },
            );
            Value::Owner {
                root: root_place(&binder),
                borrowed: false,
            }
        } else {
            value
        };
        if let Some(Value::Owner {
            root,
            borrowed: false,
        }) = state.values.get(&binder)
        {
            if !matches!(&value,Value::Owner{root:other,..}if other==root)
                && state
                    .owners
                    .get(root)
                    .is_some_and(|owner| owner.states != DEAD)
            {
                return Err(unsupported(
                    "original owner replacement requires its old generation to be consumed first",
                    None,
                ));
            }
        }
        state.values.insert(binder, value);
        Ok(())
    }
}

#[cfg(test)]
mod source_tests{
    use super::*;
    #[test]
    fn canonical_pending_and_poll_close_phi_keep_only_feasible_ready_phases(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let index=index::build(&module,&mut b).unwrap();let definition=index.definitions.keys().next().copied().unwrap();let original=CheckedOriginal::default();
        let mut births=tasks::SynchronousBirths::default();let summaries=BTreeMap::new();
        let mut engine=Engine{original:&original,module:&module,index:&index,current:definition,contracts:vec![],inputs:BTreeMap::new(),poll:false,next:5,closed_tasks:BTreeSet::new(),async_tasks:BTreeSet::new(),retiring_captures:BTreeSet::new(),capture_exits:vec![],capture_inputs:BTreeMap::new(),synchronous_births:&mut births,result_sources_by_function:&summaries,returned_sources:None,fallible_task_result:false,error_unwind:vec![],task_loop_keys:vec![],result_shapes:BTreeMap::new(),callable_roles:BTreeMap::new(),callable_targets:BTreeMap::new(),creation_shapes:BTreeMap::new()};
        let root=root_place(&Binder::Parameter{definition,ordinal:0});let local=Binder::Local{definition,node:NodeId(8),name:"observed".into()};
        let pending=Value::Poll{relations:vec![],tag:Some(ProtocolExit::PollPending)};
        let mut wait=State::default();wait.owners.insert(root.clone(),Owner{states:LIVE,generation:1,task:true,owned:true,sources:unknown_sources()});wait.values.insert(local.clone(),pending.clone());
        let mut closing=wait.copy(&mut b).unwrap();let generation=engine.update(&mut closing,&root,CLOSING|CLOSED,&mut b).unwrap();
        let observed=Value::Poll{relations:vec![Conditional{root:root.clone(),generation,pending:CLOSING,ready:CLOSED}],tag:None};closing.values.insert(local.clone(),observed.clone());
        let merged=engine.join(&wait,&closing,&mut b).unwrap();
        for value in [merged.values[&local].clone(),engine.join_poll_values(&wait,&pending,&closing,&observed,&merged,&mut b).unwrap()]{
            let mut ready=merged.copy(&mut b).unwrap();assert!(ready.refine(&value,ProtocolExit::PollReady,&mut b).unwrap());assert_eq!(ready.owner(&root,&mut b).unwrap().states,CLOSED);
            let mut pending=merged.copy(&mut b).unwrap();assert!(pending.refine(&value,ProtocolExit::PollPending,&mut b).unwrap());assert_eq!(pending.owner(&root,&mut b).unwrap().states,LIVE,"waiting forever never claims Closed");
            assert_eq!(ready.owner(&root,&mut b).unwrap().sources,unknown_sources(),"tag refinement does not erase universal source obligations");
            engine.update(&mut ready,&root,OPEN,&mut b).unwrap();assert!(ready.refine(&value,ProtocolExit::PollReady,&mut b).is_err(),"a replacement cannot reuse the old Ready generation");
            let saved=ready.values[&local].clone();assert!(ready.refine(&saved,ProtocolExit::PollReady,&mut b).unwrap());assert_eq!(ready.owner(&root,&mut b).unwrap().states,OPEN,"stored data tag survives invalidation without becoming a Closed witness");
        }
        let ready_tag=Value::Poll{relations:vec![],tag:Some(ProtocolExit::PollReady)};let mut ordinary=wait.copy(&mut b).unwrap();assert!(ordinary.refine(&ready_tag,ProtocolExit::PollReady,&mut b).unwrap());assert_eq!(ordinary.owner(&root,&mut b).unwrap().states,LIVE,"literal Ready is not evidence of task_poll_close");
        assert!(!ordinary.refine(&pending,ProtocolExit::PollReady,&mut b).unwrap());
        let nested=Value::Product(vec![("tag".into(),observed)]);closing.values.insert(local.clone(),nested);closing.invalidate(&root,&mut b).unwrap();
        let Value::Product(fields)=&closing.values[&local]else{unreachable!()};let Value::Poll{relations,..}=&fields[0].1 else{unreachable!()};assert!(relations.is_empty());
    }
    #[test]
    fn poll_tags_require_the_canonical_original_nominal_and_complete_actuals(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let(mut module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let program=module.semantic.as_mut().unwrap();let mut poll=program.definitions[0].clone();let max=program.definitions.iter().map(|row|row.id.index).max().unwrap();poll.id.index=max+1;poll.kind=SemanticDefinitionKind::Nominal;poll.parent=None;poll.parameters=vec![SemanticFormal::Type{requirements:vec![]}];poll.requirements.clear();poll.environment.clear();poll.implicit_lifetimes.clear();
        poll.body=SemanticDefinitionBody::Nominal{declaration:zil_ir::builtin_life::POLL,kind:LifeNominalKind::Enum,body:SemanticNominalBody::Enum{variants:vec![SemanticVariantRecipe{name:"Pending".into(),fields:vec![]},SemanticVariantRecipe{name:"Ready".into(),fields:vec![SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0})]}]}};
        let poll_id=poll.id;program.definitions.push(poll);
        let index=index::build(&module,&mut b).unwrap();let definition=index.definitions.keys().next().copied().unwrap();let original=CheckedOriginal::default();let mut births=tasks::SynchronousBirths::default();let summaries=BTreeMap::new();
        let engine=Engine{original:&original,module:&module,index:&index,current:definition,contracts:vec![],inputs:BTreeMap::new(),poll:false,next:5,closed_tasks:BTreeSet::new(),async_tasks:BTreeSet::new(),retiring_captures:BTreeSet::new(),capture_exits:vec![],capture_inputs:BTreeMap::new(),synchronous_births:&mut births,result_sources_by_function:&summaries,returned_sources:None,fallible_task_result:false,error_unwind:vec![],task_loop_keys:vec![],result_shapes:BTreeMap::new(),callable_roles:BTreeMap::new(),callable_targets:BTreeMap::new(),creation_shapes:BTreeMap::new()};
        let arguments=vec![Arg::Type(Ty::Atom(Type::Unit))];let ty=Ty::Nominal(poll_id,arguments.clone());let mut constructor=VariantConstruction{definition:poll_id,variant:0,actuals:arguments};
        assert_eq!(engine.poll_constructor_tag(&constructor,&ty,&mut b).unwrap(),Some((ProtocolExit::PollPending,0)));constructor.variant=1;assert_eq!(engine.poll_constructor_tag(&constructor,&ty,&mut b).unwrap(),Some((ProtocolExit::PollReady,1)));
        constructor.variant=2;assert!(engine.poll_constructor_tag(&constructor,&ty,&mut b).is_err());constructor.variant=0;constructor.actuals=vec![Arg::Type(Ty::Atom(Type::I32))];assert!(engine.poll_constructor_tag(&constructor,&ty,&mut b).is_err());
        constructor.actuals=vec![Arg::Type(Ty::Atom(Type::Unit))];constructor.definition=definition;assert!(engine.poll_constructor_tag(&constructor,&ty,&mut b).is_err());
        assert_eq!(engine.poll_constructor_tag(&constructor,&Ty::Atom(Type::I32),&mut b).unwrap(),None);
        assert_eq!(engine.poll_constructor_tag(&constructor,&Ty::Nominal(poll_id,vec![Arg::Type(Ty::Atom(Type::I32))]),&mut b).unwrap(),None,"other Poll payloads carry no Task-close protocol relation");
    }
    #[test]
    fn consuming_direct_proof_keeps_every_original_capture_exit_generation(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let index=index::build(&module,&mut b).unwrap();let definition=index.definitions.keys().next().copied().unwrap();
        let mut original=CheckedOriginal::default();
        original.prototypes.insert(definition,Prototype{parameters:vec![],result:Ty::Atom(Type::I32),environment:Env{lexical:vec![],bounds:vec![],invocation:vec![],self_type:None},modes:vec![],result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0,premise_bounds:vec![],callable_kind:Some(CallableKind::FnMut)});
        let mut births=tasks::SynchronousBirths::default();let summaries=BTreeMap::new();
        let source=Binder::Parameter{definition,ordinal:0};let binder=Binder::Capture{definition,source:Box::new(source)};let root=root_place(&binder);
        let mut state=State::default();state.owners.insert(root.clone(),Owner{states:CLOSED,generation:1,task:true,owned:true,sources:unknown_sources()});
        let mut engine=Engine{original:&original,module:&module,index:&index,current:definition,contracts:vec![],inputs:BTreeMap::new(),poll:false,next:1,closed_tasks:BTreeSet::new(),async_tasks:BTreeSet::new(),retiring_captures:BTreeSet::new(),capture_exits:vec![],capture_inputs:BTreeMap::new(),synchronous_births:&mut births,result_sources_by_function:&summaries,returned_sources:None,fallible_task_result:false,error_unwind:vec![],task_loop_keys:vec![],result_shapes:BTreeMap::new(),callable_roles:BTreeMap::new(),callable_targets:BTreeMap::new(),creation_shapes:BTreeMap::new()};
        engine.task_exit(&mut state,ProtocolExit::Return,&mut b).unwrap();
        assert_eq!(state.owner(&root,&mut b).unwrap().states,CLOSED,"checking FnMut consumption never destroys its persistent activation");
        let first=std::mem::take(&mut engine.capture_exits);
        engine.invalidate_task_source(&mut state,&root,&mut b).unwrap();engine.update(&mut state,&root,LIVE,&mut b).unwrap();engine.task_exit(&mut state,ProtocolExit::Return,&mut b).unwrap();
        let second=std::mem::take(&mut engine.capture_exits);drop(engine);
        original.capture_exit_snapshots.insert(definition,first);assert!(original.direct_capture_retirement(definition,&root,&mut b).unwrap());
        original.capture_exit_snapshots.get_mut(&definition).unwrap().extend(second);
        assert!(original.direct_capture_retirement(definition,&root,&mut b).is_err(),"a second live Unknown exit cannot borrow another generation's Closed witness");
    }
    #[test]
    fn original_closure_result_substitutes_only_the_selected_capture_once(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let index=index::build(&module,&mut b).unwrap();let definition=index.definitions.keys().next().copied().unwrap();
        let first=Binder::Parameter{definition,ordinal:0};let second=Binder::Parameter{definition,ordinal:1};
        let mut original=CheckedOriginal::default();original.free_values.insert(definition,BTreeMap::from([("first".into(),Binding{ty:Ty::Atom(Type::I32),mutable:false,identity:first.clone()}),("second".into(),Binding{ty:Ty::Atom(Type::I32),mutable:false,identity:second.clone()})]));
        let mut births=tasks::SynchronousBirths::default();let summaries=BTreeMap::new();
        let mut engine=Engine{original:&original,module:&module,index:&index,current:definition,contracts:vec![],inputs:BTreeMap::new(),poll:false,next:2,closed_tasks:BTreeSet::new(),async_tasks:BTreeSet::new(),retiring_captures:BTreeSet::new(),capture_exits:vec![],capture_inputs:BTreeMap::new(),synchronous_births:&mut births,result_sources_by_function:&summaries,returned_sources:None,fallible_task_result:false,error_unwind:vec![],task_loop_keys:vec![],result_shapes:BTreeMap::new(),callable_roles:BTreeMap::new(),callable_targets:BTreeMap::new(),creation_shapes:BTreeMap::new()};
        let mut state=State::default();let a=root_place(&first);let z=root_place(&second);
        let a_source=BTreeSet::from([TaskSource::Birth{definition,node:NodeId(7)}]);
        let z_source=BTreeSet::from([TaskSource::Input{definition,ordinal:0,path:vec![]}]);
        state.owners.insert(a.clone(),Owner{states:LIVE,generation:1,task:true,owned:true,sources:a_source.clone()});state.owners.insert(z.clone(),Owner{states:LIVE,generation:2,task:true,owned:true,sources:z_source.clone()});
        let receiver=Value::Product(vec![("$capture:first".into(),Value::Owner{root:a,borrowed:false}),("$capture:second".into(),Value::Owner{root:z,borrowed:false})]);
        let before=engine.result_sources(&state,&receiver,&mut b).unwrap();engine.task_transfer_value(&mut state,&receiver,&mut b).unwrap();
        for(source,wanted)in [(second,z_source),(first,a_source)]{
            let summary=BTreeMap::from([(vec![],BTreeSet::from([TaskSource::Capture{definition,source:Box::new(source),path:vec![]}]))]);
            let mut result=Value::FreshTask(PendingTask{states:LIVE,sources:unknown_sources()});
            engine.apply_result_sources(&mut state,&mut result,&summary,&[],definition,Some(&before),&mut b).unwrap();
            let Value::FreshTask(result)=result else{unreachable!()};assert_eq!(result.sources,wanted,"same-type captures keep exact binder identity and call-before source; replacement Input is not substituted a second time");
        }
    }
    #[test]
    fn original_state_join_never_discards_a_conditional_owner_missing_on_either_edge(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let original=CheckedOriginal::default();let index=index::build(&module,&mut b).unwrap();
        let definition=index.definitions.keys().next().copied().unwrap();
        let mut births=tasks::SynchronousBirths::default();let summaries=BTreeMap::new();
        let mut engine=Engine{original:&original,module:&module,index:&index,current:definition,contracts:vec![],inputs:BTreeMap::new(),poll:false,next:0,closed_tasks:BTreeSet::new(),async_tasks:BTreeSet::new(),retiring_captures:BTreeSet::new(),capture_exits:vec![],capture_inputs:BTreeMap::new(),synchronous_births:&mut births,result_sources_by_function:&summaries,returned_sources:None,fallible_task_result:false,error_unwind:vec![],task_loop_keys:vec![],result_shapes:BTreeMap::new(),callable_roles:BTreeMap::new(),callable_targets:BTreeMap::new(),creation_shapes:BTreeMap::new()};
        let binder=Binder::Parameter{definition,ordinal:0};
        let pending=Value::FallibleTask(PendingTask{sources:BTreeSet::from([TaskSource::Birth{definition,node:NodeId(4)}]),states:OPEN});
        let mut present=State::default();present.values.insert(binder.clone(),pending.clone());
        let missing=State::default();
        assert!(engine.join(&present,&missing,&mut b).is_err(),"left-only conditional owner cannot turn Plain");
        assert!(engine.join(&missing,&present,&mut b).is_err(),"right-only conditional owner cannot turn Plain");
        assert_eq!(engine.join(&present,&present,&mut b).unwrap().values[&binder],pending);
        let mut plain=State::default();plain.values.insert(binder.clone(),Value::Plain);
        assert_eq!(engine.join(&plain,&missing,&mut b).unwrap().values[&binder],Value::Plain);
        assert_eq!(engine.join(&missing,&plain,&mut b).unwrap().values[&binder],Value::Plain);
    }
    #[test]
    fn conditional_task_values_keep_phase_uncertainty_and_invalidate_all_nested_sources(){
        let mut b=Budget::new(&Limits::default());
        let definition=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};
        let birth=TaskSource::Birth{definition,node:NodeId(4)};
        let fresh=Value::FallibleTask(PendingTask{sources:BTreeSet::from([birth.clone()]),states:OPEN});
        let helper=Value::FallibleTask(PendingTask{sources:unknown_sources(),states:LIVE});
        let joined=union_value(&fresh,&helper,&mut b).unwrap();
        let Value::FallibleTask(task)=&joined else{panic!("conditional owner information was erased")};
        assert_eq!(task.states,LIVE);assert!(task.sources.contains(&birth));assert!(task.sources.contains(&TaskSource::Unknown));
        let binder=Binder::Parameter{definition,ordinal:0};let mut state=State::default();
        state.values.insert(binder.clone(),Value::Product(vec![("wrapped".into(),fresh.clone())]));
        state.invalidate_pending_tasks(&mut b).unwrap();
        let Value::Product(fields)=&state.values[&binder]else{unreachable!()};
        let Value::FallibleTask(task)=&fields[0].1 else{unreachable!()};
        assert_eq!(task.sources,unknown_sources());assert_eq!(task.states,LIVE);
        assert!(union_value(&fresh,&Value::Plain,&mut b).is_err(),"a missing conditional owner cannot become Plain at a join");
        let owner=Value::Owner{root:root_place(&binder),borrowed:false};
        assert!(union_value(&fresh,&owner,&mut b).is_err(),"unresolved and initialized generations cannot be interchanged");
    }
    #[test]
    fn task_source_loop_identity_keeps_unknown_and_every_birth(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let definition=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};
        let binder=Binder::Parameter{definition,ordinal:0};let root=root_place(&binder);
        let birth=TaskSource::Birth{definition,node:NodeId(4)};
        let mut a=State::default();
        a.owners.insert(root.clone(),Owner{states:LIVE,generation:1,task:true,owned:true,sources:BTreeSet::from([birth.clone()])});
        let mut z=a.copy(&mut b).unwrap();
        assert!(a.same_loop(&z,&mut b).unwrap());
        z.owners.get_mut(&root).unwrap().sources.insert(TaskSource::Unknown);
        assert!(!a.same_loop(&z,&mut b).unwrap());
        assert_eq!(a.owners[&root].sources,BTreeSet::from([birth]));
        // An abstract input is a full original parameter/path coordinate,
        // not a synonym for any concrete caller's worker.
        let input=TaskSource::Input{definition,ordinal:0,path:vec!["$capture:task".into()]};
        assert_ne!(input,TaskSource::Birth{definition,node:NodeId(4)});
    }
    #[test]
    fn captured_poststate_keeps_every_exit_exact_binder_and_poll_generation(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let index=index::build(&module,&mut b).unwrap();let definition=index.definitions.keys().next().copied().unwrap();
        let source=Binder::Parameter{definition,ordinal:0};let other=Binder::Parameter{definition,ordinal:1};
        let capture=root_place(&Binder::Capture{definition,source:Box::new(source.clone())});
        let actual=root_place(&Binder::Local{definition,node:NodeId(9),name:"callback".into()});
        let birth=BTreeSet::from([TaskSource::Birth{definition,node:NodeId(8)}]);
        let receiver=Value::Product(vec![("$capture:task".into(),Value::Owner{root:actual.clone(),borrowed:false})]);
        for scenario in 0..6{
            let mut original=CheckedOriginal::default();
            original.free_values.insert(definition,BTreeMap::from([("task".into(),Binding{ty:Ty::Atom(Type::I32),mutable:true,identity:source.clone()}),("other".into(),Binding{ty:Ty::Atom(Type::I32),mutable:true,identity:other.clone()})]));
            let origin=TaskSource::Capture{definition,source:Box::new(source.clone()),path:vec![]};
            let row=|exit,phase,unchanged|CaptureExit{root:capture.clone(),generation:2,phase,sources:BTreeSet::from([origin.clone()]),exit,unchanged};
            let rows=match scenario{
                0=>vec![row(ProtocolExit::Return,CLOSED,false)],
                1=>vec![row(ProtocolExit::Return,CLOSED,false),row(ProtocolExit::Return,LIVE,false)],
                2=>vec![row(ProtocolExit::PollPending,CLOSING,false),row(ProtocolExit::PollReady,CLOSED,false)],
                3=>vec![row(ProtocolExit::Return,LIVE,true)],
                4=>{let mut value=row(ProtocolExit::Return,CLOSED,false);value.root.binder=Binder::Capture{definition,source:Box::new(other.clone())};vec![value]},
                _=>{let mut value=row(ProtocolExit::Return,LIVE,false);value.sources=unknown_sources();vec![value]},
            };
            original.capture_exit_snapshots.insert(definition,rows);
            let mut births=tasks::SynchronousBirths::default();let summaries=BTreeMap::new();
            let mut engine=Engine{original:&original,module:&module,index:&index,current:definition,contracts:vec![],inputs:BTreeMap::new(),poll:false,next:10,closed_tasks:BTreeSet::new(),async_tasks:BTreeSet::new(),retiring_captures:BTreeSet::new(),capture_exits:vec![],capture_inputs:BTreeMap::new(),synchronous_births:&mut births,result_sources_by_function:&summaries,returned_sources:None,fallible_task_result:false,error_unwind:vec![],task_loop_keys:vec![],result_shapes:BTreeMap::new(),callable_roles:BTreeMap::new(),callable_targets:BTreeMap::new(),creation_shapes:BTreeMap::new()};
            let mut before=State::default();before.owners.insert(actual.clone(),Owner{states:if scenario==3{CLOSED}else{LIVE},generation:1,task:true,owned:true,sources:birth.clone()});
            let source_before=engine.result_sources(&before,&receiver,&mut b).unwrap();
            let mut after=before.copy(&mut b).unwrap();engine.invalidate_task_source(&mut after,&actual,&mut b).unwrap();engine.update(&mut after,&actual,LIVE,&mut b).unwrap();
            let result=engine.apply_capture_poststate(&mut after,&receiver,&before,&source_before,&[],definition,&mut b).unwrap();
            if matches!(scenario,0|3){assert_eq!(after.owners[&actual].states,CLOSED);assert_eq!(after.owners[&actual].sources,birth);}
            if scenario==1{assert_eq!(after.owners[&actual].states,LIVE,"a closed exit cannot erase another live exit");}
            if scenario==2{
                let poll=result.expect("real captured Pending/Ready relation");let mut pending=after.copy(&mut b).unwrap();let mut ready=after.copy(&mut b).unwrap();
                assert!(pending.refine(&poll,ProtocolExit::PollPending,&mut b).unwrap());assert_eq!(pending.owners[&actual].states,CLOSING);
                assert!(ready.refine(&poll,ProtocolExit::PollReady,&mut b).unwrap());assert_eq!(ready.owners[&actual].states,CLOSED);
                engine.update(&mut ready,&actual,LIVE,&mut b).unwrap();assert!(ready.refine(&poll,ProtocolExit::PollReady,&mut b).is_err(),"replacement cannot reuse the old observation");
            }
            if matches!(scenario,4|5){assert_eq!(after.owners[&actual].states,LIVE);assert_eq!(after.owners[&actual].sources,unknown_sources(),"another same-Type capture or unknown after-source grants no proof");}
        }
    }

}
