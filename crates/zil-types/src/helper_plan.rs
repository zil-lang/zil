//! Scheduling facts describe concrete types, never runtime instance origins.
//! A callee is checked normally before a parent Builder may use its instance.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Node {
    Callable(Instance),
    Store(u32),
}

impl Node {
    fn ready(self, state: &closures::State) -> bool {
        match self {
            Self::Callable(instance) => state.helper_instances.contains_key(&instance),
            Self::Store(index) => state.store_instances[index as usize].function.is_some(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Shape {
    Unknown,
    Complete {
        ty: Rc<Type>,
        work: usize,
    },
    Environment(ClosureId),
    Package(Vec<(String, ClosureId)>),
    Store(Box<Shape>),
    ErrorUnion {
        error: Box<Shape>,
        result: Box<Shape>,
    },
}

impl Shape {
    fn work(&self) -> usize {
        match self {
            Self::Complete { work, .. } => *work,
            Self::Package(fields) => fields.iter().fold(1usize, |n, (name, _)| {
                n.saturating_add(name.len()).saturating_add(1)
            }),
            Self::Store(element) => 1usize.saturating_add(element.work()),
            Self::ErrorUnion { error, result } => 1usize
                .saturating_add(error.work())
                .saturating_add(result.work()),
            _ => 1,
        }
    }
    fn environment(&self) -> Option<ClosureId> {
        match self {
            Self::Complete { ty, .. } => {
                let ty = if let Type::Address { inner, .. } = ty.as_ref() {
                    inner.as_ref()
                } else {
                    ty.as_ref()
                };
                if let Type::Closure(id) = ty {
                    return Some(*id);
                }
                if let Type::Package { fields, .. } = ty {
                    let mut ids = fields.iter().filter_map(|(_, ty)| {
                        if let Type::Closure(id) = ty {
                            Some(*id)
                        } else {
                            None
                        }
                    });
                    let first = ids.next()?;
                    return ids.all(|id| id == first).then_some(first);
                }
                None
            }
            Self::Environment(id) => Some(*id),
            Self::Package(fields) => {
                let id = fields.first()?.1;
                fields.iter().all(|(_, other)| *other == id).then_some(id)
            }
            Self::Unknown | Self::Store(_) | Self::ErrorUnion { .. } => None,
        }
    }
}

pub(crate) fn tree_work(count: usize) -> usize {
    count.checked_ilog2().map_or(1, |n| n as usize + 2)
}

struct Walk<'a, 'b> {
    context: &'a Context,
    bounds: &'a [CoreCallableParameter],
    bindings: Bindings,
    tables: Rc<Tables>,
    scopes: Vec<BTreeMap<String, Shape>>,
    found: BTreeSet<Node>,
    state: &'b mut closures::State,
    limits: &'a Limits,
    store_shapes: bool,
    full_values: bool,
    restart_full: bool,
    owned_factory: bool,
}

impl Walk<'_, '_> {
    fn node(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "helper scheduling source depth exceeds selected bound",
                Some(span),
            ));
        }
        self.state.snapshot(1, span)
    }
    fn clone_shape(&mut self, shape: &Shape, span: Span) -> R<Shape> {
        self.state.snapshot(shape.work(), span)?;
        Ok(shape.clone())
    }
    fn bind(&mut self, name: &str, shape: Shape, span: Span) -> R<()> {
        let scope = self.scopes.last_mut().expect("helper scheduling scope");
        self.state.snapshot(
            tree_work(scope.len())
                .saturating_mul(name.len().saturating_add(1))
                .saturating_add(name.len())
                .saturating_add(1),
            span,
        )?;
        scope.insert(name.to_owned(), shape);
        Ok(())
    }
    fn local(&mut self, name: &str, span: Span) -> R<Option<Shape>> {
        for scope in self.scopes.iter().rev() {
            self.state.snapshot(
                tree_work(scope.len()).saturating_mul(name.len().saturating_add(1)),
                span,
            )?;
            if let Some(shape) = scope.get(name) {
                self.state.snapshot(shape.work(), span)?;
                return Ok(Some(shape.clone()));
            }
        }
        Ok(None)
    }
    fn shape(
        &mut self,
        ty: &Type,
        bounds: &[CoreCallableParameter],
        bindings: Bindings,
        span: Span,
        depth: usize,
    ) -> R<Shape> {
        self.node(depth, span)?;
        if self.full_values {
            let ty = store_templates::resolve_argument(
                ty,
                bounds,
                bindings,
                self.state,
                self.limits,
                span,
            )?;
            let work = inference::type_snapshot_work(&ty, span, self.limits)?;
            self.state.snapshot(work.saturating_add(1), span)?;
            return Ok(Shape::Complete {
                ty: Rc::new(ty),
                work,
            });
        }
        Ok(match ty {
            Type::Named(name) => {
                self.state.snapshot(
                    name.len().saturating_mul(bounds.len().saturating_add(1)),
                    span,
                )?;
                if let Some(bound) = bounds.iter().find(|bound| bound.generic_name == *name) {
                    Shape::Environment(bindings.get(bound.generic_index, span)?)
                } else {
                    Shape::Unknown
                }
            }
            Type::Closure(id) => Shape::Environment(*id),
            Type::Managed(payload) | Type::Desc(payload) if self.store_shapes => {
                if let Type::OwnedStore { element } = payload.as_ref() {
                    let element = self.shape(element, bounds, bindings, span, depth + 1)?;
                    if matches!(element, Shape::Package(_)) {
                        self.state
                            .snapshot(element.work().saturating_add(1), span)?;
                        Shape::Store(Box::new(element))
                    } else {
                        Shape::Unknown
                    }
                } else {
                    Shape::Unknown
                }
            }
            Type::Address { inner, .. } => self.shape(inner, bounds, bindings, span, depth + 1)?,
            Type::Package { fields, .. } => {
                self.state.snapshot(fields.len(), span)?;
                let mut captures = Vec::new();
                for (name, ty) in fields {
                    let environment = match ty {
                        Type::Named(name) => {
                            self.state.snapshot(
                                name.len().saturating_mul(bounds.len().saturating_add(1)),
                                span,
                            )?;
                            bounds
                                .iter()
                                .find(|bound| bound.generic_name == *name)
                                .map(|bound| bindings.get(bound.generic_index, span))
                                .transpose()?
                        }
                        Type::Closure(id) => Some(*id),
                        _ => None,
                    };
                    if let Some(id) = environment {
                        self.state.snapshot(name.len().saturating_add(1), span)?;
                        captures.push((name.clone(), id));
                    }
                }
                if captures.is_empty() {
                    Shape::Unknown
                } else {
                    Shape::Package(captures)
                }
            }
            _ => Shape::Unknown,
        })
    }
    fn signature<'t>(&mut self, callee: &CoreExpr, tables: &'t Tables) -> R<Option<&'t Signature>> {
        self.state.snapshot(tree_work(tables.len()), callee.span)?;
        let local = tables
            .get(&self.context.module)
            .ok_or_else(|| unsupported(callee.span, "helper scheduling module lookup is absent"))?;
        match &callee.kind {
            CoreExprKind::Name(name) => {
                if self.local(name, callee.span)?.is_some() {
                    return Ok(None);
                }
                self.state.snapshot(
                    tree_work(local.len()).saturating_mul(name.len().saturating_add(1)),
                    callee.span,
                )?;
                Ok(local.get(name))
            }
            CoreExprKind::Member { object, member } => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return Ok(None);
                };
                if self.local(alias, object.span)?.is_some() {
                    return Ok(None);
                }
                let length = alias.len().saturating_add(member.len()).saturating_add(1);
                self.state.snapshot(
                    length.saturating_mul(tree_work(local.len()).saturating_add(1)),
                    callee.span,
                )?;
                if let Some(signature) = local.get(&format!("{alias}.{member}")) {
                    return Ok(Some(signature));
                }
                self.state.snapshot(
                    tree_work(self.context.imports.len())
                        .saturating_mul(alias.len().saturating_add(1)),
                    callee.span,
                )?;
                let Some(module) = self.context.imports.get(alias) else {
                    return Ok(None);
                };
                self.state.snapshot(tree_work(tables.len()), callee.span)?;
                let Some(table) = tables.get(module) else {
                    return Ok(None);
                };
                self.state.snapshot(
                    tree_work(table.len()).saturating_mul(member.len().saturating_add(1)),
                    callee.span,
                )?;
                Ok(table.get(member))
            }
            _ => Ok(None),
        }
    }
    fn applied_result_shape(
        &mut self,
        ty: &Type,
        bounds: &[CoreCallableParameter],
        bindings: Bindings,
        span: Span,
        depth: usize,
    ) -> R<Shape> {
        if self.full_values {
            return self.shape(ty, bounds, bindings, span, depth);
        }
        if let Type::ErrorUnion { error, result } = ty {
            self.node(depth, span)?;
            self.state.snapshot(2, span)?;
            let error = self.applied_result_shape(error, bounds, bindings, span, depth + 1)?;
            let result = self.applied_result_shape(result, bounds, bindings, span, depth + 1)?;
            Ok(Shape::ErrorUnion {
                error: Box::new(error),
                result: Box::new(result),
            })
        } else {
            self.shape(ty, bounds, bindings, span, depth)
        }
    }
    fn pattern(&mut self, pattern: &CorePattern, shape: &Shape, depth: usize) -> R<()> {
        self.node(depth, pattern.span)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                let copy = self.clone_shape(shape, pattern.span)?;
                self.bind(name, copy, pattern.span)?;
            }
            CorePatternKind::Tuple(fields) | CorePatternKind::Variant { fields, .. } => {
                for field in fields {
                    self.pattern(field, &Shape::Unknown, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn patterned(
        &mut self,
        pattern: &CorePattern,
        shape: &Shape,
        body: &CoreBlock,
        depth: usize,
    ) -> R<()> {
        self.state.snapshot(1, body.span)?;
        self.scopes.push(BTreeMap::new());
        self.pattern(pattern, shape, depth + 1)?;
        self.block(body, depth + 1)?;
        self.scopes.pop();
        Ok(())
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> R<()> {
        self.node(depth, block.span)?;
        self.state.snapshot(1, block.span)?;
        self.scopes.push(BTreeMap::new());
        for statement in &block.statements {
            self.node(depth + 1, statement.span)?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name,
                    ty,
                    initializer,
                    ..
                } => {
                    let inferred = if let Some(value) = initializer {
                        self.expr(value, depth + 1)?
                    } else {
                        Shape::Unknown
                    };
                    let shape = if let Some(ty) = ty {
                        if matches!(inferred, Shape::ErrorUnion { .. }) {
                            self.applied_result_shape(
                                ty,
                                self.bounds,
                                self.bindings,
                                statement.span,
                                depth + 1,
                            )?
                        } else {
                            self.shape(ty, self.bounds, self.bindings, statement.span, depth + 1)?
                        }
                    } else {
                        inferred
                    };
                    self.bind(name, shape, statement.span)?;
                }
                CoreStatementKind::Assign { target, value } => {
                    // Assignments cannot change a checked local's concrete type.
                    // The normal Builder still checks both its type and origin.
                    self.expr(target, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::Expression(e) | CoreStatementKind::Return(Some(e)) => {
                    self.expr(e, depth + 1)?;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let shape = self.expr(value, depth + 1)?;
                    self.patterned(pattern, &shape, then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expr(condition, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let shape = self.expr(value, depth + 1)?;
                    self.patterned(pattern, &shape, body, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    self.expr(start, depth + 1)?;
                    self.expr(end, depth + 1)?;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.bind(name, Shape::Unknown, statement.span)?;
                    self.block(body, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::Match { value, arms } => {
                    let shape = self.expr(value, depth + 1)?;
                    for arm in arms {
                        self.patterned(&arm.pattern, &shape, &arm.body, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(body)
                | CoreStatementKind::Unsafe(body)
                | CoreStatementKind::Defer(body) => self.block(body, depth + 1)?,
                _ => {}
            }
        }
        self.scopes.pop();
        Ok(())
    }
    fn expr(&mut self, expression: &CoreExpr, depth: usize) -> R<Shape> {
        self.node(depth, expression.span)?;
        Ok(match &expression.kind {
            CoreExprKind::Name(name) => {
                self.local(name, expression.span)?.unwrap_or(Shape::Unknown)
            }
            CoreExprKind::Call { callee, arguments } => {
                if let CoreExprKind::TypeApply { callee: target, .. } = &callee.kind {
                    self.node(depth + 1, callee.span)?;
                    self.expr(target, depth + 2)?;
                } else {
                    self.expr(callee, depth + 1)?;
                }
                self.state.snapshot(arguments.len(), expression.span)?;
                let mut shapes = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    shapes.push(self.expr(argument, depth + 1)?);
                }
                if self.full_values
                    && let Some(name) = dynamic_store::primitive(callee)
                    && let Some(Shape::Complete { ty, .. }) = shapes.first()
                    && let Some(element) = dynamic_store::element(ty)
                {
                    let work =
                        inference::type_snapshot_work(element, expression.span, self.limits)?;
                    self.state
                        .snapshot(work.saturating_mul(2), expression.span)?;
                    let result = dynamic_store::result_type(name, element);
                    return self.shape(
                        &result,
                        &[],
                        Bindings::default(),
                        expression.span,
                        depth + 1,
                    );
                }
                if let CoreExprKind::TypeApply {
                    callee: target,
                    type_arguments,
                } = &callee.kind
                {
                    let [argument] = type_arguments.as_slice() else {
                        return Err(error(
                            "E-TYPE-APPLICATION",
                            callee.span,
                            "one complete type argument is required",
                        ));
                    };
                    let tables = self.tables.clone();
                    let signature = self.signature(target, &tables)?.ok_or_else(|| {
                        error(
                            "E-TYPE-APPLICATION",
                            callee.span,
                            "type application requires a static template",
                        )
                    })?;
                    let bound = signature.type_parameter.as_ref().ok_or_else(|| {
                        error(
                            "E-TYPE-APPLICATION",
                            callee.span,
                            "type application target is not an ordinary StoreElement template",
                        )
                    })?;
                    let actual = store_templates::resolve_argument(
                        argument,
                        self.bounds,
                        self.bindings,
                        self.state,
                        self.limits,
                        callee.span,
                    )?;
                    let index = store_templates::intern(
                        signature.template.expect("validated template"),
                        &actual,
                        self.state,
                        self.limits,
                        callee.span,
                    )?;
                    self.state
                        .snapshot(tree_work(self.found.len()).saturating_add(1), callee.span)?;
                    self.found.insert(Node::Store(index));
                    let products = self.state.products.clone();
                    let result = store_templates::substitute_type(
                        &signature.result,
                        &bound.name,
                        &actual,
                        self.limits,
                        &products,
                        callee.span,
                        &mut |n| self.state.snapshot(n, callee.span),
                    )?;
                    return self.applied_result_shape(
                        &result,
                        &[],
                        Bindings::default(),
                        expression.span,
                        depth + 1,
                    );
                }
                if self.store_shapes
                    && matches!(
                        dynamic_store::primitive(callee),
                        Some("store_at" | "store_at_mut" | "store_pop")
                    )
                    && let Some(Shape::Store(element)) = shapes.first()
                {
                    return self.clone_shape(element, expression.span);
                }
                self.state.snapshot(1, expression.span)?;
                let tables = self.tables.clone();
                if let Some(signature) = self.signature(callee, &tables)? {
                    if let Some(bound) = &signature.type_parameter {
                        if !self.full_values {
                            // The original light plan is unchanged unless an
                            // actual new bare-template call requires full T.
                            self.restart_full = true;
                            return Err(unsupported(
                                expression.span,
                                "restart finite planning with complete value types",
                            ));
                        }
                        if arguments.len() != signature.parameters.len() {
                            return Err(error(
                                "E-CALL-ARITY",
                                expression.span,
                                "ordinary template value argument count differs",
                            ));
                        }
                        let mut actual = None;
                        let products = self.state.products.clone();
                        for ((pattern, shape), argument) in
                            signature.parameters.iter().zip(&shapes).zip(arguments)
                        {
                            if !aggregate_templates::parameter_in(
                                pattern,
                                &bound.name,
                                &products,
                                self.limits,
                                argument.span,
                                &mut |n| self.state.snapshot(n, argument.span),
                            )? {
                                continue;
                            }
                            let Shape::Complete { ty, .. } = shape else {
                                return Err(unsupported(
                                    argument.span,
                                    "complete value type is absent from the finite ordinary template plan",
                                ));
                            };
                            store_templates::infer_argument(
                                pattern,
                                ty,
                                &bound.name,
                                &mut actual,
                                (self.limits, &products),
                                argument.span,
                                &mut |work| self.state.snapshot(work, argument.span),
                            )?;
                        }
                        let actual = actual.ok_or_else(|| error("E-TYPE-APPLICATION", expression.span,
                            "ordinary template call has no unique complete type constraint from a value argument"))?;
                        let index = store_templates::intern(
                            signature.template.expect("ordinary template"),
                            &actual,
                            self.state,
                            self.limits,
                            expression.span,
                        )?;
                        self.state.snapshot(
                            tree_work(self.found.len()).saturating_add(1),
                            expression.span,
                        )?;
                        self.found.insert(Node::Store(index));
                        let products = self.state.products.clone();
                        let result = store_templates::substitute_type(
                            &signature.result,
                            &bound.name,
                            &actual,
                            self.limits,
                            &products,
                            expression.span,
                            &mut |work| self.state.snapshot(work, expression.span),
                        )?;
                        return self.shape(
                            &result,
                            &[],
                            Bindings::default(),
                            expression.span,
                            depth + 1,
                        );
                    }
                    if let Some(key) = signature.template {
                        if signature.callable.is_empty() {
                            return Err(unsupported(
                                expression.span,
                                "helper scheduling bound is absent",
                            ));
                        }
                        let mut bindings = Bindings::default();
                        self.state
                            .snapshot(signature.callable.len(), expression.span)?;
                        if state_origins::replacement_signature(signature) {
                            bindings =
                                self.replacement_bindings(signature, &shapes, expression.span)?;
                        } else {
                            for bound in &signature.callable {
                                let shape = shapes.get(bound.index as usize).ok_or_else(|| {
                                    error(
                                        "E-CALL-ARITY",
                                        expression.span,
                                        "helper scheduling argument count differs",
                                    )
                                })?;
                                self.state.snapshot(shape.work(), expression.span)?;
                                let selected = if let Some(state) = &signature.state_parameter {
                                    let binding = state
                                        .bindings
                                        .iter()
                                        .find(|b| b.generic_index == bound.generic_index)
                                        .ok_or_else(|| {
                                            unsupported(
                                                expression.span,
                                                "State plan binding is absent",
                                            )
                                        })?;
                                    let Type::Package { fields, .. } =
                                        &signature.parameters[state.index as usize]
                                    else {
                                        return Err(unsupported(
                                            expression.span,
                                            "State plan parameter is not a Package",
                                        ));
                                    };
                                    let (name, _) = &fields[binding.fields[0] as usize];
                                    self.state.snapshot(
                                        name.len().saturating_mul(shape.work()),
                                        expression.span,
                                    )?;
                                    if let Shape::Complete { ty, .. } = shape {
                                        let Type::Package { fields, .. } = ty.as_ref() else {
                                            return Err(unsupported(
                                                expression.span,
                                                "State plan owner is not a complete Package",
                                            ));
                                        };
                                        fields.iter().find_map(|(field, ty)| match ty {
                                            Type::Closure(id) if field == name => Some(*id),
                                            _ => None,
                                        })
                                    } else if let Shape::Package(actual) = shape {
                                        actual
                                            .iter()
                                            .find(|(field, _)| field == name)
                                            .map(|(_, id)| *id)
                                    } else {
                                        None
                                    }
                                } else {
                                    shape.environment()
                                };
                                let id = selected.ok_or_else(|| unsupported(expression.span,
                                "nested helper environment type cannot be established by the finite instance plan"))?;
                                bindings.bind(bound.generic_index, id, expression.span)?;
                            }
                        }
                        self.state.snapshot(
                            tree_work(self.found.len()).saturating_add(1),
                            expression.span,
                        )?;
                        self.found.insert(Node::Callable((key, bindings)));
                        self.shape(
                            &signature.result,
                            &signature.callable,
                            bindings,
                            expression.span,
                            depth + 1,
                        )?
                    } else {
                        self.shape(
                            &signature.result,
                            &[],
                            Bindings::default(),
                            expression.span,
                            depth + 1,
                        )?
                    }
                } else {
                    Shape::Unknown
                }
            }
            CoreExprKind::Ascribed { ty, value } => {
                let value = self.expr(value, depth + 1)?;
                if matches!(value, Shape::ErrorUnion { .. }) {
                    self.applied_result_shape(
                        ty,
                        self.bounds,
                        self.bindings,
                        expression.span,
                        depth + 1,
                    )?
                } else {
                    self.shape(ty, self.bounds, self.bindings, expression.span, depth + 1)?
                }
            }
            CoreExprKind::Unary { operand, .. } | CoreExprKind::CompileTime(operand) => {
                self.expr(operand, depth + 1)?
            }
            CoreExprKind::Try(operand) => match self.expr(operand, depth + 1)? {
                // Only the success alternative reaches the continuation. In
                // particular T!void never becomes a callback Package here.
                Shape::ErrorUnion { result, .. } => *result,
                Shape::Complete { ty, .. } if matches!(ty.as_ref(), Type::ErrorUnion { .. }) => {
                    let Type::ErrorUnion { result, .. } = ty.as_ref() else {
                        unreachable!()
                    };
                    self.shape(result, &[], Bindings::default(), expression.span, depth + 1)?
                }
                old_primitive_shape => old_primitive_shape,
            },
            CoreExprKind::IntegerConvert { value, target } => {
                self.expr(value, depth + 1)?;
                if self.full_values {
                    self.shape(target, &[], Bindings::default(), expression.span, depth + 1)?
                } else {
                    Shape::Unknown
                }
            }
            CoreExprKind::Member { object, member } => {
                let shape = self.expr(object, depth + 1)?;
                if let Shape::Complete { ty, work } = shape {
                    self.state
                        .snapshot(work.saturating_add(member.len()), expression.span)?;
                    let ty = if let Type::Address { inner, .. } = ty.as_ref() {
                        inner.as_ref()
                    } else {
                        ty.as_ref()
                    };
                    let (_, field) = aggregates::field(ty, member, expression.span)?;
                    self.shape(&field, &[], Bindings::default(), expression.span, depth + 1)?
                } else if let Shape::Package(fields) = shape {
                    self.state.snapshot(
                        fields.len().saturating_mul(member.len().saturating_add(1)),
                        expression.span,
                    )?;
                    fields
                        .iter()
                        .find_map(|(name, id)| (name == member).then_some(Shape::Environment(*id)))
                        .unwrap_or(Shape::Unknown)
                } else {
                    Shape::Unknown
                }
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
                Shape::Unknown
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                for field in fields {
                    self.expr(field, depth + 1)?;
                }
                Shape::Unknown
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(object, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expr(value, depth + 1)?;
                }
                Shape::Unknown
            }
            CoreExprKind::Record { ty, fields } => {
                for (_, field) in fields {
                    self.expr(field, depth + 1)?;
                }
                self.shape(ty, self.bounds, self.bindings, expression.span, depth + 1)?
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                let original = self.expr(base, depth + 1)?;
                for (_, field) in fields {
                    self.expr(field, depth + 1)?;
                }
                if let Some(ty) = ty {
                    self.shape(ty, self.bounds, self.bindings, expression.span, depth + 1)?
                } else {
                    original
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.state.snapshot(1, expression.span)?;
                self.scopes.push(BTreeMap::new());
                for (name, initializer) in bindings {
                    let shape = self.expr(initializer, depth + 1)?;
                    self.bind(name, shape, expression.span)?;
                }
                let shape = self.expr(value, depth + 1)?;
                self.scopes.pop();
                shape
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, depth + 1)?;
                let a = self.expr(then_value, depth + 1)?;
                let b = self.expr(else_value, depth + 1)?;
                self.state
                    .snapshot(a.work().saturating_add(b.work()), expression.span)?;
                if a == b { a } else { Shape::Unknown }
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } if self.owned_factory => {
                self.state
                    .snapshot(parameters.len().saturating_add(1), expression.span)?;
                self.scopes.push(BTreeMap::new());
                for parameter in parameters {
                    let shape = if let Some(ty) = &parameter.ty {
                        self.shape(ty, &[], Bindings::default(), parameter.span, depth + 1)?
                    } else {
                        Shape::Unknown
                    };
                    self.bind(&parameter.name, shape, parameter.span)?;
                }
                self.block(body, depth + 1)?;
                self.scopes.pop();
                // This is dependency scheduling only. No unresolved shape can
                // supply a concrete helper binding; Builder must finish the
                // real closure before using its actual identity.
                Shape::Unknown
            }
            CoreExprKind::Closure { .. } => {
                return Err(unsupported(
                    expression.span,
                    "helper-local nested environments require an additional finite instance contract",
                ));
            }
            CoreExprKind::Literal(value) if self.full_values => self.shape(
                &value.ty(),
                &[],
                Bindings::default(),
                expression.span,
                depth + 1,
            )?,
            CoreExprKind::Bool(_) if self.full_values => self.shape(
                &Type::Bool,
                &[],
                Bindings::default(),
                expression.span,
                depth + 1,
            )?,
            CoreExprKind::Unit if self.full_values => self.shape(
                &Type::Unit,
                &[],
                Bindings::default(),
                expression.span,
                depth + 1,
            )?,
            CoreExprKind::Integer(text) if self.full_values => {
                self.state.snapshot(text.len(), expression.span)?;
                let ty = integer(text, None, false, expression.span)?.ty();
                self.shape(&ty, &[], Bindings::default(), expression.span, depth + 1)?
            }
            _ => Shape::Unknown,
        })
    }

    fn replacement_bindings(
        &mut self,
        signature: &Signature,
        arguments: &[Shape],
        span: Span,
    ) -> R<Bindings> {
        let state = signature
            .state_parameter
            .as_ref()
            .expect("State replacement owner");
        let binding = &state.bindings[0];
        self.state.snapshot(
            signature
                .callable
                .len()
                .saturating_add(binding.fields.len()),
            span,
        )?;
        let direct = signature
            .callable
            .iter()
            .find(|bound| bound.index != state.index)
            .expect("State replacement direct source");
        let owner = arguments.get(state.index as usize).ok_or_else(|| {
            error(
                "E-CALL-ARITY",
                span,
                "State replacement plan owner is absent",
            )
        })?;
        let Type::Package { fields, .. } = &signature.parameters[state.index as usize] else {
            return Err(unsupported(
                span,
                "State replacement plan requires a complete Package",
            ));
        };
        self.state.snapshot(owner.work(), span)?;
        let mut result = Bindings::default();
        for selector in &binding.fields {
            let (name, _) = fields
                .get(*selector as usize)
                .ok_or_else(|| unsupported(span, "State replacement plan selector is absent"))?;
            self.state.snapshot(
                name.len().saturating_add(1).saturating_mul(owner.work()),
                span,
            )?;
            let selected = match owner {
                Shape::Complete { ty, .. } => {
                    let Type::Package { fields, .. } = ty.as_ref() else {
                        return Err(unsupported(
                            span,
                            "State replacement plan owner is not a Package",
                        ));
                    };
                    fields.iter().find_map(|(field, ty)| match ty {
                        Type::Closure(id) if field == name => Some(*id),
                        _ => None,
                    })
                }
                Shape::Package(fields) => fields
                    .iter()
                    .find(|(field, _)| field == name)
                    .map(|(_, id)| *id),
                _ => None,
            }
            .ok_or_else(|| {
                unsupported(span, "State replacement plan field environment is absent")
            })?;
            result.bind(binding.generic_index, selected, span)?;
        }
        let replacement = arguments.get(direct.index as usize).ok_or_else(|| {
            error(
                "E-CALL-ARITY",
                span,
                "State replacement plan value is absent",
            )
        })?;
        self.state.snapshot(replacement.work(), span)?;
        let id = replacement
            .environment()
            .ok_or_else(|| unsupported(span, "State replacement plan environment is absent"))?;
        result.bind(direct.generic_index, id, span)?;
        Ok(result)
    }
}

fn dependencies(
    instance: Node,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<Vec<Node>> {
    state.snapshot(
        tree_work(state.helper_templates.len()).saturating_add(2),
        span,
    )?;
    let (key, bindings) = match instance {
        Node::Callable((key, bindings)) => (key, bindings),
        Node::Store(index) => (
            state.store_instances[index as usize].key,
            Bindings::default(),
        ),
    };
    let template = state
        .helper_templates
        .get(&key)
        .cloned()
        .ok_or_else(|| unsupported(span, "helper scheduling template is absent"))?;
    let specialized;
    let function = if let Node::Store(index) = instance {
        state.snapshot(state.store_instances[index as usize].work, span)?;
        let actual = state.store_instances[index as usize].actual.clone();
        specialized = store_templates::specialize(&template, &actual, state, limits)?;
        &specialized
    } else {
        template.function.as_ref()
    };
    let bounds_view = state_origins::bounds(function);
    let bounds = bounds_view.as_ref();
    if bounds.is_empty() && matches!(instance, Node::Callable(_)) {
        return Err(unsupported(
            span,
            "helper scheduling template bound is absent",
        ));
    }
    state.snapshot(1, span)?;
    let tables = state.helper_tables.clone();
    let mut walk = Walk {
        context: &template.context,
        bounds,
        bindings,
        tables,
        scopes: vec![BTreeMap::new()],
        found: BTreeSet::new(),
        state,
        limits,
        store_shapes: template.store_shapes,
        full_values: false,
        restart_full: false,
        owned_factory: owning_factory::template(function),
    };
    for parameter in &function.parameters {
        let shape = walk.shape(&parameter.ty, bounds, bindings, parameter.span, 0)?;
        walk.bind(&parameter.name, shape, parameter.span)?;
    }
    let first = walk.block(&function.body, 0);
    if walk.restart_full {
        walk.state.snapshot(1, span)?;
        walk.scopes = vec![BTreeMap::new()];
        walk.found.clear();
        walk.full_values = true;
        walk.restart_full = false;
        for parameter in &function.parameters {
            let shape = walk.shape(&parameter.ty, bounds, bindings, parameter.span, 0)?;
            walk.bind(&parameter.name, shape, parameter.span)?;
        }
        walk.block(&function.body, 0)?;
    } else {
        first?;
    }
    walk.state.snapshot(walk.found.len(), span)?;
    Ok(walk.found.into_iter().collect())
}

/// Every frame is data on an explicitly bounded work list. The source walk's
/// local syntax depth never accumulates along the helper dependency chain.
pub(super) fn order(
    root: Node,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<Vec<Node>> {
    state.snapshot(1, span)?;
    let mut pending = vec![(root, false)];
    let mut edges = BTreeMap::<Node, Vec<Node>>::new();
    let mut active = BTreeSet::new();
    let mut heights = BTreeMap::<Node, usize>::new();
    let mut result = Vec::new();
    while let Some((instance, finish)) = pending.pop() {
        state.snapshot(tree_work(heights.len()).saturating_add(1), span)?;
        if heights.contains_key(&instance) {
            continue;
        }
        if finish {
            state.snapshot(tree_work(edges.len()), span)?;
            let children = &edges[&instance];
            let mut height = 1usize;
            for child in children {
                state.snapshot(tree_work(heights.len()), span)?;
                height = height.max(heights[child].saturating_add(1));
            }
            if height > limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "callable helper nesting exceeds selected bound",
                    Some(span),
                ));
            }
            state.snapshot(
                tree_work(heights.len())
                    .saturating_add(tree_work(active.len()))
                    .saturating_add(2),
                span,
            )?;
            heights.insert(instance, height);
            active.remove(&instance);
            state.snapshot(tree_work(state.helper_instances.len()), span)?;
            if !instance.ready(state) {
                result.push(instance);
            }
        } else {
            if let Node::Store(index) = instance {
                state.snapshot(active.len(), span)?;
                let key = state.store_instances[index as usize].key;
                if active.iter().any(|node| {
                    matches!(node, Node::Store(other)
                    if state.store_instances[*other as usize].key == key)
                }) {
                    return Err(unsupported(
                        span,
                        "recursive or growing ordinary template dependency has no finite instance plan",
                    ));
                }
            }
            state.snapshot(tree_work(active.len()).saturating_add(1), span)?;
            if !active.insert(instance) {
                return Err(unsupported(
                    span,
                    "recursive concrete helper dependency has no finite instance plan",
                ));
            }
            if active.len() > limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "callable helper nesting exceeds selected bound",
                    Some(span),
                ));
            }
            if edges.len() >= limits.max_functions {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "helper instance plan exceeds the function bound",
                    Some(span),
                ));
            }
            let children = dependencies(instance, state, limits, span)?;
            state.snapshot(
                children
                    .len()
                    .saturating_add(tree_work(edges.len()))
                    .saturating_add(2),
                span,
            )?;
            pending.push((instance, true));
            pending.extend(children.iter().rev().map(|child| (*child, false)));
            edges.insert(instance, children);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod result_shape_tests {
    use super::*;

    // Private scheduling facts only. Owned union branch patterns remain closed
    // at source/Types; source/native tests separately exercise real pop and push.
    #[test]
    fn binding_a_whole_union_preserves_both_sides_and_try_selects_only_success() {
        let limits = Limits::default();
        let context = Context {
            module: ModuleId(0),
            imports: BTreeMap::new(),
            enums: BTreeMap::new(),
        };
        let mut state = closures::State::new(0, &limits);
        let mut walk = Walk {
            context: &context,
            bounds: &[],
            bindings: Bindings::default(),
            tables: Rc::new(BTreeMap::new()),
            scopes: vec![BTreeMap::new()],
            found: BTreeSet::new(),
            state: &mut state,
            limits: &limits,
            store_shapes: false,
            full_values: false,
            restart_full: false,
            owned_factory: false,
        };
        let span = Span::default();
        let pattern = CorePattern {
            kind: CorePatternKind::Name("bound".into()),
            span,
        };
        let name = CoreExpr {
            id: NodeId(0),
            kind: CoreExprKind::Name("bound".into()),
            span,
        };
        let question = CoreExpr {
            id: NodeId(1),
            kind: CoreExprKind::Try(Box::new(name.clone())),
            span,
        };
        for (error_shape, result_shape) in [
            (
                Shape::Package(vec![("callback".into(), ClosureId(0))]),
                Shape::Unknown,
            ),
            (
                Shape::Unknown,
                Shape::Package(vec![("callback".into(), ClosureId(1))]),
            ),
            (
                Shape::Package(vec![("callback".into(), ClosureId(0))]),
                Shape::Package(vec![("callback".into(), ClosureId(1))]),
            ),
        ] {
            let union = Shape::ErrorUnion {
                error: Box::new(error_shape),
                result: Box::new(result_shape.clone()),
            };
            walk.pattern(&pattern, &union, 0).unwrap();
            let stored = walk.expr(&name, 0).unwrap();
            assert_eq!(stored, union);
            assert_eq!(stored.environment(), None);
            assert_eq!(walk.expr(&question, 0).unwrap(), result_shape);
        }
    }
}
