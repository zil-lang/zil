//! Original value identities used at an actual closure creation. Type equality
//! and equal outside lifetime images never choose the captured value.
use super::*;
/// A free value is tied to its lexical root through each enclosing
/// environment. The concrete capture ordinal is proved at each real creator.
#[derive(PartialEq, Eq)]
pub(in crate::semantic) struct CapturedSource {
    pub(in crate::semantic) root: intrinsics::BufferSource,
    pub(in crate::semantic) parents: Vec<SemanticDefinitionId>,
}

impl CapturedSource {
    fn from_binding(original: &CheckedOriginal, module: &TypedModule, mut binder: &Binder, b: &mut Budget) -> Check<Self> {
        let mut parents = Vec::new();
        loop {
            b.spend(1)?;
            let root = match binder {
                Binder::Capture { definition, source } => {
                    if parents.len() >= b.limits.max_call_depth {
                        return Err(limit("nested captured source exceeds the lexical depth budget"));
                    }
                    if parents.len() == parents.capacity() { b.spend(parents.len())?; }
                    parents.push(*definition);
                    binder = source;
                    continue;
                }
                Binder::Local { definition, node, .. } =>
                    intrinsics::BufferSource::Local { definition: *definition, node: *node },
                Binder::Parameter { definition, ordinal } =>
                    intrinsics::BufferSource::Parameter { definition: *definition, ordinal: *ordinal },
                Binder::Pattern { definition, .. } => {
                    let range = original.range_binder(module, *definition, binder, b)?.ok_or_else(||
                        unsupported("captured pattern requires its checked original activation and binding", None))?;
                    intrinsics::BufferSource::Local { definition: *definition, node: range.statement }
                }
            };
            return Ok(Self { root, parents });
        }
    }
}
impl CheckedOriginal {
    pub(super) fn index_capture_paths(&mut self, index: &index::Index<'_>, b: &mut Budget) -> Check<()> {
        for (child, &moved) in &self.creator_moves {
            b.spend(1)?;
            if moved { continue; }
            b.spend(index.definitions.len().max(1).ilog2() as usize + 2)?;
            let function = index.definitions.get(child)
                .ok_or_else(|| bad("capture origin has no checked original declaration"))?;
            if function.definition.implicit_lifetimes.is_empty() { continue; }
            let mut paths = BTreeMap::<Binder, Vec<u32>>::new();
            for site in function.sites {
                b.spend(function.nodes.len().max(1).ilog2() as usize + 2)?;
                let Some(index::Node::Expression(node)) = function.nodes.get(&site.id) else { continue; };
                if !matches!(node.kind, CoreExprKind::Name(_)) { continue; }
                b.spend(self.value_bindings.len().max(1).ilog2() as usize + 2)?;
                let Some(binder @ Binder::Capture { .. }) = self.value_bindings.get(&(*child, node.id)) else { continue; };
                for _ in 0..=paths.len().max(1).ilog2() { binder.charge_clone(b)?; }
                let path = &site.occurrence_path;
                if let Some(prior) = paths.get_mut(binder) {
                    b.spend(path.len().min(prior.len()) + 1)?;
                    if path < prior {
                        b.spend(path.len())?;
                        prior.clone_from(path);
                    }
                } else {
                    for _ in 0..=paths.len().max(1).ilog2() { binder.charge_clone(b)?; }
                    b.spend(path.len() + 1)?;
                    paths.insert(binder.clone(), path.clone());
                }
            }
            if paths.is_empty() { continue; }
            b.spend(self.first_capture_paths.len().max(1).ilog2() as usize + 2)?;
            self.first_capture_paths.insert(*child, paths);
        }
        Ok(())
    }

    pub(in crate::semantic) fn prepared_shared_call_input(
        &self, definition: SemanticDefinitionId, site: SemanticSiteId, b: &mut Budget,
    ) -> Check<Option<NodeId>> {
        if self.prepared_address_inputs.is_empty() { return Ok(None); }
        let grammar = self.source_grammar.as_ref()
            .ok_or_else(|| bad("prepared shared Address has no checked original function"))?;
        let Some(SemanticOriginalNode::Expression(call)) = grammar.site_node(definition, site, b)?
        else { return Ok(None) };
        let syntax = grammar.place_syntax(definition, b)?;
        let index::PlaceOperation::Call { arguments, .. } = &syntax.expression(call, b)?.operation
        else { return Ok(None) };
        let [argument] = arguments.as_ref() else { return Ok(None) };
        b.spend(self.prepared_address_inputs.len().saturating_add(1).ilog2() as usize + 1)?;
        Ok(self.prepared_address_inputs.get(&(definition, *argument)).copied())
    }

    pub(in crate::semantic) fn prepared_move_operand(
        &self, _module: &TypedModule, definition: SemanticDefinitionId, node: NodeId, b: &mut Budget,
    ) -> Check<Option<NodeId>> {
        let grammar = self.source_grammar.as_ref()
            .ok_or_else(|| bad("prepared transfer has no checked original body"))?;
        let syntax = grammar.place_syntax(definition, b)?;
        let Some(source) = syntax.find_expression(node, b)? else { return Ok(None); };
        let index::PlaceOperation::Give(mut operand) = source.operation else { return Ok(None); };
        while let index::PlaceOperation::Group(inner) = syntax.expression(operand, b)?.operation {
            operand = inner;
        }
        Ok(Some(operand))
    }
    pub(in crate::semantic) fn call_input_recipes(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<Vec<Option<(NodeId,SemanticTypeRecipe,Option<intrinsics::BufferSource>)>>>{
        use index::PlaceOperation as Op;
        let grammar = self.source_grammar.as_ref()
            .ok_or_else(|| bad("Call inputs have no checked original function"))?;
        let Some(SemanticOriginalNode::Expression(call)) = grammar.site_node(definition, site, b)? else {
            return Err(bad("Call inputs have no exact original expression occurrence"));
        };
        let syntax = grammar.place_syntax(definition, b)?;
        let Op::Call { callee, arguments } = &syntax.expression(call, b)?.operation else {
            return Err(bad("Call input recipe selects another original operation"));
        };
        // The original Task operation has already resolved its static worker.
        // Only the following arguments become child or constructor operands.
        b.spend(self.intrinsic_calls.len().checked_ilog2().map_or(1,|depth|depth as usize+1))?;
        let offset=usize::from(self.intrinsic_calls.get(&(definition,site)).is_some_and(|operation|operation.worker.is_some()));
        let arguments=arguments.get(offset..).ok_or_else(||bad("Task Call input omits its original worker selector"))?;
        let member=self.trait_calls.get(&(definition,site));
        if !self.call_targets.contains_key(&(definition,site))&&!self.callable_nodes.contains_key(&(definition,call))&&member.is_none(){
            return Err(bad("Call input occurrence has no independently resolved original callee"));
        }
        b.spend(arguments.len())?;
        let receiver=member.is_some_and(|member|member.receiver_mode.is_some());
        b.spend(usize::from(receiver))?;let mut recipes=Vec::with_capacity(arguments.len().saturating_add(usize::from(receiver)));
        let receiver_operand = if receiver {
            // Resolve the same grouped callee and single written application as
            // the checked trait call. Only its member object is a value operand.
            let mut head = &syntax.expression(*callee, b)?.operation;
            while let Op::Group(inner) = head {
                head = &syntax.expression(*inner, b)?.operation;
            }
            if let Op::Apply(inner) = head {
                head = &syntax.expression(*inner, b)?.operation;
            }
            let Op::Member { object, .. } = *head else {
                return Err(bad("original borrowed receiver has no method object"));
            };
            Some(object)
        } else { None };
        for (ordinal,argument) in receiver_operand.into_iter().chain(arguments.iter().copied()).enumerate(){
            let argument=if self.prepared_address_inputs.is_empty(){argument}else{
                b.spend(self.prepared_address_inputs.len().saturating_add(1).ilog2() as usize + 1)?;
                self.prepared_address_inputs.get(&(definition,argument)).copied().unwrap_or(argument)
            };
            let ty=self.original_type(definition,&SemanticOriginalTypeSource::Expression(argument),b)?;
            let mut expression=argument;
            while let Op::Group(inner) | Op::Ascribed(inner) = syntax.expression(expression, b)?.operation {
                expression = inner;
            }
            let ty=self.resolved(&ty,b)?;
            if receiver && ordinal == 0 && !matches!(ty, Ty::Address { .. } | Ty::Desc { .. }) {
                // A fresh borrow of a value gets its head at the actual call.
                recipes.push(None);
                continue;
            }
            let source=if self.temporary_description_region(definition,call,ordinal,b)?.is_some(){
                let mut producer = expression;
                let temporary = loop {
                    match syntax.expression(producer, b)?.operation {
                        Op::Group(inner) | Op::Ascribed(inner) | Op::Sequence(inner) => producer = inner,
                        Op::Call { .. } | Op::RecordConstruction | Op::Give(_) => break Some(producer),
                        _ => break None,
                    }
                };
                let node = self.prepared_source(definition, argument, b)?
                    .or(temporary)
                    .ok_or_else(||bad("temporary description argument has no exact original owning producer"))?;
                if !matches!(ty,Ty::Managed(_)){
                    return Err(bad("temporary description argument no longer carries its exact original owner"));
                }
                Some(intrinsics::BufferSource::TemporaryOwner{definition,node})
            }else if matches!(ty,Ty::Address{..}|Ty::Desc{..}){
                if let Some(source)=self.original_address_source(module,definition,&SemanticOriginalTypeSource::Expression(argument),b)?{Some(source)}
                else if self.value_places.contains_key(&(definition,expression)){Some(intrinsics::BufferSource::Place{definition,node:expression})}
                else{None}
            }else if matches!(ty, Ty::Atom(Type::ForeignHandle))
                && self.foreign_resource_member(syntax, definition, expression, b)?
            {
                Some(intrinsics::BufferSource::Place{definition,node:expression})
            }else if matches!(ty,Ty::Code{kind:SemanticCodeKind::FunctionPointer,..})
                && (self.value_places.contains_key(&(definition,expression))
                    || matches!(syntax.expression(expression, b)?.operation, Op::Index { .. } | Op::Member { .. } | Op::ReceiverMember(_)))
            {
                // A value index may have no ownership Place in original Γ.
                // Its original aggregate and subscript are still exact
                // operands of the checked physical projection.
                Some(intrinsics::BufferSource::Place{definition,node:expression})
            }else{None};
            recipes.push(Some((expression,SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(argument)},source)));
        }
        Ok(recipes)
    }
    fn foreign_resource_member(
        &self, syntax: &index::PlaceSyntax, definition: SemanticDefinitionId,
        mut node: NodeId, b: &mut Budget,
    ) -> Check<bool> {
        use index::PlaceOperation as Op;
        loop {
            node = match syntax.expression(node, b)?.operation {
                Op::Group(inner) | Op::Ascribed(inner) => inner,
                Op::Member { object, .. } => {
                    let parent = self.original_type(definition,
                        &SemanticOriginalTypeSource::Expression(object), b)?;
                    if matches!(self.resolved(&parent, b)?, Ty::Managed(_) | Ty::Desc { .. }) {
                        return Ok(true);
                    }
                    object
                }
                _ => return Ok(false),
            };
        }
    }
    pub(super) fn temporary_description_producer(&self,mut expression:&SemanticOriginalExpr,b:&mut Budget)->Check<Option<NodeId>>{
        while let CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}|CoreExprKind::Sequence{value:inner,..}=&expression.kind{
            b.spend(1)?;
            expression=inner;
        }
        Ok(matches!(expression.kind,CoreExprKind::Call{..}|CoreExprKind::Record{..}|CoreExprKind::RecordConstructor{..}
            |CoreExprKind::Unary{op:UnaryOp::Give,..}).then_some(expression.id))
    }
    pub(super) fn declare_temporary_description(&mut self,owner:SemanticDefinitionId,call:NodeId,ordinal:usize,b:&mut Budget)->Check<Life>{
        self.original_call_site(owner,call,b)?;
        if let Some(existing)=self.temporary_description_region(owner,call,ordinal,b)?{return Ok(existing);}
        let ordinal=u32::try_from(ordinal).map_err(|_|limit("temporary description argument exceeds its full ordinal domain"))?;
        let Life::Infer(id)=self.fresh_life(b)? else{unreachable!()};
        b.spend(4)?;
        self.call_regions.insert(id,(owner,call,vec![5,ordinal],vec![]));
        Ok(Life::CallRegion(id))
    }
    fn temporary_description_region(&self,owner:SemanticDefinitionId,call:NodeId,ordinal:usize,b:&mut Budget)->Check<Option<Life>>{
        let ordinal=u32::try_from(ordinal).map_err(|_|limit("temporary description argument exceeds its full ordinal domain"))?;
        let mut selected=None;
        for (id,(definition,node,role,_)) in &self.call_regions{
            b.spend(1)?;
            if *definition==owner&&*node==call&&role.as_slice()==[5,ordinal]
                && selected.replace(Life::CallRegion(*id)).is_some()
            {
                return Err(bad("temporary description repeats its original full-ordinal call role"));
            }
        }
        Ok(selected)
    }
    pub(in crate::semantic) fn temporary_call_description(
        &self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,ordinal:usize,
        node:NodeId,environment:&EvalEnv,b:&mut Budget,
    )->Check<SemanticLifeRecipe>{
        let index=self.source_index(module,&[definition],b)?;
        let function=index.definitions.get(&definition).ok_or_else(||bad("temporary description lacks its original call body"))?;
        let Some(index::Node::Expression(call))=function.node(site,b)?else{
            return Err(bad("temporary description has no exact original Call occurrence"));
        };
        if self.original_call_site(definition,call.id,b)?.1!=site{
            return Err(bad("temporary description operand selects another original operation"));
        }
        let life=self.temporary_description_region(definition,call.id,ordinal,b)?
            .ok_or_else(||bad("temporary description lacks its exact original call-only lifetime role")
                .with_note(format!("original call {site:?}, argument {ordinal}, producer expression {node:?}")))?;
        self.reify_life(module,&life,environment,b)
    }
    // Select a proved original call by its lexical node or its inferred local
    // binding. Explicit type ascriptions remain separate invariant obligations.
    pub(in crate::semantic) fn call_result_source(&self,module:&TypedModule,selection:&mut SourceType<'_>,environment:&EvalEnv,b:&mut Budget)->Check<Option<(SemanticSiteId,bool,Option<(StoredBorrowMode,SemanticOuterLifeRecipe)>)>>{
        let definition=selection.definition;let source=selection.source;
        if !matches!(source,SemanticOriginalTypeSource::Binding(_)|SemanticOriginalTypeSource::Expression(_)|SemanticOriginalTypeSource::NameAt{..}){return Ok(None);}
        let checked=selection.resolve(self,b)?;
        // These forms already reuse a complete original type argument or have
        // no fresh class construction whose domain could be compressed.
        if matches!(checked,Ty::Atom(_)|Ty::SealedError(_)|Ty::Variable(..)|Ty::SelfType(_)|Ty::Function(_)){return Ok(None);}
        let source_grammar = self.source_grammar.as_ref()
            .ok_or_else(|| bad("original Call result has no complete body index"))?;
        let syntax = source_grammar.place_syntax(definition, b)?;
        let binder=match source{
            SemanticOriginalTypeSource::Binding(node)=>Some(*node),
            SemanticOriginalTypeSource::Expression(node)=>match self.value_bindings.get(&(definition,*node)){Some(Binder::Local{definition:owner,node,..})if *owner==definition=>Some(*node),_=>None},
            SemanticOriginalTypeSource::NameAt{at,name}=>{let(kind,node)=match at{SemanticOriginalNode::Expression(node)=>(false,*node),SemanticOriginalNode::Statement(node)=>(true,*node)};b.spend(name.len())?;match self.name_binders_at.get(&(definition,kind,node,name.clone())){Some(Binder::Local{definition:owner,node,..})if *owner==definition=>Some(*node),_=>None}},
            _=>None,
        };
        let mut node = if let Some(node) = binder {
            let binding = syntax.binding(node, b)?.ok_or_else(|| bad("original Call result binding is absent"))?;
            let Some(initializer) = binding.value.filter(|_| binding.inferred) else { return Ok(None); };
            initializer
        } else if let SemanticOriginalTypeSource::Expression(node) = source {
            *node
        } else { return Ok(None); };
        if self.injections.contains_key(&(definition,node)){return Ok(None);}
        let Some(mut expression) = syntax.find_expression(node, b)? else { return Ok(None); };
        while let index::PlaceOperation::Group(inner) = expression.operation {
            node = inner;
            expression = syntax.expression(node, b)?;
        }
        // A contextual TypeUse observes the injected sum. The source Call
        // remains its sole payload operation and has a different result role.
        if self.injections.contains_key(&(definition,node)){return Ok(None);}
        if !matches!(expression.operation,index::PlaceOperation::Call { .. }){return Ok(None);}
        let result=self.expressions.get(&(definition,node)).ok_or_else(||bad("original Call result lacks its checked effective type"))?;
        // A later Describe occurrence observes the same manager through a new
        // outer head. Only the unchanged whole result role can reuse its Call
        // receipt; all adapted roles retain their own original recipe.
        if !self.same(checked,result,b)?{return Ok(None);}
        let mut selected=None;
        let callable = self.callable_nodes.contains_key(&(definition,node));
        for (_, site) in source_grammar.recipes(definition, SemanticOriginalNode::Expression(node), &index::Role::ALL, b)? {
            b.spend(1)?;
            if (self.call_targets.contains_key(&(definition,site)) || self.trait_calls.contains_key(&(definition,site)) || callable)
                && selected.replace(site).is_some() {
                return Err(bad("original Call result has several source call roles"));
            }
        }
        selected.map(|site|{let dynamic=match self.trait_calls.get(&(definition,site)){Some(call)=>matches!(self.resolved(&call.receiver,b)?,Ty::Dyn(_)),None=>false};
            // Reify only the root adapter. Its whole child domain comes from
            // the completed actual call, not a second nominal construction.
            let outer=match checked{
                Ty::Address{mutable,life,..}=>Some((if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress},self.reify_outer(module,life,environment,b)?)),
                Ty::Desc{life,..}=>Some((StoredBorrowMode::WritableDescription,self.reify_outer(module,life,environment,b)?)),
                _=>None,
            };Ok((site,dynamic,outer))}).transpose()
    }
    pub(in crate::semantic) fn original_address_source(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Option<intrinsics::BufferSource>>{
        let SemanticOriginalTypeSource::Expression(node)=source else{return Ok(None);};
        let Some(operand) = self.callable_borrow_operand(module, definition, *node, b)? else { return Ok(None); };
        let syntax = self.source_grammar.as_ref().ok_or_else(|| bad("original address has no complete body index"))?
            .place_syntax(definition, b)?;
        if !matches!(syntax.expression(operand, b)?.operation,index::PlaceOperation::Name){
            self.address_place(module,definition,operand,b)?;
            return Ok(Some(intrinsics::BufferSource::Address{definition,node:operand}));
        }
        let Some(place)=self.value_places.get(&(definition,operand))else{
            self.address_place(module,definition,operand,b)?;
            return Ok(Some(intrinsics::BufferSource::Address{definition,node:operand}));
        };
        if !place.fields.is_empty()||matches!(place.binder,Binder::Capture{..}|Binder::Pattern{..}){return Ok(Some(intrinsics::BufferSource::Place{definition,node:operand}));}
        Ok(Some(match &place.binder{Binder::Local{definition,node,..}=>intrinsics::BufferSource::Local{definition:*definition,node:*node},Binder::Parameter{definition,ordinal}=>intrinsics::BufferSource::Parameter{definition:*definition,ordinal:*ordinal},_=>return Err(unsupported("original address needs its complete captured/indirect Place boundary",None))}))
    }
    pub(in crate::semantic) fn ghost_capture_source(&self,module:&TypedModule,creator:SemanticDefinitionId,child:SemanticDefinitionId,ordinal:usize,node:NodeId,managed_mode:Option<bool>,b:&mut Budget)->Check<Option<CapturedSource>>{
        let index=self.source_index(module,&[creator,child],b)?;
        let parent=index.definitions.get(&creator).ok_or_else(||bad("logical capture has no original creator body"))?;
        let function=index.definitions.get(&child).ok_or_else(||bad("logical capture has no original child body"))?;
        if function.definition.parent!=Some(creator)||parent.closures.get(&node)!=Some(&child){return Err(bad("logical capture environment belongs to another original creation"));}
        let formal=function.definition.environment.get(ordinal).ok_or_else(||bad("logical capture environment ordinal is absent"))?;
        let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::NameAt{at:SemanticOriginalNode::Expression(at),name}})=&formal.source else{return Err(unsupported("logical value capture needs its exact original parent NameAt source",None));};
        if *at!=node{return Err(bad("logical capture source selects another original creation point"));}
        let binding=self.free_values.get(&child).and_then(|rows|rows.get(name)).ok_or_else(||bad("logical capture name has no checked original creator binding"))?;
        binding.identity.charge_clone(b)?;
        let captured=Binder::Capture{definition:child,source:Box::new(binding.identity.clone())};
        if !self.used.get(&child).is_some_and(|rows|rows.contains(&captured)){return Err(bad("physical logical capture is not a checked original free-value use"));}
        if let Some(owned)=managed_mode
            && self.capture_owns(&captured,b)?!=owned {
            // Equal payload types may identify other lexical candidates. The
            // caller still has to prove the actual creator of this operand.
            return Ok(None);
        }
        CapturedSource::from_binding(self, module, &binding.identity, b).map(Some)
    }
    pub(in crate::semantic) fn buffer_place(&self,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<(intrinsics::BufferRoot,Vec<String>)>{
        b.spend(1)?;let place=self.value_places.get(&(definition,node)).ok_or_else(||bad("original buffer observation has no complete checked Place"))?;
        b.spend(place.fields.iter().fold(2usize,|n,field|n.saturating_add(field.len()).saturating_add(1)))?;
        let root=match &place.binder{Binder::Local{definition:owner,node,..}if *owner==definition=>intrinsics::BufferRoot::Local{node:*node},Binder::Parameter{definition:owner,ordinal}if *owner==definition=>intrinsics::BufferRoot::Parameter{ordinal:*ordinal},Binder::Capture{definition:owner,..}if *owner==definition=>intrinsics::BufferRoot::Capture,Binder::Pattern{definition:owner,..}if *owner==definition=>intrinsics::BufferRoot::Pattern,_=>return Err(bad("original buffer observation uses another source scope or unsupported pattern root"))};
        Ok((root,place.fields.clone()))
    }
    pub(in crate::semantic) fn buffer_capture_source(&self,module:&TypedModule,child:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<CapturedSource>{
        b.spend(1)?;let place=self.value_places.get(&(child,node)).ok_or_else(||bad("captured buffer has no independently checked original Place"))?;
        let Binder::Capture{definition,source}=&place.binder else{return Err(bad("captured buffer selects another original root category"));};
        if *definition!=child||!self.used.get(&child).is_some_and(|used|used.contains(&place.binder)){return Err(bad("captured buffer is not a checked original free-value use"));}
        source.charge_clone(b)?;
        CapturedSource::from_binding(self, module, source, b)
    }
    pub(in crate::semantic) fn closure_creation(
        &self, _module: &TypedModule, selection: &mut SourceType<'_>, b: &mut Budget,
    ) -> Check<Option<NodeId>> {
        let definition=selection.definition; let source=selection.source;
        if !matches!(selection.resolve(self,b)?, Ty::Closure(..)) {
            return Ok(None);
        }
        let grammar = self.source_grammar.as_ref()
            .ok_or_else(|| bad("closure source has no original function"))?;
        let closures = grammar.closure_sites(definition, b)?;
        let search = closures.len().checked_ilog2().map_or(1, |depth| depth as usize + 2);
        let binding = match source {
            SemanticOriginalTypeSource::Binding(node) => *node,
            SemanticOriginalTypeSource::Expression(node) => {
                b.spend(search)?;
                if closures.contains_key(node) { return Ok(Some(*node)); }
                b.spend(self.value_bindings.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
                match self.value_bindings.get(&(definition, *node)) {
                    Some(Binder::Local { definition: owner, node, .. }) if *owner == definition => *node,
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        let syntax = grammar.place_syntax(definition, b)?;
        let initializer = syntax.binding(binding, b)?
            .ok_or_else(|| bad("original closure source binding is absent"))?.value
            .ok_or_else(|| unsupported("closure source needs its complete original initializer projection", None))?;
        b.spend(search)?;
        Ok(closures.contains_key(&initializer).then_some(initializer))
    }
    pub(in crate::semantic) fn implicit_capture_binding(&self,module:&TypedModule,child:SemanticDefinitionId,ordinal:u32,b:&mut Budget)->Check<CapturedSource> {
        if self.creator_moves.get(&child)!=Some(&false){return Err(bad("implicit captured head does not have its original default-borrow creation mode"));}
        let index=self.source_index(module,&[child],b)?;let function=index.definitions.get(&child).ok_or_else(||bad("implicit capture has no original child"))?;
        let implicit=function.definition.implicit_lifetimes.iter().find(|row|row.ordinal==ordinal).ok_or_else(||bad("implicit capture has no original lifetime occurrence"))?;
        if let Some((creator, field)) = implicit.forwarded_input() {
            let (_, name) = function.definition.environment.get(field as usize)
                .and_then(|formal| formal.source.lexical_input())
                .ok_or_else(|| bad("forwarded input lacks its original environment formal"))?;
            b.spend(function.closures.len().max(1).ilog2() as usize + 2)?;
            let nested = function.closures.get(&creator)
                .ok_or_else(|| bad("forwarded input has no original nested constructor"))?;
            let outer = self.free_values.get(&child).and_then(|rows| rows.get(name))
                .ok_or_else(|| bad("forwarded input has no lexical source at its creator"))?;
            outer.identity.charge_clone(b)?;
            let captured = Binder::Capture { definition: child, source: Box::new(outer.identity.clone()) };
            if self.capture_owns(&captured, b)? { return Err(bad("forwarded borrowed input selects an owned capture")); }
            for _ in 0..=self.forwarded_creators.len().max(1).ilog2() { captured.charge_clone(b)?; }
            if self.forwarded_creators.get(&(child, captured.clone())) != Some(&creator) ||
                self.first_capture_paths.get(&child).is_some_and(|paths| paths.contains_key(&captured)) {
                return Err(bad("forwarded input differs from its original first constructor use"));
            }
            let input = self.free_values.get(nested).and_then(|rows| rows.get(name))
                .ok_or_else(|| bad("nested creator lacks its actual lexical input"))?;
            if input.identity != captured {
                return Err(bad("nested creator substitutes a different lexical input"));
            }
            return CapturedSource::from_binding(self, module, &outer.identity, b);
        }
        let mut selected=None;
        for site in function.sites.iter().filter(|site|site.occurrence_path==implicit.occurrence_path){
            b.spend(1)?;let Some(index::Node::Expression(expression))=function.node(site.id,b)?else{continue;};
            let CoreExprKind::Name(name)=&expression.kind else{return Err(bad("implicit captured head does not select its exact original free Name occurrence"));};
            let binding=self.free_values.get(&child).and_then(|bindings|bindings.get(name)).ok_or_else(||bad("implicit capture Name is not an original lexical free value"))?;
            let captured=Binder::Capture{definition:child,source:Box::new(binding.identity.clone())};
            if self.capture_owns(&captured,b)? { return Err(bad("implicit borrowed capture selects its owned sibling")); }
            if self.value_bindings.get(&(child,expression.id))!=Some(&captured)||!self.used.get(&child).is_some_and(|used|used.contains(&captured)){return Err(bad("implicit capture occurrence does not name its checked original free binder"));}
            // occurrence_path is checked against the original AST by index.
            // Its lexicographic first Name use is a source position, not a
            // minimum arbitrary NodeId or a selected same-typed capture.
            b.spend(self.first_capture_paths.len().max(1).ilog2() as usize + 2)?;
            let paths = self.first_capture_paths.get(&child)
                .ok_or_else(|| bad("implicit capture has no checked free-value source"))?;
            for _ in 0..=paths.len().max(1).ilog2() { captured.charge_clone(b)?; }
            b.spend(implicit.occurrence_path.len() + 1)?;
            if paths.get(&captured) != Some(&implicit.occurrence_path) {
                return Err(bad("implicit capture head is not its original first lexical free-value use"));
            }
            let source = CapturedSource::from_binding(self, module, &binding.identity, b)?;
            if selected.as_ref().is_some_and(|old| old != &source) {
                return Err(bad("implicit capture selects several original source binders"));
            }
            selected = Some(source);
        }
        selected.ok_or_else(||bad("implicit captured head has no exact original first free-value use"))
    }

}
impl CheckedOriginal {
    pub(in crate::semantic) fn static_literal(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<Option<Vec<u8>>>{
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("static literal original body is absent"))?;
        let expression=match function.node(site,b)?{Some(index::Node::Expression(expression))=>expression,Some(index::Node::Statement(statement))=>match &statement.kind{CoreStatementKind::Binding{initializer:Some(expression),..}=>expression,_=>return Ok(None)},None=>return Ok(None)};
        self.checked_static_literal(definition,expression,b)
    }
    pub(in crate::semantic) fn static_call_literals(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,receiver_argument:bool,b:&mut Budget)->Check<Option<(usize,Vec<(usize,Vec<u8>)>)>>{
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("static Call literal has no exact original body"))?;
        let Some(index::Node::Expression(call))=function.node(site,b)?else{return Ok(None);};
        let CoreExprKind::Call{arguments,..}=&call.kind else{return Ok(None);};
        if !self.call_targets.contains_key(&(definition,site))&&!self.trait_calls.contains_key(&(definition,site))&&!self.callable_nodes.contains_key(&(definition,call.id)){return Ok(None);}
        let offset=usize::from(receiver_argument&&self.trait_calls.get(&(definition,site)).is_some_and(|row|row.receiver_mode.is_some()));
        let mut literals=Vec::new();for(ordinal,argument)in arguments.iter().enumerate(){if let Some(bytes)=self.checked_static_literal(definition,argument,b)?{literals.push((ordinal+offset,bytes));}}
        Ok(Some((arguments.len()+offset,literals)))
    }
    fn checked_static_literal(&self,definition:SemanticDefinitionId,mut expression:&SemanticOriginalExpr,b:&mut Budget)->Check<Option<Vec<u8>>>{
        loop{b.spend(1)?;match &expression.kind{CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=>expression=inner,_=>break}}
        let CoreExprKind::String(text)=&expression.kind else{return Ok(None);};
        let ty=if let Some((_,payload))=self.injections.get(&(definition,expression.id)){payload.clone()}else{self.original_type(definition,&SemanticOriginalTypeSource::Expression(expression.id),b)?};
        if !matches!(self.resolved(&ty,b)?,Ty::Address{mutable:false,life:Life::Static,ref inner}if matches!(inner.as_ref(),Ty::Slice(child)if child.as_ref()==&Ty::Atom(Type::U8))){return Err(bad("original static literal changes its complete byte-slice role"));}
        b.spend(text.len().saturating_mul(2))?;
        let invalid=||bad("original static string has an invalid scalar/escape spelling");
        let mut chars=text.strip_prefix('"').and_then(|text|text.strip_suffix('"')).ok_or_else(invalid)?.chars();
        let mut bytes=Vec::new();
        while let Some(mut ch)=chars.next(){
            if ch=='\\'{ch=match chars.next().ok_or_else(invalid)?{
                'n'=>'\n','r'=>'\r','t'=>'\t','0'=>'\0','\\'=>'\\','\''=>'\'','"'=>'"',
                'u'=>{if chars.next()!=Some('{'){return Err(invalid());}let mut digits=0;let mut scalar=0u32;loop{let ch=chars.next().ok_or_else(invalid)?;if ch=='}'{if digits==0{return Err(invalid());}break;}if digits==6||!ch.is_ascii_hexdigit(){return Err(invalid());}scalar=scalar*16+ch.to_digit(16).ok_or_else(invalid)?;digits+=1;}char::from_u32(scalar).ok_or_else(invalid)?},
                _=>return Err(invalid()),
            };}else if matches!(ch,'"'|'\n'|'\r'){return Err(invalid());}
            let mut encoded=[0;4];bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        }
        b.spend(bytes.len())?;Ok(Some(bytes))
    }
}
