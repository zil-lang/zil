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
    pub(in crate::semantic) fn checked_method_body(&self,module:&TypedModule,context:&CheckedContext,id:SemanticDefinitionId,b:&mut Budget)->Check<()>{
        let definition=context.definition(module,id,b)?;
        b.spend(2)?;
        if definition.kind!=SemanticDefinitionKind::Method||!matches!(definition.body,SemanticDefinitionBody::FunctionWithBody{body:SemanticOriginalFunctionBody::Defined(_),..}){return Err(bad("selected method has no unique complete original Defined body"));}
        if !self.checked.contains(&id)||!self.prototypes.contains_key(&id){return Err(bad("selected original method has unfinished universal body obligations"));}
        // This container is installed in CheckedContext only after all use,
        // cleanup, effects and protocol obligations have completed. It is not
        // an assertion about a DynContract or a runtime entry CFG.
        Ok(())
    }
    pub(in crate::semantic) fn callable_source(&self,_module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<NodeId>{b.spend(1)?;self.callable_nodes.get(&(definition,node)).copied().ok_or_else(||bad("actual callable has no independently checked original callee expression"))}

    pub(in crate::semantic) fn indirect_signature_recipe(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,environment:&EvalEnv,b:&mut Budget)->Check<SemanticSignatureRecipe>{
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("indirect call has no complete original function"))?;
        let Some(index::Node::Expression(call))=function.node(site,b)?else{return Err(bad("indirect call source is not an original expression"));};
        let source=self.callable_source(module,definition,call.id,b)?;
        let ty=self.original_type(definition,&SemanticOriginalTypeSource::Expression(source),b)?;
        let SemanticTypeRecipe::Code{kind,signature}=self.reify_type(module,&ty,environment,b)? else{return Err(bad("indirect call callee lacks its original complete code scheme"));};
        if kind==SemanticCodeKind::ForeignCallback{return Err(bad("ordinary indirect call cannot select a foreign callback ABI"));}Ok(*signature)
    }
    pub(in crate::semantic) fn type_recipe(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,env:&EvalEnv,b:&mut Budget)->Check<SemanticTypeRecipe>{
        self.type_recipe_for_use(module, SourceType::new(definition, source), env, b)
    }
    pub(in crate::semantic) fn type_recipe_for_use(&self,module:&TypedModule,selection:SourceType<'_>,env:&EvalEnv,b:&mut Budget)->Check<SemanticTypeRecipe>{
        let definition=selection.definition;
        let source=selection.source;
        let mut value=selection.into_value(self,b)?;
        if let SemanticOriginalTypeSource::Expression(node) = source
            && self.prototypes.get(&definition).is_some_and(|prototype| prototype.callable_kind == Some(CallableKind::Fn))
        {
            b.spend(self.value_places.len().saturating_add(1).ilog2() as usize + 1)?;
            if self.value_places.get(&(definition, *node)).is_some_and(|place| {
                place.fields.is_empty() && matches!(place.binder, Binder::Capture { definition: owner, .. } if owner == definition)
            }) && let Ty::Address { mutable, .. } = &mut value {
                *mutable = false;
            }
        }
        if matches!(source,SemanticOriginalTypeSource::Result)&&self.receiver_results.get(&definition)==Some(&true){
            if let Ty::Address{mutable:false,life,..}=&mut value{if self.resolved_life(life,b)?!=Life::Static&&!self.receiver_physical_source(definition,life,b)?{*life=Life::ReceiverHead(definition);}}else{return Err(bad("proved Receiver result loses its shared root Address"));}
        }
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
        self.reify_resolved_at(module,value,env,0,b)
    }
    pub(super) fn borrow_operand_source(&self,_module:&TypedModule,owner:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Option<NodeId>>{
        if !matches!(source,SemanticOriginalTypeSource::Expression(_)|SemanticOriginalTypeSource::Binding(_)|SemanticOriginalTypeSource::NameAt{..}){return Ok(None);}
        use index::PlaceOperation;
        let syntax = self.source_grammar.as_ref()
            .ok_or_else(|| bad("original borrow source has no complete function"))?
            .place_syntax(owner, b)?;
        let binding=match source{
            SemanticOriginalTypeSource::Binding(node)=>Some(*node),
            SemanticOriginalTypeSource::NameAt{at,name}=>{let(statement,node)=match at{SemanticOriginalNode::Expression(node)=>(false,*node),SemanticOriginalNode::Statement(node)=>(true,*node)};b.spend(name.len())?;match self.name_binders_at.get(&(owner,statement,node,name.clone())){Some(Binder::Local{definition,node,..})if *definition==owner=>Some(*node),_=>return Ok(None)}},
            SemanticOriginalTypeSource::Expression(_)=>None,
            _=>return Ok(None),
        };
        let mut node = if let Some(node) = binding {
            let Some(initializer) = syntax.binding_value(node, b)? else { return Ok(None); };
            initializer
        } else if let SemanticOriginalTypeSource::Expression(node) = source {
            *node
        } else { return Ok(None); };
        let mut seen=BTreeSet::new();loop{
            let expression = syntax.expression(node, b)?;
            b.spend(1)?;if !seen.insert(node){return Err(bad("original borrow source has a cyclic local initializer"));}
            match expression.operation{
                PlaceOperation::Group(inner) | PlaceOperation::Ascribed(inner) => node = inner,
                PlaceOperation::Name => {
                    let Some(Binder::Local { definition, node: binder, .. }) = self.value_bindings.get(&(owner, node)) else { return Ok(None); };
                    if *definition != owner { return Ok(None); }
                    let Some(initializer) = syntax.binding_value(*binder, b)? else { return Ok(None); };
                    node = initializer;
                }
                PlaceOperation::Borrow(operand) => return Ok(Some(operand)),
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
    pub(super) fn scope(&self,id:SemanticDefinitionId,env:&EvalEnv,b:&mut Budget)->Check<SemanticScopeRef>{b.spend(env.lexical.len())?;env.lexical.iter().position(|frame|frame.definition==id).map(|depth|SemanticScopeRef::Ancestor(depth as u32)).ok_or_else(||bad("original symbolic term escapes the actual real lexical scope"))}
    pub(super) fn reify_type(&self,module:&TypedModule,ty:&Ty,env:&EvalEnv,b:&mut Budget)->Check<SemanticTypeRecipe>{
        let complete=self.resolved(ty,b)?;
        self.reify_resolved_at(module,complete,env,0,b)
    }
    /// Recursion consumes children of the same completed substitution. A new
    /// symbolic argument or captured term still enters through reify_type.
    fn reify_resolved_at(&self,module:&TypedModule,ty:Ty,env:&EvalEnv,depth:usize,b:&mut Budget)->Check<SemanticTypeRecipe>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original term instantiation depth exceeded"));}
        Ok(match ty{
            Ty::Atom(ty)=>SemanticTypeRecipe::Atom(ty),
            Ty::SealedError(kind)=>SemanticTypeRecipe::Exact(Box::new(SemanticExactTypeRecipe{ty:kind.ty(),life:empty_life(),classes:vec![]})),
            Ty::Variable(definition,ordinal)=>SemanticTypeRecipe::Formal(SemanticFormalRef{scope:self.scope(definition,env,b)?,ordinal}),
            Ty::SelfType(definition)=>SemanticTypeRecipe::SelfType{scope:self.scope(definition,env,b)?},
            Ty::Nominal(definition,arguments)=>{let mut recipes=Vec::new();for argument in &arguments{recipes.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::Nominal{definition,arguments:recipes}},
            Ty::Opaque(definition,arguments)=>{let mut recipes=Vec::new();for argument in &arguments{recipes.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::Opaque{definition,arguments:recipes,hidden:Box::new(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Result})}},
            Ty::Projection{receiver,declaration,arguments,member}=>{let receiver=Box::new(self.reify_resolved_at(module,*receiver,env,depth+1,b)?);let mut recipes=Vec::new();for argument in &arguments{recipes.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::AssociatedType{receiver,application:Box::new(SemanticTraitRecipe{declaration,arguments:recipes}),member}},
            Ty::Tuple(fields)=>{let mut recipes=Vec::new();for field in fields{recipes.push(self.reify_resolved_at(module,field,env,depth+1,b)?);}SemanticTypeRecipe::Tuple(recipes)},
            Ty::Address{mutable,life,inner}=>{
                let captured = match (&life, inner.as_ref()) {
                    (Life::Invocation(..), Ty::Atom(Type::SyncCell(cell))) if !mutable =>
                        self.captured_sync_invocation(&life, *cell, env, b)?,
                    _ => None,
                };
                let lifetime = match captured {
                    Some(capture) => SemanticOuterLifeRecipe::Class(capture),
                    None => self.reify_outer(module,&life,env,b)?,
                };
                SemanticTypeRecipe::Address{mutable,lifetime,inner:Box::new(self.reify_resolved_at(module,*inner,env,depth+1,b)?)}
            },
            Ty::Desc{life,inner}=>SemanticTypeRecipe::Desc{lifetime:self.reify_outer(module,&life,env,b)?,inner:Box::new(self.reify_resolved_at(module,*inner,env,depth+1,b)?)},
            Ty::Slice(inner)=>SemanticTypeRecipe::Slice(Box::new(self.reify_resolved_at(module,*inner,env,depth+1,b)?)),
            Ty::Managed(inner)=>SemanticTypeRecipe::Managed(Box::new(self.reify_resolved_at(module,*inner,env,depth+1,b)?)),
            Ty::Dyn(value)=>SemanticTypeRecipe::Dyn{owned:value.owned,interface:Box::new(self.reify_dyn(module,&value,env,b)?)},
            Ty::Carrier{kind,child,retained}=>{let child=Box::new(self.reify_resolved_at(module,*child,env,depth+1,b)?);match kind{CarrierKind::Mutex=>SemanticTypeRecipe::Mutex(child),CarrierKind::MutexGuard=>SemanticTypeRecipe::MutexGuard{payload:child,retained:Box::new(self.reify_retained(module,&retained,env,b)?)},CarrierKind::Task=>SemanticTypeRecipe::Task{result:child,retained:Box::new(self.reify_retained(module,&retained,env,b)?)},CarrierKind::Registration=>SemanticTypeRecipe::Registration(child),CarrierKind::AsyncRegistration=>SemanticTypeRecipe::AsyncRegistration(child)}},
            Ty::Store(inner)=>SemanticTypeRecipe::OwnedStore(Box::new(self.reify_resolved_at(module,*inner,env,depth+1,b)?)),
            Ty::Array(element,length)=>SemanticTypeRecipe::Array{element:Box::new(self.reify_resolved_at(module,*element,env,depth+1,b)?),length:Box::new(self.reify_constant(module,&length,env,b)?)},
            Ty::ErrorUnion(error,result)=>SemanticTypeRecipe::ErrorUnion{error:Box::new(self.reify_resolved_at(module,*error,env,depth+1,b)?),result:Box::new(self.reify_resolved_at(module,*result,env,depth+1,b)?)},
            Ty::Closure(definition,arguments,environment)=>{let mut checked=Vec::new();let mut sealed=Vec::new();for argument in &arguments{checked.push(self.reify_argument(module,argument,env,b)?);}for argument in &environment{sealed.push(self.reify_argument(module,argument,env,b)?);}SemanticTypeRecipe::Closure{definition,arguments:checked,environment:sealed}},
            Ty::Infer(_)=>return Err(bad("original inferred type remains unsolved at the executable boundary")),
            Ty::Code{kind,parameters,result,modes,result_mode,call_kind,bound_lifetimes,invocation_lifetimes}=>{
                if parameters.len()!=modes.len(){return Err(bad("original code value omits a complete parameter mode"));}
                let frame=Self::code_invocation_frame(&parameters,&result,invocation_lifetimes,b)?;
                let position=|ty:Ty,b:&mut Budget|->Check<SemanticTypeRecipe>{
                    match ty {
                        Ty::Address{mutable,life:Life::Invocation(owner,index),inner}
                            if frame.get(index as usize)==Some(&Life::Invocation(owner,index)) => {
                                Ok(SemanticTypeRecipe::Address{mutable,lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Invocation(index)),inner:Box::new(self.reify_resolved_at(module,*inner,env,depth+1,b)?)})
                            }
                        other=>self.reify_resolved_at(module,other,env,depth+1,b),
                    }
                };
                let mut values=Vec::new();for(ty,mode)in parameters.into_iter().zip(modes){values.push(SemanticParameterRecipe{ty:position(ty,b)?,mode});}
                SemanticTypeRecipe::Code{kind,signature:Box::new(SemanticSignatureRecipe{bound_lifetimes,invocation_lifetimes,parameters:values,result:position(*result,b)?,result_mode,call_kind})}
            },
            Ty::Function(_)=>return Err(bad("original static function value lacks its explicit complete code coercion")),
        })
    }
    fn captured_sync_invocation(&self, life: &Life, cell: SyncCellKind, env: &EvalEnv, b: &mut Budget) -> Check<Option<SemanticLifeRecipe>> {
        let Some(frame) = env.lexical.first() else { return Ok(None); };
        let Some(prototype) = self.prototypes.get(&frame.definition) else { return Ok(None); };
        if prototype.environment.invocation.contains(life) { return Ok(None); }
        let Some(original) = prototype.environment.lexical.first() else { return Ok(None); };
        b.spend(original.environment.len().saturating_add(1))?;
        let mut selected = None;
        for (ordinal, source) in original.environment.iter().enumerate() {
            let Arg::Type(Ty::Address { mutable: false, life: source_life, inner }) = source else { continue; };
            if source_life != life || inner.as_ref() != &Ty::Atom(Type::SyncCell(cell)) { continue; }
            let Some(CheckedActual::Type(actual)) = frame.environment.get(ordinal) else { continue; };
            let Type::Address { mutable: false, inner } = &actual.ty else { continue; };
            if inner.as_ref() != &Type::SyncCell(cell) { continue; }
            if selected.replace(ordinal as u32).is_some() {
                return Err(bad("captured synchronization head has several original invocation sources"));
            }
        }
        // The sealed slot was checked at the real capture creator. Its head
        // is in the creator's invocation domain, beyond the current entry.
        Ok(selected.map(|ordinal| SemanticLifeRecipe::OuterOccurrence {
            operand: SemanticRecipeOperand::Environment { scope: SemanticScopeRef::Ancestor(0), ordinal },
            path: vec![],
            mode: StoredBorrowMode::SharedAddress,
        }))
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
            let constructor=role.first()==Some(&(u32::MAX-1));
            let(position,identity)=if constructor{let rows=self.source_grammar.as_ref().ok_or_else(||bad("original constructor region has no immutable source grammar"))?.recipes(*owner,SemanticOriginalNode::Expression(*node),&[index::Role::Construct],b)?;let [row]=rows.as_slice()else{return Err(bad("original constructor region has no unique true Construct occurrence"));};*row}else{self.original_call_site(*owner,*node,b)?};
            let site=sites.get(position).filter(|site|site.id==identity).ok_or_else(||bad("original Call region selects another immutable operation"))?;
            b.spend(site.occurrence_path.len().saturating_add(original.implicit_lifetimes.len()).saturating_add(role.len()).saturating_add(1))?;
            let mut path=site.occurrence_path.clone();if !constructor{path.push(u32::MAX);}path.extend_from_slice(role);
            let mut rows=original.implicit_lifetimes.iter().filter(|row|row.occurrence_path==path);
            let row=rows.next().ok_or_else(||unsupported("original inferred Call region lacks its independent full-ordinal Source occurrence",None))?;
            if rows.next().is_some(){return Err(bad("original Call region has several full-ordinal lifetime occurrences"));}
            SemanticLifeRecipe::Implicit{scope:self.scope(*owner,env,b)?,ordinal:row.ordinal}
        },
        Life::Head(definition,ordinal)=>{if env.lexical.first().is_none_or(|frame|frame.definition!=*definition){return Err(bad("original description head was not instantiated at its true call boundary"));}SemanticLifeRecipe::OuterOccurrence{operand:SemanticRecipeOperand::Parameter(*ordinal),path:vec![],mode:StoredBorrowMode::WritableDescription}},
        Life::ReceiverHead(owner)=>{if env.lexical.first().is_none_or(|frame|frame.definition!=*owner)||self.receiver_results.get(owner)!=Some(&true){return Err(bad("original Receiver head has no complete same-definition lending proof"));}SemanticLifeRecipe::OuterOccurrence{operand:SemanticRecipeOperand::Receiver,path:vec![],mode:StoredBorrowMode::SharedAddress}},
        Life::ReceiverSignature(_)=>return Err(bad("signature-local Receiver subject escapes its actual callable boundary")),
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
            let grammar = self.source_grammar.as_ref().ok_or_else(|| bad("original borrowed local definition absent"))?;
            let ordinal = grammar.implicit_head(*definition, *node, b)?
                .ok_or_else(|| unsupported("original local borrow needs its exact implicit head occurrence", None))?;
            SemanticLifeRecipe::Implicit{scope:self.scope(*definition,env,b)?,ordinal}
        },
        Life::Static=>return Err(bad("Static cannot be represented as a variable lifetime argument")),
        Life::Infer(_)=>return Err(bad("original call lifetime remains unresolved at the Typed boundary")),
    })}
}

impl CheckedOriginal {
    pub(in crate::semantic) fn elided_call_result_life(&self,module:&TypedModule,owner:SemanticDefinitionId,
        site:SemanticSiteId,env:&EvalEnv,b:&mut Budget)->Check<SemanticLifeRecipe> {
        let grammar = self.source_grammar.as_ref().ok_or_else(||bad("elided result has no original body"))?;
        let Some(SemanticOriginalNode::Expression(node)) = grammar.site_node(owner,site,b)? else {
            return Err(bad("elided result has no original call expression"));
        };
        if self.original_call_site(owner,node,b)?.1 != site { return Err(bad("elided result has another original call")); }
        let actual = self.expressions.get(&(owner,node)).ok_or_else(||bad("elided call result has no original type"))?;
        let Ty::Address { life,.. } = self.resolved(actual,b)? else {return Err(bad("elided call result is not an address"));};
        self.reify_life(module,&life,env,b)
    }
    pub(in crate::semantic) fn call_region_parameter_recipes(&self,module:&TypedModule,owner:SemanticDefinitionId,site:SemanticSiteId,env:&EvalEnv,b:&mut Budget)->Check<Vec<(u32,SemanticLifeRecipe)>>{
        let grammar = self.source_grammar.as_ref()
            .ok_or_else(|| bad("Call region parameter owner has no original body"))?;
        let Some(SemanticOriginalNode::Expression(call)) = grammar.site_node(owner, site, b)? else {
            return Err(bad("Call region parameter recipe is not an original Call"));
        };
        if self.original_call_site(owner,call,b)?.1!=site{return Err(bad("Call region parameter selects another original operation site"));}
        let mut result = Vec::new();
        if let Some(rows) = self.call_region_parameters.get(&(owner, call)) {
            b.spend(rows.len())?;
            for (ordinal, id) in rows {
                let (_, producer, _, _) = self.call_regions.get(id)
                    .ok_or_else(|| bad("original parameter region has no producing call"))?;
                if *producer != call && !self.same_local_call_input(module, owner, site, *ordinal, *producer, *id, b)? {
                    continue;
                }
                result.push((*ordinal, self.reify_life(module, &Life::CallRegion(*id), env, b)?));
            }
        }
        Ok(result)
    }
    /// An invariant argument can carry a region from a prior call. A fresh
    /// borrow then needs the same independently bound local declaration. The
    /// flow checker still binds both operations to their actual SSA parents.
    fn same_local_call_input(&self, module: &TypedModule, owner: SemanticDefinitionId,
        site: SemanticSiteId, ordinal: u32, producer: NodeId, region: u64, b: &mut Budget) -> Check<bool> {
        let prior_site = self.original_call_site(owner, producer, b)?.1;
        if !self.call_targets.contains_key(&(owner, site)) || !self.call_targets.contains_key(&(owner, prior_site)) {
            return Ok(false);
        }
        let index = self.source_index(module, &[owner], b)?;
        let function = index.definitions.get(&owner).ok_or_else(|| bad("inherited argument has no original caller"))?;
        let direct_local = |site, ordinal: u32, b: &mut Budget| -> Check<Option<NodeId>> {
            b.spend(1)?;
            let Some(index::Node::Expression(call)) = function.node(site,b)? else { return Ok(None) };
            let CoreExprKind::Call { arguments, .. } = &call.kind else { return Ok(None) };
            let Some(argument) = arguments.get(ordinal as usize) else { return Ok(None) };
            let CoreExprKind::Unary { op: UnaryOp::Borrow | UnaryOp::BorrowMut, operand } = &argument.kind else { return Ok(None) };
            if !matches!(operand.kind, CoreExprKind::Name(_)) { return Ok(None); }
            Ok(match self.value_bindings.get(&(owner, operand.id)) {
                Some(Binder::Local { definition, node, .. }) if *definition == owner => Some(*node),
                _ => None,
            })
        };
        let Some(local) = direct_local(site, ordinal, b)? else { return Ok(false) };
        if let Some(prior) = self.call_region_parameters.get(&(owner, producer)) {
            b.spend(prior.len())?;
            for (ordinal, id) in prior {
                if *id == region && direct_local(prior_site, *ordinal, b)? == Some(local) { return Ok(true); }
            }
        }
        function.local_covers_call_binding(local, producer, b)
    }
}

#[cfg(test)]
mod resolved_tree_tests {
    use super::*;

    #[test]
    fn reification_traverses_a_solved_tuple_once_and_keeps_scope_and_depth() {
        let (module, _) = crate::tests::baseline();
        let mut original = CheckedOriginal::default();
        original.substitutions.insert(0, Ty::Atom(Type::I32));
        let env = EvalEnv { lexical: vec![], assumptions: vec![], images: vec![], bounds: vec![], invocations: vec![], at: None };
        let nested = |depth:usize, leaf:Ty|(0..depth).fold(leaf, |leaf,_|Ty::Tuple(vec![leaf]));
        let run = |depth| {
            let mut b=Budget::new(&Limits::default());
            let before=b.remaining;
            let mut recipe=original.reify_type(&module, &nested(depth,Ty::Infer(0)), &env,&mut b).unwrap();
            for _ in 0..depth {
                let SemanticTypeRecipe::Tuple(mut fields)=recipe else {panic!("missing original tuple")};
                assert_eq!(fields.len(),1);
                recipe=fields.pop().unwrap();
            }
            assert_eq!(recipe,SemanticTypeRecipe::Atom(Type::I32));
            before-b.remaining
        };
        assert!(run(32)<=run(16)*2+2);
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(1),index:7};
        let mut b=Budget::new(&Limits::default());
        let missing=Ty::Address{mutable:false,life:Life::Formal(owner,3),inner:Box::new(Ty::Atom(Type::I32))};
        let problem=original.reify_type(&module,&nested(4,missing),&env,&mut b).unwrap_err();
        assert!(problem.message.contains("escapes the actual real lexical scope"));
        b.limits.max_call_depth=4;
        assert_eq!(original.reify_type(&module,&nested(5,Ty::Infer(0)),&env,&mut b).unwrap_err().category,zil_base::ResultCategory::ResourceLimit);
    }
}
