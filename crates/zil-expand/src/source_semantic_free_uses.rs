//! Lexical runtime free names in the complete original operation tree.
//! No type, capture mode, borrow root or lifetime class is inferred here.
use super::*;
use crate::source_semantics::closures::CaptureOccurrence;
use std::collections::BTreeSet;

pub(in crate::source_semantics) struct OriginalClosureScope {
    pub expression: NodeId,
    pub bindings: Vec<(String, SemanticOriginalTypeSource)>,
}
struct Bound {
    name: String,
    source: Option<SemanticOriginalTypeSource>,
    pattern: bool,
}
#[derive(Default)]
struct ValueUses {
    names: Vec<NodeId>,
    patterns: Vec<NodeId>,
}
struct Free<'a> {
    available: &'a BTreeSet<String>,
    bound: Vec<Bound>,
    // Child names belong to the child definition. A forwarding-only use is
    // anchored to the immediate constructor in the current definition.
    found: Vec<(String, CaptureOccurrence)>,
    child_constructor: Option<NodeId>,
    scopes: Option<Vec<OriginalClosureScope>>,
    value_uses: Option<ValueUses>,
    typing_stages: bool,
}
impl Builder<'_, '_, '_, '_> {
    pub(in crate::source_semantics) fn original_free_uses(
        &mut self,
        body: &SemanticOriginalBlock,
        parameters: &[SemanticOriginalClosureParameter],
        available: &BTreeSet<String>,
    ) -> R<Vec<(String, CaptureOccurrence)>> {
        let mut free = Free {
            available,
            bound: Vec::new(),
            found: Vec::new(),
            child_constructor: None,
            scopes: None,
            value_uses: None,
            typing_stages: false,
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
            child_constructor: None,
            scopes: Some(Vec::new()),
            value_uses: None,
            typing_stages: false,
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
    pub(in crate::source_semantics) fn original_value_uses(
        &mut self,
        body: &SemanticOriginalBlock,
        parameters: &[SemanticOriginalClosureParameter],
        available: &BTreeSet<String>,
    ) -> R<(Vec<NodeId>, Vec<NodeId>)> {
        let mut free = Free {
            available,
            bound: Vec::new(),
            found: Vec::new(),
            child_constructor: None,
            scopes: None,
            value_uses: Some(ValueUses::default()),
            typing_stages: true,
        };
        for (ordinal, parameter) in parameters.iter().enumerate() {
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| error(parameter.span, "original parameter ordinal overflow"))?;
            self.free_bind(
                &parameter.name,
                Some(SemanticOriginalTypeSource::Parameter(ordinal)),
                &mut free,
                parameter.span,
            )?;
        }
        self.free_block(body, &mut free, 0)?;
        let uses = free.value_uses
            .ok_or_else(|| error(body.span, "original runtime value-use mode was lost"))?;
        Ok((uses.names, uses.patterns))
    }
    fn free_bind(
        &mut self,
        name: &str,
        source: Option<SemanticOriginalTypeSource>,
        free: &mut Free<'_>,
        span: Span,
    ) -> R<()> {
        let name = self.text(name, span)?;
        self.push(&mut free.bound, Bound { name, source, pattern: false }, span)
    }
    fn free_closure_scope(
        &mut self,
        expression: &SemanticOriginalExpr,
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
        // NameAt denotes the exact declaration-domain lexical Type at this
        // creator. It is not the whole destructuring input, an SSA occupant,
        // or permission to read an uninitialized binding. The independent
        // original-body consumer resolves the pattern's real selected leaf.
        let mut bindings = Vec::new();
        for index in visible.into_iter().rev() {
            let bound = &free.bound[index];
            let source = if let Some(source) = &bound.source {
                prepay_recipe(source, self.budget, expression.span, 0)?;
                source.clone()
            } else {
                SemanticOriginalTypeSource::NameAt {
                    at: SemanticOriginalNode::Expression(expression.id),
                    name: self.text(&bound.name, expression.span)?,
                }
            };
            let name = self.text(&bound.name, expression.span)?;
            self.push(&mut bindings, (name, source), expression.span)?;
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
                if let Some(uses) = free.value_uses.as_mut() {
                    self.push(&mut uses.names, id, span)?;
                    if bound.pattern {
                        self.push(&mut uses.patterns, id, span)?;
                    }
                }
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
        if let Some(uses) = free.value_uses.as_mut() {
            self.push(&mut uses.names, id, span)?;
            return Ok(());
        }
        let site = free.child_constructor.map_or(CaptureOccurrence::Read(id), CaptureOccurrence::Forward);
        for (previous, occurrence) in &mut free.found {
            self.budget.text(name, span)?;
            self.budget.text(previous, span)?;
            if previous == name {
                if matches!(occurrence, CaptureOccurrence::Forward(_)) && matches!(site, CaptureOccurrence::Read(_)) { *occurrence = site; }
                return Ok(());
            }
        }
        let name = self.text(name, span)?;
        self.push(&mut free.found, (name, site), span)
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
                let name = self.text(name, pattern.span)?;
                self.push(&mut free.bound, Bound { name, source, pattern: true }, pattern.span)?;
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
            CoreStatementKind::CompileTime(statement) => {
                if free.typing_stages {
                    let scope = free.bound.len();
                    self.free_statement(statement, free, depth + 1)?;
                    free.bound.truncate(scope);
                }
            }
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
        use CoreExprKind as E;
        let mut pending = vec![(value, depth)];
        while let Some((value, depth)) = pending.pop() {
            self.budget.node(value.span, depth)?;
            match &value.kind {
                E::MakeDynBorrow { source: inner, .. } | E::MakeDynOwned { source: inner, .. }
                | E::Ascribed { value: inner, .. } | E::NumericConvert { value: inner, .. }
                | E::IntegerConvert { value: inner, .. } | E::Unary { operand: inner, .. }
                | E::Group(inner) | E::Spread(inner) | E::Try(inner)
                | E::TypeApply { callee: inner, .. } | E::Member { object: inner, .. } => {
                    pending.push((inner, depth + 1));
                }
                E::Binary { left, right, .. } | E::Range { start: left, end: right }
                | E::Pipeline { input: left, target: right } | E::Index { object: left, index: right } => {
                    pending.push((right, depth + 1));
                    pending.push((left, depth + 1));
                }
                _ => self.free_expr_scope(value, free, depth)?,
            }
        }
        Ok(())
    }
    fn free_expr_scope(
        &mut self,
        value: &SemanticOriginalExpr,
        free: &mut Free<'_>,
        depth: usize,
    ) -> R<()> {
        match &value.kind {
            CoreExprKind::Name(name) => self.free_name(name, value.id, free, value.span)?,
            CoreExprKind::ReceiverMember(_) => {
                return Err(unsupported(
                    value.span,
                    "implicit receiver capture requires its actual separate original head node",
                ));
            }
            CoreExprKind::MakeDynBorrow { .. } | CoreExprKind::MakeDynOwned { .. }
            | CoreExprKind::Ascribed { .. } | CoreExprKind::NumericConvert { .. }
            | CoreExprKind::IntegerConvert { .. } | CoreExprKind::Unary { .. }
            | CoreExprKind::Group(_) | CoreExprKind::Spread(_) | CoreExprKind::Try(_)
            | CoreExprKind::TypeApply { .. } | CoreExprKind::Member { .. } | CoreExprKind::Binary { .. }
            | CoreExprKind::Range { .. } | CoreExprKind::Pipeline { .. } | CoreExprKind::Index { .. } =>
                unreachable!("handled by the free-name operand traversal"),
            CoreExprKind::GenericApply { callee, arguments } => {
                self.free_expr(callee, free, depth + 1)?;
                if free.typing_stages {
                    for argument in arguments {
                        if let CoreGenericArgument::Const(value) = argument {
                            self.free_expr(value, free, depth + 1)?;
                        }
                    }
                }
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
            CoreExprKind::Slice { object, start, end } => {
                self.free_expr(object, free, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.free_expr(value, free, depth + 1)?;
                }
            }
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
                if free.value_uses.is_some() {
                    // This child's own original definition owns its names.
                } else if free.scopes.is_some() {
                    self.free_closure_scope(value, free)?;
                } else {
                    let scope = free.bound.len();
                    for parameter in parameters {
                        self.free_bind(&parameter.name, None, free, parameter.span)?;
                    }
                    let outer = free.child_constructor;
                    free.child_constructor.get_or_insert(value.id);
                    self.free_block(body, free, depth + 1)?;
                    free.child_constructor = outer;
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
            CoreExprKind::CompileTime(value) | CoreExprKind::Splice(value) => {
                if free.typing_stages {
                    self.free_expr(value, free, depth + 1)?;
                }
            }
            CoreExprKind::CompileTimeBlock(body) => {
                if free.typing_stages {
                    self.free_block(body, free, depth + 1)?;
                }
            }
            CoreExprKind::SyntaxQuote { template, .. } => {
                if free.typing_stages {
                    for hole in &template.holes {
                        self.free_expr(&hole.value, free, depth + 1)?;
                    }
                }
            }
            CoreExprKind::SyntaxIsland { .. } => {}
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
