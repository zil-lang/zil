//! Definition bodies see fresh original lifetime identities. Partition images
//! and caller substitutions never replace that rigid checking environment.
use super::*;
impl CheckedContext {
    pub(crate) fn environment_for_instance(&self,module:&TypedModule,id:SemanticInstanceId,active:&mut ActiveQueries,b:&mut Budget)->Check<EvalEnv> {
        let mut chain=Vec::new();let mut current=Some(id);
        while let Some(id)=current {
            b.spend(chain.len().saturating_add(1))?;
            if chain.contains(&id) {return Err(bad("instance environment has a lexical parent cycle"));}
            if chain.len()>=b.limits.max_call_depth.min(128) {return Err(limit("instance environment parent depth exceeded"));}
            chain.push(id);current=self.instance(module,id,b)?.parent;
        }
        let mut lexical=Vec::new();
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
            let sealed=if instance.environment.is_some() {
                self.seals.borrow().get(id).cloned().ok_or_else(||unsupported("semantic closure environment has not been sealed at its real creator site",None))?
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
                        let mut checked=self.checked_value(module,ty,life,keys,b)?;
                        checked.origins=origins;CheckedActual::Type(checked)
                    }
                    (SemanticFormal::Const{..},SemanticActual::Const{ty,value})=>{
                        let (actual_type,life,classes)=resolved(ty)?;
                        if !classes.is_empty()||value.ty()!=*actual_type {return Err(bad("const actual carrier differs from its complete scalar value type"));}
                        let carrier=self.checked_value(module,actual_type,life,vec![],b)?;
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
