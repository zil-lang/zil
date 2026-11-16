//! Original premises and concrete obligations have separate entry paths.
//! A successful actual Callable role check never modifies the equality graph.
use super::*;
impl CheckedContext {
    pub(crate) fn evaluate_requirements(&self,module:&TypedModule,recipes:&[SemanticRequirementRecipe],environment:&EvalEnv,mode:RequirementMode,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(recipes.len())?;
        if mode==RequirementMode::DeclarationPremise {
            let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
            let definition=self.definition(module,frame.definition,b)?;
            // A private caller cannot manufacture premises from a prospective
            // implementation: each entire row must be original in this scope.
            for recipe in recipes {
                dynamic_metadata::recipe(recipe,b)?;
                let present=definition.requirements.contains(recipe)||definition.parameters.iter().any(|formal|matches!(formal,SemanticFormal::Type{requirements} if requirements.contains(recipe)));
                if !present {return Err(bad("declaration assumption is not an original requirement of this exact scope"));}
            }
        }
        // Establish only original exact equalities first, so checking a Type
        // requirement does not depend on arbitrary source ordering. Roll back
        // this scope on any failure; no half-checked Γ escapes the operation.
        let key=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        let previous=if mode==RequirementMode::DeclarationPremise {
            let graph=self.premises.borrow();
            let old=graph.get(&key).cloned();
            if let Some(row)=&old { b.spend(row.relations.equalities.len().saturating_add(row.relations.outlives.len()).saturating_add(row.external.len()))?; }
            old
        }else{None};
        let result=(|| {
            if mode==RequirementMode::DeclarationPremise {
                for recipe in recipes {
                    match recipe {
                        SemanticRequirementRecipe::LifeEqual{left,right}=>{
                            let left=self.life(module,left,environment,b)?;let right=self.life(module,right,environment,b)?;
                            b.spend(1)?;
                            b.spend(1)?;self.invalidate_closed_frames();
                            self.premises.borrow_mut().entry(key).or_default().relations.equalities.push((left,right));
                        }
                        SemanticRequirementRecipe::Outlives{longer,shorter}=>{
                            let longer=self.life(module,longer,environment,b)?;let shorter=self.life(module,shorter,environment,b)?;
                            b.spend(1)?;
                            b.spend(1)?;self.invalidate_closed_frames();
                            self.premises.borrow_mut().entry(key).or_default().relations.outlives.push((longer,shorter));
                        }
                        _=>{}
                    }
                }
            }
            for recipe in recipes {self.requirement(module,recipe,environment,&[],mode,external,active,b)?;}
            Ok(())
        })();
        if result.is_err()&&mode==RequirementMode::DeclarationPremise {
            self.invalidate_closed_frames();
            let mut graph=self.premises.borrow_mut();
            if let Some(previous)=previous {graph.insert(key,previous);}else{graph.remove(&key);}
        }
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn requirement(&self,module:&TypedModule,recipe:&SemanticRequirementRecipe,environment:&EvalEnv,goals:&[BoundFrame],mode:RequirementMode,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        dynamic_metadata::bounds_recipe(recipe,b)?;
        match recipe {
            SemanticRequirementRecipe::LifeEqual{left,right}=>{
                let left=self.life(module,left,environment,b)?;let right=self.life(module,right,environment,b)?;
                if !self.same(environment,&left,&right,b)? {return Err(bad("actual call does not prove the original LifeEqual requirement"));}
            }
            SemanticRequirementRecipe::Outlives{longer,shorter}=>{
                let longer=self.life(module,longer,environment,b)?;let shorter=self.life(module,shorter,environment,b)?;
                if !self.outlives(environment,&longer,&shorter,b)? {return Err(bad("actual call does not prove the original Outlives requirement"));}
            }
            SemanticRequirementRecipe::TypeEqual{left,right}=>{
                let left=self.evaluate_type(module,left,environment,external,active,b)?;
                let right=self.evaluate_type(module,right,environment,external,active,b)?;
                if !self.same_type(environment,&left,&right,b)? {return Err(unsupported("semantic TypeEqual needs exact full-type congruence from its original premise",None));}
            }
            SemanticRequirementRecipe::ConstEqual{carrier,left,right}=>{
                let carrier=self.evaluate_type(module,carrier,environment,external,active,b)?;
                let left=self.evaluate_constant(module,left,&carrier,environment,external,active,b)?;
                let right=self.evaluate_constant(module,right,&carrier,environment,external,active,b)?;
                if left!=right {return Err(bad("actual const values do not satisfy the complete-carrier equality"));}
            }
            SemanticRequirementRecipe::Builtin{ty,trait_kind}=>{
                let receiver=self.evaluate_type(module,ty,environment,external,active,b)?;
                self.external_requirement(module,environment,ExternalRequest::Builtin{receiver,trait_kind:*trait_kind},mode,external,active,b)?;
            }
            SemanticRequirementRecipe::Structural{ty,bound}=>{
                let receiver=self.evaluate_type(module,ty,environment,external,active,b)?;
                self.external_requirement(module,environment,ExternalRequest::Structural{receiver,bound:*bound},mode,external,active,b)?;
            }
            SemanticRequirementRecipe::Trait{ty,application}=>{
                let receiver=self.evaluate_type(module,ty,environment,external,active,b)?;
                let application=self.application(module,application,environment,external,active,b)?;
                self.external_requirement(module,environment,ExternalRequest::Trait{receiver,application},mode,external,active,b)?;
            }
            SemanticRequirementRecipe::Callable{ty,kind,signature}=>{
                let receiver=self.evaluate_type(module,ty,environment,external,active,b)?;
                let mut signature=self.evaluate_signature(module,signature,environment,external,active,b)?;
                // These are only the fresh universal goals introduced while
                // descending this original requirement. Existing caller Bound
                // classes and retained κ stay rigid in the request domain.
                for frame in goals{
                    b.spend(frame.classes.len().saturating_add(1))?;
                    if frame.classes.is_empty(){return Err(bad("zero Callable goal frame incorrectly consumes a quantified scope"));}
                    for(index,key)in frame.classes.iter().enumerate(){
                        if key!=&(ClassKey::Bound{frame:frame.token,index:index as u32}){return Err(bad("Callable goal changes its exact fresh Bound coordinate directory"));}
                        b.spend(signature.bound.len())?;if signature.bound.contains(key){return Err(bad("Callable goal repeats another private signature frame"));}
                        signature.bound.push(key.clone());
                    }
                }
                self.external_requirement(module,environment,ExternalRequest::Callable{receiver,kind:*kind,signature},mode,external,active,b)?;
            }
            SemanticRequirementRecipe::ForAll{lifetimes,requirements}=>{
                let mut environment=environment.clone();
                b.spend(goals.iter().fold(goals.len(),|count,frame|count.saturating_add(frame.classes.len())))?;
                let mut goals=goals.to_vec();
                if *lifetimes>0 {
                    b.spend(*lifetimes as usize)?;
                    let token=active.fresh(b)?;
                    let frame=BoundFrame{token,classes:(0..*lifetimes).map(|index|ClassKey::Bound{frame:token,index}).collect()};
                    b.spend(frame.classes.len().saturating_add(1))?;goals.push(frame.clone());environment.bounds.insert(0,frame);
                }
                // Nested rows are authorized by the already checked original
                // whole ForAll row, not a fresh arbitrary premise entry.
                for recipe in requirements {self.requirement(module,recipe,&environment,&goals,mode,external,active,b)?;}
            }
        }
        Ok(())
    }
    pub(crate) fn external_requirement(&self,module:&TypedModule,environment:&EvalEnv,request:ExternalRequest,mode:RequirementMode,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let key=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        if mode==RequirementMode::DeclarationPremise {
            b.spend(1)?;
            b.spend(1)?;self.invalidate_closed_frames();
            self.premises.borrow_mut().entry(key).or_default().external.push(request);
            return Ok(());
        }
        {
            let graph=self.premises.borrow();
            for key in &environment.assumptions {
                if let Some(rows)=graph.get(key) {
                    b.spend(rows.external.len())?;
                    // Role satisfaction remains local to the complete original
                    // request. It contributes no class-equivalence edges.
                    if rows.external.contains(&request) {return Ok(());}
                }
            }
        }
        match external.query(module,self,environment,&request,mode,active,b)? {
            ExternalValue::Proved=>Ok(()),_=>Err(bad("requirement callback returned a value instead of satisfaction")),
        }
    }
}

#[cfg(test)]
mod quantified_callable_tests{
    use super::*;
    #[derive(Default)]
    struct Requests(Vec<CheckedSignature>);
    impl ExternalProofs for Requests{
        fn query(&mut self,_module:&TypedModule,_context:&CheckedContext,_environment:&EvalEnv,request:&ExternalRequest,_mode:RequirementMode,_active:&mut ActiveQueries,_budget:&mut Budget)->Check<ExternalValue>{let ExternalRequest::Callable{signature,..}=request else{panic!("unexpected request")};self.0.push(signature.clone());Ok(ExternalValue::Proved)}
    }
    #[test]
    fn callable_goal_directories_preserve_nested_nonzero_frames_without_promoting_caller_bounds(){
        let limits=Limits::default();let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("fixtures/ordinary-identity-complete-call-sites.wire"),&limits).unwrap();let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();let instance=&module.semantic.as_ref().unwrap().instances[0];let scope=ScopeKey::Instance(instance.id);
        let caller=BoundFrame{token:FrameToken(900),classes:vec![ClassKey::Bound{frame:FrameToken(900),index:0},ClassKey::Bound{frame:FrameToken(900),index:1}]};let env=EvalEnv{lexical:vec![LexicalFrame{definition:instance.definition,key:scope,actuals:vec![],environment:vec![],self_type:None}],assumptions:vec![scope],images:vec![],bounds:vec![caller.clone()],invocations:vec![],at:None};
        let parameter=|lifetime|SemanticParameterRecipe{ty:SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Class(lifetime),inner:Box::new(SemanticTypeRecipe::Atom(Type::I32))},mode:PassingMode::Infer};
        // A request-transport control with an external recorder. It is not an
        // executable Callable/SSA or actual entry/body admission proof.
        let signature=SemanticSignatureRecipe{bound_lifetimes:1,invocation_lifetimes:1,parameters:vec![parameter(SemanticLifeRecipe::Bound{depth:0,index:0}),parameter(SemanticLifeRecipe::Bound{depth:1,index:0}),parameter(SemanticLifeRecipe::Bound{depth:2,index:1}),parameter(SemanticLifeRecipe::Bound{depth:3,index:0}),parameter(SemanticLifeRecipe::Invocation(0))],result:SemanticTypeRecipe::Atom(Type::Unit),result_mode:PassingMode::Infer,call_kind:CallKind::Direct};
        let requirement=SemanticRequirementRecipe::ForAll{lifetimes:2,requirements:vec![SemanticRequirementRecipe::ForAll{lifetimes:0,requirements:vec![SemanticRequirementRecipe::ForAll{lifetimes:1,requirements:vec![SemanticRequirementRecipe::Callable{ty:SemanticTypeRecipe::Atom(Type::I32),kind:CallableKind::Fn,signature:Box::new(signature)}]}]}]};
        let mut external=Requests::default();context.evaluate_requirements(&module,&[requirement],&env,RequirementMode::ActualObligation,&mut external,&mut ActiveQueries::new(),&mut b).unwrap();let [actual]=external.0.as_slice()else{panic!()};
        assert_eq!(actual.bound.len(),4);assert_eq!(actual.invocation.len(),1);assert!(actual.bound.iter().all(|key|!caller.classes.contains(key)));assert_eq!(env.bounds,vec![caller.clone()]);
        assert!(actual.bound.contains(&actual.parameters[0].0.classes[0]));assert!(actual.bound.contains(&actual.parameters[1].0.classes[0]));assert!(actual.bound.contains(&actual.parameters[2].0.classes[0]));assert_eq!(actual.parameters[3].0.classes[0],caller.classes[0]);assert!(!actual.bound.contains(&actual.parameters[3].0.classes[0]));assert_eq!(actual.parameters[4].0.classes[0],actual.invocation[0]);
        let mut frames=std::collections::BTreeMap::<FrameToken,Vec<u32>>::new();for key in &actual.bound{let ClassKey::Bound{frame,index}=key else{panic!()};frames.entry(*frame).or_default().push(*index);}let mut sizes=frames.values().map(Vec::len).collect::<Vec<_>>();sizes.sort();assert_eq!(sizes,vec![1,1,2]);assert!(frames.values().any(|indices|indices==&vec![0,1]));
        for key in &actual.bound{assert!(!context.same(&env,key,&caller.classes[0],&mut b).unwrap());}
    }
}
