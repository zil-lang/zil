//! Kernel reconstruction of the stable owning retained callback boundary.
use super::*;
mod phase;
mod threaded;
pub(super) fn check_phase(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    if budget.protocol_contracts || budget.protocol_checked {
        phase::check(module, budget)
    } else {
        Ok(())
    }
}
pub(super) const PREFIX: &str = "@builtin:Registration:";
pub(super) const ASYNC_PREFIX: &str = "@builtin:AsyncRegistration:";
fn error(span: Option<Span>, s: &str) -> Diagnostic {
    reject("E-VERIFY-REGISTRATION", span, s)
}
pub(super) fn carrier(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    budget.registrations = true;
    let (Type::Managed(p) | Type::Desc(p)) = ty else {
        return Err(error(span, "registration requires an owner or descriptor"));
    };
    let (Type::Registration { environment } | Type::AsyncRegistration { environment }) = p.as_ref()
    else {
        return Err(error(span, "sealed registration payload is absent"));
    };
    let Type::Closure(id) = environment.as_ref() else {
        return Err(error(
            span,
            "registration environment must be an actual concrete closure",
        ));
    };
    budget.spend(module.resources.len().saturating_add(32))?;
    let prefix = if matches!(p.as_ref(), Type::AsyncRegistration { .. }) {
        ASYNC_PREFIX
    } else {
        PREFIX
    };
    let name = format!("{prefix}{}", id.0);
    if !module.target.is_supported_x64_llvm()
        || (module.resources.iter().filter(|r| r.payload == **p).count() != 1
            || !module
                .resources
                .iter()
                .any(|r| r.name == name && r.payload == **p && r.teardown.is_none()))
    {
        return Err(error(
            span,
            "registration lacks its exact canonical owning definition",
        ));
    }
    closed(
        module,
        *id,
        span,
        budget,
        ClosedPurpose::Registration{asynchronous:matches!(p.as_ref(), Type::AsyncRegistration { .. })},
    )
}
pub(super) fn semantic_closed_environment(module:&TypedModule,closure:ClosureId,budget:&mut Budget)->Check<()>{
    closed(module,closure,None,budget,ClosedPurpose::Semantic)
}
pub(super) fn semantic_thread_callable(module:&TypedModule,closure:ClosureId,budget:&mut Budget)->Check<()>{
    threaded::actual(module,closure,None,budget)
}
#[derive(Clone,Copy)]
enum ClosedPurpose{Semantic,Registration{asynchronous:bool}}
impl ClosedPurpose{fn semantic(self)->bool{matches!(self,Self::Semantic)}fn asynchronous(self)->bool{matches!(self,Self::Registration{asynchronous:true})}}
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(crate) enum ClosedTaskOccurrence{
    Capture{closure:ClosureId,position:StoragePosition,task:Type},
    Value{function:FunctionId,value:ValueId,path:Vec<ContractProjection>,task:Type},
}
fn defer_closed_task(module:&TypedModule,task:&Type,source:ClosedTaskOccurrence,b:&mut Budget)->Check<()>{
    let Type::Task{retained_life,..}=task else{return Err(error(None,"closed Task source changes its complete actual kind"));};
    if retained_life.binder_count!=0||crate::semantic::task_domains::task_metadata_domain(module,task,b)?!=0{return Err(error(None,"Task ClosedEnvironment retains a free/phantom class or physical port"));}
    dynamic_metadata::ty(task,b)?;
    b.spend(b.semantic_closed_task_captures.len().checked_ilog2().map_or(1,|n|n as usize+1))?;
    if b.semantic_closed_task_captures.len()>=b.limits.max_nodes{return Err(limit("Task ClosedEnvironment source inventory exceeded"));}
    b.semantic_closed_task_captures.insert(source);b.task_origin_roots=true;Ok(())
}
fn task_leaf<'a>(module:&'a TypedModule,mut ty:&'a Type,path:&[ContractProjection],b:&mut Budget)->Check<&'a Type>{
    for step in path{
        b.spend(1)?;
        while let Type::BorrowedStorage{storage,..}|Type::Managed(storage)=ty{ty=storage;}
        ty=match(step,ty){
            (ContractProjection::Place(PlaceProjection::Field(index)),Type::Tuple(fields))=>fields.get(*index as usize),
            (ContractProjection::Place(PlaceProjection::Field(index)),Type::Record{fields,..}|Type::Package{fields,..})=>fields.get(*index as usize).map(|(_,ty)|ty),
            (ContractProjection::Place(PlaceProjection::Field(index)),Type::Array{element,length})if *index<*length=>Some(element.as_ref()),
            (ContractProjection::Place(PlaceProjection::ErrorArm(failed)),Type::ErrorUnion{error,result})=>Some(if *failed{error.as_ref()}else{result.as_ref()}),
            (ContractProjection::Place(PlaceProjection::Variant(row)),Type::Enum{variants,..})=>variants.get(row.variant as usize).and_then(|(_,fields)|fields.get(row.index as usize)),
            (ContractProjection::Capture(index),Type::Closure(id))=>crate::closures::definition(module,*id,None)?.captures.get(*index as usize),
            _=>None,
        }.ok_or_else(||error(None,"Task ClosedEnvironment source crosses another complete actual value selector"))?;
    }
    if let Type::Managed(inner)=ty{ty=inner;}
    if !matches!(ty,Type::Task{..}){return Err(error(None,"Task ClosedEnvironment source selects another complete actual leaf"));}
    Ok(ty)
}
fn endpoint_workers(module:&TypedModule,function:FunctionId,value:ValueId,path:&[ContractProjection],task:&Type,b:&mut Budget)->Check<BTreeSet<FunctionId>>{
    let source=module.functions.get(function.0 as usize).and_then(|function|function.values.get(value.0 as usize)).ok_or_else(||error(None,"Task ClosedEnvironment actual SSA is absent"))?;
    dynamic_metadata::ty(source,b)?;
    if task_leaf(module,source,path,b)?!=task{return Err(error(None,"Task ClosedEnvironment source changes the complete retained/result type"));}
    crate::tasks::closed_frame_workers(module,function,value,path,b)
}
fn capture_workers(module:&TypedModule,closure:ClosureId,position:&StoragePosition,task:&Type,b:&mut Budget)->Check<BTreeSet<FunctionId>>{
    let definition=crate::closures::definition(module,closure,None)?;
    let StorageBase::Capture(index)=position.base else{return Err(error(None,"Task ClosedEnvironment capture source has another original role"));};
    let root=definition.captures.get(index as usize).ok_or_else(||error(None,"Task ClosedEnvironment capture ordinal is absent"))?;
    if task_leaf(module,root,&position.path,b)?!=task{return Err(error(None,"Task ClosedEnvironment capture selector changes its complete child"));}
    let mut workers=BTreeSet::new();
    for(function,value)in crate::tasks::closed_capture_sources(module,closure,index,b)?{
        workers.extend(endpoint_workers(module,function,value,&position.path,task,b)?);
    }
    Ok(workers)
}

/// Structural data qualification of an actual frame input/result. It does not
/// inspect the executable worker graph or grant synchronous retirement.
fn closed_frame_data(module:&TypedModule,ty:&Type,source:Option<(FunctionId,ValueId,Vec<ContractProjection>)>,b:&mut Budget)->Check<()>{
    let mut work=vec![(ty,source,0usize)];
    while let Some((ty,source,depth))=work.pop(){
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("closed frame data depth exceeded"));}
        let descend=|step:PlaceProjection,b:&mut Budget|->Check<Option<(FunctionId,ValueId,Vec<ContractProjection>)>>{if let Some((function,value,path))=&source{b.spend(path.len().saturating_add(1))?;let mut path=path.clone();path.push(ContractProjection::Place(step));Ok(Some((*function,*value,path)))}else{Ok(None)}};
        match ty{
            Type::Task{..}=>{
                let(function,value,path)=source.ok_or_else(||unsupported("phantom Task frame data requires its explicit closed-environment premise",None))?;
                dynamic_metadata::ty(ty,b)?;
                defer_closed_task(module,ty,ClosedTaskOccurrence::Value{function,value,path,task:ty.clone()},b)?;
            },
            Type::Closure(id)=>semantic_closed_environment(module,*id,b)?,
            Type::Managed(inner)|Type::Opaque{hidden:inner,..}=>work.push((inner,source,depth+1)),
            Type::BorrowedStorage{storage,life}=>{if life.binder_count!=0{return Err(error(None,"closed Task frame retains an independent nominal/phantom class"));}work.push((storage,source,depth+1));},
            Type::Registration{environment}|Type::AsyncRegistration{environment}=>work.push((environment,None,depth+1)),
            Type::Tuple(fields)=>{b.spend(fields.len())?;for(index,child)in fields.iter().enumerate(){work.push((child,descend(PlaceProjection::Field(index as u32),b)?,depth+1));}},
            Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;for(index,(_,child))in fields.iter().enumerate(){work.push((child,descend(PlaceProjection::Field(index as u32),b)?,depth+1));}},
            Type::Array{element,length}=>{if *length as usize>b.limits.max_nodes{return Err(limit("closed Task frame array inventory exceeded"));}b.spend(*length as usize)?;for index in 0..*length{work.push((element,descend(PlaceProjection::Field(index),b)?,depth+1));}},
            Type::Enum{variants,..}=>{for(variant,(_,fields))in variants.iter().enumerate(){b.spend(fields.len())?;for(index,child)in fields.iter().enumerate(){work.push((child,descend(PlaceProjection::Variant(VariantProjection{variant:variant as u32,index:index as u32}),b)?,depth+1));}}},
            Type::ErrorUnion{error,result}=>{work.push((error,descend(PlaceProjection::ErrorArm(true),b)?,depth+1));work.push((result,descend(PlaceProjection::ErrorArm(false),b)?,depth+1));},
            Type::OwnedStore{element}|Type::Mutex{payload:element}=>work.push((element,None,depth+1)),
            Type::BorrowedFunctionPointer{life,..}if life.fixed_binder_count==0=>{},
            Type::FunctionPointer{..}|Type::ForeignCallback{..}|Type::Unit|Type::Bool|Type::I8|Type::I16|Type::I32|Type::I64|Type::Isize|Type::U8|Type::U16|Type::U32|Type::U64|Type::Usize|Type::F32|Type::F64|Type::ForeignHandle|Type::CWideString|Type::ByteBuffer|Type::TaskI32=>{},
            _=>return Err(error(None,"Task ClosedEnvironment frame data retains a borrowed, unresolved or unsupported hidden position")),
        }
    }Ok(())
}

/// Runs after the complete constructor/source, Static, worker and resource
/// passes. Pending requests remain exact occurrences, never a same-Type frame
/// cache. Every real construction and replacement contributes its own source.
pub(crate) fn finish_semantic_closed_tasks(module:&TypedModule,b:&mut Budget)->Check<()>{
    let mut completed=BTreeSet::new();let mut checked_workers=BTreeSet::new();
    loop{
        b.spend(b.semantic_closed_task_captures.len().saturating_add(completed.len()))?;
        let pending=std::mem::take(&mut b.semantic_closed_task_captures);
        let copied:Check<Option<ClosedTaskOccurrence>>=(||{
            let Some(row)=pending.iter().find(|row|!completed.contains(*row))else{return Ok(None);};
            match row{ClosedTaskOccurrence::Capture{position,task,..}=>{dynamic_metadata::position(position,b)?;dynamic_metadata::ty(task,b)?;},ClosedTaskOccurrence::Value{path,task,..}=>{b.spend(path.len().saturating_add(1))?;dynamic_metadata::ty(task,b)?;}}
            Ok(Some(row.clone()))
        })();
        b.semantic_closed_task_captures=pending;
        let Some(request)=copied?else{break;};
        let workers=match &request{
            ClosedTaskOccurrence::Capture{closure,position,task}=>capture_workers(module,*closure,position,task,b)?,
            ClosedTaskOccurrence::Value{function,value,path,task}=>endpoint_workers(module,*function,*value,path,task,b)?,
        };
        completed.insert(request);
        for worker in workers{
            if !checked_workers.insert(worker){continue;}
            b.spend(b.task_constructors.len())?;
            let copied_keys=b.task_constructors.values().filter(|row|row.key.worker==worker).fold(0usize,|n,row|n.saturating_add(row.frames.len()).saturating_add(1));b.spend(copied_keys)?;
            let mut sites=Vec::new();
            for row in b.task_constructors.values().filter(|row|row.key.worker==worker){
                if row.state!=crate::semantic::task_domains::ObligationState::Discharged||!row.raw_checked||module.semantic.is_some()&&!row.semantic_checked{return Err(error(None,"Task ClosedEnvironment constructor source has no completed exact-site proof"));}
                sites.push((row.key,row.frames.clone()));
            }
            if sites.is_empty(){return Err(error(None,"Task ClosedEnvironment source worker lacks its real constructor obligations"));}
            let frames=crate::semantic::task_domains::metadata_candidates(worker,b)?;
            for(site,keys)in sites{
                let caller=module.functions.get(site.function.0 as usize).ok_or_else(||error(None,"closed Task constructor source function is absent"))?;
                let instruction=caller.blocks.iter().find(|block|block.id==site.block).and_then(|block|block.instructions.get(site.instruction as usize)).ok_or_else(||error(None,"closed Task constructor source instruction is absent"))?;
                let arguments=match &instruction.kind{InstructionKind::TaskCreate{worker:actual,arguments,..}|InstructionKind::TaskCreateOwned{worker:actual,arguments,..}if *actual==worker&&instruction.result==Some(site.result)=>arguments,_=>return Err(error(None,"closed Task source changes its exact actual constructor role"))};
                if keys.is_empty(){return Err(error(None,"Task ClosedEnvironment constructor has no full original frame world"));}
                for key in keys{
                    b.spend(frames.len().saturating_add(1))?;
                    let frame=frames.iter().find(|frame|frame.key==key).ok_or_else(||error(None,"Task ClosedEnvironment source frame remains Building/absent"))?;
                    if frame.key.worker!=worker||frame.shape.binder_count!=0||module.semantic.is_some()&&!frame.worker_checked||frame.parameter_types.len()!=arguments.len(){return Err(error(None,"Task ClosedEnvironment source frame changes its full checked worker/input/context domain"));}
                    for(argument,ty)in arguments.iter().zip(&frame.parameter_types){dynamic_metadata::ty(ty,b)?;closed_frame_data(module,ty,Some((caller.id,*argument,vec![])),b)?;}
                    for actual in &frame.arguments{match actual{
                        crate::semantic::api::CheckedActual::Type(ty)|crate::semantic::api::CheckedActual::Const{carrier:ty,..}=>{if ty.life.binder_count!=0{return Err(error(None,"Task ClosedEnvironment retains a complete phantom actual domain"));}closed_frame_data(module,&ty.ty,None,b)?;},
                        crate::semantic::api::CheckedActual::Life(_)=>return Err(error(None,"Task ClosedEnvironment retains an original Life actual")),
                    }}
                    let entry=module.functions.get(worker.0 as usize).ok_or_else(||error(None,"closed Task frame worker entry is absent"))?;
                    let mut returns=false;
                    for block in &entry.blocks{if let Terminator::Return(Some(value))=block.terminator{returns=true;closed_frame_data(module,&frame.result_type,Some((worker,value,vec![])),b)?;}}
                    if !returns{closed_frame_data(module,&frame.result_type,None,b)?;}
                }
            }
        }
    }
    b.semantic_closed_task_captures.clear();Ok(())
}

fn closed(
    module: &TypedModule,
    root: ClosureId,
    span: Option<Span>,
    budget: &mut Budget,
    purpose: ClosedPurpose,
) -> Check<()> {
    let mut closure_queue = vec![root];
    let mut closures_seen = BTreeSet::new();
    let mut code_queue = Vec::new();
    while let Some(id) = closure_queue.pop() {
        budget.spend(closures_seen.len().saturating_add(1))?;
        if !closures_seen.insert(id) {
            continue;
        }
        let c = closures::definition(module, id, span)?;
        budget.spend(
            c.captures
                .len()
                .saturating_add(c.lending_captures.len())
                .saturating_add(1),
        )?;
        if !c.lending_captures.is_empty() {
            return Err(error(
                span,
                "ClosedEnvironment cannot hold borrowed capture positions",
            ));
        }
        resources::charge_payload(&c.result, budget)?;
        for parameter in &c.parameters {
            resources::charge_payload(parameter, budget)?;
        }
        let entry = module
            .functions
            .get(c.function.0 as usize)
            .ok_or_else(|| error(span, "registration entry is absent"))?;
        budget.spend(c.parameters.len().saturating_add(1))?;
        if entry.environment != Some(id)
            || !purpose.semantic()&&entry.call_kind!=CallKind::Direct
            || entry.foreign.is_some()
            || entry.parameters.len() != c.parameters.len()
            || entry.return_type != c.result
            || !purpose.semantic()&&entry.result_mode != PassingMode::Infer
        {
            return Err(error(
                span,
                "registration closure differs from its actual entry",
            ));
        }
        for (local, t) in entry.parameters.iter().zip(&c.parameters) {
            if local_type(entry, *local, span)? != t {
                return Err(error(span, "registration actual entry parameter differs"));
            }
        }
        if !purpose.semantic(){code_queue.push(c.function);}
        if let Some(header)=c.storage_life.as_ref().filter(|header|!header.static_bindings.is_empty()){
            if header.fixed_binder_count!=0{return Err(error(span,"Static ClosedEnvironment cannot erase free or phantom classes"));}
            signature_life::closure_relation_resource_scopes(module,&c.parameters,&c.captures,c.kind,stored_callables::life_result(c),header,span,budget)?;
        }
        let mut fields=c.captures.iter().enumerate().map(|(index,ty)|(ty,Some(StoragePosition{base:StorageBase::Capture(index as u32),path:vec![]}),false)).collect::<Vec<_>>();
        while let Some((t,position,static_pointee))=fields.pop(){
            budget.spend(1)?;
            match t{
                Type::Address{mutable:false,inner}if !static_pointee&&position.as_ref().is_some_and(|position|c.storage_life.as_ref().is_some_and(|header|header.static_bindings.contains(position)))=>{
                    crate::semantic::task_domains::defer_static_capture(module,c.id,position.as_ref().unwrap(),budget)?;
                    fields.push((inner,None,true));
                },
                Type::Slice{element}if static_pointee=>fields.push((element,None,true)),
                Type::AsyncRegistration { environment }if purpose.semantic()=>fields.push((environment,None,false)),
                Type::AsyncRegistration { .. } => {
                    return Err(unsupported(
                        "ClosedEnvironment cannot retain a pollable registration with asynchronous teardown",
                        span,
                    ));
                }
                Type::Address { .. }
                | Type::Dyn { .. }
                | Type::Desc(_)
                | Type::Slice { .. }
                | Type::Named(_)
                | Type::Unsupported(_)
                | Type::SyncCell(_) => {
                    return Err(error(
                        span,
                        "ClosedEnvironment contains an external loan or unresolved position",
                    ));
                }
                Type::TaskI32 if purpose.asynchronous() => {
                    return Err(error(
                        span,
                        "AsyncRegistration environment lacks an actual synchronous legacy Task frame cleanup proof",
                    ));
                }
                Type::Task { .. }if purpose.semantic()=>{
                    let position=position.ok_or_else(||unsupported("closed Task capture lacks its complete actual source selector",span))?;
                    dynamic_metadata::ty(t,budget)?;
                    defer_closed_task(module,t,ClosedTaskOccurrence::Capture{closure:id,position,task:t.clone()},budget)?;
                },
                Type::Task { .. } => {
                    return Err(error(
                        span,
                        "a task result type does not prove its hidden frame ClosedEnvironment",
                    ));
                }
                Type::Closure(id) => {
                    budget.spend(1)?;
                    closure_queue.push(*id)
                }
                Type::Managed(p) => {
                    budget.spend(module.resources.len())?;
                    for resource in &module.resources {
                        resources::charge_payload(&resource.payload, budget)?;
                        if resource.payload == **p
                            && let Some(teardown) = resource.teardown
                        {
                            code_queue.push(teardown);
                        }
                    }
                    budget.spend(1)?;
                    fields.push((p,if purpose.semantic(){position}else{None},static_pointee));
                }
                Type::BorrowedStorage{storage:p,life}=>{
                    if life.binder_count!=0{return Err(error(span,"ClosedEnvironment retains an independent nominal or phantom class"));}
                    budget.spend(1)?;fields.push((p,position,static_pointee));
                },
                Type::Opaque{hidden:p,..}|Type::OwnedStore{element:p}|Type::Registration{environment:p}=>{budget.spend(1)?;fields.push((p,None,static_pointee));},
                Type::Array{element,length}=>{
                    if *length as usize>budget.limits.max_nodes{return Err(limit("closed capture array directory exceeded"));}budget.spend(*length as usize)?;
                    for index in 0..*length{fields.push((element,closed_field_position(&position,PlaceProjection::Field(index as u32),budget)?,static_pointee));}
                },
                Type::Tuple(ts)=>{budget.spend(ts.len())?;for(index,t)in ts.iter().enumerate(){fields.push((t,closed_field_position(&position,PlaceProjection::Field(index as u32),budget)?,static_pointee));}},
                Type::Package{fields:fs,..}|Type::Record{fields:fs,..}=>{budget.spend(fs.len())?;for(index,(_,t))in fs.iter().enumerate(){fields.push((t,closed_field_position(&position,PlaceProjection::Field(index as u32),budget)?,static_pointee));}},
                Type::Enum{variants,..}=>{budget.spend(variants.len())?;for(variant,(_,ts))in variants.iter().enumerate(){budget.spend(ts.len())?;for(index,t)in ts.iter().enumerate(){fields.push((t,closed_field_position(&position,PlaceProjection::Variant(VariantProjection{variant:variant as u32,index:index as u32}),budget)?,static_pointee));}}},
                Type::ErrorUnion{error,result}=>{budget.spend(2)?;fields.push((error,closed_field_position(&position,PlaceProjection::ErrorArm(true),budget)?,static_pointee));fields.push((result,closed_field_position(&position,PlaceProjection::ErrorArm(false),budget)?,static_pointee));}
                Type::BorrowedFunctionPointer { .. }
                | Type::FunctionPointer { .. }
                | Type::ForeignCallback { .. }
                | Type::Unit
                | Type::Bool
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::I32
                | Type::I8
                | Type::I16
                | Type::Isize
                | Type::I64
                | Type::U64
                | Type::Usize
                | Type::F32
                | Type::F64
                | Type::ForeignHandle
                | Type::CWideString
                | Type::ByteBuffer
                | Type::TaskI32 => {}
                Type::Mutex { .. } | Type::MutexGuard { .. } | Type::OwnedDyn { .. } => {
                    return Err(unsupported(
                        "mutex capture needs a complete thread and cleanup contract",
                        span,
                    ));
                }
            }
        }
    }
    let mut code_seen = BTreeSet::new();
    while let Some(id) = code_queue.pop() {
        budget.spend(code_seen.len().saturating_add(1))?;
        if !code_seen.insert(id) {
            continue;
        }
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| error(span, "registration Direct dependency is absent"))?;
        if f.call_kind != CallKind::Direct {
            return Err(error(span, "retained callback target selects Suspend ABI"));
        }
        for b in &f.blocks {
            budget.spend(1)?;
            for i in &b.instructions {
                budget.spend(1)?;
                match i.kind {
                    InstructionKind::TaskYield | InstructionKind::SuspendCall { .. } => {
                        return Err(error(
                            Some(i.span),
                            "registration callback may only execute through Direct ABI",
                        ));
                    }
                    InstructionKind::Call { function, .. }
                    | InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function } => {
                        budget.spend(1)?;
                        code_queue.push(function)
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
fn closed_field_position(parent:&Option<StoragePosition>,step:PlaceProjection,b:&mut Budget)->Check<Option<StoragePosition>>{
    let Some(parent)=parent else{return Ok(None);};dynamic_metadata::position(parent,b)?;let mut child=parent.clone();child.path.push(ContractProjection::Place(step));Ok(Some(child))
}
pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(i.span);
    budget.registrations = true;
    budget.head_descriptions = true;
    budget.protocol_checked |= matches!(
        i.kind,
        InstructionKind::AsyncRegistrationFinishDestroyChecked { .. }
    );
    budget.spend(1)?;
    Ok(Some(match i.kind {
        InstructionKind::AsyncRegistrationStart {
            register,
            request_close,
            poll_drain,
            callback,
            ..
        } => {
            let is_threaded = module
                .functions
                .get(register.0 as usize)
                .and_then(|f| f.foreign.as_ref())
                .and_then(|f| f.scoped_callback)
                == Some(ScopedCallbackRole::ThreadAsyncRegister);
            for (id, role) in if is_threaded {
                [
                    (register, ScopedCallbackRole::ThreadAsyncRegister),
                    (request_close, ScopedCallbackRole::ThreadAsyncRequestClose),
                    (poll_drain, ScopedCallbackRole::ThreadAsyncPollDrain),
                ]
            } else {
                [
                    (register, ScopedCallbackRole::AsyncRegister),
                    (request_close, ScopedCallbackRole::AsyncRequestClose),
                    (poll_drain, ScopedCallbackRole::AsyncPollDrain),
                ]
            } {
                let provider = module
                    .functions
                    .get(id.0 as usize)
                    .ok_or_else(|| error(span, "async provider absent"))?;
                if provider.foreign.as_ref().and_then(|f| f.scoped_callback) != Some(role) {
                    return Err(error(span, "async provider has wrong actual role"));
                }
                scoped_callback::declaration(provider, budget)?;
            }
            let Type::Closure(id) = value_type(f, callback, span)? else {
                return Err(error(
                    span,
                    "async callback requires actual consumed concrete F",
                ));
            };
            let closure = closures::definition(module, *id, span)?;
            budget.spend(closure.parameters.len().saturating_add(1))?;
            if !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                || closure.parameters != [Type::I32]
                || closure.result != Type::I32
            {
                return Err(error(
                    span,
                    "async callback requires actual FnMut(i32)->i32",
                ));
            }
            if is_threaded {
                threaded::actual(module, *id, span, budget)?;
            }
            let ty = Type::Managed(Box::new(Type::AsyncRegistration {
                environment: Box::new(Type::Closure(*id)),
            }));
            carrier(module, &ty, span, budget)?;
            ty
        }
        InstructionKind::AsyncRegistrationValid { registration } => {
            let ty = value_type(f, registration, span)?;
            if !matches!(ty,Type::Desc(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})) {
                return Err(error(span, "async validity requires actual descriptor"));
            }
            carrier(module, ty, span, budget)?;
            Type::Bool
        }
        InstructionKind::AsyncRegistrationRequestClose { registration }
        | InstructionKind::AsyncRegistrationPollClose { registration } => {
            let Type::Address {
                mutable: true,
                inner,
            } = value_type(f, registration, span)?
            else {
                return Err(error(
                    span,
                    "async close requires exclusive actual head address",
                ));
            };
            if !matches!(inner.as_ref(),Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
            {
                return Err(error(
                    span,
                    "async close requires complete pollable owning head",
                ));
            }
            carrier(module, inner, span, budget)?;
            if matches!(i.kind, InstructionKind::AsyncRegistrationPollClose { .. }) {
                Type::Enum {
                    name: format!("@std.task:v1:Poll:{}", Type::Unit.fingerprint()),
                    variants: vec![
                        ("Pending".into(), vec![]),
                        ("Ready".into(), vec![Type::Unit]),
                    ],
                }
            } else {
                Type::Unit
            }
        }
        InstructionKind::AsyncRegistrationFinishDestroy { registration }
        | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
            let ty = value_type(f, registration, span)?;
            if !matches!(ty,Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})) {
                return Err(error(span, "async finish requires actual owning head"));
            }
            carrier(module, ty, span, budget)?;
            Type::Unit
        }
        InstructionKind::RegistrationStart {
            register,
            close,
            callback,
            ..
        } => {
            for (id, role) in [
                (register, ScopedCallbackRole::Register),
                (close, ScopedCallbackRole::Close),
            ] {
                let target = module
                    .functions
                    .get(id.0 as usize)
                    .ok_or_else(|| error(span, "registration static role absent"))?;
                if target.foreign.as_ref().and_then(|f| f.scoped_callback) != Some(role) {
                    return Err(error(span, "registration static target has wrong role"));
                }
                scoped_callback::declaration(target, budget)?;
            }
            tasks::require_synchronous_value(module, f, callback, budget, span)?;
            let Type::Closure(id) = value_type(f, callback, span)? else {
                return Err(error(
                    span,
                    "registration requires actual consumed concrete F",
                ));
            };
            let c = closures::definition(module, *id, span)?;
            budget.spend(c.parameters.len().saturating_add(1))?;
            if !matches!(c.kind, CallableKind::Fn | CallableKind::FnMut)
                || c.parameters != [Type::I32]
                || c.result != Type::I32
            {
                return Err(error(
                    span,
                    "registration requires actual Fn/FnMut(i32)->i32",
                ));
            }
            let t = Type::Managed(Box::new(Type::Registration {
                environment: Box::new(Type::Closure(*id)),
            }));
            carrier(module, &t, span, budget)?;
            t
        }
        InstructionKind::RegistrationValid { registration } => {
            let t = value_type(f, registration, span)?;
            if !matches!(t, Type::Desc(p) if matches!(p.as_ref(),Type::Registration{..})) {
                return Err(error(
                    span,
                    "registration validity requires actual descriptor",
                ));
            };
            carrier(module, t, span, budget)?;
            Type::Bool
        }
        InstructionKind::RegistrationClose { registration } => {
            let t = value_type(f, registration, span)?;
            if !matches!(t, Type::Managed(p) if matches!(p.as_ref(),Type::Registration{..})) {
                return Err(error(
                    span,
                    "registration close consumes actual owning head",
                ));
            };
            carrier(module, t, span, budget)?;
            Type::Unit
        }
        _ => unreachable!(),
    }))
}

pub(super) fn factory(module: &TypedModule, f: &TypedFunction, budget: &mut Budget) -> Check<bool> {
    if f.callable_contract.is_some() {
        return Ok(false);
    }
    let Type::Managed(payload) = &f.return_type else {
        return Ok(false);
    };
    let (Type::Registration { environment } | Type::AsyncRegistration { environment }) =
        payload.as_ref()
    else {
        return Ok(false);
    };
    let Type::Closure(id) = environment.as_ref() else {
        return Err(error(
            Some(f.span),
            "registration factory environment absent",
        ));
    };
    budget.spend(f.parameters.len().saturating_add(1))?;
    let mut input = None;
    for (index, local) in f.parameters.iter().enumerate() {
        let t = local_type(f, *local, Some(f.span))?;
        if t == environment.as_ref() {
            if input.replace(index).is_some() {
                return Err(error(
                    Some(f.span),
                    "registration factory requires one actual F input",
                ));
            }
        } else {
            resources::charge_payload(t, budget)?;
            if !t.is_copy_value() {
                return Err(error(
                    Some(f.span),
                    "registration factory ordinary input is outside finite Copy profile",
                ));
            }
        }
    }
    if input.is_none()
        || f.foreign.is_some()
        || f.environment.is_some()
        || !matches!(f.result_mode, PassingMode::Create | PassingMode::Give)
    {
        return Err(error(
            Some(f.span),
            "registration factory must transfer actual F into create/give owner result",
        ));
    }
    closed(
        module,
        *id,
        Some(f.span),
        budget,
        ClosedPurpose::Registration{asynchronous:matches!(payload.as_ref(), Type::AsyncRegistration { .. })},
    )?;
    Ok(true)
}

/// Inventory every actual shape, including unused declarations. No transported
/// Closed/producer bit grants ordinary nested storage permission.
pub(super) fn storage(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    if !budget.async_registrations {
        return Ok(());
    }
    let mut actual_creations = Vec::new();
    if budget.owned_task_input_inventory {
        budget.spend(module.functions.len())?;
        for f in &module.functions {
            budget.spend(f.blocks.len())?;
            for i in f.blocks.iter().flat_map(|b| &b.instructions) {
                budget.spend(1)?;
                if let InstructionKind::TaskCreateOwned { arguments, .. } = &i.kind {
                    budget.spend(1)?;
                    actual_creations.push((f, arguments.as_slice()));
                }
            }
        }
    }
    let roots = module
        .resources
        .iter()
        .map(|r| (&r.payload, false, false))
        .chain(
            module
                .functions
                .iter()
                .flat_map(|f| {
                    let frame_tuple = f.call_kind == CallKind::Suspend && f.foreign.is_none();
                    f.values
                        .iter()
                        .chain(f.locals.iter().map(|l| &l.ty))
                        .map(move |t| (t, false, frame_tuple))
                        .chain(std::iter::once((&f.return_type, false, false)))
                }),
        )
        .chain(module.closures.iter().flat_map(|c| {
            c.captures.iter().map(|t| (t, true, false)).chain(
                c.parameters
                    .iter()
                    .chain(std::iter::once(&c.result))
                    .map(|t| (t, false, false)),
            )
        }));
    for (root, blocked, frame_tuple) in roots {
        let mut pending = vec![(root, blocked, frame_tuple)];
        while let Some((ty, blocked, frame_tuple)) = pending.pop() {
            budget.spend(1)?;
            if blocked && matches!(ty, Type::AsyncRegistration { .. }) {
                return Err(error(
                    None,
                    "pollable registration roots in ordinary aggregate storage or closure captures require an asynchronous ownership consumer",
                ));
            }
            let count = match ty {
                Type::Record { fields, .. } | Type::Package { fields, .. } => fields.len(),
                Type::Tuple(fields) => fields.len(),
                Type::Enum { variants, .. } => {
                    budget.spend(variants.len())?;
                    variants
                        .iter()
                        .fold(0usize, |n, (_, fields)| n.saturating_add(fields.len()))
                }
                Type::ErrorUnion { .. } => 2,
                Type::AsyncRegistration { .. }
                | Type::Registration { .. }
                | Type::BorrowedStorage { .. }
                | Type::Task { .. }
                | Type::Address { .. }
                | Type::Managed(_)
                | Type::Desc(_)
                | Type::Opaque { .. }
                | Type::Array { .. }
                | Type::Slice { .. }
                | Type::OwnedStore { .. } => 1,
                Type::FunctionPointer { parameters, .. }
                | Type::BorrowedFunctionPointer { parameters, .. }
                | Type::ForeignCallback { parameters, .. } => parameters.len().saturating_add(1),
                _ => 0,
            };
            budget.spend(count)?;
            let mut children = Vec::with_capacity(count);
            let mut blocked = blocked;
            match ty {
                Type::Package { name, fields } => {
                    blocked |= !name.starts_with("@std.task:v1:TaskCreateFailure:");
                    children.extend(fields.iter().map(|(_, t)| t));
                }
                Type::OwnedStore { element } => {
                    blocked = true;
                    children.push(element.as_ref());
                }
                Type::Record { fields, .. } => children.extend(fields.iter().map(|(_, t)| t)),
                // These are actual positions in a verified continuation. The
                // independent resource transfer still checks each live drop.
                Type::Tuple(fields) if frame_tuple => children.extend(fields),
                Type::Tuple(fields) => {
                    let mut present = false;
                    for (f, args) in &actual_creations {
                        budget.spend(fields.len().saturating_add(args.len()))?;
                        if fields.len() != args.len() {
                            continue;
                        }
                        let mut exact = true;
                        for (field, id) in fields.iter().zip(*args) {
                            let actual = f
                                .values
                                .get(id.0 as usize)
                                .ok_or_else(|| error(None, "owned input SSA absent"))?;
                            resources::charge_payload(field, budget)?;
                            resources::charge_payload(actual, budget)?;
                            exact &= field == actual;
                        }
                        present |= exact;
                    }
                    blocked |= !present;
                    children.extend(fields);
                }
                Type::Enum { variants, .. } => {
                    blocked = true;
                    for (_, fields) in variants {
                        children.extend(fields);
                    }
                }
                Type::ErrorUnion { error, result } => {
                    let admitted = (matches!(error.as_ref(),Type::Package{name,..}|Type::Record{name,..} if name.starts_with("@std.task:v1:TaskCreateFailure:"))
                        || crate::stored_flow::failure_shape(error, budget)?)
                        && matches!(result.as_ref(),Type::Managed(t) if matches!(t.as_ref(),Type::Task{..}));
                    blocked |= !admitted;
                    children.extend([error.as_ref(), result.as_ref()])
                }
                Type::AsyncRegistration { environment: inner }
                | Type::Registration { environment: inner }
                | Type::BorrowedStorage { storage: inner, .. }
                | Type::Task { result: inner, .. }
                | Type::Address { inner, .. }
                | Type::Managed(inner)
                | Type::Desc(inner)
                | Type::Opaque { hidden: inner, .. }
                | Type::Array { element: inner, .. }
                | Type::Slice { element: inner } => children.push(inner.as_ref()),
                Type::FunctionPointer { parameters, result }
                | Type::BorrowedFunctionPointer {
                    parameters, result, ..
                }
                | Type::ForeignCallback { parameters, result } => {
                    children.extend(parameters);
                    children.push(result.as_ref());
                }
                _ => {}
            }
            let frame_tuple = frame_tuple && matches!(ty, Type::Tuple(_));
            pending.extend(children.into_iter().map(|t| (t, blocked, frame_tuple)));
        }
    }
    Ok(())
}

pub(crate) fn transfer_callback(
    module: &TypedModule,
    id: ClosureId,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    threaded::actual(module, id, span, budget)
}

#[cfg(test)]
mod static_capture_tests{
    use super::*;
    #[test]
    fn closed_static_capture_keeps_exact_nested_slot_and_final_grounding_raw_wire(){
        let limits=Limits::default();
        let(mut module,_)=crate::tests::baseline();let span=module.functions[0].span;
        let leaf=Type::Address{mutable:false,inner:Box::new(Type::Slice{element:Box::new(Type::U8)})};
        let capture=Type::Tuple(vec![leaf.clone(),Type::I32]);
        let position=StoragePosition{base:StorageBase::Capture(0),path:vec![ContractProjection::Place(PlaceProjection::Field(0))]};
        let result=StoragePosition{base:StorageBase::Result,path:vec![]};
        let header=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:0,bindings:vec![],static_bindings:vec![position.clone(),result],classes:vec![]};
        let entry=&mut module.functions[0];entry.environment=Some(ClosureId(0));entry.storage_life=Some(header.clone());entry.parameters.clear();entry.locals.clear();entry.values=vec![capture.clone(),leaf.clone()];entry.return_type=leaf.clone();
        entry.blocks[0].instructions=vec![Instruction{result:Some(ValueId(0)),kind:InstructionKind::ReadCapture{index:0},span},Instruction{result:Some(ValueId(1)),kind:InstructionKind::ExtractField{aggregate:ValueId(0),index:0},span}];entry.blocks[0].terminator=Terminator::Return(Some(ValueId(1)));
        module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![capture],parameters:vec![],result:leaf,kind:CallableKind::FnOnce,storage_life:Some(header),lending_captures:vec![]});
        let mut suspended=module.clone();suspended.functions[0].call_kind=CallKind::Suspend;
        let mut data=Budget::new(&limits);semantic_closed_environment(&suspended,ClosureId(0),&mut data).unwrap();
        assert!(data.proven_static_occurrences.is_empty(),"a data CE obligation does not prove Static sources or synchronous execution");
        assert!(closed(&suspended,ClosureId(0),None,&mut Budget::new(&limits),ClosedPurpose::Registration{asynchronous:false}).is_err(),"C registration retains its independent Direct/no-future contract");
        for mutation in 0..8{
            let mut proposed=module.clone();
            match mutation{
                1=>proposed.closures[0].storage_life.as_mut().unwrap().static_bindings[0].path[0]=ContractProjection::Place(PlaceProjection::Field(1)),
                2=>{let Type::Tuple(fields)=&mut proposed.closures[0].captures[0]else{unreachable!()};let Type::Address{mutable,..}=&mut fields[0]else{unreachable!()};*mutable=true;},
                3=>proposed.functions[0].environment=None,
                4=>proposed.closures[0].storage_life=None,
                5=>{proposed.closures[0].storage_life.as_mut().unwrap().static_bindings.remove(0);},
                6=>proposed.closures[0].storage_life.as_mut().unwrap().fixed_binder_count=1,
                7=>proposed.closures[0].storage_life.as_mut().unwrap().static_bindings.insert(0,position.clone()),
                _=>{},
            }
            proposed.functions[0].storage_life=proposed.closures[0].storage_life.clone();
            let plan=CandidatePlan::empty_for(&proposed);
            for wire in [false,true]{
                let decoded;let current=if wire{let bytes=zil_ir::wire::encode_module_candidate(&proposed,&plan,&limits).unwrap();decoded=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0;&decoded}else{&proposed};
                let mut b=Budget::new(&limits);let result=semantic_closed_environment(current,ClosureId(0),&mut b);
                if mutation==0{
                    result.unwrap();assert!(b.proven_static_occurrences.is_empty());assert!(b.static_borrow_occurrences);assert!(b.semantic_static_occurrences.contains(&(ClosureId(0),position.clone())));
                    assert!(crate::semantic::callable_obligations::drain(current,&mut b).is_err(),"a complete nested Static declaration is no actual constructor/Return root proof, wire {wire}");
                }else{assert!(result.is_err(),"closed nested Static capture attack {mutation}, wire {wire}");}
            }
        }
    }
}
