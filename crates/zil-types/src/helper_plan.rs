//! Scheduling facts describe concrete types, never runtime instance origins.
//! A callee is checked normally before a parent Builder may use its instance.
use super::*;

#[derive(Clone, PartialEq, Eq)]
enum Shape {
    Unknown,
    Environment(ClosureId),
    Package(Vec<(String, ClosureId)>),
}

impl Shape {
    fn work(&self) -> usize {
        match self {
            Self::Package(fields) => fields.iter().fold(1usize, |n, (name, _)| {
                n.saturating_add(name.len()).saturating_add(1)
            }),
            _ => 1,
        }
    }
    fn environment(&self) -> Option<ClosureId> {
        match self {
            Self::Environment(id) => Some(*id),
            Self::Package(fields) => {
                let id = fields.first()?.1;
                fields.iter().all(|(_, other)| *other == id).then_some(id)
            }
            Self::Unknown => None,
        }
    }
}

pub(super) fn tree_work(count: usize) -> usize {
    count.checked_ilog2().map_or(1, |n| n as usize + 2)
}

struct Walk<'a, 'b> {
    context: &'a Context,
    bounds: &'a [CoreCallableParameter],
    bindings: Bindings,
    tables: Rc<Tables>,
    scopes: Vec<BTreeMap<String, Shape>>,
    found: BTreeSet<Instance>,
    state: &'b mut closures::State,
    limits: &'a Limits,
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
                        self.shape(ty, self.bounds, self.bindings, statement.span, depth + 1)?
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
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(body, depth + 1)?
                }
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
                self.expr(callee, depth + 1)?;
                self.state.snapshot(arguments.len(), expression.span)?;
                let mut shapes = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    shapes.push(self.expr(argument, depth + 1)?);
                }
                self.state.snapshot(1, expression.span)?;
                let tables = self.tables.clone();
                if let Some(signature) = self.signature(callee, &tables)? {
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
                        for bound in &signature.callable {
                            let shape = shapes.get(bound.index as usize).ok_or_else(|| {
                                error(
                                    "E-CALL-ARITY",
                                    expression.span,
                                    "helper scheduling argument count differs",
                                )
                            })?;
                            self.state.snapshot(shape.work(), expression.span)?;
                            let id = shape.environment().ok_or_else(|| unsupported(expression.span,
                                "nested helper environment type cannot be established by the finite instance plan"))?;
                            bindings.bind(bound.generic_index, id, expression.span)?;
                        }
                        self.state.snapshot(
                            tree_work(self.found.len()).saturating_add(1),
                            expression.span,
                        )?;
                        self.found.insert((key, bindings));
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
                self.expr(value, depth + 1)?;
                self.shape(ty, self.bounds, self.bindings, expression.span, depth + 1)?
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::CompileTime(operand)
            | CoreExprKind::Try(operand) => self.expr(operand, depth + 1)?,
            CoreExprKind::IntegerConvert { value, .. } => {
                self.expr(value, depth + 1)?;
                Shape::Unknown
            }
            CoreExprKind::Member { object, member } => {
                let shape = self.expr(object, depth + 1)?;
                if let Shape::Package(fields) = shape {
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
            CoreExprKind::Closure { .. } => {
                return Err(unsupported(
                    expression.span,
                    "helper-local nested environments require an additional finite instance contract",
                ));
            }
            _ => Shape::Unknown,
        })
    }
}

fn dependencies(
    instance: Instance,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<Vec<Instance>> {
    state.snapshot(
        tree_work(state.helper_templates.len()).saturating_add(2),
        span,
    )?;
    let template = state
        .helper_templates
        .get(&instance.0)
        .cloned()
        .ok_or_else(|| unsupported(span, "helper scheduling template is absent"))?;
    let function = &template.function;
    let bounds = &function.callable_parameters;
    if bounds.is_empty() {
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
        bindings: instance.1,
        tables,
        scopes: vec![BTreeMap::new()],
        found: BTreeSet::new(),
        state,
        limits,
    };
    for parameter in &function.parameters {
        let shape = walk.shape(&parameter.ty, bounds, instance.1, parameter.span, 0)?;
        walk.bind(&parameter.name, shape, parameter.span)?;
    }
    walk.block(&function.body, 0)?;
    walk.state.snapshot(walk.found.len(), span)?;
    Ok(walk.found.into_iter().collect())
}

/// Every frame is data on an explicitly bounded work list. The source walk's
/// local syntax depth never accumulates along the helper dependency chain.
pub(super) fn order(
    root: Instance,
    state: &mut closures::State,
    limits: &Limits,
    span: Span,
) -> R<Vec<Instance>> {
    state.snapshot(1, span)?;
    let mut pending = vec![(root, false)];
    let mut edges = BTreeMap::<Instance, Vec<Instance>>::new();
    let mut active = BTreeSet::new();
    let mut heights = BTreeMap::<Instance, usize>::new();
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
            if !state.helper_instances.contains_key(&instance) {
                result.push(instance);
            }
        } else {
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
