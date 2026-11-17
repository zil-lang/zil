//! Resumable cleanup of one real inline environment responsibility.
//! Capture flags are the cursor: Pending preserves every remaining flag and
//! the enclosing owner node. A finished leaf is never visited again.
use super::*;
use zil_ir::TypedModule;

pub(super) fn symbol(id: ClosureId, operation: &str) -> String {
    format!("zil.task.group.{}.{operation}", id.0)
}

fn definition(module: &TypedModule, id: ClosureId) -> CompileResult<&ClosureDefinition> {
    module.closures.get(id.0 as usize).filter(|d| d.id == id).ok_or_else(|| {
        vec![Diagnostic::error(Phase::Codegen, "E-CODEGEN-TASK", None,
            "cleanup group has no actual environment definition")]
    })
}

fn scan(module: &TypedModule, ty: &Type, depth: usize, meter: &Meter) -> CompileResult<bool> {
    meter.spend(1)?;
    if depth >= 64 {
        return unsupported("cleanup group exceeds finite inline depth", None);
    }
    Ok(match enums::storage_shape(ty) {
        Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..}|Type::TaskI32|Type::AsyncRegistration{..})=>true,
        Type::Closure(id)=>{let mut found=false;for capture in &definition(module,*id)?.captures{found|=scan(module,capture,depth+1,meter)?;}found},
        Type::Managed(payload)=>match payload.as_ref(){Type::Mutex{payload}=>scan(module,payload,depth+1,meter)?,Type::OwnedStore{element}=>scan(module,element,depth+1,meter)?,_=>false},
        Type::Tuple(fields)=>{let mut found=false;for field in fields{found|=scan(module,field,depth+1,meter)?;}found},
        Type::Record{fields,..}|Type::Package{fields,..}=>{let mut found=false;for(_,field)in fields{found|=scan(module,field,depth+1,meter)?;}found},
        Type::Array{element,length}=>*length!=0&&scan(module,element,depth+1,meter)?,
        Type::Enum{variants,..}=>{let mut found=false;for(_,fields)in variants{for field in fields{found|=scan(module,field,depth+1,meter)?;}}found},
        Type::ErrorUnion{error,result}=>scan(module,error,depth+1,meter)?|scan(module,result,depth+1,meter)?,
        // A borrowed head owns no pointee. OwnedDyn has an independent actual
        // provider/drop ABI gate before any prepared module is published.
        Type::Address{..}|Type::Desc(_)|Type::OwnedDyn{..}=>false,
        _=>false,
    })
}

fn type_work(ty: &Type, meter: &Meter) -> CompileResult<()> {
    let mut limits=meter.limits()?.clone();
    limits.max_analysis_steps=meter.remaining();
    let mut budget=drop_plan::Budget::new(&limits);
    let result=budget.tree(ty,0);
    meter.spend(budget.spent())?;
    result
}

fn synchronous(module:&TypedModule,ty:&Type,depth:usize,meter:&Meter)->CompileResult<bool> {
    meter.spend(1)?;if depth>=64{return unsupported("synchronous group data is recursive or too deep",None);}
    type_work(ty,meter)?;
    Ok(match enums::storage_shape(ty) {
        Type::Managed(payload)=>{
            if matches!(payload.as_ref(),Type::Task{..}|Type::TaskI32|Type::AsyncRegistration{..}){return Ok(false);}
            let mut actual=None;
            for resource in &module.resources{type_work(&resource.payload,meter)?;type_work(payload,meter)?;if resource.payload==**payload&&actual.replace(resource).is_some(){return invalid("E-CODEGEN-TASK","synchronous group resource binding is ambiguous",None);}}
            let Some(resource)=actual else{return invalid("E-CODEGEN-TASK","synchronous group resource binding is absent",None);};
            if let Some(drop)=resource.teardown {
                let actual=module.functions.get(drop.0 as usize).filter(|entry|entry.id==drop).ok_or_else(||vec![Diagnostic::error(Phase::Codegen,"E-CODEGEN-TASK",None,"synchronous group teardown is absent")])?;
                if actual.call_kind!=zil_ir::CallKind::Direct||actual.foreign.is_some()||actual.environment.is_some()||actual.return_type!=Type::Unit||actual.parameters.len()!=1{return Ok(false);}
                let parameter=actual.locals.get(actual.parameters[0].0 as usize).ok_or_else(||vec![Diagnostic::error(Phase::Codegen,"E-CODEGEN-TASK",None,"synchronous group teardown input is absent")])?;
                type_work(&parameter.ty,meter)?;
                if !matches!(enums::storage_shape(&parameter.ty),Type::Desc(inner)if inner.as_ref()==&resource.payload){return Ok(false);}
            }
            match payload.as_ref(){
                Type::Mutex{payload}=>synchronous(module,payload,depth+1,meter)?,
                Type::OwnedStore{element}=>synchronous(module,element,depth+1,meter)?,
                // Canonical ByteBuffer/Registration and ordinary resources
                // still complete through their existing actual manager gate.
                _=>true,
            }
        }
        Type::Closure(id)=>{let mut yes=true;for capture in &definition(module,*id)?.captures{yes&=synchronous(module,capture,depth+1,meter)?;}yes},
        Type::Tuple(fields)=>{let mut yes=true;for field in fields{yes&=synchronous(module,field,depth+1,meter)?;}yes},
        Type::Record{fields,..}|Type::Package{fields,..}=>{let mut yes=true;for(_,field)in fields{yes&=synchronous(module,field,depth+1,meter)?;}yes},
        Type::Array{element,length}=>*length==0||synchronous(module,element,depth+1,meter)?,
        Type::Enum{variants,..}=>{let mut yes=true;for(_,fields)in variants{for field in fields{yes&=synchronous(module,field,depth+1,meter)?;}}yes},
        Type::ErrorUnion{error,result}=>synchronous(module,error,depth+1,meter)?&synchronous(module,result,depth+1,meter)?,
        Type::Address{..}|Type::Desc(_)|Type::OwnedDyn{..}=>true,
        other=>other.is_copy_value(),
    })
}
fn validate(module:&TypedModule,id:ClosureId,depth:usize,meter:&Meter)->CompileResult<()> {
    meter.spend(1)?;if depth>=64{return unsupported("cleanup group is recursive or too deep",None);}
    for ty in &definition(module,id)?.captures {
        let physical=enums::storage_shape(ty);
        match physical {
            Type::Closure(child)if scan(module,ty,0,meter)?=>validate(module,*child,depth+1,meter)?,
            Type::Managed(payload)if matches!(payload.as_ref(),Type::Task{..}|Type::TaskI32|Type::AsyncRegistration{..})=>{},
            _=>{
                if scan(module,ty,0,meter)? {return unsupported("asynchronous composite capture needs its complete resumable field recipe",None);}
                if !synchronous(module,ty,0,meter)? {return unsupported("cleanup group synchronous capture lacks a complete actual Direct teardown",None);}
            }
        }
    }
    Ok(())
}

pub(super) fn closure(module: &TypedModule, ty: &Type, meter: &Meter) -> CompileResult<Option<ClosureId>> {
    let physical=enums::storage_shape(ty);
    let Type::Closure(id)=physical else {
        if matches!(physical,Type::Managed(payload)if matches!(payload.as_ref(),Type::Task{..}|Type::TaskI32|Type::AsyncRegistration{..})) {return Ok(None);}
        if scan(module,ty,0,meter)? {return unsupported("asynchronous canonical composite owner/result needs its complete resumable field recipe",None);}
        return Ok(None);
    };
    if !scan(module,ty,0,meter)? {return Ok(None);}
    validate(module,*id,0,meter)?;Ok(Some(*id))
}

fn flag(layouts: &LayoutContext<'_>, v: &mut Vec<m::Inst>, d: &ClosureDefinition, index: usize) -> CompileResult<()> {
    let position = layouts.flag_start(d.id,index as u32)?;
    gep(v, &format!("flag.{index}"), &environment_type(d.id), loc("env"), position as u32);
    load(v, &format!("live.{index}"), m::Ty::Int(1), loc(format!("flag.{index}")));
    Ok(())
}
fn clear(v: &mut Vec<m::Inst>, index: usize) {
    store(v, m::Ty::Int(1), m::val(false), loc(format!("flag.{index}")));
}
fn next(index: usize) -> String { format!("check.{}", index + 1) }

fn functions(layouts: &LayoutContext<'_>, module: &TypedModule, id: ClosureId, limits: &zil_base::Limits, meter: &Meter) -> CompileResult<Vec<m::Item>> {
    let d = definition(module, id)?;
    meter.spend(d.captures.len().saturating_mul(256).saturating_add(128))?;
    let mut poll = vec![block("entry", vec![inst(None, m::Op::Branch("check.0".into()))])];
    let mut request = vec![block("entry", vec![inst(None, m::Op::Branch("check.0".into()))])];
    for (index, ty) in d.captures.iter().enumerate() {
        let check = format!("check.{index}");
        let body = format!("leaf.{index}");
        let finish = format!("finish.{index}");
        if layout::capture_flag_count(ty) == 0 {
            poll.push(block(&check, vec![inst(None,m::Op::Branch(next(index)))]));
            request.push(block(&check, vec![inst(None,m::Op::Branch(next(index)))]));
            continue;
        }
        let mut guarded = vec![];
        flag(layouts, &mut guarded, d, index)?;
        cond(&mut guarded, &format!("live.{index}"), &body, &next(index));
        poll.push(block(&check, guarded.clone()));
        request.push(block(&check, guarded));
        let mut p = vec![];
        // Every leaf has its own SSA spelling within this helper.
        let payload = format!("payload.{index}");
        gep(&mut p, &payload, &environment_type(id), loc("env"), index as u32);
        let mut r = p.clone();
        match enums::storage_shape(ty) {
            Type::Managed(inner) if matches!(inner.as_ref(),Type::Task{..}|Type::TaskI32) => {
                call(&mut r,None,m::Ty::Void,m::global("zil.task.request_cancel".into()),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                let frame = format!("frame.{index}");
                let empty = format!("empty.{index}");
                load(&mut p,&frame,m::Ty::Ptr,loc(&payload));
                compare(&mut p,&empty,m::Ty::Ptr,loc(&frame),m::Val::Null,"eq");
                let active = format!("active.{index}");
                cond(&mut p,&empty,&finish,&active);
                poll.push(block(&body,p));
                p=vec![];
                call(&mut p,None,m::Ty::Void,m::global("zil.task.request_cancel".into()),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                let ready = format!("ready.{index}");
                call(&mut p,Some(&ready),m::Ty::Int(1),m::global("zil.task.poll_close".into()),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                let destroy = format!("destroy.{index}");
                cond(&mut p,&ready,&destroy,"pending");
                poll.push(block(&active,p));
                p=vec![];
                call(&mut p,None,m::Ty::Void,m::global("zil.task.finish_destroy".into()),vec![m::Arg::new(m::Ty::Ptr,loc(&frame))]);
                store(&mut p,m::Ty::Ptr,m::Val::Null,loc(&payload));
                branch(&mut p,&finish);
                poll.push(block(&destroy,p));
                p=vec![];
            },
            Type::Managed(inner) if matches!(inner.as_ref(),Type::AsyncRegistration{..}) => {
                call(&mut r,None,m::Ty::Void,m::global("zil.async_registration.request_head".into()),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                call(&mut p,None,m::Ty::Void,m::global("zil.async_registration.request_head".into()),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                let ready = format!("ready.{index}");
                call(&mut p,Some(&ready),m::Ty::Int(1),m::global("zil.async_registration.drop".into()),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                cond(&mut p,&ready,&finish,"pending");
                poll.push(block(&body,p));
                p=vec![];
            },
            Type::Closure(child) if scan(module,ty,0,meter)? => {
                call(&mut r,None,m::Ty::Void,m::global(symbol(*child,"request")),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                let ready = format!("ready.{index}");
                call(&mut p,Some(&ready),m::Ty::Int(1),m::global(symbol(*child,"poll")),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                cond(&mut p,&ready,&finish,"pending");
                poll.push(block(&body,p));
                p=vec![];
            },
            _ => {
                // The shared capture helper consumes the exact shell and
                // direct-child flags, including already-taken Package fields.
                call(&mut p,None,m::Ty::Void,m::global(resource_closures::capture_drop_symbol(id,index as u32)),vec![m::Arg::new(m::Ty::Ptr,loc("env"))]);
                branch(&mut p,&finish);poll.push(block(&body,p));p=vec![];
            },
        }
        clear(&mut p,index);
        branch(&mut p,&next(index));
        poll.push(block(&finish,p));
        branch(&mut r,&next(index));
        request.push(block(&body,r));
    }
    poll.push(block(&format!("check.{}",d.captures.len()),vec![inst(None,m::Op::Return(Some(m::Arg::new(m::Ty::Int(1),true))))]));
    poll.push(block("pending",vec![inst(None,m::Op::Return(Some(m::Arg::new(m::Ty::Int(1),false))))]));
    request.push(block(&format!("check.{}",d.captures.len()),vec![inst(None,m::Op::Return(None))]));
    Ok(vec![function(symbol(id,"poll"),m::Ty::Int(1),vec![m::Param::new(m::Ty::Ptr,"env")],poll),function(symbol(id,"request"),m::Ty::Void,vec![m::Param::new(m::Ty::Ptr,"env")],request)])
}

pub(super) fn helpers(layouts: &LayoutContext<'_>, verified: &VerifiedModule, meter: &Meter) -> CompileResult<Vec<m::Item>> {
    let module=verified.module();
    let mut needed=BTreeSet::new();
    let mut pending=Vec::new();
    for f in &module.functions {
        if let Some(ownership)=verified.task_ownership(f.id) {
            for owner in ownership.owners() {
                if let Some(id)=closure(module,owner.ty(),meter)? { pending.push(id); }
            }
            if let Some(ty)=ownership.result_type()
                && let Some(id)=closure(module,ty,meter)? { pending.push(id); }

        }
    }
    let mut result=vec![];
    while let Some(id)=pending.pop() {
        meter.spend(needed.len().saturating_add(1))?;
        if !needed.insert(id) {continue;}
        for ty in &definition(module,id)?.captures {
            if let Some(child)=closure(module,ty,meter)? { pending.push(child); }
        }
        result.extend(functions(layouts,module,id,verified.limits(),meter)?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn module() -> TypedModule {
        let quiet=Type::Record{name:"Quiet".into(),fields:vec![("value".into(),Type::I32)]};
        let package=Type::Package{name:"QuietPackage".into(),fields:vec![("quiet".into(),Type::Managed(Box::new(quiet.clone())))]};
        let mut module=layout::test_environments(vec![vec![Type::Managed(Box::new(Type::TaskI32)),package]]);
        let mut teardown=module.functions[0].clone();teardown.id=FunctionId(1);teardown.name="quiet_drop".into();teardown.environment=None;
        teardown.parameters=vec![LocalId(0)];teardown.locals=vec![zil_ir::LocalDecl{id:LocalId(0),name:"self".into(),ty:Type::Desc(Box::new(quiet.clone())),mutable:false,span:Span::default()}];
        module.functions.push(teardown);
        module.resources=vec![ResourceDefinition{name:"@builtin:Task<i32>".into(),payload:Type::TaskI32,teardown:None},ResourceDefinition{name:"Quiet".into(),payload:quiet,teardown:Some(FunctionId(1))}];module
    }
    #[test]
    fn resumable_environment_keeps_synchronous_package_capture_flags_and_real_teardown() {
        let module=module();let limits=zil_base::Limits::default();let layouts=&LayoutContext::for_test_module(&module,&limits).unwrap();
        assert_eq!(closure(&module,&Type::Closure(ClosureId(0)),layouts.meter()).unwrap(),Some(ClosureId(0)));
        let generated=functions(layouts,&module,ClosureId(0),&limits,layouts.meter()).unwrap();
        let poll=generated.iter().find_map(|item|match item {m::Item::Function(function)if function.name==symbol(ClosureId(0),"poll")=>Some(function),_=>None}).unwrap();
        assert!(poll.blocks.as_ref().unwrap().iter().flat_map(|block|&block.instructions).any(|instruction|matches!(&instruction.op,m::Op::Call{callee,args,..}if *callee==m::global(resource_closures::capture_drop_symbol(ClosureId(0),1))&&args[0].value==loc("env"))));
        let captures=resource_closures::drop_functions(layouts,&module.closures,&module.resources).unwrap();
        let first=captures.0.iter().position(|event|matches!(event,m::Event::Begin(function)if function.name==resource_closures::capture_drop_symbol(ClosureId(0),1))).unwrap();
        let end=first+captures.0[first..].iter().position(|event|matches!(event,m::Event::End)).unwrap();
        assert!(captures.0[first..end].iter().any(|event|matches!(event,m::Event::Instruction(m::Inst{op:m::Op::CondBranch{yes,..},..})if yes=="field.drop.1.0")),"a taken Package child must still be skipped by its own flag");
        assert!(captures.0[first..end].iter().any(|event|matches!(event,m::Event::Instruction(m::Inst{op:m::Op::Call{callee,..},..})if *callee==m::global("zil.fn.1".into()))));
        let mut wrong=module.clone();wrong.functions[1].call_kind=zil_ir::CallKind::Suspend;
        assert!(closure(&wrong,&Type::Closure(ClosureId(0)),layouts.meter()).is_err());
        let mut missing=module.clone();missing.resources.pop();
        assert!(closure(&missing,&Type::Closure(ClosureId(0)),layouts.meter()).is_err());
    }
    #[test]
    fn canonical_composite_async_debt_never_silently_selects_an_ordinary_helper() {
        let mut module=module();let limits=zil_base::Limits::default();let meter=Meter::with_limits(limits.max_analysis_steps,&limits);
        let task=Type::Managed(Box::new(Type::TaskI32));
        for ty in [Type::Tuple(vec![task.clone()]),Type::Array{element:Box::new(task.clone()),length:1},Type::Enum{name:"UnknownActive".into(),variants:vec![("Empty".into(),vec![]),("Live".into(),vec![task.clone()])]},Type::ErrorUnion{error:Box::new(Type::Unit),result:Box::new(task.clone())}] {
            assert!(closure(&module,&ty,&meter).is_err());
            module.closures[0].captures[1]=ty;
            assert!(closure(&module,&Type::Closure(ClosureId(0)),&meter).is_err());
        }
        assert_eq!(closure(&module,&task,&meter).unwrap(),None,"the dedicated canonical Task route remains available");
        let borrowed=Type::Address{mutable:false,inner:Box::new(task.clone())};
        assert_eq!(closure(&module,&borrowed,&meter).unwrap(),None);
        assert_eq!(closure(&module,&Type::Array{element:Box::new(task),length:0},&meter).unwrap(),None,"no runtime element exists in a zero-length owner");
    }
}
