use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call(&mut self,module:&TypedModule,context:&CheckedContext,call:&SemanticOriginalExpr,callee:&SemanticOriginalExpr,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let (base,written_types,written_mixed)=match &callee.kind{
            CoreExprKind::TypeApply{callee,type_arguments}=>(callee.as_ref(),Some(type_arguments.as_slice()),None),
            CoreExprKind::GenericApply{callee,arguments}=>(callee.as_ref(),None,Some(arguments.as_slice())),
            _=>(callee,None,None),
        };
        let caller=context.definition(module,env.current()?,b)?;
        let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&caller.body else{return Err(bad("original call has no complete creator body"));};
        let candidates=site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(call.id)).filter_map(|row|sites.iter().find(|site|site.id==row.site)).filter(|site|matches!(site.kind,SemanticSiteRecipeKind::Call{..}|SemanticSiteRecipeKind::CallableCall{..}|SemanticSiteRecipeKind::OriginalCall)).count();
        b.spend(site_nodes.len().saturating_mul(sites.len().saturating_add(1)))?;
        if candidates!=1{return Err(bad("original Call has no unique exact operation site"));}
        if let Some(intrinsic)=self.intrinsic_name(module,context,base,env,bindings,b)? {
            let mut written=Vec::new();
            if let Some(types)=written_types{for ty in types{written.push(Arg::Type(self.ty(module,context,ty,env,active,b)?));}}
            if let Some(mixed)=written_mixed{for arg in mixed{match arg{CoreGenericArgument::Type(ty)=>written.push(Arg::Type(self.ty(module,context,ty,env,active,b)?)),_=>return Err(unsupported("registered mixed Const/Life arguments need their exact closed scheme",None))}}}
            let site=site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(call.id)).filter_map(|row|sites.iter().find(|site|site.id==row.site)).find(|site|matches!(site.kind,SemanticSiteRecipeKind::OriginalCall)).ok_or_else(||bad("registered intrinsic has another original call role"))?;
            let result=self.intrinsic_call(module,context,intrinsic,site.id,&written,expected,arguments,env,bindings,active,b)?;
            if matches!(intrinsic,intrinsics::Intrinsic::BytesNew|intrinsics::Intrinsic::StoreNew|intrinsics::Intrinsic::AsyncStart|intrinsics::Intrinsic::MutexCreate|intrinsics::Intrinsic::TaskCreate){b.spend(1)?;self.created.insert((env.current()?,call.id));}
            if let Some(expected)=expected{self.unify(&result,expected,b)?;}
            return Ok(result);
        }
        let static_type=self.static_call_type(module,context,base,env,bindings,b)?;
        if static_type.is_none(){if let Some(result)=self.trait_member_call(module,context,call,base,written_types,written_mixed,arguments,env,bindings,expected,active,b)?{return Ok(result);}}
        if static_type.is_none(){if let Some(result)=self.variant_constructor(module,context,call,base,written_types,written_mixed,arguments,env,bindings,expected,active,b)?{return Ok(result);}}
        self.callable_nodes.insert((env.current()?,call.id),base.id);
        let(callee_type,callee_place)=match static_type{Some(ty)=>(ty,None),None=>self.place(module,context,base,env,bindings,active,b)?};if let Some(place)=&callee_place{self.read_original(module,context,place,&callee_type,env,false,active,b)?;}
        match self.resolved(&callee_type,b)?{
            Ty::Function(target)=>{
                let definition=context.definition(module,target,b)?;
                let SemanticDefinitionBody::FunctionWithBody{signature,body,..}=&definition.body else{return Err(bad("original Call target omits its complete body"));};
                if matches!(body,SemanticOriginalFunctionBody::External)&&self.unsafe_depth.get(&env.current()?).copied().unwrap_or(0)==0{return Err(bad("original foreign execution is outside its actual unsafe lexical scope"));}
                let caller=context.definition(module,env.current()?,b)?;
                let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&caller.body else{return Err(bad("original Call creator has no complete body"));};
                let rows=site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(call.id)).filter_map(|row|sites.iter().find(|site|site.id==row.site)).filter(|site|matches!(site.kind,SemanticSiteRecipeKind::Call{..}|SemanticSiteRecipeKind::OriginalCall)).collect::<Vec<_>>();
                if rows.len()!=1{return Err(bad("original static Call lacks a unique exact call site"));}
                let site=rows[0];
                let proposed=match &site.kind{SemanticSiteRecipeKind::Call{target:claim,actuals}=>{if *claim!=target{return Err(bad("original Call recipe resolves another lexical declaration"));}Some(actuals.as_slice())},SemanticSiteRecipeKind::OriginalCall=>None,_=>unreachable!()};
                if proposed.is_some_and(|actuals|actuals.len()!=definition.parameters.len()){return Err(bad("original Call omits full-ordinal actual obligations"));}
                let written_len=written_types.map_or_else(||written_mixed.map_or(0,<[CoreGenericArgument<SemanticTypeRecipe>]>::len),<[SemanticTypeRecipe]>::len);
                if written_len>definition.parameters.len(){return Err(bad("original Call has excess written mixed arguments"));}
                let mut checked=Vec::new();
                for(ordinal,formal)in definition.parameters.iter().enumerate(){
                    let written=if let Some(types)=written_types{types.get(ordinal).map(|ty|self.ty(module,context,ty,env,active,b).map(Arg::Type)).transpose()?}else if let Some(mixed)=written_mixed{match mixed.get(ordinal){Some(CoreGenericArgument::Type(ty))=>Some(Arg::Type(self.ty(module,context,ty,env,active,b)?)),Some(CoreGenericArgument::Const(_))=>return Err(unsupported("original written const argument requires its solved full carrier",None)),Some(CoreGenericArgument::Lifetime(name))=>{
                        let row=self.named(module,context,env.current()?,name,SemanticOriginalNamespace::Lifetime,b)?.ok_or_else(||bad("written original life name has no lexical binding"))?;
                        let SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Life(_))=row else{return Err(bad("original life name selects another full-ordinal kind"));};Some(self.actual(module,context,recipe,env,active,b)?)
                    },None=>None}}else{None};
                    let value=match(written,formal){(Some(value),_)=>value,(None,SemanticFormal::Type{..})=>Arg::Type(self.fresh(b)?),(None,SemanticFormal::Lifetime)=>Arg::Life(self.fresh_life(b)?),(None,_)=>return Err(unsupported("original non-Type inference needs its complete scalar/lifetime constraints",None))};
                    if !matches!((formal,&value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Const{..},Arg::Const(_))|(SemanticFormal::Lifetime,Arg::Life(_))){return Err(bad("written original argument changes full-ordinal kind"));}
                    checked.push(value);
                }
                let mut selected=self.environment(module,context,target,active,b)?;
                selected.lexical[0].actuals=checked.clone();
                let prototype=self.call_signature(module,context,target,signature,&selected,active,b)?;
                self.execute_effect(env,prototype.call_kind,b)?;
                if prototype.parameters.len()!=arguments.len(){return Err(bad("original Call value arity differs from the declared signature"));}
                if let Some(expected)=expected{if !associated::has_projection(&prototype.result,b)?{self.unify(&prototype.result,expected,b)?;}}
                for((argument,parameter),mode)in arguments.iter().zip(&prototype.parameters).zip(&prototype.modes){let parameter=self.normalize_original_type(module,context,parameter,env,active,b)?;let actual=self.argument(module,context,argument,&parameter,*mode,env,bindings,active,b)?;self.unify(&actual,&parameter,b)?;}
                let result=self.normalize_original_type(module,context,&prototype.result,env,active,b)?;
                if let Some(expected)=expected{self.unify(&result,expected,b)?;}
                // A target's requirements are obligations here. They never enter
                // the caller's equality assumptions merely by selecting target.
                for requirement in &definition.requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}
                for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}}}
                if let Some(proposed)=proposed{for(recipe,value)in proposed.iter().zip(&checked){if !matches!(recipe,SemanticActualRecipe::Infer){let claim=self.actual(module,context,recipe,env,active,b)?;self.same_argument(&claim,value,b)?;}}}
                self.calls.insert((env.current()?,site.id),checked);
                self.call_targets.insert((env.current()?,site.id),target);
                if prototype.result_mode==PassingMode::Create{b.spend(1)?;self.created.insert((env.current()?,call.id));}
                Ok(result)
            },
            Ty::Closure(definition,actuals,sealed)=>{
                if written_types.is_some()||written_mixed.is_some(){return Err(bad("original closure invocation has unsupported generic application"));}
                let original=context.definition(module,definition,b)?;let SemanticDefinitionBody::FunctionWithBody{signature,..}=&original.body else{return Err(bad("original Closure has no complete signature/body"));};
                let mut selected=self.environment(module,context,definition,active,b)?;selected.lexical[0].actuals=actuals;selected.lexical[0].environment=sealed;
                let rigid=if let Some(prototype)=self.prototypes.get(&definition){prototype.clone()}else{let rigid=self.environment(module,context,definition,active,b)?;self.signature(module,context,signature,&rigid,active,b)?};let prototype=self.instantiate_prototype(&rigid,Some(definition),&selected,b)?;
                if prototype.callable_kind==Some(CallableKind::FnOnce){if let Some(place)=&callee_place{self.read_original(module,context,place,&callee_type,env,true,active,b)?;}}
                if prototype.callable_kind==Some(CallableKind::FnMut){if let Some(place)=&callee_place{self.written.entry(env.current()?).or_default().insert(place.binder.clone());}}
                self.invoke(module,context,&prototype,arguments,env,bindings,active,b)
            },
            Ty::Code{parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes,..}=>self.invoke(module,context,&Prototype{parameters,result:*result,environment:env.clone(),modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes,callable_kind:None},arguments,env,bindings,active,b),
            variable=>{
                let prototype=self.callable_premise(module,context,&variable,env,active,b)?.ok_or_else(||bad("original indirect Call has no complete Callable premise"))?;
                if prototype.callable_kind==Some(CallableKind::FnOnce){if let Some(place)=&callee_place{self.read_original(module,context,place,&variable,env,true,active,b)?;}}
                if prototype.callable_kind==Some(CallableKind::FnMut){if let Some(place)=&callee_place{self.written.entry(env.current()?).or_default().insert(place.binder.clone());}}
                self.invoke(module,context,&prototype,arguments,env,bindings,active,b)
            },
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn invoke(&mut self,module:&TypedModule,context:&CheckedContext,prototype:&Prototype,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{self.execute_effect(env,prototype.call_kind,b)?;if prototype.parameters.len()!=arguments.len(){return Err(bad("original Callable arity differs"));}for((argument,expected),mode)in arguments.iter().zip(&prototype.parameters).zip(&prototype.modes){let actual=self.argument(module,context,argument,expected,*mode,env,bindings,active,b)?;self.unify(&actual,expected,b)?;}Ok(prototype.result.clone())}
    pub(super) fn same_argument(&mut self,left:&Arg,right:&Arg,b:&mut Budget)->Check<()>{match(left,right){(Arg::Type(left),Arg::Type(right))=>self.unify(left,right,b),(Arg::Life(left),Arg::Life(right))=>self.unify_life(left,right,b),_ if left==right=>Ok(()),_=>Err(bad("original Call argument differs from its solved symbolic identity/carrier"))}}
    pub(super) fn callable_premise(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Prototype>>{
        if let Ty::Opaque(owner,arguments)=self.resolved(ty,b)?{return self.opaque_callable(module,context,owner,&arguments,active,b);}
        let mut result=None;
        for(index,frame)in env.lexical.iter().enumerate(){let definition=context.definition(module,frame.definition,b)?;let mut lexical=env.clone();lexical.lexical=lexical.lexical[index..].to_vec();let mut requirements=definition.requirements.iter().collect::<Vec<_>>();for formal in &definition.parameters{if let SemanticFormal::Type{requirements:rows}=formal{requirements.extend(rows);}}
            for requirement in requirements{if let SemanticRequirementRecipe::Callable{ty:receiver,signature,kind}=requirement{let receiver=self.ty(module,context,receiver,&lexical,active,b)?;if self.same(&receiver,ty,b)?{let mut prototype=self.signature(module,context,signature,&lexical,active,b)?;prototype.callable_kind=Some(*kind);if let Some(previous)=&result{let previous:&Prototype=previous;if previous.parameters.len()!=prototype.parameters.len()||previous.modes!=prototype.modes||previous.result_mode!=prototype.result_mode||previous.call_kind!=prototype.call_kind||previous.bound_lifetimes!=prototype.bound_lifetimes||previous.invocation_lifetimes!=prototype.invocation_lifetimes{return Err(unsupported("original Callable overloads need exact role selection",None));}for(a,c)in previous.parameters.iter().zip(&prototype.parameters){if !self.same(a,c,b)?{return Err(unsupported("original Callable overloads need exact role selection",None));}}if !self.same(&previous.result,&prototype.result,b)?{return Err(unsupported("original Callable overloads need exact role selection",None));}}result=Some(prototype);}}}
        }Ok(result)
    }
    pub(super) fn obligation(&mut self,module:&TypedModule,context:&CheckedContext,requirement:&SemanticRequirementRecipe,target:&Env,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        match requirement {
            SemanticRequirementRecipe::TypeEqual{left,right}=>{let left=self.ty(module,context,left,target,active,b)?;let right=self.ty(module,context,right,target,active,b)?;if !self.same(&left,&right,b)?{return Err(bad("original caller does not prove target TypeEqual requirement"));}Ok(())},
            SemanticRequirementRecipe::LifeEqual{left,right}=>{let left=self.life(module,context,left,target,active,b)?;let right=self.life(module,context,right,target,active,b)?;if self.same_life(&left,&right,b)?{Ok(())}else{Err(bad("original caller does not prove the target lifetime equality"))}},
            SemanticRequirementRecipe::Callable{ty,signature,kind}=>{
                let receiver=self.ty(module,context,ty,target,active,b)?;let receiver=self.resolved(&receiver,b)?;let wanted=self.signature(module,context,signature,target,active,b)?;
                let actual=match &receiver{Ty::Closure(id,..)=>self.prototypes.get(id).cloned(),Ty::Code{parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes,..}=>Some(Prototype{parameters:parameters.clone(),result:*result.clone(),environment:caller.clone(),modes:modes.clone(),result_mode:*result_mode,call_kind:*call_kind,bound_lifetimes:*bound_lifetimes,invocation_lifetimes:*invocation_lifetimes,callable_kind:None}),_=>self.callable_premise(module,context,&receiver,caller,active,b)?}.ok_or_else(||bad("original caller does not prove the target Callable requirement"))?;
                if !callable_accepts(actual.callable_kind,*kind){return Err(unsupported("original Callable source capability is not independently established",None));}
                if actual.parameters.len()!=wanted.parameters.len()||actual.modes!=wanted.modes||actual.result_mode!=wanted.result_mode||actual.call_kind!=wanted.call_kind||actual.bound_lifetimes!=wanted.bound_lifetimes||actual.invocation_lifetimes!=wanted.invocation_lifetimes{return Err(bad("original Callable requirement has different complete modes/kind/binder domains"));}for(actual,wanted)in actual.parameters.iter().zip(&wanted.parameters){if !self.same(actual,wanted,b)?{return Err(bad("original Callable requirement parameters lack their original relation"));}}if !self.same(&actual.result,&wanted.result,b)?{return Err(bad("original Callable requirement result lacks its original relation"));}Ok(())
            },
            SemanticRequirementRecipe::Trait{ty,application}=>{let receiver=self.ty(module,context,ty,target,active,b)?;let mut arguments=Vec::new();for argument in &application.arguments{arguments.push(self.actual(module,context,argument,target,active,b)?);}self.original_trait_obligation(module,context,&receiver,application.declaration,&arguments,caller,active,b)},
            SemanticRequirementRecipe::Structural{ty,bound}=>{let ty=self.ty(module,context,ty,target,active,b)?;self.structural_obligation(module,context,&ty,*bound,caller,active,b)},
            SemanticRequirementRecipe::Builtin{ty,trait_kind}=>{let ty=self.ty(module,context,ty,target,active,b)?;let bound=match trait_kind{SemanticBuiltinTrait::Copy=>DynStructuralBound::Copy,SemanticBuiltinTrait::Value=>DynStructuralBound::Value,SemanticBuiltinTrait::SyncDrop=>DynStructuralBound::SyncDrop,SemanticBuiltinTrait::StoreElement=>DynStructuralBound::StoreElement,_=>return self.builtin_obligation(module,context,&ty,*trait_kind,caller,active,b)};self.structural_obligation(module,context,&ty,bound,caller,active,b)},
            _=>Err(unsupported("original requirement has no complete symbolic caller proof",None)),
        }
    }
}

fn callable_accepts(actual:Option<CallableKind>,wanted:CallableKind)->bool{match actual{Some(CallableKind::FunctionPointer)=>true,Some(CallableKind::Fn)=>wanted!=CallableKind::FunctionPointer,Some(CallableKind::FnMut)=>matches!(wanted,CallableKind::FnMut|CallableKind::FnOnce),Some(CallableKind::FnOnce)=>wanted==CallableKind::FnOnce,None=>false}}

impl CheckedOriginal {
    pub(super) fn structural_obligation(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,bound:DynStructuralBound,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if self.structural_premise(module,context,ty,bound,caller,active,b)?{return Ok(());}
        let proved=match bound{
            DynStructuralBound::Copy=>self.is_copy(module,context,ty,caller,active,b)?,
            DynStructuralBound::SyncDrop=>self.sync_drop(module,context,ty,caller,active,b)?,
            DynStructuralBound::Value=>self.value_type(module,context,ty,caller,active,b)?,
            DynStructuralBound::StoreElement=>self.store_element(module,context,ty,caller,active,b)?,
        };
        if proved{Ok(())}else{Err(unsupported("original caller lacks a complete structural/cleanup obligation proof",None))}
    }
    pub(super) fn value_type(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            if self.structural_premise(module,context,ty,DynStructuralBound::Value,env,active,b)?||self.is_copy(module,context,ty,env,active,b)?{return Ok(true);}
            Ok(match self.resolved(ty,b)?{
                Ty::Address{..}|Ty::Desc{..}|Ty::Managed(_)=>true,
                Ty::Closure(definition,..)=>self.checked.contains(&definition)&&self.creator_moves.contains_key(&definition),
                Ty::Tuple(fields)=>{for field in fields{if !self.value_type(module,context,&field,env,active,b)?{return Ok(false);}}true},
                Ty::Array(element,_)=>self.value_type(module,context,&element,env,active,b)?,
                Ty::ErrorUnion(error,result)=>self.value_type(module,context,&error,env,active,b)?&&self.value_type(module,context,&result,env,active,b)?,
                Ty::Nominal(id,arguments)=>{let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{kind,body,..}=&definition.body else{return Err(bad("original Value has no exact nominal body"));};if !matches!(kind,LifeNominalKind::Record|LifeNominalKind::Package|LifeNominalKind::Enum){return Ok(false);}let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=arguments;
                    match body{SemanticNominalBody::Product{fields}=>for field in fields{let ty=self.ty(module,context,&field.ty,&selected,active,b)?;if !self.value_type(module,context,&ty,env,active,b)?{return Ok(false);}},SemanticNominalBody::Enum{variants}=>for variant in variants{for field in &variant.fields{let ty=self.ty(module,context,field,&selected,active,b)?;if !self.value_type(module,context,&ty,env,active,b)?{return Ok(false);}}}}true
                },
                _=>false,
            })
        })
    }
}
