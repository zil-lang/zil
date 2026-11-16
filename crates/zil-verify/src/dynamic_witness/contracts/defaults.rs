//! Canonical original defaults are signature templates, not concrete head-only
//! access bounds. Authenticate the whole template before selecting Context rows.
use super::*;
fn head(index:u32)->StoragePosition{StoragePosition{base:StorageBase::Parameter(index),path:vec![]}}
fn port(index:u32)->StoragePosition{StoragePosition{base:StorageBase::Parameter(0),path:vec![ContractProjection::RetainedPort(index)]}}

fn canonical(module:&TypedModule,owner:&SemanticDynOwner,recipe:&DynSignatureRecipe,b:&mut Budget)->Check<bool>{
    let DynTypePattern::Semantic{scope,..}=&recipe.result else{return Ok(false);};
    let original=bridge::owner(module,owner,*scope,b)?;
    let SemanticDefinitionBody::FunctionWithBody{signature,receiver_parameter,..}=&original.body else{return Ok(false);};
    if *receiver_parameter!=Some(0)||signature.parameters.len()!=recipe.parameters.len()||signature.result_mode!=recipe.result_mode||signature.call_kind!=recipe.call_kind||signature.invocation_lifetimes!=recipe.invocation_binder_count{return Ok(false);}
    dynamic_metadata::recipe(signature,b)?;
    let pattern=|ty:&SemanticTypeRecipe|DynTypePattern::Semantic{scope:*scope,ty:Box::new(ty.clone())};
    dynamic_metadata::recipe(recipe,b)?;
    if recipe.result!=pattern(&signature.result){return Ok(false);}
    b.spend(signature.parameters.len().saturating_mul(7).saturating_add(12))?;
    let mut requires=Vec::new();let mut reads=Vec::new();let mut writes=Vec::new();
    for(index,(parameter,actual))in signature.parameters.iter().zip(&recipe.parameters).enumerate(){
        if actual.mode!=parameter.mode||actual.ty!=pattern(&parameter.ty){return Ok(false);}
        if let SemanticTypeRecipe::Address{mutable,..}=&parameter.ty{let position=head(index as u32);requires.push(DynRequirementRecipe::Read(position.clone()));reads.push(position.clone());if *mutable{requires.push(DynRequirementRecipe::Write(position.clone()));writes.push(position);}}
    }
    let result=if let SemanticTypeRecipe::Address{mutable,lifetime,..}=&signature.result{
        let mode=if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress};
        let mut sources=Vec::new();
        if !*mutable{dynamic_metadata::recipe(&recipe.result,b)?;sources.push(DynStorageSourceRecipe::Static{leaf:recipe.result.clone(),mode});}
        for(index,parameter)in signature.parameters.iter().enumerate(){
            if let SemanticTypeRecipe::Address{lifetime:input,..}=&parameter.ty{
                // Original recipe identity, never equality of substituted images.
                dynamic_metadata::recipe(input,b)?;dynamic_metadata::recipe(lifetime,b)?;dynamic_metadata::recipe(&parameter.ty,b)?;dynamic_metadata::recipe(&signature.result,b)?;
                if input==lifetime{if parameter.ty==signature.result{sources.push(DynStorageSourceRecipe::Exact(head(index as u32)));}if index==0{dynamic_metadata::recipe(&recipe.result,b)?;sources.push(DynStorageSourceRecipe::OwnedView{head:head(0),leaf:recipe.result.clone(),mode});}}
            }
        }DynResultSourceRecipe::Borrow{sources,mutable:*mutable}
    }else{match signature.result_mode{PassingMode::Create=>DynResultSourceRecipe::Create,PassingMode::Give=>DynResultSourceRecipe::Give,_=>DynResultSourceRecipe::Value}};
    let cleanup=DynCleanupRecipe{environment:false,positions:vec![],access:DynCallAccess{reads:vec![],writes:vec![]},effects:vec![]};
    let c=&recipe.contract;
    Ok(c.requires.as_ref()==Some(&requires)&&c.guarantees.as_ref()==Some(&vec![])&&c.hold.as_ref()==Some(&vec![])&&c.call.as_ref()==Some(&DynCallAccess{reads,writes})&&c.result.as_ref()==Some(&result)&&c.cleanup.as_ref()==Some(&cleanup)&&c.threads.as_ref()==Some(&DynThreadRequirements{transfer:false,share:false})&&c.effects.is_some()&&c.resume.is_some())
}
fn compatible(port:&context_ports::Port,leaf:&requests::Leaf,b:&mut Budget)->Check<bool>{
    dynamic_metadata::ty(&port.leaf,b)?;dynamic_metadata::ty(&leaf.ty,b)?;
    let(outer,mode)=match &leaf.kind{requests::LeafKind::Physical{outer,mode}|requests::LeafKind::RetainedPort{outer,mode,..}=>(outer,mode),_=>return Ok(false)};
    if port.leaf!=leaf.ty||port.mode!=*mode||!matches!(outer,requests::Borrow::Variable{binder,..}if *binder==StorageBinder::Fixed(port.binder)){return Ok(false);}
    b.spend(leaf.classes.len().saturating_mul(port.inner.len()).saturating_add(1))?;
    Ok(leaf.classes.len()==port.inner.len()&&port.inner.iter().all(|(class,binder)|leaf.classes.iter().filter(|row|row.class==*class&&row.path==[LifeTypeProjection::Pointee]&&row.binder==StorageBinder::Fixed(*binder)).count()==1))
}

fn short_result(port:&context_ports::Port,leaf:&requests::Leaf,b:&mut Budget)->Check<bool>{
    dynamic_metadata::ty(&port.leaf,b)?;dynamic_metadata::ty(&leaf.ty,b)?;
    let (Type::Address{mutable:input,inner},Type::Address{mutable:output,inner:result})=(&port.leaf,&leaf.ty)else{return Ok(false);};
    if (*output&&!*input)||inner!=result{return Ok(false);}
    b.spend(port.inner.len().saturating_mul(leaf.classes.len()).saturating_add(1))?;
    Ok(leaf.classes.len()==port.inner.len()&&port.inner.iter().all(|(class,binder)|leaf.classes.iter().filter(|row|row.class==*class&&row.path==[LifeTypeProjection::Pointee]&&row.binder==StorageBinder::Fixed(*binder)).count()==1))
}

pub(in crate::dynamic_witness) fn instantiate(module:&TypedModule,owner:&SemanticDynOwner,recipe:&DynSignatureRecipe,signature:&mut signatures::Resolved,interface:&DynInterface,scope:DynInstantiationScope,b:&mut Budget)->Check<()>{
    if interface.context.is_none()||!canonical(module,owner,recipe,b)?{return Ok(());}
    let Some(receiver)=signature.parameters.first()else{return Err(bad("default Context method has no real receiver"));};
    let mutable=match receiver.ty{Type::Address{mutable,..}=>mutable,_=>return Err(bad("default Context receiver is not a borrowed Self"))};
    let ports=context_ports::check(module,interface,|ty,b|values::intrinsic(module,ty,b).map(|value|value.count),b)?;
    let mut arguments=Vec::new();
    for(index,value)in signature.parameters.iter().enumerate().skip(1){for mut leaf in inventory::self_leaves(module,value,scope,b)?{leaf.source.base=StorageBase::Parameter(index as u32);b.spend(1)?;arguments.push(leaf);}}
    let result=if matches!(signature.result.ty,Type::Address{..})&&!signature.result.life.slots.is_empty(){inventory::self_leaves(module,&signature.result,scope,b)?.into_iter().find(|row|row.source.path.is_empty())}else{None};
    let head_binder=receiver.life.slots.iter().find(|row|row.path.is_empty()).and_then(|row|receiver.classes.get(row.binder as usize)).copied();
    let short=result.as_ref().is_some_and(|result|matches!(&result.kind,requests::LeafKind::Physical{outer:requests::Borrow::Variable{binder:StorageBinder::Invocation(index),..},..}if head_binder==Some(StorageBinder::Invocation(*index))));
    let c=&mut signature.contract;
    b.spend(ports.len().saturating_mul(11))?;
    for(index,row)in ports.iter().enumerate(){
        let position=port(index as u32);b.spend(7)?;
        c.requires.as_mut().ok_or_else(||bad("default Context requirements disappeared"))?.push(DynRequirement::Read(position.clone()));
        c.call.as_mut().ok_or_else(||bad("default Context access disappeared"))?.reads.push(position.clone());
        if mutable&&matches!(row.mode,StoredBorrowMode::ExclusiveAddress|StoredBorrowMode::WritableDescription){c.requires.as_mut().unwrap().push(DynRequirement::Write(position.clone()));c.call.as_mut().unwrap().writes.push(position.clone());}
        b.spend(2)?;let mut sources=vec![StorageSource::Exact(position.clone())];
        if mutable{for argument in &arguments{if compatible(row,argument,b)?{dynamic_metadata::position(&argument.source,b)?;b.spend(1)?;sources.push(StorageSource::Exact(argument.source.clone()));}}}
        c.guarantees.as_mut().ok_or_else(||bad("default Context after-state disappeared"))?.push(DynGuarantee::StorageSources(StorageSourceBinding{position:position.clone(),sources}));
        if let(Some(result),Some(DynResultSource::Borrow{sources,..}))=(&result,&mut c.result){if compatible(row,result,b)?{b.spend(2)?;sources.push(StorageSource::Exact(position.clone()));}
            if short&&short_result(row,result,b)?{b.spend(4)?;sources.push(StorageSource::ReborrowedLeaf{source:position,head:head(0)});}}
    }
    // Result argument alternatives require the same complete Fixed class and
    // inner occurrence map as the result; no invocation head is a Fixed source.
    if let(Some(result),Some(DynResultSource::Borrow{sources,..}))=(&result,&mut c.result){
        for argument in &arguments{
            let requests::LeafKind::Physical{outer:requests::Borrow::Variable{binder:StorageBinder::Fixed(_),..},mode}=&argument.kind else{continue;};
            let requests::LeafKind::Physical{outer:requests::Borrow::Variable{binder:result_binder,..},mode:result_mode}=&result.kind else{continue;};
            dynamic_metadata::ty(&argument.ty,b)?;dynamic_metadata::ty(&result.ty,b)?;
            b.spend(argument.classes.len().saturating_mul(result.classes.len()).saturating_add(2))?;
            if argument.ty==result.ty&&mode==result_mode&&matches!(&argument.kind,requests::LeafKind::Physical{outer:requests::Borrow::Variable{binder,..},..}if binder==result_binder)&&argument.classes.len()==result.classes.len()&&argument.classes.iter().all(|row|result.classes.iter().any(|other|row.path==other.path&&row.class==other.class&&row.binder==other.binder)){
                dynamic_metadata::position(&argument.source,b)?;for source in sources.iter(){dynamic_metadata::source(source,b)?;}
                let source=StorageSource::Exact(argument.source.clone());if !sources.contains(&source){b.spend(1)?;sources.push(source);}
            }
        }
    }
    contracts::charge(module,c,b)
}
