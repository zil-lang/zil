//! Private source declarations for the finite two-binding owning State profile.
//! A declaration restricts a use; it never supplies a runtime value or loan.
use super::*;
use std::borrow::Cow;
use std::rc::Rc;
use zil_base::NodeId;

pub(super) fn bounds(function: &CoreFunction) -> Cow<'_, [CoreCallableParameter]> {
    let Some(state) = function.state_parameters.first() else {
        return Cow::Borrowed(&function.callable_parameters);
    };
    Cow::Owned(
        state
            .bindings
            .iter()
            .map(|binding| CoreCallableParameter {
                index: state.index,
                generic_index: binding.generic_index,
                generic_name: binding.generic_name.clone(),
                kind: binding.kind,
                parameters: binding.parameters.clone(),
                result: binding.result.clone(),
            })
            .collect(),
    )
}

pub(super) fn validate_header(function: &CoreFunction) -> R<()> {
    if function.state_parameters.is_empty() {
        return Ok(());
    }
    let reject = || {
        error(
            "E-STATE-PARAMETERS",
            function.span,
            "State declarations require one owned parameter with two distinct ordered callable bindings",
        )
    };
    let [state] = function.state_parameters.as_slice() else {
        return Err(reject());
    };
    if !function.callable_parameters.is_empty()
        || !function.type_parameters.is_empty()
        || function.callable_return.is_some()
        || function.foreign.is_some()
        || function.receiver.is_some()
        || state.bindings.len() != 2
        || state.index as usize >= function.parameters.len()
        || function.parameters[state.index as usize].mode != PassingMode::Infer
    {
        return Err(reject());
    }
    for (index, binding) in state.bindings.iter().enumerate() {
        if binding.generic_index != index as u32
            || binding.generic_name.is_empty()
            || binding.kind == CallableKind::FunctionPointer
            || !binding.result.is_copy_value()
            || !closures::supported_signature(&binding.parameters, &binding.result)
            || binding.fields.len() != 1
        {
            return Err(reject());
        }
    }
    if state.bindings[0].generic_name == state.bindings[1].generic_name
        || state.bindings[0].fields == state.bindings[1].fields
    {
        return Err(reject());
    }
    Ok(())
}

/// Called only after aggregate resolution and the input's bounded metadata walk.
pub(super) fn validate_shape(function: &CoreFunction) -> R<()> {
    validate_header(function)?;
    let Some(state) = function.state_parameters.first() else {
        return Ok(());
    };
    let reject = || {
        error(
            "E-STATE-PARAMETERS",
            function.span,
            "State selectors must cover exactly the two direct fields of their declared binders",
        )
    };
    let Type::Package { fields, .. } = &function.parameters[state.index as usize].ty else {
        return Err(reject());
    };
    let mut covered = 0;
    for (index, (_, ty)) in fields.iter().enumerate() {
        if let Some(binding) = state
            .bindings
            .iter()
            .find(|binding| matches!(ty, Type::Named(name) if name == &binding.generic_name))
        {
            if binding.fields.as_slice() != [index as u32] {
                return Err(reject());
            }
            covered += 1;
        } else if !(ty.is_copy_value()
            || matches!(ty, Type::Managed(_) if resources::supported(ty)))
        {
            return Err(reject());
        }
    }
    if covered != 2 {
        return Err(reject());
    }
    Ok(())
}

pub(super) fn match_type(
    pattern: &Type,
    actual: &Type,
    bounds: &[CoreCallableParameter],
    span: Span,
) -> R<helpers::Bindings> {
    let reject = || {
        error(
            "E-TYPE-MISMATCH",
            span,
            "State requires the complete nominal shape and both concrete callback bindings",
        )
    };
    let (
        Type::Package {
            name: a,
            fields: af,
        },
        Type::Package {
            name: b,
            fields: bf,
        },
    ) = (pattern, actual)
    else {
        return Err(reject());
    };
    if callable_fields::nominal_base(a) != callable_fields::nominal_base(b) || af.len() != bf.len()
    {
        return Err(reject());
    }
    let mut bindings = helpers::Bindings::default();
    for ((an, wanted), (bn, got)) in af.iter().zip(bf) {
        if an != bn {
            return Err(reject());
        }
        if let Some(bound) = bounds
            .iter()
            .find(|bound| matches!(wanted, Type::Named(name) if name == &bound.generic_name))
        {
            let Type::Closure(id) = got else {
                return Err(reject());
            };
            bindings.bind(bound.generic_index, *id, span)?;
        } else if wanted != got {
            return Err(reject());
        }
    }
    for bound in bounds {
        bindings.get(bound.generic_index, span)?;
    }
    Ok(bindings)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Shape {
    None,
    Callable(u32),
    Package(Vec<(String, Shape)>),
    Address(Box<Shape>),
    Store(Box<Shape>),
    Union {
        error: Box<Shape>,
        result: Box<Shape>,
    },
}
impl Shape {
    fn work(&self) -> usize {
        match self {
            Self::Package(fields) => fields.iter().fold(1usize, |n, (name, ty)| {
                n.saturating_add(name.len()).saturating_add(ty.work())
            }),
            Self::Address(inner) | Self::Store(inner) => 1usize.saturating_add(inner.work()),
            Self::Union { error, result } => 1usize
                .saturating_add(error.work())
                .saturating_add(result.work()),
            _ => 1,
        }
    }
    fn has_origin(&self) -> bool {
        match self {
            Self::None => false,
            Self::Callable(_) => true,
            Self::Package(fields) => fields.iter().any(|(_, field)| field.has_origin()),
            Self::Address(inner) | Self::Store(inner) => inner.has_origin(),
            Self::Union { error, result } => error.has_origin() || result.has_origin(),
        }
    }
    fn slot(&self) -> Option<u32> {
        match self {
            Self::Callable(slot) => Some(*slot),
            Self::Address(inner) => inner.slot(),
            _ => None,
        }
    }
    fn field(&self, index: u32) -> Option<&Shape> {
        match self {
            Self::Package(fields) => fields.get(index as usize).map(|(_, shape)| shape),
            Self::Address(inner) => inner.field(index),
            _ => None,
        }
    }
}

pub(super) struct Uses {
    bindings: helpers::Bindings,
    declarations: Vec<CoreCallableParameter>,
    expressions: BTreeMap<NodeId, Shape>,
}
impl Uses {
    pub(super) fn lookup_work(&self) -> usize {
        helpers::plan::tree_work(self.expressions.len()).saturating_add(self.declarations.len())
    }
    pub(super) fn declaration(
        &self,
        expression: &CoreExpr,
        field: Option<u32>,
    ) -> Option<(ClosureId, CallableKind)> {
        let mut shape = self.expressions.get(&expression.id)?;
        if let Some(field) = field {
            shape = shape.field(field)?;
        }
        let slot = shape.slot()?;
        let bound = self
            .declarations
            .iter()
            .find(|bound| bound.generic_index == slot)?;
        Some((self.bindings.0[slot as usize]?, bound.kind))
    }
}

impl Builder<'_, '_> {
    pub(super) fn declared_state_kind(
        &mut self,
        expression: &CoreExpr,
        field: Option<u32>,
        id: ClosureId,
    ) -> R<Option<CallableKind>> {
        let Some(uses) = &self.state_uses else {
            return Ok(None);
        };
        self.closures
            .snapshot(uses.lookup_work(), expression.span)?;
        let Some((expected, kind)) = uses.declaration(expression, field) else {
            return Ok(None);
        };
        if expected != id {
            return Err(error(
                "E-STATE-ORIGIN",
                expression.span,
                "State declaration does not match the actual callable type at this use",
            ));
        }
        let pending =
            self.closures.pending.get(id.0 as usize).ok_or_else(|| {
                unsupported(expression.span, "State callable definition is absent")
            })?;
        self.closures
            .snapshot(pending.captures.len().saturating_add(1), expression.span)?;
        if !helpers::compatible(self.closures.pending[id.0 as usize].callable_kind(), kind) {
            return Err(error(
                "E-CLOSURE-MODE",
                expression.span,
                "actual State field does not implement its declaration",
            ));
        }
        Ok(Some(kind))
    }
    pub(super) fn require_state_kind(
        &mut self,
        expression: &CoreExpr,
        field: Option<u32>,
        id: ClosureId,
        requested: CallableKind,
    ) -> R<()> {
        if self
            .declared_state_kind(expression, field, id)?
            .is_some_and(|kind| !helpers::compatible(kind, requested))
        {
            return Err(error(
                "E-CLOSURE-MODE",
                expression.span,
                "State declaration does not provide the requested helper capability",
            ));
        }
        Ok(())
    }
}

/// The side table belongs to this source instance, not to a ClosureId or a
/// globally specialized Package. Ordinary T instances remain shared.
pub(super) fn collect(
    function: &CoreFunction,
    bindings: helpers::Bindings,
    context: &helpers::Context,
    state: &mut closures::State,
    limits: &Limits,
) -> R<Option<Uses>> {
    if function.state_parameters.is_empty()
        && !callable_fields::dual_constructor(
            &function.callable_parameters,
            &function.return_type,
            |index| function.parameters.get(index).map(|p| &p.ty),
        )
    {
        return Ok(None);
    }
    let span = function.span;
    let (work, _) = inference::helper_snapshot_work(function, limits)?;
    state.snapshot(work, span)?;
    let bounds = bounds(function).into_owned();
    let tables = state.helper_tables.clone();
    let mut walk = Walk {
        bounds: &bounds,
        context,
        tables,
        scopes: vec![BTreeMap::new()],
        expressions: BTreeMap::new(),
        capture: false,
        state,
        limits,
    };
    for parameter in &function.parameters {
        let shape = walk.ty(&parameter.ty, &[], true, 0, parameter.span)?;
        walk.bind(&parameter.name, shape, parameter.span)?;
    }
    walk.block(&function.body, 0)?;
    let expressions = walk.expressions;
    Ok(Some(Uses {
        bindings,
        declarations: bounds,
        expressions,
    }))
}

struct Walk<'a, 's> {
    bounds: &'a [CoreCallableParameter],
    context: &'a helpers::Context,
    tables: Rc<helpers::Tables>,
    scopes: Vec<BTreeMap<String, Shape>>,
    expressions: BTreeMap<NodeId, Shape>,
    capture: bool,
    state: &'s mut closures::State,
    limits: &'a Limits,
}
impl Walk<'_, '_> {
    fn node(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "State declaration source depth exceeded",
                Some(span),
            ));
        }
        self.state.snapshot(1, span)
    }
    fn copy(&mut self, shape: &Shape, span: Span) -> R<Shape> {
        self.state.snapshot(shape.work(), span)?;
        Ok(shape.clone())
    }
    fn bind(&mut self, name: &str, shape: Shape, span: Span) -> R<()> {
        let scope = self.scopes.last_mut().expect("State source scope");
        self.state.snapshot(
            name.len()
                .saturating_mul(helpers::plan::tree_work(scope.len()))
                .saturating_add(name.len())
                .saturating_add(1),
            span,
        )?;
        scope.insert(name.to_owned(), shape);
        Ok(())
    }
    fn lookup(&mut self, name: &str, span: Span) -> R<Option<Shape>> {
        for scope in self.scopes.iter().rev() {
            self.state.snapshot(
                (name.len().saturating_add(1))
                    .saturating_mul(helpers::plan::tree_work(scope.len())),
                span,
            )?;
            if let Some(shape) = scope.get(name) {
                self.state.snapshot(shape.work(), span)?;
                return Ok(Some(shape.clone()));
            }
        }
        Ok(None)
    }
    fn ty(
        &mut self,
        ty: &Type,
        substitutions: &[(String, Shape)],
        caller: bool,
        depth: usize,
        span: Span,
    ) -> R<Shape> {
        self.node(depth, span)?;
        Ok(match ty {
            Type::Named(name) => {
                self.state.snapshot(
                    (name.len().saturating_add(1)).saturating_mul(
                        substitutions
                            .len()
                            .saturating_add(self.bounds.len())
                            .saturating_add(1),
                    ),
                    span,
                )?;
                if let Some((_, shape)) = substitutions.iter().find(|(n, _)| n == name) {
                    self.copy(shape, span)?
                } else if caller
                    && let Some(bound) = self.bounds.iter().find(|b| &b.generic_name == name)
                {
                    Shape::Callable(bound.generic_index)
                } else {
                    Shape::None
                }
            }
            Type::Package { fields, .. } => {
                self.state.snapshot(fields.len(), span)?;
                let mut result = Vec::new();
                for (name, ty) in fields {
                    self.state.snapshot(name.len(), span)?;
                    result.push((
                        name.clone(),
                        self.ty(ty, substitutions, caller, depth + 1, span)?,
                    ));
                }
                Shape::Package(result)
            }
            Type::Address { inner, .. } => Shape::Address(Box::new(self.ty(
                inner,
                substitutions,
                caller,
                depth + 1,
                span,
            )?)),
            Type::Managed(inner) | Type::Desc(inner) => {
                if let Type::OwnedStore { element } = inner.as_ref() {
                    Shape::Store(Box::new(self.ty(
                        element,
                        substitutions,
                        caller,
                        depth + 1,
                        span,
                    )?))
                } else {
                    Shape::None
                }
            }
            Type::ErrorUnion { error, result } => Shape::Union {
                error: Box::new(self.ty(error, substitutions, caller, depth + 1, span)?),
                result: Box::new(self.ty(result, substitutions, caller, depth + 1, span)?),
            },
            _ => Shape::None,
        })
    }
    fn signature(&mut self, target: &CoreExpr) -> R<Option<Signature>> {
        let table = &self.tables[&self.context.module];
        let found = match &target.kind {
            CoreExprKind::Name(name) => {
                self.state.snapshot(
                    (name.len().saturating_add(1))
                        .saturating_mul(helpers::plan::tree_work(table.len())),
                    target.span,
                )?;
                if self
                    .scopes
                    .iter()
                    .rev()
                    .any(|scope| scope.contains_key(name))
                {
                    None
                } else {
                    table.get(name)
                }
            }
            CoreExprKind::Member { object, member } => {
                if let CoreExprKind::Name(alias) = &object.kind {
                    self.state.snapshot(
                        alias
                            .len()
                            .saturating_add(member.len())
                            .saturating_add(1)
                            .saturating_mul(helpers::plan::tree_work(table.len()).saturating_add(
                                helpers::plan::tree_work(self.context.imports.len()),
                            )),
                        target.span,
                    )?;
                    if self
                        .scopes
                        .iter()
                        .rev()
                        .any(|scope| scope.contains_key(alias))
                    {
                        None
                    } else {
                        table.get(&format!("{alias}.{member}")).or_else(|| {
                            self.context
                                .imports
                                .get(alias)
                                .and_then(|module| self.tables.get(module))
                                .and_then(|table| table.get(member))
                        })
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(signature) = found {
            let work = inference::signature_snapshot_work("", signature, target.span, self.limits)?;
            self.state.snapshot(work, target.span)?;
            Ok(Some(signature.clone()))
        } else {
            Ok(None)
        }
    }
    fn require(&mut self, shape: &Shape, requested: CallableKind, span: Span) -> R<()> {
        if let Some(slot) = shape.slot() {
            self.state.snapshot(self.bounds.len(), span)?;
            let declared = self
                .bounds
                .iter()
                .find(|b| b.generic_index == slot)
                .ok_or_else(|| unsupported(span, "State declaration source is absent"))?;
            if !helpers::compatible(declared.kind, requested) {
                return Err(error(
                    "E-CLOSURE-MODE",
                    span,
                    "State declaration does not supply the helper's requested callable capability",
                ));
            }
        }
        Ok(())
    }
    fn expr(&mut self, expression: &CoreExpr, depth: usize) -> R<Shape> {
        self.node(depth, expression.span)?;
        let span = expression.span;
        let shape = match &expression.kind {
            CoreExprKind::Name(name) => {
                let shape = self.lookup(name, span)?.unwrap_or(Shape::None);
                if self.capture && shape.has_origin() {
                    return Err(unsupported(
                        span,
                        "capturing a new State declaration source requires an explicit preserved capability contract",
                    ));
                }
                shape
            }
            CoreExprKind::Member { object, member } => {
                let base = self.expr(object, depth + 1)?;
                let base = if let Shape::Address(inner) = &base {
                    inner.as_ref()
                } else {
                    &base
                };
                if let Shape::Package(fields) = base {
                    self.state.snapshot(
                        fields.len().saturating_mul(member.len().saturating_add(1)),
                        span,
                    )?;
                    if let Some((_, field)) = fields.iter().find(|(name, _)| name == member) {
                        self.copy(field, span)?
                    } else {
                        Shape::None
                    }
                } else {
                    Shape::None
                }
            }
            CoreExprKind::Call { callee, arguments } => {
                let target = if let CoreExprKind::TypeApply { callee, .. } = &callee.kind {
                    callee.as_ref()
                } else {
                    callee.as_ref()
                };
                self.expr(target, depth + 1)?;
                self.state.snapshot(arguments.len(), span)?;
                let mut shapes = Vec::new();
                for argument in arguments {
                    shapes.push(self.expr(argument, depth + 1)?);
                }
                if let Some(primitive) = dynamic_store::primitive(target) {
                    if let Some(Shape::Store(element)) = shapes.first() {
                        match primitive {
                            "store_at" | "store_at_mut" => {
                                Shape::Address(Box::new(self.copy(element, span)?))
                            }
                            "store_pop" => Shape::Union {
                                error: Box::new(Shape::None),
                                result: Box::new(self.copy(element, span)?),
                            },
                            "store_push" => Shape::Union {
                                error: Box::new(self.copy(element, span)?),
                                result: Box::new(Shape::None),
                            },
                            _ => Shape::None,
                        }
                    } else {
                        Shape::None
                    }
                } else if let Some(signature) = self.signature(target)? {
                    let mut substitutions = Vec::new();
                    if let CoreExprKind::TypeApply { type_arguments, .. } = &callee.kind {
                        let (Some(bound), [actual]) =
                            (&signature.type_parameter, type_arguments.as_slice())
                        else {
                            return Err(error(
                                "E-TYPE-APPLICATION",
                                span,
                                "ordinary State source forwarding needs one complete type argument",
                            ));
                        };
                        let shape = self.ty(actual, &[], true, depth + 1, span)?;
                        self.state
                            .snapshot(bound.name.len().saturating_add(1), span)?;
                        substitutions.push((bound.name.clone(), shape));
                    } else {
                        for bound in &signature.callable {
                            let argument = shapes.get(bound.index as usize).ok_or_else(|| {
                                error("E-CALL-ARITY", span, "State source argument count differs")
                            })?;
                            let source = if let Some(state) = &signature.state_parameter {
                                let binding = state
                                    .bindings
                                    .iter()
                                    .find(|b| b.generic_index == bound.generic_index)
                                    .ok_or_else(|| {
                                        unsupported(span, "State declaration selector is absent")
                                    })?;
                                argument.field(binding.fields[0]).ok_or_else(|| {
                                    unsupported(span, "State field declaration source was erased")
                                })?
                            } else {
                                argument
                            };
                            self.require(source, bound.kind, span)?;
                            let source = if let Shape::Address(inner) = source {
                                inner.as_ref()
                            } else {
                                source
                            };
                            let copied = self.copy(source, span)?;
                            self.state
                                .snapshot(bound.generic_name.len().saturating_add(1), span)?;
                            substitutions.push((bound.generic_name.clone(), copied));
                        }
                        if signature.callable.is_empty() && shapes.iter().any(Shape::has_origin) {
                            return Err(unsupported(
                                span,
                                "State declaration sources cannot cross an erased ordinary signature",
                            ));
                        }
                    }
                    self.ty(&signature.result, &substitutions, false, depth + 1, span)?
                } else {
                    if foreign::callback_constructor(target) && shapes.iter().any(Shape::has_origin)
                    {
                        return Err(unsupported(
                            span,
                            "State declaration sources cannot become function pointers",
                        ));
                    }
                    Shape::None
                }
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => Shape::Address(Box::new(self.expr(operand, depth + 1)?)),
            CoreExprKind::Unary { operand, .. } | CoreExprKind::CompileTime(operand) => {
                self.expr(operand, depth + 1)?
            }
            CoreExprKind::Try(value) => match self.expr(value, depth + 1)? {
                Shape::Union { result, .. } => *result,
                shape if !shape.has_origin() => Shape::None,
                _ => {
                    return Err(unsupported(
                        span,
                        "State success source requires an explicit error-union shape",
                    ));
                }
            },
            CoreExprKind::Ascribed { ty, value } => {
                let actual = self.expr(value, depth + 1)?;
                let annotated = self.ty(ty, &[], true, depth + 1, span)?;
                self.same_origin(&actual, &annotated, span)?;
                annotated
            }
            CoreExprKind::Record { ty, fields } => {
                for (_, value) in fields {
                    self.expr(value, depth + 1)?;
                }
                self.ty(ty, &[], true, depth + 1, span)?
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                let base = self.expr(base, depth + 1)?;
                for (_, value) in fields {
                    self.expr(value, depth + 1)?;
                }
                if let Some(ty) = ty {
                    self.ty(ty, &[], true, depth + 1, span)?
                } else {
                    base
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.state.snapshot(1, span)?;
                self.scopes.push(BTreeMap::new());
                for (name, value) in bindings {
                    let shape = self.expr(value, depth + 1)?;
                    self.bind(name, shape, span)?;
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
                self.same_origin(&a, &b, span)?;
                a
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } => {
                let previous = self.capture;
                self.capture = true;
                self.state.snapshot(1, span)?;
                self.scopes.push(BTreeMap::new());
                for parameter in parameters {
                    self.bind(&parameter.name, Shape::None, parameter.span)?;
                }
                self.block(body, depth + 1)?;
                self.scopes.pop();
                self.capture = previous;
                Shape::None
            }
            CoreExprKind::IntegerConvert { value, .. } => {
                self.expr(value, depth + 1)?;
                Shape::None
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                let a = self.expr(left, depth + 1)?;
                let b = self.expr(right, depth + 1)?;
                if a.has_origin() || b.has_origin() {
                    return Err(unsupported(
                        span,
                        "this operation does not preserve a State declaration source",
                    ));
                }
                Shape::None
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for value in values {
                    if self.expr(value, depth + 1)?.has_origin() {
                        return Err(unsupported(
                            span,
                            "nested State declaration source containers require an explicit source contract",
                        ));
                    }
                }
                Shape::None
            }
            CoreExprKind::Slice { object, start, end } => {
                if self.expr(object, depth + 1)?.has_origin() {
                    return Err(unsupported(
                        span,
                        "State declaration source slicing is not implemented",
                    ));
                }
                for value in [start, end].into_iter().flatten() {
                    self.expr(value, depth + 1)?;
                }
                Shape::None
            }
            _ => Shape::None,
        };
        self.state.snapshot(
            shape
                .work()
                .saturating_add(helpers::plan::tree_work(self.expressions.len())),
            span,
        )?;
        if self.expressions.len() >= self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "State declaration use table exceeded",
                Some(span),
            ));
        }
        if let Some(previous) = self.expressions.get(&expression.id) {
            self.state.snapshot(previous.work(), span)?;
            if previous != &shape {
                return Err(error(
                    "E-STATE-ORIGIN",
                    span,
                    "one source node identifies conflicting State declaration origins",
                ));
            }
        } else {
            self.expressions.insert(expression.id, shape.clone());
        }
        Ok(shape)
    }
    fn same_origin(&mut self, a: &Shape, b: &Shape, span: Span) -> R<()> {
        self.state
            .snapshot(a.work().saturating_add(b.work()), span)?;
        if (a.has_origin() || b.has_origin()) && a != b {
            return Err(error(
                "E-STATE-ORIGIN",
                span,
                "State declaration source cannot be erased or changed by an annotation, assignment or branch",
            ));
        }
        Ok(())
    }
    fn pattern(&mut self, pattern: &CorePattern, shape: &Shape, depth: usize) -> R<()> {
        self.node(depth, pattern.span)?;
        if let CorePatternKind::Name(name) = &pattern.kind {
            let shape = self.copy(shape, pattern.span)?;
            self.bind(name, shape, pattern.span)?;
        } else if shape.has_origin() {
            return Err(unsupported(
                pattern.span,
                "this pattern does not preserve a State declaration source",
            ));
        }
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
                        Shape::None
                    };
                    let shape = if let Some(ty) = ty {
                        let shape = self.ty(ty, &[], true, depth + 1, statement.span)?;
                        // store_new has no value arguments; its declared full
                        // element type is the source of the fresh Store shape.
                        if initializer.as_ref().is_none_or(|value| !matches!(&value.kind, CoreExprKind::Call { callee, .. } if dynamic_store::primitive(callee) == Some("store_new"))) {
                            self.same_origin(&inferred, &shape, statement.span)?;
                        }
                        shape
                    } else {
                        inferred
                    };
                    self.bind(name, shape, statement.span)?;
                }
                CoreStatementKind::Assign { target, value } => {
                    let a = self.expr(target, depth + 1)?;
                    let b = self.expr(value, depth + 1)?;
                    self.same_origin(&a, &b, statement.span)?;
                }
                CoreStatementKind::Return(Some(value)) | CoreStatementKind::Expression(value) => {
                    self.expr(value, depth + 1)?;
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
                CoreStatementKind::While { condition, body } => {
                    self.expr(condition, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let shape = self.expr(value, depth + 1)?;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.pattern(pattern, &shape, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    self.scopes.pop();
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let shape = self.expr(value, depth + 1)?;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.pattern(pattern, &shape, depth + 1)?;
                    self.block(body, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                    ..
                } => {
                    self.expr(start, depth + 1)?;
                    self.expr(end, depth + 1)?;
                    self.state.snapshot(1, statement.span)?;
                    self.scopes.push(BTreeMap::new());
                    self.bind(name, Shape::None, statement.span)?;
                    self.block(body, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::Match { value, arms } => {
                    let shape = self.expr(value, depth + 1)?;
                    for arm in arms {
                        self.state.snapshot(1, statement.span)?;
                        self.scopes.push(BTreeMap::new());
                        self.pattern(&arm.pattern, &shape, depth + 1)?;
                        self.block(&arm.body, depth + 1)?;
                        self.scopes.pop();
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
}
