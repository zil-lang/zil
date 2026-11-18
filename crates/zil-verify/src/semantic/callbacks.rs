//! Exact static reference formation and its C ABI type. Forming the reference
//! neither executes the target nor certifies its actual body/effects.
use super::*;
impl CheckedContext{
    pub(super) fn foreign_callback_code(&self,module:&TypedModule,signature:&CheckedSignature,b:&mut Budget)->Check<CheckedType>{
        if signature.call_kind!=CallKind::Direct||signature.result_mode!=PassingMode::Infer||!signature.bound.is_empty()||!signature.invocation.is_empty(){return Err(bad("C callback code has another ABI, result mode or quantified domain"));}
        let mut parameters=Vec::new();b.spend(signature.parameters.len().saturating_add(1))?;
        for(value,mode)in &signature.parameters{
            dynamic_metadata::ty(&value.ty,b)?;
            if *mode!=PassingMode::Infer||value.life!=empty_life()||!value.classes.is_empty()||!value.origins.is_empty()||!value.statics.is_empty(){return Err(bad("C callback parameter has hidden borrowing or another passing mode"));}
            parameters.push(value.ty.clone());
        }
        let result=&signature.result;
        if result.life!=empty_life()||!result.classes.is_empty()||!result.origins.is_empty()||!result.statics.is_empty(){return Err(bad("C callback result has a hidden complete class or static domain"));}
        crate::native_values::target(&module.target,None)?;
        crate::native_values::callback_signature(&parameters,&result.ty,None)?;
        dynamic_metadata::ty(&result.ty,b)?;
        self.checked_value(module,&Type::ForeignCallback{parameters,result:Box::new(result.ty.clone())},&empty_life(),vec![],b)
    }
    pub(super) fn ordinary_code(&self,module:&TypedModule,kind:SemanticCodeKind,signature:&CheckedSignature,b:&mut Budget)->Check<CheckedType>{
        if matches!(kind,SemanticCodeKind::FunctionPointer|SemanticCodeKind::BorrowedFunctionPointer) && !signature.invocation.is_empty(){
            return self.invocation_code(module,signature,b);
        }
        if kind!=SemanticCodeKind::FunctionPointer||signature.call_kind!=CallKind::Direct||signature.result_mode!=PassingMode::Infer||!signature.bound.is_empty()||!signature.invocation.is_empty(){return Err(unsupported("ordinary code requires its exact complete pointer ABI and binder header",None));}
        let mut parameters=Vec::new();b.spend(signature.parameters.len().saturating_add(1))?;
        for(value,mode)in &signature.parameters{
            ghost_code_value(module,value,b)?;
            if *mode!=PassingMode::Infer{return Err(bad("ordinary code changes an encoded parameter responsibility mode"));}parameters.push(value.ty.clone());
        }
        ghost_code_value(module,&signature.result,b)?;
        self.checked_value(module,&Type::FunctionPointer{parameters,result:Box::new(signature.result.ty.clone())},&empty_life(),vec![],b)
    }
    fn invocation_code(&self,module:&TypedModule,signature:&CheckedSignature,b:&mut Budget)->Check<CheckedType>{
        if signature.call_kind!=CallKind::Direct||signature.result_mode!=PassingMode::Infer||!signature.bound.is_empty(){
            return Err(bad("thin invocation code changes its ABI or quantified domain"));
        }
        let index=|value:&CheckedType,b:&mut Budget|->Check<Option<u32>>{
            let Type::Address{mutable,..}=&value.ty else{ghost_code_value(module,value,b)?;return Ok(None);};
            b.spend(signature.invocation.len().saturating_add(1))?;
            let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
            if value.life.binder_count!=1||value.life.slots.len()!=1||!value.life.slots[0].path.is_empty()||value.life.slots[0].mode!=mode||value.life.slots[0].binder!=0
                ||!value.life.retained.is_empty()||!value.life.nominal_arguments.is_empty()||!value.statics.is_empty()||value.classes.len()!=1{
                return Err(unsupported("thin borrowed code requires the complete root address relation at this position",None));
            }
            signature.invocation.iter().position(|key|key==&value.classes[0])
                .map(|index|Some(index as u32)).ok_or_else(||bad("thin code address selects a lifetime outside its own invocation"))
        };
        let mut parameters=Vec::new();let mut parameter_lives=Vec::new();
        b.spend(signature.parameters.len().saturating_add(1))?;
        for (value,mode) in &signature.parameters{
            if *mode!=PassingMode::Infer{return Err(bad("thin invocation code changes its parameter responsibility mode"));}
            parameter_lives.push(index(value,b)?);parameters.push(value.ty.clone());
        }
        let binder_count=u32::try_from(signature.invocation.len()).map_err(|_|limit("thin invocation binder count exceeded"))?;
        let result_life=index(&signature.result,b)?;
        let relation=StorageLifeContract::from_flat(FunctionLifeContract{binder_count,parameter_lives,result_life}).ok_or_else(||bad("thin invocation relation exceeds its parameter directory"))?;
        crate::borrowed_fp::relation(&parameters,&signature.result.ty,&relation,None)?;
        let ty=Type::BorrowedFunctionPointer{parameters,result:Box::new(signature.result.ty.clone()),life:relation};
        self.checked_value(module,&ty,&empty_life(),vec![],b)
    }

}

fn ghost_code_value(module:&TypedModule,value:&CheckedType,b:&mut Budget)->Check<()>{
    dynamic_metadata::ty(&value.ty,b)?;dynamic_metadata::life_shape(&value.life,b)?;
    if value.life!=empty_life()||!value.classes.is_empty()||!value.origins.is_empty()||!value.statics.is_empty(){return Err(unsupported("plain code cannot erase a complete borrowed or intrinsic parameter/result domain",None));}
    dynamic_witness::check_type_parts(module,&value.ty,&value.life,&[],DynInstantiationScope{fixed_binder_count:0,invocation_binder_count:0},b)
}

#[cfg(test)]
mod tests{
    use super::*;
    fn value(ty:Type)->CheckedType{CheckedType{type_cell: crate::semantic::CheckedTypeData::new(ty),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]}}
    #[test]
    fn borrowed_code_reconstructs_its_private_invocation_relation() {
        let limits=Limits::default();
        let (module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let mut b=Budget::new(&limits);
        let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let key=ClassKey::Invocation{frame:FrameToken(37),index:0};
        let mut input=value(Type::Address{mutable:false,inner:Box::new(Type::I32)});
        input.life.binder_count=1;
        input.life.slots.push(StorageLifeSlot{path:vec![],binder:0,mode:StoredBorrowMode::SharedAddress});
        input.classes.push(key.clone());input.origins.push(IntrinsicOrigin::DerivedClass{source:key.clone()});
        let mut signature=CheckedSignature{parameters:vec![(input,PassingMode::Infer)],result:value(Type::I32),
            result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound:vec![],invocation:vec![key]};
        let ty=context.ordinary_code(&module,SemanticCodeKind::FunctionPointer,&signature,&mut b).unwrap();
        let Type::BorrowedFunctionPointer{life,..}=&ty.ty else {panic!("missing invocation relation")};
        assert_eq!(life,&StorageLifeContract::from_flat(FunctionLifeContract{binder_count:1,parameter_lives:vec![Some(0)],result_life:None}).unwrap());
        signature.invocation[0]=ClassKey::Invocation{frame:FrameToken(38),index:0};
        assert!(context.ordinary_code(&module,SemanticCodeKind::FunctionPointer,&signature,&mut b).is_err());
    }
    #[test]
    fn c_code_geometry_keeps_all_numeric_widths_and_excludes_borrowed_or_suspend_roles(){
        let limits=Limits::default();let bytes=include_bytes!("fixtures/ordinary-identity-complete-head.wire");
        let(module,_)=zil_ir::wire::decode_module_candidate(bytes,&limits).unwrap();
        let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        for ty in [Type::I8,Type::I16,Type::I32,Type::I64,Type::Isize,Type::U8,Type::U16,Type::U32,Type::U64,Type::Usize,Type::F32,Type::F64,Type::ForeignHandle]{
            let signature=CheckedSignature{parameters:vec![(value(ty.clone()),PassingMode::Infer)],result:value(ty.clone()),result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound:vec![],invocation:vec![]};
            let actual=context.foreign_callback_code(&module,&signature,&mut b).unwrap();
            assert_eq!(actual.ty,Type::ForeignCallback{parameters:vec![ty.clone()],result:Box::new(ty)});
            assert_eq!(actual.life,empty_life());
        }
        let plain=CheckedSignature{parameters:vec![(value(Type::I32),PassingMode::Infer)],result:value(Type::I32),result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound:vec![],invocation:vec![]};
        for attack in 0..5{
            let mut signature=plain.clone();match attack{0=>signature.call_kind=CallKind::Suspend,1=>signature.parameters[0].0=value(Type::Bool),2=>signature.result_mode=PassingMode::Give,3=>signature.bound.push(ClassKey::Bound{frame:FrameToken(0),index:0}),4=>signature.parameters[0].0.classes.push(ClassKey::Invocation{frame:FrameToken(0),index:0}),_=>unreachable!()}
            assert!(context.foreign_callback_code(&module,&signature,&mut Budget::new(&limits)).is_err(),"C code geometry attack {attack}");
        }
    }
}
