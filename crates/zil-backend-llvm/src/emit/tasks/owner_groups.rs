//! Resumable cleanup of one real inline environment responsibility.
//! Capture flags are the cursor: Pending preserves every remaining flag and
//! the enclosing owner node. A finished leaf is never visited again.
use super::*;

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
    match ty {
        Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. } | Type::TaskI32 | Type::AsyncRegistration { .. }) => Ok(true),
        Type::Closure(id) => {
            let d = definition(module, *id)?;
            let mut found = false;
            for capture in &d.captures { found |= scan(module, capture, depth + 1, meter)?; }
            Ok(found)
        }
        // Borrowed heads do not own their pointees and cannot request close.
        Type::Address { .. } | Type::Desc(_) => Ok(false),
        _ => Ok(false),
    }
}

fn validate(module: &TypedModule, id: ClosureId, depth: usize, meter: &Meter) -> CompileResult<()> {
    meter.spend(1)?;
    if depth >= 64 { return unsupported("cleanup group is recursive or too deep", None); }
    for ty in &definition(module, id)?.captures {
        meter.spend(1)?;
        match ty {
            Type::Closure(child) => validate(module, *child, depth + 1, meter)?,
            Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. } | Type::TaskI32 | Type::AsyncRegistration { .. }) => {},
            Type::Managed(payload) if payload.is_copy_value() => {
                let mut selected = None;
                for resource in &module.resources {
                    meter.spend(1)?;
                    if resource.payload == **payload && selected.replace(resource).is_some() {
                        return invalid("E-CODEGEN-TASK", "cleanup group resource definition is ambiguous", None);
                    }
                }
                let Some(resource) = selected else {
                    return invalid("E-CODEGEN-TASK", "cleanup group resource definition is absent", None);
                };
                if let Some(drop) = resource.teardown {
                    let actual = module.functions.get(drop.0 as usize).ok_or_else(|| {
                        vec![Diagnostic::error(Phase::Codegen,"E-CODEGEN-TASK",None,"cleanup group teardown is absent")]
                    })?;
                    if actual.call_kind != zil_ir::CallKind::Direct || actual.foreign.is_some() {
                        return unsupported("cleanup group needs an actual synchronous ordinary teardown", None);
                    }
                }
            },
            Type::Address { .. } | Type::Desc(_) => {},
            other if other.is_copy_value() => {},
            _ => return unsupported("nested cleanup requires its actual physical leaf/flag catalogue", None),
        }
    }
    Ok(())
}

pub(super) fn closure(module: &TypedModule, ty: &Type, meter: &Meter) -> CompileResult<Option<ClosureId>> {
    let Type::Closure(id) = ty else { return Ok(None); };
    if !scan(module, ty, 0, meter)? { return Ok(None); }
    validate(module, *id, 0, meter)?;
    Ok(Some(*id))
}

fn flag(v: &mut Vec<m::Inst>, d: &ClosureDefinition, index: usize) {
    let position = d.captures.len() + d.captures[..index].iter().map(resource_closures::flag_count).sum::<usize>();
    gep(v, &format!("flag.{index}"), &environment_type(d.id), loc("env"), position as u32);
    load(v, &format!("live.{index}"), m::Ty::Int(1), loc(format!("flag.{index}")));
}
fn clear(v: &mut Vec<m::Inst>, index: usize) {
    store(v, m::Ty::Int(1), m::val(false), loc(format!("flag.{index}")));
}
fn next(index: usize) -> String { format!("check.{}", index + 1) }

fn functions(module: &TypedModule, id: ClosureId, limits: &zil_base::Limits, meter: &Meter) -> CompileResult<Vec<m::Item>> {
    let d = definition(module, id)?;
    meter.spend(d.captures.len().saturating_mul(256).saturating_add(128))?;
    let mut poll = vec![block("entry", vec![inst(None, m::Op::Branch("check.0".into()))])];
    let mut request = vec![block("entry", vec![inst(None, m::Op::Branch("check.0".into()))])];
    for (index, ty) in d.captures.iter().enumerate() {
        let check = format!("check.{index}");
        let body = format!("leaf.{index}");
        let finish = format!("finish.{index}");
        if resource_closures::flag_count(ty) == 0 {
            poll.push(block(&check, vec![inst(None,m::Op::Branch(next(index)))]));
            request.push(block(&check, vec![inst(None,m::Op::Branch(next(index)))]));
            continue;
        }
        let mut guarded = vec![];
        flag(&mut guarded, d, index);
        cond(&mut guarded, &format!("live.{index}"), &body, &next(index));
        poll.push(block(&check, guarded.clone()));
        request.push(block(&check, guarded));
        let mut p = vec![];
        // Every leaf has its own SSA spelling within this helper.
        let payload = format!("payload.{index}");
        gep(&mut p, &payload, &environment_type(id), loc("env"), index as u32);
        let mut r = p.clone();
        match ty {
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
                if let Some(drop) = drop_plan::task_leaf(ty,&module.resources,limits,meter)? {
                    call(&mut p,None,m::Ty::Void,m::global(drop),vec![m::Arg::new(m::Ty::Ptr,loc(&payload))]);
                }
                branch(&mut p,&finish);
                poll.push(block(&body,p));
                p=vec![];
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

pub(super) fn helpers(verified: &VerifiedModule, meter: &Meter) -> CompileResult<Vec<m::Item>> {
    let module=verified.module();
    let mut needed=BTreeSet::new();
    let mut pending=Vec::new();
    for f in &module.functions {
        if let Some(ownership)=verified.task_ownership(f.id) {
            for owner in ownership.owners() {
                if let Some(id)=closure(module,owner.ty(),meter)? { pending.push(id); }
            }
        }
    }
    let mut result=vec![];
    while let Some(id)=pending.pop() {
        meter.spend(needed.len().saturating_add(1))?;
        if !needed.insert(id) {continue;}
        for ty in &definition(module,id)?.captures {
            if let Some(child)=closure(module,ty,meter)? { pending.push(child); }
        }
        result.extend(functions(module,id,verified.limits(),meter)?);
    }
    Ok(result)
}
