//! Address a node only through the path minted by the original body converter.
//! This selects the actual syntax node before cloning/rewrite. It neither uses
//! spans nor searches for a same-shaped value or declaration.
use super::*;
pub(super) enum Node<'a> {
    Expression(&'a mut s::Expr),
    Statement(&'a mut s::Stmt),
    NonRuntime,
}
enum Cursor<'a> {
    Expression(&'a mut s::Expr),
    Statement(&'a mut s::Stmt),
    Block(&'a mut s::Block),
    Holes(&'a mut [s::QuoteHole]),
    NonRuntime,
}
pub(super) struct Located<'a> {
    pub node: Node<'a>,
    pub runtime: bool,
    pub child: bool,
}
fn arrow(body: &mut s::ArrowBody) -> Cursor<'_> {
    match body {
        s::ArrowBody::Expression(e) => Cursor::Expression(e),
        s::ArrowBody::Block(b) => Cursor::Block(b),
    }
}
fn condition(condition: &mut s::Condition) -> &mut s::Expr {
    match condition {
        s::Condition::Expression(e) | s::Condition::Let { value: e, .. } => e,
    }
}
fn step<'a>(
    cursor: Cursor<'a>,
    field: u32,
    runtime: &mut bool,
    child: &mut bool,
) -> Option<Cursor<'a>> {
    let field = field as usize;
    Some(match cursor {
        Cursor::NonRuntime => Cursor::NonRuntime,
        Cursor::Block(block) => Cursor::Statement(block.statements.get_mut(field)?),
        Cursor::Holes(holes) => Cursor::Expression(&mut holes.get_mut(field)?.value),
        Cursor::Statement(statement) => match &mut statement.kind {
            s::StmtKind::Binding { value, .. }
            | s::StmtKind::Expression(value)
            | s::StmtKind::Yield(value)
                if field == 0 =>
            {
                Cursor::Expression(value)
            }
            s::StmtKind::Return(Some(value)) if field == 0 => Cursor::Expression(value),
            s::StmtKind::Assign { target, .. } if field == 0 => Cursor::Expression(target),
            s::StmtKind::Assign { value, .. } if field == 1 => Cursor::Expression(value),
            s::StmtKind::If { condition: c, .. } | s::StmtKind::While { condition: c, .. }
                if field == 0 =>
            {
                Cursor::Expression(condition(c))
            }
            s::StmtKind::If { then_body, .. } if field == 1 => Cursor::Block(then_body),
            s::StmtKind::If {
                else_body: Some(body),
                ..
            } if field == 2 => match body {
                s::ElseBranch::Block(b) => Cursor::Block(b),
                s::ElseBranch::If(s) => Cursor::Statement(s),
            },
            s::StmtKind::While { body, .. }
            | s::StmtKind::For { body, .. }
            | s::StmtKind::With { body, .. }
                if field == 1 =>
            {
                Cursor::Block(body)
            }
            s::StmtKind::For { iterable, .. } if field == 0 => Cursor::Expression(iterable),
            s::StmtKind::With { value, .. } if field == 0 => Cursor::Expression(value),
            s::StmtKind::Match { value, .. } if field == 0 => Cursor::Expression(value),
            s::StmtKind::Match { arms, .. } if field > 0 => {
                arrow(&mut arms.get_mut(field - 1)?.body)
            }
            s::StmtKind::Loop(b) | s::StmtKind::Defer(b) | s::StmtKind::Unsafe(b) if field == 0 => {
                Cursor::Block(b)
            }
            s::StmtKind::Meta(meta) => {
                *runtime = false;
                match meta {
                    s::MetaStatement::If { condition, .. } if field == 0 => {
                        Cursor::Expression(condition)
                    }
                    s::MetaStatement::If { then_body, .. } if field == 1 => {
                        Cursor::Block(then_body)
                    }
                    s::MetaStatement::If {
                        else_body: Some(b), ..
                    } if field == 2 => Cursor::Block(b),
                    s::MetaStatement::For { iterable, .. } if field == 0 => {
                        Cursor::Expression(iterable)
                    }
                    s::MetaStatement::For { body, .. } if field == 1 => Cursor::Block(body),
                    s::MetaStatement::Emit(value) if field == 0 => Cursor::Expression(value),
                    _ => return None,
                }
            }
            _ => return None,
        },
        Cursor::Expression(expression) => match &mut expression.kind {
            s::ExprKind::Group(e) | s::ExprKind::Try(e) | s::ExprKind::Unary { operand: e, .. }
                if field == 0 =>
            {
                Cursor::Expression(e)
            }
            s::ExprKind::Ascribed { value, .. } if field == 0 => Cursor::Expression(value),
            s::ExprKind::Tuple(es) | s::ExprKind::Array(es) => {
                Cursor::Expression(es.get_mut(field)?)
            }
            s::ExprKind::Binary { left, .. } if field == 0 => Cursor::Expression(left),
            s::ExprKind::Binary { right, .. } if field == 1 => Cursor::Expression(right),
            s::ExprKind::Pipeline { input, .. } if field == 0 => Cursor::Expression(input),
            s::ExprKind::Pipeline { target, .. } if field == 1 => Cursor::Expression(target),
            s::ExprKind::Member { object, .. } if field == 0 => Cursor::Expression(object),
            s::ExprKind::TypeApply { callee, .. } if field == 0 => Cursor::Expression(callee),
            s::ExprKind::TypeApply { arguments, .. } if field > 0 => {
                *runtime = false;
                match arguments.get_mut(field - 1)? {
                    s::GenericArg::Const(e) => Cursor::Expression(e),
                    // A lexical Const formal can be written through the
                    // parser's Type-shaped argument. The original body owns
                    // that disambiguation; this is explicitly a type phase,
                    // not a missing runtime value to be guessed or stamped.
                    _ => Cursor::NonRuntime,
                }
            }
            s::ExprKind::Call { callee, .. } if field == 0 => Cursor::Expression(callee),
            s::ExprKind::Call { arguments, .. } if field > 0 => {
                Cursor::Expression(&mut arguments.get_mut(field - 1)?.value)
            }
            s::ExprKind::Index { object, .. } | s::ExprKind::Slice { object, .. } if field == 0 => {
                Cursor::Expression(object)
            }
            s::ExprKind::Index { index, .. } if field == 1 => Cursor::Expression(index),
            s::ExprKind::Slice { start: Some(e), .. } if field == 1 => Cursor::Expression(e),
            s::ExprKind::Slice { end: Some(e), .. } if field == 2 => Cursor::Expression(e),
            s::ExprKind::Record { base: Some(e), .. } if field == 0 => Cursor::Expression(e),
            s::ExprKind::Record { fields, .. } if field > 0 => {
                Cursor::Expression(fields.get_mut(field - 1)?.value.as_mut()?)
            }
            s::ExprKind::Closure { body, .. } if field == 2 => {
                *child = true;
                arrow(body)
            }
            s::ExprKind::If { condition, .. } if field == 0 => Cursor::Expression(condition),
            s::ExprKind::If { then_value, .. } if field == 1 => Cursor::Expression(then_value),
            s::ExprKind::If { else_value, .. } if field == 2 => Cursor::Expression(else_value),
            s::ExprKind::MetaEval(e) | s::ExprKind::MetaSplice(e) if field == 0 => {
                *runtime = false;
                Cursor::Expression(e)
            }
            s::ExprKind::MetaBlock(block) if field == 0 => {
                *runtime = false;
                Cursor::Block(block)
            }
            s::ExprKind::Quote { template, .. } if field == 1 => {
                *runtime = false;
                Cursor::Holes(&mut template.holes)
            }
            _ => return None,
        },
    })
}
pub(super) fn function<'a>(
    body: &'a mut s::FunctionBody,
    path: &[u32],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Option<Located<'a>>> {
    let cursor = match body {
        s::FunctionBody::Block(b) => Cursor::Block(b),
        s::FunctionBody::Expression(e) => Cursor::Expression(e),
        s::FunctionBody::Declaration => return Ok(None),
    };
    locate(cursor, path, budget, span)
}
pub(super) fn arrow_body<'a>(
    body: &'a mut s::ArrowBody,
    path: &[u32],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Option<Located<'a>>> {
    locate(arrow(body), path, budget, span)
}
fn locate<'a>(
    mut cursor: Cursor<'a>,
    path: &[u32],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Option<Located<'a>>> {
    let (mut runtime, mut child) = (true, false);
    for (depth, field) in path.iter().enumerate() {
        budget.node(span, depth)?;
        if let Cursor::Expression(expression) = &mut cursor
            && *field == 0
            && let s::ExprKind::ReceiverMember(member) = &expression.kind
        {
            budget.text(member, expression.span)?;
            budget.lineage_bytes(
                2 * std::mem::size_of::<s::Expr>() + member.len() + 4,
                expression.span,
            )?;
            expression.kind = s::ExprKind::Member {
                object: Box::new(s::Expr {
                    kind: s::ExprKind::Name("self".into()),
                    span: expression.span,
                    semantic: Default::default(),
                }),
                member: s::MemberName::Name(member.clone()),
            };
        }
        // The converter records shorthand as the actual read of this exact
        // field name. Materialize that one desugaring before attaching a
        // request token; a field result is never substituted for its input.
        if let Cursor::Expression(expression) = &mut cursor
            && let s::ExprKind::Record { fields, .. } = &mut expression.kind
            && *field > 0
            && let Some(field) = fields.get_mut(*field as usize - 1)
            && field.value.is_none()
        {
            budget.text(&field.name, field.span)?;
            budget.lineage_bytes(
                std::mem::size_of::<s::Expr>() + field.name.len(),
                field.span,
            )?;
            field.value = Some(s::Expr {
                kind: s::ExprKind::Name(field.name.clone()),
                span: field.span,
                semantic: Default::default(),
            });
        }
        let Some(next) = step(cursor, *field, &mut runtime, &mut child) else {
            return Ok(None);
        };
        cursor = next;
    }
    let node = match cursor {
        Cursor::Expression(e) => Node::Expression(e),
        Cursor::Statement(s) => Node::Statement(s),
        Cursor::NonRuntime => Node::NonRuntime,
        Cursor::Block(_) | Cursor::Holes(_) => return Ok(None),
    };
    Ok(Some(Located {
        node,
        runtime,
        child,
    }))
}
