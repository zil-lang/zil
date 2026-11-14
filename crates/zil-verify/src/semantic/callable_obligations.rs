//! Original Callable requirements produce an exact entry obligation, never a
//! fabricated effect upper. Source/semantic entry checking and actual memory
//! verification are separate steps; only the final module boundary drains it.
use super::*;
use dynamic_witness::requests::CallableShapeRequest;

pub(crate) struct PendingCallable {
    request:CallableShapeRequest,
    worlds:Vec<SemanticInstanceId>,
}
impl CheckedContext {
    pub(super) fn defer_callable_shapes(&self,module:&TypedModule,external:&mut dynamic_witness::semantic_external::Proofs,checked:&std::collections::BTreeSet<SemanticInstanceId>,b:&mut Budget)->Check<()> {
        if !external.pending_statics.is_empty(){return Err(unsupported("structural Static capability retains its complete actual source obligation",None));}
        let program=module.semantic.as_ref().ok_or_else(||bad("Callable entry obligation has no original programme"))?;
        for request in std::mem::take(&mut external.callable_shapes){
            charge(&request,b)?;let definition=closure(module,request.closure,request.function,b)?;
            if request.actual_header!=*definition.storage_life.as_ref().ok_or_else(||bad("Callable actual complete header is Unknown"))?||request.receiver.ty!=Type::Closure(definition.id)||request.receiver.life.binder_count!=request.actual_header.fixed_binder_count||request.intrinsic_classes.len()!=request.receiver.life.binder_count as usize{return Err(bad("Callable pending entry changes its complete environment domain"));}
            let mut worlds=Vec::new();b.spend(program.instances.len())?;
            for instance in program.instances.iter().filter(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==request.function)){
                if !checked.contains(&instance.id)||!self.seals.borrow().contains_key(&instance.id){return Err(unsupported("Callable exact world has no completed actual creator and entry",None));}
                if !instance.environment.as_ref().is_some_and(|row|matches!(row.closure,SemanticClosureTarget::Typed(id)if id==request.closure)){return Err(bad("Callable entry world names another environment"));}
                if self.definition(module,instance.definition,b)?.kind!=SemanticDefinitionKind::Closure{return Err(bad("Callable entry substitutes another original definition kind"));}
                let boundaries=self.closure_boundaries.borrow();b.spend(boundaries.len())?;
                if !boundaries.iter().any(|row|row.child==instance.id&&row.value.ty==request.receiver.ty&&row.value.life==request.receiver.life){return Err(unsupported("Callable world lacks its complete independently sealed constructor domain",None));}
                b.spend(1)?;worlds.push(instance.id);
            }
            if worlds.is_empty(){return Err(unsupported("Callable has no exact executable child world",None));}
            if b.semantic_callables.len()>=b.limits.max_nodes{return Err(limit("pending Callable entry inventory exceeds selected bound"));}
            b.spend(1)?;b.semantic_callables.push(PendingCallable{request,worlds});
        }
        Ok(())
    }
}

/// Called after every actual CFG/type, source/loan, global Static, resource,
/// Task transfer and phase pass. This function cannot issue an early entry
/// permission from a merely successful structural signature match.
pub(crate) fn drain(module:&TypedModule,b:&mut Budget)->Check<()> {
    let pending=std::mem::take(&mut b.semantic_callables);
    for pending in pending{
        let request=&pending.request;charge(request,b)?;
        let definition=closure(module,request.closure,request.function,b)?;
        let function=module.functions.get(request.function.0 as usize).filter(|row|row.id==request.function&&row.environment==Some(request.closure)&&row.foreign.is_none()).ok_or_else(||bad("Callable final actual entry identity is absent"))?;
        if definition.storage_life.as_ref()!=Some(&request.actual_header)||function.storage_life.as_ref()!=Some(&request.actual_header)||function.call_kind!=request.call_kind||function.result_mode!=request.result_mode||definition.result!=request.result.ty||definition.parameters.len()!=request.parameters.len()||request.parameter_modes.len()!=request.parameters.len(){return Err(bad("Callable final actual ABI/domain differs from its original requirement"));}
        if !matches!((definition.kind,request.kind),(CallableKind::Fn,_)|(CallableKind::FnMut,CallableKind::FnMut|CallableKind::FnOnce)|(CallableKind::FnOnce,CallableKind::FnOnce)){return Err(bad("Callable final role exceeds its actual entry role"));}
        for(actual,required)in definition.parameters.iter().zip(&request.parameters){if actual!=&required.ty{return Err(bad("Callable final whole parameter type differs"));}}
        let program=module.semantic.as_ref().ok_or_else(||bad("Callable final original programme is absent"))?;
        for world in pending.worlds{b.spend(program.instances.len())?;let mut rows=program.instances.iter().filter(|row|row.id==world);let row=rows.next().ok_or_else(||bad("Callable final exact world disappeared"))?;if rows.next().is_some()||!matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==request.function){return Err(bad("Callable final world changed its actual target"));}}
        // The full static validator has checked all real construction/write
        // occurrences and every Return, including rootless-cycle rejection.
        for position in &request.actual_header.static_bindings{if matches!(position.base,StorageBase::Capture(_)|StorageBase::Result){static_node(module,request.closure,position,b)?;}}
        for(base,occurrence)in &request.pending_statics{match occurrence{
            StaticOccurrence::Logical{closure,position,..}=>static_node(module,*closure,position,b)?,
            StaticOccurrence::Direct{path,..}=>{
                if matches!(base,StorageBase::Parameter(_)){continue;}
                let mut projected=Vec::new();for step in path{b.spend(1)?;let LifeTypeProjection::Place(step)=step else{return Err(unsupported("Callable Static result needs its complete logical occurrence source",None));};projected.push(ContractProjection::Place(*step));}
                static_node(module,request.closure,&StoragePosition{base:*base,path:projected},b)?;
            },
        }}
    }
    Ok(())
}
fn static_node(module:&TypedModule,id:ClosureId,position:&StoragePosition,b:&mut Budget)->Check<()> {
    dynamic_metadata::position(position,b)?;b.spend(b.proven_static_occurrences.len().saturating_add(module.closures.len()))?;
    if !module.closures.iter().any(|row|row.id==id&&row.storage_life.as_ref().is_some_and(|life|life.static_bindings.contains(position)))||!b.proven_static_occurrences.contains(&(id,position.clone())){return Err(bad("Callable Static occurrence has no completed actual least-root source proof"));}
    Ok(())
}
fn closure<'a>(module:&'a TypedModule,id:ClosureId,function:FunctionId,b:&mut Budget)->Check<&'a ClosureDefinition>{
    b.spend(module.closures.len())?;let mut rows=module.closures.iter().filter(|row|row.id==id);let definition=rows.next().ok_or_else(||bad("Callable actual environment is absent"))?;if rows.next().is_some()||definition.function!=function{return Err(bad("Callable exact environment/entry identity differs"));}Ok(definition)
}
fn charge(request:&CallableShapeRequest,b:&mut Budget)->Check<()> {
    dynamic_metadata::recipe(&request.receiver,b)?;dynamic_metadata::recipe(&request.result,b)?;signature_life::charge(&request.actual_header,b)?;
    b.spend(request.parameters.len().saturating_add(request.intrinsic_classes.len()).saturating_add(request.self_leaves.len()).saturating_add(request.pending_statics.len()).saturating_add(request.rigid.len()).saturating_add(request.fixed_relations.len()))?;
    for value in &request.parameters{dynamic_metadata::recipe(value,b)?;}
    Ok(())
}
