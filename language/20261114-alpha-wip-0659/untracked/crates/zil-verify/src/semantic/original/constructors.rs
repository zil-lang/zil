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
        let (variant,fields)=variants.iter().enumerate().find(|(_,row)|row.name==*member).map(|(ordinal,row)|(ordinal as u32,&row.fields)).ok_or_else(||bad("original enum constructor selects no declared variant"))?;
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
        let mut rows=site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(call.id)).filter_map(|row|sites.iter().find(|site|site.id==row.site)).filter(|site|matches!(site.kind,SemanticSiteRecipeKind::OriginalCall));
        let site=rows.next().ok_or_else(||bad("original enum constructor lacks its actual source Call occurrence"))?;if rows.next().is_some(){return Err(bad("original enum constructor has several source Call occurrences"));}
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
