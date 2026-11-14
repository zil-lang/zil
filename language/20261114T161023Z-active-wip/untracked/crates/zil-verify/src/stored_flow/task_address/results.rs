//! Universal retained result source uppers, independently checked against each
//! worker's actual return summary. No local-worker identity is guessed at poll.
use super::*;

struct Output {
    path: Vec<ContractProjection>,
    leaf: Type,
    mode: StoredBorrowMode,
    binder: u32,
}
struct Candidate {
    port: u32,
}

fn outputs(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<Vec<Output>> {
    let (result, context) = task_type(ty).ok_or_else(|| err("Task result upper selects another carrier"))?;
    crate::tasks::charge_result_selection(ty,b)?;
    let task = Type::Task { result: Box::new(result.clone()), retained_life: context.clone() };
    let life = crate::semantic::task_domains::result_life(module, &task, b)?;
    let mut out=Vec::new();
    for (leaf,mode,class,path) in crate::semantic::task_domains::raw_input_ports(module,result,&life,b)? {
        b.spend(context.retained.len().saturating_add(path.len()))?;
        let mut rows=context.retained.iter().filter(|row|row.path==[LifeTypeProjection::Result] && row.class==class);
        let row=rows.next().ok_or_else(||err("Task result occurrence loses its full child-class map"))?;
        if rows.next().is_some(){return Err(err("Task result occurrence has an ambiguous child-class map"));}
        out.push(Output{path,leaf,mode,binder:row.binder});
    }
    Ok(out)
}

fn candidates(context:&zil_ir::RetainedLifeShape,output:&Output,b:&mut Budget)->Check<Vec<Candidate>> {
    let mut out=Vec::new();
    for (ordinal,port) in context.ports.iter().enumerate(){
        b.spend(1)?;
        type_work(&port.leaf,&b.limits.clone(),b)?;
        type_work(&output.leaf,&b.limits.clone(),b)?;
        if port.binder!=output.binder || port.mode!=output.mode || port.leaf!=output.leaf {continue;}
        if out.len()>=b.limits.max_nodes{return Err(limit("Task result source upper inventory exceeded"));}
        out.push(Candidate{port:ordinal as u32});
    }
    Ok(out)
}

fn physical(path:&[ContractProjection],b:&mut Budget)->Check<Path>{
    b.spend(path.len())?;
    path.iter().map(|part|match part{
        ContractProjection::Place(field)=>Ok(*field),
        ContractProjection::Capture(index)=>Ok(PlaceProjection::Field(*index)),
        _=>Err(err("Task result requires its exact opaque port namespace")),
    }).collect()
}

pub(super) fn check_worker(
    module:&TypedModule,worker:&TypedFunction,task:&Type,bindings:&[RetainedPortBinding],summary:&Summary,b:&mut Budget,
)->Check<bool>{
    let (_,context)=task_type(task).ok_or_else(||err("Task return proof has another result carrier"))?;
    let output=outputs(module,task,b)?;
    let mut complete=output.is_empty() || summary.complete;
    for row in &output {
        let uppers=candidates(context,row,b)?;
        if uppers.is_empty(){return Err(err("Task result has no typed retained input source upper"));}
        let mut allowed=Vec::new();
        for upper in &uppers {
            let binding=bindings.get(upper.port as usize).ok_or_else(||err("Task result source upper has no actual constructor binding"))?;
            let source=inputs::relative(module,worker,&binding.source,b)?;
            allowed.push(source);
        }
        let sources=if matches!(row.path.last(),Some(ContractProjection::RetainedPort(_))) {
            if let Some((_,sources))=summary.task_ports.iter().find(|(path,_)|*path==row.path){Some(sources.as_slice())}
            else if let Some((_,port))=summary.dyn_ports.0.iter().find(|(path,_)|*path==row.path){
                complete &= port.known; Some(port.permission.sources.as_slice())
            }else{None}
        }else{
            let path=physical(&row.path,b)?;
            summary.result.iter().find(|(known,_)|*known==path).map(|(_,sources)|sources.as_slice())
        };
        let Some(sources)=sources else{
            if !matches!(row.path.last(),Some(ContractProjection::RetainedPort(_))) {
                let path=physical(&row.path,b)?;
                summary.returned_enum_variants.work(b)?;
                if summary.returned_enum_variants.grounded(&worker.return_type,b)? && !summary.returned_enum_variants.slot_active(&path) {continue;}
            }
            complete=false;continue;
        };
        complete &= !sources.is_empty();
        for source in sources {
            source_work(std::slice::from_ref(source),b)?;
            if !contains_target(&allowed,source,b)?{
                return Err(err("Task result source exceeds its complete typed input-port upper"));
            }
        }
    }
    Ok(complete)
}

pub(super) fn poll(
    module:&TypedModule,task:&Type,payload:&Payload,out:&mut Payload,b:&mut Budget,
)->Check<()> {
    let (result,context)=task_type(task).ok_or_else(||err("Task poll result upper has another carrier"))?;
    let ready=[PlaceProjection::ErrorArm(false),PlaceProjection::Variant(VariantProjection{variant:1,index:0})];
    for row in outputs(module,task,b)? {
        let uppers=candidates(context,&row,b)?;
        if uppers.is_empty(){return Err(err("incoming Task result omits its typed retained-port source upper"));}
        let mut permission=Permission::default();
        for upper in uppers {
            let actual=select(payload,&[],upper.port,b)?;
            join_permission(&mut permission,&actual,b)?;
        }
        if matches!(row.path.last(),Some(ContractProjection::RetainedPort(_))) {
            let carrier=&row.path[..row.path.len()-1];
            let endpoint=carriers::endpoint(module,result,carrier,b)?;
            let mut path=ready.iter().copied().map(ContractProjection::Place).collect::<Vec<_>>();
            b.spend(path.len().saturating_add(row.path.len()))?;path.extend(&row.path);
            match endpoint {
                Type::Task{..}=>{insert(out,&path,&permission,b)?;}
                Type::Dyn{..}|Type::OwnedDyn{..}=>return Err(err("Task Dyn result requires its complete actual provider/head dependency transport")),
                _=>return Err(err("Task result opaque source has no actual manager transport")),
            }
        }else{
            let mut path=ready.to_vec();path.extend(physical(&row.path,b)?);add_leaf(out,&path,&permission,b)?;
        }
    }
    out.enum_variants=enum_address::Variants::runtime_poll(b)?;
    Ok(())
}
