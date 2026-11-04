//! Kernel reconstruction of the stable owning retained callback boundary.
use super::*;
pub(super) const PREFIX:&str="@builtin:Registration:";
fn error(span:Option<Span>,s:&str)->Diagnostic {reject("E-VERIFY-REGISTRATION",span,s)}
pub(super) fn carrier(module:&TypedModule,ty:&Type,span:Option<Span>,budget:&mut Budget)->Check<()> {
    budget.registrations=true;
    let (Type::Managed(p)|Type::Desc(p))=ty else{return Err(error(span,"registration requires an owner or descriptor"))};
    let Type::Registration{environment}=p.as_ref() else{return Err(error(span,"sealed registration payload is absent"))};
    let Type::Closure(id)=environment.as_ref() else{return Err(error(span,"registration environment must be an actual concrete closure"))};
    budget.spend(module.resources.len().saturating_add(32))?;
    let name=format!("{PREFIX}{}",id.0);
    if !module.target.is_supported_x64_llvm()||module.resources.iter().filter(|r|r.name==name && r.payload==**p && r.teardown.is_none()).count()!=1{return Err(error(span,"registration lacks its exact canonical owning definition"))}
    closed(module,*id,span,budget)
}
fn closed(module:&TypedModule,root:ClosureId,span:Option<Span>,budget:&mut Budget)->Check<()> {
    let mut closure_queue=vec![root];let mut closures_seen=BTreeSet::new();let mut code_queue=Vec::new();
    while let Some(id)=closure_queue.pop(){
        budget.spend(closures_seen.len().saturating_add(1))?;
        if !closures_seen.insert(id){continue}
        let c=closures::definition(module,id,span)?;
        budget.spend(c.captures.len().saturating_add(c.lending_captures.len()).saturating_add(1))?;
        if !c.lending_captures.is_empty(){return Err(error(span,"ClosedEnvironment cannot hold borrowed capture positions"))}
        let entry=module.functions.get(c.function.0 as usize).ok_or_else(||error(span,"registration entry is absent"))?;
        budget.spend(c.parameters.len().saturating_add(1))?;
        if entry.environment!=Some(id)||entry.foreign.is_some()||entry.parameters.len()!=c.parameters.len()||entry.return_type!=c.result||entry.result_mode!=PassingMode::Infer{return Err(error(span,"registration closure differs from its actual entry"))}
        for (local,t)in entry.parameters.iter().zip(&c.parameters){if local_type(entry,*local,span)?!=t{return Err(error(span,"registration actual entry parameter differs"))}}
        code_queue.push(c.function);
        let mut fields=c.captures.iter().collect::<Vec<_>>();
        while let Some(t)=fields.pop(){
            budget.spend(1)?;
            match t {
                Type::Address{..}|Type::Desc(_)|Type::Slice{..}|Type::Named(_)|Type::Unsupported(_)|Type::SyncCell(_)=>return Err(error(span,"ClosedEnvironment contains an external loan or unresolved position")),
                Type::Closure(id)=>{budget.spend(1)?;closure_queue.push(*id)},
                Type::Managed(p)|Type::Opaque{hidden:p,..}|Type::OwnedStore{element:p}|Type::Registration{environment:p}|Type::Array{element:p,..}=>{budget.spend(1)?;fields.push(p)},
                Type::Tuple(ts)=>{budget.spend(ts.len())?;fields.extend(ts)},
                Type::Package{fields:fs,..}|Type::Record{fields:fs,..}=>{budget.spend(fs.len())?;fields.extend(fs.iter().map(|(_,t)|t))},
                Type::Enum{variants,..}=>{budget.spend(variants.len())?;for (_,ts)in variants{budget.spend(ts.len())?;fields.extend(ts)}},
                Type::ErrorUnion{error,result}=>{budget.spend(2)?;fields.extend([error.as_ref(),result.as_ref()])},
                Type::FunctionPointer{..}|Type::ForeignCallback{..}|Type::Unit|Type::Bool|Type::U8|Type::U16|Type::I32|Type::I64|Type::ForeignHandle|Type::CWideString|Type::ByteBuffer|Type::TaskI32=>{}
            }
        }
    }
    let mut code_seen=BTreeSet::new();
    while let Some(id)=code_queue.pop(){
        budget.spend(code_seen.len().saturating_add(1))?;
        if !code_seen.insert(id){continue}
        let f=module.functions.get(id.0 as usize).ok_or_else(||error(span,"registration Direct dependency is absent"))?;
        for b in &f.blocks{budget.spend(1)?;for i in &b.instructions{budget.spend(1)?;match i.kind {
            InstructionKind::TaskYield=>return Err(error(Some(i.span),"registration callback may only execute through Direct ABI")),
            InstructionKind::Call{function,..}|InstructionKind::FunctionRef{function}|InstructionKind::ForeignFunctionRef{function}=>{budget.spend(1)?;code_queue.push(function)},
            _=>{}
        }}}
    }
    Ok(())
}
pub(super) fn instruction(module:&TypedModule,f:&TypedFunction,i:&Instruction,budget:&mut Budget)->Check<Option<Type>> {
    let span=Some(i.span);budget.registrations=true;budget.spend(1)?;
    Ok(Some(match i.kind {
        InstructionKind::RegistrationStart{register,close,callback,..}=>{
            for (id,role)in [(register,ScopedCallbackRole::Register),(close,ScopedCallbackRole::Close)]{
                let target=module.functions.get(id.0 as usize).ok_or_else(||error(span,"registration static role absent"))?;
                if target.foreign.as_ref().and_then(|f|f.scoped_callback)!=Some(role){return Err(error(span,"registration static target has wrong role"))}
                scoped_callback::declaration(target,budget)?;
            }
            let Type::Closure(id)=value_type(f,callback,span)? else{return Err(error(span,"registration requires actual consumed concrete F"))};
            let c=closures::definition(module,*id,span)?;budget.spend(c.parameters.len().saturating_add(1))?;
            if !matches!(c.kind,CallableKind::Fn|CallableKind::FnMut)||c.parameters!=[Type::I32]||c.result!=Type::I32{return Err(error(span,"registration requires actual Fn/FnMut(i32)->i32"))}
            let t=Type::Managed(Box::new(Type::Registration{environment:Box::new(Type::Closure(*id))}));carrier(module,&t,span,budget)?;t
        },
        InstructionKind::RegistrationValid{registration}=>{
            let t=value_type(f,registration,span)?;
            if !matches!(t,Type::Desc(_)){return Err(error(span,"registration validity requires actual descriptor"))};carrier(module,t,span,budget)?;Type::Bool
        },
        InstructionKind::RegistrationClose{registration}=>{
            let t=value_type(f,registration,span)?;
            if !matches!(t,Type::Managed(_)){return Err(error(span,"registration close consumes actual owning head"))};carrier(module,t,span,budget)?;Type::Unit
        },
        _=>unreachable!()
    }))
}

pub(super) fn factory(module:&TypedModule,f:&TypedFunction,budget:&mut Budget)->Check<bool>{
    if f.callable_contract.is_some(){return Ok(false)}
    let Type::Managed(payload)=&f.return_type else{return Ok(false)};
    let Type::Registration{environment}=payload.as_ref()else{return Ok(false)};
    let Type::Closure(id)=environment.as_ref()else{return Err(error(Some(f.span),"registration factory environment absent"))};
    budget.spend(f.parameters.len().saturating_add(1))?;
    let mut input=None;
    for (index,local)in f.parameters.iter().enumerate(){let t=local_type(f,*local,Some(f.span))?;if t==environment.as_ref(){if input.replace(index).is_some(){return Err(error(Some(f.span),"registration factory requires one actual F input"))}}else if !t.is_copy_value(){return Err(error(Some(f.span),"registration factory ordinary input is outside finite Copy profile"))}}
    if input.is_none()||f.foreign.is_some()||f.environment.is_some()||!matches!(f.result_mode,PassingMode::Create|PassingMode::Give){return Err(error(Some(f.span),"registration factory must transfer actual F into create/give owner result"))}
    closed(module,*id,Some(f.span),budget)?;Ok(true)
}
