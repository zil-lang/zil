//! Operational capabilities of a concrete actual type. Original abstract
//! callers use only their own premises; this check grants no such premise.
use super::*;

pub(super) fn check(module:&TypedModule,value:&terms::Value,kind:SemanticBuiltinTrait,b:&mut Budget)->Check<()>{
    dynamic_metadata::recipe(&value.raw,b)?;
    // Free/phantom classes are part of the whole type, even without physical
    // ports. A capability cannot erase that original domain.
    let domain=values::intrinsic(module,&value.raw.ty,b)?;
    if domain.count!=0||!value.raw.classes.is_empty()||!value.terms.is_empty()||value.raw.life.binder_count!=0||!value.raw.life.slots.is_empty()||!value.raw.life.retained.is_empty()||!value.raw.life.nominal_arguments.is_empty(){return Err(bad("operational actual capability has a nonzero or incomplete free-class domain"));}
    match kind{
        SemanticBuiltinTrait::ThreadCallable=>{
            let Type::Closure(id)=value.raw.ty else{return Err(bad("ThreadCallable requires an exact actual entry dependency graph"));};
            crate::registration::semantic_thread_callable(module,id,b)
        },
        SemanticBuiltinTrait::ClosedEnvironment=>{
            if let Type::Closure(id)=value.raw.ty{return crate::registration::semantic_closed_environment(module,id,b);}
            transfer_data(module,&value.raw.ty,false,b)
        },
        SemanticBuiltinTrait::Transfer=>transfer_data(module,&value.raw.ty,true,b),
        // No enum-family alias or Copy/ClosedEnvironment implication supplies
        // these distinct capabilities or an original user Drop provider.
        _=>Err(bad("actual capability needs its distinct provider, sharing or Drop proof")),
    }
}

fn transfer_data(module:&TypedModule,root:&Type,transfer:bool,b:&mut Budget)->Check<()>{
    enum Work<'a>{Type(&'a Type,usize),Exit(ClosureId)}
    let mut work=vec![Work::Type(root,0)];let mut active=std::collections::BTreeSet::new();
    while let Some(item)=work.pop(){b.spend(1)?;let(ty,depth)=match item{Work::Exit(id)=>{active.remove(&id);continue;},Work::Type(ty,depth)=>(ty,depth)};
        if depth>b.limits.max_call_depth.min(128){return Err(limit("actual transferable data depth exceeded"));}
        if matches!(ty,Type::Record{..}|Type::Package{..}|Type::Enum{..}){
            b.spend(module.resources.len())?;
            for resource in &module.resources{dynamic_metadata::ty(&resource.payload,b)?;if resource.payload==*ty&&resource.teardown.is_some(){return Err(bad("custom resource cleanup needs its own transfer provider graph"));}}
        }
        match ty{
            ty if stored_callables::scalar(ty)=>{},
            Type::BorrowedStorage{storage,..}=>{
                // Recheck this complete boundary; the outer zero-domain test
                // must not hide a nested phantom/opaque domain.
                if values::intrinsic(module,ty,b)?.count!=0{return Err(bad("actual transferable data contains a nonzero child domain"));}
                work.push(Work::Type(storage.as_ref(),depth+1));
            },
            Type::Tuple(fields)=>{b.spend(fields.len())?;work.extend(fields.iter().map(|ty|Work::Type(ty,depth+1)));},
            Type::Record{fields,..}|Type::Package{fields,..}=>{
                b.spend(fields.len())?;work.extend(fields.iter().map(|(_,ty)|Work::Type(ty,depth+1)));
            },
            Type::Enum{variants,..}=>{b.spend(variants.len())?;for(_,fields)in variants{b.spend(fields.len())?;work.extend(fields.iter().map(|ty|Work::Type(ty,depth+1)));}},
            Type::Array{element,..}|Type::OwnedStore{element}=>work.push(Work::Type(element.as_ref(),depth+1)),
            Type::ErrorUnion{error,result}=>{b.spend(2)?;work.extend([Work::Type(error.as_ref(),depth+1),Work::Type(result.as_ref(),depth+1)]);},
            Type::Managed(payload)=>{
                crate::resources::check_type_for(module,ty,None,b)?;
                b.spend(module.resources.len())?;
                let mut count=0usize;
                for resource in &module.resources{dynamic_metadata::ty(&resource.payload,b)?;if resource.payload==**payload{count+=1;if resource.teardown.is_some(){return Err(bad("managed cleanup requires its independent operational provider graph"));}}}
                if count!=1{return Err(bad("actual transferable owner has no unique complete resource definition"));}
                work.push(Work::Type(payload.as_ref(),depth+1));
            },
            Type::Closure(id)=>{
                b.spend(active.len().saturating_add(1))?;
                if !active.insert(*id){return Err(bad("actual transferable closure recursively contains itself"));}
                let definition=crate::closures::definition(module,*id,None)?;
                let _complete=values::intrinsic(module,ty,b)?;
                if _complete.count!=0||!definition.lending_captures.is_empty(){return Err(bad("actual transferable closure retains an external domain or lending source"));}
                work.push(Work::Exit(*id));b.spend(definition.captures.len())?;
                work.extend(definition.captures.iter().map(|ty|Work::Type(ty,depth+1)));
            },
            Type::ByteBuffer=>{
                b.spend(module.resources.len())?;let mut count=0usize;
                for resource in &module.resources{if resource.payload==Type::ByteBuffer{count+=1;b.spend(resource.name.len())?;if resource.name!=zil_ir::BYTE_BUFFER_RESOURCE_NAME||resource.teardown.is_some(){return Err(bad("actual byte owner lacks its canonical transferable cleanup"));}}}
                if count!=1{return Err(bad("actual byte owner has no unique canonical resource definition"));}
            },
            Type::ForeignHandle|Type::CWideString if !transfer=>{},
            // A task/registration owner, erased owner, borrowed pointer, mutex
            // or unknown code needs its complete dedicated capability graph.
            _=>return Err(bad("actual data has no complete operational transfer/closed proof")),
        }
    }Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn concrete_operational_checks_do_not_infer_thread_or_erase_phantom_classes(){
        let limits=Limits::default();
        let bytes=include_bytes!("../semantic/fixtures/phantom-closure-complete-creator.wire");
        let(module,_)=zil_ir::wire::decode_module_candidate(bytes,&limits).unwrap();
        let zero=StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]};
        let scalar=terms::Value{raw:DynTypeValue{ty:Type::I32,life:zero.clone(),classes:vec![]},terms:vec![]};
        check(&module,&scalar,SemanticBuiltinTrait::Transfer,&mut Budget::new(&limits)).unwrap();
        check(&module,&scalar,SemanticBuiltinTrait::ClosedEnvironment,&mut Budget::new(&limits)).unwrap();
        assert!(check(&module,&scalar,SemanticBuiltinTrait::ThreadCallable,&mut Budget::new(&limits)).is_err());
        assert!(check(&module,&scalar,SemanticBuiltinTrait::Sync,&mut Budget::new(&limits)).is_err());
        let ty=module.life_instances[0].ty.clone();let Type::BorrowedStorage{life,..}=&ty else{panic!("real phantom nominal")};
        let raw=DynTypeValue{ty:ty.clone(),life:life.clone(),classes:vec![StorageBinder::Fixed(0)]};
        let value=terms::Value::anchored(&raw,terms::Owner::Semantic(0),terms::Root::Receiver,&mut Budget::new(&limits)).unwrap();
        for kind in [SemanticBuiltinTrait::Transfer,SemanticBuiltinTrait::ClosedEnvironment]{let error=check(&module,&value,kind,&mut Budget::new(&limits)).unwrap_err();assert_eq!(error.category,zil_base::ResultCategory::SafeReject);}
        let pointer=terms::Value{raw:DynTypeValue{ty:Type::Address{mutable:false,inner:Box::new(Type::I32)},life:zero,classes:vec![]},terms:vec![]};
        assert!(check(&module,&pointer,SemanticBuiltinTrait::Transfer,&mut Budget::new(&limits)).is_err());
    }
}
