//! Completed, request-local static theorems. No provider environment, fresh
//! invocation, pending method, source, loan or runtime permission is retained.
use super::*;
use std::{collections::BTreeMap,rc::Rc};

type Key=(usize,usize,u64,Option<ScopeKey>,Option<SitePoint>,LifeDeclarationId,u8,u32);
struct Entry{identity:Rc<()>,limits:Limits,depth:usize,environment:EvalEnv,request:ExternalRequest,value:ExternalValue}
struct Relation{module:usize,identity:Rc<()>,limits:Limits,depth:usize,relation:OriginalMethodRelation}
#[derive(Default)]
pub(super) struct Completed{rows:BTreeMap<Key,Vec<Entry>>,len:usize,relations:Vec<Relation>}
fn lookup(n:usize)->usize{n.checked_ilog2().map_or(1,|n|n as usize+2)}
fn key(module:&TypedModule,context:&semantic::CheckedContext,env:&EvalEnv,request:&ExternalRequest)->Option<Key>{
    let epoch=context.completed_original_epoch()?;
    let (application,kind,member)=match request{
        ExternalRequest::Trait{application,..}=>(application,0,0),
        ExternalRequest::AssociatedType{application,member,..}=>(application,1,*member),
        ExternalRequest::AssociatedConst{application,member,..}=>(application,2,*member),
        _=>return None,
    };
    Some((module as *const TypedModule as usize,context as *const semantic::CheckedContext as usize,epoch,env.lexical.first().map(|f|f.key),env.at,application.declaration,kind,member))
}
fn pure_type(value:&CheckedType,b:&mut Budget)->Check<bool>{
    b.spend(1)?;
    if value.life!=semantic::empty_life()||!value.classes.is_empty()||!value.origins.is_empty()||!value.statics.is_empty(){return Ok(false);}
    let mut pending=vec![&value.ty];
    while let Some(ty)=pending.pop(){b.spend(1)?;match ty{
        Type::Managed(inner)|Type::OwnedStore{element:inner}|Type::Array{element:inner,..}|Type::Mutex{payload:inner}=>{b.spend(1)?;pending.push(inner);},
        Type::Tuple(fields)=>{b.spend(fields.len())?;pending.extend(fields);},
        Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|(_,ty)|ty));},
        Type::Enum{variants,..}=>for(_,fields)in variants{b.spend(fields.len().saturating_add(1))?;pending.extend(fields);},
        Type::ErrorUnion{error,result}=>{b.spend(2)?;pending.push(error);pending.push(result);},
        Type::ByteBuffer|Type::ForeignHandle|Type::CWideString|Type::SyncCell(_)=>{},
        ty if ty.is_b0_scalar()=>{},
        _=>return Ok(false),
    }}Ok(true)
}
pub(super) fn pure_environment(env:&EvalEnv,b:&mut Budget)->Check<bool>{
    b.spend(env.lexical.len().saturating_add(1))?;
    if !env.images.is_empty()||!env.bounds.is_empty()||!env.invocations.is_empty(){return Ok(false);}
    for frame in &env.lexical{
        if !frame.environment.is_empty(){return Ok(false);}
        for actual in &frame.actuals{if !pure_actual(actual,b)?{return Ok(false);}}
        if let Some(value)=&frame.self_type{if !pure_type(value,b)?{return Ok(false);}}
    }Ok(true)
}
fn pure_actual(value:&CheckedActual,b:&mut Budget)->Check<bool>{match value{
    CheckedActual::Life(_)=>Ok(false),
    CheckedActual::Type(value)|CheckedActual::Const{carrier:value,..}=>pure_type(value,b),
}}
pub(super) fn pure_request(request:&ExternalRequest,b:&mut Budget)->Check<bool>{
    let(receiver,application)=match request{
        ExternalRequest::Trait{receiver,application}|ExternalRequest::AssociatedType{receiver,application,..}|ExternalRequest::AssociatedConst{receiver,application,..}=>(receiver,application),
        _=>return Ok(false),
    };
    if !pure_type(receiver,b)?{return Ok(false);}
    for actual in &application.arguments{if !pure_actual(actual,b)?{return Ok(false);}}
    Ok(true)
}
fn pure_value(value:&ExternalValue,b:&mut Budget)->Check<bool>{match value{
    ExternalValue::Proved=>Ok(true),
    ExternalValue::Type(value)|ExternalValue::Const{carrier:value,..}=>pure_type(value,b),
}}
pub(super) fn charge_request(request:&ExternalRequest,b:&mut Budget)->Check<()>{
    let(receiver,application)=match request{
        ExternalRequest::Trait{receiver,application}|ExternalRequest::AssociatedType{receiver,application,..}|ExternalRequest::AssociatedConst{receiver,application,..}=>(receiver,application),
        _=>return Err(bad("completed static theorem has another request kind")),
    };
    charge_type(receiver,b)?;b.spend(application.arguments.len().saturating_add(1))?;
    for value in &application.arguments{charge_actual(value,b)?;}Ok(())
}
fn charge_value(value:&ExternalValue,b:&mut Budget)->Check<()>{match value{
    ExternalValue::Proved=>b.spend(1),
    ExternalValue::Type(value)=>charge_type(value,b),
    ExternalValue::Const{carrier,value}=>{charge_type(carrier,b)?;dynamic_metadata::constant(value,b)},
}}
fn charge_env(env:&EvalEnv,b:&mut Budget)->Check<()>{
    b.spend(env.lexical.len().saturating_add(env.assumptions.len()).saturating_add(env.bounds.len()).saturating_add(1))?;
    for(a,z)in &env.images{charge_key(a,b)?;charge_key(z,b)?;}
    for key in env.invocations.iter().chain(env.bounds.iter().flat_map(|bound|&bound.classes)){charge_key(key,b)?;}
    for frame in &env.lexical{
        b.spend(frame.actuals.len().saturating_add(frame.environment.len()).saturating_add(1))?;
        for value in frame.actuals.iter().chain(&frame.environment){charge_actual(value,b)?;}
        if let Some(value)=&frame.self_type{charge_type(value,b)?;}
    }Ok(())
}
fn env_equal(a:&EvalEnv,z:&EvalEnv,b:&mut Budget)->Check<bool>{
    charge_env(a,b)?;charge_env(z,b)?;
    if a.at!=z.at||a.assumptions!=z.assumptions||a.images!=z.images||a.bounds!=z.bounds||a.invocations!=z.invocations||a.lexical.len()!=z.lexical.len(){return Ok(false);}
    for(a,z)in a.lexical.iter().zip(&z.lexical){
        if a.definition!=z.definition||a.key!=z.key||a.actuals.len()!=z.actuals.len()||a.environment.len()!=z.environment.len(){return Ok(false);}
        for(a,z)in a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)){if !member_actual_equal(a,z,b)?{return Ok(false);}}
        match(&a.self_type,&z.self_type){(Some(a),Some(z))=>if !member_value_equal(a,z,b)?{return Ok(false);},(None,None)=>{},_=>return Ok(false),}
    }Ok(true)
}
fn request_equal(a:&ExternalRequest,z:&ExternalRequest,b:&mut Budget)->Check<bool>{
    let(receiver,application)=match a{ExternalRequest::Trait{receiver,application}|ExternalRequest::AssociatedType{receiver,application,..}|ExternalRequest::AssociatedConst{receiver,application,..}=>(receiver,application),_=>return Ok(false)};
    let(other,other_application)=match z{ExternalRequest::Trait{receiver,application}|ExternalRequest::AssociatedType{receiver,application,..}|ExternalRequest::AssociatedConst{receiver,application,..}=>(receiver,application),_=>return Ok(false)};
    if application.declaration!=other_application.declaration||application.arguments.len()!=other_application.arguments.len(){return Ok(false);}
    for(a,z)in application.arguments.iter().zip(&other_application.arguments){if !member_actual_equal(a,z,b)?{return Ok(false);}}
    member_value_equal(receiver,other,b)
}
impl Completed{
    pub(super) fn relation_known(&self,module:&TypedModule,context:&semantic::CheckedContext,relation:&OriginalMethodRelation,active:&semantic::ActiveQueries,b:&mut Budget)->Check<bool>{
        if context.completed_original_epoch().is_none(){return Ok(false);}
        for row in &self.relations{
            b.spend(20)?;
            if row.module==module as *const TypedModule as usize&&context.same_static_identity(&row.identity)&&row.limits==b.limits&&active.depth()<=row.depth&&row.relation==*relation{return Ok(true);}
        }Ok(false)
    }
    pub(super) fn relation_success(&mut self,module:&TypedModule,context:&semantic::CheckedContext,relation:&OriginalMethodRelation,active:&semantic::ActiveQueries,b:&mut Budget)->Check<()>{
        if context.completed_original_epoch().is_none(){return Ok(());}
        if self.relations.len()>=b.limits.max_nodes{return Err(limit("completed original method directory exceeds selected bound"));}
        b.spend(21)?;self.relations.push(Relation{module:module as *const TypedModule as usize,identity:context.static_identity(),limits:b.limits.clone(),depth:active.depth(),relation:relation.clone()});Ok(())
    }

    pub(super) fn get(&self,module:&TypedModule,context:&semantic::CheckedContext,env:&EvalEnv,request:&ExternalRequest,active:&semantic::ActiveQueries,b:&mut Budget)->Check<Option<ExternalValue>>{
        let Some(key)=key(module,context,env,request)else{return Ok(None);};b.spend(lookup(self.rows.len()))?;
        if active.depth()>=b.limits.max_call_depth.min(128){return Err(limit("semantic/witness dependency depth exceeded"));}
        let Some(rows)=self.rows.get(&key)else{return Ok(None);};
        for row in rows{
            b.spend(14)?;
            if row.limits!=b.limits||active.depth()>row.depth||!context.same_static_identity(&row.identity){continue;}
            if env_equal(&row.environment,env,b)?&&request_equal(&row.request,request,b)?{
                charge_value(&row.value,b)?;return Ok(Some(row.value.clone()));
            }
        }Ok(None)
    }
    pub(super) fn insert(&mut self,module:&TypedModule,context:&semantic::CheckedContext,env:&EvalEnv,request:&ExternalRequest,value:&ExternalValue,active:&semantic::ActiveQueries,b:&mut Budget)->Check<()>{
        let Some(key)=key(module,context,env,request)else{return Ok(());};
        if !pure_request(request,b)?||!pure_value(value,b)?{return Ok(());}
        if self.len>=b.limits.max_nodes{return Err(limit("completed static theorem directory exceeds selected bound"));}
        b.spend(lookup(self.rows.len()).saturating_add(16))?;charge_env(env,b)?;charge_request(request,b)?;charge_value(value,b)?;
        self.rows.entry(key).or_default().push(Entry{identity:context.static_identity(),limits:b.limits.clone(),depth:active.depth(),environment:env.clone(),request:request.clone(),value:value.clone()});self.len+=1;Ok(())
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn value(ty:Type)->CheckedType{CheckedType{type_cell:semantic::CheckedTypeData::new(ty),life:semantic::empty_life(),classes:vec![],origins:vec![],statics:vec![]}}
    fn environment()->EvalEnv{
        let definition=SemanticDefinitionId{module:zil_base::ModuleId(0),index:1};
        let key=ScopeKey::Witness{definition,frame:FrameToken(7)};
        EvalEnv{lexical:vec![LexicalFrame{definition,key,actuals:vec![CheckedActual::Type(value(Type::I32))],environment:vec![],self_type:None}],assumptions:vec![key],images:vec![],bounds:vec![],invocations:vec![],at:None}
    }
    #[test]
    fn static_query_environment_preserves_rigid_scope_and_rejects_runtime_domains(){
        let mut b=Budget::new(&Limits::default());let env=environment();assert!(pure_environment(&env,&mut b).unwrap());
        let mut other=env.clone();other.lexical[0].key=ScopeKey::Witness{definition:env.lexical[0].definition,frame:FrameToken(8)};
        assert!(!env_equal(&env,&other,&mut b).unwrap());
        other=env.clone();other.lexical[0].actuals[0]=CheckedActual::Type(value(Type::I64));assert!(!env_equal(&env,&other,&mut b).unwrap());
        other=env.clone();other.assumptions.clear();assert!(!env_equal(&env,&other,&mut b).unwrap());
        other=env.clone();other.invocations.push(ClassKey::Invocation{frame:FrameToken(9),index:0});assert!(!pure_environment(&other,&mut b).unwrap());
        other=env.clone();other.lexical[0].actuals.push(CheckedActual::Life(ClassKey::Formal{scope:env.lexical[0].key,ordinal:0}));assert!(!pure_environment(&other,&mut b).unwrap());
        for ty in [Type::Address{mutable:false,inner:Box::new(Type::I32)},Type::Desc(Box::new(Type::I32)),Type::Closure(ClosureId(0))]{
            assert!(!pure_type(&value(ty),&mut b).unwrap(),"empty reported κ does not erase a runtime carrier");
        }
        let mut phantom=value(Type::I32);phantom.life.binder_count=1;assert!(!pure_type(&phantom,&mut b).unwrap());
    }
}
