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
                            self.premises.borrow_mut().entry(key).or_default().relations.equalities.push((left,right));
                        }
                        SemanticRequirementRecipe::Outlives{longer,shorter}=>{
                            let longer=self.life(module,longer,environment,b)?;let shorter=self.life(module,shorter,environment,b)?;
                            b.spend(1)?;
                            self.premises.borrow_mut().entry(key).or_default().relations.outlives.push((longer,shorter));
                        }
                        _=>{}
                    }
                }
            }
            for recipe in recipes {self.requirement(module,recipe,environment,mode,external,active,b)?;}
            Ok(())
        })();
        if result.is_err()&&mode==RequirementMode::DeclarationPremise {
            let mut graph=self.premises.borrow_mut();
            if let Some(previous)=previous {graph.insert(key,previous);}else{graph.remove(&key);}
        }
        result
    }
    fn requirement(&self,module:&TypedModule,recipe:&SemanticRequirementRecipe,environment:&EvalEnv,mode:RequirementMode,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        dynamic_metadata::recipe(recipe,b)?;
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
                let signature=self.evaluate_signature(module,signature,environment,external,active,b)?;
                self.external_requirement(module,environment,ExternalRequest::Callable{receiver,kind:*kind,signature},mode,external,active,b)?;
            }
            SemanticRequirementRecipe::ForAll{lifetimes,requirements}=>{
                let mut environment=environment.clone();
                if *lifetimes>0 {
                    b.spend(*lifetimes as usize)?;
                    let token=active.fresh(b)?;
                    environment.bounds.insert(0,BoundFrame{token,classes:(0..*lifetimes).map(|index|ClassKey::Bound{frame:token,index}).collect()});
                }
                // Nested rows are authorized by the already checked original
                // whole ForAll row, not a fresh arbitrary premise entry.
                for recipe in requirements {self.requirement(module,recipe,&environment,mode,external,active,b)?;}
            }
        }
        Ok(())
    }
    fn external_requirement(&self,module:&TypedModule,environment:&EvalEnv,request:ExternalRequest,mode:RequirementMode,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let key=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        if mode==RequirementMode::DeclarationPremise {
            b.spend(1)?;
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
