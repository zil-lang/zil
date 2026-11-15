//! Project a complete child domain through its declared occurrence directory.
//! Parent images may coincide; each child κ remains injective and its original
//! intrinsic identities survive. This supplies no runtime pointer or loan.
use super::*;
impl CheckedContext {
    pub(in crate::semantic) fn child_value(&self,module:&TypedModule,parent:&CheckedType,path:&[LifeTypeProjection],b:&mut Budget)->Check<CheckedType>{
        if path.is_empty(){
            b.spend(parent.classes.len().saturating_add(parent.origins.len()))?;
            return Ok(parent.clone());
        }
        let ty=lifetimes::select(&parent.ty,path,b)?;
        let mut life=crate::tasks::frame_shape::declared_life(module,ty,b)?;
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
        // A static outer occurrence is absent from κ. The whole pointee
        // domain and every logical/static invariant remain represented.
        if matches!(ty,Type::Address{..}|Type::Desc(_))&&statics.iter().any(|row|matches!(row,StaticOccurrence::Direct{path,..}if path.is_empty())) {
            if !life.slots.iter().any(|row|row.path.is_empty()&&row.binder==0){return Err(bad("static child omits its exact outer occurrence"));}
            life.binder_count=life.binder_count.checked_sub(1).ok_or_else(||bad("static child has no outer class to remove"))?;
            life.slots.retain(|row|!(row.path.is_empty()&&row.binder==0));
            for row in &mut life.slots{row.binder=row.binder.checked_sub(1).ok_or_else(||bad("static child removes an inner class"))?;}
            for row in &mut life.retained{row.binder=row.binder.checked_sub(1).ok_or_else(||bad("static child removes an intrinsic class"))?;}
            for row in &mut life.nominal_arguments{row.binder=row.binder.checked_sub(1).ok_or_else(||bad("static child removes a nominal class"))?;}
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
