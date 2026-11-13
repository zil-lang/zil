//! Lexical runtime free names in the complete original operation tree.
//! No type, capture mode, borrow root or lifetime class is inferred here.
use super::*;
use std::collections::BTreeSet;

pub(in crate::source_semantics) struct OriginalClosureScope {
    pub expression: NodeId,
    pub bindings: Vec<(String, SemanticOriginalTypeSource)>,
}
struct Bound {
    name: String,
    source: Option<SemanticOriginalTypeSource>,
}
struct Free<'a> {
    available: &'a BTreeSet<String>,
    bound: Vec<Bound>,
    found: Vec<(String, NodeId)>,
    scopes: Option<Vec<OriginalClosureScope>>,
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
            scopes: None,
        };
        for (index, parameter) in parameters.iter().enumerate() {
            let ordinal = u32::try_from(index)
                .map_err(|_| error(parameter.span, "original value parameter ordinal overflow"))?;
            self.free_bind(
                &parameter.name,
                Some(SemanticOriginalTypeSource::Parameter(ordinal)),
                &mut free,
                parameter.span,
            )?;
        }
        self.free_block(body, &mut free, 0)?;
        Ok(free.found)
    }
    pub(in crate::source_semantics) fn original_closure_scopes(
        &mut self,
        body: &SemanticOriginalBlock,
        parameters: &[SemanticOriginalClosureParameter],
    ) -> R<Vec<OriginalClosureScope>> {
        let available = BTreeSet::new();
        let mut free = Free {
            available: &available,
            bound: Vec::new(),
            found: Vec::new(),
            scopes: Some(Vec::new()),
        };
        for (index, parameter) in parameters.iter().enumerate() {
            let ordinal = u32::try_from(index)
                .map_err(|_| error(parameter.span, "original value parameter ordinal overflow"))?;
            self.free_bind(
                &parameter.name,
                Some(SemanticOriginalTypeSource::Parameter(ordinal)),
                &mut free,
                parameter.span,
            )?;
        }
        self.free_block(body, &mut free, 0)?;
        free.scopes
            .ok_or_else(|| error(body.span, "original closure scope mode was lost"))
    }
    fn free_bind(
        &mut self,
        name: &str,
        source: Option<SemanticOriginalTypeSource>,
        free: &mut Free<'_>,
        span: Span,
    ) -> R<()> {
        let name = self.text(name, span)?;
        self.push(&mut free.bound, Bound { name, source }, span)
    }
    fn free_closure_scope(
        &mut self,
        expression: &SemanticOriginalExpr,
        parameters: &[SemanticOriginalClosureParameter],
        body: &SemanticOriginalBlock,
        free: &mut Free<'_>,
    ) -> R<()> {
        let mut available = BTreeSet::new();
        let mut visible: Vec<usize> = Vec::new();
        // Reverse lexical order keeps precisely the innermost declaration.
        for (index, bound) in free.bound.iter().enumerate().rev() {
            let mut shadowed = false;
            for previous in &visible {
                self.budget.text(&bound.name, expression.span)?;
                self.budget
                    .text(&free.bound[*previous].name, expression.span)?;
                shadowed |= bound.name == free.bound[*previous].name;
            }
            if shadowed {
                continue;
            }
            let name = self.text(&bound.name, expression.span)?;
            self.budget.identity_work(
                available.len().saturating_mul(name.len().saturating_add(1)),
                expression.span,
            )?;
            self.budget
                .lineage_bytes(12 * std::mem::size_of::<String>(), expression.span)?;
            available.insert(name);
            self.push(&mut visible, index, expression.span)?;
        }
        // A pattern subobject has no whole-binding Type source. Reject only
        // when that actual name is requested by this closure, rather than
        // laundering the tuple/record's whole type or rejecting an unused name.
        let used = self.original_free_uses(body, parameters, &available)?;
        for (name, _) in &used {
            for index in &visible {
                let bound = &free.bound[*index];
                self.budget.text(name, expression.span)?;
                self.budget.text(&bound.name, expression.span)?;
                if name == &bound.name && bound.source.is_none() {
                    return Err(unsupported(
                        expression.span,
                        "captured destructuring binder needs its exact original pattern subobject Type source",
                    ));
                }
            }
        }
        let mut bindings = Vec::new();
        for index in visible.into_iter().rev() {
            let bound = &free.bound[index];
            if let Some(source) = bound.source {
                let name = self.text(&bound.name, expression.span)?;
                self.push(&mut bindings, (name, source), expression.span)?;
            }
        }
        let scopes = free.scopes.as_mut().ok_or_else(|| {
            error(
                expression.span,
                "original scope collector has no output ledger",
            )
        })?;
        self.push(
            scopes,
            OriginalClosureScope {
                expression: expression.id,
                bindings,
            },
            expression.span,
        )
    }
    fn free_name(&mut self, name: &str, id: NodeId, free: &mut Free<'_>, span: Span) -> R<()> {
        for bound in free.bound.iter().rev() {
            self.budget.text(name, span)?;
            self.budget.text(&bound.name, span)?;
            if bound.name == name {
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
        source: Option<SemanticOriginalTypeSource>,
    ) -> R<()> {
        self.budget.node(pattern.span, depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.free_bind(name, source, free, pattern.span)?;
            }
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                for field in fields {
                    self.free_bind_pattern(field, free, depth + 1, None)?;
                }
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    self.free_bind_pattern(field, free, depth + 1, None)?;
                }
            }
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => {
                self.free_bind_pattern(pattern, free, depth + 1, None)?
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
        source: SemanticOriginalTypeSource,
    ) -> R<()> {
        let scope = free.bound.len();
        self.free_bind_pattern(pattern, free, depth + 1, Some(source))?;
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
                self.free_bind(
                    name,
                    Some(SemanticOriginalTypeSource::Binding(statement.id)),
                    free,
                    statement.span,
                )?;
            }
            CoreStatementKind::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                self.free_expr(initializer, free, depth + 1)?;
                self.free_bind_pattern(
                    pattern,
                    free,
                    depth + 1,
                    Some(SemanticOriginalTypeSource::Binding(statement.id)),
                )?;
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
                self.free_pattern_block(
                    pattern,
                    then_body,
                    free,
                    depth + 1,
                    SemanticOriginalTypeSource::Binding(statement.id),
                )?;
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
                self.free_pattern_block(
                    pattern,
                    body,
                    free,
                    depth + 1,
                    SemanticOriginalTypeSource::Binding(statement.id),
                )?;
            }
            CoreStatementKind::ForEach {
                pattern,
                iterator,
                body,
            } => {
                self.free_expr(iterator, free, depth + 1)?;
                self.free_pattern_block(
                    pattern,
                    body,
                    free,
                    depth + 1,
                    SemanticOriginalTypeSource::Binding(statement.id),
                )?;
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
                self.free_bind(
                    name,
                    Some(SemanticOriginalTypeSource::Binding(statement.id)),
                    free,
                    statement.span,
                )?;
                self.free_block(body, free, depth + 1)?;
                free.bound.truncate(scope);
            }
            CoreStatementKind::With { name, value, body } => {
                self.free_expr(value, free, depth + 1)?;
                let scope = free.bound.len();
                self.free_bind(
                    name,
                    Some(SemanticOriginalTypeSource::Binding(statement.id)),
                    free,
                    statement.span,
                )?;
                self.free_block(body, free, depth + 1)?;
                free.bound.truncate(scope);
            }
            CoreStatementKind::Match { value, arms } => {
                self.free_expr(value, free, depth + 1)?;
                for arm in arms {
                    self.free_pattern_block(
                        &arm.pattern,
                        &arm.body,
                        free,
                        depth + 1,
                        SemanticOriginalTypeSource::Binding(statement.id),
                    )?;
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
                return Err(unsupported(
                    value.span,
                    "implicit receiver capture requires its actual separate original head node",
                ));
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
                    self.free_bind(
                        name,
                        Some(SemanticOriginalTypeSource::Expression(initializer.id)),
                        free,
                        value.span,
                    )?;
                }
                self.free_expr(value, free, depth + 1)?;
                free.bound.truncate(scope);
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } => {
                if free.scopes.is_some() {
                    self.free_closure_scope(value, parameters, body, free)?;
                } else {
                    let scope = free.bound.len();
                    for parameter in parameters {
                        self.free_bind(&parameter.name, None, free, parameter.span)?;
                    }
                    self.free_block(body, free, depth + 1)?;
                    free.bound.truncate(scope);
                }
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
