//! Remote execution is a complete original dependency-graph obligation.
//! A mutex description is a scoped manager role, not a general Transfer fact.
use super::*;
impl CheckedOriginal {
    pub(super) fn scoped_thread(&mut self,module:&TypedModule,context:&CheckedContext,site:SemanticSiteId,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        if arguments.len()!=3{return Err(bad("thread_scope requires its worker, input and parent body"));}
        let worker=self.expression(module,context,&arguments[0],env,bindings,None,active,b)?;
        let Ty::Function(worker)=self.resolved(&worker,b)?else{return Err(bad("thread worker is not its original static function"));};
        let definition=context.definition(module,worker,b)?;
        if definition.kind!=SemanticDefinitionKind::Function||definition.parent.is_some()||!definition.parameters.is_empty()||!definition.environment.is_empty(){return Err(unsupported("thread worker requires its complete root static monomorphic original scheme",None));}
        let SemanticDefinitionBody::FunctionWithBody{signature,body:SemanticOriginalFunctionBody::Defined(_),foreign:None,..}=&definition.body else{return Err(bad("thread worker has no complete original Defined body"));};
        if !self.checked.contains(&worker)&&!self.active.contains(&worker){self.definition(module,context,worker,active,b)?;}
        let target=self.environment(module,context,worker,active,b)?;
        let prototype=self.call_signature(module,context,worker,signature,&target,active,b)?;
        if prototype.parameters.len()!=1||prototype.result!=Ty::Atom(Type::I32)||prototype.call_kind!=CallKind::Direct||prototype.bound_lifetimes!=0{return Err(bad("thread worker has another full original scalar result/parameter/effect signature"));}
        let input=self.expression(module,context,&arguments[1],env,bindings,None,active,b)?;
        let input=self.resolved(&input,b)?;
        if matches!(&input,Ty::Managed(inner)if matches!(inner.as_ref(),Ty::Carrier{kind:CarrierKind::Task,..})){return Err(unsupported("scoped Task transfer requires its exact original owner/phase and result relation",None));}
        let supported=match &input{
            Ty::Atom(Type::I32)=>true,
            Ty::Address{mutable:false,inner,..}=>matches!(inner.as_ref(),Ty::Atom(Type::I32|Type::SyncCell(_))),
            Ty::Desc{inner,..}=>matches!(inner.as_ref(),Ty::Carrier{kind:CarrierKind::Mutex,retained,..}if retained.as_ref()==&Retained::default()),
            _=>false,
        };
        if !supported{return Err(bad("thread input lacks its original scalar/shared-cell/mutex manager role"));}
        self.unify(&input,&prototype.parameters[0],b)?;
        for requirement in &definition.requirements{self.obligation(module,context,requirement,&target,env,active,b)?;}
        let body=self.expression(module,context,&arguments[2],env,bindings,None,active,b)?;
        let Ty::Closure(body_definition,_,_)=self.resolved(&body,b)?else{return Err(bad("thread parent body is not an original concrete closure"));};
        let body_signature=self.prototypes.get(&body_definition).cloned().ok_or_else(||bad("thread parent body has no universally checked complete prototype"))?;
        if !body_signature.parameters.is_empty()||body_signature.result!=Ty::Atom(Type::I32)||body_signature.result_mode!=PassingMode::Infer||body_signature.call_kind!=CallKind::Direct||!matches!(body_signature.callable_kind,Some(CallableKind::Fn|CallableKind::FnMut|CallableKind::FnOnce)){return Err(bad("thread parent body does not admit the complete consuming zero-input i32 Direct role"));}
        // The parent closure is also cleaned on native thread-creation failure.
        self.structural_obligation(module,context,&body,DynStructuralBound::SyncDrop,env,active,b)?;
        b.spend(2)?;
        self.scoped_threads.insert((env.current()?,site),(worker,arguments[1].id,arguments[2].id));
        self.scoped_thread_workers.push((worker,input,env.clone()));
        Ok(Ty::Tuple(vec![Ty::Atom(Type::Bool),Ty::Atom(Type::I32),Ty::Atom(Type::I32)]))
    }
    pub(in crate::semantic) fn scoped_thread_sources(&self,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<(SemanticDefinitionId,SemanticTypeRecipe,SemanticTypeRecipe)>{
        b.spend(3)?;let(worker,input,body)=*self.scoped_threads.get(&(definition,site)).ok_or_else(||bad("thread operation has no complete original worker/input/body occurrence"))?;
        let recipe=|node|SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(node)};
        Ok((worker,recipe(input),recipe(body)))
    }
    pub(super) fn check_scoped_thread_workers(&mut self,module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let pending=std::mem::take(&mut self.scoped_thread_workers);let previous=self.current;
        let outcome=(||{for(root,input,caller)in pending{
            let payload=match &input{Ty::Desc{inner,..}=>match inner.as_ref(){Ty::Carrier{kind:CarrierKind::Mutex,child,..}=>Some(child.as_ref()),_=>None},_=>None};
            let mut queue=vec![(root,self.environment(module,context,root,active,b)?)];let mut seen=Vec::<(SemanticDefinitionId,Vec<Arg>)>::new();
            while let Some((id,world))=queue.pop(){
                b.spend(seen.len().saturating_add(1))?;let args=&world.lexical.first().ok_or_else(||bad("thread graph has no original world"))?.actuals;
                if seen.iter().any(|(old,actuals)|*old==id&&actuals==args){continue;}seen.push((id,args.clone()));
                if !self.checked.contains(&id){return Err(bad("thread dependency has an unfinished universal original body"));}
                let definition=context.definition(module,id,b)?;
                if definition.kind!=SemanticDefinitionKind::Function||!definition.environment.is_empty()||!matches!(definition.body,SemanticDefinitionBody::FunctionWithBody{body:SemanticOriginalFunctionBody::Defined(_),foreign:None,..}){return Err(bad("thread dependency has foreign/environment or unknown execution authority"));}
                let prototype=self.prototypes.get(&id).cloned().ok_or_else(||bad("thread dependency prototype is absent"))?;
                if prototype.call_kind!=CallKind::Direct{return Err(bad("thread dependency changes its original Direct effect"));}
                self.current=Some(id);
                let mut types=Vec::new();for ty in prototype.parameters.iter().chain(std::iter::once(&prototype.result)){types.push(self.apply_type(ty,&world,b)?);}
                b.spend(self.expressions.len().saturating_add(self.bindings.len()))?;
                for((owner,_),ty)in self.expressions.iter().chain(self.bindings.iter()){if *owner==id&&!matches!(ty,Ty::Function(_)){types.push(self.apply_type(ty,&world,b)?);}}
                for ty in types{if !self.scoped_worker_type(module,context,&ty,payload,&caller,active,b)?{return Err(bad("thread dependency contains an original type outside its complete scoped manager profile"));}}
                for((owner,_),use_)in &self.intrinsic_calls{b.spend(1)?;if *owner==id&&!matches!(use_.kind,intrinsics::Intrinsic::Convert(_))&&!(payload.is_some()&&matches!(use_.kind,intrinsics::Intrinsic::MutexLock|intrinsics::Intrinsic::MutexValue|intrinsics::Intrinsic::MutexUnlock)){return Err(bad("thread dependency performs an unproved original remote operation"));}}
                let calls=self.call_targets.iter().filter(|((owner,_),_)|*owner==id).map(|(key,target)|(*key,*target)).collect::<Vec<_>>();b.spend(self.call_targets.len())?;
                for(key,target)in calls{let actuals=self.calls.get(&key).ok_or_else(||bad("thread dependency call lacks original full mixed actuals"))?;let actuals=self.apply_arguments(actuals,&world,0,b)?;let mut next=self.environment(module,context,target,active,b)?;if next.lexical[0].actuals.len()!=actuals.len(){return Err(bad("thread dependency changes complete original mixed arity"));}next.lexical[0].actuals=actuals;queue.push((target,next));}
                let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{unreachable!()};
                b.spend(self.trait_calls.len().saturating_add(self.callable_nodes.len().saturating_mul(site_nodes.len().saturating_add(1))))?;
                if self.trait_calls.keys().any(|(owner,_)|*owner==id)||self.callable_nodes.keys().any(|(owner,node)|*owner==id&&!site_nodes.iter().any(|row|row.node==SemanticOriginalNode::Expression(*node)&&self.call_targets.contains_key(&(id,row.site)))){return Err(unsupported("thread indirect/member dependency requires its exact complete original remote role",None));}
            }
        }Ok(())})();self.current=previous;outcome
    }
    fn scoped_worker_type(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,payload:Option<&Ty>,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|Ok(match self.resolved(ty,b)?{
            Ty::Atom(ty)=>ty.is_b0_scalar(),Ty::SealedError(_)=>true,
            Ty::Tuple(fields)=>{for field in fields{if !self.scoped_worker_type(module,context,&field,payload,caller,active,b)?{return Ok(false);}}true},
            Ty::Array(inner,_)=>self.scoped_worker_type(module,context,&inner,payload,caller,active,b)?,
            Ty::Desc{inner,..}=>matches!(inner.as_ref(),Ty::Carrier{kind:CarrierKind::Mutex,child,retained}if Some(child.as_ref())==payload&&retained.as_ref()==&Retained::default()),
            Ty::Carrier{kind:CarrierKind::MutexGuard,child,..}=>Some(child.as_ref())==payload,
            Ty::Address{inner,mutable,..}=>{
                if let Some(payload)=payload{self.same(&inner,payload,b)?||matches!(inner.as_ref(),Ty::Carrier{kind:CarrierKind::MutexGuard,child,..}if child.as_ref()==payload)}else{!mutable&&matches!(inner.as_ref(),Ty::Atom(Type::I32|Type::SyncCell(_)))}
            },
            _=>false,
        }))
    }
}
