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
        self.instantiate_prototype_roles(prototype,owner,selected,b).map(|(prototype,_)|prototype)
    }
    pub(super) fn instantiate_prototype_roles(&mut self,prototype:&Prototype,owner:Option<SemanticDefinitionId>,selected:&Env,b:&mut Budget)->Check<(Prototype,BTreeMap<Life,Life>)>{
        let mut candidates=BTreeSet::new();
        self.private_prototype_frames(prototype,b)?;
        b.spend(prototype.environment.invocation.len().saturating_add(1))?;
        for life in &prototype.environment.invocation{if !candidates.insert(life.clone()){return Err(bad("original invocation directory repeats a private role"));}}
        if prototype.bound_lifetimes!=0{for life in &prototype.environment.bounds[0]{if !candidates.insert(life.clone()){return Err(bad("original Bound directory repeats another private role"));}}}
        for frame in &prototype.premise_bounds{for life in frame{b.spend(1)?;if !candidates.insert(life.clone()){return Err(bad("original quantified premise coalesces independent roles"));}}}
        // Implicit input heads are fresh in addition to the complete explicit
        // code directories. Result-only and phantom explicit roles above are
        // never discovered or omitted by scanning physical input occurrences.
        for parameter in &prototype.parameters{self.map_invocation_type(parameter,&mut |life,b|{
            b.spend(1)?;
            if matches!(life,Life::Implicit(id,_)|Life::Head(id,_) if Some(*id)==owner){candidates.insert(life.clone());}
            Ok(life.clone())
        },0,b)?;}
        let mut map=BTreeMap::new();for life in candidates{map.insert(life,self.fresh_life(b)?);}
        let mut rewrite=|life:&Life,b:&mut Budget|{b.spend(1)?;Ok(map.get(life).cloned().unwrap_or_else(||life.clone()))};
        let environment=self.prototype_environment(prototype,selected,&mut rewrite,b)?;
        let mut premise_bounds=Vec::new();for frame in &prototype.premise_bounds{let mut values=Vec::new();for life in frame{values.push(rewrite(life,b)?);}premise_bounds.push(values);}
        let mut parameters=Vec::new();for ty in &prototype.parameters{let ty=self.map_invocation_type(ty,&mut rewrite,0,b)?;parameters.push(self.apply_type(&ty,selected,b)?);}
        let result=self.map_invocation_type(&prototype.result,&mut rewrite,0,b)?;
        Ok((Prototype{parameters,result:self.apply_type(&result,selected,b)?,environment,modes:prototype.modes.clone(),result_mode:prototype.result_mode,call_kind:prototype.call_kind,bound_lifetimes:prototype.bound_lifetimes,invocation_lifetimes:prototype.invocation_lifetimes,premise_bounds,callable_kind:prototype.callable_kind},map))
    }
    fn private_prototype_frames<'a>(&self,prototype:&'a Prototype,b:&mut Budget)->Check<Vec<&'a [Life]>>{
        if prototype.environment.invocation.len()!=prototype.invocation_lifetimes as usize{return Err(bad("original prototype lacks its complete Invocation directory"));}
        let mut frames=Vec::new();
        if prototype.bound_lifetimes!=0{
            let own=prototype.environment.bounds.first().ok_or_else(||bad("original prototype lacks its own Bound frame"))?;
            if own.len()!=prototype.bound_lifetimes as usize{return Err(bad("original prototype changes its own Bound domain"));}
            b.spend(2)?;frames.push(own.as_slice());
        }
        // These stay separate scopes, innermost first in Env and outermost
        // first in their own declaration directory. No frame is flattened.
        for frame in prototype.premise_bounds.iter().rev(){if frame.is_empty(){return Err(bad("zero quantified premise incorrectly consumes Bound depth"));}b.spend(2)?;frames.push(frame.as_slice());}
        b.spend(frames.len())?;
        for(index,frame)in frames.iter().enumerate(){
            b.spend(frame.len().saturating_add(1))?;
            if prototype.environment.bounds.get(index).is_none_or(|actual|actual.as_slice()!=*frame){return Err(bad("original prototype private scopes differ from their exact declared nesting"));}
        }
        Ok(frames)
    }
    fn prototype_environment(&self,prototype:&Prototype,selected:&Env,rewrite:&mut impl FnMut(&Life,&mut Budget)->Check<Life>,b:&mut Budget)->Check<Env>{
        let frames=self.private_prototype_frames(prototype,b)?;
        let mut environment=self.copy_environment(selected,b)?;
        b.spend(frames.iter().fold(frames.len(),|n,frame|n.saturating_add(frame.len())))?;
        // A caller may pass this prototype's existing environment, or a new
        // lexical substitution. Remove only the same exact private prefix.
        if !frames.is_empty()&&environment.bounds.first().is_some_and(|frame|frame.as_slice()==frames[0]){
            for(index,frame)in frames.iter().enumerate(){if environment.bounds.get(index).is_none_or(|actual|actual.as_slice()!=*frame){return Err(bad("original specialization carries only part of a private quantifier directory"));}}
            environment.bounds.drain(..frames.len());
        }
        let mut instantiated=Vec::new();for frame in frames{let mut values=Vec::new();for life in frame{values.push(rewrite(life,b)?);}instantiated.push(values);}
        environment.bounds.splice(0..0,instantiated);
        environment.invocation.clear();for life in &prototype.environment.invocation{environment.invocation.push(rewrite(life,b)?);}
        Ok(environment)
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
            Ty::Dyn(value)=>Ty::Dyn(Box::new(value.map(|arg,b|Ok(self.map_invocation_arguments(std::slice::from_ref(arg),life,depth+1,b)?.remove(0)),b)?)),
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

impl CheckedOriginal {
    /// Specialize a capability's original code scheme without calling it.
    /// Its own quantifiers remain complete and rigid for local alpha checking;
    /// invocation freshening belongs to the later actual call occurrence.
    pub(super) fn specialize_code_prototype(&self,prototype:&Prototype,selected:&Env,b:&mut Budget)->Check<Prototype>{
        let environment=self.prototype_environment(prototype,selected,&mut |life,b|{b.spend(1)?;Ok(life.clone())},b)?;
        b.spend(prototype.premise_bounds.iter().fold(prototype.premise_bounds.len(),|n,frame|n.saturating_add(frame.len())))?;
        let mut parameters=Vec::new();for parameter in &prototype.parameters{parameters.push(self.apply_type(parameter,selected,b)?);}
        Ok(Prototype{parameters,result:self.apply_type(&prototype.result,selected,b)?,environment,modes:prototype.modes.clone(),result_mode:prototype.result_mode,call_kind:prototype.call_kind,bound_lifetimes:prototype.bound_lifetimes,invocation_lifetimes:prototype.invocation_lifetimes,premise_bounds:prototype.premise_bounds.clone(),callable_kind:prototype.callable_kind})
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn address(life:Life)->Ty{Ty::Address{mutable:false,life,inner:Box::new(Ty::Atom(Type::I32))}}
    #[test]
    fn quantified_premises_keep_nested_phantom_result_and_external_roles(){
        let bound=vec![Life::Bound(FrameToken(200),0),Life::Bound(FrameToken(200),1)];
        let invocation=vec![Life::Invocation(FrameToken(201),0),Life::Invocation(FrameToken(201),1)];
        let outer=vec![Life::Bound(FrameToken(300),0),Life::Bound(FrameToken(300),1)];
        let inner=vec![Life::Bound(FrameToken(301),0)];
        let external=Life::Bound(FrameToken(500),0);
        let prototype=Prototype{
            parameters:vec![address(outer[0].clone())],
            result:Ty::Tuple(vec![address(invocation[0].clone()),address(bound[1].clone()),address(inner[0].clone()),address(external.clone())]),
            environment:Env{lexical:vec![],bounds:vec![bound,inner.clone(),outer.clone(),vec![external.clone()]],invocation,self_type:None},
            modes:vec![PassingMode::Infer],result_mode:PassingMode::Infer,call_kind:CallKind::Direct,
            bound_lifetimes:2,invocation_lifetimes:2,premise_bounds:vec![outer,inner],callable_kind:Some(CallableKind::FnMut),
        };
        let mut checked=CheckedOriginal::default();let mut budget=Budget::new(&Limits::default());
        let first=checked.instantiate_prototype(&prototype,None,&prototype.environment,&mut budget).unwrap();
        let second=checked.instantiate_prototype(&prototype,None,&prototype.environment,&mut budget).unwrap();
        let roles=|value:&Prototype|value.environment.bounds[..3].iter().flatten().chain(&value.environment.invocation).cloned().collect::<BTreeSet<_>>();
        let a=roles(&first);let z=roles(&second);
        assert_eq!(a.len(),7);assert_eq!(z.len(),7);assert!(a.is_disjoint(&z));
        assert!(a.iter().chain(&z).all(|life|matches!(life,Life::Infer(_))));
        for value in [&first,&second]{
            assert_eq!(value.bound_lifetimes,2);assert_eq!(value.invocation_lifetimes,2);
            assert_eq!(value.premise_bounds.iter().map(Vec::len).collect::<Vec<_>>(),vec![2,1]);
            assert_eq!(value.environment.bounds[3],vec![external.clone()]);
            assert_eq!(value.parameters[0],address(value.premise_bounds[0][0].clone()));
            let Ty::Tuple(result)=&value.result else{panic!()};
            assert_eq!(result[0],address(value.environment.invocation[0].clone()));
            assert_eq!(result[1],address(value.environment.bounds[0][1].clone()));
            assert_eq!(result[2],address(value.premise_bounds[1][0].clone()));
            assert_eq!(result[3],address(external.clone()));
        }
        let mut coalesced=prototype.clone();
        coalesced.premise_bounds[1][0]=coalesced.premise_bounds[0][0].clone();
        coalesced.environment.bounds[1]=coalesced.premise_bounds[1].clone();
        assert!(checked.instantiate_prototype(&coalesced,None,&coalesced.environment,&mut budget).is_err());
        let mut missing=prototype.clone();missing.environment.bounds.remove(1);
        assert!(checked.instantiate_prototype(&missing,None,&missing.environment,&mut budget).is_err());
    }
}
