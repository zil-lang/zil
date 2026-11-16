//! Operational capabilities are distinct original premises. A concrete F
//! never supplies a capability missing from the abstract caller's declaration.
use super::*;
impl CheckedOriginal {
    pub(super) fn builtin_premise(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,kind:SemanticBuiltinTrait,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        if self.current!=Some(env.current()?){return Err(bad("operational premise queried in another original caller domain"));}
        for(depth,frame)in env.lexical.iter().enumerate(){
            let definition=context.definition(module,frame.definition,b)?;b.spend(env.lexical.len().saturating_add(definition.requirements.len()).saturating_add(definition.parameters.len()))?;
            let mut selected=env.clone();selected.lexical=selected.lexical[depth..].to_vec();
            let mut pending=definition.requirements.iter().collect::<Vec<_>>();for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{pending.extend(requirements);}}
            while let Some(row)=pending.pop(){b.spend(1)?;match row{
                SemanticRequirementRecipe::Builtin{ty:recipe,trait_kind} if *trait_kind==kind=>{let candidate=self.ty(module,context,recipe,&selected,active,b)?;if self.same(&candidate,ty,b)?{return Ok(true);}},
                SemanticRequirementRecipe::ForAll{lifetimes:0,requirements}=>{b.spend(requirements.len())?;pending.extend(requirements);},
                _=>{},
            }}
        }Ok(false)
    }
    pub(super) fn builtin_obligation(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,kind:SemanticBuiltinTrait,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if self.builtin_premise(module,context,ty,kind,env,active,b)?{return Ok(());}
        let proved=match kind{
            SemanticBuiltinTrait::ClosedEnvironment=>self.closed_value(module,context,ty,env,active,b)?,
            SemanticBuiltinTrait::Transfer=>self.transfer_value(module,context,ty,env,active,b)?,
            // Executing an entry remotely and simultaneous shared access each
            // require a real complete graph/role proof. Neither follows from
            // moving a value, from Copy, or from ClosedEnvironment.
            SemanticBuiltinTrait::ThreadCallable=>self.thread_callable(module,context,ty,env,active,b)?,
            SemanticBuiltinTrait::Share|SemanticBuiltinTrait::Send|SemanticBuiltinTrait::Sync|SemanticBuiltinTrait::Drop=>false,
            _=>return Err(bad("structural capability entered the operational checker")),
        };
        if proved{Ok(())}else{Err(unsupported("original caller lacks the exact operational capability/entry proof",None))}
    }
    pub(super) fn closed_value(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            if self.builtin_premise(module,context,ty,SemanticBuiltinTrait::ClosedEnvironment,env,active,b)?{return Ok(true);}
            Ok(match self.resolved(ty,b)?{
                Ty::SealedError(_)=>true,
            Ty::Atom(ty)=>ty.is_b0_scalar()||matches!(ty,Type::ByteBuffer|Type::ForeignHandle|Type::CWideString),
                Ty::Address{mutable:false,life,inner}if self.resolved_life(&life,b)?==Life::Static=>{
                    // An exact original program-static shared view has no
                    // activation dependency. Its complete pointee must still
                    // close; a Slice is inspected only beneath this real head.
                    // Every actual literal/constructor/return Static source
                    // remains pending in the independent memory kernel.
                    match *inner{Ty::Slice(element)=>self.closed_value(module,context,&element,env,active,b)?,inner=>self.closed_value(module,context,&inner,env,active,b)?}
                },
                Ty::Address{..}|Ty::Desc{..}|Ty::Slice(_)|Ty::Variable(..)|Ty::SelfType(_)|Ty::Projection{..}|Ty::Infer(_)=>false,
                Ty::Opaque(owner,arguments)=>{
                    for argument in &arguments{match argument{Arg::Life(_)=>return Ok(false),Arg::Type(ty)=>if !self.closed_value(module,context,ty,env,active,b)?{return Ok(false);},Arg::Const(_)=>{}}}
                    let hidden=self.opaque_hidden(module,context,owner,&arguments,active,b)?;
                    self.closed_value(module,context,&hidden,env,active,b)?
                },
                Ty::Tuple(fields)=>{for field in fields{if !self.closed_value(module,context,&field,env,active,b)?{return Ok(false);}}true},
                Ty::Array(child,_)|Ty::Store(child)|Ty::Managed(child)=>self.closed_value(module,context,&child,env,active,b)?,
                Ty::Carrier{child,retained,..}=>{if !self.retained_zero_domain(module,context,&retained,env,active,b,0)?{return Ok(false);}self.closed_value(module,context,&child,env,active,b)?},
                Ty::ErrorUnion(error,result)=>self.closed_value(module,context,&error,env,active,b)?&&self.closed_value(module,context,&result,env,active,b)?,
                Ty::Dyn(value)=>{if !value.owned||value.explicit_life(b)?{return Ok(false);}let mut types=Vec::new();value.types(&mut types,b)?;for ty in types{if !self.closed_value(module,context,ty,env,active,b)?{return Ok(false);}}true},
                Ty::Function(id)=>self.checked.contains(&id),
                Ty::Code{bound_lifetimes,invocation_lifetimes,parameters,result,..}=>{if bound_lifetimes!=0||invocation_lifetimes!=0{return Ok(false);}for parameter in parameters{if !self.closed_value(module,context,&parameter,env,active,b)?{return Ok(false);}}self.closed_value(module,context,&result,env,active,b)?},
                Ty::Closure(definition,arguments,sealed)=>{
                    for argument in arguments.iter().chain(&sealed){match argument{Arg::Life(_)=>return Ok(false),Arg::Type(ty)=>if !self.closed_value(module,context,ty,env,active,b)?{return Ok(false);},Arg::Const(_)=>{}}}
                    if !self.checked.contains(&definition){return Ok(false);}
                    let captures=self.closure_capture_types(module,context,definition,&arguments,&sealed,b)?;
                    if !self.creator_moves.get(&definition).copied().unwrap_or(false)&&!captures.is_empty(){return Ok(false);}
                    for capture in captures{if !self.closed_value(module,context,&capture,env,active,b)?{return Ok(false);}}true
                },
                Ty::Nominal(id,arguments)=>self.nominal_capability(module,context,id,&arguments,SemanticBuiltinTrait::ClosedEnvironment,env,active,b)?,
            })
        })
    }
    fn transfer_value(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        if self.builtin_premise(module,context,ty,SemanticBuiltinTrait::Transfer,env,active,b)?{return Ok(true);}
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|Ok(match self.resolved(ty,b)?{
            Ty::SealedError(_)=>true,
            Ty::Atom(ty)=>ty.is_b0_scalar()||ty==Type::ByteBuffer,
            Ty::Managed(child)|Ty::Store(child)|Ty::Array(child,_)=>self.transfer_value(module,context,&child,env,active,b)?,
            Ty::Tuple(fields)=>{for field in fields{if !self.transfer_value(module,context,&field,env,active,b)?{return Ok(false);}}true},
            Ty::ErrorUnion(a,c)=>self.transfer_value(module,context,&a,env,active,b)?&&self.transfer_value(module,context,&c,env,active,b)?,
            Ty::Nominal(id,arguments)=>{if self.custom_drop(module,id,b)?{return Ok(false);}self.nominal_capability(module,context,id,&arguments,SemanticBuiltinTrait::Transfer,env,active,b)?},
            Ty::Closure(definition,arguments,sealed)=>{
                if !self.checked.contains(&definition)||!self.creator_moves.get(&definition).copied().unwrap_or(false){return Ok(false);}
                for argument in arguments.iter().chain(&sealed){match argument{Arg::Life(_)=>return Ok(false),Arg::Type(ty)=>if !self.transfer_value(module,context,ty,env,active,b)?{return Ok(false);},Arg::Const(_)=>{}}}
                let captures=self.closure_capture_types(module,context,definition,&arguments,&sealed,b)?;
                for capture in captures{if !self.transfer_value(module,context,&capture,env,active,b)?{return Ok(false);}}true
            },
            Ty::Opaque(owner,arguments)=>{
                for argument in &arguments{match argument{Arg::Life(_)=>return Ok(false),Arg::Type(ty)=>if !self.transfer_value(module,context,ty,env,active,b)?{return Ok(false);},Arg::Const(_)=>{}}}
                let hidden=self.opaque_hidden(module,context,owner,&arguments,active,b)?;
                self.transfer_value(module,context,&hidden,env,active,b)?
            },
            // Concrete callback cleanup and remote-entry obligations belong to
            // the actual entry graph; do not infer them from capture spelling.
            _=>false,
        }))
    }
    fn nominal_capability(&mut self,module:&TypedModule,context:&CheckedContext,id:SemanticDefinitionId,args:&[Arg],kind:SemanticBuiltinTrait,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{body,..}=&definition.body else{return Err(bad("capability receiver lacks its original nominal body"));};
        // Life and Type actuals remain part of the whole nominal domain even
        // when absent from physical fields. Phantom borrowing is not closed.
        for argument in args{match argument{Arg::Life(_)=>return Ok(false),Arg::Type(ty)=>{let ok=if kind==SemanticBuiltinTrait::Transfer{self.transfer_value(module,context,ty,caller,active,b)?}else{self.closed_value(module,context,ty,caller,active,b)?};if !ok{return Ok(false);}},Arg::Const(_)=>{}}}
        let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=args.to_vec();
        let fields=match body{SemanticNominalBody::Product{fields}=>fields.iter().map(|row|&row.ty).collect::<Vec<_>>(),SemanticNominalBody::Enum{variants}=>variants.iter().flat_map(|row|&row.fields).collect()};
        for field in fields{let ty=self.ty(module,context,field,&selected,active,b)?;let ok=if kind==SemanticBuiltinTrait::Transfer{self.transfer_value(module,context,&ty,caller,active,b)?}else{self.closed_value(module,context,&ty,caller,active,b)?};if !ok{return Ok(false);}}Ok(true)
    }
}

#[cfg(test)]
mod static_environment_tests{
    use super::*;
    #[test]
    fn closed_environment_static_view_is_exact_readonly_and_not_transfer(){
        let limits=Limits::default();
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();let mut active=ActiveQueries::new();let mut original=CheckedOriginal::check(&module,&context,&mut active,&mut b).unwrap();
        let owner=module.semantic.as_ref().unwrap().definitions.iter().find(|row|row.kind==SemanticDefinitionKind::Function&&row.parent.is_none()&&row.parameters.is_empty()).unwrap().id;
        original.current=Some(owner);let env=original.prototypes[&owner].environment.clone();
        for variant in 0..7{
            let element=if variant==5{Ty::Address{mutable:false,life:Life::Local(owner,NodeId(8123)),inner:Box::new(Ty::Atom(Type::I32))}}else{Ty::Atom(Type::U8)};
            let life=match variant{1=>Life::Local(owner,NodeId(8123)),2=>Life::Invocation(FrameToken(7),0),3=>Life::Formal(owner,0),_=>Life::Static};
            let inner=if variant==6{Ty::Atom(Type::I32)}else{Ty::Slice(Box::new(element))};
            let ty=Ty::Address{mutable:variant==4,life,inner:Box::new(inner)};
            assert_eq!(original.closed_value(&module,&context,&ty,&env,&mut active,&mut b).unwrap(),matches!(variant,0|6),"Static view variant {variant}");
            assert!(original.builtin_obligation(&module,&context,&ty,SemanticBuiltinTrait::Transfer,&env,&mut active,&mut b).is_err());
        }
        assert!(!original.closed_value(&module,&context,&Ty::Slice(Box::new(Ty::Atom(Type::U8))),&env,&mut active,&mut b).unwrap());
    }
}
