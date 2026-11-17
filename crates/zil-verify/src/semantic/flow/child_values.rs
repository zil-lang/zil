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
        let mut life=if !opaque&&(statics.iter().any(|row|matches!(row,StaticOccurrence::Direct{..}))||parent.life.retained.iter().any(|row|row.path==path)){
            projected_life(parent,path,physical.as_deref(),b)?
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
