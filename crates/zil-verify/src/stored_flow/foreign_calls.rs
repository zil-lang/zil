//! The ordinary C boundary coexists with complete stored class equations.
//! External promises authorize no new stored root, returned pointer or lease.
use super::*;
pub(super) fn declaration(module:&TypedModule,f:&TypedFunction,b:&mut Budget)->Check<bool>{
    let Some(foreign)=&f.foreign else{return Ok(false);};
    if f.call_kind!=CallKind::Direct||f.environment.is_some()||f.result_mode!=PassingMode::Infer
        ||f.reference_return_bound.is_some()||f.callable_contract.is_some()||f.callable_return_contract.is_some()
        ||!f.protocol_contracts.is_empty()||foreign.scoped_callback.is_some(){return Ok(false);}
    // Full canonical ABI, unique symbol, all noescape indices, supported C
    // payloads, static callback contracts and the absence of a body are checked
    // independently. A Some storage header is an additional equation claim.
    crate::foreign::check_declarations(module,b)?;
    if let Some(header)=&f.storage_life {
        crate::signature_life::charge(header,b)?;
        if header.fixed_binder_count!=0{return Err(err("ordinary foreign ABI cannot invent a free Fixed environment"));}
    }
    Ok(true)
}
pub(super) fn operation(module:&TypedModule,f:&TypedFunction,i:&Instruction,b:&mut Budget)->Check<bool>{
    match &i.kind{
        InstructionKind::ForeignFunctionRef{function}=>{
            let expected=crate::native_values::function_ref(module,*function,Some(i.span),b)?;
            let actual=vt(f,i.result.ok_or_else(||err("static C reference has no actual result"))?)?;
            type_work(actual,&b.limits.clone(),b)?;
            if actual!=&expected{return Err(err("static C reference differs from its exact actual target signature"));}Ok(true)
        },
        InstructionKind::ForeignCall{function,arguments,..}=>{
            let target=module.functions.get(function.0 as usize).ok_or_else(||err("stored foreign actual target is absent"))?;
            if !declaration(module,target,b)?{return Err(err("stored foreign call has no complete ordinary C declaration"));}
            let expected=crate::foreign::check_call(module,f,*function,arguments,Some(i.span),b)?;
            let actual=i.result.map(|value|vt(f,value)).transpose()?;
            if actual!=expected.as_ref(){return Err(err("stored foreign call actual result differs from its complete C declaration"));}
            Ok(true)
        },
        _=>Ok(false),
    }
}
pub(super) fn accesses(module:&TypedModule,f:&TypedFunction,i:&Instruction,values:&[Payload],checks:&mut access_candidates::Access<'_>,requirements:&mut Vec<(Source,Source)>,b:&mut Budget)->Check<()>{
    let InstructionKind::ForeignCall{arguments,..}=&i.kind else{return Ok(());};
    operation(module,f,i,b)?;
    b.spend(arguments.len())?;
    let mut pointers:Vec<(&Permission,bool)>=Vec::new();
    for argument in arguments {
        let Type::Address{mutable,..}=vt(f,*argument)? else{continue;};
        let permission=&values.get(argument.0 as usize).ok_or_else(||err("foreign borrowed operand has no actual permission"))?.address;
        checks.check(permission,*mutable,requirements,b)?;
        // Every simultaneous borrowed parameter belongs to this real call.
        // Delegating its own parent loan cannot excuse another mutable role.
        for(prior,write)in &pointers {
            if *mutable||*write {
                for a in &permission.sources {for z in &prior.sources {apart(a,z,requirements,b)?;}}
            }
        }
        pointers.push((permission,*mutable));
    }
    Ok(())
}
