//! Concrete closure invocations retain their exact entry worlds, complete
//! creator environments and independent parameter/result intrinsic domains.
//! Borrowing an environment supplies no permission beyond the normal kernels.
use super::*;
fn cursor_actual_work(value:&CheckedActual,b:&mut Budget)->Check<()>{
    match value{
        CheckedActual::Type(value)=>value.charge_clone(b),
        CheckedActual::Const{carrier,value}=>{carrier.charge_clone(b)?;dynamic_metadata::constant(value,b)},
        CheckedActual::Life(key)=>cursor_key_work(key,b),
    }
}
fn cursor_key_work(key:&ClassKey,b:&mut Budget)->Check<()>{b.spend(1)?;match key{ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=>b.spend(path.len()),_=>Ok(())}}
fn cursor_environment_work(env:&EvalEnv,b:&mut Budget)->Check<()>{
    b.spend(env.lexical.len().saturating_add(env.assumptions.len()).saturating_add(env.images.len()).saturating_add(env.bounds.len()))?;
    for frame in &env.lexical{for value in frame.actuals.iter().chain(&frame.environment){cursor_actual_work(value,b)?;}if let Some(value)=&frame.self_type{value.charge_clone(b)?;}}
    for(a,z)in &env.images{cursor_key_work(a,b)?;cursor_key_work(z,b)?;}
    for key in env.bounds.iter().flat_map(|frame|&frame.classes).chain(&env.invocations){cursor_key_work(key,b)?;}
    Ok(())
}
impl CheckedContext {
    pub(in crate::semantic) fn finish_closure_cursor(&self,module:&TypedModule,cursor:&ClosureCursor,actual:&EvalEnv,b:&mut Budget)->Check<()>{
        let instance=self.instance(module,cursor.instance,b)?;
        let definition=self.definition(module,instance.definition,b)?;
        if actual.lexical.first().is_none_or(|row|row.key!=ScopeKey::Instance(instance.id)||row.definition!=definition.id){return Err(bad("pending closure cursor selects another completed original world"));}
        cursor_environment_work(&cursor.environment,b)?;
        for value in cursor.arguments.iter().chain(&cursor.sealed){cursor_actual_work(value,b)?;}
        let mut wanted=cursor.environment.clone();
        if let Some(parent)=definition.parent{
            let at=wanted.lexical.iter().position(|row|row.definition==parent).ok_or_else(||bad("pending closure cursor omits its exact original lexical parent"))?;
            wanted.lexical.drain(..at);
        }else{wanted.lexical.clear();}
        wanted.lexical.insert(0,LexicalFrame{definition:definition.id,key:ScopeKey::Instance(instance.id),actuals:cursor.arguments.clone(),environment:cursor.sealed.clone(),self_type:None});
        if actual.lexical.len()!=wanted.lexical.len(){return Err(bad("pending closure cursor changes its complete lexical ancestry"));}
        let mut boundary=trait_members::Boundary::default();
        for(a,z)in actual.lexical.iter().zip(&wanted.lexical){
            b.spend(a.actuals.len().saturating_add(a.environment.len()).saturating_add(1))?;
            if a.definition!=z.definition||a.actuals.len()!=z.actuals.len()||a.environment.len()!=z.environment.len(){return Err(bad("pending closure cursor changes its complete original mixed frames"));}
            boundary.scope(a.key,z.key,b)?;
            for(a,z)in a.actuals.iter().zip(&z.actuals).chain(a.environment.iter().zip(&z.environment)){boundary.actual(self,&wanted,a,z,b)?;}
            match(&a.self_type,&z.self_type){(Some(a),Some(z))=>boundary.value(self,&wanted,a,z,b)?,(None,None)=>{},_=>return Err(bad("pending closure cursor changes its actual Self scope"))}
        }
        let constructors=self.closure_boundaries.borrow();b.spend(constructors.len())?;
        let mut found=false;
        for constructor in constructors.iter().filter(|row|row.child==instance.id){
            let projected=boundary.project_type(self,&wanted,&constructor.value,b)?;
            if !self.same_type(&wanted,&projected,&cursor.expected,b)?{return Err(bad("pending closure cursor changes its fully sealed constructor domain or outside image"));}
            found=true;
        }
        if !found{return Err(bad("pending closure cursor has no complete actual constructor boundary"));}
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn closure_call(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,id:ClosureId,receiver:&CheckedType,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let closure=crate::closures::definition(module,id,None)?;
        let entry=module.functions.get(closure.function.0 as usize).filter(|row|row.id==closure.function&&row.environment==Some(id)).ok_or_else(||bad("closure call has no exact actual environment entry"))?;
        let header=closure.storage_life.as_ref().ok_or_else(||bad("closure call actual complete environment header is Unknown"))?;
        if entry.storage_life.as_ref()!=Some(header)||receiver.ty!=Type::Closure(id)||receiver.classes.len()!=header.fixed_binder_count as usize||receiver.origins.len()!=receiver.classes.len(){return Err(bad("closure call changes its complete actual receiver domain"));}
        signature_life::closure_relation_resource_scopes(module,&closure.parameters,&closure.captures,closure.kind,crate::stored_callables::life_result(closure),header,None,b)?;
        ghost_captures::complete_environment(module,receiver,b)?;
        let programme=module.semantic.as_ref().ok_or_else(||bad("closure call has no original entry world directory"))?;
        b.spend(programme.instances.len().saturating_add(arguments.len()))?;
        let mut result:Option<CheckedType>=None;let mut worlds=0usize;
        for instance in programme.instances.iter().filter(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))if function==closure.function)){
            let definition=self.definition(module,instance.definition,b)?;
            if definition.kind!=SemanticDefinitionKind::Closure||instance.environment.as_ref().is_none_or(|row|row.closure!=SemanticClosureTarget::Typed(id)){return Err(bad("closure call actual entry world changes its exact original closure identity"));}
            let SemanticDefinitionBody::FunctionWithBody{signature:recipe,..}=&definition.body else{return Err(bad("closure call original entry omits its complete body/signature"));};
            let mut pending_parent=None;
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
                pending_parent=Some(parent);world
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
            let owner=CheckedType{type_cell: crate::semantic::CheckedTypeData::new(Type::Closure(id)),life:StorageLifeShape{binder_count:header.fixed_binder_count,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:free,origins:(0..header.fixed_binder_count).map(|class|IntrinsicOrigin::Existing{scope:ScopeKey::Instance(instance.id),operand:OperandKey::Receiver,path:vec![],class}).collect(),statics:receiver.statics.clone()};
            if let Some(parent)=pending_parent{
                // The pending creator is expressed in this actual entry
                // world's complete parent domain. The caller receiver is a
                // separate outside image, checked by the joint boundary below.
                self.register_closure_cursor(instance.id,&parent,&world.lexical[0].actuals,&world.lexical[0].environment,&owner,b)?;
            }
            boundary.value(self,environment,&owner,receiver,b)?;
            for(parameter,_)in &mut signature.parameters{*parameter=boundary.project_type(self,environment,parameter,b)?;}
            signature.result=boundary.project_type(self,environment,&signature.result,b)?;
            let(source_inputs,source_checks)=self.original_call_inputs(module,caller,site,&signature,call_key,arguments,values,environment,external,active,b)?;
            let inputs=source_inputs.iter().collect::<Vec<_>>();
            self.instantiate_call_heads(&mut signature,&inputs,call_key,environment,b)?;
            for(index,((expected,_),argument))in signature.parameters.iter().zip(arguments).enumerate(){if !self.call_argument(module,caller,*argument,values,expected,environment,source_checks[index],b)?{return Err(bad("closure call argument changes a complete original world/borrow/child occurrence"));}}
            if let Some(expected)=&result{if !self.same_type(environment,expected,&signature.result,b)?{return Err(bad("closure entry worlds disagree on the complete actual result domain"));}}else{result=Some(signature.result);}
            worlds+=1;
        }
        if worlds==0{return Err(bad("closure call has no complete independently checked original entry world"));}
        let result=result.ok_or_else(||bad("closure call omits its complete result domain"))?;
        Ok((result.ty!=Type::Unit).then_some(result))
    }
}
