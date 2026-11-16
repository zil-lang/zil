//! Original-value use state is separate from each materialized resource graph.
//! A concrete Copy specialization cannot justify a second use of an abstract T.
use super::*;
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Binder {
    Parameter{definition:SemanticDefinitionId,ordinal:u32},
    Local{definition:SemanticDefinitionId,node:NodeId,name:String},
    Pattern{definition:SemanticDefinitionId,token:FrameToken},
    Capture{definition:SemanticDefinitionId,source:Box<Binder>},
}
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct Place {pub(super) binder:Binder,pub(super) fields:Vec<String>}
impl Binder{
    pub(super) fn charge_clone(&self,b:&mut Budget)->Check<()>{let mut binder=self;loop{b.spend(1)?;match binder{Self::Capture{source,..}=>binder=source,Self::Local{name,..}=>return b.spend(name.len()),_=>return Ok(())}}}
}
#[derive(Clone,Default)]
pub(super) struct Uses {pub(super) moved:BTreeSet<Place>,pub(super) definite:BTreeSet<Place>}
impl Uses {
    pub(super) fn navigation_conflicts(&self,place:&Place,b:&mut Budget)->Check<bool>{
        b.spend(self.moved.len().saturating_mul(place.fields.len().saturating_add(1)))?;
        Ok(self.moved.iter().any(|old|old.binder==place.binder&&place.fields.starts_with(&old.fields)))
    }
    pub(super) fn conflicts(&self,place:&Place,b:&mut Budget)->Check<bool> {
        b.spend(self.moved.len().saturating_mul(place.fields.len().saturating_add(1)))?;
        Ok(self.moved.iter().any(|old|old.binder==place.binder&&(old.fields.starts_with(&place.fields)||place.fields.starts_with(&old.fields))))
    }
    pub(super) fn initialize(&mut self,place:&Place,b:&mut Budget)->Check<()> {b.spend(self.moved.len().saturating_mul(place.fields.len().saturating_add(1)))?;if self.moved.iter().any(|old|old.binder==place.binder&&old.fields.len()<place.fields.len()&&place.fields.starts_with(&old.fields)){return Err(bad("original partial assignment cannot resurrect a moved ancestor"));}self.moved.retain(|old|old.binder!=place.binder||!old.fields.starts_with(&place.fields));self.definite.retain(|old|old.binder!=place.binder||!old.fields.starts_with(&place.fields));Ok(())}
    pub(super) fn join(&mut self,other:&Self,b:&mut Budget)->Check<()> {
        b.spend(self.definite.len().saturating_mul(other.definite.len().saturating_add(1)).saturating_add(self.moved.len()).saturating_add(other.moved.len()))?;
        let mut definite=BTreeSet::new();
        for place in &self.definite{if other.definite.iter().any(|other|other.binder==place.binder&&place.fields.starts_with(&other.fields)){definite.insert(place.clone());}}
        for place in &other.definite{if self.definite.iter().any(|other|other.binder==place.binder&&place.fields.starts_with(&other.fields)){definite.insert(place.clone());}}
        self.moved.extend(other.moved.iter().cloned());self.definite=definite;Ok(())
    }
}
impl CheckedOriginal {
    /// Selecting a product child observes its parent shell, not all sibling
    /// values. A moved parent/ancestor is still a read conflict; a moved
    /// descendant does not stop a disjoint field use or exact field refill.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn read_original_parent(&mut self,module:&TypedModule,context:&CheckedContext,place:&Place,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let id=env.current()?;b.spend(1)?;self.used.entry(id).or_default().insert(place.binder.clone());
        if self.uses.get(&id).map(|uses|uses.navigation_conflicts(place,b)).transpose()?.unwrap_or(false){
            self.read_original(module,context,place,ty,env,false,active,b)?;
        }
        Ok(())
    }
    pub(super) fn is_copy(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            let ty=self.resolved(ty,b)?;
            Ok(match &ty {
                Ty::SealedError(_)=>true,
                Ty::Atom(ty)=>ty.is_b0_scalar()||matches!(ty,Type::ForeignHandle|Type::CWideString),
                Ty::Address{mutable,..}=>!*mutable,
                Ty::Desc{..}|Ty::Code{..}|Ty::Function(_)=>true,
                Ty::Tuple(fields)=>{for field in fields{if !self.is_copy(module,context,field,env,active,b)?{return Ok(false);}}true},
                Ty::Array(element,_)=>self.is_copy(module,context,element,env,active,b)?,
                Ty::Nominal(id,arguments)=>{
                    let definition=context.definition(module,*id,b)?;
                    let SemanticDefinitionBody::Nominal{kind,body,..}=&definition.body else{return Err(bad("original Copy query has no exact nominal body"));};
                    if !matches!(kind,LifeNominalKind::Record|LifeNominalKind::Enum)||self.custom_drop(module,*id,b)?{return Ok(false);}
                    let mut selected=self.environment(module,context,*id,active,b)?;selected.lexical[0].actuals=arguments.clone();
                    match body{
                        SemanticNominalBody::Product{fields}=>for field in fields{let field=self.ty(module,context,&field.ty,&selected,active,b)?;if !self.is_copy(module,context,&field,env,active,b)?{return Ok(false);}},
                        SemanticNominalBody::Enum{variants}=>for variant in variants{for field in &variant.fields{let field=self.ty(module,context,field,&selected,active,b)?;if !self.is_copy(module,context,&field,env,active,b)?{return Ok(false);}}},
                    }true
                },
                Ty::Variable(..)|Ty::SelfType(_)|Ty::Projection{..}=>{
                    if self.structural_premise(module,context,&ty,DynStructuralBound::Copy,env,active,b)?{return Ok(true);}
                    let equations=self.current.and_then(|id|self.equalities.get(&id)).cloned().unwrap_or_default();
                    for(left,right)in equations{for candidate in [left,right]{if !matches!(candidate,Ty::Variable(..)|Ty::SelfType(_)|Ty::Infer(_))&&self.same(&ty,&candidate,b)?&&self.is_copy(module,context,&candidate,env,active,b)?{return Ok(true);}}}
                    false
                },
                Ty::Infer(_)=>false,
                _=>false,
            })
        })
    }
}
impl CheckedOriginal {
    pub(super) fn read_original(&mut self,module:&TypedModule,context:&CheckedContext,place:&Place,ty:&Ty,env:&Env,consume:bool,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let id=env.current()?;self.used.entry(id).or_default().insert(place.binder.clone());
        if self.is_copy(module,context,ty,env,active,b)?{return Ok(());}
        let conflict=match self.uses.get(&id){Some(uses)=>uses.conflicts(place,b)?,None=>false};
        if conflict {
            if has_inference(ty,&self.substitutions,b)?{b.spend(env.lexical.len().saturating_add(1))?;self.copy_obligations.push((ty.clone(),env.clone()));}
            else{return Err(bad("original abstract value is used after move without an original Copy proof"));}
        }
        if consume {b.spend(place.fields.len().saturating_add(1))?;let uses=self.uses.entry(id).or_default();uses.moved.insert(place.clone());uses.definite.insert(place.clone());}Ok(())
    }
    pub(super) fn use_snapshot(&self,env:&Env,b:&mut Budget)->Check<Uses>{let uses=self.uses.get(&env.current()?).cloned().unwrap_or_default();b.spend(uses.moved.len())?;Ok(uses)}
    pub(super) fn set_uses(&mut self,env:&Env,uses:Uses)->Check<()>{self.uses.insert(env.current()?,uses);Ok(())}
    pub(super) fn join_uses(&mut self,env:&Env,left:&Uses,right:&Uses,b:&mut Budget)->Check<()> {let mut joined=left.clone();b.spend(left.moved.len())?;joined.join(right,b)?;self.set_uses(env,joined)}
}
pub(super) fn has_inference(ty:&Ty,substitutions:&BTreeMap<u64,Ty>,b:&mut Budget)->Check<bool>{
    let mut pending=vec![ty];let mut seen=BTreeSet::new();
    while let Some(ty)=pending.pop(){b.spend(1)?;match ty{
        Ty::Infer(id)=>{if !seen.insert(*id){continue;}match substitutions.get(id){Some(ty)=>pending.push(ty),None=>return Ok(true)}},
        Ty::Tuple(fields)=>pending.extend(fields),
        Ty::Address{inner,..}|Ty::Desc{inner,..}|Ty::Managed(inner)|Ty::Store(inner)|Ty::Slice(inner)|Ty::Array(inner,_)=>pending.push(inner),
        Ty::Nominal(_,arguments)|Ty::Opaque(_,arguments)|Ty::Closure(_,arguments,_)|Ty::Projection{arguments,..}=>{if let Ty::Projection{receiver,..}=ty{pending.push(receiver);}for argument in arguments{match argument{Arg::Type(ty)=>pending.push(ty),Arg::Const(value)=>pending.push(&value.carrier),_=>{}}}},
        Ty::Dyn(value)=>value.types(&mut pending,b)?,
        Ty::Carrier{child,retained,..}=>{pending.push(child);retained.types(&mut pending,b)?;},
        Ty::ErrorUnion(error,result)=>{pending.push(error);pending.push(result);},
        Ty::Code{parameters,result,..}=>{pending.extend(parameters);pending.push(result);},
        _=>{},
    }}Ok(false)
}
impl CheckedOriginal {
    pub(super) fn branch_uses(&mut self,env:&Env,before:&Uses,yes:&Uses,yes_returns:bool,no:&Uses,no_returns:bool,b:&mut Budget)->Check<()> {
        match(yes_returns,no_returns){(true,true)=>self.set_uses(env,before.clone()),(true,false)=>self.set_uses(env,no.clone()),(false,true)=>self.set_uses(env,yes.clone()),(false,false)=>self.join_uses(env,yes,no,b)}
    }
    pub(super) fn loop_uses(&mut self,module:&TypedModule,context:&CheckedContext,env:&Env,before:&Uses,after:&Uses,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        for place in after.moved.difference(&before.moved){b.spend(bindings.len().saturating_add(place.fields.len()))?;if let Some(binding)=bindings.values().find(|binding|binding.identity==place.binder){if !self.is_copy(module,context,&binding.ty,env,active,b)?{if has_inference(&binding.ty,&self.substitutions,b)?{self.copy_obligations.push((binding.ty.clone(),env.clone()));}else{return Err(bad("original loop backedge repeats a move of a non-Copy outer value"));}}}}
        self.set_uses(env,before.clone())
    }
}
impl CheckedOriginal {
    pub(super) fn custom_drop(&self,module:&TypedModule,definition:SemanticDefinitionId,b:&mut Budget)->Check<bool>{
        let program=module.semantic.as_ref().ok_or_else(||bad("original Drop declaration directory absent"))?;b.spend(program.definitions.len())?;
        for row in &program.definitions{if let SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{self_type,trait_kind:SemanticBuiltinTrait::Drop})=&row.body{
            let mut receiver=self_type;while let SemanticTypeRecipe::Qualified{storage,..}=receiver{b.spend(1)?;receiver=storage;}
            match receiver{SemanticTypeRecipe::Nominal{definition:actual,..}if *actual==definition=>return Ok(true),SemanticTypeRecipe::Nominal{..}=>{},_=>return Err(unsupported("original generic Drop receiver requires complete original witness matching",None))}
        }}Ok(false)
    }
    pub(super) fn require_copy(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if self.is_copy(module,context,ty,env,active,b)?{return Ok(());}
        if has_inference(ty,&self.substitutions,b)?{self.copy_obligations.push((ty.clone(),env.clone()));Ok(())}else{Err(bad("original explicit Copy operation lacks an original Copy proof"))}
    }
}

#[cfg(test)]
mod partial_place_tests{
    use super::*;
    #[test]
    fn field_refill_keeps_parent_live_without_reading_moved_siblings(){
        let limits=Limits::default();
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();let mut active=ActiveQueries::new();
        let mut checked=CheckedOriginal::check(&module,&context,&mut active,&mut b).unwrap();
        let id=module.semantic.as_ref().unwrap().definitions.iter().find(|row|row.kind==SemanticDefinitionKind::Function&&matches!(row.parameters.as_slice(),[SemanticFormal::Type{..}])).unwrap().id;
        checked.current=Some(id);let env=checked.prototypes[&id].environment.clone();
        let f=Ty::Variable(id,0);let whole=Ty::Tuple(vec![f.clone(),f.clone(),Ty::Atom(Type::I32)]);
        assert!(!checked.is_copy(&module,&context,&f,&env,&mut active,&mut b).unwrap());
        assert!(!checked.sync_drop(&module,&context,&f,&env,&mut active,&mut b).unwrap());
        let binder=Binder::Local{definition:id,node:NodeId(900),name:"next".into()};
        let root=Place{binder:binder.clone(),fields:vec![]};let a=Place{binder:binder.clone(),fields:vec!["0".into()]};let sibling=Place{binder:binder.clone(),fields:vec!["1".into()]};
        let binding=Binding{ty:whole.clone(),mutable:true,identity:binder};
        let bindings=BTreeMap::from([("next".into(),binding)]);
        checked.set_uses(&env,Uses::default()).unwrap();
        checked.read_original(&module,&context,&a,&f,&env,true,&mut active,&mut b).unwrap();
        checked.read_original(&module,&context,&sibling,&f,&env,false,&mut active,&mut b).unwrap();
        assert!(checked.read_original(&module,&context,&root,&whole,&env,false,&mut active,&mut b).is_err());
        assert!(checked.read_original(&module,&context,&a,&f,&env,false,&mut active,&mut b).is_err());
        let span=zil_base::Span::default();
        let target=SemanticOriginalExpr{id:NodeId(901),kind:CoreExprKind::Member{object:Box::new(SemanticOriginalExpr{id:NodeId(902),kind:CoreExprKind::Name("next".into()),span}),member:"0".into(),owner:None},span};
        let(ty,place)=checked.place(&module,&context,&target,&env,&bindings,&mut active,&mut b).unwrap();
        checked.mutable_place(&module,&context,&target,&env,&bindings,&mut active,&mut b).unwrap();
        checked.overwrite_cleanup(&env,target.id,&ty,place.as_ref(),&mut b).unwrap();
        checked.uses.get_mut(&id).unwrap().initialize(&a,&mut b).unwrap();
        checked.check_cleanups(&module,&context,&mut active,&mut b).unwrap();
        checked.current=Some(id);
        checked.read_original(&module,&context,&root,&whole,&env,false,&mut active,&mut b).unwrap();
        // Copy fields do not revive a moved non-Copy parent. A different
        // target likewise cannot refill the missing field or resurrect it.
        checked.set_uses(&env,Uses{moved:BTreeSet::from([root.clone()]),definite:BTreeSet::from([root])}).unwrap();
        let mut copy_field=target.clone();let CoreExprKind::Member{member,..}=&mut copy_field.kind else{unreachable!()};*member="2".into();
        assert!(checked.place(&module,&context,&copy_field,&env,&bindings,&mut active,&mut b).is_err());
        assert!(checked.uses.get_mut(&id).unwrap().initialize(&a,&mut b).is_err());
        let mut state=Uses{moved:BTreeSet::from([a.clone()]),definite:BTreeSet::from([a.clone()])};
        state.initialize(&sibling,&mut b).unwrap();assert!(state.conflicts(&a,&mut b).unwrap());
        let mut restored=state.clone();restored.initialize(&a,&mut b).unwrap();
        let mut one_branch=restored.clone();one_branch.join(&state,&mut b).unwrap();assert!(one_branch.conflicts(&a,&mut b).unwrap());
        restored.join(&restored.clone(),&mut b).unwrap();assert!(!restored.conflicts(&a,&mut b).unwrap());
    }
}
