//! Abstract original methods are declaration schemes, never executable bodies.
//! Their identity and full symbolic header come from the exact trait member.
use super::*;
mod relation;

impl CheckedOriginal {
    pub(super) fn abstract_method(&mut self,module:&TypedModule,context:&CheckedContext,definition:&SemanticDefinition,prototype:&Prototype,_active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if definition.kind!=SemanticDefinitionKind::Method{return Err(bad("abstract original body is not a method declaration"));}
        let programme=module.semantic.as_ref().ok_or_else(||bad("abstract original method lacks its programme"))?;
        b.spend(programme.dyn_scopes.len())?;
        let mut bindings=programme.dyn_scopes.iter().filter(|row|row.scope==definition.id&&matches!(row.owner,SemanticDynOwner::MethodSignature{..}));
        let binding=bindings.next().ok_or_else(||bad("abstract original method has no exact trait member identity"))?;
        if bindings.next().is_some(){return Err(bad("abstract original method has multiple trait member identities"));}
        let SemanticDynOwner::MethodSignature{declaration,member}=binding.owner else{unreachable!()};
        let parent=dynamic_witness::semantic_external::original_scope(module,declaration,b)?;
        if definition.parent!=Some(parent.id)||!matches!(parent.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration:actual})if actual==declaration){return Err(bad("abstract method has another original trait/lexical parent"));}
        let mut declarations=module.dyn_declarations.iter().filter(|row|row.id==declaration);
        let original=declarations.next().ok_or_else(||bad("abstract original trait declaration is absent"))?;
        if declarations.next().is_some()||original.kind!=DynDeclarationKind::Trait{return Err(bad("abstract original trait identity/kind is ambiguous"));}
        dynamic_metadata::declaration(original,b)?;
        let Some(DynMemberDeclaration{kind:DynMemberKind::Method(formals),..})=original.members.get(member as usize) else{return Err(bad("abstract original member changes its kind or ordinal"));};
        if formals.len()!=definition.parameters.len(){return Err(bad("abstract original method omits its full mixed parameter list"));}
        for(formal,actual)in definition.parameters.iter().zip(formals){match(formal,actual){
            (SemanticFormal::Lifetime,LifeParameterKind::Lifetime)|(SemanticFormal::Type{..},LifeParameterKind::Type)=>{},
            (SemanticFormal::Const{carrier},LifeParameterKind::Const(ty))=>{
                if !matches!(carrier,SemanticTypeRecipe::Atom(actual)if actual==ty){return Err(unsupported("abstract method Const carrier needs its complete symbolic formal correspondence",None));}
            },
            _=>return Err(bad("abstract original method changes an ordered formal kind")),
        }}
        let rows=original.method_signatures.as_ref().ok_or_else(||unsupported("abstract original method signature inventory is Unknown",None))?;
        let mut rows=rows.iter().filter(|row|row.member==member);
        let row=rows.next().ok_or_else(||bad("abstract original member omits its signature"))?;
        if rows.next().is_some(){return Err(bad("abstract original member repeats its signature"));}
        let SemanticDefinitionBody::FunctionWithBody{signature,protocol_contracts,foreign,resume_bound,..}=&definition.body else{unreachable!()};
        if !protocol_contracts.is_empty()||foreign.is_some()||*resume_bound!=TaskResumeBound::Local{return Err(unsupported("abstract method protocol/foreign/resume promise needs its complete original provider refinement",None));}
        if signature.bound_lifetimes!=0||row.signature.parameters.len()!=signature.parameters.len()||row.signature.result_mode!=signature.result_mode||row.signature.call_kind!=signature.call_kind||row.signature.invocation_binder_count!=signature.invocation_lifetimes{return Err(bad("abstract member raw and original complete signature domains differ"));}
        for(raw,actual)in row.signature.parameters.iter().zip(&signature.parameters){
            if raw.mode!=actual.mode||!matches!(&raw.ty,DynTypePattern::Semantic{scope,ty}if *scope==definition.id&&ty.as_ref()==&actual.ty){return Err(bad("abstract member parameter changes its original scoped type/mode"));}
        }
        if !matches!(&row.signature.result,DynTypePattern::Semantic{scope,ty}if *scope==definition.id&&ty.as_ref()==&signature.result){return Err(bad("abstract member result changes its original scoped type"));}
        if prototype.parameters.len()!=signature.parameters.len(){return Err(bad("abstract method prototype loses a source parameter"));}
        // The raw contract is still only an upper. This establishes no actual
        // entry, implementation membership, resource effect or loan source.
        Ok(())
    }
}
