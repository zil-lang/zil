//! Only a proved caller trait premise exposes that trait's associated bounds.
//! The member RHS and its generic arguments remain a complete symbolic term.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic::original) fn associated_structural_premise(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,bound:DynStructuralBound,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let Ty::Projection{receiver,declaration,arguments,member}=ty else{return Ok(false);};
        if !self.trait_premise(module,context,receiver,*declaration,arguments,env,active,b)?{return Ok(false);}
        let scope=dynamic_witness::semantic_external::original_scope(module,*declaration,b)?;
        let mut declarations=module.dyn_declarations.iter().filter(|row|row.id==*declaration);
        let original=declarations.next().ok_or_else(||bad("associated bound trait original is absent"))?;
        if declarations.next().is_some()||original.kind!=DynDeclarationKind::Trait{return Err(bad("associated bound has an ambiguous trait identity"));}
        let rows=original.associated_definitions.as_ref().ok_or_else(||unsupported("associated bound inventory is Unknown",None))?;
        b.spend(rows.len())?;
        let mut rows=rows.iter().filter(|row|row.member==*member);let row=rows.next().ok_or_else(||bad("associated bound member row is absent"))?;
        if rows.next().is_some(){return Err(bad("associated bound member row is duplicated"));}
        let mut target=self.environment(module,context,scope.id,active,b)?;
        target.lexical[0].actuals=self.resolve_arguments(arguments,0,b)?;
        target.self_type=Some((scope.id,self.resolved(receiver,b)?));
        let key_receiver=self.resolved(receiver,b)?;let key_args=self.resolve_arguments(arguments,0,b)?;let term=self.resolved(ty,b)?;
        self.projection_terms.push((key_receiver,*declaration,key_args,*member,term));
        let result=(||{
            let recipes=dynamic_witness::semantic_external::original_requirement_recipes(module,*declaration,&row.requirements,b)?;
            let mut pending=recipes.iter().collect::<Vec<_>>();
            while let Some(row)=pending.pop(){b.spend(1)?;match row{
                SemanticRequirementRecipe::Structural{ty:receiver,bound:established}=>{
                    if structural::entails(*established,bound){let receiver=self.ty(module,context,receiver,&target,active,b)?;if self.same(&receiver,ty,b)?{return Ok(true);}}
                },
                SemanticRequirementRecipe::Builtin{ty:receiver,trait_kind}=>{
                    let established=match trait_kind{SemanticBuiltinTrait::Copy=>Some(DynStructuralBound::Copy),SemanticBuiltinTrait::Value=>Some(DynStructuralBound::Value),SemanticBuiltinTrait::SyncDrop=>Some(DynStructuralBound::SyncDrop),SemanticBuiltinTrait::StoreElement=>Some(DynStructuralBound::StoreElement),_=>None};
                    if established.is_some_and(|established|structural::entails(established,bound)){let receiver=self.ty(module,context,receiver,&target,active,b)?;if self.same(&receiver,ty,b)?{return Ok(true);}}
                },
                SemanticRequirementRecipe::ForAll{lifetimes:0,requirements}=>{b.spend(requirements.len())?;pending.extend(requirements);},
                _=>{},
            }}Ok(false)
        })();
        self.projection_terms.pop();result
    }
}
