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
}
pub(super) type Sources=BTreeSet<TaskSource>;
pub(super) type ResultSources=BTreeMap<Vec<String>,Sources>;
pub(super) fn unknown_sources()->Sources{BTreeSet::from([TaskSource::Unknown])}
pub(super) fn source_width(sources:&Sources)->usize{sources.iter().fold(1usize,|n,source|n.saturating_add(match source{TaskSource::Input{path,..}=>path.iter().fold(2usize,|n,s|n.saturating_add(s.len())),_=>1}))}
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
        for value in self.values.values_mut() {
            if matches!(value,Value::Poll{relations,..}if relations.iter().any(|row|&row.root==root))
            {
                *value = Value::Plain;
            }
        }
        Ok(())
    }
    pub(super) fn strip_observations(&mut self, b: &mut Budget) -> Check<()> {
        b.spend(self.width())?;
        for value in self.values.values_mut() {
            if matches!(value, Value::Poll { .. }) {
                *value = Value::Plain;
            }
        }
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
        for (key, value) in &mut result.values {
            *value = match right.values.get(key) {
                Some(other) => union_value(value, other, b)?,
                None => union_value(value,&Value::Plain,b)?,
            };
        }
        for(key,value)in &right.values {
            if !result.values.contains_key(key) {
                let joined=union_value(&Value::Plain,value,b)?;
                result.values.insert(key.clone(),joined);
            }
        }
        // Keeping a relation requires the exact generation on every path.
        for value in result.values.values_mut() {
            if matches!(value,Value::Poll{relations,..}if relations.iter().any(|row|result.owners.get(&row.root).is_none_or(|owner|owner.generation!=row.generation)))
            {
                *value = Value::Plain;
            }
        }
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
    fn original_state_join_never_discards_a_conditional_owner_missing_on_either_edge(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let original=CheckedOriginal::default();let index=index::build(&module,&mut b).unwrap();
        let definition=index.definitions.keys().next().copied().unwrap();
        let mut births=tasks::SynchronousBirths::default();let summaries=BTreeMap::new();
        let mut engine=Engine{original:&original,module:&module,index:&index,current:definition,contracts:vec![],inputs:BTreeMap::new(),poll:false,next:0,closed_tasks:BTreeSet::new(),async_tasks:BTreeSet::new(),retiring_captures:BTreeSet::new(),synchronous_births:&mut births,result_sources_by_function:&summaries,returned_sources:None,fallible_task_result:false,error_unwind:vec![],task_loop_keys:vec![],result_shapes:BTreeMap::new(),callable_roles:BTreeMap::new(),creation_shapes:BTreeMap::new()};
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
}
