//! Definition-time error-arm choices are tied to syntax sites, not to the
//! substituted payload type. This matters when distinct formals become equal.
use super::*;

#[derive(Default)]
pub(super) struct Choices {
    pub expression: Option<bool>,
    pub statements: BTreeMap<usize, (Span, bool)>,
}

pub(super) fn statement_ids(
    body: &FunctionBody,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<usize, usize>> {
    use crate::ct_selection::Visitor;
    struct Ordinals<'a, 'l> {
        budget: &'a mut Budget<'l>,
        ids: BTreeMap<usize, usize>,
    }
    impl Visitor for Ordinals<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn statement(&mut self, statement: &Stmt, depth: usize) -> CompileResult<()> {
            self.node(statement.span, depth)?;
            self.ids
                .insert(statement as *const Stmt as usize, self.ids.len());
            crate::ct_selection::walk_statement(self, statement, depth)
        }
    }
    let mut visitor = Ordinals {
        budget,
        ids: BTreeMap::new(),
    };
    if let FunctionBody::Block(block) = body {
        visitor.block(block, 0).map_err(|mut e| e.remove(0))?;
    }
    Ok(visitor.ids)
}

fn wrap(
    value: &mut Expr,
    error: &TypeRef,
    result: &TypeRef,
    is_error: bool,
    budget: &mut Budget<'_>,
    moved_sites: &mut BTreeMap<usize, usize>,
) -> R<()> {
    budget.const_expression(value)?;
    budget.ty(error)?;
    budget.ty(result)?;
    let span = value.span;
    budget.lineage_bytes(std::mem::size_of::<(usize, usize)>(), span)?;
    let old_site = value as *const Expr as usize;
    let payload = std::mem::replace(
        value,
        Expr {
            kind: ExprKind::Tuple(vec![]),
            span,
        },
    );
    let name = if is_error { "error_err" } else { "error_ok" };
    *value = Expr {
        kind: ExprKind::Call {
            callee: Box::new(Expr {
                kind: ExprKind::TypeApply {
                    callee: Box::new(Expr {
                        kind: ExprKind::Name(name.into()),
                        span,
                    }),
                    arguments: vec![
                        GenericArg::Type(error.clone()),
                        GenericArg::Type(result.clone()),
                    ],
                },
                span,
            }),
            arguments: vec![Argument {
                spread: false,
                value: payload,
                span,
            }],
        },
        span,
    };
    let ExprKind::Call { arguments, .. } = &value.kind else {
        unreachable!()
    };
    moved_sites.insert(old_site, &arguments[0].value as *const Expr as usize);
    Ok(())
}

pub(super) fn apply(
    function: &mut Function,
    choices: &Choices,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<usize, usize>> {
    if choices.expression.is_none() && choices.statements.is_empty() {
        return Ok(BTreeMap::new());
    }
    // This feature-only walk follows every nested statement site, including
    // closure bodies and meta expressions. Prepay the complete current tree.
    budget.function_snapshot(function)?;
    let Some(TypeKind::Error { error, result }) =
        function.return_type.as_ref().map(|t| &grouped(t).kind)
    else {
        // Local annotated unions may be present even when the result is ordinary.
        return apply_body(function, choices, None, budget);
    };
    let pair = Some(((**error).clone(), (**result).clone()));
    apply_body(function, choices, pair, budget)
}

fn apply_body(
    function: &mut Function,
    choices: &Choices,
    return_pair: Option<(TypeRef, TypeRef)>,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<usize, usize>> {
    let mut next = 0;
    let mut moved_sites = BTreeMap::new();
    match &mut function.body {
        FunctionBody::Expression(value) => {
            if let (Some(is_error), Some((error, result))) = (choices.expression, &return_pair) {
                wrap(value, error, result, is_error, budget, &mut moved_sites)?;
            }
            rewrite_expr(
                value,
                choices,
                &return_pair,
                &mut next,
                budget,
                &mut moved_sites,
            )?;
        }
        FunctionBody::Block(block) => rewrite_block(
            block,
            choices,
            &return_pair,
            &mut next,
            budget,
            &mut moved_sites,
        )?,
        FunctionBody::Declaration => {
            return Err(error(
                function.span,
                "selected generic error arm lacks a body",
            ));
        }
    }
    if choices
        .statements
        .keys()
        .next_back()
        .is_some_and(|last| *last >= next)
    {
        return Err(error(
            function.span,
            "selected generic error arm lost its source site",
        ));
    }
    Ok(moved_sites)
}

fn rewrite_block(
    block: &mut Block,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut BTreeMap<usize, usize>,
) -> R<()> {
    for statement in &mut block.statements {
        rewrite_statement(statement, choices, return_pair, next, budget, moved_sites)?;
    }
    Ok(())
}

fn rewrite_statement(
    statement: &mut Stmt,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut BTreeMap<usize, usize>,
) -> R<()> {
    let site = *next;
    *next += 1;
    if let Some(&(original_span, is_error)) = choices.statements.get(&site) {
        if original_span != statement.span {
            return Err(error(
                statement.span,
                "selected generic error arm changed source site",
            ));
        }
        match &mut statement.kind {
            StmtKind::Return(Some(value)) => {
                let (error, result) = return_pair.as_ref().ok_or_else(|| {
                    error(statement.span, "selected error return lacks union type")
                })?;
                wrap(value, error, result, is_error, budget, moved_sites)?;
            }
            StmtKind::Binding {
                ty: Some(ty),
                value,
                ..
            } => {
                let TypeKind::Error { error, result } = &grouped(ty).kind else {
                    return Err(error(
                        statement.span,
                        "selected error binding lacks union type",
                    ));
                };
                wrap(value, error, result, is_error, budget, moved_sites)?;
            }
            _ => {
                return Err(error(
                    statement.span,
                    "selected generic error arm changed statement kind",
                ));
            }
        }
    }
    match &mut statement.kind {
        StmtKind::Binding { value, .. }
        | StmtKind::Return(Some(value))
        | StmtKind::Expression(value)
        | StmtKind::Yield(value) => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::Assign { target, value, .. } => {
            rewrite_expr(target, choices, return_pair, next, budget, moved_sites)?;
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => {
            rewrite_condition(condition, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(then_body, choices, return_pair, next, budget, moved_sites)?;
            match else_body {
                Some(ElseBranch::Block(body)) => {
                    rewrite_block(body, choices, return_pair, next, budget, moved_sites)?
                }
                Some(ElseBranch::If(statement)) => {
                    rewrite_statement(statement, choices, return_pair, next, budget, moved_sites)?
                }
                None => {}
            }
        }
        StmtKind::While { condition, body } => {
            rewrite_condition(condition, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::For { iterable, body, .. }
        | StmtKind::With {
            value: iterable,
            body,
            ..
        } => {
            rewrite_expr(iterable, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::Loop(body) | StmtKind::Unsafe(body) | StmtKind::Defer(body) => {
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?
        }
        StmtKind::Match { value, arms } => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
            for arm in arms {
                rewrite_arrow(
                    &mut arm.body,
                    choices,
                    return_pair,
                    next,
                    budget,
                    moved_sites,
                )?;
            }
        }
        StmtKind::Meta(MetaStatement::If {
            condition,
            then_body,
            else_body,
        }) => {
            rewrite_expr(condition, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(then_body, choices, return_pair, next, budget, moved_sites)?;
            if let Some(body) = else_body {
                rewrite_block(body, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        StmtKind::Meta(MetaStatement::For { iterable, body, .. }) => {
            rewrite_expr(iterable, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::Meta(MetaStatement::Emit(value)) => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?
        }
        _ => {}
    }
    Ok(())
}
fn rewrite_condition(
    condition: &mut Condition,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut BTreeMap<usize, usize>,
) -> R<()> {
    match condition {
        Condition::Expression(value) | Condition::Let { value, .. } => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)
        }
    }
}

fn rewrite_arrow(
    arrow: &mut ArrowBody,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut BTreeMap<usize, usize>,
) -> R<()> {
    match arrow {
        ArrowBody::Block(body) => {
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)
        }
        ArrowBody::Expression(value) => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)
        }
    }
}

fn rewrite_expr(
    value: &mut Expr,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut BTreeMap<usize, usize>,
) -> R<()> {
    match &mut value.kind {
        ExprKind::Group(inner)
        | ExprKind::Try(inner)
        | ExprKind::MetaEval(inner)
        | ExprKind::MetaSplice(inner)
        | ExprKind::Unary { operand: inner, .. } => {
            rewrite_expr(inner, choices, return_pair, next, budget, moved_sites)?
        }
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                rewrite_expr(item, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        ExprKind::Binary { left, right, .. }
        | ExprKind::Pipeline {
            input: left,
            target: right,
        } => {
            rewrite_expr(left, choices, return_pair, next, budget, moved_sites)?;
            rewrite_expr(right, choices, return_pair, next, budget, moved_sites)?;
        }
        ExprKind::Member { object, .. } => {
            rewrite_expr(object, choices, return_pair, next, budget, moved_sites)?
        }
        ExprKind::TypeApply { callee, arguments } => {
            rewrite_expr(callee, choices, return_pair, next, budget, moved_sites)?;
            for argument in arguments {
                if let GenericArg::Const(value) = argument {
                    rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
                }
            }
        }
        ExprKind::Call { callee, arguments } => {
            rewrite_expr(callee, choices, return_pair, next, budget, moved_sites)?;
            for argument in arguments {
                rewrite_expr(
                    &mut argument.value,
                    choices,
                    return_pair,
                    next,
                    budget,
                    moved_sites,
                )?;
            }
        }
        ExprKind::Index { object, index } => {
            rewrite_expr(object, choices, return_pair, next, budget, moved_sites)?;
            rewrite_expr(index, choices, return_pair, next, budget, moved_sites)?;
        }
        ExprKind::Slice { object, start, end } => {
            rewrite_expr(object, choices, return_pair, next, budget, moved_sites)?;
            for part in [start, end].into_iter().flatten() {
                rewrite_expr(part, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        ExprKind::Record { ty, base, fields } => {
            for part in [ty, base].into_iter().flatten() {
                rewrite_expr(part, choices, return_pair, next, budget, moved_sites)?;
            }
            for field in fields {
                if let Some(part) = &mut field.value {
                    rewrite_expr(part, choices, return_pair, next, budget, moved_sites)?;
                }
            }
        }
        ExprKind::Closure { body, .. } => {
            rewrite_arrow(body, choices, return_pair, next, budget, moved_sites)?
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            for part in [condition, then_value, else_value] {
                rewrite_expr(part, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        ExprKind::MetaBlock(body) => {
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?
        }
        ExprKind::Quote { template, .. } => {
            for hole in &mut template.holes {
                rewrite_expr(
                    &mut hole.value,
                    choices,
                    return_pair,
                    next,
                    budget,
                    moved_sites,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}
