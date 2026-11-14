//! The native thread instruction must select the exact original worker and
//! parent closure occurrence. The mature kernel separately proves actual
//! remote ABI, capture storage, manager access and synchronous completion.
use super::*;
impl CheckedContext {
    pub(super) fn actual_scoped_thread(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,site:&SemanticSite,values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let InstructionKind::WithScopedThread{worker,argument,body}=instruction.kind else{return Err(bad("thread original scheme selects another actual opcode"));};
        let(target,input_recipe,body_recipe)=self.original.borrow().as_ref().ok_or_else(||bad("thread scope has no universal original proof"))?.scoped_thread_sources(caller.definition,site.recipe,b)?;
        let original=self.definition(module,target,b)?;
        if original.parent.is_some()||!original.parameters.is_empty()||!original.environment.is_empty(){return Err(bad("thread actual worker changes its full static original scheme"));}
        let SemanticDefinitionBody::FunctionWithBody{signature,..}=&original.body else{return Err(bad("thread original worker signature is absent"));};
        let token=active.fresh(b)?;let key=ScopeKey::Witness{definition:target,frame:token};
        let selected=EvalEnv{lexical:vec![LexicalFrame{definition:target,key,actuals:vec![],environment:vec![],self_type:None}],assumptions:environment.assumptions.clone(),images:environment.images.clone(),bounds:vec![],invocations:vec![],at:environment.at};
        let worlds=self.call_worlds(module,worker,target,&selected,environment,active,b)?;
        self.evaluate_requirements(module,&original.requirements,&selected,RequirementMode::ActualObligation,external,active,b)?;
        // call_worlds/entry requests use the same actual proof session. No
        // static matching ABI or first Typed world authenticates this target.
        let mut signature=self.world_signature(module,signature,worlds,environment,external,active,b)?;
        let input=value_at(values,argument)?;
        let expected_input=self.evaluate_type(module,&input_recipe,environment,external,active,b)?;
        if !self.same_type(environment,input,&expected_input,b)?{return Err(bad("thread actual input differs from its original whole source occurrence"));}
        self.instantiate_call_heads(&mut signature,&[input],key,environment,b)?;
        if signature.parameters.len()!=1||!self.same_type(environment,input,&signature.parameters[0].0,b)?||signature.result.ty!=Type::I32||!signature.result.classes.is_empty()||!signature.result.statics.is_empty(){return Err(bad("thread actual worker has another complete original payload/result domain"));}
        let parent=value_at(values,body)?;
        let expected_body=self.evaluate_type(module,&body_recipe,environment,external,active,b)?;
        if !self.same_type(environment,parent,&expected_body,b)?{return Err(bad("thread parent closure differs from its exact original creator/value domain"));}
        let Type::Closure(closure)=parent.ty else{return Err(bad("thread parent body is not its complete actual closure"));};
        let definition=crate::closures::definition(module,closure,Some(instruction.span))?;
        if !definition.parameters.is_empty()||definition.result!=Type::I32{return Err(bad("thread actual parent body has another complete consuming signature"));}
        let result=crate::scoped_thread::instruction(module,f,instruction,b)?.ok_or_else(||bad("thread instruction has no independent actual result type"))?;
        let expected=Type::Tuple(vec![Type::Bool,Type::I32,Type::I32]);
        dynamic_metadata::ty(&result,b)?;
        if result!=expected{return Err(bad("thread scalar/manager role changes its complete result carrier"));}
        self.closed_value(module,&result,b)
    }
}
