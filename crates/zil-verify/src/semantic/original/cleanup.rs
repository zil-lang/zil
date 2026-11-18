//! Original cleanup edges require the original SyncDrop premise. Neither a
//! concrete specialization nor Value/Copy-like spelling invents an effect bound.
use super::*;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum CallableRetirement{ActivationCapture,SuspendedOwner}
#[derive(Clone,Copy)]
enum DropPurpose{Ordinary{capture_exit:bool},DirectConsuming}
#[derive(Clone)]
pub(super) struct Cleanup {definition:SemanticDefinitionId,binding:Binding,path:Vec<String>,capture_exit:bool,uses:Uses,environment:Env}
impl CheckedOriginal {
    /// A closure's stored captures are its exact sealed environment values,
    /// not the rigid types recorded while checking its creator generically.
    /// Resolve each used free binder through its original NameAt row; neither
    /// equal physical types nor the order of the free-value map is evidence.
    pub(super) fn closure_capture_types(&self,module:&TypedModule,context:&CheckedContext,definition:SemanticDefinitionId,arguments:&[Arg],sealed:&[Arg],b:&mut Budget)->Check<Vec<Ty>>{
        Ok(self.closure_capture_fields(module,context,definition,arguments,sealed,b)?.into_iter().map(|(_,ty)|ty).collect())
    }
    // Private responsibility paths use the original free name, never a
    // physical capture-layout ordinal or an independently sorted Type list.
    pub(super) fn capture_field(name:&str,b:&mut Budget)->Check<String>{b.spend(name.len().saturating_add(1))?;Ok(format!("$capture:{name}"))}
    pub(super) fn closure_capture_fields(&self,module:&TypedModule,context:&CheckedContext,definition:SemanticDefinitionId,arguments:&[Arg],sealed:&[Arg],b:&mut Budget)->Check<Vec<(String,Ty)>>{
        let original=context.definition(module,definition,b)?;
        if original.kind!=SemanticDefinitionKind::Closure||!self.checked.contains(&definition){return Err(bad("closure cleanup lacks a completely checked original closure"));}
        if original.parameters.len()!=arguments.len()||original.environment.len()!=sealed.len(){return Err(bad("closure cleanup changes its complete mixed argument/environment domains"));}
        for(formal,actual)in original.parameters.iter().zip(arguments).chain(original.environment.iter().map(|row|&row.kind).zip(sealed)){
            b.spend(1)?;
            if !matches!((formal,actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Const{..},Arg::Const(_))|(SemanticFormal::Lifetime,Arg::Life(_))){return Err(bad("closure cleanup changes an original mixed formal kind"));}
        }
        let parent=original.parent.ok_or_else(||bad("closure cleanup has no original creator"))?;
        let index=self.source_index(module,&[parent,definition],b)?;
        let creator=index.definitions.get(&parent).ok_or_else(||bad("closure cleanup creator has no original body"))?;
        b.spend(creator.closures.len())?;
        let mut sites=creator.closures.iter().filter(|(_,child)|**child==definition);
        let(node,_)=sites.next().ok_or_else(||bad("closure cleanup has no exact original creation"))?;
        if sites.next().is_some(){return Err(bad("closure cleanup has ambiguous original creations"));}
        let free=self.free_values.get(&definition).ok_or_else(||bad("closure cleanup has no original free-value inventory"))?;
        let used=self.used.get(&definition);
        let mut captures=Vec::new();
        for(name,binding)in free{
            b.spend(name.len().saturating_add(original.environment.len()))?;
            binding.identity.charge_clone(b)?;
            let child=Binder::Capture{definition,source:Box::new(binding.identity.clone())};
            if !used.is_some_and(|rows|rows.contains(&child)){continue;}
            let mut selected=None;
            for(ordinal,formal)in original.environment.iter().enumerate(){
                let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::NameAt{at:SemanticOriginalNode::Expression(at),name:source}})=&formal.source else{continue;};
                if source!=name{continue;}
                if at!=node{return Err(bad("closure cleanup environment selects another original creation occurrence"));}
                if self.name_binders_at.get(&(parent,false,*at,source.clone()))!=Some(&binding.identity){return Err(bad("closure cleanup environment selects another original free binder"));}
                let Arg::Type(ty)=&sealed[ordinal] else{return Err(bad("closure cleanup value capture has no sealed Type"));};
                if selected.replace(ty).is_some(){return Err(bad("closure cleanup duplicates an original free binder"));}
            }
            // Some concrete captures (for example a local i32) introduce no
            // sealed formal at all. Their independently checked original
            // type remains the subject. Absence grants no substitution or
            // capability to an abstract creator type such as F.
            let ty=selected.unwrap_or(&binding.ty);
            captures.push((Self::capture_field(name,b)?,self.resolved(ty,b)?));
        }
        Ok(captures)
    }
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
            if let Binder::Capture{definition,..}=&cleanup.binding.identity{
                if *definition!=cleanup.definition{return Err(bad("original cleanup Capture belongs to another body"));}
                cleanup.binding.identity.charge_clone(b)?;b.spend(1)?;
                // free_values is the complete creator name lookup universe.
                // Only body-used exact binders actually form this environment.
                // Filter here, after every original branch has been checked;
                // filtering an early Return while discovering uses would erase
                // a real capture that is used on a later branch.
                if !self.used.get(definition).is_some_and(|used|used.contains(&cleanup.binding.identity)) {
                    continue;
                }
                // Only a proved borrowed capture lacks an activation-exit
                // debt. An unknown mode keeps its original cleanup obligation;
                // a projected overwrite must still settle the actual occupant.
                if cleanup.capture_exit
                    && self.creator_moves.get(definition) == Some(&false)
                    && !self.capture_owns(&cleanup.binding.identity, b)?
                {
                    continue;
                }
            }
            if cleanup.capture_exit&&matches!(cleanup.binding.identity,Binder::Capture{..}){
                cleanup.binding.identity.charge_clone(b)?;
                let ty=self.resolved(&cleanup.binding.ty,b)?;
                for place in cleanup.uses.moved.iter().chain(&cleanup.uses.definite){place.binder.charge_clone(b)?;b.spend(place.fields.iter().map(String::len).sum::<usize>().saturating_add(1))?;}
                b.spend(cleanup.path.iter().map(String::len).sum::<usize>().saturating_add(2))?;
                let environment=self.copy_environment(&cleanup.environment,b)?;
                self.direct_cleanup_snapshots.push(Cleanup{definition:cleanup.definition,binding:Binding{ty,mutable:cleanup.binding.mutable,identity:cleanup.binding.identity.clone()},path:cleanup.path.clone(),capture_exit:cleanup.capture_exit,uses:cleanup.uses.clone(),environment});
            }
            if cleanup.capture_exit&&matches!(cleanup.binding.identity,Binder::Capture{..})&&!self.prototypes.get(&cleanup.definition).is_some_and(|prototype|prototype.callable_kind==Some(CallableKind::FnOnce)){continue;}
            let place=Place{binder:cleanup.binding.identity.clone(),fields:cleanup.path.clone()};
            self.remaining_drop(module,context,&cleanup.binding.ty,&place,&cleanup.uses,&cleanup.environment,DropPurpose::Ordinary{capture_exit:cleanup.capture_exit},active,b)?;
        }self.current=None;Ok(())
    }
    fn remaining_drop(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,place:&Place,uses:&Uses,env:&Env,purpose:DropPurpose,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let capture_exit=matches!(purpose,DropPurpose::Ordinary{capture_exit:true});
        let direct=matches!(purpose,DropPurpose::DirectConsuming);
        b.spend(uses.moved.len().saturating_mul(place.fields.len().saturating_add(1)))?;
        if uses.definite.iter().any(|moved|moved.binder==place.binder&&place.fields.starts_with(&moved.fields)){return Ok(());}
        if self.is_copy(module,context,ty,env,active,b)?{return Ok(());}
        if let Ty::Opaque(owner,arguments)=self.resolved(ty,b)?{
            let hidden=self.opaque_hidden(module,context,owner,&arguments,active,b)?;
            return self.remaining_drop(module,context,&hidden,place,uses,env,purpose,active,b);
        }
        let partial=uses.definite.iter().any(|moved|moved.binder==place.binder&&moved.fields.starts_with(&place.fields));
        b.spend(self.task_closed_cleanups.len().saturating_add(self.task_async_cleanups.len()).saturating_add(self.task_synchronous_cleanups.len()).saturating_add(self.callable_retirements.len()))?;
        let nested_task=self.task_closed_cleanups.iter().chain(&self.task_async_cleanups).chain(&self.task_synchronous_cleanups).any(|(definition,child)|Some(*definition)==env.lexical.first().map(|frame|frame.definition)&&child.binder==place.binder&&child.fields.len()>place.fields.len()&&child.fields.starts_with(&place.fields));
        let nested_retirement=capture_exit&&self.callable_retirements.iter().any(|((definition,child),role)|*role==CallableRetirement::ActivationCapture&&Some(*definition)==env.lexical.first().map(|frame|frame.definition)&&child.binder==place.binder&&child.fields.len()>place.fields.len()&&child.fields.starts_with(&place.fields));
        if partial||nested_task||nested_retirement {
            let ty=self.resolved(ty,b)?;
            let fields=match &ty{
                Ty::Tuple(fields)=>fields.iter().enumerate().map(|(index,field)|(index.to_string(),field.clone())).collect::<Vec<_>>(),
                Ty::Nominal(id, arguments) => {
                    if self.custom_drop(module, *id, b)? {
                        if !partial && self.sync_drop(module, context, &ty, env, active, b)? {
                            return Ok(());
                        }
                        return Err(bad("original field cleanup cannot replace a custom whole Drop"));
                    }
                    let definition = context.definition(module, *id, b)?;
                    let mut selected = self.environment(module, context, *id, active, b)?;
                    selected.lexical[0].actuals = arguments.clone();
                    let mut children = Vec::new();
                    match &definition.body {
                        SemanticDefinitionBody::Nominal { body: SemanticNominalBody::Product { fields }, .. } => {
                            for field in fields {
                                children.push((
                                    field.name.clone(),
                                    self.ty(module, context, &field.ty, &selected, active, b)?,
                                ));
                            }
                        }
                        SemanticDefinitionBody::Nominal { body: SemanticNominalBody::Enum { variants }, .. }
                            if nested_task && !partial =>
                        {
                            for (tag, variant) in variants.iter().enumerate() {
                                b.spend(variant.fields.len().saturating_add(1))?;
                                let tag = u32::try_from(tag).map_err(|_| limit("original enum cleanup ordinal overflow"))?;
                                for (index, field) in variant.fields.iter().enumerate() {
                                    b.spend(32)?;
                                    children.push((
                                        super::protocol::owner_variant_field(tag, index),
                                        self.ty(module, context, field, &selected, active, b)?,
                                    ));
                                }
                            }
                        }
                        _ => return Err(unsupported("original partial cleanup requires exact finite owning fields", None)),
                    }
                    children
                },
                Ty::Closure(definition,arguments,sealed)=>{
                    self.capture_creator_moves(*definition)?;
                    let mut owned=Vec::new();
                    for (name,ty) in self.closure_capture_fields(module,context,*definition,arguments,sealed,b)? {
                        if self.capture_field_owns(*definition,&name,b)? { owned.push((name,ty)); }
                    }
                    owned
                },
                _=>return Err(unsupported("original partial cleanup needs its complete value/array debt ledger",None)),
            };
            for(name,field)in fields{let mut child=place.clone();child.fields.push(name);self.remaining_drop(module,context,&field,&child,uses,env,purpose,active,b)?;}return Ok(());
        }
        // This exact-place set is published only after the phase engine has
        // proved every cleanup edge, including all old loop generations.
        if !direct&&self.task_closed_cleanups.contains(&(env.current()?,place.clone()))&&matches!(self.resolved(ty,b)?,Ty::Managed(inner)if matches!(*inner,Ty::Carrier{kind:CarrierKind::Task,..})){return Ok(());}
        if !direct&&self.task_async_cleanups.contains(&(env.current()?,place.clone()))&&self.prototypes.get(&env.current()?).is_some_and(|prototype|prototype.call_kind==CallKind::Suspend)&&matches!(self.resolved(ty,b)?,Ty::Managed(inner)if matches!(*inner,Ty::Carrier{kind:CarrierKind::Task|CarrierKind::AsyncRegistration,..})){return Ok(());}
        // This is a proved source/occupant relation, not Task<T>:SyncDrop.
        // The actual worker/source graph and every real cleanup remain required.
        if !direct&&self.task_synchronous_cleanups.contains(&(env.current()?,place.clone()))&&matches!(self.resolved(ty,b)?,Ty::Managed(inner)if matches!(*inner,Ty::Carrier{kind:CarrierKind::Task,..})){return Ok(());}
        if direct&&self.sync_drop(module,context,ty,env,active,b)?{return Ok(());}
        if direct&&matches!(self.resolved(ty,b)?,Ty::Managed(inner)if matches!(*inner,Ty::Carrier{kind:CarrierKind::Task,..}))&&self.direct_capture_retirement(env.current()?,place,b)?{return Ok(());}
        if capture_exit&&self.callable_retirements.get(&(env.current()?,place.clone()))==Some(&CallableRetirement::ActivationCapture)
            &&matches!(place.binder,Binder::Capture{definition,..}if definition==env.current()?)
            &&self.prototypes.get(&env.current()?).is_some_and(|prototype|prototype.callable_kind==Some(CallableKind::FnOnce))
            &&matches!(self.resolved(ty,b)?,Ty::Managed(inner)if matches!(*inner,Ty::Carrier{kind:CarrierKind::Task,..})){
            // Only the final consuming activation exports this exact remaining
            // capture retirement. It changes neither the user body ABI nor an
            // overwrite/mid-body Drop. Every actual consuming call and owner
            // exit must still settle it through the final resource kernel.
            return Ok(());
        }
        if self.sync_drop(module,context,ty,env,active,b)?{return Ok(());}
        if !direct&&self.prototypes.get(&env.current()?).is_some_and(|prototype|prototype.call_kind==CallKind::Suspend)
            &&self.callable_premise(module,context,ty,env,active,b)?.is_some_and(|prototype|matches!(prototype.callable_kind,Some(CallableKind::Fn|CallableKind::FnMut|CallableKind::FnOnce)))
            &&self.closed_value(module,context,ty,env,active,b)?{
            // This is the declared Suspend upper for retiring this exact owner,
            // not Callable => SyncDrop and not evidence from a concrete caller.
            self.remember_callable_retirement(env.current()?,place,CallableRetirement::SuspendedOwner,b)?;
            return Ok(());
        }
        let diagnostic_ty=self.resolved(ty,b)?;place.binder.charge_clone(b)?;
        b.spend(place.fields.iter().map(String::len).sum::<usize>().saturating_add(4))?;
        Err(bad("original cleanup edge has no original SyncDrop proof").with_note(format!("original cleanup {:?}, place {place:?}, type {diagnostic_ty:?}, capture_exit {capture_exit}, direct_consuming {direct}, original call kind {:?}",env.current()?,self.prototypes.get(&env.current()?).map(|p|p.call_kind))))
    }
    /// Prove concrete providers in their universal declaration, even with no
    /// actual instance. The stored creator/receiver validates source identity;
    /// it never specializes away an original unknown incoming capability.
    pub(in crate::semantic) fn verify_direct_consuming_goals(&mut self,module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let goals=std::mem::take(&mut self.direct_consuming_goals);
        let snapshots=std::mem::take(&mut self.direct_cleanup_snapshots);
        for(definition,receiver,creator)in goals{
            b.spend(snapshots.len())?;
            if !matches!(receiver,Ty::Closure(id,..)if id==definition)||!self.checked.contains(&definition)||creator.lexical.first().is_none_or(|frame|frame.definition!=definition){return Err(bad("original Direct consuming provider loses its exact checked creator/receiver"));}
            self.capture_creator_moves(definition)?;
            for cleanup in snapshots.iter().filter(|cleanup|cleanup.definition==definition){
                self.current=Some(definition);
                let place=Place{binder:cleanup.binding.identity.clone(),fields:cleanup.path.clone()};
                // This is an original caller-closed substitution, never a
                // Typed actual specialization. Declaration goals keep their
                // rigid creator formals and cannot be washed by safe main.
                let ty=self.apply_type(&cleanup.binding.ty,&creator,b)?;
                self.remaining_drop(module,context,&ty,&place,&cleanup.uses,&creator,DropPurpose::DirectConsuming,active,b)?;
            }
        }
        self.current=None;Ok(())
    }
    pub(super) fn remember_callable_retirement(&mut self,definition:SemanticDefinitionId,place:&Place,role:CallableRetirement,b:&mut Budget)->Check<()>{
        place.binder.charge_clone(b)?;b.spend(place.fields.iter().fold(1usize,|n,field|n.saturating_add(field.len())).saturating_add(self.callable_retirements.len().checked_ilog2().map_or(1,|n|n as usize+1)))?;
        if self.callable_retirements.len()>=b.limits.max_nodes{return Err(limit("original callable retirement inventory exceeded"));}
        if let Some(previous)=self.callable_retirements.insert((definition,place.clone()),role){if previous!=role{return Err(bad("original callable retirement changes its exact cleanup role"));}}
        Ok(())
    }
    /// Invoked only at the full module success boundary after the independent
    /// resource/frame/phase kernels. Original permissions above are justified
    /// by the original role/upper; all existing actual worlds remain obligations.
    pub(in crate::semantic) fn finish_callable_retirements(&self,module:&TypedModule,context:&CheckedContext,worlds:&[(SemanticInstanceId,EvalEnv)],b:&mut Budget)->Check<()>{
        let program=module.semantic.as_ref().ok_or_else(||bad("callable retirement has no original programme"))?;
        for((definition,place),role)in &self.callable_retirements{
            place.binder.charge_clone(b)?;b.spend(program.instances.len().saturating_add(worlds.len()).saturating_add(place.fields.len()))?;
            let prototype=self.prototypes.get(definition).ok_or_else(||bad("callable retirement loses its exact original head"))?;
            if !self.checked.contains(definition){return Err(bad("callable retirement lacks its original universal body proof"));}
            match role{
                CallableRetirement::ActivationCapture=>{
                    if context.definition(module,*definition,b)?.kind!=SemanticDefinitionKind::Closure||prototype.callable_kind!=Some(CallableKind::FnOnce)||!matches!(place.binder,Binder::Capture{definition:owner,..}if owner==*definition){return Err(bad("callable retirement is not its exact final consuming capture exit"));}
                    if !self.used.get(definition).is_some_and(|used|used.contains(&place.binder)){return Err(bad("callable retirement selects no original used capture occurrence"));}
                },
                CallableRetirement::SuspendedOwner=>if prototype.call_kind!=CallKind::Suspend{return Err(bad("callable owner retirement lacks its original Suspend upper"));},
            }
            // Zero actual instances supplies no additional original premise:
            // the exact declaration/role/upper was proved above even when unused.
            for instance in program.instances.iter().filter(|instance|instance.definition==*definition&&matches!(instance.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_)))){
                let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))=instance.target else{return Err(bad("callable retirement substitutes a nonexecutable original world"));};
                if !worlds.iter().any(|(checked,_)|*checked==instance.id){return Err(bad("callable retirement has an unfinished exact actual world"));}
                let function=module.functions.get(id.0 as usize).filter(|function|function.id==id&&function.foreign.is_none()).ok_or_else(||bad("callable retirement actual function is absent"))?;
                if function.call_kind!=prototype.call_kind{return Err(bad("callable retirement changes its declared actual ABI upper"));}
                if *role==CallableRetirement::ActivationCapture{
                    let closure=function.environment.ok_or_else(||bad("callable retirement actual capture environment is absent"))?;
                    let actual=crate::closures::definition(module,closure,None)?;
                    if actual.function!=id||actual.kind!=CallableKind::FnOnce||!context.seals.borrow().contains_key(&instance.id){return Err(bad("callable retirement lacks its full independently sealed consuming environment"));}
                }
            }
        }Ok(())
    }
    pub(super) fn sync_drop(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            if self.is_copy(module,context,ty,env,active,b)?||self.has_sync_drop_premise(module,context,ty,env,active,b)?{return Ok(true);}
            let ty=self.resolved(ty,b)?;
            Ok(match ty{
                Ty::Atom(Type::ByteBuffer)=>true,
                // Owned erasure has a sealed synchronous destructor upper.
                // Each original erasure proves its source SyncDrop; the actual
                // table/cleanup graph is a separate mandatory gate.
                Ty::Dyn(value)=>value.owned,
                Ty::Managed(ref inner)=>self.resource_sync_drop(module,context,inner,env,active,b)?,
                Ty::Carrier{kind:CarrierKind::MutexGuard,child,..}=>self.value_type(module,context,&child,env,active,b)?,
                Ty::Address{..}|Ty::Desc{..}=>true,
                Ty::Tuple(fields)=>{for field in fields{if !self.sync_drop(module,context,&field,env,active,b)?{return Ok(false);}}true},
                Ty::Array(inner,_)=>self.sync_drop(module,context,&inner,env,active,b)?,
                Ty::ErrorUnion(error,result)=>self.sync_drop(module,context,&error,env,active,b)?&&self.sync_drop(module,context,&result,env,active,b)?,
                Ty::Nominal(id,arguments)=>{let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{kind,body,..}=&definition.body else{return Err(bad("original cleanup nominal absent"));};if !matches!(kind,LifeNominalKind::Record|LifeNominalKind::Package|LifeNominalKind::Enum)||self.custom_drop(module,id,b)?{return Ok(false);}let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=arguments;
                    match body{SemanticNominalBody::Product{fields}=>for field in fields{let field=self.ty(module,context,&field.ty,&selected,active,b)?;if !self.sync_drop(module,context,&field,env,active,b)?{return Ok(false);}},SemanticNominalBody::Enum{variants}=>for variant in variants{for field in &variant.fields{let field=self.ty(module,context,field,&selected,active,b)?;if !self.sync_drop(module,context,&field,env,active,b)?{return Ok(false);}}}}true
                },
                Ty::Opaque(owner,arguments)=>{let hidden=self.opaque_hidden(module,context,owner,&arguments,active,b)?;self.sync_drop(module,context,&hidden,env,active,b)?},
                Ty::Closure(definition,arguments,sealed)=>{
                    let captures=self.closure_capture_fields(module,context,definition,&arguments,&sealed,b)?;
                    self.capture_creator_moves(definition)?;
                    for (name,capture) in captures {
                        if self.capture_field_owns(definition,&name,b)? && !self.sync_drop(module,context,&capture,env,active,b)? { return Ok(false); }
                    }
                    true
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

#[cfg(test)]
mod capture_subject_tests{
    use super::*;
    #[test]
    fn cleanup_subject_uses_exact_sealed_capture_and_preserves_borrow_identity(){
        let limits=Limits::default();
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/phantom-closure-complete-creator.wire"),&limits).unwrap();
        let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();let mut active=ActiveQueries::new();
        let mut checked=CheckedOriginal::check(&module,&context,&mut active,&mut b).unwrap();
        let original=module.semantic.as_ref().unwrap().definitions.iter().find(|row|row.kind==SemanticDefinitionKind::Closure&&!row.environment.is_empty()).unwrap();
        let id=original.id;let env=checked.environment(&module,&context,id,&mut active,&mut b).unwrap();
        let frame=&env.lexical[0];let arguments=frame.actuals.clone();let mut sealed=frame.environment.clone();
        let(ordinal,at,name)=original.environment.iter().enumerate().find_map(|(ordinal,row)|match &row.source{
            SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::NameAt{at:SemanticOriginalNode::Expression(at),name}})=>Some((ordinal,*at,name.clone())),_=>None,
        }).unwrap();
        checked.current=Some(original.parent.unwrap());let caller=checked.prototypes[&original.parent.unwrap()].environment.clone();
        let head=Life::Local(original.parent.unwrap(),NodeId(8123));
        let capture=Ty::Address{mutable:false,life:head.clone(),inner:Box::new(Ty::Atom(Type::I32))};
        sealed[ordinal]=Arg::Type(capture.clone());
        let subjects=checked.closure_capture_types(&module,&context,id,&arguments,&sealed,&mut b).unwrap();
        assert!(subjects.contains(&capture));
        assert!(checked.sync_drop(&module,&context,&capture,&caller,&mut active,&mut b).unwrap());
        assert!(!checked.closed_value(&module,&context,&capture,&caller,&mut active,&mut b).unwrap());
        assert_eq!(checked.resolved_life(&head,&mut b).unwrap(),head);
        let mut wrong=sealed.clone();wrong[ordinal]=Arg::Life(Life::Static);
        assert!(checked.closure_capture_types(&module,&context,id,&arguments,&wrong,&mut b).is_err());
        assert!(checked.closure_capture_types(&module,&context,id,&arguments,&sealed[..sealed.len()-1],&mut b).is_err());
        // Equal types at another binder cannot stand in for the source of
        // this original environment entry.
        checked.name_binders_at.insert((original.parent.unwrap(),false,at,name),Binder::Local{definition:original.parent.unwrap(),node:NodeId(8124),name:"other".into()});
        assert!(checked.closure_capture_types(&module,&context,id,&arguments,&sealed,&mut b).is_err());
    }
}

#[cfg(test)]
mod materialized_capture_cleanup_tests{
    use super::*;
    #[test]
    fn unused_creator_bindings_form_no_capture_debt_but_real_unknown_capture_stays_strict(){
        let limits=Limits::default();let mut b=Budget::new(&limits);let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let context=CheckedContext::catalogue(&module,&mut b).unwrap();let definition=module.semantic.as_ref().unwrap().definitions.iter().find(|row|matches!(row.kind,SemanticDefinitionKind::Function)).unwrap().id;
        let source=Binder::Parameter{definition,ordinal:0};let capture=Binder::Capture{definition,source:Box::new(source)};
        let mut active = ActiveQueries::new();
        let env = CheckedOriginal::default().environment(&module, &context, definition, &mut active, &mut b).unwrap();
        let ty=Ty::Managed(Box::new(Ty::Carrier{kind:CarrierKind::Task,child:Box::new(Ty::Atom(Type::I32)),retained:Box::default()}));
        let sibling=Binder::Capture{definition,source:Box::new(Binder::Parameter{definition,ordinal:1})};
        for (used, creator_moves, inferred, capture_exit, owes_cleanup) in [
            (false, None, None, true, false),
            (true, None, None, true, true),
            (true, Some(true), None, true, true),
            (true, Some(false), None, true, false),
            (true, Some(false), Some(&capture), true, true),
            (true, Some(false), Some(&sibling), true, false),
            (true, Some(false), None, false, true),
        ] {
            let mut checked=CheckedOriginal::default();
            if used { checked.used.insert(definition,BTreeSet::from([capture.clone()])); }
            if let Some(moves) = creator_moves { checked.creator_moves.insert(definition, moves); }
            if let Some(binder) = inferred {
                checked.inferred_owner_captures.insert(definition, BTreeSet::from([binder.clone()]));
            }
            checked.prototypes.insert(definition,Prototype{parameters:vec![],result:Ty::Atom(Type::Unit),environment:env.clone(),modes:vec![],result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0,premise_bounds:vec![],callable_kind:Some(CallableKind::FnOnce)});
            checked.cleanups.push(Cleanup{definition,binding:Binding{ty:ty.clone(),mutable:true,identity:capture.clone()},path:vec![],capture_exit,uses:Uses::default(),environment:env.clone()});
            let mut active=ActiveQueries::new();let result=checked.check_cleanups(&module,&context,&mut active,&mut b);
            if owes_cleanup {
                let error = result.expect_err("a genuinely captured unknown Task still needs an original retirement proof");
                assert_eq!(error.message, "original cleanup edge has no original SyncDrop proof");
            } else {
                result.unwrap();
                assert!(checked.callable_retirements.is_empty());
            }
            assert_eq!(checked.direct_cleanup_snapshots.len(), usize::from(owes_cleanup && capture_exit));
        }
    }
    #[test]
    fn write_only_capture_keeps_overwrite_and_earlier_exit_debt_but_shadowing_does_not_capture(){
        let limits=Limits::default();let mut b=Budget::new(&limits);let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let context=CheckedContext::catalogue(&module,&mut b).unwrap();let definition=module.semantic.as_ref().unwrap().definitions.iter().find(|row|matches!(row.kind,SemanticDefinitionKind::Function)).unwrap().id;
        let capture=Binder::Capture{definition,source:Box::new(Binder::Parameter{definition,ordinal:0})};let local=Binder::Local{definition,node:NodeId(20),name:"task".into()};
        let env=Env{lexical:vec![Frame{definition,actuals:vec![],environment:vec![]}],bounds:vec![],invocation:vec![],self_type:None};
        let ty=Ty::Managed(Box::new(Ty::Carrier{kind:CarrierKind::Task,child:Box::new(Ty::Atom(Type::I32)),retained:Box::default()}));
        for scenario in 0..3{
            let mut checked=CheckedOriginal::default();checked.current=Some(definition);
            checked.prototypes.insert(definition,Prototype{parameters:vec![],result:Ty::Atom(Type::Unit),environment:env.clone(),modes:vec![],result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0,premise_bounds:vec![],callable_kind:Some(CallableKind::FnOnce)});
            // The first snapshot can be an early Return checked before another
            // original branch reveals this same real captured mutable binder.
            checked.cleanups.push(Cleanup{definition,binding:Binding{ty:ty.clone(),mutable:true,identity:capture.clone()},path:vec![],capture_exit:scenario!=0,uses:Uses::default(),environment:env.clone()});
            let selected=if scenario==1{local.clone()}else{capture.clone()};let bindings=BTreeMap::from([("task".into(),Binding{ty:ty.clone(),mutable:true,identity:selected.clone()})]);
            let target=SemanticOriginalExpr{id:NodeId(21),span:Default::default(),kind:CoreExprKind::Name("task".into())};let mut active=ActiveQueries::new();
            checked.mutable_place(&module,&context,&target,&env,&bindings,&mut active,&mut b).unwrap();assert!(checked.used[&definition].contains(&selected));
            let result=checked.check_cleanups(&module,&context,&mut active,&mut b);
            if scenario==1{assert!(!checked.used[&definition].contains(&capture));result.unwrap();assert!(checked.direct_cleanup_snapshots.is_empty(),"same spelling of a local is not the original Capture");}
            else{assert!(result.is_err(),"write-only captured old Unknown Task keeps its overwrite/early-exit responsibility");}
        }
    }

}
