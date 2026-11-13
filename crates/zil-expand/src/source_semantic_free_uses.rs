//! Lexical runtime free names in the complete original operation tree.
//! No type, capture mode, borrow root or lifetime class is inferred here.
use super::*;
use std::collections::BTreeSet;

struct Free<'a> {
    available: &'a BTreeSet<String>,
    bound: Vec<String>,
    found: Vec<(String, NodeId)>,
}
impl Builder<'_, '_, '_, '_> {
    pub(in crate::source_semantics) fn original_free_uses(
        &mut self,
        body: &SemanticOriginalBlock,
        parameters: &[SemanticOriginalClosureParameter],
        available: &BTreeSet<String>,
    ) -> R<Vec<(String, NodeId)>> {
        let mut free = Free {
            available,
            bound: Vec::new(),
            found: Vec::new(),
        };
        for parameter in parameters {
            let name = self.text(&parameter.name, parameter.span)?;
            self.push(&mut free.bound, name, parameter.span)?;
        }
        self.free_block(body, &mut free, 0)?;
        Ok(free.found)
    }
    fn free_name(&mut self, name: &str, id: NodeId, free: &mut Free<'_>, span: Span) -> R<()> {
        for bound in free.bound.iter().rev() {
            self.budget.text(name, span)?;
            self.budget.text(bound, span)?;
            if bound == name {
                return Ok(());
            }
        }
        // Pay the complete finite parent lexical directory before matching;
        // matching a name itself grants no physical capture or permission.
        let mut available = false;
        for parent in free.available {
            self.budget.text(name, span)?;
            self.budget.text(parent, span)?;
            available |= parent == name;
        }
        if !available {
            return Ok(());
        }
        for (previous, _) in &free.found {
            self.budget.text(name, span)?;
            self.budget.text(previous, span)?;
            if previous == name {
                return Ok(());
            }
        }
        let name = self.text(name, span)?;
        self.push(&mut free.found, (name, id), span)
    }
    fn free_bind_pattern(
        &mut self,
        pattern: &SemanticOriginalPattern,
        free: &mut Free<'_>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(pattern.span, depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                let name = self.text(name, pattern.span)?;
                self.push(&mut free.bound, name, pattern.span)?;
            }
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                for field in fields {
                    self.free_bind_pattern(field, free, depth + 1)?;
                }
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    self.free_bind_pattern(field, free, depth + 1)?;
                }
            }
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => {
                self.free_bind_pattern(pattern, free, depth + 1)?
            }
            CorePatternKind::Wildcard
            | CorePatternKind::Integer(_)
            | CorePatternKind::Bool(_)
            | CorePatternKind::Float(_)
            | CorePatternKind::String(_)
            | CorePatternKind::Character(_) => {}
        }
        Ok(())
    }
    fn free_block(
        &mut self,
        block: &SemanticOriginalBlock,
        free: &mut Free<'_>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(block.span, depth)?;
        let scope = free.bound.len();
        for statement in &block.statements {
            self.free_statement(statement, free, depth + 1)?;
        }
        free.bound.truncate(scope);
        Ok(())
    }
    fn free_pattern_block(
        &mut self,
        pattern: &SemanticOriginalPattern,
        block: &SemanticOriginalBlock,
        free: &mut Free<'_>,
        depth: usize,
    ) -> R<()> {
        let scope = free.bound.len();
        self.free_bind_pattern(pattern, free, depth + 1)?;
        self.free_block(block, free, depth + 1)?;
        free.bound.truncate(scope);
        Ok(())
    }
    fn free_statement(
        &mut self,
        statement: &SemanticOriginalStatement,
        free: &mut Free<'_>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(statement.span, depth)?;
        match &statement.kind {
            CoreStatementKind::Binding {
                name, initializer, ..
            } => {
                if let Some(value) = initializer {
                    self.free_expr(value, free, depth + 1)?;
                }
                let name = self.text(name, statement.span)?;
                self.push(&mut free.bound, name, statement.span)?;
            }
            CoreStatementKind::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                self.free_expr(initializer, free, depth + 1)?;
                self.free_bind_pattern(pattern, free, depth + 1)?;
            }
            CoreStatementKind::Assign { target, value }
            | CoreStatementKind::CompoundAssign { target, value, .. } => {
                self.free_expr(target, free, depth + 1)?;
                self.free_expr(value, free, depth + 1)?;
            }
            CoreStatementKind::Expression(value) | CoreStatementKind::Yield(value) => {
                self.free_expr(value, free, depth + 1)?
            }
            CoreStatementKind::Return(value) => {
                if let Some(value) = value {
                    self.free_expr(value, free, depth + 1)?;
                }
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.free_expr(condition, free, depth + 1)?;
                self.free_block(then_body, free, depth + 1)?;
                if let Some(body) = else_body {
                    self.free_block(body, free, depth + 1)?;
                }
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.free_expr(value, free, depth + 1)?;
                self.free_pattern_block(pattern, then_body, free, depth + 1)?;
                if let Some(body) = else_body {
                    self.free_block(body, free, depth + 1)?;
                }
            }
            CoreStatementKind::While { condition, body } => {
                self.free_expr(condition, free, depth + 1)?;
                self.free_block(body, free, depth + 1)?;
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => {
                self.free_expr(value, free, depth + 1)?;
                self.free_pattern_block(pattern, body, free, depth + 1)?;
            }
            CoreStatementKind::ForEach {
                pattern,
                iterator,
                body,
            } => {
                self.free_expr(iterator, free, depth + 1)?;
                self.free_pattern_block(pattern, body, free, depth + 1)?;
            }
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.free_expr(start, free, depth + 1)?;
                self.free_expr(end, free, depth + 1)?;
                let scope = free.bound.len();
                let name = self.text(name, statement.span)?;
                self.push(&mut free.bound, name, statement.span)?;
                self.free_block(body, free, depth + 1)?;
                free.bound.truncate(scope);
            }
            CoreStatementKind::With { name, value, body } => {
                self.free_expr(value, free, depth + 1)?;
                let scope = free.bound.len();
                let name = self.text(name, statement.span)?;
                self.push(&mut free.bound, name, statement.span)?;
                self.free_block(body, free, depth + 1)?;
                free.bound.truncate(scope);
            }
            CoreStatementKind::Match { value, arms } => {
                self.free_expr(value, free, depth + 1)?;
                for arm in arms {
                    self.free_pattern_block(&arm.pattern, &arm.body, free, depth + 1)?;
                }
            }
            CoreStatementKind::Block(body)
            | CoreStatementKind::Unsafe(body)
            | CoreStatementKind::Loop(body)
            | CoreStatementKind::Defer(body) => self.free_block(body, free, depth + 1)?,
            // These computations belong to their actual CT staging proof,
            // not the runtime closure's capture directory.
            CoreStatementKind::CompileTime(_) => {}
            CoreStatementKind::Break | CoreStatementKind::Continue => {}
            CoreStatementKind::Unsupported(_) => {
                return Err(unsupported(
                    statement.span,
                    "free-name inventory cannot omit an unknown original statement",
                ));
            }
        }
        Ok(())
    }
    fn free_expr(
        &mut self,
        value: &SemanticOriginalExpr,
        free: &mut Free<'_>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(value.span, depth)?;
        match &value.kind {
            CoreExprKind::Name(name) => self.free_name(name, value.id, free, value.span)?,
            CoreExprKind::ReceiverMember(_) => {
                self.free_name("self", value.id, free, value.span)?
            }
            CoreExprKind::MakeDynBorrow { source, .. }
            | CoreExprKind::MakeDynOwned { source, .. } => {
                self.free_expr(source, free, depth + 1)?
            }
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::NumericConvert { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::Unary { operand: value, .. }
            | CoreExprKind::Group(value)
            | CoreExprKind::Spread(value)
            | CoreExprKind::Try(value) => self.free_expr(value, free, depth + 1)?,
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Pipeline {
                input: left,
                target: right,
            }
            | CoreExprKind::Range {
                start: left,
                end: right,
            } => {
                self.free_expr(left, free, depth + 1)?;
                self.free_expr(right, free, depth + 1)?;
            }
            CoreExprKind::TypeApply { callee, .. } | CoreExprKind::GenericApply { callee, .. } => {
                // Written Const/Type/Life actuals are semantic/CT arguments,
                // never independent runtime reads of a parent local.
                self.free_expr(callee, free, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments }
            | CoreExprKind::CallDynSlot {
                callee, arguments, ..
            } => {
                self.free_expr(callee, free, depth + 1)?;
                for argument in arguments {
                    self.free_expr(argument, free, depth + 1)?;
                }
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for value in values {
                    self.free_expr(value, free, depth + 1)?;
                }
            }
            CoreExprKind::Index { object, index } => {
                self.free_expr(object, free, depth + 1)?;
                self.free_expr(index, free, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.free_expr(object, free, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.free_expr(value, free, depth + 1)?;
                }
            }
            CoreExprKind::Member { object, .. } => self.free_expr(object, free, depth + 1)?,
            CoreExprKind::Record { fields, .. }
            | CoreExprKind::RecordConstructor { fields, .. } => {
                for (_, value) in fields {
                    self.free_expr(value, free, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.free_expr(base, free, depth + 1)?;
                for (_, value) in fields {
                    self.free_expr(value, free, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                let scope = free.bound.len();
                for (name, initializer) in bindings {
                    self.free_expr(initializer, free, depth + 1)?;
                    let name = self.text(name, value.span)?;
                    self.push(&mut free.bound, name, value.span)?;
                }
                self.free_expr(value, free, depth + 1)?;
                free.bound.truncate(scope);
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } => {
                let scope = free.bound.len();
                for parameter in parameters {
                    let name = self.text(&parameter.name, parameter.span)?;
                    self.push(&mut free.bound, name, parameter.span)?;
                }
                self.free_block(body, free, depth + 1)?;
                free.bound.truncate(scope);
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.free_expr(condition, free, depth + 1)?;
                self.free_expr(then_value, free, depth + 1)?;
                self.free_expr(else_value, free, depth + 1)?;
            }
            CoreExprKind::CompileTime(_)
            | CoreExprKind::Splice(_)
            | CoreExprKind::CompileTimeBlock(_)
            | CoreExprKind::SyntaxQuote { .. }
            | CoreExprKind::SyntaxIsland { .. } => {}
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Unit
            | CoreExprKind::Character(_) => {}
            CoreExprKind::Unsupported(_) => {
                return Err(unsupported(
                    value.span,
                    "free-name inventory cannot omit an unknown original expression",
                ));
            }
        }
        Ok(())
    }
}
