//! Complete finite zero-domain values are reconstructed, never inferred from
//! an empty row list or a None contract. Logical/physical borrow children and
//! static pending occurrences keep their separate consumers.
use super::*;
impl CheckedContext {
    pub(super) fn closed_value(&self,module:&TypedModule,ty:&Type,b:&mut Budget)->Check<CheckedType>{
        let mut pending=vec![ty];let mut closures=std::collections::BTreeSet::new();
        while let Some(ty)=pending.pop(){b.spend(1)?;match ty{
            Type::Address{..}|Type::Desc(_)=>return Err(unsupported("closed value contains a real outer borrow occurrence",None)),
            Type::BorrowedStorage{storage,life}=>{if *life!=empty_life(){return Err(unsupported("complete value has a nonzero qualified/phantom domain",None));}pending.push(storage);},
            Type::Tuple(fields)=>pending.extend(fields),Type::Record{fields,..}|Type::Package{fields,..}=>pending.extend(fields.iter().map(|(_,ty)|ty)),
            Type::Enum{variants,..}=>pending.extend(variants.iter().flat_map(|(_,fields)|fields)),Type::ErrorUnion{error,result}=>{pending.push(error);pending.push(result);},
            Type::Managed(inner)|Type::Array{element:inner,..}|Type::OwnedStore{element:inner}|Type::Mutex{payload:inner}|Type::Registration{environment:inner}|Type::AsyncRegistration{environment:inner}=>pending.push(inner),
            Type::Task{result,retained_life}=>{if *retained_life!=(RetainedLifeShape{binder_count:0,ports:vec![],retained:vec![],nominal_arguments:vec![]}){return Err(unsupported("Task retains a nonzero or incomplete actual domain",None));}pending.push(result);},
            Type::Closure(id)=>{
                if !closures.insert(*id){continue;}let definition=module.closures.iter().find(|row|row.id==*id).ok_or_else(||bad("closed value Closure definition is absent"))?;
                let relation=definition.storage_life.as_ref().ok_or_else(||unsupported("closed value Closure complete scope is Unknown",None))?;
                if relation.fixed_binder_count!=0||!relation.static_bindings.is_empty(){return Err(unsupported("Closure still has free/static source obligations",None));}
                pending.extend(&definition.captures);
            },
            Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..}=>return Err(unsupported("opaque value requires its full checked manager/port consumer",None)),
            Type::Slice{..}|Type::FunctionPointer{..}|Type::BorrowedFunctionPointer{..}|Type::ForeignCallback{..}=>return Err(unsupported("closed value requires its complete code/unsized scope consumer",None)),
            _=>{},
        }}
        let(count,_)=dynamic_witness::intrinsic_domain(module,ty,b)?;if count!=0{return Err(bad("complete value inventory has a nonzero intrinsic domain"));}
        self.checked_value(module,ty,&empty_life(),vec![],b)
    }
}
