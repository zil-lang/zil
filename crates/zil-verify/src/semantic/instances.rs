//! Definition bodies see fresh original lifetime identities. Partition images
//! and caller substitutions never replace that rigid checking environment.
use super::*;
mod default_self;
pub(super) struct ClosedFrames {
    limits: Limits,
    // None is possible only for a root function whose closed arguments were
    // reconstructed directly from its immutable instance. Self, seals and
    // caller premises never take part in such a frame. Concrete callback
    // arguments still require the current creator domains below.
    premises_epoch: Option<u64>,
    lexical: Vec<std::rc::Rc<LexicalFrame>>,
    callbacks: Vec<ClosureId>,
}
impl CheckedContext {
    pub(crate) fn environment_for_instance(&self,module:&TypedModule,id:SemanticInstanceId,active:&mut ActiveQueries,b:&mut Budget)->Check<EvalEnv> {
        self.environment_for_instance_with_seal(module,id,None,active,b)
    }
    pub(in crate::semantic) fn environment_for_instance_with_seal(&self,module:&TypedModule,id:SemanticInstanceId,provisional:Option<&[CheckedActual]>,active:&mut ActiveQueries,b:&mut Budget)->Check<EvalEnv> {
        let requested=id;
        let epoch=self.premises_epoch.get();
        let memo_key=(module as *const TypedModule as usize,id,self.original.borrow().is_some(),self.domain_only.get());
        let reusable=provisional.is_none();
        let lexical=if reusable{
            b.spend(self.closed_frames.borrow().len().checked_ilog2().map_or(1,|n|n as usize+2).saturating_add(11))?;
            let frames=self.closed_frames.borrow();
            if let Some(known)=frames.get(&memo_key).filter(|known|known.limits==b.limits && known.premises_epoch.is_none_or(|generation| generation == epoch))
                && self.callbacks_still_empty(&known.callbacks, b)? {
                b.spend(known.lexical.len())?;
                if let Some(progress)=&mut b.diagnostic_progress{progress.frame_hits=progress.frame_hits.saturating_add(1);}
                Some(known.lexical.clone())
            }else{None}
        }else{None};
        let lexical=if let Some(lexical)=lexical{lexical}else{
        if let Some(progress)=&mut b.diagnostic_progress{progress.frames_built=progress.frames_built.saturating_add(1);}
        let mut chain=Vec::new();let mut current=Some(id);
        while let Some(id)=current {
            b.spend(chain.len().saturating_add(1))?;
            if chain.contains(&id) {return Err(bad("instance environment has a lexical parent cycle"));}
            if chain.len()>=b.limits.max_call_depth.min(128) {return Err(limit("instance environment parent depth exceeded"));}
            chain.push(id);current=self.instance(module,id,b)?.parent;
        }
        let mut lexical=Vec::new();
        let mut no_seals=true;
        let mut independent = chain.len() == 1;
        // Frames are built outermost first so references to real parents never
        // temporarily mean a child's same-numbered formal or SSA operand.
        for id in chain.iter().rev() {
            let instance=self.instance(module,*id,b)?;
            let definition=self.definition(module,instance.definition,b)?;
            independent &= matches!(definition.kind, SemanticDefinitionKind::Function | SemanticDefinitionKind::TaskEntry)
                && instance.self_type.is_none() && instance.environment.is_none();
            let key=ScopeKey::Instance(*id);
            let rigid=match instance.lifetime_instantiation {
                SemanticLifetimeInstantiation::Rigid=>key,
                SemanticLifetimeInstantiation::Partition{rigid}=>ScopeKey::Instance(rigid),
            };
            no_seals&=instance.environment.is_none();
            let sealed=if instance.environment.is_some() {
                if *id==requested&&provisional.is_some(){provisional.unwrap().to_vec()}else{self.seals.borrow().get(id).cloned().ok_or_else(||unsupported("semantic closure environment has not been sealed at its real creator site",None))?}
            } else {vec![]};
            let mut actuals=Vec::new();
            for (ordinal,(formal,actual)) in definition.parameters.iter().zip(&instance.actuals).enumerate() {
                b.spend(1)?;
                let value=match (formal,actual) {
                    (SemanticFormal::Lifetime,SemanticActual::Life(expression))=>{
                        if !matches!(expression,SemanticClassExpr::Formal{scope:SemanticScopeRef::Ancestor(0),ordinal:i} if *i==ordinal as u32) {return Err(bad("definition Life actual substituted a caller/other formal for its original rigid identity"));}
                        CheckedActual::Life(ClassKey::Formal{scope:rigid,ordinal:ordinal as u32})
                    }
                    (SemanticFormal::Type{..},SemanticActual::Type(value))=>{
                        let (ty,life,classes)=resolved(value)?;
                        let mut keys=Vec::new();let mut origins=Vec::new();
                        for (class,expression) in classes.iter().enumerate() {
                            if !matches!(expression,SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::TypeArgument{scope:SemanticScopeRef::Ancestor(0),ordinal:i},path,class:c} if *i==ordinal as u32 && path.is_empty() && *c==class as u32) {return Err(unsupported("semantic Type actual requires exact callee-local intrinsic anchor correspondence",None));}
                            keys.push(ClassKey::Intrinsic{scope:key,operand:OperandKey::TypeArgument{scope:key,ordinal:ordinal as u32},path:vec![],class:class as u32});
                            origins.push(IntrinsicOrigin::Existing{scope:key,operand:OperandKey::TypeArgument{scope:key,ordinal:ordinal as u32},path:vec![],class:class as u32});
                        }
                        let mut checked=self.construct_value(module,ty,life,keys,b)?;
                        checked.origins=origins;CheckedActual::Type(checked)
                    }
                    (SemanticFormal::Const{..},SemanticActual::Const{ty,value})=>{
                        let (actual_type,life,classes)=resolved(ty)?;
                        if !classes.is_empty()||value.ty()!=*actual_type {return Err(bad("const actual carrier differs from its complete scalar value type"));}
                        let carrier=self.construct_value(module,actual_type,life,vec![],b)?;
                        let pattern=DynConstPattern::Value(value.clone());
                        scalar::closed(&pattern,actual_type,module.target.pointer_width,b)?.ok_or_else(||bad("const actual is not a finite scalar"))?;
                        CheckedActual::Const{carrier,value:value.clone()}
                    }
                    (_,SemanticActual::CoreRecipe{..}|SemanticActual::CoreInferred{..})=>return Err(bad("Typed instance retains an unresolved Core actual")),
                    _=>return Err(bad("instance actual differs from its original full formal kind")),
                };
                actuals.push(value);
            }
            for (ordinal,actual) in sealed.iter().enumerate() {if let CheckedActual::Type(value)=actual {self.record(key,OperandKey::Environment{scope:key,ordinal:ordinal as u32},value,b)?;}}
            lexical.insert(0,std::rc::Rc::new(LexicalFrame{definition:definition.id,key,actuals,environment:sealed,self_type:None}));
            // Reconstruct Self from the real original impl pattern in its own
            // full actual/parent environment. A raw Self claim is compared only
            // afterwards, never used as its own receiver authority.
            let self_recipe=match &definition.body{
                SemanticDefinitionBody::Scope(SemanticScopeRecipe::InherentImpl{self_type,..}|SemanticScopeRecipe::TraitImpl{self_type,..}|SemanticScopeRecipe::BuiltinImpl{self_type,..})=>Some(self_type),
                _=>None,
            };
            let self_value=if let Some(recipe)=self_recipe{
                let scope_environment=EvalEnv{assumptions:lexical.iter().map(|frame|frame.key).collect(),lexical:lexical.clone(),images:vec![],bounds:vec![],invocations:vec![],at:None};
                let mut external=dynamic_witness::semantic_external::Proofs::default();
                let value=self.evaluate_type(module,recipe,&scope_environment,&mut external,active,b)?;
                self.finish_external_methods(module,&mut external,active,b)?;
                if !external.callable_shapes.is_empty()||!external.pending_statics.is_empty(){return Err(unsupported("impl Self construction retains unproved entry/static obligations",None));}
                Some(value)
            }else if let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration})=&definition.body
                && instance.self_type.is_some(){
                let token=FrameToken((u64::from(id.module.0)<<32)|u64::from(id.index));
                Some(active.with_query(DependencyKey{kind:QueryKind::Instance,token},b,|active,b|
                    self.default_self(module,instance,*declaration,&lexical[0].actuals,active,b))?)
            }else if matches!(definition.kind,SemanticDefinitionKind::Method){
                lexical.get(1).and_then(|parent|parent.self_type.clone())
            }else{None};
            if let Some(claim)=&instance.self_type{
                let expected=self_value.as_ref().ok_or_else(||bad("actual Self claim has no real original impl/receiver scope"))?;
                let scope_environment=EvalEnv{assumptions:lexical.iter().map(|frame|frame.key).collect(),lexical:lexical.clone(),images:vec![],bounds:vec![],invocations:vec![],at:None};
                self.claim(module,claim,expected,&scope_environment,b)?;
            }
            LexicalFrame::edit(&mut lexical[0],b)?.self_type=self_value;
        }
        let mut callbacks = Vec::new();
        let closed_callbacks = independent && !self.domain_only.get() && self.original.borrow().is_some() && self.site_index.same_input(module,b);
        if reusable&&no_seals&&epoch==self.premises_epoch.get()&&pure_frames(&lexical,module,closed_callbacks,&mut callbacks,b)?{
            b.spend(lexical.len())?;
            b.spend(self.closed_frames.borrow().len().checked_ilog2().map_or(1,|n|n as usize+2).saturating_add(13))?;
            if independent {
                for actual in &lexical[0].actuals {
                    let value = match actual {
                        CheckedActual::Type(value) | CheckedActual::Const { carrier: value, .. } => value,
                        CheckedActual::Life(_) => continue,
                    };
                    // Top-level callbacks were qualified in pure_frames and their creator
                    // domains are checked again on every retrieval.
                    if !matches!(value.ty, Type::Closure(_)) && !type_storage::closed_storage(&value.ty, b)? { independent = false; break; }
                }
            }
            self.closed_frames.borrow_mut().insert(memo_key,ClosedFrames{
                limits:b.limits.clone(), premises_epoch: (!independent).then_some(epoch), lexical:lexical.clone(), callbacks,
            });
            if let Some(progress)=&mut b.diagnostic_progress{progress.frame_cache_entries=self.closed_frames.borrow().len();}
        }
        lexical
        };
        // Invocation identities belong to this use, never to a lexical memo.
        let frame=active.fresh(b)?;
        let current=self.instance(module,id,b)?;
        let invocation_count=match current.target {
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=>{
                let actual=module.functions.get(function.0 as usize).ok_or_else(||bad("semantic actual function is absent"))?;
                match &actual.storage_life {Some(life)=>life.invocation_binder_count,None if current.header_embedding.is_empty()&&self.zero_header_function(module,actual,b)?=>0,_=>return Err(bad("semantic function invocation domain is Unknown"))}
            },
            SemanticInstanceTarget::Scope=>0,
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Core{..}) if self.frozen_functions.contains_key(&id)=>{
                let definition=self.definition(module,current.definition,b)?;
                let SemanticDefinitionBody::FunctionWithBody{signature,..}=&definition.body else{return Err(bad("frozen Core instance has no original signature"));};
                signature.invocation_lifetimes
            },
            SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure { .. })
                if self.seals.borrow().get(&id).is_some_and(Vec::is_empty) =>
            {
                let definition = self.definition(module, current.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else {
                    return Err(bad("frozen closure query has no actual original signature"));
                };
                if signature.invocation_lifetimes != 0 {
                    return Err(unsupported("frozen closure query needs its complete original invocation domain", None));
                }
                0
            }
            _=>return Err(bad("Typed instance retains a Core target")),
        };
        b.spend(invocation_count as usize)?;
        let assumptions=lexical.iter().map(|frame|frame.key).collect();
        Ok(EvalEnv{lexical,assumptions,images:vec![],bounds:vec![],invocations:(0..invocation_count).map(|index|ClassKey::Invocation{frame,index}).collect(),at:None})
    }
}

impl CheckedContext {
    // Construction reads the mutable intrinsic domain of each concrete closure.
    // A later constructor may supply its first domain or introduce a conflict.
    fn callbacks_still_empty(&self, callbacks: &[ClosureId], b: &mut Budget) -> Check<bool> {
        for id in callbacks {
            b.spend(1)?;
            if let Some(domain) = self.closure_domain(*id, b)?
                && (domain.life != empty_life()
                    || !domain.classes.is_empty()
                    || !domain.origins.is_empty()
                    || !domain.statics.is_empty())
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Only a concrete, capture-free Fn with an explicitly empty entry and
/// environment header can contribute an immutable physical argument. Its
/// lexical identity and independently checked creator domain remain separate.
pub(super) fn closed_callback(module: &TypedModule, id: ClosureId, b: &mut Budget) -> Check<bool> {
    b.spend(2)?;
    let Some(closure) = module.closures.get(id.0 as usize).filter(|c| c.id == id) else {
        return Ok(false);
    };
    let Some(entry) = module
        .functions
        .get(closure.function.0 as usize)
        .filter(|f| f.id == closure.function)
    else {
        return Ok(false);
    };
    let empty_header = |header: &Option<StorageLifeContract>| {
        header.as_ref().is_some_and(|h| {
            h.fixed_binder_count == 0
                && h.invocation_binder_count == 0
                && h.bindings.is_empty()
                && h.static_bindings.is_empty()
                && h.classes.is_empty()
        })
    };
    if closure.kind != CallableKind::Fn
        || !closure.captures.is_empty()
        || !closure.lending_captures.is_empty()
        || entry.environment != Some(id)
        || entry.foreign.is_some()
        || entry.call_kind != CallKind::Direct
        || !empty_header(&closure.storage_life)
        || !empty_header(&entry.storage_life)
        || entry
            .retained_return_sources
            .as_ref()
            .is_some_and(|r| !r.is_empty())
        || entry.parameters.len() != closure.parameters.len()
    {
        return Ok(false);
    }
    for (local, external) in entry.parameters.iter().zip(&closure.parameters) {
        b.spend(1)?;
        let Some(local) = entry
            .locals
            .get(local.0 as usize)
            .filter(|l| l.id == *local)
        else {
            return Ok(false);
        };
        dynamic_metadata::ty(&local.ty, b)?;
        dynamic_metadata::ty(external, b)?;
        if local.ty != *external || !type_storage::closed_storage(external, b)? {
            return Ok(false);
        }
    }
    dynamic_metadata::ty(&closure.result, b)?;
    dynamic_metadata::ty(&entry.return_type, b)?;
    Ok(closure.result == entry.return_type && type_storage::closed_storage(&closure.result, b)?)
}

impl LexicalFrame {
    /// A Self installation changes only this particular lexical environment.
    /// Other worlds keep the immutable frame they originally received.
    pub(crate) fn edit<'a>(frame: &'a mut std::rc::Rc<Self>, b: &mut Budget) -> Check<&'a mut Self> {
        b.spend(1)?;
        if std::rc::Rc::strong_count(frame) > 1 {
            b.spend(frame.actuals.len().saturating_add(frame.environment.len()).saturating_add(1))?;
            for value in frame.actuals.iter().chain(&frame.environment) {
                match value {
                    CheckedActual::Type(value) => value.charge_clone(b)?,
                    CheckedActual::Const { carrier,value } => { carrier.charge_clone(b)?; dynamic_metadata::constant(value,b)?; },
                    CheckedActual::Life(key) => {
                        b.spend(1)?;
                        if let ClassKey::Outer { path,.. } | ClassKey::Intrinsic { path,.. }=key { b.spend(path.len())?; }
                    }
                }
            }
            if let Some(value)=&frame.self_type { value.charge_clone(b)?; }
        }
        Ok(std::rc::Rc::make_mut(frame))
    }
}
fn pure_frames(frames:&[std::rc::Rc<LexicalFrame>],module:&TypedModule,closed_callbacks:bool,callbacks:&mut Vec<ClosureId>,b:&mut Budget)->Check<bool>{
    b.spend(frames.len())?;
    for frame in frames{
        if !frame.environment.is_empty(){return Ok(false);}
        for value in frame.actuals.iter().filter_map(|value|match value{CheckedActual::Type(value)|CheckedActual::Const{carrier:value,..}=>Some(value),_=>None}).chain(frame.self_type.iter()){
            if value.life!=empty_life()||!value.classes.is_empty()||!value.origins.is_empty()||!value.statics.is_empty(){return Ok(false);}
            if let Type::Closure(id) = value.ty {
                if !closed_callbacks || !closed_callback(module,id,b)? { return Ok(false); }
                callbacks.push(id);
                continue;
            }
            let mut pending=vec![&value.ty];
            while let Some(ty)=pending.pop(){b.spend(1)?;match ty{
                Type::BorrowedStorage{storage,life}if *life==empty_life()=>pending.push(storage),
                Type::Managed(inner)|Type::OwnedStore{element:inner}|Type::Array{element:inner,..}|Type::Slice{element:inner}|Type::Mutex{payload:inner}=>pending.push(inner),
                Type::Tuple(fields)=>{b.spend(fields.len())?;pending.extend(fields);},
                Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|(_,ty)|ty));},
                Type::Enum{variants,..}=>for(_,fields)in variants{b.spend(fields.len().saturating_add(1))?;pending.extend(fields);},
                Type::ErrorUnion{error,result}=>{pending.push(error);pending.push(result);},
                Type::FunctionPointer{parameters,result}|Type::ForeignCallback{parameters,result}=>{b.spend(parameters.len())?;pending.extend(parameters);pending.push(result);},
                Type::ByteBuffer|Type::ForeignHandle|Type::CWideString|Type::SyncCell(_)=>{},
                ty if ty.is_b0_scalar()=>{},
                _=>return Ok(false),
            }}
        }
    }Ok(true)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn closed_lexical_reuse_keeps_invocations_fresh_and_premises_and_limits_separate(){
        let limits=Limits::default();
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let instance=module.semantic.as_ref().unwrap().instances.iter().find(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_)))).unwrap().clone();
        let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();let mut active=ActiveQueries::new();
        let first=context.environment_for_instance(&module,instance.id,&mut active,&mut b).unwrap();
        let entries=context.closed_frames.borrow().len();assert!(entries>0);
        let fresh_before=active.next_token;
        let second=context.environment_for_instance(&module,instance.id,&mut active,&mut b).unwrap();
        assert_eq!(first.lexical,second.lexical);assert!(std::rc::Rc::ptr_eq(&first.lexical[0],&second.lexical[0]));assert!(active.next_token>fresh_before,"even a zero invocation domain owns a fresh use frame");
        if !first.invocations.is_empty(){assert_ne!(first.invocations,second.invocations);}
        assert_eq!(entries,context.closed_frames.borrow().len());
        context.invalidate_closed_frames();
        context.environment_for_instance(&module,instance.id,&mut active,&mut b).unwrap();
        assert_eq!(context.closed_frames.borrow().len(), entries);
        assert!(context.closed_frames.borrow().values().all(|v| v.premises_epoch.is_none()),
            "an ordinary root function uses only independently reconstructed closed inputs");
        b.limits.max_call_depth=0;
        assert!(context.environment_for_instance(&module,instance.id,&mut active,&mut b).is_err());
        b.limits=limits.clone();
        let full_entries=context.closed_frames.borrow().len();
        context.with_domain_metadata(||context.environment_for_instance(&module,instance.id,&mut active,&mut b)).unwrap();
        assert!(context.closed_frames.borrow().len()>full_entries,"domain-only metadata has a separate cache, never an actual WF receipt");
        let mut frame=first.lexical[0].clone();
        let sealed=CheckedType{type_cell:CheckedTypeData::new(Type::Closure(ClosureId(0))),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]};
        LexicalFrame::edit(&mut frame,&mut b).unwrap().actuals.push(CheckedActual::Type(sealed.clone()));assert!(!std::rc::Rc::ptr_eq(&first.lexical[0],&frame));assert_eq!(frame.actuals.len(),first.lexical[0].actuals.len()+1);assert!(!pure_frames(&[frame.clone()],&module,true,&mut vec![],&mut b).unwrap());
        LexicalFrame::edit(&mut frame,&mut b).unwrap().actuals.pop();LexicalFrame::edit(&mut frame,&mut b).unwrap().environment.push(CheckedActual::Type(sealed));assert!(!pure_frames(&[frame],&module,true,&mut vec![],&mut b).unwrap());
    }
    #[test]
    fn callback_frame_rechecks_new_creator_domains_and_both_headers() {
        let limits = Limits::default();
        let (mut module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("fixtures/local-closure-complete-head.wire"),
            &limits,
        )
        .unwrap();
        let id = module.closures[0].id;
        let entry = module.closures[0].function.0 as usize;
        let mut budget = Budget::new(&limits);
        assert!(!closed_callback(&module, id, &mut budget).unwrap());
        let original = module.closures[0].storage_life.clone();
        let mut empty = original.clone().unwrap();
        empty.fixed_binder_count = 0;
        empty.bindings.clear();
        module.closures[0].captures.clear();
        module.closures[0].storage_life = Some(empty.clone());
        assert!(!closed_callback(&module, id, &mut budget).unwrap());
        module.functions[entry].storage_life = Some(empty);
        assert!(closed_callback(&module, id, &mut budget).unwrap());
        module.closures[0].storage_life = original;
        assert!(!closed_callback(&module, id, &mut budget).unwrap());

        // The receipt is private to this domain query. Supply a later creator
        // with a concrete retained head and confirm the earlier empty result expires.
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("fixtures/local-closure-complete-head.wire"),
            &limits,
        )
        .unwrap();
        let context = CheckedContext::catalogue(&module, &mut budget).unwrap();
        assert!(context.callbacks_still_empty(&[id], &mut budget).unwrap());
        let child = module
            .semantic
            .as_ref()
            .unwrap()
            .instances
            .iter()
            .find(|row| row.environment.is_some())
            .unwrap();
        let scope = ScopeKey::Instance(child.parent.unwrap());
        let key = ClassKey::Implicit { scope, ordinal: 0 };
        let mut life = empty_life();
        life.binder_count = 1;
        let value = CheckedType {
            type_cell: CheckedTypeData::new(Type::Closure(id)),
            life,
            classes: vec![key.clone()],
            origins: vec![IntrinsicOrigin::DerivedClass { source: key }],
            statics: vec![],
        };
        context
            .closure_boundaries
            .borrow_mut()
            .push(ClosureBoundary {
                creator: scope,
                node: zil_base::NodeId(0),
                at: SitePoint {
                    function: FunctionId(0),
                    block: BlockId(0),
                    instruction: 0,
                    after: true,
                },
                child: child.id,
                value,
            });
        assert!(!context.callbacks_still_empty(&[id], &mut budget).unwrap());
    }
}
