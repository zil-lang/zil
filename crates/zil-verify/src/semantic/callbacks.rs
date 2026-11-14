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
}

#[cfg(test)]
mod tests{
    use super::*;
    fn value(ty:Type)->CheckedType{CheckedType{ty,life:empty_life(),classes:vec![],origins:vec![],statics:vec![]}}
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
