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
    /// An actual inserted through a generic Formal remains caller-owned.
    /// Only this raw original source role can select another creator scope.
    pub(super) fn closure_source_creator(&self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticTypeRecipe,ty:&Ty,source:&Env,b:&mut Budget)->Check<bool>{
        let Ty::Closure(child,..)=self.resolved(ty,b)? else{return Ok(false);};
        let SemanticTypeRecipe::OriginalType{scope,source:occurrence}=recipe else{return Ok(false);};
        let frame=source.frame(*scope)?;
        Ok(context.definition(module,child,b)?.parent==Some(frame.definition)
            &&matches!(self.original_type(frame.definition,occurrence,b)?,Ty::Closure(original,..)if original==child))
    }
    /// Reuse the actual creator's complete lexical suffix. Sealed values
    /// validate that boundary; their Type shapes never infer parent actuals.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn closure_creator_environment(&mut self,module:&TypedModule,context:&CheckedContext,owner:SemanticDefinitionId,arguments:&[Arg],sealed:&[Arg],creator:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Env>{
        let definition=context.definition(module,owner,b)?;
        if definition.kind!=SemanticDefinitionKind::Closure||definition.parameters.len()!=arguments.len()||definition.environment.len()!=sealed.len(){return Err(bad("original Closure boundary changes its complete mixed argument/environment directories"));}
        for(formal,actual)in definition.parameters.iter().zip(arguments).chain(definition.environment.iter().map(|row|&row.kind).zip(sealed)){
            b.spend(1)?;
            if !matches!((formal,actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original Closure boundary changes a full-ordinal actual kind"));}
        }
        let parent=definition.parent.ok_or_else(||bad("original Closure boundary has no declared creator"))?;
        let at=creator.lexical.iter().position(|frame|frame.definition==parent).ok_or_else(||unsupported("original Closure boundary lacks its real creator lexical suffix",None))?;
        let mut selected=self.copy_environment(creator,b)?;selected.lexical.drain(..at);
        let mut ancestor=Some(parent);
        for frame in &selected.lexical{
            b.spend(1)?;
            if ancestor!=Some(frame.definition){return Err(bad("original Closure boundary changes its complete declared lexical ancestry"));}
            let original=context.definition(module,frame.definition,b)?;
            if original.parameters.len()!=frame.actuals.len()||original.environment.len()!=frame.environment.len(){return Err(bad("original Closure creator suffix omits a complete mixed frame"));}
            for(formal,actual)in original.parameters.iter().zip(&frame.actuals).chain(original.environment.iter().map(|row|&row.kind).zip(&frame.environment)){
                b.spend(1)?;if !matches!((formal,actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original Closure creator suffix changes a full-ordinal actual kind"));}
            }
            ancestor=original.parent;
        }
        if ancestor.is_some(){return Err(bad("original Closure creator suffix omits an enclosing declaration"));}
        let mut expected=Vec::new();for formal in &definition.environment{expected.push(self.actual(module,context,&formal.source,&selected,active,b)?);}
        if !self.same_arguments_readonly(&expected,sealed,b)?{return Err(bad("original Closure sealed environment differs from its exact actual creator sources"));}
        selected.lexical.insert(0,Frame{definition:owner,actuals:self.resolve_arguments(arguments,0,b)?,environment:self.resolve_arguments(sealed,0,b)?});
        Ok(selected)
    }
    pub(super) fn instantiate_prototype(&mut self,prototype:&Prototype,owner:Option<SemanticDefinitionId>,selected:&Env,b:&mut Budget)->Check<Prototype>{
        self.instantiate_prototype_roles(prototype,owner,selected,b).map(|(prototype,_)|prototype)
    }
    pub(super) fn instantiate_prototype_roles(&mut self,prototype:&Prototype,owner:Option<SemanticDefinitionId>,selected:&Env,b:&mut Budget)->Check<(Prototype,BTreeMap<Life,Life>)>{
        self.prototype_roles(prototype,owner,selected,true,b)
    }
    /// The code was already interpreted in its actual creator/caller domain.
    /// Invocation introduces its private roles without applying that closed
    /// actual vector to itself a second time (including recursive callers).
    pub(super) fn freshen_code_prototype_roles(&mut self,prototype:&Prototype,owner:Option<SemanticDefinitionId>,b:&mut Budget)->Check<(Prototype,BTreeMap<Life,Life>)>{
        self.prototype_roles(prototype,owner,&prototype.environment,false,b)
    }
    pub(super) fn freshen_code_prototype(&mut self,prototype:&Prototype,owner:Option<SemanticDefinitionId>,b:&mut Budget)->Check<Prototype>{
        self.freshen_code_prototype_roles(prototype,owner,b).map(|(prototype,_)|prototype)
    }
    fn prototype_roles(&mut self,prototype:&Prototype,owner:Option<SemanticDefinitionId>,selected:&Env,substitute:bool,b:&mut Budget)->Check<(Prototype,BTreeMap<Life,Life>)>{
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
        let mut map=BTreeMap::new();for life in candidates{
            let role=match &life{
                Life::Implicit(_,ordinal)=>vec![1,*ordinal],Life::Head(_,ordinal)=>vec![2,*ordinal],Life::Invocation(_,index)=>vec![3,*index],
                Life::Bound(_,index)=>{let depth=prototype.environment.bounds.iter().position(|frame|frame.contains(&life)).ok_or_else(||bad("original call Bound role has no exact private frame"))?;vec![4,u32::try_from(depth).map_err(|_|limit("original Call Bound role depth exceeds its complete domain"))?,*index]},
                _=>return Err(bad("original call freshens a lifetime outside its exact private role directory")),
            };
            let fresh=self.fresh_life(b)?;let Life::Infer(id)=fresh else{unreachable!()};b.spend(role.len().saturating_add(1))?;self.call_head_roles.insert(id,role);map.insert(life,Life::Infer(id));
        }
        let mut rewrite=|life:&Life,b:&mut Budget|{b.spend(1)?;Ok(map.get(life).cloned().unwrap_or_else(||life.clone()))};
        let environment=self.prototype_environment(prototype,selected,&mut rewrite,b)?;
        let mut premise_bounds=Vec::new();for frame in &prototype.premise_bounds{let mut values=Vec::new();for life in frame{values.push(rewrite(life,b)?);}premise_bounds.push(values);}
        let mut parameters=Vec::new();for ty in &prototype.parameters{let ty=self.map_invocation_type(ty,&mut rewrite,0,b)?;parameters.push(if substitute{self.apply_type(&ty,selected,b)?}else{ty});}
        let result=self.map_invocation_type(&prototype.result,&mut rewrite,0,b)?;
        let result=if substitute{self.apply_type(&result,selected,b)?}else{result};
        Ok((Prototype{parameters,result,environment,modes:prototype.modes.clone(),result_mode:prototype.result_mode,call_kind:prototype.call_kind,bound_lifetimes:prototype.bound_lifetimes,invocation_lifetimes:prototype.invocation_lifetimes,premise_bounds,callable_kind:prototype.callable_kind},map))
    }
    pub(super) fn private_prototype_frames<'a>(&self,prototype:&'a Prototype,b:&mut Budget)->Check<Vec<&'a [Life]>>{
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
    fn closure_creator_suffix_checks_all_original_mixed_sources_without_inference(){
        let limits=Limits::default();let(mut module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let program=module.semantic.as_mut().unwrap();let mut parent=program.definitions.iter().find(|row|row.kind==SemanticDefinitionKind::Function&&row.parent.is_none()).unwrap().clone();
        parent.id.index=program.definitions.iter().filter(|row|row.id.module==parent.id.module).map(|row|row.id.index).max().unwrap()+1;
        let owner=parent.id;let child=SemanticDefinitionId{module:owner.module,index:owner.index+1};
        let body=SemanticDefinitionBody::FunctionWithBody{signature:SemanticSignatureRecipe{bound_lifetimes:0,invocation_lifetimes:0,parameters:vec![],result:SemanticTypeRecipe::Atom(Type::Unit),result_mode:PassingMode::Infer,call_kind:CallKind::Direct},protocol_contracts:vec![],foreign:None,resume_bound:TaskResumeBound::Local,sites:vec![],parameters:vec![],receiver_parameter:None,names:vec![],site_nodes:vec![],body:SemanticOriginalFunctionBody::Defined(SemanticOriginalBlock{span:Default::default(),statements:vec![]})};
        parent.identity.declaration_path.push("creator_suffix_parent".into());parent.parameters=vec![SemanticFormal::Type{requirements:vec![]},SemanticFormal::Lifetime,SemanticFormal::Const{carrier:SemanticTypeRecipe::Atom(Type::Usize)}];parent.implicit_lifetimes.clear();parent.requirements.clear();parent.environment.clear();parent.body=body.clone();
        let mut closure=parent.clone();closure.id=child;closure.parent=Some(owner);closure.kind=SemanticDefinitionKind::Closure;closure.identity.declaration_path.push("child".into());closure.parameters.clear();
        let formal=|ordinal|SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal};
        closure.environment=vec![SemanticEnvironmentFormal{kind:SemanticFormal::Type{requirements:vec![]},source:SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(formal(0)))},SemanticEnvironmentFormal{kind:SemanticFormal::Lifetime,source:SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(formal(1)))},SemanticEnvironmentFormal{kind:SemanticFormal::Const{carrier:SemanticTypeRecipe::Atom(Type::Usize)},source:SemanticActualRecipe::Const{carrier:SemanticTypeRecipe::Atom(Type::Usize),value:SemanticConstRecipe::Formal(formal(2))}}];
        program.definitions.push(parent);program.definitions.push(closure);
        let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();let mut checked=CheckedOriginal::default();checked.current=Some(owner);let mut active=ActiveQueries::new();
        let values=vec![Arg::Type(address(Life::CallRegion(18))),Arg::Life(Life::CallRegion(19)),Arg::Const(Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Value(Constant::Usize(8))})];
        let creator=Env{lexical:vec![Frame{definition:owner,actuals:values.clone(),environment:vec![]}],bounds:vec![],invocation:vec![],self_type:None};
        let selected=checked.closure_creator_environment(&module,&context,child,&[],&values,&creator,&mut active,&mut b).unwrap();assert_eq!(selected.lexical[1].actuals,values);
        let mut wrong=values.clone();wrong[1]=Arg::Life(Life::CallRegion(20));assert!(checked.closure_creator_environment(&module,&context,child,&[],&wrong,&creator,&mut active,&mut b).is_err());
        let mut wrong_creator=creator.clone();wrong_creator.lexical[0].actuals.swap(0,1);assert!(checked.closure_creator_environment(&module,&context,child,&[],&values,&wrong_creator,&mut active,&mut b).is_err());
        wrong_creator=creator.clone();wrong_creator.lexical.clear();assert!(checked.closure_creator_environment(&module,&context,child,&[],&values,&wrong_creator,&mut active,&mut b).is_err());
        let inner=Ty::Closure(child,vec![],values.clone());
        checked.bindings.insert((owner,NodeId(7)),inner.clone());
        let source=SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Binding(NodeId(7))};
        assert!(checked.closure_source_creator(&module,&context,&source,&inner,&creator,&mut b).unwrap(),"a nested captured sibling keeps its checked original source scope");
        checked.bindings.insert((owner,NodeId(8)),Ty::Variable(owner,0));
        let inserted=SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Binding(NodeId(8))};
        assert!(!checked.closure_source_creator(&module,&context,&inserted,&inner,&creator,&mut b).unwrap(),"a whole Type inserted through a caller Formal does not select an ambient same-id creator");
        assert!(!checked.closure_source_creator(&module,&context,&SemanticTypeRecipe::Formal(formal(0)),&inner,&creator,&mut b).unwrap());
        assert!(checked.substitutions.is_empty());assert!(checked.life_substitutions.is_empty(),"a sealed boundary check grants no global identity equation");
    }
    #[test]
    fn closed_code_instantiation_does_not_resubstitute_recursive_parent_actuals(){
        let parent=SemanticDefinitionId{module:zil_base::ModuleId(0),index:4};
        let child=SemanticDefinitionId{module:parent.module,index:5};
        let scalar=Ty::Atom(Type::I32);let variable=|ordinal|Ty::Variable(parent,ordinal);
        let constant=|ordinal|Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Variable(parent,ordinal)};
        let env=Env{lexical:vec![Frame{definition:child,actuals:vec![],environment:vec![]},Frame{definition:parent,actuals:vec![Arg::Life(Life::Formal(parent,0)),Arg::Life(Life::Formal(parent,1)),Arg::Type(variable(2)),Arg::Type(variable(3)),Arg::Const(constant(4)),Arg::Const(constant(5))],environment:vec![]}],bounds:vec![],invocation:vec![],self_type:None};
        let prototype=Prototype{parameters:vec![],result:Ty::Tuple(vec![address(Life::Formal(parent,0)),variable(2),Ty::Array(Box::new(scalar.clone()),Box::new(constant(4)))]),environment:env,modes:vec![],result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0,premise_bounds:vec![],callable_kind:Some(CallableKind::FnOnce)};
        let mut selected=prototype.environment.clone();selected.lexical[1].actuals=vec![Arg::Life(Life::Formal(parent,1)),Arg::Life(Life::Static),Arg::Type(variable(3)),Arg::Type(scalar),Arg::Const(constant(5)),Arg::Const(Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Value(Constant::Usize(7))})];
        let mut checked=CheckedOriginal::default();let mut b=Budget::new(&Limits::default());
        let expected=Ty::Tuple(vec![address(Life::Formal(parent,1)),variable(3),Ty::Array(Box::new(Ty::Atom(Type::I32)),Box::new(constant(5)))]);
        let direct=checked.instantiate_prototype(&prototype,Some(child),&selected,&mut b).unwrap();assert_eq!(direct.result,expected);
        let closed=checked.specialize_code_prototype(&prototype,&selected,&mut b).unwrap();assert_eq!(closed.result,expected);
        let first=checked.freshen_code_prototype(&closed,None,&mut b).unwrap();let second=checked.freshen_code_prototype(&closed,None,&mut b).unwrap();
        assert_eq!(first.result,expected);assert_eq!(second.result,expected);
    }
    #[test]
    fn raw_closure_freshens_before_inserting_an_existing_caller_whole_type(){
        let parent=SemanticDefinitionId{module:zil_base::ModuleId(0),index:4};let child=SemanticDefinitionId{module:parent.module,index:5};
        let variable=Ty::Variable(parent,0);let caller=address(Life::Implicit(child,0));
        let env=Env{lexical:vec![Frame{definition:child,actuals:vec![],environment:vec![]},Frame{definition:parent,actuals:vec![Arg::Type(variable.clone())],environment:vec![]}],bounds:vec![],invocation:vec![],self_type:None};
        let prototype=Prototype{parameters:vec![variable.clone()],result:variable,environment:env,modes:vec![PassingMode::Infer],result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0,premise_bounds:vec![],callable_kind:Some(CallableKind::Fn)};
        let mut selected=prototype.environment.clone();selected.lexical[1].actuals=vec![Arg::Type(caller.clone())];
        let mut checked=CheckedOriginal::default();let mut b=Budget::new(&Limits::default());
        let (actual,roles)=checked.instantiate_prototype_roles(&prototype,Some(child),&selected,&mut b).unwrap();
        assert!(roles.is_empty(),"the inserted caller's same-numbered head is not a private code role");
        assert_eq!(actual.parameters,vec![caller.clone()]);assert_eq!(actual.result,caller);
    }
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

impl CheckedOriginal {
    /// A region belongs to this exact Call and its original root parameter
    /// occurrences. Parameter selectors avoid guessing a private Bound token
    /// from a caller frame, and do not descend into an existing Type's κ.
    pub(super) fn remember_call_region_parameters(&mut self,parameters:&[Ty],node:NodeId,env:&Env,b:&mut Budget)->Check<()> {
        let owner=env.current()?;self.original_call_site(owner,node,b)?;let mut rows=Vec::new();
        for(ordinal,parameter)in parameters.iter().enumerate(){
            let life=match self.resolved(parameter,b)?{Ty::Address{life,..}|Ty::Desc{life,..}=>life,_=>continue};
            let Life::CallRegion(id)=life else{continue;};
            let Some((definition,call,_,_))=self.call_regions.get(&id)else{return Err(bad("original Call parameter selects an absent region"));};
            if *definition!=owner||*call!=node{continue;}
            b.spend(1)?;rows.push((u32::try_from(ordinal).map_err(|_|limit("original Call region parameter exceeds its full domain"))?,id));
        }
        if !rows.is_empty(){b.spend(rows.len())?;self.call_region_parameters.insert((owner,node),rows);}Ok(())
    }
}
