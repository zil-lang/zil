//! Universal original-body checking uses the shared Core operation grammar.
//! No actual instance, Typed SSA Type or target proposal supplies inference.
use super::*;
use zil_base::NodeId;
mod terms;
mod recipes;
mod bodies;
mod index;
mod expressions;
mod calls;
mod patterns;
mod materialize;
use terms::*;
use std::collections::{BTreeMap,BTreeSet};
#[derive(Clone)]
pub(super) struct Prototype {pub(super) parameters:Vec<Ty>,pub(super) result:Ty,pub(super) environment:Env,pub(super) modes:Vec<PassingMode>,pub(super) result_mode:PassingMode,pub(super) call_kind:CallKind,pub(super) bound_lifetimes:u32,pub(super) invocation_lifetimes:u32}
#[derive(Default)]
pub(super) struct CheckedOriginal {
    expressions:BTreeMap<(SemanticDefinitionId,NodeId),Ty>,
    bindings:BTreeMap<(SemanticDefinitionId,NodeId),Ty>,
    parameters:BTreeMap<(SemanticDefinitionId,u32),Ty>,
    results:BTreeMap<SemanticDefinitionId,Ty>,
    calls:BTreeMap<(SemanticDefinitionId,SemanticSiteId),Vec<Arg>>,
    call_targets:BTreeMap<(SemanticDefinitionId,SemanticSiteId),SemanticDefinitionId>,
    callable_nodes:BTreeMap<(SemanticDefinitionId,NodeId),NodeId>,
    prototypes:BTreeMap<SemanticDefinitionId,Prototype>,
    substitutions:BTreeMap<u64,Ty>,
    next_inference:u64,
    environments:BTreeMap<SemanticDefinitionId,Env>,
    active:BTreeSet<SemanticDefinitionId>,
    checked:BTreeSet<SemanticDefinitionId>,
    equalities:BTreeMap<SemanticDefinitionId,Vec<(Ty,Ty)>>,
    current:Option<SemanticDefinitionId>,
    free_values:BTreeMap<SemanticDefinitionId,BTreeMap<String,Binding>>,
}
impl CheckedOriginal {
    pub(super) fn check(module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<Self> {
        let mut result=Self::default();
        let index=index::build(module,b)?;
        for function in index.definitions.values(){b.spend(function.parameters.len().saturating_add(1))?;if function.parameters.len()!=function.signature.parameters.len(){return Err(bad("original body index and signature have different value arity"));}}
        let program=module.semantic.as_ref().ok_or_else(||bad("original body program is absent"))?;
        let mut pending=program.definitions.iter().filter(|definition|matches!(definition.kind,SemanticDefinitionKind::Function|SemanticDefinitionKind::Method|SemanticDefinitionKind::Closure|SemanticDefinitionKind::TaskEntry)).collect::<Vec<_>>();
        b.spend(pending.len())?;
        // Actual source parents must be checked first, so free values close
        // over the original creator/binding point instead of a child LocalId.
        b.spend(pending.len().saturating_mul(pending.len().checked_ilog2().map_or(1,|n|n as usize+1)).saturating_mul(b.limits.max_call_depth.min(128)))?;
        pending.sort_by_key(|definition|{let mut depth=0;let mut parent=definition.parent;while let Some(id)=parent {depth+=1;parent=program.definitions.iter().find(|row|row.id==id).and_then(|row|row.parent);}depth});
        for definition in pending {result.definition(module,context,definition.id,active,b)?;}
        Ok(result)
    }
    fn fresh(&mut self,b:&mut Budget)->Check<Ty>{b.spend(1)?;let id=self.next_inference;self.next_inference=id.checked_add(1).ok_or_else(||limit("original body inference identity exhausted"))?;Ok(Ty::Infer(id))}
    fn resolved(&self,ty:&Ty,b:&mut Budget)->Check<Ty>{self.resolve_at(ty,0,b)}
    fn resolve_at(&self,ty:&Ty,depth:usize,b:&mut Budget)->Check<Ty>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original symbolic type substitution depth exceeded"));}
        Ok(match ty{
            Ty::Infer(id)=>match self.substitutions.get(id){Some(value)=>return self.resolve_at(value,depth+1,b),None=>ty.clone()},
            Ty::Tuple(fields)=>{let mut values=Vec::new();for field in fields{values.push(self.resolve_at(field,depth+1,b)?);}Ty::Tuple(values)},
            Ty::Address{mutable,life,inner}=>Ty::Address{mutable:*mutable,life:life.clone(),inner:Box::new(self.resolve_at(inner,depth+1,b)?)},
            Ty::Desc{life,inner}=>Ty::Desc{life:life.clone(),inner:Box::new(self.resolve_at(inner,depth+1,b)?)},
            Ty::Array(inner,length)=>Ty::Array(Box::new(self.resolve_at(inner,depth+1,b)?),length.clone()),
            Ty::Slice(inner)=>Ty::Slice(Box::new(self.resolve_at(inner,depth+1,b)?)),
            Ty::Managed(inner)=>Ty::Managed(Box::new(self.resolve_at(inner,depth+1,b)?)),
            Ty::Store(inner)=>Ty::Store(Box::new(self.resolve_at(inner,depth+1,b)?)),
            Ty::ErrorUnion(error,result)=>Ty::ErrorUnion(Box::new(self.resolve_at(error,depth+1,b)?),Box::new(self.resolve_at(result,depth+1,b)?)),
            Ty::Code{kind,parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}=>{let mut values=Vec::new();for parameter in parameters{values.push(self.resolve_at(parameter,depth+1,b)?);}Ty::Code{kind:*kind,parameters:values,result:Box::new(self.resolve_at(result,depth+1,b)?),modes:modes.clone(),result_mode:*result_mode,call_kind:*call_kind,bound_lifetimes:*bound_lifetimes,invocation_lifetimes:*invocation_lifetimes}},
            Ty::Nominal(id,arguments)=>Ty::Nominal(*id,self.resolve_arguments(arguments,depth,b)?),
            Ty::Closure(id,arguments,environment)=>Ty::Closure(*id,self.resolve_arguments(arguments,depth,b)?,self.resolve_arguments(environment,depth,b)?),
            _=>ty.clone(),
        })
    }
    fn resolve_arguments(&self,arguments:&[Arg],depth:usize,b:&mut Budget)->Check<Vec<Arg>>{let mut values=Vec::new();for argument in arguments{b.spend(1)?;values.push(match argument{Arg::Type(ty)=>Arg::Type(self.resolve_at(ty,depth+1,b)?),_=>argument.clone()});}Ok(values)}
    fn same(&self,left:&Ty,right:&Ty,b:&mut Budget)->Check<bool>{
        let left=self.resolved(left,b)?;let right=self.resolved(right,b)?;
        let equalities=self.current.and_then(|id|self.equalities.get(&id)).map(Vec::as_slice).unwrap_or(&[]);
        b.spend(equalities.len().saturating_add(1))?;
        if left==right{return Ok(true);}
        let mut queue=vec![&left];let mut seen=Vec::new();
        while let Some(current)=queue.pop(){b.spend(seen.len().saturating_add(equalities.len()).saturating_add(1))?;if seen.contains(&current){continue;}for(a,c)in equalities{if a==current{if c==&right{return Ok(true);}queue.push(c);}if c==current{if a==&right{return Ok(true);}queue.push(a);}}seen.push(current);}
        Ok(false)
    }
    fn unify(&mut self,left:&Ty,right:&Ty,b:&mut Budget)->Check<()> {
        let left=self.resolved(left,b)?;let right=self.resolved(right,b)?;
        if self.same(&left,&right,b)?{return Ok(());}
        match (&left,&right) {
            (Ty::Infer(id),ty)|(ty,Ty::Infer(id))=>{if contains(ty,*id,b)?{return Err(bad("infinite original type inference"));}b.spend(1)?;self.substitutions.insert(*id,ty.clone());Ok(())},
            (Ty::Tuple(a),Ty::Tuple(c)) if a.len()==c.len()=>{for(a,c)in a.iter().zip(c){self.unify(a,c,b)?;}Ok(())},
            (Ty::Address{mutable:a,life:al,inner:ai},Ty::Address{mutable:c,life:cl,inner:ci}) if a==c&&al==cl=>self.unify(ai,ci,b),
            (Ty::Nominal(a,aa),Ty::Nominal(c,ca)) if a==c&&aa.len()==ca.len()=>{for(a,c)in aa.iter().zip(ca){match(a,c){(Arg::Type(a),Arg::Type(c))=>self.unify(a,c,b)?,_ if a==c=>{},_=>return Err(bad("original nominal actuals require a proved equality"))}}Ok(())},
            _=>Err(bad("original body relates independent rigid type/const identities without an original equality")),
        }
    }
    fn environment(&mut self,module:&TypedModule,context:&CheckedContext,id:SemanticDefinitionId,active:&mut ActiveQueries,b:&mut Budget)->Check<Env>{
        if let Some(environment)=self.environments.get(&id){b.spend(environment.lexical.len())?;return Ok(environment.clone());}
        let definition=context.definition(module,id,b)?;
        let mut environment=if let Some(parent)=definition.parent{self.environment(module,context,parent,active,b)?}else{Env{lexical:vec![],bounds:vec![],invocation:vec![]}};
        let mut actuals=Vec::new();
        for(ordinal,formal)in definition.parameters.iter().enumerate(){b.spend(1)?;actuals.push(match formal{
            SemanticFormal::Type{..}=>Arg::Type(Ty::Variable(id,ordinal as u32)),
            SemanticFormal::Lifetime=>Arg::Life(Life::Formal(id,ordinal as u32)),
            SemanticFormal::Const{carrier}=>{let mut scope=environment.clone();scope.lexical.insert(0,Frame{definition:id,actuals:actuals.clone(),environment:vec![]});let carrier=self.ty(module,context,carrier,&scope,active,b)?;Arg::Const(Const{carrier:Box::new(carrier),value:ConstValue::Variable(id,ordinal as u32)})},
        });}
        let mut sealed=Vec::new();for formal in &definition.environment{sealed.push(self.actual(module,context,&formal.source,&environment,active,b)?);}
        environment.lexical.insert(0,Frame{definition:id,actuals,environment:sealed});
        self.environments.insert(id,environment.clone());Ok(environment)
    }
}
fn contains(ty:&Ty,needle:u64,b:&mut Budget)->Check<bool>{let mut queue=vec![ty];while let Some(ty)=queue.pop(){b.spend(1)?;match ty{Ty::Infer(id)if *id==needle=>return Ok(true),Ty::Tuple(children)=>queue.extend(children),Ty::Address{inner,..}|Ty::Desc{inner,..}|Ty::Managed(inner)|Ty::Store(inner)|Ty::Slice(inner)|Ty::Array(inner,_)=>queue.push(inner),Ty::ErrorUnion(a,c)=>{queue.push(a);queue.push(c);},Ty::Code{parameters,result,..}=>{queue.extend(parameters);queue.push(result);},Ty::Nominal(_,arguments)=>for actual in arguments{if let Arg::Type(ty)=actual{queue.push(ty);}},_=>{}}}Ok(false)}
