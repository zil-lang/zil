//! Original cleanup edges require the original SyncDrop premise. Neither a
//! concrete specialization nor Value/Copy-like spelling invents an effect bound.
use super::*;
#[derive(Clone)]
pub(super) struct Cleanup {definition:SemanticDefinitionId,binding:Binding,path:Vec<String>,capture_exit:bool,uses:Uses,environment:Env}
impl CheckedOriginal {
    pub(super) fn declare(&mut self,env:&Env,binding:&Binding,b:&mut Budget)->Check<()> {b.spend(1)?;self.live.entry(env.current()?).or_default().insert(binding.identity.clone(),binding.clone());Ok(())}
    pub(super) fn live_keys(&self,env:&Env,b:&mut Budget)->Check<BTreeSet<Binder>>{let rows=self.live.get(&env.current()?);b.spend(rows.map_or(0,BTreeMap::len))?;Ok(rows.into_iter().flat_map(BTreeMap::keys).cloned().collect())}
    pub(super) fn queue_cleanup(&mut self,env:&Env,only:Option<&BTreeSet<Binder>>,b:&mut Budget)->Check<()> {
        let id=env.current()?;let uses=self.use_snapshot(env,b)?;
        if let Some(live)=self.live.get(&id){b.spend(live.len().saturating_mul(uses.moved.len().saturating_add(1)))?;for(key,binding)in live{if only.is_none_or(|only|only.contains(key)){self.cleanups.push(Cleanup{definition:id,binding:binding.clone(),path:vec![],capture_exit:true,uses:uses.clone(),environment:env.clone()});}}}Ok(())
    }
    pub(super) fn forget_scope(&mut self,env:&Env,before:&BTreeSet<Binder>,b:&mut Budget)->Check<()> {let id=env.current()?;if let Some(live)=self.live.get_mut(&id){b.spend(live.len())?;live.retain(|key,_|before.contains(key));}Ok(())}
    pub(super) fn exit_scope(&mut self,env:&Env,before:&BTreeSet<Binder>,b:&mut Budget)->Check<()> {
        let after=self.live_keys(env,b)?;let leaving=after.difference(before).cloned().collect::<BTreeSet<_>>();self.queue_cleanup(env,Some(&leaving),b)?;if let Some(live)=self.live.get_mut(&env.current()?){live.retain(|key,_|before.contains(key));}Ok(())
    }
    pub(super) fn check_cleanups(&mut self,module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let cleanups=std::mem::take(&mut self.cleanups);
        for cleanup in cleanups{self.current=Some(cleanup.definition);
            if cleanup.capture_exit&&matches!(cleanup.binding.identity,Binder::Capture{..})&&!self.prototypes.get(&cleanup.definition).is_some_and(|prototype|prototype.callable_kind==Some(CallableKind::FnOnce)){continue;}
            let place=Place{binder:cleanup.binding.identity.clone(),fields:cleanup.path.clone()};
            self.remaining_drop(module,context,&cleanup.binding.ty,&place,&cleanup.uses,&cleanup.environment,active,b)?;
        }self.current=None;Ok(())
    }
    fn remaining_drop(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,place:&Place,uses:&Uses,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(uses.moved.len().saturating_mul(place.fields.len().saturating_add(1)))?;
        if uses.definite.iter().any(|moved|moved.binder==place.binder&&place.fields.starts_with(&moved.fields)){return Ok(());}
        if self.is_copy(module,context,ty,env,active,b)?{return Ok(());}
        let partial=uses.definite.iter().any(|moved|moved.binder==place.binder&&moved.fields.starts_with(&place.fields));
        if partial {
            let ty=self.resolved(ty,b)?;
            let fields=match &ty{
                Ty::Tuple(fields)=>fields.iter().enumerate().map(|(index,field)|(index.to_string(),field.clone())).collect::<Vec<_>>(),
                Ty::Nominal(id,arguments)=>{if self.custom_drop(module,*id,b)?{return Err(bad("original partial cleanup crosses a custom whole Drop"));}let definition=context.definition(module,*id,b)?;let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..}=&definition.body else{return Err(unsupported("original partial cleanup requires exact finite product fields",None));};let mut selected=self.environment(module,context,*id,active,b)?;selected.lexical[0].actuals=arguments.clone();let mut result=Vec::new();for field in fields{result.push((field.name.clone(),self.ty(module,context,&field.ty,&selected,active,b)?));}result},
                _=>return Err(unsupported("original partial cleanup needs its complete value/array debt ledger",None)),
            };
            for(name,field)in fields{let mut child=place.clone();child.fields.push(name);self.remaining_drop(module,context,&field,&child,uses,env,active,b)?;}return Ok(());
        }
        if self.sync_drop(module,context,ty,env,active,b)?{Ok(())}else{Err(bad("original cleanup edge has no original SyncDrop proof"))}
    }
    pub(super) fn sync_drop(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            if self.is_copy(module,context,ty,env,active,b)?||self.has_sync_drop_premise(module,context,ty,env,active,b)?{return Ok(true);}
            let ty=self.resolved(ty,b)?;
            Ok(match ty{
                Ty::Atom(Type::ByteBuffer)=>true,
                Ty::Managed(ref inner)=>self.resource_sync_drop(module,context,inner,env,active,b)?,
                Ty::Carrier{kind:CarrierKind::MutexGuard,child,..}=>self.value_type(module,context,&child,env,active,b)?,
                Ty::Address{..}|Ty::Desc{..}=>true,
                Ty::Tuple(fields)=>{for field in fields{if !self.sync_drop(module,context,&field,env,active,b)?{return Ok(false);}}true},
                Ty::Array(inner,_)=>self.sync_drop(module,context,&inner,env,active,b)?,
                Ty::ErrorUnion(error,result)=>self.sync_drop(module,context,&error,env,active,b)?&&self.sync_drop(module,context,&result,env,active,b)?,
                Ty::Nominal(id,arguments)=>{let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{kind,body,..}=&definition.body else{return Err(bad("original cleanup nominal absent"));};if !matches!(kind,LifeNominalKind::Record|LifeNominalKind::Package|LifeNominalKind::Enum)||self.custom_drop(module,id,b)?{return Ok(false);}let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=arguments;
                    match body{SemanticNominalBody::Product{fields}=>for field in fields{let field=self.ty(module,context,&field.ty,&selected,active,b)?;if !self.sync_drop(module,context,&field,env,active,b)?{return Ok(false);}},SemanticNominalBody::Enum{variants}=>for variant in variants{for field in &variant.fields{let field=self.ty(module,context,field,&selected,active,b)?;if !self.sync_drop(module,context,&field,env,active,b)?{return Ok(false);}}}}true
                },
                Ty::Closure(definition,..)=>{
                    if !self.creator_moves.get(&definition).copied().unwrap_or(false){return Ok(true);}
                    let used=self.used.get(&definition).cloned().unwrap_or_default();let free=self.free_values.get(&definition).cloned().unwrap_or_default();
                    for binding in free.values(){let child=Binder::Capture{definition,source:Box::new(binding.identity.clone())};if used.contains(&child)&&!self.sync_drop(module,context,&binding.ty,env,active,b)?{return Ok(false);}}true
                },
                _=>self.has_sync_drop_premise(module,context,&ty,env,active,b)?,
            })
        })
    }
    fn has_sync_drop_premise(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        self.structural_premise(module,context,ty,DynStructuralBound::SyncDrop,env,active,b)
    }
}

impl CheckedOriginal {
    pub(super) fn overwrite_cleanup(&mut self,env:&Env,node:NodeId,ty:&Ty,place:Option<&Place>,b:&mut Budget)->Check<()> {
        let definition=env.current()?;let uses=self.use_snapshot(env,b)?;
        let (identity,path)=match place{Some(place)=>{b.spend(place.fields.len())?;(place.binder.clone(),place.fields.clone())},None=>(Binder::Local{definition,node,name:"$original_overwrite".into()},vec![])};
        self.cleanups.push(Cleanup{definition,binding:Binding{ty:ty.clone(),mutable:true,identity},path,capture_exit:false,uses,environment:env.clone()});Ok(())
    }
}
