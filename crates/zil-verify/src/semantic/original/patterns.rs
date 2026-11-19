use super::*;

#[derive(Clone)]
pub(super) struct PatternOrigin {
    node: NodeId,
    path: Vec<u32>,
    retained: Option<Place>,
    record: bool,
}

pub(super) struct PatternBorrow {
    statement: NodeId,
    path: Vec<u32>,
    ordinal: u32,
    mutable: bool,
}

pub(in crate::semantic) struct PatternSource {
    pub subject: NodeId,
    pub projections: Vec<LifeTypeProjection>,
    pub name: String,
    pub span: Span,
    pub mutable: bool,
    pub reference: Option<Span>,
    pub preparation: Option<(NodeId, Span)>,
    pub record: bool,
    pub package: bool,
}

impl CheckedOriginal {
    pub(in crate::semantic) fn package_pattern_root(
        &self, module: &TypedModule, definition: SemanticDefinitionId, subject: NodeId, b: &mut Budget,
    ) -> Check<Option<(NodeId, intrinsics::BufferRoot, Vec<intrinsics::AddressProjection>)>> {
        b.spend(1)?;
        if !self.value_places.contains_key(&(definition, subject)) { return Ok(None); }
        let (root, path) = self.address_place(module, definition, subject, b)?;
        let intrinsics::AddressRoot::Place(root) = root else { return Ok(None); };
        let (binder, fields) = self.buffer_place(definition, root, b)?;
        if !fields.is_empty() { return Err(bad("Package pattern has no complete lexical root")); }
        Ok(Some((root, binder, path)))
    }

    #[allow(clippy::too_many_arguments)]
    fn pattern(&mut self,module:&TypedModule,context:&CheckedContext,pattern:&SemanticOriginalPattern,subject:&Ty,mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<bool> {
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            if let Some(origin)=&self.pattern_origin{b.spend(origin.path.len().saturating_add(1))?;self.pattern_types.insert((env.current()?,origin.node,origin.path.clone()),subject.clone());}
            let irrefutable = match &pattern.kind {
                CorePatternKind::Wildcard=>true,
                CorePatternKind::Name(name)=>{
                    if let Some(place) = self.pattern_origin.as_ref().and_then(|origin| origin.retained.clone()) {
                        self.read_original(module, context, &place, subject, env, true, active, b)?;
                    }
                    let definition = env.current()?;
                    let identity = Binder::Pattern { definition, token: active.fresh(b)? };
                    if let Some(origin) = &self.pattern_origin {
                        b.spend(origin.path.len().saturating_mul(2).saturating_add(1))?;
                        self.pattern_types.insert((definition, origin.node, origin.path.clone()), subject.clone());
                        self.pattern_bindings.insert((definition, origin.node, origin.path.clone()), identity.clone());
                        if origin.record {
                            b.spend(origin.path.len().saturating_add(1).saturating_add(
                                self.record_pattern_bindings.len().saturating_add(1).ilog2() as usize))?;
                            self.record_pattern_bindings.entry(definition).or_default().push((origin.node, origin.path.clone()));
                        }
                    }
                    let binding = Binding { ty: subject.clone(), mutable, identity };
                    self.declare(env, &binding, b)?;
                    bindings.insert(name.clone(), binding);
                    true
                },
                CorePatternKind::Tuple(patterns)=>{let fields=match self.resolved(subject,b)?{Ty::Tuple(fields)=>fields,Ty::Atom(Type::Unit)if patterns.is_empty()=>vec![],_=>return Err(bad("original tuple pattern has non-Tuple subject"))};if fields.len()!=patterns.len(){return Err(bad("original tuple pattern arity differs"));}let mut irrefutable=true;for(index,(pattern,field))in patterns.iter().zip(fields).enumerate(){irrefutable &= self.pattern_child(module,context,index as u32,Some(index.to_string()),false,pattern,&field,mutable,env,bindings,active,b)?;}irrefutable},
                CorePatternKind::Reference { mutable: exclusive, pattern: inner } => {
                    if !matches!(inner.kind, CorePatternKind::Name(_) | CorePatternKind::Wildcard) {
                        return Err(bad("original reference pattern requires a name or discard"));
                    }
                    if *exclusive && matches!(self.resolved(subject,b)?, Ty::Managed(_)) {
                        return Err(bad("ordinary original resource head reference must remain shared"));
                    }
                    let origin = self.pattern_origin.as_ref().ok_or_else(|| bad("reference pattern has no original binding occurrence"))?;
                    let definition = context.definition(module,env.current()?,b)?;
                    b.spend(definition.implicit_lifetimes.len().saturating_mul(origin.path.len().saturating_add(2)))?;
                    let mut matches = definition.implicit_lifetimes.iter().filter(|row|
                        row.occurrence_path.strip_prefix(&[ORIGINAL_PATTERN_BORROW, origin.node.0]) == Some(origin.path.as_slice()));
                    let ordinal = matches.next().ok_or_else(|| bad("reference pattern lacks its exact original lifetime"))?.ordinal;
                    if matches.next().is_some() { return Err(bad("reference pattern repeats its original lifetime")); }
                    if let Some(place) = origin.retained.clone() {
                        self.read_original(module,context,&place,subject,env,false,active,b)?;
                    }
                    if matches!(inner.kind, CorePatternKind::Name(_)) {
                        let origin = self.pattern_origin.as_ref().expect("validated pattern origin");
                        b.spend(origin.path.len().saturating_add(2))?;
                        let mut path = origin.path.clone();
                        path.push(0);
                        let borrow = PatternBorrow { statement: origin.node, path, ordinal, mutable: *exclusive };
                        if let Some(old) = self.pattern_borrows.insert((definition.id, pattern.span), borrow)
                            && (old.ordinal != ordinal || old.mutable != *exclusive)
                        {
                            return Err(bad("reference pattern has ambiguous original borrow occurrence"));
                        }
                    }
                    let address = Ty::Address { mutable: *exclusive, life: Life::Implicit(definition.id,ordinal), inner: Box::new(subject.clone()) };
                    self.pattern_child(module,context,0,None,false,inner,&address,mutable,env,bindings,active,b)?
                },
                CorePatternKind::Record { fields, .. } | CorePatternKind::RecordConstructor { fields, .. } => {
                    if let CorePatternKind::Record { ty, .. } = &pattern.kind {
                        let expected = self.ty(module, context, ty, env, active, b)?;
                        self.unify(subject, &expected, b)?;
                    }
                    let Ty::Nominal(actual, _) = self.resolved(subject, b)? else {
                        return Err(bad("original record pattern subject lacks nominal identity"));
                    };
                    if let CorePatternKind::RecordConstructor { definition: Some(expected), .. } = &pattern.kind
                        && *expected != actual
                    {
                        return Err(bad("original record pattern changes declaration"));
                    }
                    let declaration = context.definition(module, actual, b)?;
                    let SemanticDefinitionBody::Nominal {
                        kind: kind @ (LifeNominalKind::Record | LifeNominalKind::Package),
                        body: SemanticNominalBody::Product { fields: declared }, ..
                    } = &declaration.body else {
                        return Err(bad("original named pattern requires a Record or Package declaration"));
                    };
                    if fields.len() != declared.len() {
                        return Err(bad("original record pattern changes its complete field arity"));
                    }
                    if *kind == LifeNominalKind::Record {
                        self.require_copy(module, context, subject, env, active, b)?;
                    }
                    let mut seen = BTreeSet::new();
                    let mut irrefutable = true;
                    for (index, (name, pattern)) in fields.iter().enumerate() {
                        b.spend(name.len().saturating_add(1).saturating_mul(declared.len().saturating_add(1)))?;
                        if !seen.insert(name) {
                            return Err(bad("duplicate original record pattern field"));
                        }
                        let field = self.field(module, context, subject, name, env, active, b)?;
                        let index = u32::try_from(index).map_err(|_| limit("record pattern occurrence ordinal overflow"))?;
                        irrefutable &= self.pattern_child(module, context, index, Some(name.clone()), true,
                            pattern, &field, mutable, env, bindings, active, b)?;
                    }
                    irrefutable
                },
                CorePatternKind::ErrorArm{is_error,pattern}=>{let Ty::ErrorUnion(error,result)=self.resolved(subject,b)? else{return Err(bad("original ErrorArm subject is not ErrorUnion"));};self.pattern_child(module,context,0,Some(format!("$error:{}", u32::from(*is_error))),false,pattern,if *is_error{&error}else{&result},mutable,env,bindings,active,b)?;false},
                CorePatternKind::Variant{ty,variant,fields}=>{let expected=self.ty(module,context,ty,env,active,b)?;self.unify(subject,&expected,b)?;self.variant_pattern(module,context,subject,variant,fields,mutable,env,bindings,active,b)?},
                CorePatternKind::Constructor{definition,variant,fields}=>{let Ty::Nominal(actual,_)=self.resolved(subject,b)? else{return Err(bad("original variant subject lacks nominal identity"));};if definition.is_some_and(|id|id!=actual){return Err(bad("original variant changes declaration"));}self.variant_pattern(module,context,subject,variant,fields,mutable,env,bindings,active,b)?},
                CorePatternKind::Integer(text)=>{let ty=self.literal_original(module,text,false,Some(subject),b)?;self.unify(subject,&ty,b)?;false},
                CorePatternKind::Float(text)=>{let ty=self.literal_original(module,text,true,Some(subject),b)?;self.unify(subject,&ty,b)?;false},
                CorePatternKind::Bool(_)=>{self.unify(subject,&Ty::Atom(Type::Bool),b)?;false},
                _=>return Err(unsupported("original pattern needs its complete symbolic literal/constructor rule",Some(pattern.span))),
            };Ok(irrefutable)
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn variant_pattern(&mut self,module:&TypedModule,context:&CheckedContext,subject:&Ty,name:&str,patterns:&[SemanticOriginalPattern],mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<bool> {
        let Ty::Nominal(id,arguments)=self.resolved(subject,b)? else{return Err(bad("original variant subject is not nominal"));};
        let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Enum{variants},..}=&definition.body else{return Err(bad("original variant owner is not Enum"));};b.spend(variants.len())?;let (tag, variant)=variants.iter().enumerate().find(|(_,variant)|variant.name==name).ok_or_else(||bad("original enum has no such variant"))?;if variant.fields.len()!=patterns.len(){return Err(bad("original variant pattern arity differs"));}
        let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=arguments;
        let mut irrefutable=variants.len()==1;
        for(index,(pattern,recipe))in patterns.iter().zip(&variant.fields).enumerate(){let ty=self.ty(module,context,recipe,&selected,active,b)?;irrefutable &= self.pattern_child(module,context,index as u32,Some(protocol::owner_variant_field(tag as u32,index)),false,pattern,&ty,mutable,env,bindings,active,b)?;}Ok(irrefutable)
    }
}

impl CheckedOriginal {
    pub(super) fn package_pattern(
        &mut self, module: &TypedModule, context: &CheckedContext,
        pattern: &SemanticOriginalPattern, env: &Env, active: &mut ActiveQueries, b: &mut Budget,
    ) -> Check<bool> {
        let definition = match &pattern.kind {
            CorePatternKind::RecordConstructor { definition, .. } => *definition,
            CorePatternKind::Record { ty, .. } => match self.ty(module, context, ty, env, active, b)? {
                Ty::Nominal(definition, _) => Some(definition),
                _ => None,
            },
            _ => None,
        };
        let Some(definition) = definition else { return Ok(false); };
        Ok(matches!(context.definition(module, definition, b)?.body,
            SemanticDefinitionBody::Nominal { kind: LifeNominalKind::Package, .. }))
    }

    pub(in crate::semantic) fn record_pattern_sources(
        &self, module: &TypedModule, definition: SemanticDefinitionId, b: &mut Budget,
    ) -> Check<Vec<PatternSource>> {
        if self.record_pattern_bindings.is_empty() { return Ok(Vec::new()); }
        b.spend(self.record_pattern_bindings.len().saturating_add(1).ilog2() as usize + 1)?;
        let Some(bindings) = self.record_pattern_bindings.get(&definition) else { return Ok(Vec::new()); };
        b.spend(bindings.len())?;
        bindings.iter().map(|(statement, path)| self.pattern_binding_source(module, definition, *statement, path, b)).collect()
    }

    pub(in crate::semantic) fn pattern_place(&self,module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<PatternSource>{
        let place=self.value_places.get(&(definition,node)).ok_or_else(||bad("pattern observation has no independently checked original Place"))?;
        let Binder::Pattern{definition:owner,..}=&place.binder else{return Err(bad("pattern observation changes its original binder category"));};
        if *owner!=definition{return Err(bad("pattern observation changes its original lexical owner"));}
        let mut selected=None;
        for((scope,statement,path),binder)in &self.pattern_bindings{b.spend(path.len().saturating_add(1))?;if *scope==definition&&binder==&place.binder{if selected.replace((*statement,path.clone())).is_some(){return Err(bad("pattern binder has multiple original binding occurrences"));}}}
        let(statement,path)=selected.ok_or_else(||bad("pattern binder lacks its complete original occurrence"))?;
        self.pattern_binding_source(module,definition,statement,&path,b)
    }

    pub(in crate::semantic) fn pattern_borrow_source(
        &self, module: &TypedModule, definition: SemanticDefinitionId, span: Span, b: &mut Budget,
    ) -> Check<Option<(PatternSource, u32, bool)>> {
        if self.pattern_borrows.is_empty() { return Ok(None); }
        b.spend(self.pattern_borrows.len().saturating_add(1).ilog2() as usize + 1)?;
        let Some(borrow) = self.pattern_borrows.get(&(definition, span)) else { return Ok(None); };
        let source = self.pattern_binding_source(module,definition,borrow.statement,&borrow.path,b)?;
        Ok(Some((source, borrow.ordinal, borrow.mutable)))
    }

    fn pattern_binding_source(&self,module:&TypedModule,definition:SemanticDefinitionId,statement:NodeId,path:&[u32],b:&mut Budget)->Check<PatternSource>{
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("pattern owner has no original body"))?;
        let statement=function.statement(statement,b)?.ok_or_else(||bad("pattern binding statement is absent"))?;
        let(subject,mut pattern,mut consumed,mutable)=match &statement.kind{
            CoreStatementKind::Match{value,arms}=>{let arm=*path.first().ok_or_else(||bad("match binder omits its original arm ordinal"))?;let selected=arms.get(arm as usize).ok_or_else(||bad("match binder arm is absent"))?;(value,&selected.pattern,1usize,false)},
            CoreStatementKind::IfLet{value,pattern,..}|CoreStatementKind::WhileLet{value,pattern,..}=>(value,pattern,0,false),
            CoreStatementKind::TupleBinding{initializer,pattern,mutable,..}=>(initializer,pattern,0,*mutable),
            _=>return Err(bad("pattern binder selects another original statement role")),
        };
        let root_pattern = pattern;
        let binding_subject_span=subject.span;
        let mut subject=subject;let mut projections=Vec::new();let mut reference=None;let mut record=false;let mut package=false;
        loop{
            b.spend(1)?;
            if self.injections.contains_key(&(definition,subject.id)){break;}
            subject=match &subject.kind{
                CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=>inner,
                CoreExprKind::Try(inner)=>{projections.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)));inner},
                CoreExprKind::Unary{op:UnaryOp::Dereference,operand}=>{projections.push(LifeTypeProjection::Pointee);operand},
                _=>break,
            };
        }
        // The actual producer walk runs from the selected payload back to the
        // source. Envelopes precede pattern fields in the forward path.
        projections.reverse();
        while consumed<path.len(){
            b.spend(1)?;let child=path[consumed] as usize;
            let parent=self.pattern_types.get(&(definition,statement.id,path[..consumed].to_vec())).ok_or_else(||bad("pattern projection lacks its independently checked whole parent type"))?;
            pattern=match &pattern.kind{
                CorePatternKind::Reference{pattern: inner,..}if child==0=>{reference=Some(pattern.span);inner},
                CorePatternKind::ErrorArm{is_error,pattern}if child==0=>{projections.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(*is_error)));pattern},
                CorePatternKind::Tuple(fields)=>{projections.push(LifeTypeProjection::Place(PlaceProjection::Field(child as u32)));fields.get(child).ok_or_else(||bad("tuple pattern child is absent"))?},
                CorePatternKind::Variant{variant,fields,..}|CorePatternKind::Constructor{variant,fields,..}=>{
                    let Ty::Nominal(owner,_)=self.resolved(parent,b)? else{return Err(bad("variant pattern parent has no exact original nominal identity"));};
                    let definition=function_definition(module,owner,b)?;let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Enum{variants},..}=&definition.body else{return Err(bad("variant pattern owner is not its original Enum"));};
                    b.spend(variants.len())?;let ordinal=variants.iter().position(|row|row.name==*variant).ok_or_else(||bad("pattern original variant is absent"))?;
                    projections.push(LifeTypeProjection::Place(PlaceProjection::Variant(VariantProjection{variant:ordinal as u32,index:child as u32})));fields.get(child).ok_or_else(||bad("variant pattern child is absent"))?
                },
                CorePatternKind::Record{fields,..}|CorePatternKind::RecordConstructor{fields,..}=>{
                    record=true;
                    let(name,pattern)=fields.get(child).ok_or_else(||bad("record pattern child is absent"))?;
                    let Ty::Nominal(owner,_)=self.resolved(parent,b)? else{return Err(bad("record pattern parent lacks its exact original nominal identity"));};
                    let definition=function_definition(module,owner,b)?;let SemanticDefinitionBody::Nominal{kind,body:SemanticNominalBody::Product{fields},..}=&definition.body else{return Err(bad("record pattern owner is not its original product"));};
                    package |= *kind == LifeNominalKind::Package;
                    b.spend(fields.len().saturating_mul(name.len().saturating_add(1)))?;let ordinal=fields.iter().position(|row|row.name==*name).ok_or_else(||bad("record pattern field is absent"))?;
                    projections.push(LifeTypeProjection::Place(PlaceProjection::Field(ordinal as u32)));pattern
                },
                _=>return Err(unsupported("pattern source requires its complete original physical projection rule",Some(pattern.span))),
            };consumed+=1;
        }
        let CorePatternKind::Name(name)=&pattern.kind else{return Err(bad("pattern source does not end at its actual original value binder"));};
        b.spend(name.len())?;
        let preparation = if record && !package && matches!(statement.kind, CoreStatementKind::TupleBinding { .. })
            && Self::pattern_mode(root_pattern, b)?.is_none()
        {
            Some((statement.id, binding_subject_span))
        } else {
            None
        };
        Ok(PatternSource { subject: subject.id, projections, name: name.clone(), span: pattern.span, mutable, reference, preparation, record, package })
    }
}

fn function_definition<'a>(module:&'a TypedModule,id:SemanticDefinitionId,b:&mut Budget)->Check<&'a SemanticDefinition>{
    let definitions=&module.semantic.as_ref().ok_or_else(||bad("pattern nominal original programme is absent"))?.definitions;b.spend(definitions.len())?;
    let mut rows=definitions.iter().filter(|row|row.id==id);let row=rows.next().ok_or_else(||bad("pattern nominal original owner is absent"))?;if rows.next().is_some(){return Err(bad("pattern nominal original owner is duplicated"));}Ok(row)
}

impl CheckedOriginal {
    pub(super) fn pattern_mode(pattern: &SemanticOriginalPattern, b: &mut Budget) -> Check<Option<bool>> {
        original_pattern_reference_mode(pattern, 0, &mut |depth| {
            b.spend(1)?;
            if depth > b.limits.max_call_depth { return Err(limit("original reference pattern exceeds its finite depth")); }
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern_subject(
        &mut self, module: &TypedModule, context: &CheckedContext, source: &SemanticOriginalExpr,
        expected: Option<&Ty>, mode: Option<bool>, package: bool, env: &Env, bindings: &BTreeMap<String, Binding>,
        active: &mut ActiveQueries, b: &mut Budget,
    ) -> Check<(Ty, Option<Place>)> {
        if mode.is_none() && !package {
            return Ok((self.expression(module,context,source,env,bindings,expected,active,b)?, None));
        }
        let (ty, place) = self.place(module,context,source,env,bindings,active,b)?;
        if let Some(place) = &place {
            if mode == Some(true) { self.mutable_place(module,context,source,env,bindings,active,b)?; }
            self.read_original_parent(module,context,place,&ty,env,active,b)?;
        }
        if let Some(expected) = expected { self.relate_value(&ty,expected,b)?; }
        Ok((ty, place))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern_root(&mut self,module:&TypedModule,context:&CheckedContext,node:NodeId,path:&[u32],pattern:&SemanticOriginalPattern,subject:&Ty,retained:Option<Place>,mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<bool> {
        let mut pending=vec![pattern];let mut names=BTreeSet::new();
        while let Some(pattern)=pending.pop(){b.spend(1)?;match &pattern.kind{
            CorePatternKind::Name(name)=>{b.spend(name.len())?;if !names.insert(name){return Err(bad("original pattern binds the same value name twice"));}},
            CorePatternKind::Tuple(fields)|CorePatternKind::Variant{fields,..}|CorePatternKind::Constructor{fields,..}=>pending.extend(fields),
            CorePatternKind::Record{fields,..}|CorePatternKind::RecordConstructor{fields,..}=>pending.extend(fields.iter().map(|(_,pattern)|pattern)),
            CorePatternKind::Reference{pattern,..}|CorePatternKind::ErrorArm{pattern,..}=>pending.push(pattern),_=>{},
        }}
        b.spend(path.len())?;let previous=self.pattern_origin.replace(PatternOrigin { node, path: path.to_vec(), retained, record: false });
        let result=self.pattern(module,context,pattern,subject,mutable,env,bindings,active,b);self.pattern_origin=previous;result
    }
    #[allow(clippy::too_many_arguments)]
    fn pattern_child(&mut self,module:&TypedModule,context:&CheckedContext,index:u32,field:Option<String>,record:bool,pattern:&SemanticOriginalPattern,subject:&Ty,mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<bool> {
        let mut origin=self.pattern_origin.clone().ok_or_else(||bad("original pattern child has no actual binder root"))?;
        b.spend(origin.path.len().saturating_add(1))?;
        origin.path.push(index);
        origin.record |= record;
        origin.retained = match (origin.retained, field) {
            (Some(mut place), Some(field)) => { b.spend(field.len().saturating_add(1))?; place.fields.push(field); Some(place) }
            _ => None,
        };
        let previous=self.pattern_origin.replace(origin);let result=self.pattern(module,context,pattern,subject,mutable,env,bindings,active,b);self.pattern_origin=previous;result
    }
}
