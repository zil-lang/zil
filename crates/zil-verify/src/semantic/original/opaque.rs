//! An opaque result keeps its defining identity and ordered actuals. Its
//! hidden term is checked in that definition's domain, never the caller's
//! same-numbered Result, Implicit or local namespace.
use super::*;

/// Select a complete advertised result contract without flattening its
/// quantifier tree. The entire original row is proved once by obligation.
fn result_callable_contract(row:&SemanticRequirementRecipe,b:&mut Budget)->Check<bool>{
    let mut pending=vec![(row,0usize)];
    while let Some((row,depth))=pending.pop(){
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("opaque result requirement depth exceeded"));}
        match row{
            SemanticRequirementRecipe::Callable{ty:SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Result},..}=>return Ok(true),
            SemanticRequirementRecipe::ForAll{requirements,..}=>{b.spend(requirements.len())?;pending.extend(requirements.iter().map(|row|(row,depth+1)));},
            _=>{},
        }
    }Ok(false)
}

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn opaque_type(&mut self, module: &TypedModule, context: &CheckedContext, owner: SemanticDefinitionId, arguments: &[SemanticActualRecipe], hidden: &SemanticTypeRecipe, caller: &Env, active: &mut ActiveQueries, b: &mut Budget) -> Check<Ty> {
        let original = context.definition(module, owner, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &original.body else { return Err(bad("original opaque owner has no complete original function")); };
        if !matches!(&signature.result, SemanticTypeRecipe::Opaque { definition, hidden: declared, .. } if *definition == owner && declared.as_ref() == hidden)
            || !matches!(hidden, SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result })
            || original.parameters.len() != arguments.len() {
            return Err(unsupported("original opaque result lacks its exact defining result role and full ordered arguments", None));
        }
        b.spend(arguments.len())?;
        let mut actuals = Vec::new();
        for (formal, recipe) in original.parameters.iter().zip(arguments) {
            let actual = self.actual(module, context, recipe, caller, active, b)?;
            if !matches!((formal, &actual), (SemanticFormal::Type { .. }, Arg::Type(_)) | (SemanticFormal::Lifetime, Arg::Life(_)) | (SemanticFormal::Const { .. }, Arg::Const(_))) {
                return Err(bad("original opaque argument changes its complete formal kind"));
            }
            actuals.push(actual);
        }
        Ok(Ty::Opaque(owner, actuals))
    }

    pub(super) fn opaque_hidden(&mut self, module: &TypedModule, context: &CheckedContext, owner: SemanticDefinitionId, arguments: &[Arg], active: &mut ActiveQueries, b: &mut Budget) -> Check<Ty> {
        if !self.checked.contains(&owner) {
            if self.active.contains(&owner) { return Err(unsupported("recursive opaque result has no completed original role proof", None)); }
            self.definition(module, context, owner, active, b)?;
        }
        let original = context.definition(module, owner, b)?;
        let SemanticDefinitionBody::FunctionWithBody{signature,..}=&original.body else{return Err(bad("opaque hidden result has no exact original function role"));};
        if !matches!(&signature.result,SemanticTypeRecipe::Opaque{definition,hidden,..}if *definition==owner&&matches!(hidden.as_ref(),SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Result})){
            return Err(bad("opaque hidden result selects another original declaration result role"));
        }
        if original.parameters.len() != arguments.len() { return Err(bad("opaque result omits a complete original argument")); }
        b.spend(arguments.len())?;
        for(formal,actual)in original.parameters.iter().zip(arguments){
            if !matches!((formal,actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("opaque hidden result changes its full original argument order or kind"));}
        }
        let hidden = self.resolved(self.results.get(&owner).ok_or_else(|| bad("opaque owner has no checked result role"))?, b)?;
        if usage::has_inference(&hidden, &self.substitutions, b)? { return Err(bad("opaque owner result remains unresolved after its original body")); }
        let mut selected = self.environment(module, context, owner, active, b)?;
        selected.lexical[0].actuals = self.resolve_arguments(arguments, 0, b)?;
        self.apply_type(&hidden, &selected, b)
    }

    pub(super) fn opaque_callable(&mut self, module: &TypedModule, context: &CheckedContext, owner: SemanticDefinitionId, arguments: &[Arg], active: &mut ActiveQueries, b: &mut Budget) -> Check<Option<Prototype>> {
        // A declared result capability is usable only after all original
        // returns have established it. It never yields TypeEqual or LifeEqual.
        self.opaque_hidden(module, context, owner, arguments, active, b)?;
        let original = context.definition(module, owner, b)?;
        let mut selected = self.environment(module, context, owner, active, b)?;
        selected.lexical[0].actuals = self.resolve_arguments(arguments, 0, b)?;
        let rigid = self.environment(module, context, owner, active, b)?;
        let mut result = None;
        for row in &original.requirements {
            // Select exactly this declaration's Result occurrence even when its
            // hidden value is equal to a parameter with another Callable bound.
            self.callable_premise_row(module, context, row, None, &rigid, &[], false,
                &mut result, active, b)?;
        }
        result.map(|prototype| self.specialize_code_prototype(&prototype, &selected, b)).transpose()
    }

    pub(super) fn check_opaque_result(&mut self, module: &TypedModule, context: &CheckedContext, owner: SemanticDefinitionId, env: &Env, active: &mut ActiveQueries, b: &mut Budget) -> Check<()> {
        let original = context.definition(module, owner, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &original.body else { return Ok(()); };
        if !matches!(&signature.result, SemanticTypeRecipe::Opaque { definition, .. } if *definition == owner) { return Ok(()); }
        let hidden = self.results.get(&owner).ok_or_else(|| bad("opaque result has no original role"))?;
        if usage::has_inference(hidden, &self.substitutions, b)? { return Err(bad("opaque result is not determined by every original return")); }
        for row in &original.requirements {
            if result_callable_contract(row,b)?{
                self.obligation(module, context, row, env, env, active, b)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod structural_tests{
    use super::*;
    #[test]
    fn unused_opaque_provider_checks_zero_and_nonzero_quantified_result_contracts_raw_wire(){
        let limits=Limits::default();
        let(template,plan)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        for wrapper in [None,Some(0),Some(1)]{for hostile in 0..3{
            if hostile==2&&wrapper!=Some(1){continue;}
            let mut module=template.clone();let program=module.semantic.as_mut().unwrap();
            let mut owner=program.definitions.iter().find(|row|row.kind==SemanticDefinitionKind::Function&&matches!(row.parameters.as_slice(),[SemanticFormal::Type{..}])).unwrap().clone();
            owner.id.index=program.definitions.iter().map(|row|row.id.index).max().unwrap()+1;owner.identity.declaration_path.push("quantified_opaque_code".into());let id=owner.id;
            let mut code=SemanticSignatureRecipe{bound_lifetimes:0,invocation_lifetimes:0,parameters:vec![SemanticParameterRecipe{ty:SemanticTypeRecipe::Atom(Type::I32),mode:PassingMode::Move}],result:SemanticTypeRecipe::Atom(Type::I32),result_mode:PassingMode::Move,call_kind:CallKind::Direct};
            if hostile==2{code.parameters[0].ty=SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Static,inner:Box::new(SemanticTypeRecipe::Atom(Type::I32))};}
            let mut wanted=code.clone();if hostile==1{wanted.parameters.clear();}
            if hostile==2{wanted.parameters[0].ty=SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound{depth:0,index:0}),inner:Box::new(SemanticTypeRecipe::Atom(Type::I32))};}
            let row=SemanticRequirementRecipe::Callable{ty:SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Result},kind:CallableKind::FnOnce,signature:Box::new(wanted)};
            owner.requirements=vec![if let Some(lifetimes)=wrapper{SemanticRequirementRecipe::ForAll{lifetimes,requirements:vec![row]}}else{row}];
            let SemanticDefinitionBody::FunctionWithBody{signature,parameters,..}=&mut owner.body else{unreachable!()};
            let pointer=SemanticTypeRecipe::Code{kind:SemanticCodeKind::FunctionPointer,signature:Box::new(code)};
            signature.parameters[0].ty=pointer.clone();parameters[0].ty=Some(pointer);
            signature.result=SemanticTypeRecipe::Opaque{definition:id,arguments:vec![SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0}))],hidden:Box::new(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Result})};
            program.definitions.push(owner);assert!(!program.instances.iter().any(|row|row.definition==id));
            for wire in [false,true]{
                let decoded;let module=if wire{let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;&decoded}else{&module};
                let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(module,&mut b).unwrap();let result=CheckedOriginal::check(module,&context,&mut ActiveQueries::new(),&mut b);
                assert_eq!(result.is_ok(),hostile==0,"uninstantiated result contract wrapper {wrapper:?}, wrong signature/rigid-frame attack {hostile}, wire {wire}");
                if let Ok(mut checked)=result {
                    checked.current=Some(id);
                    let mut active=ActiveQueries::new();
                    let arguments=[Arg::Type(Ty::Atom(Type::I32))];
                    let advertised=checked.opaque_callable(module,&context,id,&arguments,&mut active,&mut b).unwrap().unwrap();
                    assert_eq!(advertised.premise_bounds.iter().map(Vec::len).collect::<Vec<_>>(),
                        if wrapper==Some(1) {vec![1]} else {vec![]});
                    assert_eq!(advertised.parameters,vec![Ty::Atom(Type::I32)]);
                    assert_eq!(advertised.result,Ty::Atom(Type::I32));
                    let applied=checked.freshen_code_prototype(&advertised,None,&mut b).unwrap();
                    let again=checked.freshen_code_prototype(&advertised,None,&mut b).unwrap();
                    assert_eq!(applied.premise_bounds.len(),advertised.premise_bounds.len());
                    if wrapper==Some(1) {
                        assert!(matches!(&applied.premise_bounds[..],[frame] if matches!(frame.as_slice(),[Life::Infer(_)])));
                        assert_ne!(applied.premise_bounds,again.premise_bounds);
                    }
                }
            }
        }}
    }
    #[test]
    fn opaque_structural_obligations_use_exact_original_hidden_role_and_all_actuals_raw_wire(){
        let limits=Limits::default();
        let(mut module,plan)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let program=module.semantic.as_mut().unwrap();
        let mut owner=program.definitions.iter().find(|row|row.kind==SemanticDefinitionKind::Function&&matches!(row.parameters.as_slice(),[SemanticFormal::Type{..}])).unwrap().clone();
        let ordinary=owner.id;
        let caller=program.definitions.iter().find(|row|row.kind==SemanticDefinitionKind::Function&&row.parent.is_none()&&row.parameters.is_empty()).unwrap().id;
        owner.id.index=program.definitions.iter().filter(|row|row.id.module==owner.id.module).map(|row|row.id.index).max().unwrap()+1;
        owner.identity.declaration_path.push("opaque_original_identity".into());
        let id=owner.id;
        owner.parameters=(0..2).map(|ordinal|SemanticFormal::Type{requirements:vec![SemanticRequirementRecipe::Structural{ty:SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal}),bound:DynStructuralBound::Value}]}).collect();
        let opaque=SemanticTypeRecipe::Opaque{definition:id,arguments:(0..2).map(|ordinal|SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal}))).collect(),hidden:Box::new(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Result})};
        let SemanticDefinitionBody::FunctionWithBody{signature,..}=&mut owner.body else{unreachable!()};
        signature.result=opaque;
        // This unused generic original returns its T parameter. U is a real
        // phantom type actual, not permission to erase a borrowed dependency.
        program.definitions.push(owner);
        for wire in [false,true]{
            let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
            let(candidate,_)=if wire{zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap()}else{(module.clone(),plan.clone())};
            let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&candidate,&mut b).unwrap();let mut active=ActiveQueries::new();
            let mut checked=CheckedOriginal::check(&candidate,&context,&mut active,&mut b).unwrap();
            checked.current=Some(caller);let env=checked.prototypes[&caller].environment.clone();
            let scalar=Arg::Type(Ty::Atom(Type::I32));
            let borrowed=Arg::Type(Ty::Address{mutable:false,life:Life::Local(caller,NodeId(8123)),inner:Box::new(Ty::Atom(Type::I32))});
            let constant=Arg::Type(Ty::Address{mutable:false,life:Life::Static,inner:Box::new(Ty::Atom(Type::I32))});
            for(hidden,phantom,closed,transferable,element)in [
                (scalar.clone(),scalar.clone(),true,true,true),
                (Arg::Type(Ty::Atom(Type::Unit)),scalar.clone(),true,true,false),
                (borrowed.clone(),scalar.clone(),false,false,false),
                (scalar.clone(),borrowed,false,false,false),
                (constant.clone(),scalar.clone(),true,false,false),
                (scalar.clone(),constant,true,false,true),
            ]{
                let ty=Ty::Opaque(id,vec![hidden,phantom]);
                assert!(checked.value_type(&candidate,&context,&ty,&env,&mut active,&mut b).unwrap());
                assert!(checked.sync_drop(&candidate,&context,&ty,&env,&mut active,&mut b).unwrap());
                assert_eq!(checked.closed_value(&candidate,&context,&ty,&env,&mut active,&mut b).unwrap(),closed);
                let transfer=checked.builtin_obligation(&candidate,&context,&ty,SemanticBuiltinTrait::Transfer,&env,&mut active,&mut b);
                if transferable{transfer.unwrap();}else{assert_eq!(transfer.unwrap_err().category,zil_base::ResultCategory::Unsupported);}
                assert_eq!(checked.store_element(&candidate,&context,&ty,&env,&mut active,&mut b).unwrap(),element);
                assert!(!checked.is_copy(&candidate,&context,&ty,&env,&mut active,&mut b).unwrap(),"structural Value/StoreElement never supplies Copy");
            }
            assert!(checked.opaque_hidden(&candidate,&context,ordinary,&[scalar.clone()],&mut active,&mut b).unwrap_err().message.contains("another original declaration result role"));
            assert!(checked.opaque_hidden(&candidate,&context,id,&[scalar,Arg::Life(Life::Static)],&mut active,&mut b).unwrap_err().message.contains("argument order or kind"));
        }
    }
}
