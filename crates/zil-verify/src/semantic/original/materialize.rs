//! Instantiate a proved original symbolic term in the actual caller frame.
//! This never infers a source type from the Typed value it is checking.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic) fn intrinsic_recipes(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,env:&EvalEnv,b:&mut Budget)->Check<Vec<SemanticActualRecipe>> {
        self.intrinsic(definition,site,b)?;
        let mut recipes=Vec::new();
        if let Some(actuals)=self.calls.get(&(definition,site)) {
            for(ordinal,value)in actuals.iter().enumerate(){
                b.spend(1)?;
                if let Some(node)=self.intrinsic_type_sources.get(&(definition,site,ordinal as u32)) {
                    recipes.push(SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(*node)}));
                } else {recipes.push(self.reify_argument(module,value,env,b)?);}
            }
        }
        Ok(recipes)
    }
    pub(in crate::semantic) fn opaque_result_recipe(&self,module:&TypedModule,owner:SemanticDefinitionId,env:&EvalEnv,b:&mut Budget)->Check<SemanticTypeRecipe>{
        if !self.checked.contains(&owner){return Err(bad("opaque result lacks its completed universal original body proof"));}
        let hidden=self.results.get(&owner).ok_or_else(||bad("opaque original result role is absent"))?;
        self.reify_type(module,hidden,env,b)
    }
    pub(in crate::semantic) fn checked_method_body(&self,module:&TypedModule,id:SemanticDefinitionId,b:&mut Budget)->Check<()>{
        let programme=module.semantic.as_ref().ok_or_else(||bad("original method obligation has no complete programme"))?;b.spend(programme.definitions.len().saturating_add(2))?;
        let mut definitions=programme.definitions.iter().filter(|row|row.id==id);let definition=definitions.next().ok_or_else(||bad("selected original method body is absent"))?;
        if definitions.next().is_some()||definition.kind!=SemanticDefinitionKind::Method||!matches!(definition.body,SemanticDefinitionBody::FunctionWithBody{body:SemanticOriginalFunctionBody::Defined(_),..}){return Err(bad("selected method has no unique complete original Defined body"));}
        if !self.checked.contains(&id)||!self.prototypes.contains_key(&id){return Err(bad("selected original method has unfinished universal body obligations"));}
        // This container is installed in CheckedContext only after all use,
        // cleanup, effects and protocol obligations have completed. It is not
        // an assertion about a DynContract or a runtime entry CFG.
        Ok(())
    }
    pub(in crate::semantic) fn callable_source(&self,_module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<NodeId>{b.spend(1)?;self.callable_nodes.get(&(definition,node)).copied().ok_or_else(||bad("actual callable has no independently checked original callee expression"))}

    pub(in crate::semantic) fn indirect_signature_recipe(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,environment:&EvalEnv,b:&mut Budget)->Check<SemanticSignatureRecipe>{
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("indirect call has no complete original function"))?;
        let Some(index::Node::Expression(call))=function.nodes.get(&site)else{return Err(bad("indirect call source is not an original expression"));};
        let source=self.callable_source(module,definition,call.id,b)?;
        let ty=self.original_type(definition,&SemanticOriginalTypeSource::Expression(source),b)?;
        let SemanticTypeRecipe::Code{kind,signature}=self.reify_type(module,&ty,environment,b)? else{return Err(bad("indirect call callee lacks its original complete code scheme"));};
        if kind==SemanticCodeKind::ForeignCallback{return Err(bad("ordinary indirect call cannot select a foreign callback ABI"));}Ok(*signature)
    }
    pub(in crate::semantic) fn type_recipe(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,env:&EvalEnv,b:&mut Budget)->Check<SemanticTypeRecipe>{
        let value=self.original_type(definition,source,b)?;
        if let Ty::Address{mutable,life,inner}=&value{if !matches!(inner.as_ref(),Ty::Atom(_)){
            if let Some(operand)=self.borrow_operand_source(module,definition,source,b)?{
                let child=self.original_type(definition,&SemanticOriginalTypeSource::Expression(operand),b)?;
                if !self.same(inner,&child,b)?{return Err(bad("original borrow changes its exact operand's complete child type"));}
                // The root is this borrow's own occurrence. Its child is an
                // existing whole value, including an already sealed Closure
                // or opaque Call domain; reification must not rebuild its κ
                // from an entry header's unrelated outside image.
                return Ok(SemanticTypeRecipe::Address{mutable:*mutable,lifetime:self.reify_outer(module,life,env,b)?,inner:Box::new(SemanticTypeRecipe::OriginalType{scope:self.scope(definition,env,b)?,source:SemanticOriginalTypeSource::Expression(operand)})});
            }
        }}
        self.reify_type(module,&value,env,b)
    }
    pub(super) fn borrow_operand_source(&self,module:&TypedModule,owner:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Option<NodeId>>{
        if !matches!(source,SemanticOriginalTypeSource::Expression(_)|SemanticOriginalTypeSource::Binding(_)|SemanticOriginalTypeSource::NameAt{..}){return Ok(None);}
        let index=self.source_index(module,&[owner],b)?;let function=index.definitions.get(&owner).ok_or_else(||bad("original borrow source has no complete function"))?;
        let binding=match source{
            SemanticOriginalTypeSource::Binding(node)=>Some(*node),
            SemanticOriginalTypeSource::NameAt{at,name}=>{let(statement,node)=match at{SemanticOriginalNode::Expression(node)=>(false,*node),SemanticOriginalNode::Statement(node)=>(true,*node)};b.spend(name.len())?;match self.name_binders_at.get(&(owner,statement,node,name.clone())){Some(Binder::Local{definition,node,..})if *definition==owner=>Some(*node),_=>return Ok(None)}},
            SemanticOriginalTypeSource::Expression(_)=>None,
            _=>return Ok(None),
        };
        let initializer=|node|->Check<Option<&SemanticOriginalExpr>>{let Some(statement)=function.bindings.get(&node)else{return Ok(function.expressions.get(&node).copied());};Ok(match &statement.kind{CoreStatementKind::Binding{initializer,..}=>initializer.as_ref(),_=>None})};
        let mut expression=if let Some(node)=binding{let Some(expression)=initializer(node)?else{return Ok(None);};expression}else if let SemanticOriginalTypeSource::Expression(node)=source{*function.expressions.get(node).ok_or_else(||bad("original borrow expression is absent"))?}else{return Ok(None);};
        let mut seen=BTreeSet::new();loop{
            b.spend(1)?;if !seen.insert(expression.id){return Err(bad("original borrow source has a cyclic local initializer"));}
            match &expression.kind{
                CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=>expression=inner,
                CoreExprKind::Name(_)=>{let Some(Binder::Local{definition,node,..})=self.value_bindings.get(&(owner,expression.id))else{return Ok(None);};if *definition!=owner{return Ok(None);}let Some(next)=initializer(*node)?else{return Ok(None);};expression=next;},
                CoreExprKind::Unary{op:UnaryOp::Borrow|UnaryOp::BorrowMut,operand}=>return Ok(Some(operand.id)),
                _=>return Ok(None),
            }
        }
    }
    pub(in crate::semantic) fn call_recipes(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,env:&EvalEnv,b:&mut Budget)->Check<(SemanticDefinitionId,Vec<SemanticActualRecipe>)>{
        let target=*self.call_targets.get(&(definition,site)).ok_or_else(||bad("actual static Call has no original symbolically resolved target"))?;
        let arguments=self.calls.get(&(definition,site)).ok_or_else(||bad("actual Call has no original full-ordinal inference result"))?;
        let mut recipes=Vec::new();for argument in arguments{recipes.push(self.reify_argument(module,argument,env,b)?);}Ok((target,recipes))
    }
    pub(in crate::semantic) fn call_type_sources(&self,definition:SemanticDefinitionId,site:SemanticSiteId,ordinal:u32,b:&mut Budget)->Check<Vec<(SemanticTypeRecipe,bool)>>{
        let mut sources=Vec::new();if let Some(nodes)=self.call_type_sources.get(&(definition,site,ordinal)){b.spend(nodes.len())?;for(node,shared_view)in nodes{sources.push((SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(*node)},*shared_view));}}Ok(sources)
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
            Ty::Opaque(definition,arguments)=>{let mut recipes=Vec::new();for argument in &arguments{recipes.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::Opaque{definition,arguments:recipes,hidden:Box::new(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Result})}},
            Ty::Projection{receiver,declaration,arguments,member}=>{let receiver=Box::new(self.reify_type_at(module,&receiver,env,depth+1,b)?);let mut recipes=Vec::new();for argument in &arguments{recipes.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::AssociatedType{receiver,application:Box::new(SemanticTraitRecipe{declaration,arguments:recipes}),member}},
            Ty::Tuple(fields)=>{let mut recipes=Vec::new();for field in &fields{recipes.push(self.reify_type_at(module,field,env,depth+1,b)?);}SemanticTypeRecipe::Tuple(recipes)},
            Ty::Address{mutable,life,inner}=>SemanticTypeRecipe::Address{mutable,lifetime:self.reify_outer(module,&life,env,b)?,inner:Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)},
            Ty::Desc{life,inner}=>SemanticTypeRecipe::Desc{lifetime:self.reify_outer(module,&life,env,b)?,inner:Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)},
            Ty::Slice(inner)=>SemanticTypeRecipe::Slice(Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)),
            Ty::Managed(inner)=>SemanticTypeRecipe::Managed(Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)),
            Ty::Dyn(value)=>SemanticTypeRecipe::Dyn{owned:value.owned,interface:Box::new(self.reify_dyn(module,&value,env,b)?)},
            Ty::Carrier{kind,child,retained}=>{let child=Box::new(self.reify_type_at(module,&child,env,depth+1,b)?);match kind{CarrierKind::Mutex=>SemanticTypeRecipe::Mutex(child),CarrierKind::MutexGuard=>SemanticTypeRecipe::MutexGuard{payload:child,retained:Box::new(self.reify_retained(module,&retained,env,b)?)},CarrierKind::Task=>SemanticTypeRecipe::Task{result:child,retained:Box::new(self.reify_retained(module,&retained,env,b)?)},CarrierKind::Registration=>SemanticTypeRecipe::Registration(child),CarrierKind::AsyncRegistration=>SemanticTypeRecipe::AsyncRegistration(child)}},
            Ty::Store(inner)=>SemanticTypeRecipe::OwnedStore(Box::new(self.reify_type_at(module,&inner,env,depth+1,b)?)),
            Ty::Array(element,length)=>SemanticTypeRecipe::Array{element:Box::new(self.reify_type_at(module,&element,env,depth+1,b)?),length:Box::new(self.reify_constant(module,&length,env,b)?)},
            Ty::ErrorUnion(error,result)=>SemanticTypeRecipe::ErrorUnion{error:Box::new(self.reify_type_at(module,&error,env,depth+1,b)?),result:Box::new(self.reify_type_at(module,&result,env,depth+1,b)?)},
            Ty::Closure(definition,arguments,environment)=>{let mut checked=Vec::new();let mut sealed=Vec::new();for argument in &arguments{checked.push(self.reify_argument(module,argument,env,b)?);}for argument in &environment{sealed.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::Closure{definition,arguments:checked,environment:sealed}},
            Ty::Infer(_)=>return Err(bad("original inferred type remains unsolved at the executable boundary")),
            Ty::Code{kind,parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}=>{
                if parameters.len()!=modes.len(){return Err(bad("original code value omits a complete parameter mode"));}
                let mut values=Vec::new();for(ty,mode)in parameters.iter().zip(modes){values.push(SemanticParameterRecipe{ty:self.reify_type_at(module,ty,env,depth+1,b)?,mode});}
                SemanticTypeRecipe::Code{kind,signature:Box::new(SemanticSignatureRecipe{bound_lifetimes,invocation_lifetimes,parameters:values,result:self.reify_type_at(module,&result,env,depth+1,b)?,result_mode,call_kind})}
            },
            Ty::Function(_)=>return Err(bad("original static function value lacks its explicit complete code coercion")),
        })
    }
    pub(super) fn reify_argument(&self,module:&TypedModule,argument:&Arg,env:&EvalEnv,b:&mut Budget)->Check<SemanticActualRecipe>{match argument{
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
    pub(super) fn reify_outer(&self,module:&TypedModule,life:&Life,env:&EvalEnv,b:&mut Budget)->Check<SemanticOuterLifeRecipe>{let life=self.resolved_life(life,b)?;if life==Life::Static{Ok(SemanticOuterLifeRecipe::Static)}else{Ok(SemanticOuterLifeRecipe::Class(self.reify_life(module,&life,env,b)?))}}
    pub(super) fn reify_life(&self,module:&TypedModule,life:&Life,env:&EvalEnv,b:&mut Budget)->Check<SemanticLifeRecipe>{let resolved=self.resolved_life(life,b)?;let life=&resolved;b.spend(1)?;Ok(match life{
        Life::CallRegion(id)=>{
            let(owner,node,role,_)=self.call_regions.get(id).ok_or_else(||bad("original Call region has no complete source role"))?;
            let original=module.semantic.as_ref().and_then(|program|program.definitions.iter().find(|row|row.id==*owner)).ok_or_else(||bad("original Call region owner is absent"))?;
            let SemanticDefinitionBody::FunctionWithBody{sites,..}=&original.body else{return Err(bad("original Call region owner has no complete body"));};
            let(position,identity)=self.original_call_site(*owner,*node,b)?;
            let site=sites.get(position).filter(|site|site.id==identity).ok_or_else(||bad("original Call region selects another immutable operation"))?;
            b.spend(site.occurrence_path.len().saturating_add(original.implicit_lifetimes.len()).saturating_add(role.len()).saturating_add(1))?;
            let mut path=site.occurrence_path.clone();path.push(u32::MAX);path.extend_from_slice(role);
            let mut rows=original.implicit_lifetimes.iter().filter(|row|row.occurrence_path==path);
            let row=rows.next().ok_or_else(||unsupported("original inferred Call region lacks its independent full-ordinal Source occurrence",None))?;
            if rows.next().is_some(){return Err(bad("original Call region has several full-ordinal lifetime occurrences"));}
            SemanticLifeRecipe::Implicit{scope:self.scope(*owner,env,b)?,ordinal:row.ordinal}
        },
        Life::Head(definition,ordinal)=>{if env.lexical.first().is_none_or(|frame|frame.definition!=*definition){return Err(bad("original description head was not instantiated at its true call boundary"));}SemanticLifeRecipe::OuterOccurrence{operand:SemanticRecipeOperand::Parameter(*ordinal),path:vec![],mode:StoredBorrowMode::WritableDescription}},
        Life::ReceiverRole(..)=>return Err(unsupported("receiver role requires the private exact actual-call value bridge",None)),
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

impl CheckedOriginal {
    pub(in crate::semantic) fn call_region_parameter_recipes(&self,module:&TypedModule,owner:SemanticDefinitionId,site:SemanticSiteId,env:&EvalEnv,b:&mut Budget)->Check<Vec<(u32,SemanticLifeRecipe)>>{
        let index=self.source_index(module,&[owner],b)?;let function=index.definitions.get(&owner).ok_or_else(||bad("Call region parameter owner has no original body"))?;
        let Some(index::Node::Expression(call))=function.nodes.get(&site)else{return Err(bad("Call region parameter recipe is not an original Call"));};
        if self.original_call_site(owner,call.id,b)?.1!=site{return Err(bad("Call region parameter selects another original operation site"));}
        let mut result=Vec::new();if let Some(rows)=self.call_region_parameters.get(&(owner,call.id)){b.spend(rows.len())?;for(ordinal,id)in rows{result.push((*ordinal,self.reify_life(module,&Life::CallRegion(*id),env,b)?));}}Ok(result)
    }
}
