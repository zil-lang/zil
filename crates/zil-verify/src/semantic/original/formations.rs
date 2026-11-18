//! Formation obligations belong to the complete original application. They
//! are not capabilities of an unconstrained formal or a concrete Typed image.
use super::*;
impl CheckedOriginal {
    // A nested nominal field can forward a condition to its own Type formal.
    // This directory only controls formation of complete applications. It is
    // never consulted as a value, ownership, or cleanup premise in a body.
    pub(super) fn formation_value(&mut self,module:&TypedModule,context:&CheckedContext,value:&Ty,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if let Some(owner) = self.nominal_field_owner
            && let Ty::Variable(definition, ordinal) = self.resolved(value,b)?
            && owner == definition && caller.current()? == owner
        {
            b.spend(self.nominal_value_formals.len().saturating_add(1))?;
            self.nominal_value_formals.insert((owner,ordinal));
            return Ok(());
        }
        self.structural_obligation(module,context,value,DynStructuralBound::Value,caller,active,b)
    }
    pub(super) fn nominal_field_schemes(&mut self,module:&TypedModule,context:&CheckedContext,owner:SemanticDefinitionId,body:&SemanticNominalBody,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let previous = self.nominal_field_owner.replace(owner);
        let mut check = |recipe: &SemanticTypeRecipe| {
            let ty = self.ty(module,context,recipe,env,active,b)?;
            self.scheme_term(module,context,&ty,env,active,b)
        };
        let result = match body {
            SemanticNominalBody::Product { fields } => fields.iter().try_for_each(|field| check(&field.ty)),
            SemanticNominalBody::Enum { variants } => variants.iter().flat_map(|variant| &variant.fields).try_for_each(check),
        };
        self.nominal_field_owner = previous;
        result
    }
    pub(super) fn collect_nominal_formation(&mut self,module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let program = module.semantic.as_ref().ok_or_else(||bad("nominal formation lacks its original declarations"))?;
        b.spend(program.definitions.len())?;
        let mut generic = Vec::new();
        let mut has_enum = false;
        for definition in &program.definitions {
            let SemanticDefinitionBody::Nominal { body, .. } = &definition.body else {continue;};
            b.spend(definition.parameters.len())?;
            if definition.parameters.iter().any(|p|matches!(p,SemanticFormal::Type { .. })) {
                has_enum |= matches!(body,SemanticNominalBody::Enum { .. });
                generic.push((definition,body));
            }
        }
        if !has_enum {return Ok(());}
        let previous = self.current;
        let result = (|| loop {
            let before = self.nominal_value_formals.len();
            for &(definition,body) in &generic {
                b.spend(1)?;
                self.current = Some(definition.id);
                let env = self.environment(module,context,definition.id,active,b)?;
                self.nominal_field_schemes(module,context,definition.id,body,&env,active,b)?;
            }
            if self.nominal_value_formals.len()==before {break Ok(());}
        })();
        self.current = previous;
        result
    }

    pub(super) fn raw_binding_family(&self,module:&TypedModule,context:&CheckedContext,ty:&Ty,b:&mut Budget)->Check<bool>{
        Ok(match self.resolved(ty,b)? {
            Ty::Store(_)|Ty::Atom(Type::ByteBuffer)=>true,
            Ty::Carrier{kind:CarrierKind::Task|CarrierKind::Mutex|CarrierKind::Registration|CarrierKind::AsyncRegistration,..}=>true,
            Ty::Nominal(id,_)=>matches!(context.definition(module,id,b)?.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Resource,..}),
            _=>false,
        })
    }
    pub(super) fn bind_family_result(&mut self,actual:&Ty,family:&Ty,b:&mut Budget)->Check<()> {
        match self.resolved(actual,b)? {
            Ty::Managed(inner)|Ty::Desc{inner,..}=>self.unify(&inner,family,b),
            _=>Err(bad("raw resource binding initializer has no complete owning or description category")),
        }
    }
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
