//! Original closure declarations and semantic free variables. These are syntax
//! recipes; Types still checks the actual capture directory and emitted body.
use super::*;
use std::collections::BTreeMap;

pub(super) struct ClosureUse {
    pub path: Vec<u32>,
    pub ty: SemanticTypeRecipe,
}
pub(super) struct ClosureTemplate {
    pub owner: usize,
    pub ordinal: u32,
    pub definition: SemanticDefinition,
    pub environment: Vec<SemanticActualRecipe>,
    pub uses: Vec<ClosureUse>,
}
impl Builder<'_, '_, '_, '_> {
    fn simple_value(
        &mut self,
        value: &s::Expr,
        env: &BTreeMap<String, SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.budget.node(value.span, depth)?;
        match &value.kind {
            s::ExprKind::Group(inner) => self.simple_value(inner, env, depth + 1),
            s::ExprKind::Name(name) => {
                let ty = env.get(name).ok_or_else(|| {
                    unsupported(value.span, "closure value needs its original lexical type")
                })?;
                prepay_recipe(ty, self.budget, value.span, depth + 1)?;
                Ok(ty.clone())
            }
            s::ExprKind::Literal(s::Literal::Integer(text)) => {
                let mut ty = Type::I32;
                for (suffix, atom) in [
                    ("usize", Type::Usize),
                    ("isize", Type::Isize),
                    ("u64", Type::U64),
                    ("i64", Type::I64),
                    ("u32", Type::U32),
                    ("i32", Type::I32),
                    ("u16", Type::U16),
                    ("i16", Type::I16),
                    ("u8", Type::U8),
                    ("i8", Type::I8),
                ] {
                    self.budget.text(text, value.span)?;
                    if text.ends_with(suffix) {
                        ty = atom;
                        break;
                    }
                }
                Ok(SemanticTypeRecipe::Atom(ty))
            }
            s::ExprKind::Literal(s::Literal::Bool(_)) => Ok(SemanticTypeRecipe::Atom(Type::Bool)),
            s::ExprKind::Binary { left, right, .. } => {
                let left = self.simple_value(left, env, depth + 1)?;
                let right = self.simple_value(right, env, depth + 1)?;
                prepay_recipe(&left, self.budget, value.span, depth + 1)?;
                prepay_recipe(&right, self.budget, value.span, depth + 1)?;
                if left != right {
                    return Err(unsupported(
                        value.span,
                        "closure arithmetic needs complete source operand constraints",
                    ));
                }
                Ok(left)
            }
            _ => Err(unsupported(
                value.span,
                "closure result needs its complete original expected signature",
            )),
        }
    }
    fn closure_environment(
        &mut self,
        expression: &s::Expr,
        env: &BTreeMap<String, SemanticTypeRecipe>,
    ) -> R<(Vec<SemanticActualRecipe>, Vec<ClosureUse>)> {
        use crate::ct_selection::Visitor;
        let s::ExprKind::Closure { params, body, .. } = &expression.kind else {
            unreachable!()
        };
        struct Free<'a, 'b, 'l> {
            names: Vec<String>,
            paths: Vec<Vec<u32>>,
            path: Vec<u32>,
            bound: &'a [s::ClosureParam],
            env: &'a BTreeMap<String, SemanticTypeRecipe>,
            budget: &'b mut Budget<'l>,
        }
        impl Visitor for Free<'_, '_, '_> {
            fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                self.budget.node(span, depth).map_err(|e| vec![e])
            }
            fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
                self.budget.text(text, span).map_err(|e| vec![e])
            }
            fn expression(&mut self, e: &s::Expr, depth: usize) -> CompileResult<()> {
                self.node(e.span, depth)?;
                if let s::ExprKind::Name(name) = &e.kind {
                    self.text(name, e.span)?;
                    for parameter in self.bound {
                        self.budget
                            .text(&parameter.name, e.span)
                            .map_err(|e| vec![e])?;
                    }
                    for previous in &self.names {
                        self.budget.text(previous, e.span).map_err(|e| vec![e])?;
                    }
                    for key in self.env.keys() {
                        self.budget.text(key, e.span).map_err(|e| vec![e])?;
                    }
                    self.budget
                        .identity_work(
                            self.bound.len() + self.names.len() + self.env.len() + 1,
                            e.span,
                        )
                        .map_err(|e| vec![e])?;
                    if !self.bound.iter().any(|p| p.name == *name)
                        && self.env.contains_key(name)
                        && !self.names.contains(name)
                    {
                        self.budget
                            .lineage_bytes(std::mem::size_of::<String>() + name.len(), e.span)
                            .map_err(|e| vec![e])?;
                        self.budget
                            .lineage_bytes(self.path.len() * std::mem::size_of::<u32>(), e.span)
                            .map_err(|e| vec![e])?;
                        self.paths.push(self.path.clone());
                        self.names.push(name.clone());
                    }
                }
                let children: Vec<(u32, &s::Expr)> = match &e.kind {
                    s::ExprKind::Name(_) | s::ExprKind::Literal(_) => Vec::new(),
                    s::ExprKind::Member { object, .. } => vec![(0, object.as_ref())],
                    s::ExprKind::Binary { left, right, .. } => {
                        vec![(1, left.as_ref()), (2, right.as_ref())]
                    }
                    s::ExprKind::Unary { operand, .. } => vec![(1, operand.as_ref())],
                    s::ExprKind::Group(inner) => vec![(0, inner.as_ref())],
                    s::ExprKind::Ascribed { value, .. } => vec![(1, value.as_ref())],
                    _ => {
                        return Err(vec![unsupported(
                            e.span,
                            "capture occurrence needs its complete original expression path",
                        )]);
                    }
                };
                for (field, child) in children {
                    self.path.push(field);
                    let result = self.expression(child, depth + 1);
                    self.path.pop();
                    result?;
                }
                Ok(())
            }
        }
        let mut free = Free {
            names: Vec::new(),
            paths: Vec::new(),
            path: vec![0],
            bound: params,
            env,
            budget: self.budget,
        };
        match body {
            s::ArrowBody::Expression(value) => {
                free.expression(value, 0).map_err(|mut e| e.remove(0))?
            }
            s::ArrowBody::Block(_) => {
                return Err(unsupported(
                    expression.span,
                    "closure block requires complete lexical capture scopes",
                ));
            }
        }
        let names = free.names;
        let paths = free.paths;
        let mut actuals = Vec::new();
        fn dependencies(
            value: &SemanticTypeRecipe,
            out: &mut Vec<SemanticActualRecipe>,
            budget: &mut Budget<'_>,
            span: Span,
            depth: usize,
        ) -> R<()> {
            budget.node(span, depth)?;
            match value {
                SemanticTypeRecipe::Atom(_) => {}
                SemanticTypeRecipe::Formal(reference) => {
                    let actual =
                        SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference.clone()));
                    budget.identity_work(out.len() + 1, span)?;
                    if !out.contains(&actual) {
                        budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(), span)?;
                        out.push(actual);
                    }
                }
                SemanticTypeRecipe::Nominal { arguments, .. } => {
                    for argument in arguments {
                        match argument {
                            SemanticActualRecipe::Type(ty) => {
                                dependencies(ty, out, budget, span, depth + 1)?
                            }
                            SemanticActualRecipe::Life(life) => {
                                budget.identity_work(out.len() + 1, span)?;
                                if !out.contains(argument) {
                                    prepay_recipe(life, budget, span, depth + 1)?;
                                    budget.lineage_bytes(
                                        std::mem::size_of::<SemanticActualRecipe>(),
                                        span,
                                    )?;
                                    out.push(argument.clone());
                                }
                            }
                            SemanticActualRecipe::Const { .. } => {
                                return Err(unsupported(
                                    span,
                                    "captured const type requires its complete ghost carrier binding",
                                ));
                            }
                        }
                    }
                }
                SemanticTypeRecipe::Tuple(fields) => {
                    for field in fields {
                        dependencies(field, out, budget, span, depth + 1)?;
                    }
                }
                _ => {
                    return Err(unsupported(
                        span,
                        "closure capture requires its complete original semantic dependency tree",
                    ));
                }
            }
            Ok(())
        }
        for name in &names {
            dependencies(&env[name], &mut actuals, self.budget, expression.span, 0)?;
        }
        fn closed_capture(
            ty: &SemanticTypeRecipe,
            actuals: &[SemanticActualRecipe],
            budget: &mut Budget<'_>,
            span: Span,
        ) -> R<SemanticTypeRecipe> {
            budget.node(span, 0)?;
            match ty {
                SemanticTypeRecipe::Atom(_) => {
                    prepay_recipe(ty, budget, span, 0)?;
                    Ok(ty.clone())
                }
                SemanticTypeRecipe::Formal(reference) => {
                    let index=actuals.iter().position(|a|matches!(a,SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(r))if r==reference)).ok_or_else(||error(span,"captured whole Type has no sealed ghost source"))?;
                    Ok(SemanticTypeRecipe::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: index as u32,
                    })
                }
                SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                } => {
                    let mut result = Vec::new();
                    for argument in arguments {
                        let value = match argument {
                            SemanticActualRecipe::Type(ty) => SemanticActualRecipe::Type(
                                closed_capture(ty, actuals, budget, span)?,
                            ),
                            SemanticActualRecipe::Life(_) => {
                                let index = actuals.iter().position(|a| a == argument).ok_or_else(
                                    || error(span, "captured life has no sealed ghost source"),
                                )?;
                                SemanticActualRecipe::Life(SemanticLifeRecipe::Environment {
                                    scope: SemanticScopeRef::Ancestor(0),
                                    ordinal: index as u32,
                                })
                            }
                            _ => {
                                return Err(unsupported(
                                    span,
                                    "captured Const needs its original closed environment recipe",
                                ));
                            }
                        };
                        budget.lineage_bytes(std::mem::size_of::<SemanticActualRecipe>(), span)?;
                        result.push(value);
                    }
                    Ok(SemanticTypeRecipe::Nominal {
                        definition: *definition,
                        arguments: result,
                    })
                }
                _ => Err(unsupported(
                    span,
                    "captured semantic type needs complete environment relocation",
                )),
            }
        }
        let mut uses = Vec::new();
        for (name, path) in names.into_iter().zip(paths) {
            let ty = closed_capture(&env[&name], &actuals, self.budget, expression.span)?;
            self.push(&mut uses, ClosureUse { path, ty }, expression.span)?;
        }
        Ok((actuals, uses))
    }
    pub(super) fn discover_closures(
        &mut self,
        function: &s::Function,
    ) -> R<Option<SemanticTypeRecipe>> {
        let mut env = BTreeMap::new();
        for (i, p) in function.params.iter().enumerate() {
            let ty = self.ty(&p.ty, &mut vec![1, i as u32], 0, false)?;
            self.budget.lineage_bytes(
                p.name.len() + std::mem::size_of::<(String, SemanticTypeRecipe)>(),
                p.span,
            )?;
            env.insert(p.name.clone(), ty);
        }
        let mut opaque = None;
        let s::FunctionBody::Block(body) = &function.body else {
            return Ok(None);
        };
        for (ordinal, statement) in body.statements.iter().enumerate() {
            let value = match &statement.kind {
                s::StmtKind::Binding { value, .. } => Some(value),
                s::StmtKind::Return(Some(value)) => Some(value),
                _ => None,
            };
            if let Some(expression) = value
                && let s::ExprKind::Closure {
                    params,
                    return_type,
                    body,
                    ..
                } = &expression.kind
            {
                let (environment, uses) = self.closure_environment(expression, &env)?;
                let mut inputs = Vec::new();
                self.budget.lineage_bytes(
                    env.len()
                        .saturating_add(11)
                        .saturating_mul(2 * std::mem::size_of::<(String, SemanticTypeRecipe)>()),
                    expression.span,
                )?;
                for (name, ty) in &env {
                    self.budget.text(name, expression.span)?;
                    self.budget.lineage_bytes(name.len(), expression.span)?;
                    prepay_recipe(ty, self.budget, expression.span, 0)?;
                }
                let mut inner = env.clone();
                for (i, p) in params.iter().enumerate() {
                    let annotation = p.ty.as_ref().ok_or_else(|| {
                        unsupported(
                            p.span,
                            "closure parameter needs its actual source annotation",
                        )
                    })?;
                    let ty = self.ty(
                        annotation,
                        &mut vec![10, ordinal as u32, i as u32],
                        0,
                        false,
                    )?;
                    prepay_recipe(&ty, self.budget, p.span, 0)?;
                    self.push(
                        &mut inputs,
                        SemanticParameterRecipe {
                            ty: ty.clone(),
                            mode: PassingMode::Infer,
                        },
                        p.span,
                    )?;
                    self.budget.lineage_bytes(
                        p.name.len() + std::mem::size_of::<(String, SemanticTypeRecipe)>(),
                        p.span,
                    )?;
                    inner.insert(p.name.clone(), ty);
                }
                let result = if let Some(ty) = return_type {
                    self.ty(
                        ty,
                        &mut vec![10, ordinal as u32, params.len() as u32],
                        0,
                        false,
                    )?
                } else if let Some(s::TypeRef {
                    kind: s::TypeKind::Impl(bounds),
                    ..
                }) = &function.return_type
                {
                    let [bound] = bounds.as_slice() else {
                        return Err(unsupported(
                            expression.span,
                            "opaque closure needs its complete callable result bound",
                        ));
                    };
                    let s::TypeKind::Named { path, args } = &bound.kind else {
                        return Err(unsupported(
                            bound.span,
                            "opaque result has no original callable bound",
                        ));
                    };
                    if !matches!(path.as_slice(),[name] if matches!(name.as_str(),"Fn"|"FnMut"|"FnOnce"))
                    {
                        return Err(unsupported(
                            bound.span,
                            "opaque closure result is not callable",
                        ));
                    }
                    let [s::GenericArg::Type(_), s::GenericArg::Type(result)] = args.as_slice()
                    else {
                        return Err(error(bound.span, "opaque callable signature is incomplete"));
                    };
                    self.ty(result, &mut vec![2], 0, false)?
                } else {
                    match body {
                        s::ArrowBody::Expression(value) => self.simple_value(value, &inner, 0)?,
                        _ => {
                            return Err(unsupported(
                                expression.span,
                                "closure block result needs complete source constraints",
                            ));
                        }
                    }
                };
                if !matches!(&result,SemanticTypeRecipe::Atom(ty) if ty.is_b0_scalar() || *ty==Type::Unit)
                {
                    return Err(unsupported(
                        expression.span,
                        "closure result scope needs its full child environment substitution",
                    ));
                }
                let source = &self.inventory.declarations[self.owner];
                let next = self
                    .next_body_nodes
                    .iter_mut()
                    .find(|(module, _)| *module == source.module)
                    .ok_or_else(|| {
                        error(expression.span, "body declaration namespace is absent")
                    })?;
                let id = SemanticDefinitionId {
                    module: source.module,
                    index: next.1,
                };
                next.1 = next.1.checked_add(1).ok_or_else(|| {
                    error(expression.span, "body declaration namespace exhausted")
                })?;
                let mut identity = self.identity(self.owner)?;
                self.push(
                    &mut identity.declaration_path,
                    "closure".into(),
                    expression.span,
                )?;
                // An authored closure is a lexical child, not a generated
                // declaration event. Keep the real source generation path.
                self.budget.lineage_bytes(10, expression.span)?;
                let lexical = ordinal.to_string();
                let lexical = self.text(&lexical, expression.span)?;
                self.push(&mut identity.declaration_path, lexical, expression.span)?;
                let mut fields = Vec::new();
                for actual in &environment {
                    let kind = match actual {
                        SemanticActualRecipe::Life(_) => SemanticFormal::Lifetime,
                        SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference)) => {
                            if reference.scope != SemanticScopeRef::Ancestor(0) {
                                return Err(unsupported(
                                    expression.span,
                                    "closure ghost Type requires full lexical kind relocation",
                                ));
                            }
                            let formal = self.inventory.declarations[self.owner]
                                .parameters()
                                .get(reference.ordinal as usize)
                                .ok_or_else(|| {
                                    error(expression.span, "closure ghost formal is absent")
                                })?;
                            let s::GenericParamKind::Type(bounds) = &formal.kind else {
                                return Err(error(
                                    expression.span,
                                    "closure ghost formal kind mismatch",
                                ));
                            };
                            let mut requirements = Vec::new();
                            for bound in bounds {
                                let r = self.requirement(
                                    SemanticTypeRecipe::Formal(reference.clone()),
                                    bound,
                                    &mut vec![10, ordinal as u32],
                                    0,
                                )?;
                                self.push(&mut requirements, r, bound.span)?;
                            }
                            SemanticFormal::Type { requirements }
                        }
                        _ => {
                            return Err(unsupported(
                                expression.span,
                                "closure ghost kind needs its full original formal",
                            ));
                        }
                    };
                    prepay_recipe(actual, self.budget, expression.span, 0)?;
                    self.push(
                        &mut fields,
                        SemanticEnvironmentFormal {
                            kind,
                            source: actual.clone(),
                        },
                        expression.span,
                    )?;
                }
                let definition = SemanticDefinition {
                    id,
                    identity,
                    parent: Some(self.id(self.owner)),
                    kind: SemanticDefinitionKind::Closure,
                    parameters: Vec::new(),
                    implicit_lifetimes: if matches!(
                        expression.kind,
                        s::ExprKind::Closure {
                            move_capture: true,
                            ..
                        }
                    ) {
                        Vec::new()
                    } else {
                        let mut result = Vec::new();
                        for (ordinal, usage) in uses.iter().enumerate() {
                            self.budget.lineage_bytes(
                                std::mem::size_of::<SemanticImplicitLifetime>()
                                    + usage.path.len() * 4,
                                expression.span,
                            )?;
                            result.push(SemanticImplicitLifetime {
                                ordinal: ordinal as u32,
                                occurrence_path: usage.path.clone(),
                            });
                        }
                        result
                    },
                    requirements: Vec::new(),
                    environment: fields,
                    body: SemanticDefinitionBody::Function {
                        signature: SemanticSignatureRecipe {
                            bound_lifetimes: 0,
                            invocation_lifetimes: 0,
                            parameters: inputs,
                            result,
                            result_mode: PassingMode::Infer,
                            call_kind: CallKind::Direct,
                        },
                        sites: Vec::new(),
                    },
                };
                for actual in &environment {
                    prepay_recipe(actual, self.budget, expression.span, 0)?;
                }
                let recipe = SemanticTypeRecipe::Closure {
                    definition: id,
                    arguments: Vec::new(),
                    environment: environment.clone(),
                };
                self.push_closure(
                    ClosureTemplate {
                        owner: self.owner,
                        ordinal: ordinal as u32,
                        definition,
                        environment,
                        uses,
                    },
                    expression.span,
                )?;
                if let s::StmtKind::Binding { pattern, .. } = &statement.kind
                    && let s::PatternKind::Name(name) = &pattern.kind
                {
                    self.budget.lineage_bytes(
                        name.len() + std::mem::size_of::<(String, SemanticTypeRecipe)>(),
                        expression.span,
                    )?;
                    env.insert(name.clone(), recipe);
                } else if matches!(statement.kind, s::StmtKind::Return(_)) {
                    opaque = Some(recipe);
                }
            } else if let s::StmtKind::Binding {
                pattern, ty, value, ..
            } = &statement.kind
                && let s::PatternKind::Name(name) = &pattern.kind
            {
                let inferred = if let Some(ty) = ty {
                    Some(self.ty(ty, &mut vec![10, ordinal as u32], 0, false)?)
                } else {
                    match self.simple_value(value, &env, 0) {
                        Ok(ty) => Some(ty),
                        Err(error) if error.category == zil_base::ResultCategory::Unsupported => {
                            Some(SemanticTypeRecipe::Unsupported(
                                "source local type requires actual expression inference".into(),
                            ))
                        }
                        Err(error) => return Err(error),
                    }
                };
                if let Some(ty) = inferred {
                    self.budget.lineage_bytes(
                        name.len() + std::mem::size_of::<(String, SemanticTypeRecipe)>(),
                        statement.span,
                    )?;
                    env.insert(name.clone(), ty);
                }
            }
        }
        Ok(opaque)
    }
    fn push_closure(&mut self, value: ClosureTemplate, span: Span) -> R<()> {
        let mut rows = std::mem::take(&mut self.closures);
        self.push(&mut rows, value, span)?;
        self.closures = rows;
        Ok(())
    }
}
