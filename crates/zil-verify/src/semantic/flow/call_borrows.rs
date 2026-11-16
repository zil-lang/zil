//! A compiler-created argument reborrow adapts this outer parameter role.
//! It does not equate two SSA heads, retag a pointee class, or change the
//! caller's original lifetime actual/result image.
use super::*;
impl CheckedContext {
    /// Invocation images come from the exact checked original operands. An
    /// extra compiler argument reborrow is an input role, not the lifetime of
    /// an opaque/nominal result that retains the original caller occurrence.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn original_call_inputs(&self,module:&TypedModule,caller:&SemanticInstance,site:&SemanticSite,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Vec<CheckedType>>{
        let recipes=self.original.borrow().as_ref().ok_or_else(||bad("Call inputs have no universal original body proof"))?.call_input_recipes(module,caller.definition,site.recipe,b)?;
        if recipes.len()!=arguments.len(){return Err(bad("actual Call inputs change the exact original argument arity"));}
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=caller.target else{return Err(bad("original Call inputs have no actual caller function"));};
        let f=module.functions.get(function.0 as usize).filter(|row|row.id==function).ok_or_else(||bad("original Call input actual caller is absent"))?;
        b.spend(arguments.len())?;
        let mut checked=Vec::with_capacity(arguments.len());
        for((node,recipe,origin),argument)in recipes.iter().zip(arguments){
            let source=self.evaluate_type(module,recipe,environment,external,active,b)?;
            if matches!(source.ty,Type::Address{..}|Type::Desc(_))&&source.life.binder_count!=0{
                self.call_input_source(module,caller,f,*argument,*node,*origin,b)?;
            }
            if !self.call_argument(module,caller,*argument,values,&source,environment,b)?{
                return Err(bad("actual Call input changes its original whole operand or exact argument reborrow role"));
            }
            checked.push(source);
        }
        Ok(checked)
    }
    pub(super) fn instantiate_call_heads(&self,signature:&mut CheckedSignature,arguments:&[&CheckedType],key:ScopeKey,environment:&EvalEnv,b:&mut Budget)->Check<()>{
        if arguments.len()!=signature.parameters.len(){return Err(bad("Call head substitution changes its actual arity"));}
        let mut invocation=Vec::<(ClassKey,ClassKey)>::new();
        let mut static_heads=Vec::<ClassKey>::new();
        for(index,((expected,_),actual))in signature.parameters.iter().zip(arguments).enumerate(){
            for slot in &expected.life.slots{
                b.spend(1)?;if !slot.path.is_empty(){continue;}
                let source=expected.classes.get(slot.binder as usize).ok_or_else(||bad("Call parameter head class is absent"))?;
                let fresh=signature.invocation.contains(source)||signature.bound.contains(source)||matches!(source,ClassKey::Implicit{scope,..}if *scope==key)
                    ||matches!(source,ClassKey::Outer{scope,operand:OperandKey::Parameter(ordinal),path,mode}if *scope==key&&*ordinal==index as u32&&path.is_empty()&&*mode==slot.mode);
                if !fresh{continue;}
                let mut slots=actual.life.slots.iter().filter(|row|row.path.is_empty()&&row.mode==slot.mode);
                let Some(actual_slot)=slots.next()else{
                    let mut statics=actual.statics.iter().filter(|row|matches!(row,StaticOccurrence::Direct{path,leaf,mode:StoredBorrowMode::SharedAddress}if path.is_empty()&&leaf==&actual.ty));
                    if slot.mode!=StoredBorrowMode::SharedAddress||actual.ty!=expected.ty||statics.next().is_none()||statics.next().is_some(){return Err(unsupported("Call fresh input head needs its complete actual static/borrow role",None));}
                    if invocation.iter().any(|(key,_)|key==source){return Err(bad("Call repeated invocation head mixes static and variable actual roles"));}
                    if !static_heads.contains(source){static_heads.push(source.clone());}continue;
                };
                if static_heads.contains(source){return Err(bad("Call repeated invocation head mixes variable and static actual roles"));}
                if slots.next().is_some(){return Err(bad("Call argument has duplicate head occurrences"));}
                let image=actual.classes.get(actual_slot.binder as usize).ok_or_else(||bad("Call actual head class is absent"))?;
                b.spend(invocation.len())?;
                if let Some((_,previous))=invocation.iter().find(|(key,_)|key==source){if !self.same(environment,previous,image,b)?{return Err(bad("Call violates a repeated original invocation relation"));}}
                else{invocation.push((source.clone(),image.clone()));}
            }
        }
        let mut boundary=trait_members::Boundary::default();for(source,image)in invocation{boundary.class(self,environment,&source,&image,b)?;}
        for(parameter,_)in &mut signature.parameters{*parameter=boundary.project_type(self,environment,parameter,b)?;}
        signature.result=boundary.project_type(self,environment,&signature.result,b)?;
        for source in static_heads{
            for(parameter,_)in &mut signature.parameters{static_call_head(parameter,&source,b)?;}
            static_call_head(&mut signature.result,&source,b)?;
            signature.invocation.retain(|key|key!=&source);signature.bound.retain(|key|key!=&source);
        }Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_argument(&self,module:&TypedModule,caller:&SemanticInstance,argument:ValueId,values:&[Option<CheckedType>],expected:&CheckedType,environment:&EvalEnv,b:&mut Budget)->Check<bool>{
        let actual=value_at(values,argument)?;
        if self.compatible(environment,actual,expected,b)?{return Ok(true);}
        if actual.ty!=expected.ty||!matches!(actual.ty,Type::Address{..}){return Ok(false);}
        let mut rows=expected.life.slots.iter().filter(|row|row.path.is_empty());
        let Some(outer)=rows.next()else{return Ok(false);};if rows.next().is_some(){return Err(bad("Call parameter has duplicate outer borrow occurrences"));}
        // Independent child/phantom classes must retain their own origin.
        // A pre-existing collapsed representation cannot be repaired here.
        if expected.life.retained.iter().any(|row|row.binder==outer.binder)||expected.life.nominal_arguments.iter().any(|row|row.binder==outer.binder)||expected.life.slots.iter().any(|row|!row.path.is_empty()&&row.binder==outer.binder){return Err(unsupported("Call outer role shares an unresolved whole-child intrinsic class",None));}
        let Some(actual_outer)=actual.life.slots.iter().find(|row|row.path.is_empty()&&row.mode==outer.mode)else{return Ok(false);};
        let source=expected.classes.get(outer.binder as usize).ok_or_else(||bad("Call original outer class is absent"))?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=caller.target else{return Err(bad("Call borrow role has no actual caller"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("Call borrow caller is absent"))?;
        let scope=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.key;
        let mut current=argument;let mut seen=std::collections::BTreeSet::new();let mut related=false;
        loop{
            b.spend(1)?;if !seen.insert(current){return Err(bad("Call argument reborrow has an SSA producer cycle"));}
            let mut producer=None;for block in &f.blocks{b.spend(block.instructions.len())?;for row in &block.instructions{if row.result==Some(current)&&producer.replace(row).is_some(){return Err(bad("Call argument has several SSA definitions"));}}}
            let InstructionKind::ReborrowAddress{address,..}=&producer.ok_or_else(||bad("Call argument borrow is undefined"))?.kind else{break;};
            current=*address;
            let parent=value_at(values,current)?;
            let Some(parent_outer)=parent.life.slots.iter().find(|row|row.path.is_empty()&&row.mode==outer.mode)else{break;};
            let key=parent.classes.get(parent_outer.binder as usize).ok_or_else(||bad("Call reborrow parent class is absent"))?;
            if self.same(environment,source,key,b)?{
                // The exact original expression head must be a real earlier
                // operand in this same caller, never another class image.
                let exact_occurrence=matches!(key,ClassKey::Outer{scope:owner,operand:OperandKey::Value(id),path,mode}if *owner==scope&&*id==current&&path.is_empty()&&*mode==outer.mode);
                let parameter_role=match key{
                    ClassKey::Formal{..}=>{
                        b.spend(environment.lexical.iter().fold(1usize,|n,frame|n.saturating_add(frame.actuals.len())))?;
                        environment.lexical.iter().any(|frame|frame.actuals.iter().any(|actual|matches!(actual,CheckedActual::Life(life)if life==key)))&&loaded_parameter(f,current,b)?
                    },
                    ClassKey::Implicit{scope:owner,..}if *owner==scope=>loaded_parameter(f,current,b)?,
                    _=>false,
                };
                if !exact_occurrence&&!parameter_role{return Ok(false);}
                if !self.compatible(environment,parent,expected,b)?{return Ok(false);}
                related=true;break;
            }
        }
        if !related{return Ok(false);}
        expected.charge_clone(b)?;
        let mut role=expected.clone();
        role.classes[outer.binder as usize]=actual.classes.get(actual_outer.binder as usize).cloned().ok_or_else(||bad("Call actual child head is absent"))?;
        role.origins[outer.binder as usize]=actual.origins.get(actual_outer.binder as usize).cloned().ok_or_else(||bad("Call actual child head origin is absent"))?;
        self.same_type(environment,actual,&role,b)
    }
}

fn loaded_parameter(f:&TypedFunction,value:ValueId,b:&mut Budget)->Check<bool>{
    let mut selected=None;
    for block in &f.blocks{b.spend(block.instructions.len())?;for operation in &block.instructions{if operation.result==Some(value)&&selected.replace(operation).is_some(){return Err(bad("Call parameter head has several actual SSA definitions"));}}}
    b.spend(f.parameters.len())?;
    Ok(matches!(selected.map(|row|&row.kind),Some(InstructionKind::Load(local))if f.parameters.contains(local)))
}

// Specialize only a fresh outer invocation occurrence. An intrinsic child
// remains a variable class even when it happens to have the same outside image.
fn static_call_head(value:&mut CheckedType,source:&ClassKey,b:&mut Budget)->Check<()>{
    b.spend(value.classes.len().saturating_add(value.life.slots.len()).saturating_add(value.life.retained.len()).saturating_add(value.life.nominal_arguments.len()))?;
    let indices=value.classes.iter().enumerate().filter(|(_,key)|*key==source).map(|(index,_)|index as u32).collect::<Vec<_>>();
    for index in indices.into_iter().rev(){
        let mut outer=value.life.slots.iter().filter(|row|row.binder==index);
        let row=outer.next().ok_or_else(||bad("static invocation cannot erase a whole intrinsic child class"))?;
        if !row.path.is_empty()||row.mode!=StoredBorrowMode::SharedAddress||outer.next().is_some()||value.life.retained.iter().any(|row|row.binder==index)||value.life.nominal_arguments.iter().any(|row|row.binder==index)||!matches!(value.ty,Type::Address{mutable:false,..}){return Err(bad("static invocation shares a retained or non-shared original role"));}
        value.life.slots.retain(|row|row.binder!=index);
        for row in &mut value.life.slots{if row.binder>index{row.binder-=1;}}
        for row in &mut value.life.retained{if row.binder>index{row.binder-=1;}}
        for row in &mut value.life.nominal_arguments{if row.binder>index{row.binder-=1;}}
        value.classes.remove(index as usize);value.origins.remove(index as usize);value.life.binder_count=value.classes.len() as u32;
        let row=StaticOccurrence::Direct{path:vec![],leaf:value.raw_type_copy(b)?,mode:StoredBorrowMode::SharedAddress};if !value.statics.contains(&row){value.statics.push(row);}
    }Ok(())
}

#[cfg(test)]
mod input_image_tests{
    use super::*;
    fn address(key:ClassKey)->CheckedType{
        CheckedType{type_cell:crate::semantic::CheckedTypeData::new(Type::Address{mutable:false,inner:Box::new(Type::I32)}),life:StorageLifeShape{binder_count:1,slots:vec![StorageLifeSlot{path:vec![],binder:0,mode:StoredBorrowMode::SharedAddress}],retained:vec![],nominal_arguments:vec![]},origins:vec![IntrinsicOrigin::DerivedClass{source:key.clone()}],classes:vec![key],statics:vec![]}
    }
    // Original operand proofs supply these images; this test exercises only
    // the scoped invocation substitution, never a Source or borrow authority.
    #[test]
    fn invocation_result_preserves_original_head_images_and_opaque_occurrences(){
        let mut module=crate::resource_closure_tests::fixture(CallableKind::Fn,false,0);
        module.semantic=Some(SemanticModule{definitions:vec![],instances:vec![],sites:vec![],dyn_scopes:vec![]});
        let mut b=Budget::new(&Limits::default());
        let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let owner=SemanticDefinitionId{module:module.module_id,index:0};
        let call=ScopeKey::Witness{definition:owner,frame:0};
        let caller=ScopeKey::Instance(SemanticInstanceId{module:module.module_id,index:0});
        let env=EvalEnv{lexical:vec![],assumptions:vec![],images:vec![],bounds:vec![],invocations:vec![],at:None};
        let fresh=[ClassKey::Implicit{scope:call,ordinal:0},ClassKey::Implicit{scope:call,ordinal:1}];
        let origins=(0..2).map(|class|IntrinsicOrigin::Existing{scope:caller,operand:OperandKey::Receiver,path:vec![],class}).collect::<Vec<_>>();
        let template=CheckedSignature{parameters:fresh.iter().cloned().map(|key|(address(key),PassingMode::Borrow)).collect(),result:CheckedType{type_cell:crate::semantic::CheckedTypeData::new(Type::Closure(ClosureId(0))),life:StorageLifeShape{binder_count:2,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:fresh.to_vec(),origins:origins.clone(),statics:vec![]},result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound:vec![],invocation:vec![]};
        for equal_image in [false,true]{
            let a=ClassKey::Implicit{scope:caller,ordinal:0};
            let z=ClassKey::Implicit{scope:caller,ordinal:if equal_image{0}else{1}};
            let arguments=[address(a.clone()),address(z.clone())];
            let mut signature=template.clone();
            context.instantiate_call_heads(&mut signature,&arguments.iter().collect::<Vec<_>>(),call,&env,&mut b).unwrap();
            assert_eq!(signature.result.classes,vec![a,z]);
            assert_eq!(signature.result.life.binder_count,2);
            assert_eq!(signature.result.origins,origins);
            assert_ne!(signature.result.origins[0],signature.result.origins[1]);
            assert!(!context.same(&env,&fresh[0],&signature.result.classes[0],&mut b).unwrap(),"a call image must not become a global equality");
        }
        let mut repeated=template;
        repeated.parameters[1].0=address(fresh[0].clone());
        let arguments=[address(ClassKey::Implicit{scope:caller,ordinal:0}),address(ClassKey::Implicit{scope:caller,ordinal:1})];
        assert!(context.instantiate_call_heads(&mut repeated,&arguments.iter().collect::<Vec<_>>(),call,&env,&mut b).unwrap_err().message.contains("repeated original invocation relation"));
    }
}
