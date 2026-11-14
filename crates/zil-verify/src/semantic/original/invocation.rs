//! Call instantiation freshens the callee's own input heads before substituting
//! actual Types. A recursive actual T already belongs to the caller; traversing
//! it afterwards to freshen same-numbered lifetimes would change its identity.
use super::*;
impl CheckedOriginal {
    pub(super) fn call_signature(&mut self,module:&TypedModule,context:&CheckedContext,target:SemanticDefinitionId,signature:&SemanticSignatureRecipe,selected:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Prototype>{
        let rigid=self.environment(module,context,target,active,b)?;
        let prototype=self.signature(module,context,signature,&rigid,active,b)?;
        self.instantiate_prototype(&prototype,Some(target),selected,b)
    }
    pub(super) fn instantiate_prototype(&mut self,prototype:&Prototype,owner:Option<SemanticDefinitionId>,selected:&Env,b:&mut Budget)->Check<Prototype>{
        let mut candidates=BTreeSet::new();
        for parameter in &prototype.parameters{self.map_invocation_type(parameter,&mut |life,b|{
            b.spend(1)?;
            let local=matches!(life,Life::Implicit(id,_)|Life::Head(id,_) if Some(*id)==owner);
            let invocation=prototype.environment.invocation.contains(life);
            let bound=prototype.bound_lifetimes!=0&&prototype.environment.bounds.first().is_some_and(|frame|frame.contains(life));
            if local||invocation||bound{candidates.insert(life.clone());}Ok(life.clone())
        },0,b)?;}
        let mut map=BTreeMap::new();for life in candidates{map.insert(life,self.fresh_life(b)?);}
        let mut rewrite=|life:&Life,b:&mut Budget|{b.spend(1)?;Ok(map.get(life).cloned().unwrap_or_else(||life.clone()))};
        let mut parameters=Vec::new();for ty in &prototype.parameters{let ty=self.map_invocation_type(ty,&mut rewrite,0,b)?;parameters.push(self.apply_type(&ty,selected,b)?);}
        let result=self.map_invocation_type(&prototype.result,&mut rewrite,0,b)?;
        Ok(Prototype{parameters,result:self.apply_type(&result,selected,b)?,environment:selected.clone(),modes:prototype.modes.clone(),result_mode:prototype.result_mode,call_kind:prototype.call_kind,bound_lifetimes:prototype.bound_lifetimes,invocation_lifetimes:prototype.invocation_lifetimes,callable_kind:prototype.callable_kind})
    }
    pub(super) fn map_invocation_type(&self,ty:&Ty,life:&mut impl FnMut(&Life,&mut Budget)->Check<Life>,depth:usize,b:&mut Budget)->Check<Ty>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original call input domain depth exceeded"));}
        Ok(match self.resolved(ty,b)?{
            Ty::Address{mutable,life:outer,inner}=>Ty::Address{mutable,life:life(&outer,b)?,inner:Box::new(self.map_invocation_type(&inner,life,depth+1,b)?)},
            Ty::Desc{life:outer,inner}=>Ty::Desc{life:life(&outer,b)?,inner:Box::new(self.map_invocation_type(&inner,life,depth+1,b)?)},
            Ty::Managed(child)=>Ty::Managed(Box::new(self.map_invocation_type(&child,life,depth+1,b)?)),
            Ty::Store(child)=>Ty::Store(Box::new(self.map_invocation_type(&child,life,depth+1,b)?)),
            Ty::Slice(child)=>Ty::Slice(Box::new(self.map_invocation_type(&child,life,depth+1,b)?)),
            Ty::Array(child,length)=>Ty::Array(Box::new(self.map_invocation_type(&child,life,depth+1,b)?),length),
            Ty::Tuple(fields)=>{let mut values=Vec::new();for field in fields{values.push(self.map_invocation_type(&field,life,depth+1,b)?);}Ty::Tuple(values)},
            Ty::ErrorUnion(error,result)=>Ty::ErrorUnion(Box::new(self.map_invocation_type(&error,life,depth+1,b)?),Box::new(self.map_invocation_type(&result,life,depth+1,b)?)),
            Ty::Nominal(id,arguments)=>Ty::Nominal(id,self.map_invocation_arguments(&arguments,life,depth+1,b)?),
            Ty::Opaque(id,arguments)=>Ty::Opaque(id,self.map_invocation_arguments(&arguments,life,depth+1,b)?),
            Ty::Projection{receiver,declaration,arguments,member}=>Ty::Projection{receiver:Box::new(self.map_invocation_type(&receiver,life,depth+1,b)?),declaration,arguments:self.map_invocation_arguments(&arguments,life,depth+1,b)?,member},
            Ty::Closure(id,arguments,environment)=>Ty::Closure(id,self.map_invocation_arguments(&arguments,life,depth+1,b)?,self.map_invocation_arguments(&environment,life,depth+1,b)?),
            Ty::Carrier{kind,child,retained}=>{
                // map()'s two closures cannot simultaneously borrow one life
                // mapper. Walk a closed copy in two passes: first children,
                // then only the shape's own class occurrences.
                let mut mapped=retained.map(|ty,b|self.map_invocation_type(ty,life,depth+1,b),|value,b|{b.spend(1)?;Ok(value.clone())},b)?;
                mapped=mapped.map(|ty,b|{b.spend(1)?;Ok(ty.clone())},|value,b|life(value,b),b)?;
                Ty::Carrier{kind,child:Box::new(self.map_invocation_type(&child,life,depth+1,b)?),retained:Box::new(mapped)}
            },
            Ty::Code{kind,parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}=>{let mut values=Vec::new();for parameter in parameters{values.push(self.map_invocation_type(&parameter,life,depth+1,b)?);}Ty::Code{kind,parameters:values,result:Box::new(self.map_invocation_type(&result,life,depth+1,b)?),modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}},
            value=>value,
        })
    }
    fn map_invocation_arguments(&self,args:&[Arg],life:&mut impl FnMut(&Life,&mut Budget)->Check<Life>,depth:usize,b:&mut Budget)->Check<Vec<Arg>>{let mut values=Vec::new();for arg in args{b.spend(1)?;values.push(match arg{Arg::Type(ty)=>Arg::Type(self.map_invocation_type(ty,life,depth,b)?),Arg::Life(value)=>Arg::Life(life(value,b)?),Arg::Const(value)=>Arg::Const(value.clone())});}Ok(values)}
}
