//! Definition-time error-arm choices are tied to syntax sites, not to the
//! substituted payload type. This matters when distinct formals become equal.
use super::*;

#[derive(Default)]
pub(super) struct Choices {
    pub expression: Option<bool>,
    pub statements: BTreeMap<usize, bool>,
}

pub(super) fn statement_ids(body: &FunctionBody, budget: &mut Budget<'_>) -> R<BTreeMap<usize, usize>> {
    use super::super::ct_selection::Visitor;
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
            self.ids.insert(statement as *const Stmt as usize, self.ids.len());
            super::super::ct_selection::walk_statement(self, statement, depth)
        }
    }
    let mut visitor = Ordinals { budget, ids: BTreeMap::new() };
    if let FunctionBody::Block(block) = body {
        visitor.block(block, 0).map_err(|mut e| e.remove(0))?;
    }
    Ok(visitor.ids)
}

fn wrap(value: &mut Expr, error: &TypeRef, result: &TypeRef, is_error: bool, budget: &mut Budget<'_>) -> R<()> {
    budget.expression(value)?;
    budget.ty(error)?;
    budget.ty(result)?;
    let span = value.span;
    let payload = std::mem::replace(value, Expr { kind: ExprKind::Tuple(vec![]), span });
    let name = if is_error { "error_err" } else { "error_ok" };
    *value = Expr {
        kind: ExprKind::Call {
            callee: Box::new(Expr {
                kind: ExprKind::TypeApply {
                    callee: Box::new(Expr { kind: ExprKind::Name(name.into()), span }),
                    arguments: vec![GenericArg::Type(error.clone()), GenericArg::Type(result.clone())],
                },
                span,
            }),
            arguments: vec![Argument { spread: false, value: payload, span }],
        },
        span,
    };
    Ok(())
}

pub(super) fn apply(function: &mut Function, choices: &Choices, budget: &mut Budget<'_>) -> R<()> {
    if choices.expression.is_none() && choices.statements.is_empty() {
        return Ok(());
    }
    let Some(TypeKind::Error { error, result }) = function.return_type.as_ref().map(|t| &grouped(t).kind) else {
        // Local annotated unions may be present even when the result is ordinary.
        return apply_body(function, choices, None, budget);
    };
    let pair = Some(((**error).clone(), (**result).clone()));
    apply_body(function, choices, pair, budget)
}

fn apply_body(function: &mut Function, choices: &Choices, return_pair: Option<(TypeRef, TypeRef)>, budget: &mut Budget<'_>) -> R<()> {
    if let (FunctionBody::Expression(value), Some(is_error), Some((error, result))) = (&mut function.body, choices.expression, &return_pair) {
        wrap(value, error, result, is_error, budget)?;
    }
    let FunctionBody::Block(block) = &mut function.body else { return Ok(()); };
    let mut next = 0;
    rewrite_block(block, choices, &return_pair, &mut next, budget)?;
    if next < choices.statements.len() || choices.statements.keys().next_back().is_some_and(|last| *last >= next) {
        return Err(error(function.span, "selected generic error arm lost its source site"));
    }
    Ok(())
}

fn rewrite_block(block: &mut Block, choices: &Choices, return_pair: &Option<(TypeRef, TypeRef)>, next: &mut usize, budget: &mut Budget<'_>) -> R<()> {
    for statement in &mut block.statements {
        let site = *next;
        *next += 1;
        if let Some(&is_error) = choices.statements.get(&site) {
            match &mut statement.kind {
                StmtKind::Return(Some(value)) => {
                    let (error, result) = return_pair.as_ref().ok_or_else(|| error(statement.span, "selected error return lacks union type"))?;
                    wrap(value, error, result, is_error, budget)?;
                }
                StmtKind::Binding { ty: Some(ty), value, .. } => {
                    let TypeKind::Error { error, result } = &grouped(ty).kind else { return Err(error(statement.span, "selected error binding lacks union type")); };
                    wrap(value, error, result, is_error, budget)?;
                }
                _ => return Err(error(statement.span, "selected generic error arm changed statement kind")),
            }
        }
        match &mut statement.kind {
            StmtKind::If { then_body, else_body, .. } => {
                rewrite_block(then_body, choices, return_pair, next, budget)?;
                match else_body {
                    Some(ElseBranch::Block(body)) => rewrite_block(body, choices, return_pair, next, budget)?,
                    Some(ElseBranch::If(statement)) => rewrite_one(statement, choices, return_pair, next, budget)?,
                    None => {}
                }
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } | StmtKind::Loop(body)
            | StmtKind::With { body, .. } | StmtKind::Unsafe(body) | StmtKind::Defer(body) => rewrite_block(body, choices, return_pair, next, budget)?,
            StmtKind::Match { arms, .. } => for arm in arms {
                if let ArrowBody::Block(body) = &mut arm.body { rewrite_block(body, choices, return_pair, next, budget)?; }
            },
            StmtKind::Meta(MetaStatement::If { then_body, else_body, .. }) => {
                rewrite_block(then_body, choices, return_pair, next, budget)?;
                if let Some(body) = else_body { rewrite_block(body, choices, return_pair, next, budget)?; }
            }
            StmtKind::Meta(MetaStatement::For { body, .. }) => rewrite_block(body, choices, return_pair, next, budget)?,
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_one(statement: &mut Stmt, choices: &Choices, return_pair: &Option<(TypeRef, TypeRef)>, next: &mut usize, budget: &mut Budget<'_>) -> R<()> {
    // Else-if is one statement in the same preorder as the definition visitor.
    let block = Block { statements: vec![statement.clone()], span: statement.span };
    budget.node(statement.span, 0)?;
    let mut block = block;
    rewrite_block(&mut block, choices, return_pair, next, budget)?;
    *statement = block.statements.remove(0);
    Ok(())
}
