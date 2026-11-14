use super::*;
impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_c_callback(&self,module:&TypedModule,instruction:&Instruction,source:SemanticDefinitionId,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let InstructionKind::ForeignFunctionRef{function}=instruction.kind else{return Err(bad("c_callback selects another actual reference operation"));};
        let definition=self.definition(module,source,b)?;
        if definition.kind!=SemanticDefinitionKind::Function||definition.parent.is_some()||!definition.parameters.is_empty()||!definition.environment.is_empty(){return Err(bad("C reference target is not its original root monomorphic static definition"));}
        let SemanticDefinitionBody::FunctionWithBody{signature,body:SemanticOriginalFunctionBody::Defined(_),foreign:None,receiver_parameter:None,..}=&definition.body else{return Err(bad("C reference target has a different original body or ABI role"));};
        // A static function has no parent-instance actuals to select. Every
        // compatible exact definition/target world must still be checked.
        let key=ScopeKey::Witness{definition:source,frame:active.fresh(b)?};
        let expected=EvalEnv{lexical:vec![LexicalFrame{definition:source,key,actuals:vec![],environment:vec![],self_type:None}],assumptions:environment.assumptions.clone(),images:environment.images.clone(),bounds:vec![],invocations:vec![],at:environment.at};
        self.call_worlds(module,function,source,&expected,environment,active,b)?;
        // Selecting a code reference cannot assume its target's requirements.
        self.evaluate_requirements(module,&definition.requirements,&expected,RequirementMode::ActualObligation,external,active,b)?;
        // This non-generic, nonquantified numeric ABI has no free domain to
        // substitute. All selected worlds still undergo their actual body pass.
        let signature=self.evaluate_signature(module,signature,&expected,external,active,b)?;
        let value=self.foreign_callback_code(module,&signature,b)?;
        let actual=crate::native_values::function_ref(module,function,Some(instruction.span),b)?;
        if actual!=value.ty{return Err(bad("C reference complete actual signature differs from its original static definition"));}
        Ok(value)
    }
}
