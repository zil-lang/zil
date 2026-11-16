use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn call(&mut self,module:&TypedModule,context:&CheckedContext,call:&SemanticOriginalExpr,callee:&SemanticOriginalExpr,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        // Capture the actual original call-site Γ before target lookup,
        // signature elaboration or associated-result normalization can enter
        // a substituted declaration scope. It supplies premises, never a
        // conclusion from this pending body or a target's pending where list.
        if self.current!=Some(env.current()?){return Err(bad("original call does not enter its exact active declaration environment"));}
        let evidence=self.copy_environment(env,b)?;
        let previous=self.premise_environment.replace(evidence);
        let result=self.call_inner(module,context,call,callee,arguments,env,bindings,expected,active,b);
        self.premise_environment=previous;
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn call_inner(&mut self,module:&TypedModule,context:&CheckedContext,call:&SemanticOriginalExpr,callee:&SemanticOriginalExpr,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let (base,written_types,written_mixed)=match &callee.kind{
            CoreExprKind::TypeApply{callee,type_arguments}=>(callee.as_ref(),Some(type_arguments.as_slice()),None),
            CoreExprKind::GenericApply{callee,arguments}=>(callee.as_ref(),None,Some(arguments.as_slice())),
            _=>(callee,None,None),
        };
        let family=self.binding_families.get(&(env.current()?,call.id)).map(|ty|self.resolved(ty,b)).transpose()?;
        let caller=context.definition(module,env.current()?,b)?;
        let SemanticDefinitionBody::FunctionWithBody{sites,..}=&caller.body else{return Err(bad("original call has no complete creator body"));};
        let(position,identity)=self.original_call_site(env.current()?,call.id,b)?;
        let site=sites.get(position).filter(|site|site.id==identity).ok_or_else(||bad("original call reverse directory selects another immutable site identity"))?;
        if let Some(intrinsic)=self.intrinsic_name(module,context,base,env,bindings,b)? {
            let mut written=Vec::new();
            if let Some(types)=written_types{for ty in types{written.push(Arg::Type(self.ty(module,context,ty,env,active,b)?));}}
            if let Some(mixed)=written_mixed{for arg in mixed{match arg{CoreGenericArgument::Type(ty)=>written.push(Arg::Type(self.ty(module,context,ty,env,active,b)?)),_=>return Err(unsupported("registered mixed Const/Life arguments need their exact closed scheme",None))}}}
            if !matches!(site.kind,SemanticSiteRecipeKind::OriginalCall){return Err(bad("registered intrinsic has another original call role"));}
            let result=self.intrinsic_call(module,context,intrinsic,site.id,&written,expected.or(family.as_ref()),arguments,env,bindings,active,b)?;
            if matches!(intrinsic,intrinsics::Intrinsic::BytesNew|intrinsics::Intrinsic::StoreNew|intrinsics::Intrinsic::AsyncStart|intrinsics::Intrinsic::MutexCreate|intrinsics::Intrinsic::TaskCreate|intrinsics::Intrinsic::TaskCreateOwned){b.spend(1)?;self.created.insert((env.current()?,call.id));}
            if let Some(expected)=expected{self.relate_value(&result,expected,b)?;}
            return Ok(result);
        }
        let static_type=self.static_call_type(module,context,base,env,bindings,b)?;
        if let Some(Ty::Function(target))=&static_type{if let Some(result)=self.static_trait_call(module,context,*target,call,base,written_types,written_mixed,arguments,env,bindings,expected,active,b)?{return Ok(result);}}
        if static_type.is_none(){if let Some(result)=self.slice_len_call(module,context,call,base,written_types.is_some()||written_mixed.is_some(),arguments,env,bindings,active,b)?{if let Some(expected)=expected{self.relate_value(&result,expected,b)?;}return Ok(result);}}
        if static_type.is_none(){if let Some(result)=self.trait_member_call(module,context,call,base,written_types,written_mixed,arguments,env,bindings,expected,active,b)?{return Ok(result);}}
        if static_type.is_none(){if let Some(result)=self.variant_constructor(module,context,call,base,written_types,written_mixed,arguments,env,bindings,expected,active,b)?{return Ok(result);}}
        self.callable_nodes.insert((env.current()?,call.id),base.id);
        let(callee_type,callee_place)=match static_type{Some(ty)=>(ty,None),None=>self.place(module,context,base,env,bindings,active,b)?};if let Some(place)=&callee_place{self.read_original(module,context,place,&callee_type,env,false,active,b)?;}
        match self.resolved(&callee_type,b)?{
            Ty::Function(target)=>{
                let definition=context.definition(module,target,b)?;
                let SemanticDefinitionBody::FunctionWithBody{signature,body,..}=&definition.body else{return Err(bad("original Call target omits its complete body"));};
                if matches!(body,SemanticOriginalFunctionBody::External)&&self.unsafe_depth.get(&env.current()?).copied().unwrap_or(0)==0{return Err(bad("original foreign execution is outside its actual unsafe lexical scope"));}
                if !matches!(site.kind,SemanticSiteRecipeKind::Call{..}|SemanticSiteRecipeKind::OriginalCall){return Err(bad("original static Call lacks a unique exact call site"));}
                let proposed=match &site.kind{SemanticSiteRecipeKind::Call{target:claim,actuals}=>{if *claim!=target{return Err(bad("original Call recipe resolves another lexical declaration"));}Some(actuals.as_slice())},SemanticSiteRecipeKind::OriginalCall=>None,_=>unreachable!()};
                if proposed.is_some_and(|actuals|actuals.len()!=definition.parameters.len()){return Err(bad("original Call omits full-ordinal actual obligations"));}
                let written_slots=Self::written_original_slots(&definition.parameters,written_types,written_mixed,b)?;
                let mut checked=Vec::new();
                for(ordinal,formal)in definition.parameters.iter().enumerate(){
                    let written_slot=written_slots[ordinal];
                    let written=if let Some(types)=written_types{written_slot.and_then(|index|types.get(index)).map(|ty|self.ty(module,context,ty,env,active,b).map(Arg::Type)).transpose()?}else if let Some(mixed)=written_mixed{match written_slot.and_then(|index|mixed.get(index)){Some(CoreGenericArgument::Type(ty))=>Some(Arg::Type(self.ty(module,context,ty,env,active,b)?)),Some(CoreGenericArgument::Const(_))=>return Err(unsupported("original written const argument requires its solved full carrier",None)),Some(CoreGenericArgument::Lifetime(name))=>{
                        let row=self.named(module,context,env.current()?,name,SemanticOriginalNamespace::Lifetime,b)?.ok_or_else(||bad("written original life name has no lexical binding"))?;
                        let SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Life(_))=row else{return Err(bad("original life name selects another full-ordinal kind"));};Some(self.actual(module,context,recipe,env,active,b)?)
                    },None=>None}}else{None};
                    let value=match(written,formal){(Some(value),_)=>value,(None,SemanticFormal::Type{..})=>Arg::Type(self.fresh(b)?),(None,SemanticFormal::Lifetime)=>Arg::Life(self.fresh_life(b)?),(None,_)=>return Err(unsupported("original non-Type inference needs its complete scalar/lifetime constraints",None))};
                    if !matches!((formal,&value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Const{..},Arg::Const(_))|(SemanticFormal::Lifetime,Arg::Life(_))){return Err(bad("written original argument changes full-ordinal kind"));}
                    checked.push(value);
                }
                let mut selected=self.environment(module,context,target,active,b)?;
                selected.lexical[0].actuals=checked.clone();
                let outer_floor=self.next_inference;
                let prototype=self.call_signature(module,context,target,signature,&selected,active,b)?;
                if let Some(family)=&family {self.bind_family_result(&prototype.result,family,b)?;}
                let outer_heads=self.defer_call_heads(&prototype,&checked,outer_floor,b)?;
                self.execute_effect(env,prototype.call_kind,b)?;
                if prototype.parameters.len()!=arguments.len(){return Err(bad("original Call value arity differs from the declared signature"));}
                if let Some(expected)=expected{if !associated::has_projection(&prototype.result,b)?{self.unify(&prototype.result,expected,b)?;}}
                b.spend(prototype.parameters.len())?;let mut region_parameters=Vec::with_capacity(prototype.parameters.len());
                for((argument,parameter),mode)in arguments.iter().zip(&prototype.parameters).zip(&prototype.modes){let parameter=self.normalize_original_type(module,context,parameter,env,active,b)?;let actual=self.argument(module,context,argument,&parameter,*mode,env,bindings,active,b)?;self.relate_value(&actual,&parameter,b)?;region_parameters.push(parameter);}
                self.finish_call_heads(&outer_heads,call.id,b)?;
                self.remember_call_region_parameters(&region_parameters,call.id,env,b)?;
                let result=self.normalize_original_type(module,context,&prototype.result,env,active,b)?;
                if let Some(expected)=expected{self.relate_value(&result,expected,b)?;}
                // Opaque result capabilities refer to the defining body's
                // actual Result role. Complete every original return before
                // interpreting those obligations in this call's full actuals.
                if let Ty::Opaque(owner,actuals)=self.resolved(&result,b)?{self.opaque_hidden(module,context,owner,&actuals,active,b)?;}
                // A target's requirements are obligations here. They never enter
                // the caller's equality assumptions merely by selecting target.
                for requirement in &definition.requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}
                for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}}}
                if let Some(proposed)=proposed{for(recipe,value)in proposed.iter().zip(&checked){if !matches!(recipe,SemanticActualRecipe::Infer){let claim=self.actual(module,context,recipe,env,active,b)?;self.same_argument(&claim,value,b)?;}}}
                // An inferred whole Type is the existing argument domain, not
                // a fresh nominal reconstructed from its outside Life images.
                // Preserve every inducing occurrence; the actual bridge must
                // compare all of them rather than choose the first match.
                self.remember_inferred_type_sources(module,env.current()?,site.id,definition,signature,arguments,0,&written_slots,&checked,b)?;
                self.calls.insert((env.current()?,site.id),checked);
                self.call_targets.insert((env.current()?,site.id),target);
                if prototype.result_mode==PassingMode::Create{b.spend(1)?;self.created.insert((env.current()?,call.id));}
                Ok(result)
            },
            Ty::Closure(definition,actuals,sealed)=>{
                if written_types.is_some()||written_mixed.is_some(){return Err(bad("original closure invocation has unsupported generic application"));}
                let original=context.definition(module,definition,b)?;let SemanticDefinitionBody::FunctionWithBody{signature,..}=&original.body else{return Err(bad("original Closure has no complete signature/body"));};
                let mut selected=self.environment(module,context,definition,active,b)?;selected.lexical[0].actuals=actuals;selected.lexical[0].environment=sealed;
                let rigid=if let Some(prototype)=self.prototypes.get(&definition){prototype.clone()}else{let rigid=self.environment(module,context,definition,active,b)?;self.signature(module,context,signature,&rigid,active,b)?};let outer_floor=self.next_inference;let prototype=self.instantiate_prototype(&rigid,Some(definition),&selected,b)?;
                if prototype.callable_kind==Some(CallableKind::FnOnce){if let Some(place)=&callee_place{self.read_original(module,context,place,&callee_type,env,true,active,b)?;}}
                if prototype.callable_kind==Some(CallableKind::FnMut){if let Some(place)=&callee_place{self.written.entry(env.current()?).or_default().insert(place.binder.clone());}}
                self.invoke(module,context,call.id,outer_floor,&prototype,arguments,env,bindings,active,b)
            },
            Ty::Code{parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes,..}=>{let prototype=Prototype{parameters,result:*result,environment:env.clone(),modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes,premise_bounds:vec![],callable_kind:None};let outer_floor=self.next_inference;self.invoke(module,context,call.id,outer_floor,&prototype,arguments,env,bindings,active,b)},
            Ty::Address{mutable,inner,..}=>{
                if written_types.is_some()||written_mixed.is_some(){return Err(bad("original borrowed callable has a written generic application"));}
                let target=self.resolved(&inner,b)?;
                let outer_floor=self.next_inference;
                let prototype=match &target{
                    Ty::Closure(definition,actuals,sealed)=>{
                        let original=context.definition(module,*definition,b)?;let SemanticDefinitionBody::FunctionWithBody{signature,..}=&original.body else{return Err(bad("original borrowed Closure omits its body/signature"));};
                        let mut selected=self.environment(module,context,*definition,active,b)?;selected.lexical[0].actuals=actuals.clone();selected.lexical[0].environment=sealed.clone();
                        let rigid=if let Some(prototype)=self.prototypes.get(definition){prototype.clone()}else{let rigid=self.environment(module,context,*definition,active,b)?;self.signature(module,context,signature,&rigid,active,b)?};
                        self.instantiate_prototype(&rigid,Some(*definition),&selected,b)?
                    },
                    _=>{let prototype=self.callable_premise(module,context,&target,env,active,b)?.ok_or_else(||bad("original borrowed indirect Call has no complete Callable premise"))?;self.instantiate_prototype(&prototype,None,&prototype.environment,b)?},
                };
                match prototype.callable_kind{
                    Some(CallableKind::Fn|CallableKind::FunctionPointer)=>{},
                    Some(CallableKind::FnMut)if mutable=>{if let Some(place)=&callee_place{self.written.entry(env.current()?).or_default().insert(place.binder.clone());}},
                    Some(CallableKind::FnMut)=>return Err(bad("original shared callable borrow cannot provide exclusive FnMut invocation")),
                    Some(CallableKind::FnOnce)=>return Err(bad("original borrowed callable cannot transfer its FnOnce owner")),
                    None=>return Err(bad("original borrowed callable has no independently proved invocation capability")),
                }
                self.invoke(module,context,call.id,outer_floor,&prototype,arguments,env,bindings,active,b)
            },
            variable=>{
                let prototype=self.callable_premise(module,context,&variable,env,active,b)?.ok_or_else(||bad("original indirect Call has no complete Callable premise"))?;
                // The premise owns this code signature's quantifiers. Each
                // call gets fresh roles; caller input heads and retained
                // domains are not among those code binders (owner is None).
                let outer_floor=self.next_inference;let prototype=self.instantiate_prototype(&prototype,None,&prototype.environment,b)?;
                if prototype.callable_kind==Some(CallableKind::FnOnce){if let Some(place)=&callee_place{self.read_original(module,context,place,&variable,env,true,active,b)?;}}
                if prototype.callable_kind==Some(CallableKind::FnMut){if let Some(place)=&callee_place{self.written.entry(env.current()?).or_default().insert(place.binder.clone());}}
                self.invoke(module,context,call.id,outer_floor,&prototype,arguments,env,bindings,active,b)
            },
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn invoke(&mut self,module:&TypedModule,context:&CheckedContext,node:NodeId,outer_floor:u64,prototype:&Prototype,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{let outer_heads=self.defer_call_heads(prototype,&[],outer_floor,b)?;self.execute_effect(env,prototype.call_kind,b)?;if prototype.parameters.len()!=arguments.len(){return Err(bad("original Callable arity differs"));}for((argument,expected),mode)in arguments.iter().zip(&prototype.parameters).zip(&prototype.modes){let actual=self.argument(module,context,argument,expected,*mode,env,bindings,active,b)?;self.relate_value(&actual,expected,b)?;}self.finish_call_heads(&outer_heads,node,b)?;self.remember_call_region_parameters(&prototype.parameters,node,env,b)?;Ok(prototype.result.clone())}
    pub(super) fn same_argument(&mut self,left:&Arg,right:&Arg,b:&mut Budget)->Check<()>{match(left,right){(Arg::Type(left),Arg::Type(right))=>self.unify(left,right,b),(Arg::Life(left),Arg::Life(right))=>self.unify_life(left,right,b),_ if left==right=>Ok(()),_=>Err(bad("original Call argument differs from its solved symbolic identity/carrier"))}}
    pub(super) fn callable_premise(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Prototype>>{
        if let Ty::Opaque(owner,arguments)=self.resolved(ty,b)?{return self.opaque_callable(module,context,owner,&arguments,active,b);}
        let mut result=None;
        for(index,frame)in env.lexical.iter().enumerate(){
            let definition=context.definition(module,frame.definition,b)?;
            let mut lexical=self.copy_environment(env,b)?;lexical.lexical.drain(..index);
            for requirement in &definition.requirements{self.callable_premise_row(module,context,requirement,ty,&lexical,&[],&mut result,active,b)?;}
            for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.callable_premise_row(module,context,requirement,ty,&lexical,&[],&mut result,active,b)?;}}}
        }
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn callable_premise_row(&mut self,module:&TypedModule,context:&CheckedContext,requirement:&SemanticRequirementRecipe,ty:&Ty,env:&Env,quantifiers:&[Vec<Life>],result:&mut Option<Prototype>,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let token=active.fresh(b)?;
        active.with_query(DependencyKey{kind:QueryKind::Requirement,token},b,|active,b|{
            match requirement{
                SemanticRequirementRecipe::ForAll{lifetimes,requirements}=>{
                    let mut selected=self.copy_environment(env,b)?;
                    b.spend(quantifiers.iter().fold(quantifiers.len(),|n,frame|n.saturating_add(frame.len())))?;
                    let mut owned=quantifiers.to_vec();
                    // Zero frames retain their recipe identity but do not
                    // consume Bound depth. Nonzero frames keep exact nesting.
                    if *lifetimes!=0{
                        b.spend((*lifetimes as usize).saturating_mul(2).saturating_add(1))?;
                        let frame=(0..*lifetimes).map(|index|Life::Bound(token,index)).collect::<Vec<_>>();
                        selected.bounds.insert(0,frame.clone());owned.push(frame);
                    }
                    for row in requirements{self.callable_premise_row(module,context,row,ty,&selected,&owned,result,active,b)?;}
                },
                SemanticRequirementRecipe::Callable{ty:receiver,signature,kind}=>{
                    let receiver=self.ty(module,context,receiver,env,active,b)?;
                    if !self.same(&receiver,ty,b)?{return Ok(());}
                    let mut prototype=self.signature(module,context,signature,env,active,b)?;
                    b.spend(quantifiers.iter().fold(quantifiers.len(),|n,frame|n.saturating_add(frame.len())))?;
                    prototype.premise_bounds=quantifiers.to_vec();prototype.callable_kind=Some(*kind);
                    if let Some(previous)=result.as_ref(){
                        if previous.parameters.len()!=prototype.parameters.len()||previous.modes!=prototype.modes||previous.result_mode!=prototype.result_mode||previous.call_kind!=prototype.call_kind||previous.bound_lifetimes!=prototype.bound_lifetimes||previous.invocation_lifetimes!=prototype.invocation_lifetimes{return Err(unsupported("original Callable overloads need exact role selection",None));}
                        let(parameters,return_type)=self.callable_signature_types(previous,&prototype,b)?;
                        for(a,c)in parameters.iter().zip(&prototype.parameters){if !self.same(a,c,b)?{return Err(unsupported("original Callable overloads need exact role selection",None));}}
                        if !self.same(&return_type,&prototype.result,b)?{return Err(unsupported("original Callable overloads need exact role selection",None));}
                        // These are conjunctions on the same original receiver,
                        // not interchangeable concrete values with equal Σ.
                        if callable_accepts(previous.callable_kind,*kind){return Ok(());}
                    }
                    *result=Some(prototype);
                },
                _=>{},
            }
            Ok(())
        })
    }
    #[track_caller]
    pub(super) fn obligation(&mut self,module:&TypedModule,context:&CheckedContext,requirement:&SemanticRequirementRecipe,target:&Env,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        // Type recipes keep the target's scope, but every recursively selected
        // provider must discharge its requirements in the actual caller's Γ.
        // In particular a target's pending where clauses are not premises.
        let evidence=self.proof_environment(caller,b)?;
        if self.current!=Some(evidence.current()?){return Err(bad(&format!("original obligation has no exact active caller environment [active {:?}, evidence {:?}, input caller {:?}, target {:?}, inherited {}, callsite {}]",self.current,evidence.current()?,caller.current()?,target.current()?,self.premise_environment.is_some(),std::panic::Location::caller())));}
        let previous=self.premise_environment.replace(evidence);
        let result=self.obligation_inner(module,context,requirement,target,previous.as_ref().unwrap_or(caller),active,b);
        self.premise_environment=previous;
        result
    }
    fn obligation_inner(&mut self,module:&TypedModule,context:&CheckedContext,requirement:&SemanticRequirementRecipe,target:&Env,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        match requirement {
            SemanticRequirementRecipe::TypeEqual{left,right}=>{let left=self.ty(module,context,left,target,active,b)?;let right=self.ty(module,context,right,target,active,b)?;if !self.same(&left,&right,b)?{return Err(bad("original caller does not prove target TypeEqual requirement"));}Ok(())},
            SemanticRequirementRecipe::LifeEqual{left,right}=>{let left=self.life(module,context,left,target,active,b)?;let right=self.life(module,context,right,target,active,b)?;if self.same_life(&left,&right,b)?{Ok(())}else{Err(bad("original caller does not prove the target lifetime equality"))}},
            SemanticRequirementRecipe::Outlives{longer,shorter}=>{let longer=self.life(module,context,longer,target,active,b)?;let shorter=self.life(module,context,shorter,target,active,b)?;if self.outer_outlives(&longer,&shorter,&mut BTreeSet::new(),b)?{Ok(())}else{Err(bad("original caller does not prove every source of the target outer lifetime relation"))}},
            SemanticRequirementRecipe::Callable{ty,signature,kind}=>self.callable_goal(module,context,ty,signature,*kind,target,caller,active,b),
            SemanticRequirementRecipe::ForAll{lifetimes,requirements}=>{
                let mut selected=self.copy_environment(target,b)?;
                let mut evidence=self.copy_environment(caller,b)?;
                if *lifetimes!=0{
                    let token=active.fresh(b)?;b.spend((*lifetimes as usize).saturating_mul(2).saturating_add(2))?;
                    let rigid=(0..*lifetimes).map(|index|Life::Bound(token,index)).collect::<Vec<_>>();
                    selected.bounds.insert(0,rigid.clone());evidence.bounds.insert(0,rigid);
                }
                // Universal introduction adds only fresh rigid names, never
                // the target's unproved requirements as caller assumptions.
                let proof=self.copy_environment(&evidence,b)?;
                let previous=self.premise_environment.replace(proof);
                let result=(||{for requirement in requirements{self.obligation(module,context,requirement,&selected,&evidence,active,b)?;}Ok(())})();
                self.premise_environment=previous;result
            },
            SemanticRequirementRecipe::Trait{ty,application}=>{let receiver=self.ty(module,context,ty,target,active,b)?;let mut arguments=Vec::new();for argument in &application.arguments{arguments.push(self.actual(module,context,argument,target,active,b)?);}self.original_trait_obligation(module,context,&receiver,application.declaration,&arguments,caller,active,b)},
            SemanticRequirementRecipe::Structural{ty,bound}=>{let ty=self.ty(module,context,ty,target,active,b)?;self.structural_obligation(module,context,&ty,*bound,caller,active,b)},
            SemanticRequirementRecipe::Builtin{ty,trait_kind}=>{let ty=self.ty(module,context,ty,target,active,b)?;let bound=match trait_kind{SemanticBuiltinTrait::Copy=>DynStructuralBound::Copy,SemanticBuiltinTrait::Value=>DynStructuralBound::Value,SemanticBuiltinTrait::SyncDrop=>DynStructuralBound::SyncDrop,SemanticBuiltinTrait::StoreElement=>DynStructuralBound::StoreElement,_=>return self.builtin_obligation(module,context,&ty,*trait_kind,caller,active,b)};self.structural_obligation(module,context,&ty,bound,caller,active,b)},
            _=>Err(unsupported("original requirement has no complete symbolic caller proof",None)),
        }
    }
}

#[cfg(test)]
mod quantified_goal_tests{
    use super::*;
    fn address(life:Life,inner:Ty)->Ty{Ty::Address{mutable:false,life,inner:Box::new(inner)}}
    fn signature(input:Life,result:Life)->Prototype{Prototype{parameters:vec![address(input,Ty::Atom(Type::I32))],result:address(result,Ty::Atom(Type::I32)),environment:Env{lexical:vec![],bounds:vec![],invocation:vec![],self_type:None},modes:vec![PassingMode::Infer],result_mode:PassingMode::Infer,call_kind:CallKind::Direct,bound_lifetimes:0,invocation_lifetimes:0,premise_bounds:vec![],callable_kind:Some(CallableKind::FnMut)}}
    #[test]
    fn callable_elision_eliminates_its_own_role_into_the_introduced_goal_frame(){
        let mut checked=CheckedOriginal::default();let mut b=Budget::new(&Limits::default());
        let invocation=Life::Invocation(FrameToken(800),0);let goal=Life::Bound(FrameToken(900),0);let other=Life::Bound(FrameToken(901),0);
        let mut source=signature(invocation.clone(),invocation.clone());source.invocation_lifetimes=1;source.environment.invocation=vec![invocation];
        let mut wanted=signature(goal.clone(),goal.clone());wanted.environment.bounds=vec![vec![goal.clone()]];
        let(actual,flexible)=checked.callable_goal_source(&source,&wanted,None,&mut b).unwrap();
        assert_eq!(actual.invocation_lifetimes,1);assert_eq!(actual.environment.invocation.len(),1);assert_eq!(wanted.invocation_lifetimes,0);assert_eq!(wanted.environment.bounds,vec![vec![goal.clone()]]);
        checked.callable_goal_type(&actual.parameters[0],&wanted.parameters[0],&flexible,&mut b).unwrap();checked.callable_goal_type(&actual.result,&wanted.result,&flexible,&mut b).unwrap();
        assert!(checked.callable_goal_type(&actual.result,&address(other.clone(),Ty::Atom(Type::I32)),&flexible,&mut b).is_err(),"the source's repeated role cannot switch to another nonzero frame");
        let fixed=signature(other.clone(),other);let(actual,flexible)=checked.callable_goal_source(&fixed,&wanted,None,&mut b).unwrap();assert!(checked.callable_goal_type(&actual.parameters[0],&wanted.parameters[0],&flexible,&mut b).is_err(),"a captured rigid role is not universally callable");
        for malformed in [0,2]{let mut bad=source.clone();bad.invocation_lifetimes=malformed;assert!(checked.callable_goal_source(&bad,&wanted,None,&mut b).is_err());}
        let mut bad=wanted.clone();bad.bound_lifetimes=2;assert!(checked.callable_goal_source(&source,&bad,None,&mut b).is_err());
        let mut bad=wanted.clone();bad.modes[0]=PassingMode::BorrowMut;assert!(checked.callable_goal_source(&source,&bad,None,&mut b).is_err());
        let mut bad=wanted.clone();bad.result_mode=PassingMode::Copy;assert!(checked.callable_goal_source(&source,&bad,None,&mut b).is_err());
        let mut bad=wanted;bad.call_kind=CallKind::Suspend;assert!(checked.callable_goal_source(&source,&bad,None,&mut b).is_err());
    }
    #[test]
    fn universal_callable_goal_solves_only_explicit_private_call_roles(){
        let mut checked=CheckedOriginal::default();let mut budget=Budget::new(&Limits::default());
        let private=checked.fresh_life(&mut budget).unwrap();
        let flexible=BTreeSet::from([private.clone()]);
        let goal=Life::Bound(FrameToken(900),0);
        let other_goal=Life::Bound(FrameToken(900),1);
        let retained=Life::Formal(SemanticDefinitionId{module:zil_base::ModuleId(0),index:4},0);
        let scalar=Ty::Atom(Type::I32);
        checked.callable_goal_type(&address(private.clone(),scalar.clone()),&address(goal.clone(),scalar.clone()),&flexible,&mut budget).unwrap();
        assert_eq!(checked.resolved_life(&private,&mut budget).unwrap(),goal);
        // One private variable cannot satisfy two independent goal roles.
        assert!(checked.callable_goal_type(&address(private,scalar.clone()),&address(other_goal.clone(),scalar.clone()),&flexible,&mut budget).is_err());
        for fixed in [Life::Static,retained.clone(),Life::Bound(FrameToken(901),0)]{
            assert!(checked.callable_goal_type(&address(fixed,scalar.clone()),&address(other_goal.clone(),scalar.clone()),&flexible,&mut budget).is_err());
        }
        assert_eq!(checked.resolved_life(&retained,&mut budget).unwrap(),retained);
        let a=SemanticDefinitionId{module:zil_base::ModuleId(0),index:7};
        let z=SemanticDefinitionId{module:zil_base::ModuleId(1),index:7};
        assert!(checked.callable_goal_type(&Ty::Variable(a,0),&Ty::Variable(z,0),&flexible,&mut budget).is_err());
        let constant=|owner|Ty::Nominal(a,vec![Arg::Const(Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Variable(owner,0)})]);
        assert!(checked.callable_goal_type(&constant(a),&constant(z),&flexible,&mut budget).is_err());
        assert!(checked.substitutions.is_empty());
    }
}

impl CheckedOriginal{
    #[allow(clippy::too_many_arguments)]
    fn callable_goal(&mut self,module:&TypedModule,context:&CheckedContext,receiver:&SemanticTypeRecipe,signature:&SemanticSignatureRecipe,kind:CallableKind,target:&Env,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()>{
        let receiver=self.ty(module,context,receiver,target,active,b)?;let receiver=self.resolved(&receiver,b)?;
        let wanted=self.signature(module,context,signature,target,active,b)?;
        let (actual,owner)=match &receiver{
            Ty::Closure(id,actuals,sealed)=>{
                if !self.checked.contains(id){return Err(bad("original Callable source body is not independently complete"));}
                let prototype=self.prototypes.get(id).cloned().ok_or_else(||bad("original Callable source lacks its complete prototype"))?;
                let mut selected=self.environment(module,context,*id,active,b)?;
                selected.lexical[0].actuals=self.resolve_arguments(actuals,0,b)?;
                selected.lexical[0].environment=self.resolve_arguments(sealed,0,b)?;
                (self.specialize_code_prototype(&prototype,&selected,b)?,Some(*id))
            },
            Ty::Code{kind,parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}=>{
                let capability=match kind{SemanticCodeKind::FunctionPointer|SemanticCodeKind::BorrowedFunctionPointer=>Some(CallableKind::FunctionPointer),SemanticCodeKind::ForeignCallback=>None};
                (Prototype{parameters:parameters.clone(),result:*result.clone(),environment:self.copy_environment(caller,b)?,modes:modes.clone(),result_mode:*result_mode,call_kind:*call_kind,bound_lifetimes:*bound_lifetimes,invocation_lifetimes:*invocation_lifetimes,premise_bounds:vec![],callable_kind:capability},None)
            },
            _=>(self.callable_premise(module,context,&receiver,caller,active,b)?.ok_or_else(||bad("original caller does not prove the target Callable requirement"))?,None),
        };
        if !callable_accepts(actual.callable_kind,kind){return Err(unsupported("original Callable source capability is not independently established",None));}
        // Universal elimination instantiates only this source code scheme's
        // private roles. A source's elided Invocation and a target's outer
        // ForAll need not put equivalent quantifiers in the same code field.
        // Their complete directories are checked separately; the real P/R
        // occurrences discharge every substitution, never numeric ordinals.
        let (actual,flexible)=self.callable_goal_source(&actual,&wanted,owner,b)?;
        for(actual,wanted)in actual.parameters.iter().zip(&wanted.parameters){
            let actual=self.normalize_original_type(module,context,actual,caller,active,b)?;
            let wanted=self.normalize_original_type(module,context,wanted,caller,active,b)?;
            self.callable_goal_type(&actual,&wanted,&flexible,b)?;
        }
        let actual=self.normalize_original_type(module,context,&actual.result,caller,active,b)?;
        let wanted=self.normalize_original_type(module,context,&wanted.result,caller,active,b)?;
        self.callable_goal_type(&actual,&wanted,&flexible,b)
    }
    fn callable_goal_source(&mut self,actual:&Prototype,wanted:&Prototype,owner:Option<SemanticDefinitionId>,b:&mut Budget)->Check<(Prototype,BTreeSet<Life>)>{
        if actual.parameters.len()!=wanted.parameters.len()||actual.modes!=wanted.modes||actual.result_mode!=wanted.result_mode||actual.call_kind!=wanted.call_kind{return Err(bad("original Callable requirement has different complete modes/kind"));}
        self.private_prototype_frames(wanted,b)?;
        let(actual,roles)=self.instantiate_prototype_roles(actual,owner,&actual.environment,b)?;
        b.spend(roles.len())?;Ok((actual,roles.values().cloned().collect()))
    }
    fn callable_goal_type(&mut self,actual:&Ty,wanted:&Ty,flexible:&BTreeSet<Life>,b:&mut Budget)->Check<()>{
        let mut sources=Vec::new();let mut goals=Vec::new();
        // Compare complete non-life structure, then discharge every original
        // life occurrence in order. Erasing the scaffold grants no equality.
        let source_shape=self.map_invocation_type(actual,&mut |life,b|{b.spend(1)?;sources.push(life.clone());Ok(Life::Static)},0,b)?;
        let goal_shape=self.map_invocation_type(wanted,&mut |life,b|{b.spend(1)?;goals.push(life.clone());Ok(Life::Static)},0,b)?;
        if sources.len()!=goals.len()||!self.same(&source_shape,&goal_shape,b)?{return Err(bad("original Callable requirement changes its complete rigid Type/Const structure"));}
        for(source,goal)in sources.iter().zip(&goals){
            b.spend(flexible.len().checked_ilog2().map_or(1,|n|n as usize+1))?;
            if flexible.contains(source){self.unify_life(source,goal,b)?;}
            else if !self.same_life(source,goal,b)?{return Err(bad("original Callable requirement changes a rigid caller or retained lifetime"));}
        }
        Ok(())
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
        if proved{Ok(())}else{Err(unsupported(format!("original caller lacks a complete structural/cleanup obligation proof [bound {bound:?}, receiver {}]",type_category(ty)),None))}
    }
    pub(super) fn value_type(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            if self.structural_premise(module,context,ty,DynStructuralBound::Value,env,active,b)?||self.is_copy(module,context,ty,env,active,b)?{return Ok(true);}
            Ok(match self.resolved(ty,b)?{
                Ty::Address{..}|Ty::Desc{..}|Ty::Managed(_)=>true,
                Ty::Dyn(value)=>value.owned,
                Ty::Closure(definition,..)=>self.checked.contains(&definition)&&self.creator_moves.contains_key(&definition),
                Ty::Opaque(owner,arguments)=>{let hidden=self.opaque_hidden(module,context,owner,&arguments,active,b)?;self.value_type(module,context,&hidden,env,active,b)?},
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

impl CheckedOriginal {
    /// Code binders are locally quantified roles. Alpha alignment changes only
    /// the compared signature's own complete binder frames; retained caller
    /// lifetimes and all global equality premises remain rigid and untouched.
    fn callable_signature_types(&self,actual:&Prototype,wanted:&Prototype,b:&mut Budget)->Check<(Vec<Ty>,Ty)>{
        if actual.bound_lifetimes!=wanted.bound_lifetimes||actual.invocation_lifetimes!=wanted.invocation_lifetimes{return Err(bad("original Callable comparison changes its complete code binder domain"));}
        let mut mapping=Vec::new();
        if actual.premise_bounds.len()!=wanted.premise_bounds.len(){return Err(bad("original Callable comparison changes its quantified premise nesting"));}
        for(a,z)in actual.premise_bounds.iter().zip(&wanted.premise_bounds){
            b.spend(a.len().saturating_add(z.len()).saturating_add(1))?;
            if a.len()!=z.len(){return Err(bad("original Callable comparison changes a quantified premise frame"));}
            for(a,z)in a.iter().zip(z){mapping.push((a.clone(),z.clone()));}
        }
        if actual.invocation_lifetimes!=0{
            if actual.environment.invocation.len()!=actual.invocation_lifetimes as usize||wanted.environment.invocation.len()!=wanted.invocation_lifetimes as usize{return Err(bad("original Callable comparison lacks its exact invocation binder frame"));}
            for(a,z)in actual.environment.invocation.iter().zip(&wanted.environment.invocation){mapping.push((a.clone(),z.clone()));}
        }
        if actual.bound_lifetimes!=0{
            let a=actual.environment.bounds.first().ok_or_else(||bad("original Callable comparison lacks its bound frame"))?;
            let z=wanted.environment.bounds.first().ok_or_else(||bad("original Callable requirement lacks its bound frame"))?;
            if a.len()!=actual.bound_lifetimes as usize||z.len()!=wanted.bound_lifetimes as usize{return Err(bad("original Callable comparison changes a protected binder frame"));}
            for(a,z)in a.iter().zip(z){mapping.push((a.clone(),z.clone()));}
        }
        b.spend(mapping.len().saturating_mul(3).saturating_add(1))?;
        let mut sources=BTreeSet::new();let mut targets=BTreeSet::new();
        for(a,z)in &mapping{if !sources.insert(a)||!targets.insert(z){return Err(bad("original Callable code-role alignment is not bijective"));}}
        let mut rewrite=|life:&Life,b:&mut Budget|{b.spend(mapping.len().saturating_add(1))?;Ok(mapping.iter().find(|(a,_)|a==life).map(|(_,z)|z.clone()).unwrap_or_else(||life.clone()))};
        let mut parameters=Vec::new();for parameter in &actual.parameters{parameters.push(self.map_invocation_type(parameter,&mut rewrite,0,b)?);}
        let result=self.map_invocation_type(&actual.result,&mut rewrite,0,b)?;
        Ok((parameters,result))
    }
}

impl CheckedOriginal {
    /// Written syntax may omit Life actuals. The solved vector still has one
    /// slot per original mixed formal: only an elided Life can be skipped,
    /// and all explicit Type/Const/Life arguments retain their written order.
    pub(super) fn written_original_slots(formals:&[SemanticFormal],types:Option<&[SemanticTypeRecipe]>,mixed:Option<&[CoreGenericArgument<SemanticTypeRecipe>]>,b:&mut Budget)->Check<Vec<Option<usize>>>{
        if types.is_some()&&mixed.is_some(){return Err(bad("original application has two competing written actual lists"));}
        let count=types.map_or_else(||mixed.map_or(0,<[CoreGenericArgument<SemanticTypeRecipe>]>::len),<[SemanticTypeRecipe]>::len);
        b.spend(formals.len().saturating_add(count))?;
        let mut slots=Vec::with_capacity(formals.len());let mut next=0;
        for formal in formals {
            let argument=if types.is_some(){(next<count).then_some(0)}else{mixed.and_then(|arguments|arguments.get(next)).map(|argument|match argument{CoreGenericArgument::Type(_)=>0,CoreGenericArgument::Const(_)=>1,CoreGenericArgument::Lifetime(_)=>2})};
            let kind=match formal{SemanticFormal::Type{..}=>0,SemanticFormal::Const{..}=>1,SemanticFormal::Lifetime=>2};
            match argument {
                None=>slots.push(None),
                Some(actual)if kind==2&&actual!=2=>slots.push(None),
                Some(actual)if actual==kind=>{slots.push(Some(next));next+=1;},
                Some(_)=>return Err(bad("written original argument changes full-ordinal kind")),
            }
        }
        if next!=count{return Err(bad("original application has excess written mixed arguments"));}Ok(slots)
    }
}

#[cfg(test)]
mod written_kind_tests {
    use super::*;
    fn scalar()->SemanticTypeRecipe{SemanticTypeRecipe::Atom(Type::I32)}
    fn formal_type()->SemanticFormal{SemanticFormal::Type{requirements:vec![]}}
    fn formal_const()->SemanticFormal{SemanticFormal::Const{carrier:scalar()}}
    fn ty()->CoreGenericArgument<SemanticTypeRecipe>{CoreGenericArgument::Type(scalar())}
    fn life(name:&str)->CoreGenericArgument<SemanticTypeRecipe>{CoreGenericArgument::Lifetime(name.into())}
    fn constant()->CoreGenericArgument<SemanticTypeRecipe>{CoreGenericArgument::Const(CoreExpr{id:NodeId(1),kind:CoreExprKind::Integer("3".into()),span:Span{file:zil_base::FileId(0),start:0,end:1}})}
    fn slots(formals:&[SemanticFormal],arguments:&[CoreGenericArgument<SemanticTypeRecipe>])->Check<Vec<Option<usize>>>{CheckedOriginal::written_original_slots(formals,None,Some(arguments),&mut Budget::new(&Limits::default()))}
    #[test]
    fn omitted_lives_preserve_complete_mixed_slots_and_explicit_order(){
        let map_get=[SemanticFormal::Lifetime,formal_type(),formal_type()];
        assert_eq!(slots(&map_get,&[ty(),ty()]).unwrap(),vec![None,Some(0),Some(1)]);
        let types=[scalar(),scalar()];
        assert_eq!(CheckedOriginal::written_original_slots(&map_get,Some(&types),None,&mut Budget::new(&Limits::default())).unwrap(),vec![None,Some(0),Some(1)]);
        let interleaved=[SemanticFormal::Lifetime,formal_type(),SemanticFormal::Lifetime,formal_const(),formal_type(),SemanticFormal::Lifetime];
        assert_eq!(slots(&interleaved,&[ty(),life("a"),constant(),ty()]).unwrap(),vec![None,Some(0),Some(1),Some(2),Some(3),None]);
        assert_eq!(slots(&interleaved,&[life("a"),ty(),life("b"),constant(),ty(),life("c")]).unwrap(),(0..6).map(Some).collect::<Vec<_>>());
        assert_eq!(slots(&interleaved,&[]).unwrap(),vec![None;6],"missing constraints remain inference obligations in their own full slots");
        assert_eq!(slots(&[SemanticFormal::Lifetime,SemanticFormal::Lifetime,formal_type()],&[life("a"),life("a"),ty()]).unwrap(),vec![Some(0),Some(1),Some(2)],"equal written life images must retain both formal occurrences");
        assert_eq!(slots(&map_get,&[ty()]).unwrap(),vec![None,Some(0),None]);
    }
    #[test]
    fn wrong_kinds_cannot_skip_types_or_consts_or_leave_extra_actuals(){
        for(formals,arguments)in [
            (vec![formal_type(),formal_const()],vec![constant(),ty()]),
            (vec![formal_const(),formal_type()],vec![ty()]),
            (vec![formal_type(),SemanticFormal::Lifetime],vec![life("a"),ty()]),
            (vec![SemanticFormal::Lifetime,formal_type()],vec![life("a"),life("b"),ty()]),
            (vec![SemanticFormal::Lifetime,formal_type(),formal_const()],vec![ty(),ty(),constant()]),
            (vec![formal_type()],vec![ty(),ty()]),
            (vec![SemanticFormal::Lifetime],vec![ty()]),
            (vec![],vec![life("a")]),
        ]{let error=slots(&formals,&arguments).unwrap_err();assert_eq!(error.category,zil_base::ResultCategory::SafeReject);assert!(error.message.contains("full-ordinal kind")||error.message.contains("excess written mixed"),"{error:?}");}
        let formals=[formal_const()];let types=[scalar()];
        assert!(CheckedOriginal::written_original_slots(&formals,Some(&types),None,&mut Budget::new(&Limits::default())).unwrap_err().message.contains("full-ordinal kind"));
        assert!(CheckedOriginal::written_original_slots(&[],Some(&[]),Some(&[]),&mut Budget::new(&Limits::default())).is_err(),"two competing syntax lists cannot share a full-ordinal map");
    }
}

impl CheckedOriginal{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn remember_inferred_type_sources(&mut self,module:&TypedModule,owner:SemanticDefinitionId,site:SemanticSiteId,definition:&SemanticDefinition,signature:&SemanticSignatureRecipe,arguments:&[SemanticOriginalExpr],parameter_offset:usize,written:&[Option<usize>],actuals:&[Arg],b:&mut Budget)->Check<()>{
        if written.len()!=definition.parameters.len()||actuals.len()!=definition.parameters.len()||signature.parameters.len()!=arguments.len().saturating_add(parameter_offset){return Err(bad("inferred whole Type sources change their full original parameter domains"));}
        for(ordinal,formal)in definition.parameters.iter().enumerate(){
            if written[ordinal].is_some()||!matches!(formal,SemanticFormal::Type{..}){continue;}
            let Arg::Type(inferred)=&actuals[ordinal]else{return Err(bad("inferred Type source has another complete kind"));};let mut sources=Vec::new();
            for(parameter,argument)in signature.parameters[parameter_offset..].iter().zip(arguments){
                let mut parameter=&parameter.ty;let mut depth=0usize;
                while let SemanticTypeRecipe::Address{inner,..}=parameter{b.spend(1)?;depth=depth.checked_add(1).ok_or_else(||limit("inferred Type source borrow depth exhausted"))?;parameter=inner;}
                if !matches!(parameter,SemanticTypeRecipe::Formal(reference)if reference.scope==SemanticScopeRef::Ancestor(0)&&reference.ordinal==ordinal as u32){continue;}
                let mut node=Some(argument.id);
                for _ in 0..depth{let Some(current)=node else{break;};node=self.borrow_operand_source(module,owner,&SemanticOriginalTypeSource::Expression(current),b)?;}
                let Some(node)=node else{continue;};let source=self.original_type(owner,&SemanticOriginalTypeSource::Expression(node),b)?;
                if self.same(inferred,&source,b)?{sources.push((node,false));}
                else{let view=self.contextual_borrow_value(&source,inferred,b)?;if self.same(inferred,&view,b)?{sources.push((node,true));}}
            }
            if !sources.is_empty(){b.spend(sources.len())?;self.call_type_sources.insert((owner,site,ordinal as u32),sources);}
        }Ok(())
    }
}
