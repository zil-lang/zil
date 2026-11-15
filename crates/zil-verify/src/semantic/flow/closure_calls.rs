//! Concrete closure invocations retain their exact entry worlds, complete
//! creator environments and independent parameter/result intrinsic domains.
//! Borrowing an environment supplies no permission beyond the normal kernels.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn closure_call(&self,module:&TypedModule,caller:&SemanticInstance,id:ClosureId,receiver:&CheckedType,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let closure=crate::closures::definition(module,id,None)?;
        let entry=module.functions.get(closure.function.0 as usize).filter(|row|row.id==closure.function&&row.environment==Some(id)).ok_or_else(||bad("closure call has no exact actual environment entry"))?;
        let header=closure.storage_life.as_ref().ok_or_else(||bad("closure call actual complete environment header is Unknown"))?;
        if entry.storage_life.as_ref()!=Some(header)||receiver.ty!=Type::Closure(id)||receiver.classes.len()!=header.fixed_binder_count as usize||receiver.origins.len()!=receiver.classes.len(){return Err(bad("closure call changes its complete actual receiver domain"));}
        signature_life::closure_relation_resource_scopes(module,&closure.parameters,&closure.captures,closure.kind,crate::stored_callables::life_result(closure),header,None,b)?;
        ghost_captures::complete_environment(module,receiver,b)?;
        let programme=module.semantic.as_ref().ok_or_else(||bad("closure call has no original entry world directory"))?;
        b.spend(programme.instances.len().saturating_add(arguments.len()))?;
        let inputs=arguments.iter().map(|argument|value_at(values,*argument)).collect::<Check<Vec<_>>>()?;
        let mut result:Option<CheckedType>=None;let mut worlds=0usize;
        for instance in programme.instances.iter().filter(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==closure.function)){
            let definition=self.definition(module,instance.definition,b)?;
            if definition.kind!=SemanticDefinitionKind::Closure||instance.environment.as_ref().is_none_or(|row|row.closure!=SemanticClosureTarget::Typed(id)){return Err(bad("closure call actual entry world changes its exact original closure identity"));}
            let SemanticDefinitionBody::FunctionWithBody{signature:recipe,..}=&definition.body else{return Err(bad("closure call original entry omits its complete body/signature"));};
            let world=if self.seals.borrow().contains_key(&instance.id){self.environment_for_instance(module,instance.id,active,b)?}else{
                // Calls may precede a generic factory's actual body pass.
                // Interpret only its original environment sources here; this
                // cursor remains pending until the real creator and body pass.
                let parent=instance.parent.ok_or_else(||bad("closure call world has no exact original creator parent"))?;
                let parent=self.environment_for_instance(module,parent,active,b)?;
                let mut sealed=Vec::new();for formal in &definition.environment{
                    let value=self.evaluate_actual(module,&formal.source,&parent,external,active,b)?;
                    if !matches!((&formal.kind,&value),(SemanticFormal::Type{..},CheckedActual::Type(_))|(SemanticFormal::Lifetime,CheckedActual::Life(_))|(SemanticFormal::Const{..},CheckedActual::Const{..})){return Err(bad("closure call provisional environment changes an original formal kind"));}sealed.push(value);
                }
                let world=self.environment_for_instance_with_seal(module,instance.id,Some(&sealed),active,b)?;
                self.register_closure_cursor(instance.id,&parent,&world.lexical[0].actuals,&sealed,receiver,b)?;world
            };
            self.evaluate_requirements(module,&definition.requirements,&world,RequirementMode::DeclarationPremise,external,active,b)?;
            for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{self.evaluate_requirements(module,requirements,&world,RequirementMode::DeclarationPremise,external,active,b)?;}}
            let mut signature=self.evaluate_signature(module,recipe,&world,external,active,b)?;
            if signature.parameters.len()!=arguments.len()||signature.parameters.iter().map(|(value,_)|&value.ty).ne(closure.parameters.iter())||signature.result.ty!=closure.result||signature.result.ty!=entry.return_type||signature.result_mode!=entry.result_mode||signature.call_kind!=entry.call_kind{return Err(bad("closure call world changes its complete original parameter/result ABI"));}
            let call_key=ScopeKey::Witness{definition:definition.id,frame:active.fresh(b)?};
            let mut boundary=trait_members::Boundary::default();boundary.scope(ScopeKey::Instance(instance.id),call_key,b)?;
            let mut free=Vec::new();
            for class in 0..header.fixed_binder_count{
                b.spend(instance.header_embedding.len())?;
                let mut rows=instance.header_embedding.iter().filter(|row|row.header==StorageBinder::Fixed(class));
                let row=rows.next().ok_or_else(||bad("closure call loses an actual fixed intrinsic occurrence"))?;
                if rows.next().is_some(){return Err(bad("closure call duplicates a fixed intrinsic occurrence"));}
                let key=self.class(module,&row.actual,&world,b)?;free.push(key.clone());
                boundary.class(self,environment,&key,&receiver.classes[class as usize],b)?;
            }
            let owner=CheckedType{ty:Type::Closure(id),life:StorageLifeShape{binder_count:header.fixed_binder_count,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:free,origins:(0..header.fixed_binder_count).map(|class|IntrinsicOrigin::Existing{scope:ScopeKey::Instance(instance.id),operand:OperandKey::Receiver,path:vec![],class}).collect(),statics:receiver.statics.clone()};
            boundary.value(self,environment,&owner,receiver,b)?;
            for(parameter,_)in &mut signature.parameters{*parameter=boundary.project_type(self,environment,parameter,b)?;}
            signature.result=boundary.project_type(self,environment,&signature.result,b)?;
            self.instantiate_call_heads(&mut signature,&inputs,call_key,environment,b)?;
            for((expected,_),argument)in signature.parameters.iter().zip(arguments){if !self.call_argument(module,caller,*argument,values,expected,environment,b)?{return Err(bad("closure call argument changes a complete original world/borrow/child occurrence"));}}
            if let Some(expected)=&result{if !self.same_type(environment,expected,&signature.result,b)?{return Err(bad("closure entry worlds disagree on the complete actual result domain"));}}else{result=Some(signature.result);}
            worlds+=1;
        }
        if worlds==0{return Err(bad("closure call has no complete independently checked original entry world"));}
        let result=result.ok_or_else(||bad("closure call omits its complete result domain"))?;
        Ok((result.ty!=Type::Unit).then_some(result))
    }
}
