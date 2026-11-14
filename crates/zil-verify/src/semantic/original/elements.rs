//! Finite Store qualification is stricter than Value/SyncDrop/Copy. A genuine
//! phantom actual stays in its type even when it contributes no physical byte.
use super::*;
impl CheckedOriginal {
    pub(super) fn store_element(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        if self.structural_premise(module,context,ty,DynStructuralBound::StoreElement,env,active,b)?{return Ok(true);}
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|Ok(match self.resolved(ty,b)?{
            Ty::Atom(ty)=>ty.is_b0_scalar()&&ty!=Type::Unit,
            Ty::Tuple(fields)=>{if fields.is_empty(){return Ok(false);}for field in fields{if !self.store_element(module,context,&field,env,active,b)?{return Ok(false);}}true},
            Ty::Array(child,length)=>{if !matches!(length.value,ConstValue::Value(Constant::Usize(n))if n>0){return Ok(false);}self.store_element(module,context,&child,env,active,b)?},
            Ty::Managed(child)=>self.resource_sync_drop(module,context,&child,env,active,b)?,
            Ty::Nominal(id,arguments)=>{
                let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{kind,body,..}=&definition.body else{return Err(bad("Store element has no exact original nominal body"));};
                if *kind==LifeNominalKind::Resource{return Ok(false);}
                let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=arguments;
                let mut nonempty=false;
                let fields=match body{SemanticNominalBody::Product{fields}=>fields.iter().map(|row|&row.ty).collect::<Vec<_>>(),SemanticNominalBody::Enum{variants}=>{if variants.is_empty(){return Ok(false);}nonempty=true;variants.iter().flat_map(|row|&row.fields).collect()}};
                for field in fields{let ty=self.ty(module,context,field,&selected,active,b)?;if ty==Ty::Atom(Type::Unit){continue;}nonempty=true;if !self.store_element(module,context,&ty,env,active,b)?{return Ok(false);}}
                nonempty&&self.sync_drop(module,context,ty,env,active,b)?
            },
            Ty::ErrorUnion(error,result)=>self.store_element(module,context,&error,env,active,b)?&&self.store_element(module,context,&result,env,active,b)?,
            Ty::Closure(..)=>self.closed_value(module,context,ty,env,active,b)?&&self.sync_drop(module,context,ty,env,active,b)?,
            _=>false,
        }))
    }
}
