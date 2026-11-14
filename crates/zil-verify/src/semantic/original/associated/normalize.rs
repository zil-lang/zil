//! Projection normalization after simultaneous call substitution. It never
//! substitutes a caller's rigid variable from a concrete runtime instance.
use super::*;

impl CheckedOriginal {
    pub(in crate::semantic::original) fn normalize_original_type(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        self.normalize_original_type_at(module,context,ty,caller,active,0,b)
    }
    fn normalize_original_type_at(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,caller:&Env,active:&mut ActiveQueries,depth:usize,b:&mut Budget)->Check<Ty>{
        if depth>b.limits.max_call_depth.min(128){return Err(limit("original associated normalization depth exceeded"));}
        Ok(match self.resolved(ty,b)?{
            Ty::Projection{receiver,declaration,arguments,member}=>{
                let receiver=self.normalize_original_type_at(module,context,&receiver,caller,active,depth+1,b)?;
                let arguments=self.normalize_original_arguments(module,context,&arguments,caller,active,depth+1,b)?;
                if usage::has_inference(&receiver,&self.substitutions,b)?{Ty::Projection{receiver:Box::new(receiver),declaration,arguments,member}}
                else{self.associated_type(module,context,&receiver,declaration,&arguments,member,caller,active,b)?}
            },
            Ty::Nominal(owner,args)=>Ty::Nominal(owner,self.normalize_original_arguments(module,context,&args,caller,active,depth+1,b)?),
            Ty::Opaque(owner,args)=>Ty::Opaque(owner,self.normalize_original_arguments(module,context,&args,caller,active,depth+1,b)?),
            Ty::Closure(owner,args,sealed)=>Ty::Closure(owner,self.normalize_original_arguments(module,context,&args,caller,active,depth+1,b)?,self.normalize_original_arguments(module,context,&sealed,caller,active,depth+1,b)?),
            Ty::Tuple(fields)=>{let mut values=Vec::new();for field in fields{values.push(self.normalize_original_type_at(module,context,&field,caller,active,depth+1,b)?);}Ty::Tuple(values)},
            Ty::Address{mutable,life,inner}=>Ty::Address{mutable,life,inner:Box::new(self.normalize_original_type_at(module,context,&inner,caller,active,depth+1,b)?)},
            Ty::Desc{life,inner}=>Ty::Desc{life,inner:Box::new(self.normalize_original_type_at(module,context,&inner,caller,active,depth+1,b)?)},
            Ty::Managed(inner)=>Ty::Managed(Box::new(self.normalize_original_type_at(module,context,&inner,caller,active,depth+1,b)?)),
            Ty::Store(inner)=>Ty::Store(Box::new(self.normalize_original_type_at(module,context,&inner,caller,active,depth+1,b)?)),
            Ty::Slice(inner)=>Ty::Slice(Box::new(self.normalize_original_type_at(module,context,&inner,caller,active,depth+1,b)?)),
            Ty::Array(inner,length)=>Ty::Array(Box::new(self.normalize_original_type_at(module,context,&inner,caller,active,depth+1,b)?),length),
            Ty::ErrorUnion(error,result)=>Ty::ErrorUnion(Box::new(self.normalize_original_type_at(module,context,&error,caller,active,depth+1,b)?),Box::new(self.normalize_original_type_at(module,context,&result,caller,active,depth+1,b)?)),
            Ty::Carrier{kind,child,retained}=>{
                let retained=retained.map(|ty,b|self.normalize_original_type_at(module,context,ty,caller,active,depth+1,b),|life,b|{b.spend(1)?;Ok(life.clone())},b)?;
                Ty::Carrier{kind,child:Box::new(self.normalize_original_type_at(module,context,&child,caller,active,depth+1,b)?),retained:Box::new(retained)}
            },
            Ty::Code{kind,parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}=>{
                let mut values=Vec::new();for ty in parameters{values.push(self.normalize_original_type_at(module,context,&ty,caller,active,depth+1,b)?);}
                Ty::Code{kind,parameters:values,result:Box::new(self.normalize_original_type_at(module,context,&result,caller,active,depth+1,b)?),modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}
            },
            other=>other,
        })
    }
    fn normalize_original_arguments(&mut self,module:&TypedModule,context:&CheckedContext,args:&[Arg],caller:&Env,active:&mut ActiveQueries,depth:usize,b:&mut Budget)->Check<Vec<Arg>>{
        let mut values=Vec::new();for arg in args{b.spend(1)?;values.push(match arg{Arg::Type(ty)=>Arg::Type(self.normalize_original_type_at(module,context,ty,caller,active,depth+1,b)?),Arg::Const(value)=>{let mut value=value.clone();value.carrier=Box::new(self.normalize_original_type_at(module,context,&value.carrier,caller,active,depth+1,b)?);Arg::Const(value)},Arg::Life(life)=>Arg::Life(self.resolved_life(life,b)?)});}Ok(values)
    }
}
