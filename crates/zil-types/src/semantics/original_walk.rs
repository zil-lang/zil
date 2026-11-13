//! Bounded structural traversal of the original operation grammar. Rewriters
//! use this only for embedded annotation payloads; it performs no inference.
use crate::*;
pub(super) trait Context {
    fn node(&mut self, depth: usize) -> R<()>;
    fn ty(&mut self, value: &mut SemanticTypeRecipe, depth: usize) -> R<()>;
    fn statement(
        &mut self,
        _id: zil_base::NodeId,
        _kind: &SemanticOriginalStatementKind,
        depth: usize,
    ) -> R<()> {
        self.node(depth)
    }
    fn expression(
        &mut self,
        _id: zil_base::NodeId,
        _kind: &SemanticOriginalExprKind,
        depth: usize,
    ) -> R<()> {
        self.node(depth)
    }
}
fn pattern(value: &mut SemanticOriginalPattern, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.expression(value.id, &value.kind, depth)?;
    match &mut value.kind {
        CorePatternKind::Tuple(fields) => {
            for field in fields {
                pattern(field, cx, depth + 1)?;
            }
        }
        CorePatternKind::Record { ty, fields } => {
            cx.ty(ty, depth + 1)?;
            for (_, field) in fields {
                pattern(field, cx, depth + 1)?;
            }
        }
        CorePatternKind::Reference { pattern: field, .. }
        | CorePatternKind::ErrorArm { pattern: field, .. } => pattern(field, cx, depth + 1)?,
        CorePatternKind::Variant { ty, fields, .. } => {
            cx.ty(ty, depth + 1)?;
            for field in fields {
                pattern(field, cx, depth + 1)?;
            }
        }
        CorePatternKind::Name(_)
        | CorePatternKind::Wildcard
        | CorePatternKind::Integer(_)
        | CorePatternKind::Bool(_) => {}
    }
    Ok(())
}
pub(super) fn block(
    value: &mut SemanticOriginalBlock,
    cx: &mut impl Context,
    depth: usize,
) -> R<()> {
    cx.node(depth)?;
    for statement in &mut value.statements {
        cx.statement(statement.id, &statement.kind, depth + 1)?;
        match &mut statement.kind {
            CoreStatementKind::Binding {
                ty, initializer, ..
            } => {
                if let Some(ty) = ty {
                    cx.ty(ty, depth + 2)?;
                }
                if let Some(value) = initializer {
                    expression(value, cx, depth + 2)?;
                }
            }
            CoreStatementKind::TupleBinding {
                pattern: p,
                ty,
                initializer,
                ..
            } => {
                pattern(p, cx, depth + 2)?;
                if let Some(ty) = ty {
                    cx.ty(ty, depth + 2)?;
                }
                expression(initializer, cx, depth + 2)?;
            }
            CoreStatementKind::Assign { target, value } => {
                expression(target, cx, depth + 2)?;
                expression(value, cx, depth + 2)?;
            }
            CoreStatementKind::Expression(value) => expression(value, cx, depth + 2)?,
            CoreStatementKind::Return(value) => {
                if let Some(value) = value {
                    expression(value, cx, depth + 2)?;
                }
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                expression(condition, cx, depth + 2)?;
                block(then_body, cx, depth + 2)?;
                if let Some(body) = else_body {
                    block(body, cx, depth + 2)?;
                }
            }
            CoreStatementKind::IfLet {
                pattern: p,
                value,
                then_body,
                else_body,
            } => {
                pattern(p, cx, depth + 2)?;
                expression(value, cx, depth + 2)?;
                block(then_body, cx, depth + 2)?;
                if let Some(body) = else_body {
                    block(body, cx, depth + 2)?;
                }
            }
            CoreStatementKind::While { condition, body } => {
                expression(condition, cx, depth + 2)?;
                block(body, cx, depth + 2)?;
            }
            CoreStatementKind::WhileLet {
                pattern: p,
                value,
                body,
            } => {
                pattern(p, cx, depth + 2)?;
                expression(value, cx, depth + 2)?;
                block(body, cx, depth + 2)?;
            }
            CoreStatementKind::ForRange {
                start, end, body, ..
            } => {
                expression(start, cx, depth + 2)?;
                expression(end, cx, depth + 2)?;
                block(body, cx, depth + 2)?;
            }
            CoreStatementKind::Match { value, arms } => {
                expression(value, cx, depth + 2)?;
                for arm in arms {
                    cx.node(depth + 2)?;
                    pattern(&mut arm.pattern, cx, depth + 3)?;
                    block(&mut arm.body, cx, depth + 3)?;
                }
            }
            CoreStatementKind::Block(body)
            | CoreStatementKind::Defer(body)
            | CoreStatementKind::Unsafe(body) => block(body, cx, depth + 2)?,
            CoreStatementKind::Break
            | CoreStatementKind::Continue
            | CoreStatementKind::Unsupported(_) => {}
        }
    }
    Ok(())
}
fn expression(value: &mut SemanticOriginalExpr, cx: &mut impl Context, depth: usize) -> R<()> {
    cx.node(depth)?;
    match &mut value.kind {
        CoreExprKind::MakeDynBorrow { source, .. } | CoreExprKind::MakeDynOwned { source, .. } => {
            expression(source, cx, depth + 1)?
        }
        CoreExprKind::CallDynSlot {
            callee, arguments, ..
        }
        | CoreExprKind::Call { callee, arguments } => {
            expression(callee, cx, depth + 1)?;
            for argument in arguments {
                expression(argument, cx, depth + 1)?;
            }
        }
        CoreExprKind::Ascribed { ty, value }
        | CoreExprKind::NumericConvert { value, target: ty }
        | CoreExprKind::IntegerConvert { value, target: ty } => {
            cx.ty(ty, depth + 1)?;
            expression(value, cx, depth + 1)?;
        }
        CoreExprKind::Unary { operand, .. }
        | CoreExprKind::CompileTime(operand)
        | CoreExprKind::Try(operand) => expression(operand, cx, depth + 1)?,
        CoreExprKind::Binary { left, right, .. } => {
            expression(left, cx, depth + 1)?;
            expression(right, cx, depth + 1)?;
        }
        CoreExprKind::TypeApply {
            callee,
            type_arguments,
        } => {
            expression(callee, cx, depth + 1)?;
            for argument in type_arguments {
                cx.ty(argument, depth + 1)?;
            }
        }
        CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
            for value in values {
                expression(value, cx, depth + 1)?;
            }
        }
        CoreExprKind::Index { object, index } => {
            expression(object, cx, depth + 1)?;
            expression(index, cx, depth + 1)?;
        }
        CoreExprKind::Slice { object, start, end } => {
            expression(object, cx, depth + 1)?;
            if let Some(value) = start {
                expression(value, cx, depth + 1)?;
            }
            if let Some(value) = end {
                expression(value, cx, depth + 1)?;
            }
        }
        CoreExprKind::Member { object, owner, .. } => {
            expression(object, cx, depth + 1)?;
            if let Some(owner) = owner {
                cx.ty(owner, depth + 1)?;
            }
        }
        CoreExprKind::Record { ty, fields } => {
            cx.ty(ty, depth + 1)?;
            for (_, value) in fields {
                expression(value, cx, depth + 1)?;
            }
        }
        CoreExprKind::RecordUpdate { ty, base, fields } => {
            if let Some(ty) = ty {
                cx.ty(ty, depth + 1)?;
            }
            expression(base, cx, depth + 1)?;
            for (_, value) in fields {
                expression(value, cx, depth + 1)?;
            }
        }
        CoreExprKind::Sequence { bindings, value } => {
            for (_, value) in bindings {
                expression(value, cx, depth + 1)?;
            }
            expression(value, cx, depth + 1)?;
        }
        CoreExprKind::Closure {
            parameters,
            return_type,
            body,
            ..
        } => {
            for parameter in parameters {
                cx.node(depth + 1)?;
                if let Some(ty) = &mut parameter.ty {
                    cx.ty(ty, depth + 2)?;
                }
            }
            if let Some(ty) = return_type {
                cx.ty(ty, depth + 1)?;
            }
            block(body, cx, depth + 1)?;
        }
        CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            expression(condition, cx, depth + 1)?;
            expression(then_value, cx, depth + 1)?;
            expression(else_value, cx, depth + 1)?;
        }
        CoreExprKind::Literal(_)
        | CoreExprKind::Integer(_)
        | CoreExprKind::Float(_)
        | CoreExprKind::Bool(_)
        | CoreExprKind::String(_)
        | CoreExprKind::Unit
        | CoreExprKind::Name(_)
        | CoreExprKind::ReceiverMember(_)
        | CoreExprKind::Unsupported(_) => {}
    }
    Ok(())
}
