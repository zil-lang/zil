//! Recipe interpretation in the original, unspecialized declaration domain.
use super::*;
impl CheckedOriginal {
    pub(super) fn ty(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticTypeRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let token=active.fresh(b)?;
        active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|self.ty_inner(module,context,recipe,env,active,b))
    }
    fn ty_inner(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticTypeRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        b.spend(1)?;
        Ok(match recipe {
            SemanticTypeRecipe::Atom(ty) if SealedError::from_type(ty).is_some()=>Ty::SealedError(SealedError::from_type(ty).unwrap()),
            SemanticTypeRecipe::Exact(exact) if exact.life==empty_life()&&exact.classes.is_empty()&&SealedError::from_type(&exact.ty).is_some()=>Ty::SealedError(SealedError::from_type(&exact.ty).unwrap()),
            SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar()||matches!(ty,Type::ForeignHandle|Type::CWideString|Type::ByteBuffer|Type::SyncCell(_))=>Ty::Atom(ty.clone()),
            SemanticTypeRecipe::Formal(reference)=>match env.frame(reference.scope)?.actuals.get(reference.ordinal as usize){Some(Arg::Type(ty))=>ty.clone(),_=>return Err(bad("original Type recipe selects a non-Type formal"))},
            SemanticTypeRecipe::Environment{scope,ordinal}=>match env.frame(*scope)?.environment.get(*ordinal as usize){Some(Arg::Type(ty))=>ty.clone(),_=>return Err(bad("original Type recipe selects a non-Type environment"))},
            SemanticTypeRecipe::SelfType{scope}=>{
                let definition=context.definition(module,env.frame(*scope)?.definition,b)?;
                match &definition.body {
                    SemanticDefinitionBody::Scope(SemanticScopeRecipe::InherentImpl{self_type,..}|SemanticScopeRecipe::TraitImpl{self_type,..}|SemanticScopeRecipe::BuiltinImpl{self_type,..})=>{
                        let depth=env.lexical.iter().position(|row|row.definition==definition.id).ok_or_else(||bad("original Self scope is not lexical"))?;
                        let mut selected=env.clone();selected.lexical=selected.lexical[depth..].to_vec();
                        self.ty(module,context,self_type,&selected,active,b)?
                    },
                    SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{..})=>Ty::SelfType(definition.id),
                    _=>return Err(bad("original SelfType selects no receiver scope")),
                }
            },
            SemanticTypeRecipe::Nominal{definition,arguments}=>{
                let nominal=context.definition(module,*definition,b)?;
                if !matches!(nominal.body,SemanticDefinitionBody::Nominal{..})||arguments.len()!=nominal.parameters.len(){return Err(bad("original nominal identity/arity is incomplete"));}
                let mut checked=Vec::new();for(formal,actual)in nominal.parameters.iter().zip(arguments){let value=self.actual(module,context,actual,env,active,b)?;if !matches!((formal,&value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Const{..},Arg::Const(_))|(SemanticFormal::Lifetime,Arg::Life(_))){return Err(bad("original nominal argument changes full-ordinal kind"));}checked.push(value);}
                Ty::Nominal(*definition,checked)
            },
            SemanticTypeRecipe::Tuple(fields)=>{b.spend(fields.len().saturating_add(1))?;let mut checked=Vec::new();for field in fields{checked.push(self.ty(module,context,field,env,active,b)?);}if checked.is_empty(){Ty::Atom(Type::Unit)}else{Ty::Tuple(checked)}},
            SemanticTypeRecipe::Opaque{definition,arguments,hidden}=>self.opaque_type(module,context,*definition,arguments,hidden,env,active,b)?,
            SemanticTypeRecipe::AssociatedType{receiver,application,member}=>{let receiver=self.ty(module,context,receiver,env,active,b)?;let mut arguments=Vec::new();for argument in &application.arguments{arguments.push(self.actual(module,context,argument,env,active,b)?);}self.associated_type(module,context,&receiver,application.declaration,&arguments,*member,env,active,b)?},
            SemanticTypeRecipe::Address{mutable,lifetime,inner}=>Ty::Address{mutable:*mutable,life:self.outer(module,context,lifetime,env,active,b)?,inner:Box::new(self.ty(module,context,inner,env,active,b)?)},
            SemanticTypeRecipe::Desc{lifetime,inner}=>Ty::Desc{life:self.outer(module,context,lifetime,env,active,b)?,inner:Box::new(self.ty(module,context,inner,env,active,b)?)},
            SemanticTypeRecipe::Slice(inner)=>Ty::Slice(Box::new(self.ty(module,context,inner,env,active,b)?)),
            SemanticTypeRecipe::Managed(inner)=>{
                let child=self.ty(module,context,inner,env,active,b)?;
                let valid=match self.resolved(&child,b)?{Ty::Atom(Type::ByteBuffer)|Ty::Store(_)|Ty::Carrier{kind:CarrierKind::Mutex|CarrierKind::Task|CarrierKind::Registration|CarrierKind::AsyncRegistration,..}=>true,Ty::Nominal(id,_)=>matches!(context.definition(module,id,b)?.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Resource,..}),_=>false};
                if !valid{return Err(unsupported("original Managed payload is not an independently identified canonical resource",None));}Ty::Managed(Box::new(child))
            },
            SemanticTypeRecipe::Mutex(inner)=>Ty::Carrier{kind:CarrierKind::Mutex,child:Box::new(self.ty(module,context,inner,env,active,b)?),retained:Box::default()},
            SemanticTypeRecipe::MutexGuard{payload,retained}=>Ty::Carrier{kind:CarrierKind::MutexGuard,child:Box::new(self.ty(module,context,payload,env,active,b)?),retained:Box::new(self.retained(module,context,retained,env,active,b)?)},
            SemanticTypeRecipe::Task{result,retained}=>Ty::Carrier{kind:CarrierKind::Task,child:Box::new(self.ty(module,context,result,env,active,b)?),retained:Box::new(self.retained(module,context,retained,env,active,b)?)},
            SemanticTypeRecipe::Registration(inner)|SemanticTypeRecipe::AsyncRegistration(inner)=>Ty::Carrier{kind:if matches!(recipe,SemanticTypeRecipe::Registration(_)){CarrierKind::Registration}else{CarrierKind::AsyncRegistration},child:Box::new(self.ty(module,context,inner,env,active,b)?),retained:Box::default()},
            SemanticTypeRecipe::OwnedStore(inner)=>Ty::Store(Box::new(self.ty(module,context,inner,env,active,b)?)),
            SemanticTypeRecipe::Array{element,length}=>Ty::Array(Box::new(self.ty(module,context,element,env,active,b)?),Box::new(self.constant(module,context,length,&Ty::Atom(Type::Usize),env,active,b)?)),
            SemanticTypeRecipe::ErrorUnion{error,result}=>Ty::ErrorUnion(Box::new(self.ty(module,context,error,env,active,b)?),Box::new(self.ty(module,context,result,env,active,b)?)),
            SemanticTypeRecipe::Code{kind,signature}=>{let signature=self.signature(module,context,signature,env,active,b)?;Ty::Code{kind:*kind,parameters:signature.parameters,result:Box::new(signature.result),modes:signature.modes,result_mode:signature.result_mode,call_kind:signature.call_kind,bound_lifetimes:signature.bound_lifetimes,invocation_lifetimes:signature.invocation_lifetimes}},
            SemanticTypeRecipe::Closure{definition,arguments,environment}=>{let mut args=Vec::new();let mut sealed=Vec::new();for actual in arguments{args.push(self.actual(module,context,actual,env,active,b)?);}for actual in environment{sealed.push(self.actual(module,context,actual,env,active,b)?);}Ty::Closure(*definition,args,sealed)},
            SemanticTypeRecipe::OriginalType{scope,source}=>{
                let definition=env.frame(*scope)?.definition;
                let value=self.original_type(definition,source,b)?;
                self.apply_type(&value,env,b)?
            },
            SemanticTypeRecipe::Exact(exact) if exact.life.binder_count==0&&exact.life.slots.is_empty()&&exact.life.retained.is_empty()&&exact.life.nominal_arguments.is_empty()&&exact.classes.is_empty()&&exact.ty.is_b0_scalar()=>Ty::Atom(exact.ty.clone()),
            _=>return Err(unsupported("original symbolic type constructor is not independently interpreted",None)),
        })
    }
    fn outer(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticOuterLifeRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Life>{match recipe{SemanticOuterLifeRecipe::Static=>Ok(Life::Static),SemanticOuterLifeRecipe::Class(life)=>self.life(module,context,life,env,active,b)}}
    pub(super) fn life(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticLifeRecipe,env:&Env,_active:&mut ActiveQueries,b:&mut Budget)->Check<Life>{b.spend(1)?;match recipe{
        SemanticLifeRecipe::Formal(reference)=>match env.frame(reference.scope)?.actuals.get(reference.ordinal as usize){Some(Arg::Life(life))=>Ok(life.clone()),_=>Err(bad("original lifetime selects another formal kind"))},
        SemanticLifeRecipe::Implicit{scope,ordinal}=>{let owner=env.frame(*scope)?.definition;let definition=context.definition(module,owner,b)?;b.spend(definition.implicit_lifetimes.len())?;if !definition.implicit_lifetimes.iter().any(|row|row.ordinal==*ordinal){return Err(bad("original implicit lifetime has no occurrence in its actual declaration"));}Ok(Life::Implicit(owner,*ordinal))},
        SemanticLifeRecipe::Environment{scope,ordinal}=>match env.frame(*scope)?.environment.get(*ordinal as usize){Some(Arg::Life(life))=>Ok(life.clone()),_=>Err(bad("original lifetime selects another sealed kind"))},
        SemanticLifeRecipe::Bound{depth,index}=>env.bounds.get(*depth as usize).and_then(|row|row.get(*index as usize)).cloned().ok_or_else(||bad("original Bound escapes its nonzero frame")),
        SemanticLifeRecipe::Invocation(index)=>env.invocation.get(*index as usize).cloned().ok_or_else(||bad("original Invocation escapes its signature")),
        _=>Err(unsupported("original lifetime occurrence needs the complete symbolic operand directory",None)),
    }}
    pub(super) fn actual(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticActualRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Arg>{match recipe{
        SemanticActualRecipe::Type(ty)=>Ok(Arg::Type(self.ty(module,context,ty,env,active,b)?)),
        SemanticActualRecipe::Life(life)=>Ok(Arg::Life(self.life(module,context,life,env,active,b)?)),
        SemanticActualRecipe::Const{carrier,value}=>{let carrier=self.ty(module,context,carrier,env,active,b)?;Ok(Arg::Const(self.constant(module,context,value,&carrier,env,active,b)?))},
        SemanticActualRecipe::Infer=>Err(bad("original Infer requires its real Call/formal constraint context")),
    }}
    pub(super) fn constant(&mut self,module:&TypedModule,context:&CheckedContext,recipe:&SemanticConstRecipe,carrier:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Const>{
        b.spend(1)?;
        let value=match recipe {
            SemanticConstRecipe::Formal(reference)=>{let Some(Arg::Const(value))=env.frame(reference.scope)?.actuals.get(reference.ordinal as usize)else{return Err(bad("original constant selects another formal kind"));};self.unify(&value.carrier,carrier,b)?;return Ok(value.clone());},
            SemanticConstRecipe::Environment{scope,ordinal}=>{let Some(Arg::Const(value))=env.frame(*scope)?.environment.get(*ordinal as usize)else{return Err(bad("original constant selects another sealed kind"));};self.unify(&value.carrier,carrier,b)?;return Ok(value.clone());},
            SemanticConstRecipe::Value(value)=>{self.unify(&Ty::Atom(value.ty()),carrier,b)?;ConstValue::Value(value.clone())},
            SemanticConstRecipe::IntegerLiteral(text)|SemanticConstRecipe::FloatLiteral(text)=>{
                let Ty::Atom(ty)=self.resolved(carrier,b)? else{return Err(unsupported("original numeric literal has a nonconcrete carrier",None));};
                let pattern=if matches!(recipe,SemanticConstRecipe::IntegerLiteral(_)){DynConstPattern::IntegerLiteral(text.clone())}else{DynConstPattern::FloatLiteral(text.clone())};
                ConstValue::Value(scalar::closed(&pattern,&ty,module.target.pointer_width,b)?.ok_or_else(||bad("original scalar literal remains unknown"))?)
            },
            SemanticConstRecipe::Unary{op,operand_type,value}=>{let operand=self.ty(module,context,operand_type,env,active,b)?;self.unify(&operand,carrier,b)?;ConstValue::Unary(*op,Box::new(self.constant(module,context,value,&operand,env,active,b)?))},
            SemanticConstRecipe::Binary{op,operand_type,left,right}=>{let operand=self.ty(module,context,operand_type,env,active,b)?;self.unify(&operand,carrier,b)?;ConstValue::Binary(*op,Box::new(self.constant(module,context,left,&operand,env,active,b)?),Box::new(self.constant(module,context,right,&operand,env,active,b)?))},
            SemanticConstRecipe::Convert{source_type,target_type,value}=>{let source=self.ty(module,context,source_type,env,active,b)?;let target=self.ty(module,context,target_type,env,active,b)?;self.unify(&target,carrier,b)?;ConstValue::Convert(Box::new(self.constant(module,context,value,&source,env,active,b)?))},
            _=>return Err(unsupported("original associated constant requires a complete original witness",None)),
        };Ok(Const{carrier:Box::new(carrier.clone()),value})
    }
    pub(super) fn signature(&mut self,module:&TypedModule,context:&CheckedContext,signature:&SemanticSignatureRecipe,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Prototype>{
        let mut env=env.clone();let token=active.fresh(b)?;
        b.spend(signature.bound_lifetimes as usize+signature.invocation_lifetimes as usize)?;
        if signature.bound_lifetimes!=0{env.bounds.insert(0,(0..signature.bound_lifetimes).map(|index|Life::Bound(token,index)).collect());}
        env.invocation=(0..signature.invocation_lifetimes).map(|index|Life::Invocation(token,index)).collect();
        let mut parameters=Vec::new();for(ordinal,parameter)in signature.parameters.iter().enumerate(){let ty=self.ty(module,context,&parameter.ty,&env,active,b)?;parameters.push(self.mode_type(module,context,ty,parameter.mode,Some(ordinal as u32),&env,b)?);}
        let result=self.ty(module,context,&signature.result,&env,active,b)?;let result=self.mode_type(module,context,result,signature.result_mode,None,&env,b)?;
        Ok(Prototype{parameters,result,environment:env,modes:signature.parameters.iter().map(|parameter|parameter.mode).collect(),result_mode:signature.result_mode,call_kind:signature.call_kind,bound_lifetimes:signature.bound_lifetimes,invocation_lifetimes:signature.invocation_lifetimes,callable_kind:None})
    }
}
