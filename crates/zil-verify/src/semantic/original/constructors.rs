//! An enum constructor is an original nominal operation, never a fabricated
//! function target. Its full actuals are solved in the caller's original Γ.
use super::*;
impl CheckedOriginal{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn variant_constructor(&mut self,module:&TypedModule,context:&CheckedContext,call:&SemanticOriginalExpr,base:&SemanticOriginalExpr,written_types:Option<&[SemanticTypeRecipe]>,written_mixed:Option<&[CoreGenericArgument<SemanticTypeRecipe>]>,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Ty>>{
        let CoreExprKind::Member{object,member,owner:None}=&base.kind else{return Ok(None);};
        let(owner,owner_types,owner_mixed)=match &object.kind{CoreExprKind::TypeApply{callee,type_arguments}=>(callee.as_ref(),Some(type_arguments.as_slice()),None),CoreExprKind::GenericApply{callee,arguments}=>(callee.as_ref(),None,Some(arguments.as_slice())),_=>(object.as_ref(),None,None)};
        if (owner_types.is_some()||owner_mixed.is_some())&&(written_types.is_some()||written_mixed.is_some()){return Err(bad("original enum constructor duplicates its written full actual application"));}
        let written_types=written_types.or(owner_types);let written_mixed=written_mixed.or(owner_mixed);
        let Some((id,alias))=self.constructor_owner(module,context,owner,env,bindings,active,b)?else{return Ok(None);};
        let definition=context.definition(module,id,b)?;
        let SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Enum,body:SemanticNominalBody::Enum{variants},..}=&definition.body else{return Ok(None);};
        b.spend(variants.len().saturating_mul(member.len().saturating_add(1)))?;
        let (ordinal,row)=variants.iter().enumerate().find(|(_,row)|row.name==*member).ok_or_else(||bad("original enum constructor selects no declared variant"))?;
        let variant=u32::try_from(ordinal).map_err(|_|limit("original enum variant ordinal exceeds its complete index domain"))?;let fields=&row.fields;
        if fields.len()!=arguments.len(){return Err(bad("original enum constructor does not supply every declared field"));}
        let contextual=expected.map(|ty|self.resolved(ty,b)).transpose()?;
        let contextual=match contextual{Some(Ty::Nominal(actual,values))if actual==id=>Some(values),_=>None};
        let written=written_types.map_or_else(||written_mixed.map_or(0,<[CoreGenericArgument<SemanticTypeRecipe>]>::len),<[SemanticTypeRecipe]>::len);
        if written>definition.parameters.len(){return Err(bad("original enum constructor has excess mixed actuals"));}
        let mut actuals=Vec::new();
        for(ordinal,formal)in definition.parameters.iter().enumerate(){
            let explicit=if let Some(types)=written_types{types.get(ordinal).map(|ty|self.ty(module,context,ty,env,active,b).map(Arg::Type)).transpose()?}else if let Some(values)=written_mixed{match values.get(ordinal){Some(CoreGenericArgument::Type(ty))=>Some(Arg::Type(self.ty(module,context,ty,env,active,b)?)),Some(CoreGenericArgument::Lifetime(name))=>{let value=self.named(module,context,env.current()?,name,SemanticOriginalNamespace::Lifetime,b)?.ok_or_else(||bad("enum constructor lifetime has no original name binding"))?;let SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Life(_))=value else{return Err(bad("enum constructor lifetime selects another formal kind"));};Some(self.actual(module,context,recipe,env,active,b)?)},Some(CoreGenericArgument::Const(_))=>return Err(unsupported("original enum written Const needs its complete original expression/carrier solution",None)),None=>None}}else{None};
            let value=match explicit.or_else(||alias.as_ref().and_then(|values|values.get(ordinal)).cloned()).or_else(||contextual.as_ref().and_then(|values|values.get(ordinal)).cloned()){Some(value)=>value,None=>match formal{SemanticFormal::Type{..}=>Arg::Type(self.fresh(b)?),SemanticFormal::Lifetime=>Arg::Life(self.fresh_life(b)?),_=>return Err(unsupported("original enum Const inference has no complete caller constraint",None))}};
            if !matches!((formal,&value),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original enum constructor changes a full-ordinal formal kind"));}
            if let Some(old)=alias.as_ref().and_then(|values|values.get(ordinal)){self.same_argument(old,&value,b)?;}
            if let Some(old)=contextual.as_ref().and_then(|values|values.get(ordinal)){self.same_argument(old,&value,b)?;}
            actuals.push(value);
        }
        let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=actuals.clone();
        for(field,value)in fields.iter().zip(arguments){let parameter=self.ty(module,context,field,&selected,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&parameter),active,b)?;self.unify(&actual,&parameter,b)?;}
        let actuals=self.resolve_arguments(&actuals,0,b)?;
        for value in &actuals{match value{Arg::Type(ty)if usage::has_inference(ty,&self.substitutions,b)?=>return Err(bad("original enum Type actual is not uniquely determined")),Arg::Life(Life::Infer(_))=>return Err(bad("original enum lifetime actual is not uniquely determined")),_=>{}}}
        selected.lexical[0].actuals=actuals.clone();
        for requirement in &definition.requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}
        for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}}}
        let caller=context.definition(module,env.current()?,b)?;let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&caller.body else{return Err(bad("original enum constructor has no complete caller body"));};
        b.spend(site_nodes.len().saturating_mul(sites.len().saturating_add(1)))?;
        let eligible=site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(call.id)).filter_map(|row|sites.iter().find(|site|site.id==row.site)).collect::<Vec<_>>();
        let calls=eligible.iter().copied().filter(|site|matches!(site.kind,SemanticSiteRecipeKind::OriginalCall)).collect::<Vec<_>>();
        let rows=if calls.is_empty(){eligible.into_iter().filter(|site|matches!(site.kind,SemanticSiteRecipeKind::Construct{..})).collect::<Vec<_>>()}else{calls};
        if rows.len()!=1{return Err(bad("original enum constructor lacks one exact Call or zero-field formation occurrence"));}let site=rows[0];
        self.constructors.insert((env.current()?,site.id),VariantConstruction{definition:id,variant,actuals:actuals.clone()});
        self.execute_effect(env,CallKind::Direct,b)?;Ok(Some(Ty::Nominal(id,actuals)))
    }
    fn constructor_owner(&mut self,module:&TypedModule,context:&CheckedContext,owner:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<(SemanticDefinitionId,Option<Vec<Arg>>)>>{
        let mut node=owner;let mut tail=Vec::new();loop{b.spend(1)?;match &node.kind{
            CoreExprKind::Group(inner)=>node=inner,
            CoreExprKind::Member{object,member,owner:None}=>{b.spend(member.len())?;tail.push(member.as_str());node=object;},
            CoreExprKind::Name(name)=>{
                if bindings.contains_key(name){return Ok(None);}b.spend(name.len().saturating_add(tail.iter().map(|part|part.len().saturating_add(2)).sum::<usize>()))?;
                let mut path=name.clone();for part in tail.iter().rev(){path.push_str("::");path.push_str(part);}
                if self.named(module,context,env.current()?,&path,SemanticOriginalNamespace::Value,b)?.is_some(){return Ok(None);}
                let Some(value)=self.named(module,context,env.current()?,&path,SemanticOriginalNamespace::Type,b)?else{return Ok(None);};
                return match value{SemanticOriginalNameValue::Definition(id)=>Ok(Some((*id,None))),SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(recipe))=>match self.ty(module,context,recipe,env,active,b)?{Ty::Nominal(id,args)=>Ok(Some((id,Some(args)))),_=>Ok(None)},_=>Err(bad("original constructor owner has a different namespace kind"))};
            },_=>return Ok(None),
        }}
    }
    pub(in crate::semantic) fn variant_call(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,env:&EvalEnv,b:&mut Budget)->Check<(u32,Vec<SemanticActualRecipe>)>{
        b.spend(1)?;let value=self.constructors.get(&(definition,site)).ok_or_else(||bad("actual enum construction has no independently checked original constructor"))?;
        let _=value.definition;let mut actuals=Vec::new();for value in &value.actuals{actuals.push(self.reify_argument(module,value,env,b)?);}Ok((value.variant,actuals))
    }
}

impl CheckedOriginal {
    /// A type-qualified trait method carries the same complete application as
    /// a value method. The declaration binding alone supplies neither Self nor
    /// the trait's mixed actuals, and never selects an executable provider.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn static_trait_call(&mut self,module:&TypedModule,context:&CheckedContext,target:SemanticDefinitionId,call:&SemanticOriginalExpr,base:&SemanticOriginalExpr,written_types:Option<&[SemanticTypeRecipe]>,written_mixed:Option<&[CoreGenericArgument<SemanticTypeRecipe>]>,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Ty>> {
        let original=context.definition(module,target,b)?;
        let Some(parent)=original.parent else{return Ok(None);};
        let scope=context.definition(module,parent,b)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration})=&scope.body else{return Ok(None);};
        let CoreExprKind::Member{object,member,owner:None}=&base.kind else{return Ok(None);};
        let mut node=object.as_ref();let mut parts=Vec::new();
        let receiver=loop{b.spend(1)?;match &node.kind{
            CoreExprKind::Group(inner)=>node=inner,
            CoreExprKind::Member{object,member,owner:None}=>{b.spend(member.len())?;parts.push(member.as_str());node=object;},
            CoreExprKind::Name(name)=>{
                if bindings.contains_key(name){return Ok(None);}
                b.spend(name.len().saturating_add(parts.iter().map(|part|part.len().saturating_add(2)).sum::<usize>()))?;
                let mut path=name.clone();for part in parts.iter().rev(){path.push_str("::");path.push_str(part);}
                let Some(value)=self.named(module,context,env.current()?,&path,SemanticOriginalNamespace::Type,b)?else{return Ok(None);};
                break match value{
                    SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(recipe))=>self.ty(module,context,recipe,env,active,b)?,
                    SemanticOriginalNameValue::Definition(id)=>{let definition=context.definition(module,*id,b)?;if definition.kind!=SemanticDefinitionKind::Nominal||!definition.parameters.is_empty(){return Ok(None);}Ty::Nominal(*id,vec![])},
                    _=>return Err(bad("original static receiver does not denote a complete Type application")),
                };
            },
            _=>return Ok(None),
        }};
        let mut applications=self.original_method_applications(module,context,&receiver,member,env,active,b)?;
        // The bound must independently name the exact declaration selected by
        // the lexical Value namespace, including its original member ordinal.
        if applications.len()!=1{return Err(bad("original static member lacks one complete trait application"));}
        let(actual_declaration,trait_arguments,ordinal,method)=applications.pop().unwrap();
        if actual_declaration!=*declaration||method!=target{return Err(bad("original static member declaration differs from its complete trait application"));}
        let SemanticDefinitionBody::FunctionWithBody{signature,receiver_parameter:None,..}=&original.body else{return Err(bad("original static member requires a value receiver"));};
        let mut selected=self.environment(module,context,target,active,b)?;
        let frame=selected.lexical.iter_mut().find(|row|row.definition==scope.id).ok_or_else(||bad("original static member omits its trait parent"))?;
        frame.actuals=self.resolve_arguments(&trait_arguments,0,b)?;
        selected.self_type=Some((scope.id,self.resolved(&receiver,b)?));
        let written=written_types.map_or_else(||written_mixed.map_or(0,<[CoreGenericArgument<SemanticTypeRecipe>]>::len),<[SemanticTypeRecipe]>::len);
        if written>original.parameters.len(){return Err(bad("original static member has excess written mixed actuals"));}
        let mut actuals=Vec::new();
        for(ordinal,formal)in original.parameters.iter().enumerate(){
            let written=if let Some(types)=written_types{types.get(ordinal).map(|ty|self.ty(module,context,ty,env,active,b).map(Arg::Type)).transpose()?}else if let Some(mixed)=written_mixed{match mixed.get(ordinal){
                Some(CoreGenericArgument::Type(ty))=>Some(Arg::Type(self.ty(module,context,ty,env,active,b)?)),
                Some(CoreGenericArgument::Lifetime(name))=>{let value=self.named(module,context,env.current()?,name,SemanticOriginalNamespace::Lifetime,b)?.ok_or_else(||bad("original static member lifetime is unresolved"))?;let SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Life(_))=value else{return Err(bad("original static member lifetime has another kind"));};Some(self.actual(module,context,recipe,env,active,b)?)},
                Some(CoreGenericArgument::Const(_))=>return Err(unsupported("original static method Const requires its exact declared carrier",None)),None=>None,
            }}else{None};
            let actual=match(written,formal){(Some(value),_)=>value,(None,SemanticFormal::Type{..})=>Arg::Type(self.fresh(b)?),(None,SemanticFormal::Lifetime)=>Arg::Life(self.fresh_life(b)?),_=>return Err(unsupported("original static method Const is not independently inferred",None))};
            if !matches!((formal,&actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original static method changes a full-ordinal actual kind"));}actuals.push(actual);
        }
        selected.lexical[0].actuals=actuals;
        let prototype=self.call_signature(module,context,target,signature,&selected,active,b)?;
        if prototype.parameters.len()!=arguments.len(){return Err(bad("original static method argument arity differs"));}
        self.execute_effect(env,prototype.call_kind,b)?;
        if let Some(expected)=expected{if !associated::has_projection(&prototype.result,b)?{self.unify(&prototype.result,expected,b)?;}}
        for((argument,parameter),mode)in arguments.iter().zip(&prototype.parameters).zip(&prototype.modes){let parameter=self.normalize_original_type(module,context,parameter,env,active,b)?;let actual=self.argument(module,context,argument,&parameter,*mode,env,bindings,active,b)?;self.unify(&actual,&parameter,b)?;}
        for definition in [scope,original]{
            let depth=selected.lexical.iter().position(|frame|frame.definition==definition.id).ok_or_else(||bad("original static method obligation loses its declaration scope"))?;
            let mut obligation_env=selected.clone();obligation_env.lexical=obligation_env.lexical[depth..].to_vec();
            for requirement in &definition.requirements{self.obligation(module,context,requirement,&obligation_env,env,active,b)?;}
            for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&obligation_env,env,active,b)?;}}}
        }
        let result=self.normalize_original_type(module,context,&prototype.result,env,active,b)?;
        if let Some(expected)=expected{self.unify(&result,expected,b)?;}
        let caller=context.definition(module,env.current()?,b)?;let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&caller.body else{return Err(bad("original static method has no caller body"));};
        b.spend(site_nodes.len().saturating_mul(sites.len().saturating_add(1)))?;
        let mut rows=site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(call.id)).filter_map(|row|sites.iter().find(|site|site.id==row.site)).filter(|site|matches!(site.kind,SemanticSiteRecipeKind::OriginalCall));
        let site=rows.next().ok_or_else(||bad("original static member requires its exact BodyCall occurrence"))?;
        if rows.next().is_some(){return Err(bad("original static member Call occurrence is ambiguous"));}
        let actuals=self.resolve_arguments(&selected.lexical[0].actuals,0,b)?;
        self.trait_calls.insert((env.current()?,site.id),super::trait_calls::TraitCall{receiver,declaration:*declaration,arguments:trait_arguments,member:ordinal,actuals,node:call.id,receiver_mode:None});
        if prototype.result_mode==PassingMode::Create{b.spend(1)?;self.created.insert((env.current()?,call.id));}
        Ok(Some(result))
    }
}

impl CheckedOriginal {
    /// The structural cursor admits a Member only as a possible nullary enum
    /// constructor. Its checked nominal operation must exist even when no
    /// actual instance or emitted instruction is present for this definition.
    pub(super) fn check_constructor_sites(&self,index:&super::index::Index<'_>,b:&mut Budget)->Check<()> {
        for function in index.definitions.values(){
            for site in function.sites{
                b.spend(1)?;
                if !matches!(site.kind,SemanticSiteRecipeKind::Construct{..}){continue;}
                let Some(super::index::Node::Expression(expression))=function.nodes.get(&site.id)else{continue;};
                if matches!(expression.kind,CoreExprKind::Member{..})&&!self.constructors.contains_key(&(function.definition.id,site.id)){
                    return Err(bad("original Member formation is not its independently checked nominal constructor"));
                }
            }
        }
        Ok(())
    }
}
