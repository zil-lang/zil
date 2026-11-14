//! Source witnesses defer all universal member refinements to the common gate.
//! No partly checked implementation can publish an executable programme.
use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::original) fn original_trait_obligation(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&Ty,declaration:LifeDeclarationId,arguments:&[Arg],caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let token=active.fresh(b)?;
        active.with_query(DependencyKey{kind:QueryKind::Requirement,token},b,|active,b|{
            let scope=dynamic_witness::semantic_external::original_scope(module,declaration,b)?;
            if !matches!(scope.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration:actual})if actual==declaration)||scope.parameters.len()!=arguments.len(){return Err(bad("original trait obligation loses its trait identity or full mixed arity"));}
            if self.trait_premise(module,context,receiver,declaration,arguments,caller,active,b)?{return Ok(());}
            let mut target=self.environment(module,context,scope.id,active,b)?;
            target.lexical[0].actuals=self.resolve_arguments(arguments,0,b)?;
            target.self_type=Some((scope.id,self.resolved(receiver,b)?));
            for(formal,actual)in scope.parameters.iter().zip(arguments){match(formal,actual){
                (SemanticFormal::Type{requirements},Arg::Type(_))=>for requirement in requirements{self.obligation(module,context,requirement,&target,caller,active,b)?;},
                (SemanticFormal::Lifetime,Arg::Life(_))=>{},
                (SemanticFormal::Const{carrier},Arg::Const(actual))=>{let expected=self.ty(module,context,carrier,&target,active,b)?;self.scheme_carrier(&expected,b)?;if !self.same(&expected,&actual.carrier,b)?{return Err(bad("original trait obligation changes its complete Const carrier"));}},
                _=>return Err(bad("original trait obligation changes an ordered actual kind")),
            }}
            for requirement in &scope.requirements{self.obligation(module,context,requirement,&target,caller,active,b)?;}
            let provider=self.original_provider(module,context,receiver,declaration,arguments,caller,active,b)?.ok_or_else(||unsupported("original trait receiver has no unique complete applicable impl",None))?;
            b.spend(self.trait_witnesses.len().saturating_add(1))?;
            if !self.trait_witnesses.contains(&(declaration,provider.declaration)){self.trait_witnesses.push((declaration,provider.declaration));}
            Ok(())
        })
    }
    pub(in crate::semantic::original) fn check_trait_witnesses(&mut self,module:&TypedModule,context:&CheckedContext,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let programme=module.semantic.as_ref().ok_or_else(||bad("original trait proof lacks its programme"))?;
        let mut checked=Vec::new();
        while let Some((trait_id,impl_id))=self.trait_witnesses.pop(){
            b.spend(checked.len().saturating_add(module.dyn_declarations.len()).saturating_add(programme.dyn_scopes.len()))?;
            if checked.contains(&(trait_id,impl_id)){continue;}
            let trait_decl=module.dyn_declarations.iter().find(|row|row.id==trait_id).ok_or_else(||bad("original witness trait is absent"))?;
            let implementation=module.dyn_declarations.iter().find(|row|row.id==impl_id).ok_or_else(||bad("original witness impl is absent"))?;
            let signatures=trait_decl.method_signatures.as_ref().ok_or_else(||unsupported("original trait method inventory is Unknown",None))?;
            for(ordinal,member)in trait_decl.members.iter().enumerate(){if matches!(member.kind,DynMemberKind::Method(_)){
                let signature=signatures.iter().find(|row|row.member==ordinal as u32).ok_or_else(||bad("original witness omits a trait method signature"))?;
                let mut expected=programme.dyn_scopes.iter().filter(|row|row.owner==SemanticDynOwner::MethodSignature{declaration:trait_id,member:ordinal as u32});
                let expected=expected.next().ok_or_else(||bad("original witness has no exact method declaration scope"))?;
                let overrides=implementation.members.iter().enumerate().filter(|(_,row)|row.name==member.name&&row.kind==member.kind).collect::<Vec<_>>();
                if overrides.len()>1{return Err(bad("original witness has ambiguous method overrides"));}
                let(source,definition)=if let Some((index,_))=overrides.first(){
                    let definitions=implementation.method_definitions.as_ref().ok_or_else(||unsupported("original override inventory is Unknown",None))?;
                    let mut bodies=definitions.iter().filter(|row|row.member==*index as u32&&row.scope==DynDefinitionScope::ImplOverride);
                    let body=bodies.next().ok_or_else(||bad("original method override lacks its body"))?;if bodies.next().is_some(){return Err(bad("original method override is duplicated"));}(DynDefinitionScope::ImplOverride,body.id)
                }else{(DynDefinitionScope::TraitDefault,signature.default_definition.ok_or_else(||bad("original trait method has no override/default body"))?)};
                let mut bodies=programme.dyn_scopes.iter().filter(|row|row.owner==SemanticDynOwner::MethodDefinition(definition));let body=bodies.next().ok_or_else(||bad("original witness body has no exact source scope"))?;
                if bodies.next().is_some(){return Err(bad("original witness body scope is ambiguous"));}
                self.method_relation(module,context,trait_id,impl_id,expected.scope,body.scope,source,active,b)?;
            }}
            checked.push((trait_id,impl_id));
        }
        Ok(())
    }
}
