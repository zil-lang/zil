//! Simultaneous original-boundary substitution. An actual is already closed in
//! the caller; its same-numbered callee formal must not be recursively read.
use super::*;
impl CheckedOriginal {
    pub(super) fn apply_type(&self,ty:&Ty,env:&Env,b:&mut Budget)->Check<Ty>{self.apply_type_at(ty,env,0,b)}
    fn apply_type_at(&self,ty:&Ty,env:&Env,depth:usize,b:&mut Budget)->Check<Ty>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original simultaneous substitution depth exceeded"));}
        let ty=self.resolved(ty,b)?;
        Ok(match ty{
            Ty::Variable(definition,ordinal)=>{
                if let Some(frame)=env.lexical.iter().find(|frame|frame.definition==definition){match frame.actuals.get(ordinal as usize){Some(Arg::Type(value))=>self.resolved(value,b)?,_=>return Err(bad("original Type substitution changes formal kind"))}}else{Ty::Variable(definition,ordinal)}
            },
            Ty::Nominal(definition,arguments)=>Ty::Nominal(definition,self.apply_arguments(&arguments,env,depth,b)?),
            Ty::Closure(definition,arguments,environment)=>Ty::Closure(definition,self.apply_arguments(&arguments,env,depth,b)?,self.apply_arguments(&environment,env,depth,b)?),
            Ty::Tuple(fields)=>{let mut result=Vec::new();for field in fields{result.push(self.apply_type_at(&field,env,depth+1,b)?);}Ty::Tuple(result)},
            Ty::Address{mutable,life,inner}=>Ty::Address{mutable,life:self.apply_life(&life,env,b)?,inner:Box::new(self.apply_type_at(&inner,env,depth+1,b)?)},
            Ty::Desc{life,inner}=>Ty::Desc{life:self.apply_life(&life,env,b)?,inner:Box::new(self.apply_type_at(&inner,env,depth+1,b)?)},
            Ty::Slice(inner)=>Ty::Slice(Box::new(self.apply_type_at(&inner,env,depth+1,b)?)),
            Ty::Managed(inner)=>Ty::Managed(Box::new(self.apply_type_at(&inner,env,depth+1,b)?)),
            Ty::Store(inner)=>Ty::Store(Box::new(self.apply_type_at(&inner,env,depth+1,b)?)),
            Ty::Array(inner,length)=>Ty::Array(Box::new(self.apply_type_at(&inner,env,depth+1,b)?),Box::new(self.apply_constant(&length,env,depth+1,b)?)),
            Ty::ErrorUnion(error,result)=>Ty::ErrorUnion(Box::new(self.apply_type_at(&error,env,depth+1,b)?),Box::new(self.apply_type_at(&result,env,depth+1,b)?)),
            Ty::Code{kind,parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}=>{let mut values=Vec::new();for parameter in parameters{values.push(self.apply_type_at(&parameter,env,depth+1,b)?);}Ty::Code{kind,parameters:values,result:Box::new(self.apply_type_at(&result,env,depth+1,b)?),modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}},
            other=>other,
        })
    }
    fn apply_arguments(&self,arguments:&[Arg],env:&Env,depth:usize,b:&mut Budget)->Check<Vec<Arg>>{let mut result=Vec::new();for argument in arguments{b.spend(1)?;result.push(match argument{Arg::Type(ty)=>Arg::Type(self.apply_type_at(ty,env,depth+1,b)?),Arg::Life(life)=>Arg::Life(self.apply_life(life,env,b)?),Arg::Const(value)=>Arg::Const(self.apply_constant(value,env,depth+1,b)?)});}Ok(result)}
    pub(super) fn apply_life(&self,life:&Life,env:&Env,b:&mut Budget)->Check<Life>{
        let life=self.resolved_life(life,b)?;b.spend(env.lexical.len())?;
        if let Life::Formal(definition,ordinal)=life{if let Some(frame)=env.lexical.iter().find(|frame|frame.definition==definition){match frame.actuals.get(ordinal as usize){Some(Arg::Life(value))=>return self.resolved_life(value,b),_=>return Err(bad("original lifetime substitution changes full-ordinal kind"))}}}Ok(life)
    }
    fn apply_constant(&self,value:&Const,env:&Env,depth:usize,b:&mut Budget)->Check<Const>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original const substitution depth exceeded"));}
        if let ConstValue::Variable(definition,ordinal)=&value.value{if let Some(frame)=env.lexical.iter().find(|frame|frame.definition==*definition){match frame.actuals.get(*ordinal as usize){Some(Arg::Const(actual))=>{b.spend(1)?;return Ok(actual.clone());},_=>return Err(bad("original Const substitution changes full-ordinal kind"))}}}
        let carrier=Box::new(self.apply_type_at(&value.carrier,env,depth+1,b)?);
        let value=match &value.value{ConstValue::Unary(op,value)=>ConstValue::Unary(*op,Box::new(self.apply_constant(value,env,depth+1,b)?)),ConstValue::Binary(op,left,right)=>ConstValue::Binary(*op,Box::new(self.apply_constant(left,env,depth+1,b)?),Box::new(self.apply_constant(right,env,depth+1,b)?)),ConstValue::Convert(value)=>ConstValue::Convert(Box::new(self.apply_constant(value,env,depth+1,b)?)),other=>other.clone()};Ok(Const{carrier,value})
    }
}
