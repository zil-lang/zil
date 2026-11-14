//! Instantiate a proved original symbolic term in the actual caller frame.
//! This never infers a source type from the Typed value it is checking.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic) fn callable_source(&self,_module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<NodeId>{b.spend(1)?;self.callable_nodes.get(&(definition,node)).copied().ok_or_else(||bad("actual callable has no independently checked original callee expression"))}

    pub(in crate::semantic) fn type_recipe(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,env:&EvalEnv,b:&mut Budget)->Check<SemanticTypeRecipe>{
        let value=self.original_type(definition,source,b)?;
        self.reify_type(module,&value,env,b)
    }
    pub(in crate::semantic) fn call_recipes(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,env:&EvalEnv,b:&mut Budget)->Check<(SemanticDefinitionId,Vec<SemanticActualRecipe>)>{
        let target=*self.call_targets.get(&(definition,site)).ok_or_else(||bad("actual static Call has no original symbolically resolved target"))?;
        let arguments=self.calls.get(&(definition,site)).ok_or_else(||bad("actual Call has no original full-ordinal inference result"))?;
        let mut recipes=Vec::new();for argument in arguments{recipes.push(self.reify_argument(module,argument,env,b)?);}Ok((target,recipes))
    }
    fn scope(&self,id:SemanticDefinitionId,env:&EvalEnv,b:&mut Budget)->Check<SemanticScopeRef>{b.spend(env.lexical.len())?;env.lexical.iter().position(|frame|frame.definition==id).map(|depth|SemanticScopeRef::Ancestor(depth as u32)).ok_or_else(||bad("original symbolic term escapes the actual real lexical scope"))}
    pub(super) fn reify_type(&self,module:&TypedModule,ty:&Ty,env:&EvalEnv,b:&mut Budget)->Check<SemanticTypeRecipe>{
        self.reify_type_at(module,ty,env,0,b)
    }
    fn reify_type_at(&self,module:&TypedModule,ty:&Ty,env:&EvalEnv,depth:usize,b:&mut Budget)->Check<SemanticTypeRecipe>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original term instantiation depth exceeded"));}
        let ty=self.resolved(ty,b)?;
        Ok(match ty{
            Ty::Atom(ty)=>SemanticTypeRecipe::Atom(ty),
            Ty::SealedError(kind)=>SemanticTypeRecipe::Exact(Box::new(SemanticExactTypeRecipe{ty:kind.ty(),life:empty_life(),classes:vec![]})),
            Ty::Variable(definition,ordinal)=>SemanticTypeRecipe::Formal(SemanticFormalRef{scope:self.scope(definition,env,b)?,ordinal}),
            Ty::SelfType(definition)=>SemanticTypeRecipe::SelfType{scope:self.scope(definition,env,b)?},
            Ty::Nominal(definition,arguments)=>{let mut recipes=Vec::new();for argument in &arguments{recipes.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::Nominal{definition,arguments:recipes}},
            Ty::Tuple(fields)=>{let mut recipes=Vec::new();for field in &fields{recipes.push(self.reify_type_at(module,field,env,depth+1,b)?);}SemanticTypeRecipe::Tuple(recipes)},
            Ty::Address{mutable,life,inner}=>SemanticTypeRecipe::Address{mutable,lifetime:self.reify_outer(module,&life,env,b)?,inner:Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)},
            Ty::Desc{life,inner}=>SemanticTypeRecipe::Desc{lifetime:self.reify_outer(module,&life,env,b)?,inner:Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)},
            Ty::Slice(inner)=>SemanticTypeRecipe::Slice(Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)),
            Ty::Managed(inner)=>SemanticTypeRecipe::Managed(Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)),
            Ty::Carrier{kind,child,retained}=>{let child=Box::new(self.reify_type_at(module,&child,env,depth+1,b)?);match kind{CarrierKind::Mutex=>SemanticTypeRecipe::Mutex(child),CarrierKind::MutexGuard=>SemanticTypeRecipe::MutexGuard{payload:child,retained:Box::new(self.reify_retained(module,&retained,env,b)?)},CarrierKind::Task=>SemanticTypeRecipe::Task{result:child,retained:Box::new(self.reify_retained(module,&retained,env,b)?)},CarrierKind::Registration=>SemanticTypeRecipe::Registration(child),CarrierKind::AsyncRegistration=>SemanticTypeRecipe::AsyncRegistration(child)}},
            Ty::Store(inner)=>SemanticTypeRecipe::OwnedStore(Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)),
            Ty::Array(element,length)=>SemanticTypeRecipe::Array{element:Box::new(self.reify_type_at(module,&element,env,depth+1,b)?),length:Box::new(self.reify_constant(module,&length,env,b)?)},
            Ty::ErrorUnion(error,result)=>SemanticTypeRecipe::ErrorUnion{error:Box::new(self.reify_type_at(module,&error,env,depth+1,b)?),result:Box::new(self.reify_type_at(module,&result,env,depth+1,b)?)},
            Ty::Closure(definition,arguments,environment)=>{let mut checked=Vec::new();let mut sealed=Vec::new();for argument in &arguments{checked.push(self.reify_argument(module,argument,env,b)?);}for argument in &environment{sealed.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::Closure{definition,arguments:checked,environment:sealed}},
            Ty::Infer(_)=>return Err(bad("original inferred type remains unsolved at the executable boundary")),
            Ty::Code{..}|Ty::Function(_)=>return Err(unsupported("original function value requires its complete code-role instantiation",None)),
        })
    }
    fn reify_argument(&self,module:&TypedModule,argument:&Arg,env:&EvalEnv,b:&mut Budget)->Check<SemanticActualRecipe>{match argument{
        Arg::Type(ty)=>Ok(SemanticActualRecipe::Type(self.reify_type(module,ty,env,b)?)),
        Arg::Life(life)=>Ok(SemanticActualRecipe::Life(self.reify_life(module,life,env,b)?)),
        Arg::Const(value)=>Ok(SemanticActualRecipe::Const{carrier:self.reify_type(module,&value.carrier,env,b)?,value:self.reify_constant(module,value,env,b)?}),
    }}
    fn reify_constant(&self,module:&TypedModule,value:&Const,env:&EvalEnv,b:&mut Budget)->Check<SemanticConstRecipe>{b.spend(1)?;Ok(match &value.value{
        ConstValue::Variable(definition,ordinal)=>SemanticConstRecipe::Formal(SemanticFormalRef{scope:self.scope(*definition,env,b)?,ordinal:*ordinal}),
        ConstValue::Value(value)=>SemanticConstRecipe::Value(value.clone()),
        ConstValue::Unary(op,value)=>SemanticConstRecipe::Unary{op:*op,operand_type:Box::new(self.reify_type(module,&value.carrier,env,b)?),value:Box::new(self.reify_constant(module,value,env,b)?)},
        ConstValue::Binary(op,left,right)=>SemanticConstRecipe::Binary{op:*op,operand_type:Box::new(self.reify_type(module,&left.carrier,env,b)?),left:Box::new(self.reify_constant(module,left,env,b)?),right:Box::new(self.reify_constant(module,right,env,b)?)},
        ConstValue::Convert(source)=>SemanticConstRecipe::Convert{source_type:Box::new(self.reify_type(module,&source.carrier,env,b)?),target_type:Box::new(self.reify_type(module,&value.carrier,env,b)?),value:Box::new(self.reify_constant(module,source,env,b)?)},
    })}
    fn reify_outer(&self,module:&TypedModule,life:&Life,env:&EvalEnv,b:&mut Budget)->Check<SemanticOuterLifeRecipe>{if *life==Life::Static{Ok(SemanticOuterLifeRecipe::Static)}else{Ok(SemanticOuterLifeRecipe::Class(self.reify_life(module,life,env,b)?))}}
    pub(super) fn reify_life(&self,module:&TypedModule,life:&Life,env:&EvalEnv,b:&mut Budget)->Check<SemanticLifeRecipe>{let resolved=self.resolved_life(life,b)?;let life=&resolved;b.spend(1)?;Ok(match life{
        Life::Head(definition,ordinal)=>{if env.lexical.first().is_none_or(|frame|frame.definition!=*definition){return Err(bad("original description head was not instantiated at its true call boundary"));}SemanticLifeRecipe::OuterOccurrence{operand:SemanticRecipeOperand::Parameter(*ordinal),path:vec![],mode:StoredBorrowMode::ReadOnlyDescription}},
        Life::Formal(definition,ordinal)=>SemanticLifeRecipe::Formal(SemanticFormalRef{scope:self.scope(*definition,env,b)?,ordinal:*ordinal}),
        Life::Implicit(definition,ordinal)=>SemanticLifeRecipe::Implicit{scope:self.scope(*definition,env,b)?,ordinal:*ordinal},
        Life::Invocation(token,index)=>{
            let definition=env.lexical.first().ok_or_else(||bad("actual original Invocation has no frame"))?.definition;
            let prototype=self.prototypes.get(&definition).ok_or_else(||bad("actual original Invocation has no original signature"))?;
            if !prototype.environment.invocation.contains(&Life::Invocation(*token,*index)){return Err(bad("original invocation class was not substituted at its real call boundary"));}
            SemanticLifeRecipe::Invocation(*index)
        },
        Life::Bound(token,index)=>{let depth=env.bounds.iter().position(|frame|frame.token==*token).ok_or_else(||bad("original quantified term escapes its exact bound frame"))?;SemanticLifeRecipe::Bound{depth:depth as u32,index:*index}},
        Life::Local(definition,node)=>{
            let original=module.semantic.as_ref().and_then(|program|program.definitions.iter().find(|row|row.id==*definition)).ok_or_else(||bad("original borrowed local definition absent"))?;
            let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&original.body else{return Err(bad("original borrowed local lacks body"));};
            let mut ordinals=BTreeSet::new();for row in site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(*node)){if let Some(site)=sites.iter().find(|site|site.id==row.site){for implicit in &original.implicit_lifetimes{b.spend(implicit.occurrence_path.len().saturating_add(1))?;if implicit.occurrence_path==site.occurrence_path{ordinals.insert(implicit.ordinal);}}}}
            if ordinals.len()!=1{return Err(unsupported("original local borrow needs its exact implicit head occurrence",None));}
            SemanticLifeRecipe::Implicit{scope:self.scope(*definition,env,b)?,ordinal:*ordinals.iter().next().unwrap()}
        },
        Life::Static=>return Err(bad("Static cannot be represented as a variable lifetime argument")),
        Life::Infer(_)=>return Err(bad("original call lifetime remains unresolved at the Typed boundary")),
    })}
}
