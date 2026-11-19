//! Original place observations retain binder identity and ordered field paths.
//! They do not create physical addresses or executable borrow permissions.
use super::*;
#[derive(Clone)]
pub(in crate::semantic) struct Assignment {
    pub(in crate::semantic) statement: NodeId,
    pub(in crate::semantic) target: NodeId,
    pub(in crate::semantic) value: NodeId,
    pub(in crate::semantic) span: Span,
    pub(in crate::semantic) statement_span: Span,
    pub(in crate::semantic) compound: Option<BinaryOp>,
}
fn nonnegative_index(value:&Constant)->Option<u64>{match value{Constant::I8(value)=>u64::try_from(*value).ok(),Constant::I16(value)=>u64::try_from(*value).ok(),Constant::I32(value)=>u64::try_from(*value).ok(),Constant::I64(value)|Constant::Isize(value)=>u64::try_from(*value).ok(),Constant::U8(value)=>Some(u64::from(*value)),Constant::U16(value)=>Some(u64::from(*value)),Constant::U32(value)=>Some(u64::from(*value)),Constant::U64(value)|Constant::Usize(value)=>Some(*value),_=>None}}
type AddressPath = (intrinsics::AddressRoot, Vec<intrinsics::AddressProjection>);

enum ProjectionSource {
    Place(AddressPath),
    Expression(Span),
}

pub(in crate::semantic) enum ScalarProjection {
    Binding,
    Success(NodeId),
    Other,
}

impl CheckedOriginal {
    pub(in crate::semantic) fn resource_field_read(
        &self,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<bool> {
        if self.resource_field_reads.is_empty() {
            return Ok(false);
        }
        b.spend(self.resource_field_reads.len().checked_ilog2().map_or(1, |n| n as usize + 1))?;
        Ok(self.resource_field_reads.contains(&(definition, node)))
    }

    pub(in crate::semantic) fn injection_arm(
        &self,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<Option<bool>> {
        b.spend(self.injections.len().checked_ilog2().map_or(1, |n| n as usize + 1))?;
        Ok(self.injections.get(&(definition, node)).map(|(failed, _)| *failed))
    }

    pub(in crate::semantic) fn original_stored_member(
        &self, owner: SemanticDefinitionId, site: SemanticSiteId, b: &mut Budget,
    ) -> Check<Option<(NodeId, u32)>> {
        let grammar = self.source_grammar.as_ref().ok_or_else(|| bad("stored reborrow has no checked original grammar"))?;
        let Some(SemanticOriginalNode::Expression(node)) = grammar.site_node(owner, site, b)? else { return Ok(None); };
        let syntax = grammar.place_syntax(owner, b)?;
        if !matches!(syntax.expression(node, b)?.operation, index::PlaceOperation::Member { .. }) { return Ok(None); }
        Ok(grammar.implicit_head(owner, node, b)?.map(|ordinal| (node, ordinal)))
    }

    pub(in crate::semantic) fn affine_field_initializer(
        &self, module: &TypedModule, context: &CheckedContext, definition: SemanticDefinitionId,
        site: SemanticSiteId, b: &mut Budget,
    ) -> Check<Option<NodeId>> {
        let source = self.source_grammar.as_ref().ok_or_else(|| bad("affine field declaration lacks its original grammar"))?;
        let Some(SemanticOriginalNode::Statement(binding)) = source.site_node(definition, site, b)? else { return Ok(None); };
        let syntax = source.place_syntax(definition, b)?;
        let Some(initializer) = syntax.binding(binding, b)?.and_then(|binding| binding.value) else { return Ok(None); };
        let transparent = |mut node, b: &mut Budget| -> Check<NodeId> {
            while let index::PlaceOperation::Group(inner) | index::PlaceOperation::Ascribed(inner)
                | index::PlaceOperation::Give(inner) = syntax.expression(node, b)?.operation {
                b.spend(1)?;
                node = inner;
            }
            Ok(node)
        };
        let expression = transparent(initializer, b)?;
        let index::PlaceOperation::Member { object, .. } = syntax.expression(expression, b)?.operation else { return Ok(None); };
        let owner = transparent(object, b)?;
        if !self.bound_value_name(definition, owner, b)? { return Ok(None); }
        let parent = self.original_type(definition, &SemanticOriginalTypeSource::Expression(owner), b)?;
        let package = match self.resolved(&parent, b)? {
            Ty::Nominal(declaration, _) => matches!(context.definition(module, declaration, b)?.body,
                SemanticDefinitionBody::Nominal { kind: LifeNominalKind::Package, .. }),
            Ty::Atom(Type::Package { .. }) => true,
            _ => false,
        };
        Ok(package.then_some(expression))
    }

    pub(in crate::semantic) fn callable_borrow_operand(
        &self,
        _module: &TypedModule,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<Option<NodeId>> {
        let syntax = self.source_grammar.as_ref()
            .ok_or_else(|| bad("callable borrow lacks its independently checked original body"))?
            .place_syntax(definition, b)?;
        let mut node = node;
        loop {
            let expression = syntax.expression(node, b)?;
            b.spend(1)?;
            match expression.operation {
                index::PlaceOperation::Group(inner) | index::PlaceOperation::Ascribed(inner) => node = inner,
                index::PlaceOperation::Borrow(operand) => return Ok(Some(operand)),
                _ => return Ok(None),
            }
        }
    }

    /// The private callback cell represents this actual mutable source binder,
    /// not an arbitrary one-element array of a coincidentally equal type.
    pub(in crate::semantic) fn persistent_binding(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<(NodeId,NodeId)>{
        if !self.checked.contains(&definition){return Err(bad("persistent callback binder lacks its complete original proof"));}
        let index=self.source_index(module,&[definition],b)?;
        let function=index.definitions.get(&definition).ok_or_else(||bad("persistent callback binder has no original function"))?;
        let Some(index::Node::Statement(statement))=function.node(site,b)?else{return Err(bad("persistent callback cell selects no original declaration"));};
        let CoreStatementKind::Binding{mutable:true,initializer:Some(initializer),..}=&statement.kind else{return Err(bad("persistent callback cell does not represent an initialized mutable binder"));};
        Ok((statement.id,initializer.id))
    }
    /// These source wrappers emit no value operation. Their full types and
    /// ownership effects have already passed the complete original body check;
    /// actual transport must still prove the selected operand's real origin.
    pub(in crate::semantic) fn assignment_value_source(
        &self,
        _module: &TypedModule,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<(NodeId, Option<AddressPath>, bool)> {
        if !self.checked.contains(&definition) {
            return Err(bad("assignment value source has no complete original body proof"));
        }
        let syntax = self.source_grammar.as_ref()
            .ok_or_else(|| bad("assignment value owner has no checked original grammar"))?
            .place_syntax(definition, b)?;
        let mut current = node;
        let mut gives = false;
        loop {
            let expression = syntax.expression(current, b)?;
            b.spend(1)?;
            self.original_type(definition, &SemanticOriginalTypeSource::Expression(current), b)?;
            match expression.operation {
                index::PlaceOperation::Group(value) | index::PlaceOperation::Ascribed(value) => {
                    current = value;
                }
                index::PlaceOperation::Give(value) => {
                    gives = true;
                    current = value;
                }
                _ => break,
            }
        }
        // Indices and reads through an address retain the original expression
        // and parent even when no lexical ownership-map entry exists.
        let path = match self.projection_source(syntax, definition, current, b)? {
            ProjectionSource::Place(path) => Some(path),
            ProjectionSource::Expression(_) => None,
        };
        Ok((current, path, gives))
    }
    pub(in crate::semantic) fn scalar_initializer(&self, owner: SemanticDefinitionId, node: NodeId, b: &mut Budget) -> Check<Option<NodeId>> {
        b.spend(self.value_bindings.len().checked_ilog2().map_or(1, |n| n as usize + 1))?;
        let Some(Binder::Local { definition, node: producer, .. }) = self.value_bindings.get(&(owner, node)) else { return Ok(None); };
        b.spend(self.scalar_initializers.len().checked_ilog2().map_or(1, |n| n as usize + 1))?;
        if *definition != owner || !self.scalar_initializers.contains(&(owner, *producer)) { return Ok(None); }
        let syntax = self.source_grammar.as_ref().ok_or_else(|| bad("prepared scalar has no checked original body"))?.place_syntax(owner, b)?;
        let mut value = *producer;
        loop {
            use index::PlaceOperation as Operation;
            match syntax.expression(value, b)?.operation {
                Operation::Group(inner) | Operation::Ascribed(inner) | Operation::Sequence(inner) => value = inner,
                _ => return Ok(Some(value)),
            }
        }
    }
    pub(in crate::semantic) fn scalar_projection(&self, definition: SemanticDefinitionId, mut node: NodeId, b: &mut Budget) -> Check<ScalarProjection> {
        use index::PlaceOperation as Operation;
        if self.bound_value_name(definition, node, b)? { return Ok(ScalarProjection::Binding); }
        let syntax = self.source_grammar.as_ref().ok_or_else(|| bad("scalar projection has no original body"))?.place_syntax(definition, b)?;
        loop {
            match syntax.expression(node, b)?.operation {
                Operation::Group(inner) | Operation::Ascribed(inner) => node = inner,
                Operation::Try(inner) => return Ok(ScalarProjection::Success(inner)),
                Operation::Member { object, .. } => {
                    // A managed or described product still selects a field of a real owner.
                    // Bind both the SSA read and its complete path back to the original owner.
                    let parent = self.original_type(definition, &SemanticOriginalTypeSource::Expression(object), b)?;
                    let mut logical = &parent;
                    loop {
                        b.spend(1)?;
                        match logical {
                            Ty::Managed(inner) | Ty::Desc { inner, .. } | Ty::Address { inner, .. } => logical = inner,
                            Ty::Nominal(..) => return Ok(ScalarProjection::Binding),
                            Ty::Atom(ty) => {
                                let mut storage = ty;
                                loop {
                                    b.spend(1)?;
                                    match storage {
                                        Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. }
                                        | Type::BorrowedStorage { storage: inner, .. } => storage = inner,
                                        Type::Record { .. } | Type::Package { .. } => return Ok(ScalarProjection::Binding),
                                        _ => return Ok(ScalarProjection::Other),
                                    }
                                }
                            }
                            _ => return Ok(ScalarProjection::Other),
                        }
                    }
                }
                _ => return Ok(ScalarProjection::Other),
            }
        }
    }
    pub(in crate::semantic) fn bound_value_name(&self,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<bool>{
        b.spend(self.value_bindings.len().checked_ilog2().map_or(1,|depth|depth as usize+1))?;
        Ok(self.value_bindings.contains_key(&(definition,node)))
    }
    pub(in crate::semantic) fn assignments(&self,_module:&TypedModule,definition:SemanticDefinitionId,b:&mut Budget)->Check<Vec<Assignment>> {
        if !self.checked.contains(&definition){return Err(bad("assignment source body is not completely checked"));}
        self.source_grammar.as_ref()
            .ok_or_else(|| bad("assignment source has no complete original body"))?
            .place_syntax(definition, b)?.assignments(b)
    }
    pub(in crate::semantic) fn record_spread(&self,module:&TypedModule,definition:SemanticDefinitionId,span:Span,b:&mut Budget)->Check<Option<(NodeId,Vec<(String,NodeId)>)>> {
        if !self.checked.contains(&definition){return Err(bad("record spread lacks its independently checked original body"));}
        let index=self.source_index(module,&[definition],b)?;
        let function=index.definitions.get(&definition).ok_or_else(||bad("record spread original body is absent"))?;
        let mut found=None;
        for &node in function.record_updates(){
            let expression=function.expression(node,b)?.ok_or_else(||bad("original record update expression is absent"))?;
            b.spend(1)?;
            if expression.span!=span{continue;}
            if let CoreExprKind::RecordUpdate{base,fields,..}=&expression.kind {
                if found.is_some(){return Err(bad("actual record spread selects ambiguous original expressions"));}
                b.spend(fields.len())?;
                let mut rows=Vec::new();
                for (name,value) in fields {
                    b.spend(name.len().saturating_add(1))?;
                    rows.push((name.clone(),value.id));
                }
                found=Some((base.id,rows));
            }
        }
        Ok(found)
    }
    pub(in crate::semantic) fn scalar_input<'a>(&'a self,module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,environment:Option<&EvalEnv>,b:&mut Budget)->Check<(&'a SemanticOriginalExpr,Type)>{
        let original = self.original_type(definition, &SemanticOriginalTypeSource::Expression(node), b)?;
        let carrier = match original {
            Ty::Atom(ty) => ty,
            Ty::Variable(owner, _) | Ty::SelfType(owner) => {
                let environment = environment.ok_or_else(|| bad("scalar source input lacks its actual lexical specialization"))?;
                b.spend(environment.lexical.len())?;
                let frame = environment.lexical.iter().find(|frame| frame.definition == owner)
                    .ok_or_else(|| bad("scalar source input has no original declaration frame"))?;
                let actual = match original {
                    Ty::Variable(_, ordinal) => match frame.actuals.get(ordinal as usize) {
                        Some(CheckedActual::Type(ty)) => Some(ty),
                        _ => None,
                    },
                    _ => frame.self_type.as_ref(),
                }.ok_or_else(|| bad("scalar source input has no concrete original Type argument"))?;
                if !actual.ty.is_b0_scalar() || !actual.classes.is_empty() || !actual.statics.is_empty() {
                    return Err(bad("scalar source input changes its complete original specialization"));
                }
                actual.ty.clone()
            }
            _ => return Err(bad("scalar source input lacks its independent original carrier")),
        };
        if !carrier.is_integer()&&!carrier.is_float()&&!matches!(carrier,Type::Unit|Type::Bool){return Err(bad("source input is not an independently checked scalar expression"));}
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("scalar source input original owner is absent"))?;
        let expression=function.expression(node,b)?.ok_or_else(||bad("scalar source input original expression is absent"))?;
        Ok((expression,carrier))
    }
    pub(in crate::semantic) fn tuple_give_operand(
        &self, definition: SemanticDefinitionId, node: NodeId, b: &mut Budget,
    ) -> Check<Option<NodeId>> {
        use index::PlaceOperation as Operation;
        let syntax = self.source_grammar.as_ref()
            .ok_or_else(|| bad("tuple move lacks its independently checked original body"))?
            .place_syntax(definition, b)?;
        let Operation::Give(operand) = syntax.expression(node, b)?.operation else {
            return Ok(None);
        };
        match &syntax.expression(operand, b)?.operation {
            Operation::Member { name, .. } if name.parse::<u32>().is_ok() => Ok(Some(operand)),
            _ => Ok(None),
        }
    }

    pub(in crate::semantic) fn address_place(
        &self,
        _module: &TypedModule,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<AddressPath> {
        let syntax = self.source_grammar.as_ref()
            .ok_or_else(|| bad("indirect address has no complete original owner body"))?
            .place_syntax(definition, b)?;
        match self.projection_source(syntax, definition, node, b)? {
            ProjectionSource::Place(path) => Ok(path),
            ProjectionSource::Expression(span) => Err(unsupported(
                "indirect address has no complete original Place/address-parent projection", Some(span),
            )),
        }
    }

    fn projection_source(
        &self, syntax: &index::PlaceSyntax, definition: SemanticDefinitionId,
        node: NodeId, b: &mut Budget,
    ) -> Check<ProjectionSource> {
        use index::PlaceOperation as Operation;
        use intrinsics::{AddressProjection, AddressRoot};
        let mut current = node;
        let mut projections = Vec::new();
        loop {
            let expression = syntax.expression(current, b)?;
            b.spend(1)?;
            if current != node && matches!(self.resolved(
                self.expressions.get(&(definition, current))
                    .ok_or_else(|| bad("address parent has no independent original type"))?, b,
            )?, Ty::Address { .. }) {
                if self.value_places.contains_key(&(definition, current)) {
                    return Ok(ProjectionSource::Place((AddressRoot::Place(current), projections)));
                }
                return Ok(ProjectionSource::Place((AddressRoot::Value(current), projections)));
            }
            current = match &expression.operation {
                Operation::Group(inner) | Operation::Ascribed(inner) | Operation::Dereference(inner) => *inner,
                Operation::Member { object, name } => {
                    b.spend(name.len())?;
                    projections.push(AddressProjection::Field(name.clone()));
                    *object
                }
                Operation::Index { object, index } => {
                    projections.push(AddressProjection::Index(*index));
                    *object
                }
                Operation::Slice { object, start, end } => {
                    projections.push(AddressProjection::Slice { start: *start, end: *end });
                    *object
                }
                Operation::ReceiverMember(name) => {
                    let ordinal = syntax.receiver.ok_or_else(|| {
                        bad("borrowed receiver field lacks its exact original receiver parameter")
                    })?;
                    b.spend(name.len())?;
                    projections.push(AddressProjection::Field(name.clone()));
                    return Ok(ProjectionSource::Place((AddressRoot::Parameter(ordinal), projections)));
                }
                _ if self.value_places.contains_key(&(definition, current)) => {
                    return Ok(ProjectionSource::Place((AddressRoot::Place(current), projections)));
                }
                _ => return Ok(ProjectionSource::Expression(expression.span)),
            };
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn place(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<(Ty,Option<Place>)>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            self.remember_names(env,SemanticOriginalNode::Expression(expression.id),bindings,b)?;
            let result=match &expression.kind{
                CoreExprKind::Name(name)=>match bindings.get(name){Some(binding)=>{self.value_bindings.insert((env.current()?,expression.id),binding.identity.clone());(binding.ty.clone(),Some(Place{binder:binding.identity.clone(),fields:vec![]}))},None=>match self.bound_name(module,context,name,env,active,b)?{Arg::Type(ty)=>(ty,None),Arg::Const(value)=>(*value.carrier,None),Arg::Life(_)=>return Err(bad("original value place selects a lifetime name"))}},
                CoreExprKind::ReceiverMember(member)=>{
                    let definition=context.definition(module,env.current()?,b)?;let SemanticDefinitionBody::FunctionWithBody{receiver_parameter:Some(ordinal),..}=&definition.body else{return Err(bad("original receiver shorthand has no true source receiver parameter"));};
                    let owner=env.current()?;let receiver=bindings.values().find(|row|matches!(row.identity,Binder::Parameter{definition,ordinal:actual} if definition==owner&&actual==*ordinal)).ok_or_else(||bad("original receiver parameter is absent"))?;
                    self.read_original(module,context,&Place{binder:receiver.identity.clone(),fields:vec![]},&receiver.ty,env,false,active,b)?;
                    let(storage,observed)=self.field_observation_storage(module,context,&receiver.ty,b)?;
                    if !observed{return Err(bad("original receiver shorthand lacks an actual resource or borrowed receiver"));}
                    let field=self.field(module,context,&storage,member,env,active,b)?;
                    self.observe_projection_parent(expression.id,&receiver.ty,expression.id,env,b)?;
                    (field,None)
                },
                CoreExprKind::Group(inner)=>{let result=self.place(module,context,inner,env,bindings,active,b)?;self.observe_projection_parent(inner.id,&result.0,expression.id,env,b)?;result},
                CoreExprKind::Unary{op:UnaryOp::Dereference,operand}=>{
                    let mut source = operand.as_ref();
                    while let CoreExprKind::Group(inner) = &source.kind {
                        b.spend(1)?;
                        source = inner;
                    }
                    let pointer = if matches!(source.kind, CoreExprKind::Member { .. }) {
                        // Dereferencing a stored address observes the field's
                        // live child window before observing its pointee.
                        self.expression(module, context, operand, env, bindings, None, active, b)?
                    } else {
                        let (pointer, place) = self.place(module, context, operand, env, bindings, active, b)?;
                        if let Some(place) = place {
                            self.read_original(module, context, &place, &pointer, env, false, active, b)?;
                        }
                        pointer
                    };
                    self.observe_projection_parent(operand.id,&pointer,expression.id,env,b)?;
                    match self.resolved(&pointer,b)?{Ty::Address{inner,..}=>(*inner,None),_=>return Err(bad("original dereference requires a real Address"))}
                },
                CoreExprKind::Member{object:source_object,member,owner}=>{
                    let (object,mut place)=self.place(module,context,source_object,env,bindings,active,b)?;
                    let(storage,observed)=self.field_observation_storage(module,context,&object,b)?;
                    if observed {
                        let mut parent = source_object.as_ref();
                        while let CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } = &parent.kind {
                            b.spend(1)?;
                            parent = inner;
                        }
                        if matches!(parent.kind, CoreExprKind::Member { .. }) {
                            let definition = env.current()?;
                            let key = (definition, source_object.id);
                            b.spend(self.place_types.len().checked_ilog2().map_or(1, |n| n as usize + 1)
                                .saturating_add(self.value_places.len().checked_ilog2().map_or(1, |n| n as usize + 1)))?;
                            if matches!(self.place_types.get(&key), Some(Ty::Managed(_)))
                                && self.value_places.get(&key).is_some_and(|place| !place.fields.is_empty())
                            {
                                b.spend(self.resource_field_reads.len().checked_ilog2().map_or(1, |n| n as usize + 1))?;
                                self.resource_field_reads.insert((definition, expression.id));
                            }
                        }
                    }
                    if observed{if let Some(place)=&place{self.read_original(module,context,place,&object,env,false,active,b)?;}self.observed_argument(source_object,&object,env,b)?;place=None;}
                    else if let Some(place)=&place{self.read_original_parent(module,context,place,&object,env,active,b)?;}
                    if let Some(owner)=owner{let expected=self.ty(module,context,owner,env,active,b)?;self.unify(&storage,&expected,b)?;}
                    let ty=self.field(module,context,&storage,member,env,active,b)?;
                    self.observe_projection_parent(source_object.id,&object,expression.id,env,b)?;
                    if let Some(place)=&mut place{place.fields.push(member.clone());}(ty,place)
                },
                CoreExprKind::Slice{object,start,end}=>{
                    let(parent,place)=self.place(module,context,object,env,bindings,active,b)?;
                    if let Some(place)=&place{self.read_original(module,context,place,&parent,env,false,active,b)?;}
                    let storage=match self.resolved(&parent,b)?{Ty::Address{inner,..}=>self.resolved(&inner,b)?,ty=>ty};
                    let element=match storage{Ty::Array(element,_)|Ty::Slice(element)=>element,_=>return Err(bad("original slice projection has no complete Array/Slice parent"))};
                    for bound in [start.as_deref(),end.as_deref()].into_iter().flatten(){let ty=self.expression(module,context,bound,env,bindings,None,active,b)?;if !self.integer_carrier(&self.resolved(&ty,b)?){return Err(bad("original slice endpoint lacks an actual integer carrier"));}}
                    // The projection observes the original whole parent root;
                    // range checks and the actual slice loan remain separate.
                    self.observe_projection_parent(object.id,&parent,expression.id,env,b)?;
                    (Ty::Slice(element),place)
                },
                CoreExprKind::Index{object:source_object,index}=>{
                    let (object,mut place)=self.place(module,context,source_object,env,bindings,active,b)?;let mut storage=self.resolved(&object,b)?;
                    if let Some(place)=&place{self.read_original_parent(module,context,place,&object,env,active,b)?;place.binder.charge_clone(b)?;b.spend(place.fields.iter().fold(1usize,|n,field|n.saturating_add(field.len()).saturating_add(1)))?;}
                    let parent=place.clone();
                    if let Ty::Address{inner,..}=storage{storage=*inner;place=None;}
                    // Runtime indices accept every actual integer carrier.
                    // An existing u32/i16 index is not implicitly a usize, and
                    // an unsuffixed value keeps normal original inference.
                    let index_ty=self.expression(module,context,index,env,bindings,None,active,b)?;
                    if !self.integer_carrier(&self.resolved(&index_ty,b)?){return Err(bad("original index is not integer"));}
                    // An index expression may execute a call. Recheck the
                    // already selected parent after it, without replaying it.
                    if let Some(parent)=&parent{self.read_original_parent(module,context,parent,&object,env,active,b)?;}
                    let child=match storage{Ty::Array(child,_)|Ty::Slice(child)=>*child,_=>return Err(bad("original indexed place is not an Array/Slice"))};
                    if let Some(whole)=&mut place{if let CoreExprKind::Integer(text)=&index.kind{let Ty::Atom(carrier)=self.resolved(&index_ty,b)? else{return Err(bad("original constant index has no integer carrier"));};b.spend(text.len())?;let value=scalar::closed(&DynConstPattern::IntegerLiteral(text.clone()),&carrier,module.target.pointer_width,b)?.ok_or_else(||bad("original constant index unresolved"))?;let value=nonnegative_index(&value);if let Some(value)=value{b.spend(24)?;whole.fields.push(format!("[{value}]"));}else{place=None;}}else{place=None;}}
                    if place.is_none(){if let Some(parent)=&parent{self.read_original(module,context,parent,&object,env,false,active,b)?;}}
                    self.observe_projection_parent(source_object.id,&object,expression.id,env,b)?;
                    (child,place)
                },
                _=>(self.expression(module,context,expression,env,bindings,None,active,b)?,None),
            };
            if let Some(place)=&result.1{place.binder.charge_clone(b)?;b.spend(place.fields.iter().fold(2usize,|n,field|n.saturating_add(field.len()).saturating_add(1)))?;self.value_places.insert((env.current()?,expression.id),place.clone());}
            let place_type=self.resolved(&result.0,b)?;
            b.spend(self.place_types.len().checked_ilog2().map_or(1,|n|n as usize+1))?;
            self.place_types.insert((env.current()?,expression.id),place_type);
            self.expressions.insert((env.current()?,expression.id),result.0.clone());Ok(result)
        })
    }
    pub(super) fn take_place(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,place:Option<&Place>,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if let Some(place)=place{return self.read_original(module,context,place,ty,env,true,active,b);}
        if self.is_copy(module,context,ty,env,active,b)?{return Ok(());}
        Err(unsupported("original read cannot move a non-Copy value out of a borrowed/unknown element",None))
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn argument(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,expected:&Ty,mode:PassingMode,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        self.argument_at_call(module,context,expression,expected,mode,None,env,bindings,active,b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn argument_at_call(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,expected:&Ty,mode:PassingMode,call:Option<(NodeId,usize)>,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        if let Ty::Desc{life:expected_life,inner:payload}=self.resolved(expected,b)?{if matches!(mode,PassingMode::Infer|PassingMode::Describe){
            let(actual,place)=self.place(module,context,expression,env,bindings,active,b)?;
            if let Some(place)=&place{self.read_original(module,context,place,&actual,env,false,active,b)?;}
            let effective=match self.resolved(&actual,b)?{
                Ty::Managed(inner)=>{
                    self.unify(&inner,&payload,b)?;
                    let owner=env.current()?;
                    if let Some((call,ordinal))=call && (
                        self.prepared_source(owner, expression.id, b)?.is_some()
                        || place.is_none() && self.temporary_description_producer(expression,b)?.is_some())
                    {
                        let life=self.declare_temporary_description(owner,call,ordinal,b)?;
                        self.relate_outer(&life,&expected_life,b)?;
                        // Keep the initializer's original owning type. Only
                        // this argument occurrence has the call-only description.
                        return Ok(Ty::Desc{life,inner});
                    }
                    let life=Life::Local(owner,expression.id);
                    self.relate_outer(&life,&expected_life,b)?;
                    Ty::Desc{life,inner}
                },
                Ty::Desc{life,inner}=>{self.unify(&inner,&payload,b)?;self.relate_outer(&life,&expected_life,b)?;Ty::Desc{life,inner}},
                _=>return Err(bad("original description argument has no actual owner or description"))
            };
            // Only this actual argument occurrence is a description. The
            // lexical binding and parameter declaration retain their owning
            // category, source identity and complete intrinsic domain.
            b.spend(1)?;self.expressions.insert((env.current()?,expression.id),effective.clone());
            return Ok(effective);
        }}
        let actual=match mode{
            PassingMode::Borrow | PassingMode::BorrowMut
                if matches!(expression.kind, CoreExprKind::Unary { op: UnaryOp::Borrow | UnaryOp::BorrowMut, .. }) =>
                self.expression(module, context, expression, env, bindings, Some(expected), active, b),
            PassingMode::Borrow|PassingMode::BorrowMut|PassingMode::Describe=>{
                let (source,place)=self.place(module,context,expression,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&source,env,false,active,b)?;}let actual=self.contextual_borrow_value(&source,expected,b)?;self.relate_value(&actual,expected,b)?;Ok(actual)
            },
            PassingMode::Copy=>{
                let (source,place)=self.place(module,context,expression,env,bindings,active,b)?;
                let actual=self.contextual_borrow_value(&source,expected,b)?;
                self.require_copy(module,context,&actual,env,active,b)?;
                if let Some(place)=place{self.read_original(module,context,&place,&source,env,false,active,b)?;}self.relate_value(&actual,expected,b)?;Ok(actual)
            },
            PassingMode::Give=>{let actual=self.expression(module,context,expression,env,bindings,Some(expected),active,b)?;if !self.transfer_operand(env.current()?,expression,b)?{return Err(bad("original owning resource argument omits explicit give"));}Ok(actual)},
            _=>self.expression(module,context,expression,env,bindings,Some(expected),active,b),
        }?;
        self.require_input_address_wf(module,context,&actual,expected,active,b)?;
        Ok(actual)
    }
}

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn description_binding(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,expected:Option<&Ty>,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Ty>>{
        // An existing managed name is observed by a description binding.
        // Fresh construction and explicit give remain owners.
        let mut source=expression;
        while let CoreExprKind::Group(inner)=&source.kind{b.spend(1)?;source=inner;}
        let CoreExprKind::Name(name)=&source.kind else{return Ok(None);};
        let Some(binding)=bindings.get(name) else{return Ok(None);};
        let managed=self.resolved(&binding.ty,b)?;
        if !matches!(managed,Ty::Managed(_)) || expected.is_some_and(|ty|!matches!(ty,Ty::Desc{..})){
            return Ok(None);
        }
        let (ty,place)=self.place(module,context,expression,env,bindings,active,b)?;
        let place=place.ok_or_else(||bad("description binding has no actual managed place"))?;
        self.read_original(module,context,&place,&ty,env,false,active,b)?;
        let alias=self.observed_argument(source,&ty,env,b)?;
        if expression.id!=source.id{
            self.expressions.insert((env.current()?,expression.id),alias.clone());
        }
        Ok(Some(alias))
    }

    /// Closed observation schemes use the same argument-category adaptation
    /// as ordinary Describe parameters. Exclusive owner addresses stay intact.
    pub(super) fn observed_argument(&mut self,expression:&SemanticOriginalExpr,value:&Ty,env:&Env,b:&mut Budget)->Check<Ty>{
        let effective=match self.resolved(value,b)?{
            Ty::Managed(inner)=>Ty::Desc{life:Life::Local(env.current()?,expression.id),inner},
            value=>value,
        };
        b.spend(1)?;self.expressions.insert((env.current()?,expression.id),effective.clone());
        Ok(effective)
    }
}

impl CheckedOriginal {
    /// Resource fields are observed through the actual owner, just as through
    /// an explicit description. The returned storage is a nominal selector;
    /// it grants no field move, new manager, copy qualification or access mode.
    fn field_observation_storage(&self,module:&TypedModule,context:&CheckedContext,ty:&Ty,b:&mut Budget)->Check<(Ty,bool)>{
        let mut storage=self.resolved(ty,b)?;let mut observed=false;
        loop{b.spend(1)?;match storage{
            Ty::Address{inner,..}|Ty::Desc{inner,..}=>{storage=self.resolved(&inner,b)?;observed=true;},
            Ty::Managed(inner)=>{
                let payload=self.resolved(&inner,b)?;
                let Ty::Nominal(id,_) = &payload else{return Err(bad("original managed field receiver has no nominal resource declaration"));};
                if !matches!(context.definition(module,*id,b)?.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Resource,..}){return Err(bad("original managed field receiver changes its canonical resource category"));}
                storage=payload;observed=true;
            },
            _=>return Ok((storage,observed)),
        }}
    }
}

impl CheckedOriginal {
    /// Observing an owned field through an existing borrowed product retains
    /// that exact observer's lifetime. This records no movable Place, owner,
    /// permission or equality, and does not change the selected child's type.
    fn observe_projection_parent(&mut self,source:NodeId,source_type:&Ty,projection:NodeId,env:&Env,b:&mut Budget)->Check<()> {
        let owner=env.current()?;
        b.spend(self.borrow_parents.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
        let parent=match self.resolved(source_type,b)? {
            Ty::Address{life,..}|Ty::Desc{life,..}=>Some(life),
            Ty::Managed(_)if source!=projection=>Some(Life::Local(owner,source)),
            _=>self.borrow_parents.get(&(owner,source)).cloned(),
        };
        let Some(parent)=parent else{return Ok(());};
        if parent==Life::Local(owner,projection){return Ok(());}
        if let Some(previous)=self.borrow_parents.get(&(owner,projection)){
            if !self.same_life(previous,&parent,b)?{return Err(bad("original storage projection has conflicting observer source roles"));}
        }else{b.spend(1)?;self.borrow_parents.insert((owner,projection),parent);}
        Ok(())
    }
}

#[cfg(test)]
mod projection_source_tests {
    use super::*;
    #[test]
    fn observed_owner_chain_preserves_the_exact_rigid_source_and_local_owner_boundary(){
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};
        let a=Life::Formal(owner,0);let z=Life::Formal(owner,1);
        let env=Env{lexical:vec![Frame{definition:owner,actuals:vec![Arg::Life(a.clone()),Arg::Life(z.clone())],environment:vec![]}],bounds:vec![],invocation:vec![],self_type:None};
        let owned=Ty::Managed(Box::new(Ty::Store(Box::new(Ty::Variable(owner,2)))));
        let product=Ty::Tuple(vec![owned.clone()]);
        let address=|life|Ty::Address{mutable:false,life,inner:Box::new(product.clone())};
        let mut checked=CheckedOriginal::default();checked.current=Some(owner);let mut b=Budget::new(&Limits::default());
        checked.observe_projection_parent(NodeId(0),&address(a.clone()),NodeId(1),&env,&mut b).unwrap();
        checked.observe_projection_parent(NodeId(1),&product,NodeId(2),&env,&mut b).unwrap();
        checked.observe_projection_parent(NodeId(2),&owned,NodeId(3),&env,&mut b).unwrap();
        checked.observe_projection_parent(NodeId(3),&owned,NodeId(4),&env,&mut b).unwrap();
        assert!(checked.outer_outlives(&Life::Local(owner,NodeId(4)),&a,&mut BTreeSet::new(),&mut b).unwrap());
        assert!(!checked.outer_outlives(&Life::Local(owner,NodeId(4)),&z,&mut BTreeSet::new(),&mut b).unwrap());
        checked.observe_projection_parent(NodeId(10),&owned,NodeId(11),&env,&mut b).unwrap();
        assert!(!checked.outer_outlives(&Life::Local(owner,NodeId(11)),&a,&mut BTreeSet::new(),&mut b).unwrap(),"a local manager cannot acquire an unrelated rigid output lifetime");
        assert!(checked.observe_projection_parent(NodeId(5),&address(z.clone()),NodeId(2),&env,&mut b).is_err(),"one original projection cannot silently replace its observer source");
        assert_eq!(checked.borrow_parents[&(owner,NodeId(2))],a);
        assert!(!checked.same_life(&a,&z,&mut b).unwrap());
    }
}

impl CheckedOriginal {
    /// A contextual shared reborrow consumes a view, while the source
    /// expression and lexical binder retain their actual exclusive category.
    pub(super) fn contextual_borrow_value(&self,actual:&Ty,expected:&Ty,b:&mut Budget)->Check<Ty>{
        let actual=self.resolved(actual,b)?;
        if matches!(self.resolved(expected,b)?,Ty::Address{mutable:false,..}){if let Ty::Address{mutable:true,life,inner}=actual{return Ok(Ty::Address{mutable:false,life,inner});}}
        Ok(actual)
    }
}

#[cfg(test)]
mod shared_reborrow_tests {
    use super::*;
    #[test]
    fn shared_reborrow_changes_only_the_outer_view_and_never_upgrades_or_retags_the_child(){
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};let a=Life::Formal(owner,0);let z=Life::Formal(owner,1);
        let inner=Ty::Nominal(owner,vec![Arg::Life(a.clone()),Arg::Life(z.clone())]);
        let address=|mutable,inner|Ty::Address{mutable,life:a.clone(),inner:Box::new(inner)};
        let exclusive=address(true,inner.clone());let shared=address(false,inner.clone());
        let mut checked=CheckedOriginal::default();checked.current=Some(owner);let mut b=Budget::new(&Limits::default());
        let view=checked.contextual_borrow_value(&exclusive,&shared,&mut b).unwrap();assert_eq!(view,shared);assert_eq!(exclusive,address(true,inner));
        checked.relate_value(&exclusive,&shared,&mut b).unwrap();
        let reverse=checked.contextual_borrow_value(&shared,&exclusive,&mut b).unwrap();assert!(checked.relate_value(&reverse,&exclusive,&mut b).is_err());
        let swapped=address(false,Ty::Nominal(owner,vec![Arg::Life(z.clone()),Arg::Life(a.clone())]));
        assert!(checked.relate_value(&exclusive,&swapped,&mut b).is_err());
        let scalar=Ty::Atom(Type::I32);let nested_exclusive=address(true,address(true,scalar.clone()));let nested_shared=address(false,address(false,scalar));
        assert!(checked.relate_value(&nested_exclusive,&nested_shared,&mut b).is_err(),"root reborrow cannot change a pointee's own address mode");
        assert!(!checked.same_life(&a,&z,&mut b).unwrap());
    }
}
