//! Cleanup predicates over the checked original Gamma. These helpers neither
//! infer expression types nor inspect actual instances or emitted Drop nodes.
use super::*;

fn with_check<T>(
    program:&SemanticModule,proof:&mut Proof,owner:SemanticDefinitionId,
    limits:&Limits,left:&mut usize,run:impl FnOnce(&mut Check<'_, '_>)->R<T>,
)->R<T>{
    *left=left.checked_sub(proof.complete.len().saturating_add(1)).ok_or_else(||Diagnostic::limit(Phase::Types,"original cleanup context work exceeded",None))?;
    if !proof.complete.contains(&owner){return Err(bad("cleanup obligation lacks its complete original owner body"));}
    let mut check=Check{
        program,limits,left,proof:std::mem::take(proof),active:BTreeSet::new(),captures:BTreeMap::new(),loops:Vec::new(),trait_active:Vec::new(),contexts:vec![owner],predicate_active:Vec::new(),stage_depth:0,cleanup_depth:0,requested_snapshot:None,escape_active:Vec::new(),
    };
    let result=run(&mut check);
    // Restore even on rejection or resource exhaustion. No speculative fact
    // from an actual specialization is ever installed in this context.
    *proof=std::mem::take(&mut check.proof);
    result
}
pub(super) fn copy(program:&SemanticModule,proof:&mut Proof,owner:SemanticDefinitionId,ty:&SemanticTypeRecipe,limits:&Limits,left:&mut usize)->R<bool>{
    with_check(program,proof,owner,limits,left,|check|check.copy_type(owner,ty,0))
}
pub(super) fn capability(program:&SemanticModule,proof:&mut Proof,owner:SemanticDefinitionId,ty:&SemanticTypeRecipe,limits:&Limits,left:&mut usize)->R<bool>{
    with_check(program,proof,owner,limits,left,|check|check.cleanup_capability(owner,ty,0))
}
/// Exact product fields, in declaration order and in the use definition's
/// original domain. None is an unknown/non-product, never an empty inventory.
pub(super) fn fields(program:&SemanticModule,proof:&mut Proof,owner:SemanticDefinitionId,ty:&SemanticTypeRecipe,limits:&Limits,left:&mut usize)->R<Option<Vec<(String,SemanticTypeRecipe)>>>{
    with_check(program,proof,owner,limits,left,|check|check.cleanup_fields(owner,ty,0))
}
impl Check<'_, '_>{
    fn cleanup_capability(&mut self,owner:SemanticDefinitionId,ty:&SemanticTypeRecipe,depth:usize)->R<bool>{
        self.pay(1,depth)?;
        if let SemanticTypeRecipe::Environment{scope:SemanticScopeRef::Ancestor(hops),ordinal}=ty{
            let mut holder=owner;for step in 0..*hops{self.pay(1,depth+step as usize)?;holder=self.definition(holder)?.parent.ok_or_else(||bad("cleanup environment escapes its actual lexical ancestry"))?;}
            let (parent,source)=self.proof.environments.get(&(holder,*ordinal)).ok_or_else(||pending("cleanup requires its sealed original capture Type source"))?;
            let parent=*parent;let source=copy_recipe(source,self.limits,self.left)?;
            self.contexts.push(parent);let result=self.cleanup_capability(parent,&source,depth+1);self.contexts.pop();return result;
        }
        let ty=self.observation_type(owner,ty,depth+1)?;
        // Releasing an address/description ends that view; it does not destroy
        // the referent and does not establish SyncDrop of its payload type.
        if matches!(ty,SemanticTypeRecipe::Address{..}|SemanticTypeRecipe::Desc{..}|SemanticTypeRecipe::Code{..}){return Ok(true);}
        if let SemanticTypeRecipe::Qualified{storage,..}=&ty{return self.cleanup_capability(owner,storage,depth+1);}
        if self.copy_type(owner,&ty,depth+1)?{return Ok(true);}
        for requirement in self.original_requirements(owner,depth+1)?{
            self.pay(1,depth)?;
            let premise=match &requirement{
                SemanticRequirementRecipe::Structural{ty,bound:DynStructuralBound::SyncDrop|DynStructuralBound::Copy|DynStructuralBound::StoreElement}=>Some(ty),
                SemanticRequirementRecipe::Builtin{ty,trait_kind:SemanticBuiltinTrait::SyncDrop|SemanticBuiltinTrait::Copy|SemanticBuiltinTrait::StoreElement}=>Some(ty),_=>None,
            };
            if let Some(premise)=premise{if self.premise_type_equal(owner,&ty,premise,depth+1)?{return Ok(true);}}
        }
        match &ty{
            SemanticTypeRecipe::Tuple(fields)|SemanticTypeRecipe::AnonymousSum(fields)=>{for field in fields{if !self.cleanup_capability(owner,field,depth+1)?{return Ok(false);}}return Ok(true);}
            SemanticTypeRecipe::Array{element,..}=>return self.cleanup_capability(owner,element,depth+1),
            SemanticTypeRecipe::ErrorUnion{error,result}=>return Ok(self.cleanup_capability(owner,error,depth+1)?&&self.cleanup_capability(owner,result,depth+1)?),
            SemanticTypeRecipe::Nominal{definition,..}=>{
                let original=self.definition(*definition)?;
                if matches!(original.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Record|LifeNominalKind::Package,body:SemanticNominalBody::Product{..},..}){
                    let fields=self.cleanup_fields(owner,&ty,depth+1)?.ok_or_else(||bad("cleanup product lost its actual source declaration"))?;
                    for (_,field)in fields{if !self.cleanup_capability(owner,&field,depth+1)?{return Ok(false);}}return Ok(true);
                }
            }
            _=>{},
        }
        if let Some(value)=self.sealed_capability(owner,&ty,SemanticBuiltinTrait::SyncDrop,depth+1)?{return Ok(value);}
        Ok(false)
    }
    fn cleanup_fields(&mut self,owner:SemanticDefinitionId,ty:&SemanticTypeRecipe,depth:usize)->R<Option<Vec<(String,SemanticTypeRecipe)>>>{
        self.pay(1,depth)?;
        let ty=self.observation_type(owner,ty,depth+1)?;
        if let SemanticTypeRecipe::Qualified{storage,..}=&ty{return self.cleanup_fields(owner,storage,depth+1);}
        let SemanticTypeRecipe::Nominal{definition,arguments}=&ty else{return Ok(None);};
        let original=self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..}=&original.body else{return Ok(None);};
        self.call_requirements(owner,original,arguments,depth+1)?;
        self.pay(fields.len(),depth)?;
        let mut out=Vec::with_capacity(fields.len());
        for field in fields{
            self.pay(field.name.len().saturating_add(1),depth)?;
            let ty=super::super::instantiate::ty(self.program,&self.proof,&field.ty,super::super::instantiate::DomainMapping{source:original.id,use_definition:owner,arguments,parent_call:None},self.limits,self.left)?;
            out.push((field.name.clone(),ty));
        }
        Ok(Some(out))
    }
}
