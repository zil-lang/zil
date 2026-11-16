//! Declared static occurrences are pending source obligations. No header tag
//! publishes a root, variable port, lifetime class, loan or cleanup permission.
use super::*;

fn declared<'a>(module:&'a TypedModule,id:ClosureId,b:&mut Budget)->Check<&'a ClosureDefinition>{
    b.spend(module.closures.len())?;
    let mut rows=module.closures.iter().filter(|row|row.id==id);
    let definition=rows.next().ok_or_else(||bad("Task static occurrence has no actual Closure definition"))?;
    if rows.next().is_some(){return Err(bad("Task static occurrence has ambiguous actual Closure identity"));}
    let entry=module.functions.get(definition.function.0 as usize).filter(|row|row.id==definition.function&&row.environment==Some(id)).ok_or_else(||bad("Task static occurrence has no exact actual Closure entry"))?;
    let header=definition.storage_life.as_ref().ok_or_else(||bad("Task static occurrence complete header is Unknown"))?;
    signature_life::charge(header,b)?;
    if entry.storage_life.as_ref()!=Some(header){return Err(bad("Task static occurrence changes its exact actual entry header"));}
    Ok(definition)
}
pub(super) fn occurrence(module:&TypedModule,id:ClosureId,position:&StoragePosition,b:&mut Budget)->Check<StaticOccurrence>{
    let definition=declared(module,id,b)?;
    let header=definition.storage_life.as_ref().unwrap();
    b.spend(header.static_bindings.len().saturating_mul(position.path.len().saturating_add(1)))?;
    if header.static_bindings.iter().filter(|row|*row==position).count()!=1||header.bindings.iter().any(|row|row.position==*position){return Err(bad("Task static occurrence is absent, repeated or also variable"));}
    let root=match position.base{
        StorageBase::Capture(index)=>definition.captures.get(index as usize),
        StorageBase::Parameter(index)=>definition.parameters.get(index as usize),
        StorageBase::Result=>Some(&definition.result),StorageBase::Receiver=>None,
    }.ok_or_else(||bad("Task static occurrence selects an absent actual root"))?;
    let leaf=super::super::closures::static_leaf(module,root,&position.path,b)?;
    let mode=match leaf{Type::Address{mutable:false,..}=>StoredBorrowMode::SharedAddress,Type::Desc(_)=>StoredBorrowMode::ReadOnlyDescription,_=>return Err(bad("Task static occurrence is not an immutable shared or readonly leaf"))};
    dynamic_metadata::ty(leaf,b)?;dynamic_metadata::position(position,b)?;
    Ok(StaticOccurrence::Logical{closure:id,position:position.clone(),leaf:leaf.clone(),mode})
}

/// The complete raw declaration tree supplies obligations only. In particular,
/// this does not inspect a candidate's roots or turn a missing slot into Static.
pub(super) fn pending(module:&TypedModule,root:&Type,b:&mut Budget)->Check<Vec<StaticOccurrence>>{
    let mut work=vec![(root,0usize)];let mut out=Vec::new();
    while let Some((ty,depth))=work.pop(){
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("Task static occurrence tree exceeds selected depth"));}
        match ty{
            Type::Closure(id)=>{
                let definition=declared(module,*id,b)?;let header=definition.storage_life.as_ref().unwrap();
                for position in &header.static_bindings{if out.len()>=b.limits.max_nodes{return Err(limit("Task complete static occurrence inventory exceeded"));}out.push(occurrence(module,*id,position,b)?);}
                // A nested opaque environment owns its own occurrence table;
                // header selectors, not artificial physical fields, name it.
            },
            Type::BorrowedStorage{storage:child,..}|Type::Address{inner:child,..}|Type::Desc(child)|Type::Managed(child)|Type::OwnedStore{element:child}|Type::Mutex{payload:child}|Type::Registration{environment:child}|Type::AsyncRegistration{environment:child}|Type::Opaque{hidden:child,..}=>work.push((child,depth+1)),
            Type::Array{element,length}=>{if *length as usize>b.limits.max_nodes{return Err(limit("Task static array occurrence inventory exceeded"));}b.spend(*length as usize)?;work.extend((0..*length).map(|_|(element.as_ref(),depth+1)));},
            Type::Tuple(fields)=>{b.spend(fields.len())?;work.extend(fields.iter().rev().map(|child|(child,depth+1)));},
            Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;work.extend(fields.iter().rev().map(|(_,child)|(child,depth+1)));},
            Type::Enum{variants,..}=>{for(_,fields)in variants.iter().rev(){b.spend(fields.len())?;work.extend(fields.iter().rev().map(|child|(child,depth+1)));}},
            Type::ErrorUnion{error,result}=>{work.push((result,depth+1));work.push((error,depth+1));},
            // An existing opaque carrier's result/phantom Type directory is
            // not an initialized child value in this input. Its actual stored
            // ports and whole class map retain their separate consumers.
            Type::Task{..}|Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..}=>{},
            _=>{},
        }
    }
    Ok(out)
}

pub(super) fn validate(module:&TypedModule,ty:&Type,rows:&[StaticOccurrence],b:&mut Budget)->Check<()>{
    let declared=pending(module,ty,b)?;
    for row in rows{
        b.spend(declared.len().saturating_add(1))?;
        match row{
            StaticOccurrence::Logical{closure,position,..}=>{
                let expected=occurrence(module,*closure,position,b)?;
                if &expected!=row||!declared.contains(row){return Err(bad("Task static input changes its exact original Closure/position/leaf/mode"));}
                if !matches!(position.base,StorageBase::Capture(_)|StorageBase::Result){return Err(unsupported("Task static callable parameter requires its exact actual invocation source obligation",None));}
            },
            StaticOccurrence::Direct{..}=>return Err(unsupported("Task direct constant-source input requires its exact actual constructor operand proof",None)),
        }
    }
    Ok(())
}

pub(super) fn discharge(module:&TypedModule,ty:&Type,rows:&[StaticOccurrence],b:&mut Budget)->Check<()>{
    validate(module,ty,rows,b)?;
    let declared=pending(module,ty,b)?;
    for row in declared.iter().chain(rows){
        let StaticOccurrence::Logical{closure,position,..}=row else{unreachable!()};
        if !matches!(position.base,StorageBase::Capture(_)|StorageBase::Result){return Err(unsupported("Task static callable parameter requires its exact actual invocation source obligation",None));}
        super::super::callable_obligations::static_node(module,*closure,position,b)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    fn module()->TypedModule{
        let(mut module,_)=crate::tests::baseline();let leaf=Type::Address{mutable:false,inner:Box::new(Type::Slice{element:Box::new(Type::U8)})};
        let header=StorageLifeContract{fixed_binder_count:1,invocation_binder_count:0,bindings:vec![StorageLifeBinding{position:StoragePosition{base:StorageBase::Capture(1),path:vec![]},binder:StorageBinder::Fixed(0)}],classes:vec![],static_bindings:vec![StoragePosition{base:StorageBase::Capture(0),path:vec![]}]};
        module.functions[0].environment=Some(ClosureId(0));module.functions[0].storage_life=Some(header.clone());
        module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![leaf,Type::Address{mutable:false,inner:Box::new(Type::I32)}],parameters:vec![],result:Type::I32,kind:CallableKind::Fn,storage_life:Some(header),lending_captures:vec![]});module
    }
    #[test]
    fn zero_variable_static_environment_still_requires_final_actual_source_proof(){
        let mut module=module();module.closures[0].captures.truncate(1);
        let header=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:0,bindings:vec![],static_bindings:vec![StoragePosition{base:StorageBase::Capture(0),path:vec![]}],classes:vec![]};
        module.closures[0].storage_life=Some(header.clone());module.functions[0].storage_life=Some(header);module.functions[0].return_type=Type::I32;
        let ty=Type::Closure(ClosureId(0));let limits=Limits::default();let mut b=Budget::new(&limits);
        assert_eq!(super::super::metadata_zero_type(&module,&ty,&mut b).unwrap(),Some(true));
        let rows=pending(&module,&ty,&mut b).unwrap();assert_eq!(rows.len(),1);
        let value=CheckedType{type_cell:CheckedTypeData::new(ty),life:empty_life(),classes:vec![],origins:vec![],statics:rows.clone()};
        assert!(super::super::defer_zero_header_statics(&module,&value,&mut b).unwrap());
        assert_eq!(b.semantic_static_occurrences.len(),1);assert!(b.proven_static_occurrences.is_empty());
        assert!(super::super::super::callable_obligations::drain(&module,&mut b).is_err());
        assert!(super::super::defer_zero_header_statics(&module,&value,&mut b).unwrap());
        b.proven_static_occurrences.insert((ClosureId(0),StoragePosition{base:StorageBase::Capture(0),path:vec![]}));
        super::super::super::callable_obligations::drain(&module,&mut b).unwrap();
        assert_eq!(value.statics,rows,"known variable N0 never clears its Static obligations");
        let mut missing=module.clone();missing.closures[0].storage_life.as_mut().unwrap().static_bindings.clear();missing.functions[0].storage_life=missing.closures[0].storage_life.clone();
        assert!(super::super::metadata_zero_type(&missing,&value.ty,&mut Budget::new(&limits)).is_err(),"an undeclared capture borrow cannot become known N0");
    }
    #[test]
    fn static_capture_retains_an_exact_pending_root_without_a_variable_port(){
        let module=module();let ty=Type::Closure(ClosureId(0));let mut b=Budget::new(&Limits::default());
        let rows=pending(&module,&ty,&mut b).unwrap();assert_eq!(rows.len(),1);validate(&module,&ty,&rows,&mut b).unwrap();
        let life=StorageLifeShape{binder_count:1,slots:vec![],retained:vec![],nominal_arguments:vec![]};
        let ports=super::super::raw_input_ports(&module,&ty,&life,&mut b).unwrap();
        assert_eq!(ports.len(),1);assert_eq!(ports[0].2,0);assert_eq!(ports[0].3,vec![ContractProjection::Capture(1)]);
        assert!(discharge(&module,&ty,&rows,&mut b).is_err(),"a static header is no actual source proof");
        b.proven_static_occurrences.insert((ClosureId(0),StoragePosition{base:StorageBase::Capture(0),path:vec![]}));
        discharge(&module,&ty,&rows,&mut b).unwrap();
        assert_eq!(rows,pending(&module,&ty,&mut b).unwrap(),"discharge never clears the complete static directory");
        for mutation in 0..3{
            let mut wrong=rows.clone();let StaticOccurrence::Logical{leaf,mode,position,..}=&mut wrong[0]else{unreachable!()};
            match mutation{0=>*leaf=Type::Address{mutable:false,inner:Box::new(Type::I32)},1=>*mode=StoredBorrowMode::ExclusiveAddress,_=>position.base=StorageBase::Capture(1)}
            assert!(validate(&module,&ty,&wrong,&mut b).is_err());
        }
        let direct=StaticOccurrence::Direct{path:vec![],leaf:Type::Address{mutable:false,inner:Box::new(Type::I32)},mode:StoredBorrowMode::SharedAddress};
        assert!(validate(&module,&ty,&[direct],&mut b).is_err(),"a direct operand needs its own actual constructor source proof");
        let mut foreign=module.clone();foreign.functions[0].environment=None;assert!(pending(&foreign,&ty,&mut b).is_err());
    }
}
