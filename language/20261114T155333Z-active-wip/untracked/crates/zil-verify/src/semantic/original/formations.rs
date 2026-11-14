//! Formation obligations belong to the complete original application. They
//! are not capabilities of an unconstrained formal or a concrete Typed image.
use super::*;
impl CheckedOriginal {
    pub(super) fn check_original_applications(&mut self,module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let mut pending=Vec::new();
        b.spend(self.expressions.len().saturating_add(self.bindings.len()).saturating_add(self.prototypes.len()))?;
        for((definition,_),ty)in self.expressions.iter().chain(self.bindings.iter()) {
            // Resolve and prepay before copying the original term. Closure
            // environments and complete mixed actuals remain part of it.
            pending.push((*definition,self.resolved(ty,b)?));
        }
        for(definition,prototype)in &self.prototypes {
            b.spend(prototype.parameters.len().saturating_add(1))?;
            for ty in &prototype.parameters {pending.push((*definition,self.resolved(ty,b)?));}
            pending.push((*definition,self.resolved(&prototype.result,b)?));
        }
        let previous=self.current;
        let outcome=(||{
            for(definition,ty)in pending {
                self.current=Some(definition);
                let environment=self.prototypes.get(&definition).map(|prototype|prototype.environment.clone()).ok_or_else(||bad("original application has no completely checked caller prototype"))?;
                b.spend(environment.lexical.iter().map(|frame|frame.actuals.len().saturating_add(frame.environment.len()).saturating_add(1)).sum())?;
                self.scheme_term(module,context,&ty,&environment,active,b).map_err(|mut error|{error.message.push_str(&format!(" [original application caller {definition:?}]"));error})?;
            }
            Ok(())
        })();
        self.current=previous;outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unused_enum_applications_use_their_original_caller_formation_premises(){
        let limits=Limits::default();
        let(original,plan)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        for bound in [Some(DynStructuralBound::Value),None,Some(DynStructuralBound::SyncDrop)]{
            let mut module=original.clone();
            let program=module.semantic.as_mut().unwrap();
            let prototype=program.definitions.iter().find(|row|row.kind==SemanticDefinitionKind::Function&&matches!(row.parameters.as_slice(),[SemanticFormal::Type{..}])).unwrap().clone();
            let module_id=prototype.id.module;
            let ordinal=program.definitions.iter().filter(|row|row.id.module==module_id).map(|row|row.id.index).max().unwrap()+1;
            let nominal_id=SemanticDefinitionId{module:module_id,index:ordinal};
            let caller_id=SemanticDefinitionId{module:module_id,index:ordinal+1};
            let declaration=LifeDeclarationId{module:module_id,declaration:NodeId(module.life_declarations.iter().filter(|row|row.id.module==module_id).map(|row|row.id.declaration.0).max().unwrap_or(0)+1),compiler:false};
            let mut identity=prototype.identity.clone();identity.declaration_path.push("FormationChoice".into());
            module.life_declarations.push(LifeNominalDeclaration{id:declaration,identity:identity.clone(),kind:LifeNominalKind::Enum,parameters:vec![LifeParameterKind::Type]});
            let formal=SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0});
            program.definitions.push(SemanticDefinition{id:nominal_id,identity,parent:None,kind:SemanticDefinitionKind::Nominal,parameters:vec![SemanticFormal::Type{requirements:vec![]}],implicit_lifetimes:vec![],requirements:vec![],environment:vec![],body:SemanticDefinitionBody::Nominal{declaration,kind:LifeNominalKind::Enum,body:SemanticNominalBody::Enum{variants:vec![SemanticVariantRecipe{name:"None".into(),fields:vec![]},SemanticVariantRecipe{name:"Some".into(),fields:vec![formal.clone()]}]}}});
            let mut caller=prototype;caller.id=caller_id;caller.identity.declaration_path.push("unused_formation_identity".into());
            caller.parameters=vec![SemanticFormal::Type{requirements:bound.map(|bound|SemanticRequirementRecipe::Structural{ty:formal.clone(),bound}).into_iter().collect()}];
            let application=SemanticTypeRecipe::Nominal{definition:nominal_id,arguments:vec![SemanticActualRecipe::Type(formal)]};
            let SemanticDefinitionBody::FunctionWithBody{signature,parameters,..}=&mut caller.body else{unreachable!()};
            signature.parameters[0].ty=application.clone();signature.result=application.clone();parameters[0].ty=Some(application);
            // This complete original identity is intentionally never instantiated.
            // The existing concrete i32 identity cannot satisfy its premises.
            program.definitions.push(caller);
            for wire in [false,true]{
                let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
                let(candidate,_)=if wire{zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap()}else{(module.clone(),plan.clone())};
                let mut budget=Budget::new(&limits);let context=CheckedContext::catalogue(&candidate,&mut budget).unwrap();let mut active=ActiveQueries::new();
                let result=CheckedOriginal::check(&candidate,&context,&mut active,&mut budget);
                if bound==Some(DynStructuralBound::Value){
                    let mut checked=result.unwrap();checked.frozen_schemes(&candidate,&context,&mut active,&mut budget).unwrap();
                    checked.current=Some(caller_id);let prototype=checked.prototypes[&caller_id].clone();
                    assert!(!checked.is_copy(&candidate,&context,&prototype.parameters[0],&prototype.environment,&mut active,&mut budget).unwrap());
                    assert!(!checked.sync_drop(&candidate,&context,&prototype.parameters[0],&prototype.environment,&mut active,&mut budget).unwrap());
                }else{
                    let error=match result{Err(error)=>error,Ok(_)=>panic!("missing original Value formation premise accepted")};
                    assert!(error.message.contains("original caller lacks a complete structural/cleanup obligation proof"),"{error:?}");
                }
            }
        }
    }
}
