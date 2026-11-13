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
        self.callable_nodes.insert((env.current()?,call.id),base.id);
        let callee_type=self.expression(module,context,base,env,bindings,None,active,b)?;
        match self.resolved(&callee_type,b)?{
            Ty::Function(target)=>{
                let definition=context.definition(module,target,b)?;
                let SemanticDefinitionBody::FunctionWithBody{signature,..}=&definition.body else{return Err(bad("original Call target omits its complete body"));};
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
                    let written=if let Some(types)=written_types{types.get(ordinal).map(|ty|self.ty(module,context,ty,env,active,b).map(Arg::Type)).transpose()?}else if let Some(mixed)=written_mixed{match mixed.get(ordinal){Some(CoreGenericArgument::Type(ty))=>Some(Arg::Type(self.ty(module,context,ty,env,active,b)?)),Some(CoreGenericArgument::Const(_))=>return Err(unsupported("original written const argument requires its solved full carrier",None)),Some(CoreGenericArgument::Lifetime(_))=>return Err(unsupported("original written lifetime needs its original name/full-ordinal binding directory",None)),None=>None}}else{None};
                    let value=match(written,formal){(Some(value),_)=>value,(None,SemanticFormal::Type{..})=>Arg::Type(self.fresh(b)?),(None,_)=>return Err(unsupported("original non-Type inference needs its complete scalar/lifetime constraints",None))};
                    if !matches!((formal,&value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Const{..},Arg::Const(_))|(SemanticFormal::Lifetime,Arg::Life(_))){return Err(bad("written original argument changes full-ordinal kind"));}
                    checked.push(value);
                }
                let mut selected=self.environment(module,context,target,active,b)?;
                selected.lexical[0].actuals=checked.clone();
                let prototype=self.signature(module,context,signature,&selected,active,b)?;
                if prototype.parameters.len()!=arguments.len(){return Err(bad("original Call value arity differs from the declared signature"));}
                for(argument,parameter)in arguments.iter().zip(&prototype.parameters){let actual=self.expression(module,context,argument,env,bindings,Some(parameter),active,b)?;self.unify(&actual,parameter,b)?;}
                if let Some(expected)=expected{self.unify(&prototype.result,expected,b)?;}
                // A target's requirements are obligations here. They never enter
                // the caller's equality assumptions merely by selecting target.
                for requirement in &definition.requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}
                for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}}}
                if let Some(proposed)=proposed{for(recipe,value)in proposed.iter().zip(&checked){if !matches!(recipe,SemanticActualRecipe::Infer){let claim=self.actual(module,context,recipe,env,active,b)?;self.same_argument(&claim,value,b)?;}}}
                self.calls.insert((env.current()?,site.id),checked);
                self.call_targets.insert((env.current()?,site.id),target);
                Ok(prototype.result)
            },
            Ty::Closure(definition,actuals,sealed)=>{
                if written_types.is_some()||written_mixed.is_some(){return Err(bad("original closure invocation has unsupported generic application"));}
                let original=context.definition(module,definition,b)?;let SemanticDefinitionBody::FunctionWithBody{signature,..}=&original.body else{return Err(bad("original Closure has no complete signature/body"));};
                let mut selected=self.environment(module,context,definition,active,b)?;selected.lexical[0].actuals=actuals;selected.lexical[0].environment=sealed;
                let prototype=if let Some(prototype)=self.prototypes.get(&definition){prototype.clone()}else{self.signature(module,context,signature,&selected,active,b)?};
                self.invoke(module,context,&prototype,arguments,env,bindings,active,b)
            },
            Ty::Code{parameters,result}=>self.invoke(module,context,&Prototype{parameters,result:*result,environment:env.clone()},arguments,env,bindings,active,b),
            variable=>{
                let prototype=self.callable_premise(module,context,&variable,env,active,b)?.ok_or_else(||bad("original indirect Call has no complete Callable premise"))?;
                self.invoke(module,context,&prototype,arguments,env,bindings,active,b)
            },
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn invoke(&mut self,module:&TypedModule,context:&CheckedContext,prototype:&Prototype,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{if prototype.parameters.len()!=arguments.len(){return Err(bad("original Callable arity differs"));}for(argument,expected)in arguments.iter().zip(&prototype.parameters){let actual=self.expression(module,context,argument,env,bindings,Some(expected),active,b)?;self.unify(&actual,expected,b)?;}Ok(prototype.result.clone())}
    fn same_argument(&mut self,left:&Arg,right:&Arg,b:&mut Budget)->Check<()>{match(left,right){(Arg::Type(left),Arg::Type(right))=>self.unify(left,right,b),_ if left==right=>Ok(()),_=>Err(bad("original Call argument differs from its solved symbolic identity/carrier"))}}
    fn callable_premise(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Prototype>>{
        let mut result=None;
        for(index,frame)in env.lexical.iter().enumerate(){let definition=context.definition(module,frame.definition,b)?;let mut lexical=env.clone();lexical.lexical=lexical.lexical[index..].to_vec();let mut requirements=definition.requirements.iter().collect::<Vec<_>>();for formal in &definition.parameters{if let SemanticFormal::Type{requirements:rows}=formal{requirements.extend(rows);}}
            for requirement in requirements{if let SemanticRequirementRecipe::Callable{ty:receiver,signature,..}=requirement{let receiver=self.ty(module,context,receiver,&lexical,active,b)?;if self.same(&receiver,ty,b)?{let prototype=self.signature(module,context,signature,&lexical,active,b)?;if let Some(previous)=&result{let previous:&Prototype=previous;if previous.parameters.len()!=prototype.parameters.len(){return Err(unsupported("original Callable overloads need exact role selection",None));}for(a,c)in previous.parameters.iter().zip(&prototype.parameters){if !self.same(a,c,b)?{return Err(unsupported("original Callable overloads need exact role selection",None));}}if !self.same(&previous.result,&prototype.result,b)?{return Err(unsupported("original Callable overloads need exact role selection",None));}}result=Some(prototype);}}}
        }Ok(result)
    }
    fn obligation(&mut self,module:&TypedModule,context:&CheckedContext,requirement:&SemanticRequirementRecipe,target:&Env,caller:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        match requirement {
            SemanticRequirementRecipe::TypeEqual{left,right}=>{let left=self.ty(module,context,left,target,active,b)?;let right=self.ty(module,context,right,target,active,b)?;if !self.same(&left,&right,b)?{return Err(bad("original caller does not prove target TypeEqual requirement"));}Ok(())},
            SemanticRequirementRecipe::Callable{ty,signature,..}=>{
                let receiver=self.ty(module,context,ty,target,active,b)?;let receiver=self.resolved(&receiver,b)?;let wanted=self.signature(module,context,signature,target,active,b)?;
                let actual=match &receiver{Ty::Closure(id,..)=>self.prototypes.get(id).cloned(),Ty::Code{parameters,result}=>Some(Prototype{parameters:parameters.clone(),result:*result.clone(),environment:caller.clone()}),_=>self.callable_premise(module,context,&receiver,caller,active,b)?}.ok_or_else(||bad("original caller does not prove the target Callable requirement"))?;
                if actual.parameters.len()!=wanted.parameters.len(){return Err(bad("original Callable requirement arity differs"));}for(actual,wanted)in actual.parameters.iter().zip(&wanted.parameters){if !self.same(actual,wanted,b)?{return Err(bad("original Callable requirement parameters lack their original relation"));}}if !self.same(&actual.result,&wanted.result,b)?{return Err(bad("original Callable requirement result lacks its original relation"));}Ok(())
            },
            SemanticRequirementRecipe::Structural{ty,bound}=>{let ty=self.ty(module,context,ty,target,active,b)?;if let Ty::Atom(ty)=self.resolved(&ty,b)?{if ty.is_b0_scalar()&&matches!(bound,DynStructuralBound::Copy|DynStructuralBound::Value|DynStructuralBound::SyncDrop|DynStructuralBound::StoreElement){return Ok(());}}Err(unsupported("original structural obligation needs its full symbolic provider proof",None))},
            _=>Err(unsupported("original requirement has no complete symbolic caller proof",None)),
        }
    }
}
