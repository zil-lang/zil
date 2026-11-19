//! Driver graph policy only. It never provides a lifetime, owner, phase or
//! target. Every admitted opcode is replayed through the ordinary verifier's
//! complete actual instruction contract; user cleanup is a separate graph edge.
use super::*;
use crate::tasks::context_projection;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum Kind{Physical,DirectCall,DynamicCall,ClosureCall,Lifecycle,ClosedFinish,Cleanup,Replacement,Forbidden}
pub(super) fn kind(operation:&InstructionKind)->Kind{
    // Exhaustive over the actual IR: a new opcode must choose its execution
    // and hidden-user-code contract here, rather than inherit a wildcard.
    match operation{
        InstructionKind::AbortPlaceReservation{..}
        |InstructionKind::AddressField{..}
        |InstructionKind::AddressIndex{..}
        |InstructionKind::AddressSlice{..}
        |InstructionKind::Binary{..}
        |InstructionKind::BindPackageCapture{..}
        |InstructionKind::BorrowCapture{..}
        |InstructionKind::BorrowDynOwned{..}
        |InstructionKind::BorrowFieldLocal{..}
        |InstructionKind::BorrowLocal{..}
        |InstructionKind::BorrowLocalMut{..}
        |InstructionKind::BorrowOwnedArrayElement{..}
        |InstructionKind::BorrowPackageField{..}
        |InstructionKind::BorrowPlaceAddress{..}
        |InstructionKind::BorrowPlaceLocal{..}
        |InstructionKind::BorrowVariantAddress{..}
        |InstructionKind::BorrowVariantLocal{..}
        |InstructionKind::ByteBufferBorrow{..}
        |InstructionKind::ByteBufferCapacity{..}
        |InstructionKind::ByteBufferLen{..}
        |InstructionKind::ByteBufferNew
        |InstructionKind::ByteBufferPush{..}
        |InstructionKind::ByteBufferReserve{..}
        |InstructionKind::ByteBufferTruncate{..}
        |InstructionKind::Constant(_)
        |InstructionKind::ConvertInteger{..}
        |InstructionKind::ConvertNumber{..}
        |InstructionKind::DescribeAddress{..}
        |InstructionKind::DescribeCapture{..}
        |InstructionKind::DescribeLocal{..}
        |InstructionKind::DescribePackageField{..}
        |InstructionKind::EndPackageCapture{..}
        |InstructionKind::ErrorTag{..}
        |InstructionKind::ExtractField{..}
        |InstructionKind::ExtractIndex{..}
        |InstructionKind::ForeignFunctionRef{..}
        |InstructionKind::FunctionRef{..}
        |InstructionKind::Load(_)
        |InstructionKind::LoadStoredBorrow{..}
        |InstructionKind::MakeAggregate{..}
        |InstructionKind::MakeClosure{..}
        |InstructionKind::MakeDynBorrow{..}
        |InstructionKind::MakeErrorUnion{..}
        |InstructionKind::MakeManaged{..}
        |InstructionKind::MakeOwnedArray{..}
        |InstructionKind::MakeOwnedTuple{..}
        |InstructionKind::MakePackage{..}
        |InstructionKind::MakeVariant{..}
        |InstructionKind::OwnedStoreBorrow{..}
        |InstructionKind::OwnedStoreBorrowMut{..}
        |InstructionKind::OwnedStoreCapacity{..}
        |InstructionKind::OwnedStoreLen{..}
        |InstructionKind::OwnedStoreNew{..}
        |InstructionKind::OwnedStorePop{..}
        |InstructionKind::OwnedStorePush{..}
        |InstructionKind::OwnedStoreReserve{..}
        |InstructionKind::PlaceTagAddress{..}
        |InstructionKind::PlaceTagLocal{..}
        |InstructionKind::ProjectOwnedArrayField{..}
        |InstructionKind::ReadAddress{..}
        |InstructionKind::ReadCapture{..}
        |InstructionKind::ReadDesc{..}
        |InstructionKind::ReadErrorUnion{..}
        |InstructionKind::ReadPackageField{..}
        |InstructionKind::ReadVariant{..}
        |InstructionKind::ReadVariantLocal{..}
        |InstructionKind::ReborrowAddress{..}
        |InstructionKind::RegisterDefer{..}
        |InstructionKind::ReservePlaceAddress{..}
        |InstructionKind::ReservePlaceLocal{..}
        |InstructionKind::ScopeEnter{..}
        |InstructionKind::ScopeExit{..}
        |InstructionKind::SliceLength{..}
        |InstructionKind::StaticUtf8{..}
        |InstructionKind::StorageDead(_)
        |InstructionKind::StorageLive(_)
        |InstructionKind::StoreStoredBorrow{..}
        |InstructionKind::SyncI32{..}
        |InstructionKind::SyncI32CompareExchange{..}
        |InstructionKind::SyncInteger{..}
        |InstructionKind::SyncIntegerCompareExchange{..}
        |InstructionKind::TakeCapture{..}
        |InstructionKind::TakeErrorUnion{..}
        |InstructionKind::TakeLocal{..}
        |InstructionKind::TakeOwnedArrayElement{..}
        |InstructionKind::TakePackageField{..}
        |InstructionKind::TakeTupleField{..}
        |InstructionKind::TakeVariantField{..}
        |InstructionKind::Unary{..}
        |InstructionKind::UnpackTuple{..}
        |InstructionKind::UnpackVariant{..}
        |InstructionKind::UnsafeEnter{..}
        |InstructionKind::UnsafeExit{..}
        |InstructionKind::UpdateField{..}
        |InstructionKind::UpdateIndex{..}
        |InstructionKind::VariantTag{..}
        |InstructionKind::VariantTagAddress{..}
        |InstructionKind::VariantTagLocal{..}
        |InstructionKind::WriteAddress{..}
        |InstructionKind::WriteCapture{..}
        |InstructionKind::WriteDesc{..}=>Kind::Physical,
        InstructionKind::MakeDynOwned{..}
        |InstructionKind::DropClosure{..}
        |InstructionKind::DropDynOwned{..}
        |InstructionKind::DropEnum{..}
        |InstructionKind::DropErrorUnion{..}
        |InstructionKind::DropManaged{..}
        |InstructionKind::DropOwnedArray{..}
        |InstructionKind::DropOwnedArrayElement{..}
        |InstructionKind::DropPackage{..}
        |InstructionKind::DropPackageField{..}
        |InstructionKind::MutexCreate{..}
        |InstructionKind::DropTuple{..}=>Kind::Cleanup,
        InstructionKind::CommitPlaceReplace{..}
        |InstructionKind::Store{..}
        |InstructionKind::StoreOwnedArrayElement{..}
        |InstructionKind::StorePackageField{..}=>Kind::Replacement,
        InstructionKind::Call{..}
        |InstructionKind::ForeignCall{..}=>Kind::DirectCall,
        InstructionKind::CallDynSlot{..}=>Kind::DynamicCall,
        InstructionKind::CallClosure{..}
        |InstructionKind::CallClosureBorrowed{..}
        |InstructionKind::CallClosureMut{..}
        |InstructionKind::CallClosureOnce{..}
        |InstructionKind::CallClosureShared{..}=>Kind::ClosureCall,
        InstructionKind::AsyncRegistrationPollClose{..}
        |InstructionKind::AsyncRegistrationRequestClose{..}
        |InstructionKind::AsyncRegistrationStart{..}
        |InstructionKind::AsyncRegistrationValid{..}
        |InstructionKind::RegistrationClose{..}
        |InstructionKind::RegistrationStart{..}
        |InstructionKind::RegistrationValid{..}
        |InstructionKind::TaskCreate{..}
        |InstructionKind::TaskCreateOwned{..}
        |InstructionKind::TaskFinishDestroy{..}
        |InstructionKind::TaskPoll{..}
        |InstructionKind::TaskPollClose{..}
        |InstructionKind::TaskRequestCancel{..}
        |InstructionKind::TaskStart{..}
        |InstructionKind::TaskValid{..}=>Kind::Lifecycle,
        InstructionKind::AsyncRegistrationFinishDestroy{..}
        |InstructionKind::AsyncRegistrationFinishDestroyChecked{..}=>Kind::ClosedFinish,
        InstructionKind::CallIndirect{..}
        |InstructionKind::DropMutexGuard{..}
        |InstructionKind::MutexLock{..}
        |InstructionKind::MutexUnlock{..}
        |InstructionKind::MutexValue{..}
        |InstructionKind::SuspendCall{..}
        |InstructionKind::TaskYield
        |InstructionKind::Unsupported(_)
        |InstructionKind::WithForeignCallback{..}
        |InstructionKind::WithScopedThread{..}
        |InstructionKind::WithSyncCell{..}=>Kind::Forbidden,
    }
}

/// The old endpoint of a real destruction/replacement, never a type selected
/// by layout or by the replacement's new value. The independent resource
/// kernel still proves its current tag, initialization, pins and move phase.
pub(super) fn cleanup_roots<'a>(
    module:&'a TypedModule,f:&'a TypedFunction,i:&Instruction,
    places:Option<&'a crate::place_replacements::Inventory>,b:&mut Budget,
)->Check<Vec<&'a Type>>{
    let span=Some(i.span);
    let local=|id|local_type(f,id,span);
    let field=|id,index,b:&mut Budget|crate::dynamic_ports::physical_path(local(id)?,&[PlaceProjection::Field(index)],b);
    let root=match &i.kind{
        InstructionKind::DropDynOwned{local:id}|InstructionKind::DropManaged{local:id}
        |InstructionKind::DropPackage{local:id}|InstructionKind::DropOwnedArray{local:id}
        |InstructionKind::DropTuple{local:id}|InstructionKind::DropEnum{local:id}
        |InstructionKind::DropErrorUnion{local:id}|InstructionKind::DropClosure{local:id}
        |InstructionKind::Store{local:id,..}=>Some(local(*id)?),
        InstructionKind::DropPackageField{local,index}|InstructionKind::StorePackageField{local,index,..}
        |InstructionKind::DropOwnedArrayElement{local,index}|InstructionKind::StoreOwnedArrayElement{local,index,..}=>Some(field(*local,*index,b)?),
        InstructionKind::CommitPlaceReplace{reservation,value}=>{
            let places=places.ok_or_else(||reject("E-VERIFY-TASK",span,"Task helper replacement has no complete reservation inventory"))?;
            b.spend(places.places.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
            let old=&places.places.get(reservation).ok_or_else(||reject("E-VERIFY-TASK",span,"Task helper replacement has no exact old place"))?.ty;
            let new=value_type(f,*value,span)?;resources::charge_payload(old,b)?;resources::charge_payload(new,b)?;
            if old!=new{return Err(reject("E-VERIFY-TASK",span,"Task helper replacement changes its full old endpoint type"));}
            Some(old)
        },
        InstructionKind::TaskFinishDestroy{task}=>Some(value_type(f,*task,span)?),
        // Allocation or initialization can fail after the actual payload has
        // transferred. The existing cleanup graph must follow that payload.
        InstructionKind::MutexCreate{value}=>Some(value_type(f,*value,span)?),
        InstructionKind::RegistrationClose{registration}|InstructionKind::AsyncRegistrationFinishDestroy{registration}|InstructionKind::AsyncRegistrationFinishDestroyChecked{registration}=>Some(value_type(f,*registration,span)?),
        InstructionKind::RegistrationStart{callback,..}|InstructionKind::AsyncRegistrationStart{callback,..}=>Some(value_type(f,*callback,span)?),
        InstructionKind::CallClosureOnce{callee,..}=>Some(value_type(f,*callee,span)?),
        InstructionKind::MakeDynOwned{source,..}=>Some(value_type(f,*source,span)?),
        _=>None,
    };
    b.spend(usize::from(root.is_some()))?;Ok(root.into_iter().collect())
}

/// Follow the concrete default cleanup, separately from the opcode's physical
/// qualification. A Task's hidden frame is never inferred from R or layout:
/// its exact creation/worker/source and CE obligations stay with the existing
/// origin, Task-frame and protocol passes. Only the declared layout role is
/// selected here; synchronous user destructors enter this same actual graph.
pub(super) fn cleanup_dependencies(
    module:&TypedModule,root:&Type,driving:bool,closed_retirement:bool,b:&mut Budget,
)->Check<Vec<FunctionId>>{
    resources::charge_payload(root,b)?;
    let mut pending=vec![(root,0usize)];let mut targets=BTreeSet::new();let mut closures=BTreeSet::new();
    while let Some((ty,depth))=pending.pop(){
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("Task actual default-cleanup type graph exceeded"));}
        match ty{
            Type::BorrowedStorage{storage,..}=>pending.push((storage,depth+1)),
            Type::Managed(payload)=>match payload.as_ref(){
                task@Type::Task{result,..}=>{
                    if !driving{return Err(reject("E-VERIFY-TASK",None,"Task synchronous cleanup has an unresolved asynchronous child head"));}
                    if !pollable_storage(module,ty,b)?{return Err(reject("E-VERIFY-TASK",None,"Task default cleanup lacks its complete declared frame layout"));}
                    super::super::require_closed_context(task,None)?;
                    // A live Ready result is another real owned subtree. Its
                    // hidden inputs remain selected by actual frame origins.
                    pending.push((result,depth+1));
                },
                Type::AsyncRegistration{environment}=>{
                    if !(driving||closed_retirement){return Err(reject("E-VERIFY-TASK",None,"Task synchronous cleanup has an unclosed pollable registration"));}
                    if !pollable_storage(module,ty,b)?{return Err(reject("E-VERIFY-TASK",None,"Task registration cleanup lacks its actual retained environment layout"));}
                    pending.push((environment,depth+1));
                },
                Type::Registration{environment}=>{
                    // The canonical registration ABI independently proves
                    // no-future/drain; dropping it cleans its real environment.
                    crate::registration::carrier(module,ty,None,b)?;
                    pending.push((environment,depth+1));
                },
                Type::Mutex{payload}=>{
                    crate::dynamic_witness::structural_actual(module,ty,DynStructuralBound::SyncDrop,b)?;
                    pending.push((payload,depth+1));
                },
                Type::TaskI32=>{}, // Legacy TaskStart has its separate NoExternal invariant.
                _=>{
                    let mut found=None;
                    for resource in &module.resources{
                        resources::charge_payload(&resource.payload,b)?;resources::charge_payload(payload,b)?;
                        if resource.payload==**payload{if found.replace(resource).is_some(){return Err(reject("E-VERIFY-TASK",None,"Task cleanup resource has ambiguous complete identity"));}}
                    }
                    let resource=found.ok_or_else(||reject("E-VERIFY-TASK",None,"Task cleanup resource lacks its exact actual declaration"))?;
                    match payload.as_ref(){
                        Type::ByteBuffer if resource.name==BYTE_BUFFER_RESOURCE_NAME&&resource.teardown.is_none()=>{},
                        Type::OwnedStore{element}if resource.teardown.is_none()=>{crate::dynamic_store::check_carrier(module,ty,None,b)?;pending.push((element,depth+1));},
                        Type::Record{name,..}if name==&resource.name=>{
                            if let Some(id)=resource.teardown{
                                let target=module.functions.get(id.0 as usize).filter(|f|f.id==id).ok_or_else(||reject("E-VERIFY-TASK",None,"Task actual teardown entry is absent"))?;
                                if target.foreign.is_some()||target.environment.is_some()||target.call_kind!=CallKind::Direct||target.return_type!=Type::Unit||target.parameters.len()!=1||!matches!(target.locals.get(target.parameters[0].0 as usize),Some(local)if matches!(&local.ty,Type::Desc(inner)if inner.as_ref()==payload.as_ref())){return Err(reject("E-VERIFY-TASK",None,"Task actual teardown changes its concrete payload descriptor/Direct ABI"));}
                                b.spend(targets.len().saturating_add(1))?;targets.insert(id);
                            }
                            pending.push((payload,depth+1));
                        },
                        _=>return Err(reject("E-VERIFY-TASK",None,"Task default cleanup has no independently checked concrete resource route")),
                    }
                },
            },
            Type::Closure(id)=>{
                b.spend(closures.len().saturating_add(1))?;
                if !closures.insert(*id){continue;}
                let closure=crate::closures::definition(module,*id,None)?;
                b.spend(closure.captures.len())?;pending.extend(closure.captures.iter().map(|ty|(ty,depth+1)));
            },
            Type::OwnedDyn{interface}=>{
                let item=crate::dynamic_ports::interface(module,*interface,b)?;
                crate::dynamic_cleanup::interface_bound(module,item,b)?;
                for table in &module.dyn_vtables{
                    b.spend(1)?;if table.interface!=*interface{continue;}
                    crate::dynamic_ports::validate_table(module,table,b)?;
                    if let Some(entry)=&table.cleanup{b.spend(targets.len().saturating_add(1))?;targets.insert(entry.function);}
                    pending.push((&table.self_type,depth+1));
                }
            },
            Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|(_,ty)|(ty,depth+1)));},
            Type::Tuple(fields)=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|ty|(ty,depth+1)));},
            Type::Enum{variants,..}=>for(_,fields)in variants{b.spend(fields.len())?;pending.extend(fields.iter().map(|ty|(ty,depth+1)));},
            Type::ErrorUnion{error,result}=>{b.spend(2)?;pending.push((error,depth+1));pending.push((result,depth+1));},
            Type::Array{element,..}|Type::OwnedStore{element}=>pending.push((element,depth+1)),
            // Borrowed heads and pure/code values have no owning destructor.
            Type::Address{..}|Type::Desc(_)|Type::Dyn{..}|Type::FunctionPointer{..}|Type::BorrowedFunctionPointer{..}|Type::ForeignCallback{..}|Type::TaskI32=>{},
            ty if ty.is_copy_value()=>{},
            _=>return Err(reject("E-VERIFY-TASK",None,"Task default cleanup has an unknown concrete owned subtree")),
        }
    }
    b.spend(targets.len())?;Ok(targets.into_iter().collect())
}

pub(super) fn terminator(
    f:&TypedFunction,block:&BasicBlock,reachable:bool,
    definitions:&mut Option<Vec<context_projection::Definition>>,b:&mut Budget,
)->Check<()>{
    b.spend(1)?;
    let span=Some(f.span);
    match &block.terminator{
        Terminator::Return(value)=>{
            let ty=if let Some(value)=value{value_type(f,*value,span)?}else{&Type::Unit};
            resources::charge_payload(ty,b)?;resources::charge_payload(&f.return_type,b)?;
            if ty!=&f.return_type{
                if let Some(value)=value{
                    if definitions.is_none(){*definitions=Some(context_projection::definitions(f,b)?);}
                    if context_projection::matches(f,definitions.as_deref().unwrap_or_default(),*value,ty,&f.return_type,b)?{
                        return Ok(());
                    }
                }
                return Err(reject("E-VERIFY-TASK",span,"Task actual helper Return changes its complete result type"));
            }
        },
        Terminator::Jump(target)=>if !f.blocks.get(target.0 as usize).is_some_and(|block|block.id==*target){return Err(reject("E-VERIFY-TASK",span,"Task actual helper Jump target is absent"));},
        Terminator::Branch{condition,then_block,else_block}=>{
            if value_type(f,*condition,span)?!=&Type::Bool||!f.blocks.get(then_block.0 as usize).is_some_and(|block|block.id==*then_block)||!f.blocks.get(else_block.0 as usize).is_some_and(|block|block.id==*else_block){return Err(reject("E-VERIFY-TASK",span,"Task actual helper Branch changes its actual condition or targets"));}
        },
        Terminator::Trap=>{},
        Terminator::Unreachable if !reachable=>{},
        Terminator::Unreachable=>return Err(reject("E-VERIFY-TASK",span,"Task actual helper has reachable undefined termination")),
    }
    Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    fn budget(module:&TypedModule)->Budget{let mut b=Budget::new(&Limits::default());b.function_count=module.functions.len();b}
    fn quiet()->Type{Type::Record{name:"Quiet".into(),fields:vec![("code".into(),Type::I32)]}}
    fn resource_fixture()->TypedModule{
        let(mut module,_)=crate::tests::baseline();let payload=quiet();
        module.resources.push(ResourceDefinition{name:"Quiet".into(),payload:payload.clone(),teardown:Some(FunctionId(1))});
        let mut drop=module.functions[0].clone();drop.id=FunctionId(1);drop.name="drop_quiet".into();drop.parameters=vec![LocalId(0)];drop.return_type=Type::Unit;
        drop.locals[0].ty=Type::Desc(Box::new(payload.clone()));drop.values.clear();drop.blocks[0].instructions.clear();drop.blocks[0].terminator=Terminator::Return(None);
        module.functions.push(drop);
        let root=&mut module.functions[0];root.return_type=Type::Unit;root.locals[0].ty=Type::Managed(Box::new(payload));root.locals[0].mutable=true;root.values.clear();root.blocks[0].instructions=vec![Instruction{result:None,kind:InstructionKind::DropManaged{local:LocalId(0)},span:root.span}];root.blocks[0].terminator=Terminator::Return(None);
        module
    }
    fn roundtrip(module:&TypedModule)->TypedModule{let limits=Limits::default();let bytes=zil_ir::wire::encode_module_candidate(module,&CandidatePlan::empty_for(module),&limits).unwrap();zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0}
    #[test]
    fn actual_selected_error_field_runs_the_typed_rule_instead_of_a_graph_opcode_list_raw_wire(){
        let mut module=resource_fixture();module.resources[0].teardown=None;module.functions.pop();
        let f=&mut module.functions[0];f.return_type=Type::I32;f.locals[0].ty=Type::ErrorUnion{error:Box::new(Type::I32),result:Box::new(f.locals[0].ty.clone())};f.values=vec![Type::I32];
        f.blocks[0].instructions=vec![Instruction{result:Some(ValueId(0)),kind:InstructionKind::TakeVariantField{local:LocalId(0),path:vec![PlaceProjection::ErrorArm(true)]},span:f.span}];f.blocks[0].terminator=Terminator::Return(Some(ValueId(0)));
        let wire=roundtrip(&module);
        for actual in [&module,&wire]{
            // Graph/type qualification only; the independent Sum/resource
            // passes still owe this synthetic local its tag and initialization.
            helper_graph(actual,FunctionId(0),&mut budget(actual),None,true).unwrap();
            let mut forged=actual.clone();forged.functions[0].values[0]=Type::Bool;forged.functions[0].return_type=Type::Bool;
            assert!(helper_graph(&forged,FunctionId(0),&mut budget(&forged),None,true).is_err());
        }
    }
    #[test]
    fn default_teardown_is_a_real_synchronous_edge_and_cannot_hide_yield_raw_wire(){
        let module=resource_fixture();
        let wire=roundtrip(&module);
        for actual in [&module,&wire]{
            assert_eq!(cleanup_dependencies(actual,&actual.functions[0].locals[0].ty,true,false,&mut budget(actual)).unwrap(),vec![FunctionId(1)]);
            helper_graph(actual,FunctionId(0),&mut budget(actual),None,true).unwrap();
            let mut forged=actual.clone();let f=&mut forged.functions[1];f.blocks[0].instructions.push(Instruction{result:None,kind:InstructionKind::TaskYield,span:f.span});
            let error=helper_graph(&forged,FunctionId(0),&mut budget(&forged),None,true).unwrap_err();assert!(error.message.contains("asynchronous"));
        }
    }
    #[test]
    fn replacement_cleanup_comes_from_the_real_reserved_endpoint_not_new_value_layout_raw_wire(){
        let mut module=resource_fixture();let f=&mut module.functions[0];let owner=f.locals[0].ty.clone();f.values=vec![owner];
        f.blocks[0].instructions=vec![Instruction{result:None,kind:InstructionKind::ReservePlaceLocal{reservation:zil_ir::ReservationId(0),local:LocalId(0),path:vec![]},span:f.span}];
        let wire=roundtrip(&module);
        for actual in [&module,&wire]{
            let f=&actual.functions[0];let mut b=budget(actual);let places=crate::place_replacements::Inventory::reconstruct(actual,f,&mut b).unwrap();
            let commit=Instruction{result:None,kind:InstructionKind::CommitPlaceReplace{reservation:zil_ir::ReservationId(0),value:ValueId(0)},span:f.span};
            let roots=cleanup_roots(actual,f,&commit,Some(&places),&mut b).unwrap();assert_eq!(roots,vec![&f.locals[0].ty]);
            let mut forged=actual.clone();forged.functions[0].values[0]=Type::Managed(Box::new(Type::Record{name:"DifferentIdentity".into(),fields:vec![("code".into(),Type::I32)]}));
            assert!(cleanup_roots(&forged,&forged.functions[0],&commit,Some(&places),&mut b).is_err());
        }
    }
    #[test]
    fn indirect_and_external_user_execution_never_inherit_physical_admission(){
        assert_eq!(kind(&InstructionKind::TaskYield),Kind::Forbidden);
        assert_eq!(kind(&InstructionKind::SuspendCall{function:FunctionId(0),arguments:vec![]}),Kind::Forbidden);
        assert_eq!(kind(&InstructionKind::CallIndirect{callee:ValueId(0),arguments:vec![]}),Kind::Forbidden);
        assert_eq!(kind(&InstructionKind::Unsupported("new raw opcode".into())),Kind::Forbidden);
        assert_eq!(kind(&InstructionKind::TakeVariantField{local:LocalId(0),path:vec![PlaceProjection::ErrorArm(true)]}),Kind::Physical);
        assert_eq!(kind(&InstructionKind::DropClosure{local:LocalId(0)}),Kind::Cleanup);
        assert_eq!(kind(&InstructionKind::Store{local:LocalId(0),value:ValueId(0)}),Kind::Replacement);
    }
}
