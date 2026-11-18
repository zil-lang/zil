//! Project a complete child domain through its declared occurrence directory.
//! Parent images may coincide; each child κ remains injective and its original
//! intrinsic identities survive. This supplies no runtime pointer or loan.
use super::*;
impl CheckedContext {
    pub(in crate::semantic) fn child_value(&self,module:&TypedModule,parent:&CheckedType,path:&[LifeTypeProjection],b:&mut Budget)->Check<CheckedType>{
        self.child_value_mode(module,parent,path,None,b)
    }
    pub(in crate::semantic) fn child_value_mode(&self,module:&TypedModule,parent:&CheckedType,path:&[LifeTypeProjection],outer_mode:Option<StoredBorrowMode>,b:&mut Budget)->Check<CheckedType>{
        if path.is_empty(){
            b.spend(parent.classes.len().saturating_add(parent.origins.len()))?;
            return Ok(parent.clone());
        }
        let ty=lifetimes::select(&parent.ty,path,b)?;
        let physical=path.iter().map(|step|match step{LifeTypeProjection::Place(step)=>Some(*step),_=>None}).collect::<Option<Vec<_>>>();
        let mut statics=Vec::new();
        for row in &parent.statics {
            match row {
                StaticOccurrence::Direct{path:source,leaf,mode}=>{
                    b.spend(source.len().saturating_add(path.len()))?;
                    if let Some(tail)=source.strip_prefix(path){
                        dynamic_metadata::ty(leaf,b)?;
                        statics.push(StaticOccurrence::Direct{path:tail.to_vec(),leaf:leaf.clone(),mode:*mode});
                    }
                }
                StaticOccurrence::Logical{closure,..} if contains_closure(ty,*closure,b)?=>{
                    if let StaticOccurrence::Logical{position,leaf,..}=row{dynamic_metadata::position(position,b)?;dynamic_metadata::ty(leaf,b)?;}
                    statics.push(row.clone());
                }
                _=>{},
            }
        }
        // Select the already checked complete child occurrence, not a fresh
        // context-free raw Type domain. In particular a nested Static field
        // has no variable outer κ, while its pointee/phantom domain survives.
        b.spend(parent.life.retained.len().saturating_mul(path.len().saturating_add(1)))?;
        let opaque=matches!(ty,Type::BorrowedStorage{..}|Type::Closure(_)|Type::Task{..}|Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..});
        // Parent physical slots only describe inline children. A Pointee
        // starts its own local slot directory, even when its whole κ map is
        // present in the enclosing logical directory. Raw products need not
        // have a redundant whole-product row: rebuild their local context
        // and use the exact occurrence lookup below for every class.
        let mut life=if !opaque&&physical.is_some()&&parent.life.retained.iter().any(|row|row.path==path){
            projected_life(parent,path,physical.as_deref(),b)?
        }else if !opaque&&statics.iter().any(|row|matches!(row,StaticOccurrence::Direct{..})){
            crate::tasks::frame_shape::static_child_life(module,ty,&statics,b)?
        }else{crate::tasks::frame_shape::declared_life(module,ty,b)?};
        if let Some(mode)=outer_mode{
            if !matches!((ty,mode),(Type::Address{mutable:false,..},StoredBorrowMode::SharedAddress)|(Type::Address{mutable:true,..},StoredBorrowMode::ExclusiveAddress)|(Type::Desc(_),StoredBorrowMode::ReadOnlyDescription|StoredBorrowMode::WritableDescription)){return Err(bad("stored child mode changes its complete actual Address/Desc kind"));}
            let mut rows=life.slots.iter_mut().filter(|row|row.path.is_empty());
            if let Some(row)=rows.next(){row.mode=mode;}else if !statics.iter().any(|row|matches!(row,StaticOccurrence::Direct{path,mode:actual,..}if path.is_empty()&&*actual==mode)){return Err(bad("stored child has no exact declared outer mode/occurrence"));}
            if rows.next().is_some(){return Err(bad("stored child repeats its outer occurrence"));}
        }
        let mut classes=Vec::new();let mut origins=Vec::new();let mut injective=std::collections::BTreeSet::new();
        for class in 0..life.binder_count {
            let mut binder=None;
            let mut assign=|candidate:u32|->Check<()>{
                if binder.is_some_and(|old|old!=candidate){return Err(bad("complete child occurrences disagree on their parent class"));}
                binder=Some(candidate);Ok(())
            };
            b.spend(parent.life.retained.len().saturating_mul(path.len().saturating_add(1)))?;
            for row in &parent.life.retained{if row.path==path&&row.class==class{assign(row.binder)?;}}
            if let Some(prefix)=&physical {
                for slot in &life.slots {
                    if slot.binder!=class{continue;}
                    b.spend(prefix.len().saturating_add(slot.path.len()))?;
                    let mut full=prefix.clone();full.extend_from_slice(&slot.path);
                    b.spend(parent.life.slots.len().saturating_mul(full.len().saturating_add(1)))?;
                    for row in &parent.life.slots{if row.path==full&&row.mode==slot.mode{assign(row.binder)?;}}
                }
            }
            for child in &life.retained {
                if child.binder!=class{continue;}
                b.spend(path.len().saturating_add(child.path.len()))?;
                let mut full=path.to_vec();full.extend_from_slice(&child.path);
                b.spend(parent.life.retained.len().saturating_mul(full.len().saturating_add(1)))?;
                for row in &parent.life.retained{if row.path==full&&row.class==child.class{assign(row.binder)?;}}
            }
            let binder=binder.ok_or_else(||bad("complete child has no exact parent occurrence map"))?;
            if !injective.insert(binder){return Err(bad("complete child projection coalesces independent intrinsic classes"));}
            let key=parent.classes.get(binder as usize).ok_or_else(||bad("child projection escapes the complete parent domain"))?;
            if let ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=key{b.spend(path.len())?;}
            let origin=parent.origins.get(binder as usize).ok_or_else(||bad("child projection has no original intrinsic identity"))?;
            charge_intrinsic_origin(origin,b)?;
            classes.push(key.clone());origins.push(origin.clone());
        }
        let mut child=self.checked_value(module,ty,&life,classes,b)?;
        if let Type::Closure(id)=ty && physical.is_some() {
            for (index,origin) in origins.iter_mut().enumerate(){
                let IntrinsicOrigin::Nominal{source,class}=origin else{continue;};
                b.spend(source.domain.retained.len().saturating_mul(path.len().saturating_add(1)))?;
                let mut physical_rows=source.domain.retained.iter().filter(|row|
                    row.path==path&&row.class as usize==index&&row.binder==*class);
                if physical_rows.next().is_none()||physical_rows.next().is_some(){
                    return Err(bad("nominal callback projection changes its exact physical child class occurrence")
                        .with_note(format!("field path {path:?}, callback class {index}, nominal class {class}")));
                }
                let projected=source.class_sources.get(*class as usize).ok_or_else(||bad("physical callback projection lacks its nominal source class"))?;
                charge_intrinsic_origin(projected,b)?;
                let IntrinsicOrigin::Existing{scope:ScopeKey::Instance(frame),operand:OperandKey::Receiver,path:origin_path,class:ordinal}=projected else{
                    return Err(bad("physical callback projection has no original Receiver origin")
                        .with_note(format!("field path {path:?}, callback class {index}, nominal class {class}")));
                };
                if !origin_path.is_empty() || *ordinal as usize!=index {
                    return Err(bad("physical callback projection changes its complete Receiver class ordinal")
                        .with_note(format!("field path {path:?}, callback class {index}, selected {ordinal}")));
                }
                // The exact physical entry and original world exist before
                // the body traversal records any actual construction images.
                let closure=crate::closures::definition(module,*id,None)?;
                self.check_closure_origin_scope(module,closure,*frame,b)?;
                *origin=projected.clone();
            }
        }
        child.origins=origins;child.statics=statics;Ok(child)
    }
}
fn projected_life(parent:&CheckedType,path:&[LifeTypeProjection],physical:Option<&[PlaceProjection]>,b:&mut Budget)->Check<StorageLifeShape>{
    let mut map=Vec::new();
    for row in &parent.life.retained {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if row.path!=path{continue;}
        b.spend(map.len().saturating_add(1))?;
        if row.class as usize!=map.len()||map.contains(&row.binder){return Err(bad("selected child omits, reorders or coalesces its complete intrinsic classes"));}
        map.push(row.binder);
    }
    let mut result=empty_life();
    result.binder_count=u32::try_from(map.len()).map_err(|_|limit("projected child full domain exceeds u32"))?;
    let local=|binder:u32,b:&mut Budget|->Check<u32>{
        b.spend(map.len())?;
        map.iter().position(|image|*image==binder).map(|index|index as u32).ok_or_else(||bad("selected child occurrence escapes its complete whole-child map"))
    };
    if let Some(prefix)=physical{
        for row in &parent.life.slots{
            b.spend(row.path.len().saturating_add(prefix.len()))?;
            if let Some(tail)=row.path.strip_prefix(prefix){b.spend(tail.len().saturating_add(1))?;result.slots.push(StorageLifeSlot{path:tail.to_vec(),mode:row.mode,binder:local(row.binder,b)?});}
        }
    }
    for row in &parent.life.retained{
        b.spend(row.path.len().saturating_add(path.len()))?;
        if let Some(tail)=row.path.strip_prefix(path){if !tail.is_empty(){b.spend(tail.len().saturating_add(1))?;result.retained.push(StorageRetainedBinding{path:tail.to_vec(),class:row.class,binder:local(row.binder,b)?});}}
    }
    for row in &parent.life.nominal_arguments{
        b.spend(row.path.len().saturating_add(path.len()))?;
        if let Some(tail)=row.path.strip_prefix(path){b.spend(tail.len().saturating_add(1))?;result.nominal_arguments.push(NominalLifeArgument{path:tail.to_vec(),declaration:row.declaration,argument:row.argument,binder:local(row.binder,b)?});}
    }
    Ok(result)
}
fn contains_closure(ty:&Type,closure:ClosureId,b:&mut Budget)->Check<bool>{
    let mut pending=vec![ty];
    while let Some(ty)=pending.pop(){b.spend(1)?;match ty{
        Type::Closure(id)if *id==closure=>return Ok(true),
        Type::BorrowedStorage{storage:inner,..}|Type::Managed(inner)|Type::Desc(inner)|Type::Address{inner,..}|Type::Array{element:inner,..}|Type::Slice{element:inner}=>pending.push(inner),
        Type::Tuple(fields)=>pending.extend(fields),
        Type::Record{fields,..}|Type::Package{fields,..}=>pending.extend(fields.iter().map(|(_,ty)|ty)),
        Type::Enum{variants,..}=>pending.extend(variants.iter().flat_map(|(_,fields)|fields)),
        Type::ErrorUnion{error,result}=>{pending.push(error);pending.push(result);},
        _=>{},
    }}Ok(false)
}

#[cfg(test)]
mod projected_scope_tests {
    use super::*;
    #[test]
    fn complete_projection_preserves_local_indices_descendants_and_phantoms(){
        let field=LifeTypeProjection::Place(PlaceProjection::Field(0));
        let prefix=vec![field];
        let mut parent=CheckedType{type_cell:crate::semantic::CheckedTypeData::new(Type::Unit),life:empty_life(),classes:vec![],origins:vec![],statics:vec![]};
        parent.life.binder_count=3;
        for(class,binder)in [2,0,1].into_iter().enumerate(){parent.life.retained.push(StorageRetainedBinding{path:prefix.clone(),class:class as u32,binder});}
        parent.life.slots.push(StorageLifeSlot{path:vec![PlaceProjection::Field(0)],binder:2,mode:StoredBorrowMode::SharedAddress});
        parent.life.retained.extend([StorageRetainedBinding{path:vec![field,LifeTypeProjection::Pointee],class:0,binder:0},StorageRetainedBinding{path:vec![field,LifeTypeProjection::Pointee],class:1,binder:1}]);
        parent.life.nominal_arguments.push(NominalLifeArgument{path:vec![field,LifeTypeProjection::Pointee],declaration:zil_ir::builtin_life::OPTION,argument:0,binder:1});
        let query=|parent:&CheckedType|projected_life(parent,&prefix,Some(&[PlaceProjection::Field(0)]),&mut Budget::new(&Limits::default()));
        let child=query(&parent).unwrap();assert_eq!(child.binder_count,3);assert_eq!(child.slots[0].binder,0);assert_eq!(child.retained.iter().map(|row|row.binder).collect::<Vec<_>>(),vec![1,2]);assert_eq!(child.nominal_arguments[0].binder,2);
        let mut missing=parent.clone();missing.life.retained.remove(1);assert!(query(&missing).is_err());
        let mut coalesced=parent.clone();coalesced.life.retained[1].binder=2;assert!(query(&coalesced).is_err());
        let mut escaped=parent.clone();escaped.life.nominal_arguments[0].binder=3;assert!(query(&escaped).is_err());
    }
}

#[cfg(test)]
mod checked_child_scope_tests {
    use super::*;
    fn context()->(TypedModule,CheckedContext,ScopeKey){
        let limits=Limits::default();
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let scope=ScopeKey::Instance(module.semantic.as_ref().unwrap().instances[0].id);
        let context=CheckedContext::catalogue(&module,&mut Budget::new(&limits)).unwrap();
        (module,context,scope)
    }
    fn keys(scope:ScopeKey,count:u32)->Vec<ClassKey>{(0..count).map(|ordinal|ClassKey::Formal{scope,ordinal}).collect()}
    fn wire(module:&TypedModule)->TypedModule{
        let limits=Limits::default();let bytes=zil_ir::wire::encode_module_candidate(module,&CandidatePlan::empty_for(module),&limits).unwrap();zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap().0
    }
    fn environment()->EvalEnv{EvalEnv{lexical:vec![],assumptions:vec![],images:vec![],bounds:vec![],invocations:vec![],at:None}}
    #[test]
    fn checked_pointee_projection_rebuilds_its_local_physical_slots_raw_wire(){
        let(mut module,context,scope)=context();
        let inner=Type::Address{mutable:false,inner:Box::new(Type::I32)};
        let outer=Type::Address{mutable:false,inner:Box::new(inner.clone())};
        module.functions[0].values.push(outer);
        for module in [&module,&wire(&module)]{
            let limits=Limits::default();let mut b=Budget::new(&limits);
            let ty=module.functions[0].values.last().unwrap();
            let life=crate::tasks::frame_shape::declared_life(module,ty,&mut b).unwrap();
            let parent=context.checked_value(module,ty,&life,keys(scope,life.binder_count),&mut b).unwrap();
            let child=context.child_value(module,&parent,&[LifeTypeProjection::Pointee],&mut b).unwrap();
            assert_eq!(child.ty,inner);assert_eq!(child.life.binder_count,1);
            assert_eq!(child.life.slots,vec![StorageLifeSlot{path:vec![],binder:0,mode:StoredBorrowMode::SharedAddress}]);
            assert_eq!(child.classes,vec![parent.classes[1].clone()]);assert_eq!(child.origins,vec![parent.origins[1].clone()]);
            let rebuilt=crate::dynamic_witness::semantic_external::construct(&context,&environment(),crate::dynamic_witness::semantic_external::Constructor::Address{mode:StoredBorrowMode::SharedAddress,outer:Some(parent.classes[0].clone())},&[child],&mut ActiveQueries::new(),&mut b).unwrap();
            assert_eq!(rebuilt.ty,parent.ty);assert_eq!(rebuilt.life,parent.life);assert_eq!(rebuilt.classes,parent.classes);assert_eq!(rebuilt.origins,parent.origins);
            let mut missing=life.clone();missing.retained.clear();
            assert!(context.checked_value(module,ty,&missing,keys(scope,life.binder_count),&mut b).is_err());
            let mut coalesced=life.clone();coalesced.retained[0].binder=0;
            assert!(context.checked_value(module,ty,&coalesced,keys(scope,life.binder_count),&mut b).is_err());
        }
    }
    #[test]
    fn checked_nested_static_products_preserve_nonzero_child_and_phantom_maps_raw_wire(){
        let(mut module,context,scope)=context();
        let task=Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{binder_count:2,ports:vec![],retained:(0..2).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class,binder:class}).collect(),nominal_arguments:vec![]}};
        let head=Type::Address{mutable:false,inner:Box::new(task)};
        let child_ty=Type::Tuple(vec![head.clone()]);
        let outer_types=[Type::Tuple(vec![child_ty.clone()]),Type::Record{name:"static_child_scope".into(),fields:vec![("value".into(),child_ty.clone())]},Type::ErrorUnion{error:Box::new(Type::I32),result:Box::new(child_ty.clone())}];
        module.functions[0].values.extend(outer_types);
        for module in [&module,&wire(&module)]{
            for(ordinal,ty)in module.functions[0].values.iter().rev().take(3).rev().enumerate(){
                let limits=Limits::default();let mut b=Budget::new(&limits);
                let selected=if ordinal==2{PlaceProjection::ErrorArm(false)}else{PlaceProjection::Field(0)};
                let path=vec![LifeTypeProjection::Place(selected),LifeTypeProjection::Place(PlaceProjection::Field(0))];
                let statics=vec![StaticOccurrence::Direct{path:path.clone(),leaf:head.clone(),mode:StoredBorrowMode::SharedAddress}];
                let life=crate::tasks::frame_shape::static_child_life(module,ty,&statics,&mut b).unwrap();
                assert_eq!(life.binder_count,2);assert!(life.slots.is_empty());
                assert!(!life.retained.iter().any(|row|row.path==[LifeTypeProjection::Place(selected)]),"raw product carries descendants without a redundant whole-product row");
                let mut parent=context.checked_value(module,ty,&life,keys(scope,2),&mut b).unwrap();parent.statics=statics;
                let child=context.child_value(module,&parent,&[LifeTypeProjection::Place(selected)],&mut b).unwrap();
                assert_eq!(child.ty,child_ty);assert_eq!(child.life.binder_count,2);assert_eq!(child.classes,parent.classes);assert_eq!(child.origins,parent.origins);
                assert_eq!(child.statics,vec![StaticOccurrence::Direct{path:vec![LifeTypeProjection::Place(PlaceProjection::Field(0))],leaf:head.clone(),mode:StoredBorrowMode::SharedAddress}]);
                let pointer=context.child_value(module,&child,&[LifeTypeProjection::Place(PlaceProjection::Field(0))],&mut b).unwrap();
                assert_eq!(pointer.life.binder_count,2);assert!(pointer.life.slots.is_empty());assert_eq!(pointer.classes,child.classes);
                let pointee=context.child_value(module,&pointer,&[LifeTypeProjection::Pointee],&mut b).unwrap();assert_eq!(pointee.life.binder_count,2);assert_eq!(pointee.classes,pointer.classes);assert_eq!(pointee.origins,pointer.origins);
                for attack in 0..3{
                    let mut forged=life.clone();
                    match attack{0=>{forged.retained.pop();},1=>{forged.retained[1].binder=forged.retained[0].binder;},_=>{forged.binder_count=1;}}
                    assert!(context.checked_value(module,ty,&forged,keys(scope,forged.binder_count),&mut b).is_err(),"phantom map attack {attack}");
                }
                if ordinal==0{
                    let rebuilt=crate::dynamic_witness::semantic_external::construct(&context,&environment(),crate::dynamic_witness::semantic_external::Constructor::Tuple,&[child],&mut ActiveQueries::new(),&mut b).unwrap();
                    // The existing fresh Tuple constructor explicitly stores
                    // its nonzero scope. Validate it before projecting again.
                    let mut checked=context.checked_value(module,&rebuilt.ty,&rebuilt.life,rebuilt.classes.clone(),&mut b).unwrap();checked.origins=rebuilt.origins;checked.statics=rebuilt.statics;
                    let again=context.child_value(module,&checked,&[LifeTypeProjection::Place(PlaceProjection::Field(0))],&mut b).unwrap();
                    assert_eq!(again.ty,child_ty);assert_eq!(again.classes,parent.classes);assert_eq!(again.origins,parent.origins);assert_eq!(again.life.binder_count,2);
                }
                assert!(b.proven_static_occurrences.is_empty(),"scope context is no actual Static source proof");
            }
        }
    }
    #[test]
    fn checked_existing_callback_array_element_scope_roundtrips_without_losing_opaque_classes(){
        let limits=Limits::default();
        let(mut module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/local-closure-complete-head.wire"),&limits).unwrap();
        let context=CheckedContext::catalogue(&module,&mut Budget::new(&limits)).unwrap();
        let scope=ScopeKey::Instance(module.semantic.as_ref().unwrap().instances[0].id);
        let closure=module.closures[0].id;let count=module.closures[0].storage_life.as_ref().unwrap().fixed_binder_count;assert!(count>0);
        module.functions[0].values.push(Type::Array{element:Box::new(Type::Closure(closure)),length:1});
        for module in [&module,&wire(&module)]{
            let mut b=Budget::new(&limits);let ty=module.functions[0].values.last().unwrap();let life=crate::tasks::frame_shape::declared_life(module,ty,&mut b).unwrap();
            let parent=context.checked_value(module,ty,&life,keys(scope,count),&mut b).unwrap();
            let child=context.child_value(module,&parent,&[LifeTypeProjection::Element],&mut b).unwrap();
            assert_eq!(child.ty,Type::Closure(closure));assert_eq!(child.life.binder_count,count);assert_eq!(child.classes,parent.classes);assert_eq!(child.origins,parent.origins);
            let rebuilt=crate::dynamic_witness::semantic_external::construct(&context,&environment(),crate::dynamic_witness::semantic_external::Constructor::Tuple,&[parent.clone()],&mut ActiveQueries::new(),&mut b).unwrap();
            let mut checked=context.checked_value(module,&rebuilt.ty,&rebuilt.life,rebuilt.classes.clone(),&mut b).unwrap();checked.origins=rebuilt.origins;checked.statics=rebuilt.statics;
            let again=context.child_value(module,&checked,&[LifeTypeProjection::Place(PlaceProjection::Field(0))],&mut b).unwrap();assert_eq!(again.ty,parent.ty);assert_eq!(again.life,parent.life);assert_eq!(again.classes,parent.classes);assert_eq!(again.origins,parent.origins);
        }
    }
}
