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
#[derive(Clone,Default)]
pub(super) struct Uses {pub(super) moved:BTreeSet<Place>,pub(super) definite:BTreeSet<Place>}
impl Uses {
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
                Ty::Variable(..)|Ty::SelfType(_)=>{
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
        Ty::Nominal(_,arguments)|Ty::Closure(_,arguments,_)=>for argument in arguments{if let Arg::Type(ty)=argument{pending.push(ty);}},
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
