//! Join independently instantiated original contracts to real CFG summaries.
//! Trait and implementation requests remain separate; no upper creates a root.
use super::*;
use crate::dynamic_witness::requests::{ContractRequest,Origin,CallableRequest,Occurrence};
fn bad(message:&str)->Diagnostic{reject("E-VERIFY-DYN-CONTRACT",None,message)}
fn covers(upper:&StoragePosition,actual:&StoragePosition,b:&mut Budget)->Check<bool>{
    b.spend(upper.path.len().saturating_add(actual.path.len()))?;
    Ok(upper.base==actual.base&&actual.path.starts_with(&upper.path)&&actual.path[upper.path.len()..].iter().all(|step|matches!(step,ContractProjection::Place(_))))
}
fn position(request:&ContractRequest,module:&TypedModule,source:&Source,b:&mut Budget)->Check<Option<StoragePosition>>{
    if let Origin::TableSlot{table,..}=request.origin{
        let table=module.dyn_vtables.iter().find(|row|row.id==table).ok_or_else(||bad("actual original contract table disappeared"))?;
        let mut position=super::dyn_providers::selected(table,source,b)?;
        if let Some(position)=&mut position{b.spend(source.inline.len())?;position.path.extend(source.inline.iter().copied().map(ContractProjection::Place));}return Ok(position);
    }
    let base=match source.base{HoldRoot::Parameter(index)=>StorageBase::Parameter(index),HoldRoot::Capture(index)=>{if matches!(request.origin,Origin::Callable{..}){let mut path=vec![ContractProjection::Capture(index)];path.extend(source.leaf.iter().copied().map(ContractProjection::Place));path.extend(source.inline.iter().copied().map(ContractProjection::Place));return Ok(Some(StoragePosition{base:StorageBase::Receiver,path}));}StorageBase::Capture(index)},HoldRoot::Static|HoldRoot::Local(_)=>return Ok(None)};
    if source.heap_element.is_some()||!source.heap_parents.is_empty(){return Err(bad("original contract actual source requires an explicit owned-region selector"));}
    let mut path=source.leaf.iter().copied().map(ContractProjection::Place).collect::<Vec<_>>();
    if let Some(port)=&source.retained_port{let(carrier,port)=match port{PortOrigin::Task{carrier,port}|PortOrigin::Dyn{carrier,port}=>(carrier,*port)};path=carrier.clone();path.push(ContractProjection::RetainedPort(port));}
    path.extend(source.inline.iter().copied().map(ContractProjection::Place));b.spend(path.len())?;Ok(Some(StoragePosition{base,path}))
}
fn header(module:&TypedModule,request:&ContractRequest,b:&mut Budget)->Check<()>{
    let f=module.functions.get(request.function.0 as usize).filter(|f|f.id==request.function).ok_or_else(||bad("original executable contract has no exact actual function"))?;
    if f.foreign.is_some()||f.parameters.len()!=request.parameters.len()||f.call_kind!=request.call_kind||f.result_mode!=request.result_mode||f.return_type!=request.result.ty||f.dyn_instance!=request.specialization||f.storage_life!=request.actual_header{return Err(bad("original executable contract changes exact function/header/result responsibility"));}
    if request.parameter_modes.len()!=request.parameters.len(){return Err(bad("original executable contract omits complete parameter modes"));}
    for((local,value),mode)in f.parameters.iter().zip(&request.parameters).zip(&request.parameter_modes){let actual=f.locals.get(local.0 as usize).ok_or_else(||bad("contract actual parameter local is absent"))?;if actual.ty!=value.ty||matches!(mode,PassingMode::Borrow)&&!matches!(actual.ty,Type::Address{mutable:false,..})||matches!(mode,PassingMode::BorrowMut)&&!matches!(actual.ty,Type::Address{mutable:true,..})||matches!(mode,PassingMode::Describe)&&!matches!(actual.ty,Type::Desc(_)){return Err(bad("contract actual parameter changes full type or passing mode"));}crate::dynamic_witness::check_type_value(module,value,request.scope,b)?;}
    crate::dynamic_witness::check_type_value(module,&request.result,request.scope,b)?;
    for operand in &request.operands{crate::dynamic_witness::check_type_value(module,&operand.value,request.scope,b)?;}
    if let Some(actual)=&request.actual_header{
        signature_life::charge(actual,b)?;
        if actual.bindings.len()!=request.expected_header.bindings.len()||actual.static_bindings!=request.expected_header.static_bindings||actual.classes.len()!=request.expected_header.classes.len(){return Err(bad("original executable contract loses a whole header occurrence"));}
        for row in &actual.bindings{let mut image=request.header_embedding.iter().filter(|map|map.from==row.binder);let map=image.next().ok_or_else(||bad("contract actual input binder has no exact original image"))?;if image.next().is_some()||!request.expected_header.bindings.iter().any(|expected|expected.position==row.position&&expected.binder==map.to){return Err(bad("contract input occurrence changes its original scoped role"));}}
        for row in &actual.classes{let mut image=request.header_embedding.iter().filter(|map|map.from==row.binder);let map=image.next().ok_or_else(||bad("contract actual class has no exact original image"))?;if image.next().is_some()||!request.expected_header.classes.iter().any(|expected|expected.position==row.position&&expected.binder==map.to){return Err(bad("contract actual full intrinsic class changes its original occurrence"));}}
    }else if request.parameters.iter().any(|value|value.life.binder_count!=0)||request.result.life.binder_count!=0||!request.expected_header.bindings.is_empty()||!request.expected_header.classes.is_empty()||!request.expected_header.static_bindings.is_empty(){return Err(bad("class-bearing original contract actual header is Unknown"));}
    match &request.origin{
        Origin::TableSlot{table,slot,member,definition}=>{let table=module.dyn_vtables.iter().find(|row|row.id==*table).ok_or_else(||bad("contract original table is absent"))?;let iface=crate::dynamic_ports::interface(module,table.interface,b)?;if iface.slots.get(*slot as usize).is_none_or(|row|row.member!=*member)||!table.entries.iter().any(|entry|entry.slot==*slot&&entry.function==f.id)||f.dyn_instance.as_ref().is_none_or(|instance|instance.definition!=*definition){return Err(bad("contract request changes its exact table/member/definition entry"));}},
        Origin::Definition{owner,member,definition}=>{let declaration=module.dyn_declarations.iter().find(|row|row.id==*owner).ok_or_else(||bad("contract original definition owner is absent"))?;if !declaration.method_definitions.as_ref().is_some_and(|rows|rows.iter().any(|row|row.id==*definition&&row.member==*member&&row.instances.iter().any(|instance|instance.function==DynFunctionTarget::Typed(f.id)))){return Err(bad("contract request has no exact complete original specialization"));}},
        Origin::Callable{closure}=>{if f.environment!=Some(*closure)||!module.closures.iter().any(|row|row.id==*closure&&row.function==f.id){return Err(bad("contract callable substitutes another actual creator environment"));}},
    }
    Ok(())
}
fn callable(module:&TypedModule,request:&CallableRequest,b:&mut Budget)->Check<()>{
    let Origin::Callable{closure}=&request.contract.origin else{return Err(bad("callable executable contract changes its original role"));};let definition=module.closures.iter().find(|row|row.id==*closure).ok_or_else(||bad("callable actual environment is absent"))?;
    if request.receiver.ty!=Type::Closure(*closure)||definition.function!=request.contract.function||!crate::closures::supports(definition.kind,request.kind)||request.receiver.life.binder_count as usize!=request.intrinsic_classes.len(){return Err(bad("callable contract changes its whole actual environment or call kind"));}
    crate::dynamic_witness::check_type_value(module,&request.receiver,request.contract.scope,b)?;
    let actual=request.contract.actual_header.as_ref().ok_or_else(||bad("callable actual complete header is Unknown"))?;
    for row in &request.rigid{let valid=match row.actual{StorageBinder::Fixed(index)=>index<actual.fixed_binder_count,StorageBinder::Invocation(index)=>index<actual.invocation_binder_count};if !valid||!request.contract.header_embedding.iter().any(|map|map.from==row.actual&&map.to==row.image)||!request.contract.expected_header.origins.iter().any(|occurrence|match occurrence{Occurrence::Borrow{origin,..}|Occurrence::Class{origin,..}=>*origin==row.required}){return Err(bad("callable actual rigid role loses its complete original occurrence"));}}
    for row in &request.fixed_relations{b.spend(actual.bindings.len().saturating_add(request.contract.expected_header.origins.len()))?;if !actual.bindings.iter().any(|binding|binding.position==row.position&&binding.binder==row.lexical)||!request.intrinsic_classes.contains(&row.actual)||!request.contract.expected_header.origins.iter().any(|occurrence|matches!(occurrence,Occurrence::Borrow{position,origin,..}if *position==row.position&&*origin==row.required)){return Err(bad("callable Fixed role changes its exact actual input occurrence"));}}
    Ok(())
}
fn source_upper(module:&TypedModule,request:&ContractRequest,actual:&Source,upper:&[StorageSource],result_path:&[PlaceProjection],b:&mut Budget)->Check<()>{
    if actual.base==HoldRoot::Static{
        let pos=StoragePosition{base:StorageBase::Result,path:result_path.iter().copied().map(ContractProjection::Place).collect()};
        let proven=match &actual.static_dependency{StaticProvenance::Literal=>true,StaticProvenance::Conditional(key)=>b.proven_static_occurrences.contains(key),_=>false};
        if !proven||!request.expected_header.static_bindings.contains(&pos){return Err(bad("contract static result has no exact actual literal/header proof"));}return Ok(());
    }
    let actual=position(request,module,actual,b)?.ok_or_else(||bad("contract result retains activation-local storage"))?;
    if !crate::exact_storage_sources_contain(upper,&actual)?{return Err(bad("actual returned target exceeds its independently instantiated original source upper"));}Ok(())
}
fn contract(module:&TypedModule,request:&ContractRequest,summary:&Summary,b:&mut Budget)->Check<()>{
    header(module,request,b)?;
    if !summary.complete||!summary.dyn_effects_known{return Err(bad("original executable contract lacks a complete actual CFG memory/effect summary"));}
    let c=&request.contract;let requires=c.requires.as_ref().ok_or_else(||bad("original contract requires are Unknown"))?;let guarantees=c.guarantees.as_ref().ok_or_else(||bad("original contract guarantees are Unknown"))?;let hold=c.hold.as_ref().ok_or_else(||bad("original contract hold is Unknown"))?;let access=c.call.as_ref().ok_or_else(||bad("original contract access is Unknown"))?;let effects=c.effects.as_ref().ok_or_else(||bad("original contract effects are Unknown"))?;let result=c.result.as_ref().ok_or_else(||bad("original contract result source is Unknown"))?;let cleanup=c.cleanup.as_ref().ok_or_else(||bad("original contract cleanup is Unknown"))?;
    if c.resume!=Some(DynResumeBound::Local)||c.threads.as_ref().is_none_or(|threads|threads.transfer||threads.share){return Err(bad("original contract requires a separately proved nonlocal actual execution role"));}
    for requirement in requires{match requirement{DynRequirement::Read(pos)|DynRequirement::Write(pos)|DynRequirement::Manage(pos)=>dynamic_metadata::position(pos,b)?,DynRequirement::Apart(..)|DynRequirement::State{..}=>return Err(bad("original executable precondition has no independently checked actual relational/state consumer"))}}
    for effect in &summary.dyn_effects{b.spend(effects.len())?;if !effects.contains(effect){return Err(bad("actual reachable method/helper effect exceeds an original trait or implementation upper"));}}
    for(source,writing)in &summary.dyn_accesses{let Some(actual)=position(request,module,source,b)? else{continue;};let upper=if *writing{&access.writes}else{&access.reads};let mut allowed=false;for pos in upper{if covers(pos,&actual,b)?{allowed=true;break;}}if !allowed{return Err(bad("actual method target access exceeds an original head/port upper"));}}
    for(path,sources)in &summary.result{
        let position=StoragePosition{base:StorageBase::Result,path:path.iter().copied().map(ContractProjection::Place).collect()};
        let upper=if path.is_empty(){match result{DynResultSource::Borrow{sources,..}=>Some(sources.as_slice()),_=>None}}else{None}.or_else(||guarantees.iter().find_map(|guarantee|match guarantee{DynGuarantee::StorageSources(row)if row.position==position=>Some(row.sources.as_slice()),_=>None}));
        let upper=upper.ok_or_else(||bad("actual borrowed result leaf has no complete original target-source upper"))?;
        for source in sources{source_upper(module,request,source,upper,path,b)?;}
    }
    if let DynResultSource::Borrow{mutable,..}=result{if !matches!(request.result.ty,Type::Address{mutable:actual,..}if actual==*mutable){return Err(bad("original borrowed result changes its actual outer mode"));}}
    if matches!(result,DynResultSource::Create|DynResultSource::Give)&&!matches!(request.result_mode,PassingMode::Create|PassingMode::Give){return Err(bad("original owning result does not match its actual responsibility mode"));}
    for guarantee in guarantees{match guarantee{
        DynGuarantee::StorageSources(upper)=>{if let StorageBase::Parameter(parameter)=upper.position.base{let path=crate::dynamic_ports::source_places(&upper.position,b)?;let actual=summary.external_after.iter().find(|(index,at,_)|*index==parameter&&*at==path).ok_or_else(||bad("actual Return omits an original after-state source leaf"))?;if actual.2.is_empty(){return Err(bad("actual after-state leaf is Unknown, not known empty"));}for source in &actual.2{source_upper(module,request,source,&upper.sources,&[],b)?;}}},
        DynGuarantee::ManageResult=>if !matches!(request.result_mode,PassingMode::Create|PassingMode::Give){return Err(bad("actual result has no original owning responsibility"));},
        DynGuarantee::State{..}=>return Err(bad("original state guarantee has no completed actual Return state proof")),
    }}
    for(index,path,sources)in &summary.external_after{let actual=StoragePosition{base:StorageBase::Parameter(*index),path:path.iter().copied().map(ContractProjection::Place).collect()};for source in sources{let Some(target)=position(request,module,source,b)? else{continue;};if target!=actual&&!hold.iter().any(|pos|pos==&target)&&!guarantees.iter().any(|row|matches!(row,DynGuarantee::StorageSources(upper)if upper.position==actual)){return Err(bad("actual retained after-state target is outside the original hold/guarantee upper"));}}}
    if cleanup.environment{let Origin::Callable{closure}=request.origin else{return Err(bad("nonenvironment method claims environment cleanup authority"));};super::dyn_providers::cleanup_proof(module,&Type::Closure(closure),cleanup,b)?;}
    else if !cleanup.positions.is_empty()||!cleanup.access.reads.is_empty()||!cleanup.access.writes.is_empty()||!cleanup.effects.is_empty(){return Err(bad("original explicit cleanup positions have no actual owning environment role"));}
    Ok(())
}
pub(super) fn validate(module:&TypedModule,summaries:&[Summary],b:&mut Budget)->Check<()>{
    let pending=std::mem::take(&mut b.pending_dyn_contracts);
    for request in pending.methods{let summary=summaries.get(request.function.0 as usize).ok_or_else(||bad("original method actual CFG summary is absent"))?;contract(module,&request,summary,b)?;}
    for request in pending.callables{callable(module,&request,b)?;let summary=summaries.get(request.contract.function.0 as usize).ok_or_else(||bad("original callable actual CFG summary is absent"))?;contract(module,&request.contract,summary,b)?;}
    Ok(())
}
