//! Erasure transports original types and exact runtime operands independently.
//! Interface selection supplies no provider, retained target or cleanup proof.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_dyn_owned(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,site:&SemanticSite,values:&[Option<CheckedType>],env:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let InstructionKind::MakeDynOwned{interface,vtable,source,..}=instruction.kind else{return Err(bad("original owning erasure selects another actual opcode"));};
        let recipes=self.original.borrow().as_ref().ok_or_else(||bad("erasure lacks its original body proof"))?.intrinsic_recipes(module,caller.definition,site.recipe,env,b)?;
        let [SemanticActualRecipe::Type(target),SemanticActualRecipe::Type(original_source)]=recipes.as_slice() else{return Err(bad("owning erasure loses its complete original Type arguments"));};
        let erased=self.evaluate_type(module,target,env,external,active,b)?;
        if erased.ty!=(Type::Dyn{interface}){return Err(bad("owning erasure selects another complete original interface"));}
        let expected=self.evaluate_type(module,original_source,env,external,active,b)?;
        let source=value_at(values,source)?;
        if !self.compatible(env,source,&expected,b)?{return Err(bad("owning erasure changes its original complete source value"));}
        b.spend(module.dyn_vtables.len())?;let mut tables=module.dyn_vtables.iter().filter(|row|row.id==vtable);let table=tables.next().ok_or_else(||bad("erasure actual provider table is absent"))?;
        if tables.next().is_some()||table.interface!=interface||table.self_type!=source.ty||table.self_life.as_ref()!=Some(&source.life)||table.self_classes.len()!=source.classes.len(){return Err(bad("erasure provider loses its whole actual Self domain"));}
        for(actual,binder)in source.classes.iter().zip(&table.self_classes){let StorageBinder::Fixed(index)=binder else{return Err(bad("erasure Self Context includes an invocation image"));};let image=erased.classes.get(*index as usize).ok_or_else(||bad("erasure Self class has no complete Context image"))?;if !self.same(env,actual,image,b)?{return Err(bad("erasure Self-to-Context transport changes an original class image"));}}
        let raw=crate::dynamic_dispatch::instruction(module,f,instruction,b)?.ok_or_else(||bad("erasure actual kernel has no result"))?;
        let owner=CheckedType{type_cell: crate::semantic::CheckedTypeData::new(Type::OwnedDyn{interface}),..erased};
        let Type::ErrorUnion{error,..}=&raw else{return Err(bad("owning erasure kernel omits its canonical failure sum"));};
        let error=self.closed_value(module,error,b)?;
        let mut result=CheckedType{type_cell: crate::semantic::CheckedTypeData::new(raw),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]};
        for(arm,child)in [(true,error),(false,owner)]{let projection=PlaceProjection::ErrorArm(arm);self.attach_child(env,&mut result,&child,vec![LifeTypeProjection::Place(projection)],Some(projection),b)?;}result.life.binder_count=result.classes.len() as u32;Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_dyn_borrow(&self,module:&TypedModule,f:&TypedFunction,instruction:&Instruction,state:&State,values:&[Option<CheckedType>],scope:ScopeKey,env:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        let raw=crate::dynamic_dispatch::instruction(module,f,instruction,b)?.ok_or_else(||bad("erased borrow kernel has no complete result"))?;
        let Type::Address{mutable,inner}= &raw else{return Err(bad("erased borrowing result has another raw type"));};
        let Type::Dyn{interface}=inner.as_ref() else{return Err(bad("erased borrowing result has no exact interface"));};
        let result=instruction.result.ok_or_else(||bad("erased borrow omits its actual result"))?;
        let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
        let outer=ClassKey::Outer{scope,operand:OperandKey::Value(result),path:vec![],mode};
        let child=match &instruction.kind{
            InstructionKind::BorrowDynOwned{local,..}=>{let owner=state.locals.get(local.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("erased borrowing has no initialized complete owner"))?;let owner=if matches!(owner.ty,Type::Address{..}){self.child_value(module,owner,&[LifeTypeProjection::Pointee],b)?}else{owner.clone()};if owner.ty!=(Type::OwnedDyn{interface:*interface}){return Err(bad("erased borrowing changes its actual owner interface"));}CheckedType{type_cell: crate::semantic::CheckedTypeData::new(Type::Dyn{interface:*interface}),..owner}},
            InstructionKind::MakeDynBorrow{source,vtable,..}=>{
                let source=self.child_value(module,value_at(values,*source)?,&[LifeTypeProjection::Pointee],b)?;
                b.spend(module.dyn_vtables.len().saturating_add(module.dyn_interfaces.len()))?;
                let mut tables=module.dyn_vtables.iter().filter(|row|row.id==*vtable);let table=tables.next().ok_or_else(||bad("borrowed erasure provider table is absent"))?;if tables.next().is_some()||table.interface!=*interface||table.self_type!=source.ty||table.self_life.as_ref()!=Some(&source.life)||table.self_classes.len()!=source.classes.len(){return Err(bad("borrowed erasure loses its full actual provider/Self type"));}
                let shape=&module.dyn_interfaces.iter().find(|row|row.id==*interface).ok_or_else(||bad("borrowed erasure interface is absent"))?.retained_life;
                let mut classes=vec![None;shape.binder_count as usize];let mut origins=vec![None;shape.binder_count as usize];
                for((key,origin),binder)in source.classes.iter().zip(&source.origins).zip(&table.self_classes){let StorageBinder::Fixed(index)=binder else{return Err(bad("borrowed erasure Context includes invocation authority"));};let target=classes.get_mut(*index as usize).ok_or_else(||bad("borrowed erasure class is outside Context"))?;if let Some(previous)=target{if !self.same(env,previous,key,b)?{return Err(bad("borrowed erasure Context changes a full class image"));}}else{*target=Some(key.clone());}let target=&mut origins[*index as usize];if target.is_none(){*target=Some(origin.clone());}}
                let classes=classes.into_iter().collect::<Option<Vec<_>>>().ok_or_else(||bad("borrowed erasure has an unanchored original Context class"))?;let origins=origins.into_iter().collect::<Option<Vec<_>>>().ok_or_else(||bad("borrowed erasure has an unanchored original Context occurrence"))?;
                let life=StorageLifeShape{binder_count:shape.binder_count,slots:vec![],retained:vec![],nominal_arguments:vec![]};let mut child=self.construct_value(module,&Type::Dyn{interface:*interface},&life,classes,b)?;child.origins=origins;child
            },_=>return Err(bad("erased borrowing selects another actual operation")),
        };
        dynamic_witness::semantic_external::construct(self,env,dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(outer)},&[child],active,b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_dyn_call(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,instruction:&Instruction,sites:&[&SemanticSite],values:&[Option<CheckedType>],env:&EvalEnv,external:&mut dynamic_witness::semantic_external::Proofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let InstructionKind::CallDynSlot{callee,interface,slot,arguments}=&instruction.kind else{return Err(bad("dynamic source call selects another actual operation"));};
        let mut claims=sites.iter().filter(|site|matches!(site.action,SemanticSiteAction::BodyCall{..}));let site=claims.next().ok_or_else(||bad("dynamic call lacks its exact original member occurrence"))?;if claims.next().is_some(){return Err(bad("dynamic call has ambiguous original occurrences"));}
        let original=self.original.borrow().as_ref().ok_or_else(||bad("dynamic call lacks its universal original proof"))?.trait_call_recipes(module,caller.definition,site.recipe,env,b)?.ok_or_else(||bad("dynamic call has no independently selected original trait member"))?;
        let receiver=self.evaluate_type(module,&original.receiver,env,external,active,b)?;
        if receiver.ty!=(Type::Dyn{interface:*interface}){return Err(bad("dynamic call changes its original whole receiver interface"));}
        let actual_receiver=value_at(values,*callee)?;let pointee=self.child_value(module,actual_receiver,&[LifeTypeProjection::Pointee],b)?;
        if !self.compatible(env,&pointee,&receiver,b)?{return Err(bad("dynamic call receiver changes complete Context identities"));}
        let application=self.application(module,&original.application,env,external,active,b)?;
        b.spend(module.dyn_interfaces.len())?;let iface=module.dyn_interfaces.iter().find(|row|row.id==*interface).ok_or_else(||bad("dynamic call actual interface is absent"))?;let selected=iface.slots.get(*slot as usize).ok_or_else(||bad("dynamic call selects no exact actual slot"))?;
        if selected.member.ordinal!=original.member||!self.dyn_application_matches(module,&application,&selected.member.application,&receiver.classes,&receiver.origins,env,b)?{return Err(bad("dynamic call selects another complete original trait/member application"));}
        let mut actuals=Vec::new();for recipe in &original.actuals{actuals.push(match recipe{original::MethodActualRecipe::Recipe(recipe)=>self.evaluate_actual(module,recipe,env,external,active,b)?,original::MethodActualRecipe::ReceiverRole(role)=>CheckedActual::Life(self.class(module,role,env,b)?)});}
        let SemanticSiteAction::BodyCall{actuals:source_claims}=&site.action else{unreachable!()};if source_claims.len()!=actuals.len()||selected.actuals.len()!=actuals.len(){return Err(bad("dynamic call loses its complete mixed method actual vector"));}
        for(value,claim)in actuals.iter().zip(source_claims){self.actual_claim(module,claim,value,env,b)?;}
        let programme=module.semantic.as_ref().ok_or_else(||bad("dynamic call lacks its original programme"))?;b.spend(programme.dyn_scopes.len())?;let mut methods=programme.dyn_scopes.iter().filter(|row|row.owner==SemanticDynOwner::MethodSignature{declaration:application.declaration,member:original.member});let method=methods.next().ok_or_else(||bad("dynamic member lacks its exact original declaration scope"))?;if methods.next().is_some(){return Err(bad("dynamic member declaration scope is ambiguous"));}
        let mut selected_env=self.dynamic_member_environment(module,method.scope,&application,&receiver,&actuals,env,active,b)?;
        let definition=self.definition(module,method.scope,b)?;let (SemanticDefinitionBody::FunctionWithBody{signature,..}|SemanticDefinitionBody::Function{signature,..})=&definition.body else{return Err(bad("dynamic member original scope has no complete signature"));};
        let signature=self.evaluate_signature(module,signature,&selected_env,external,active,b)?;
        selected_env.invocations=signature.invocation.clone();
        let mut boundary=trait_members::Boundary::default();
        for(value,recorded)in actuals.iter().zip(&selected.actuals){let checked=match recorded{DynGenericActual::Life(StorageBinder::Invocation(index))=>CheckedActual::Life(signature.invocation.get(*index as usize).ok_or_else(||bad("slot method Life escapes its invocation domain"))?.clone()),DynGenericActual::Life(StorageBinder::Fixed(index))=>CheckedActual::Life(receiver.classes.get(*index as usize).ok_or_else(||bad("slot method Life escapes its complete Context"))?.clone()),DynGenericActual::Type(ty)=>CheckedActual::Type(self.construct_value(module,ty,&empty_life(),vec![],b)?),DynGenericActual::QualifiedType(value)=>self.dyn_method_actual(module,value,&receiver.classes,&receiver.origins,&signature.invocation,b).map(CheckedActual::Type)?,DynGenericActual::Const(value)=>CheckedActual::Const{carrier:atomic(&value.ty(),b)?,value:value.clone()}};boundary.actual(self,env,&checked,value,b)?;}
        for(value,_)in &signature.parameters{dynamic_metadata::ty(&value.ty,b)?;}
        dynamic_metadata::ty(&signature.result.ty,b)?;
        if signature.parameters.len()!=arguments.len()+1||signature.parameters[0].1!=selected.receiver.mode||signature.parameters.iter().skip(1).map(|(_,mode)|*mode).collect::<Vec<_>>()!=selected.parameter_modes||signature.result_mode!=selected.result_mode||signature.call_kind!=selected.call_kind||signature.result.ty!=selected.result||signature.parameters.iter().skip(1).map(|(value,_)|&value.ty).ne(selected.parameters.iter()){return Err(bad("dynamic slot whole ABI differs from its exact original signature"));}
        for((expected,_),actual)in signature.parameters.iter().zip(std::iter::once(callee).chain(arguments)){boundary.value(self,env,expected,value_at(values,*actual)?,b)?;}
        self.evaluate_requirements(module,&definition.requirements,&selected_env,RequirementMode::ActualObligation,external,active,b)?;
        for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{self.evaluate_requirements(module,requirements,&selected_env,RequirementMode::ActualObligation,external,active,b)?;}}
        let result=boundary.project_type(self,env,&signature.result,b)?;
        let raw=crate::dynamic_dispatch::instruction(module,f,instruction,b)?;if raw.as_ref()!=if result.ty==Type::Unit{None}else{Some(&result.ty)}{return Err(bad("dynamic call kernel result differs from original full result"));}
        Ok((result.ty!=Type::Unit).then_some(result))
    }
    fn dyn_method_actual(&self,module:&TypedModule,value:&DynTypeValue,fixed:&[ClassKey],fixed_origins:&[IntrinsicOrigin],invocation:&[ClassKey],b:&mut Budget)->Check<CheckedType>{let mut classes=Vec::new();let mut origins=Vec::new();let mut seen=std::collections::BTreeSet::new();for binder in &value.classes{b.spend(1)?;if !seen.insert(*binder){return Err(bad("dynamic method actual coalesces independent full intrinsic classes"));}let key=match binder{StorageBinder::Fixed(index)=>fixed.get(*index as usize),StorageBinder::Invocation(index)=>invocation.get(*index as usize)}.ok_or_else(||bad("dynamic method actual has an absent full class"))?;let origin=match binder{StorageBinder::Fixed(index)=>fixed_origins.get(*index as usize).ok_or_else(||bad("dynamic method actual loses a complete Context occurrence"))?.clone(),StorageBinder::Invocation(_)=>IntrinsicOrigin::DerivedClass{source:key.clone()}};classes.push(key.clone());origins.push(origin);}let mut result=self.construct_value(module,&value.ty,&value.life,classes,b)?;result.origins=origins;Ok(result)}
    #[allow(clippy::too_many_arguments)]
    fn dynamic_member_environment(&self,module:&TypedModule,method:SemanticDefinitionId,application:&CheckedApplication,receiver:&CheckedType,actuals:&[CheckedActual],caller:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<EvalEnv>{
        let scope=dynamic_witness::semantic_external::original_scope(module,application.declaration,b)?;
        let definition=self.definition(module,method,b)?;if definition.parent!=Some(scope.id)||definition.parameters.len()!=actuals.len(){return Err(bad("dynamic method changes its exact original parent/formal arity"));}
        let token=active.fresh(b)?;let mut lexical=Vec::new();let mut next=Some(method);
        while let Some(id)=next{b.spend(1)?;if lexical.len()>=b.limits.max_call_depth.min(128){return Err(limit("dynamic member lexical parent depth exceeded"));}let definition=self.definition(module,id,b)?;let frame=if id==method||id==scope.id{LexicalFrame{definition:id,key:ScopeKey::Witness{definition:id,frame:token},actuals:if id==method{actuals.to_vec()}else{application.arguments.clone()},environment:vec![],self_type:Some(receiver.clone())}}else if let Some(frame)=caller.lexical.iter().find(|row|row.definition==id){frame.clone()}else{if !definition.parameters.is_empty()||!definition.environment.is_empty(){return Err(bad("dynamic method omits a complete original lexical parent application"));}LexicalFrame{definition:id,key:ScopeKey::Witness{definition:id,frame:token},actuals:vec![],environment:vec![],self_type:None}};if !definition.environment.is_empty(){return Err(bad("dynamic method has an unsealed original environment"));}lexical.push(frame);next=definition.parent;}
        Ok(EvalEnv{lexical,assumptions:caller.assumptions.clone(),images:caller.images.clone(),bounds:caller.bounds.clone(),invocations:vec![],at:caller.at})
    }
}
