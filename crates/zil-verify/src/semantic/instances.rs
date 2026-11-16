//! Definition bodies see fresh original lifetime identities. Partition images
//! and caller substitutions never replace that rigid checking environment.
use super::*;
pub(super) struct ClosedFrames{limits:Limits,lexical:Vec<LexicalFrame>}
impl CheckedContext {
    pub(crate) fn environment_for_instance(&self,module:&TypedModule,id:SemanticInstanceId,active:&mut ActiveQueries,b:&mut Budget)->Check<EvalEnv> {
        self.environment_for_instance_with_seal(module,id,None,active,b)
    }
    pub(in crate::semantic) fn environment_for_instance_with_seal(&self,module:&TypedModule,id:SemanticInstanceId,provisional:Option<&[CheckedActual]>,active:&mut ActiveQueries,b:&mut Budget)->Check<EvalEnv> {
        let requested=id;
        let epoch=self.premises_epoch.get();
        let memo_key=(module as *const TypedModule as usize,id,epoch,self.original.borrow().is_some(),self.domain_only.get());
        let reusable=provisional.is_none();
        let lexical=if reusable{
            b.spend(self.closed_frames.borrow().len().checked_ilog2().map_or(1,|n|n as usize+2).saturating_add(11))?;
            let frames=self.closed_frames.borrow();
            if let Some(known)=frames.get(&memo_key).filter(|known|known.limits==b.limits){
                charge_frames(&known.lexical,b)?;
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
        // Frames are built outermost first so references to real parents never
        // temporarily mean a child's same-numbered formal or SSA operand.
        for id in chain.iter().rev() {
            let instance=self.instance(module,*id,b)?;
            let definition=self.definition(module,instance.definition,b)?;
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
            lexical.insert(0,LexicalFrame{definition:definition.id,key,actuals,environment:sealed,self_type:None});
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
            }else if matches!(definition.kind,SemanticDefinitionKind::Method){
                lexical.get(1).and_then(|parent|parent.self_type.clone())
            }else{None};
            if let Some(claim)=&instance.self_type{
                let expected=self_value.as_ref().ok_or_else(||bad("actual Self claim has no real original impl/receiver scope"))?;
                let scope_environment=EvalEnv{assumptions:lexical.iter().map(|frame|frame.key).collect(),lexical:lexical.clone(),images:vec![],bounds:vec![],invocations:vec![],at:None};
                self.claim(module,claim,expected,&scope_environment,b)?;
            }
            lexical[0].self_type=self_value;
        }
        if reusable&&no_seals&&epoch==self.premises_epoch.get()&&pure_frames(&lexical,b)?{
            charge_frames(&lexical,b)?;
            b.spend(self.closed_frames.borrow().len().checked_ilog2().map_or(1,|n|n as usize+2).saturating_add(13))?;
            self.closed_frames.borrow_mut().insert(memo_key,ClosedFrames{limits:b.limits.clone(),lexical:lexical.clone()});
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
            _=>return Err(bad("Typed instance retains a Core target")),
        };
        b.spend(invocation_count as usize)?;
        let assumptions=lexical.iter().map(|frame|frame.key).collect();
        Ok(EvalEnv{lexical,assumptions,images:vec![],bounds:vec![],invocations:(0..invocation_count).map(|index|ClassKey::Invocation{frame,index}).collect(),at:None})
    }
}

fn charge_frames(frames:&[LexicalFrame],b:&mut Budget)->Check<()>{
    b.spend(frames.len())?;
    for frame in frames{
        b.spend(frame.actuals.len().saturating_add(frame.environment.len()).saturating_add(1))?;
        for value in frame.actuals.iter().chain(&frame.environment){match value{
            CheckedActual::Type(value)=>value.charge_clone(b)?,
            CheckedActual::Const{carrier,value}=>{carrier.charge_clone(b)?;dynamic_metadata::constant(value,b)?;},
            CheckedActual::Life(key)=>{b.spend(1)?;if let ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=key{b.spend(path.len())?;}},
        }}
        if let Some(value)=&frame.self_type{value.charge_clone(b)?;}
    }Ok(())
}
fn pure_frames(frames:&[LexicalFrame],b:&mut Budget)->Check<bool>{
    b.spend(frames.len())?;
    for frame in frames{
        if !frame.environment.is_empty(){return Ok(false);}
        for value in frame.actuals.iter().filter_map(|value|match value{CheckedActual::Type(value)|CheckedActual::Const{carrier:value,..}=>Some(value),_=>None}).chain(frame.self_type.iter()){
            if value.life!=empty_life()||!value.classes.is_empty()||!value.origins.is_empty()||!value.statics.is_empty(){return Ok(false);}
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
        assert_eq!(first.lexical,second.lexical);assert!(active.next_token>fresh_before,"even a zero invocation domain owns a fresh use frame");
        if !first.invocations.is_empty(){assert_ne!(first.invocations,second.invocations);}
        assert_eq!(entries,context.closed_frames.borrow().len());
        context.invalidate_closed_frames();
        context.environment_for_instance(&module,instance.id,&mut active,&mut b).unwrap();
        assert!(context.closed_frames.borrow().len()>entries);
        b.limits.max_call_depth=0;
        assert!(context.environment_for_instance(&module,instance.id,&mut active,&mut b).is_err());
        b.limits=limits.clone();
        let full_entries=context.closed_frames.borrow().len();
        context.with_domain_metadata(||context.environment_for_instance(&module,instance.id,&mut active,&mut b)).unwrap();
        assert!(context.closed_frames.borrow().len()>full_entries,"domain-only metadata has a separate cache, never an actual WF receipt");
        let mut frame=first.lexical[0].clone();
        let sealed=CheckedType{type_cell:CheckedTypeData::new(Type::Closure(ClosureId(0))),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]};
        frame.actuals.push(CheckedActual::Type(sealed.clone()));assert!(!pure_frames(&[frame.clone()],&mut b).unwrap());
        frame.actuals.pop();frame.environment.push(CheckedActual::Type(sealed));assert!(!pure_frames(&[frame],&mut b).unwrap());
    }
}
