//! Environment-free code is formed from an exact original static definition.
//! A later indirect call uses the complete original code scheme, never a
//! first same-signature body or a target inferred from machine pointer types.
use super::*;
impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn function_pointer(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let InstructionKind::FunctionRef{function}=instruction.kind else{return Err(bad("ordinary code formation selects another actual operation"));};
        crate::check_instruction(module,f,instruction,b)?;
        let result=instruction.result.ok_or_else(||bad("ordinary code reference omits its actual SSA result"))?;
        let program=module.semantic.as_ref().ok_or_else(||bad("ordinary code reference has no original source programme"))?;
        let caller_definition=self.definition(module,caller.definition,b)?;let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&caller_definition.body else{return Err(bad("ordinary code reference creator has no complete original body"));};
        let mut source=None;
        for site in program.sites.iter().filter(|site|site.instance==caller.id&&matches!(site.action,SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(value),..}if value==result)){
            b.spend(site_nodes.len())?;
            let node=site_nodes.iter().find(|row|row.site==site.recipe).ok_or_else(||bad("code reference use has no original node link"))?;
            let SemanticOriginalNode::Expression(node)=node.node else{return Err(bad("code reference use does not select an original expression"));};
            let target=self.original.borrow().as_ref().ok_or_else(||bad("code reference has no universal original proof"))?.code_source(caller.definition,node,b)?.ok_or_else(||bad("code reference has no exact original static definition coercion"))?;
            if source.replace(target).is_some_and(|old|old!=target){return Err(bad("one actual code reference substitutes several original targets"));}
        }
        let source=source.ok_or_else(||bad("actual code reference omits its exact original formation occurrence"))?;
        let definition=self.definition(module,source,b)?;
        if definition.kind!=SemanticDefinitionKind::Function||definition.parent.is_some()||!definition.parameters.is_empty()||!definition.environment.is_empty(){return Err(bad("ordinary code reference changes its monomorphic static original owner"));}
        let SemanticDefinitionBody::FunctionWithBody{signature,foreign:None,body:SemanticOriginalFunctionBody::Defined(_),..}=&definition.body else{return Err(bad("ordinary code reference changes its original body or ABI role"));};
        let key=ScopeKey::Witness{definition:source,frame:active.fresh(b)?};
        let selected=EvalEnv{lexical:vec![LexicalFrame{definition:source,key,actuals:vec![],environment:vec![],self_type:None}],assumptions:environment.assumptions.clone(),images:environment.images.clone(),bounds:vec![],invocations:vec![],at:environment.at};
        let worlds=self.call_worlds(module,function,source,&selected,environment,active,b)?;
        self.evaluate_requirements(module,&definition.requirements,&selected,RequirementMode::ActualObligation,external,active,b)?;
        let signature=self.world_signature(module,signature,worlds,environment,external,active,b)?;
        let value=self.ordinary_code(module,SemanticCodeKind::FunctionPointer,&signature,b)?;
        if f.values.get(result.0 as usize)!=Some(&value.ty){return Err(bad("actual code reference changes the complete original code type"));}Ok(value)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn indirect_call(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,callee:&CheckedType,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let recipe=self.original.borrow().as_ref().ok_or_else(||bad("indirect code call has no universal original body proof"))?.indirect_signature_recipe(module,caller.definition,site.recipe,environment,b)?;
        let mut signature=self.evaluate_signature(module,&recipe,environment,external,active,b)?;
        let expected=self.ordinary_code(module,SemanticCodeKind::FunctionPointer,&signature,b)?;
        if !self.same_type(environment,callee,&expected,b)?{return Err(bad("indirect code call changes its complete original code scheme"));}
        let key=ScopeKey::Witness{definition:caller.definition,frame:active.fresh(b)?};
        let(source_inputs,source_checks)=self.original_call_inputs(module,caller,site,&signature,key,arguments,values,environment,external,active,b)?;
        let inputs=source_inputs.iter().collect::<Vec<_>>();
        self.instantiate_call_heads(&mut signature,&inputs,key,environment,b)?;
        for(index,((expected,_),argument))in signature.parameters.iter().zip(arguments).enumerate(){if !self.call_argument(module,caller,*argument,values,expected,environment,source_checks[index],b)?{return Err(bad("indirect code argument changes its complete original type/class/source role"));}}
        Ok((signature.result.ty!=Type::Unit).then_some(signature.result))
    }
}
