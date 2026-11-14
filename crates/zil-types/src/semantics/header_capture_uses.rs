//! The original first free use which formed one lexical capture. This mirrors
//! Source's lexical walk, including shadowing, staging and nested closures.
use crate::*;

struct Search<'a> {
    name: &'a str,
    shadowed: usize,
    found: Option<zil_base::NodeId>,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl Search<'_> {
    fn visit(&mut self, span: Span, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "capture first-use depth exceeded",
                Some(span),
            ));
        }
        *self.left = self.left.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "capture first-use work exceeded", Some(span))
        })?;
        Ok(())
    }
    fn bind(&mut self, name: &str, span: Span) -> R<()> {
        *self.left = self
            .left
            .checked_sub(name.len().saturating_add(self.name.len()).saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "capture lexical binding work exceeded",
                    Some(span),
                )
            })?;
        if name == self.name {
            self.shadowed = self.shadowed.checked_add(1).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "capture lexical binding depth exceeded",
                    Some(span),
                )
            })?;
        }
        Ok(())
    }
    fn pattern(&mut self, pattern: &SemanticOriginalPattern, depth: usize) -> R<()> {
        self.visit(pattern.span, depth)?;
        use CorePatternKind as P;
        match &pattern.kind {
            P::Name(name) => self.bind(name, pattern.span)?,
            P::Tuple(fields) | P::Variant { fields, .. } | P::Constructor { fields, .. } => {
                for field in fields {
                    self.pattern(field, depth + 1)?
                }
            }
            P::Record { fields, .. } | P::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    self.pattern(field, depth + 1)?
                }
            }
            P::Reference { pattern, .. } | P::ErrorArm { pattern, .. } => {
                self.pattern(pattern, depth + 1)?
            }
            P::Wildcard
            | P::Integer(_)
            | P::Bool(_)
            | P::Float(_)
            | P::String(_)
            | P::Character(_) => {}
        }
        Ok(())
    }
    fn block(&mut self, block: &SemanticOriginalBlock, depth: usize) -> R<()> {
        self.visit(block.span, depth)?;
        let before = self.shadowed;
        for statement in &block.statements {
            if self.found.is_some() {
                break;
            }
            self.statement(statement, depth + 1)?;
        }
        self.shadowed = before;
        Ok(())
    }
    fn pattern_block(
        &mut self,
        pattern: &SemanticOriginalPattern,
        body: &SemanticOriginalBlock,
        depth: usize,
    ) -> R<()> {
        let before = self.shadowed;
        self.pattern(pattern, depth + 1)?;
        self.block(body, depth + 1)?;
        self.shadowed = before;
        Ok(())
    }
    fn statement(&mut self, statement: &SemanticOriginalStatement, depth: usize) -> R<()> {
        if self.found.is_some() {
            return Ok(());
        }
        self.visit(statement.span, depth)?;
        use CoreStatementKind as S;
        match &statement.kind {
            S::Binding {
                name, initializer, ..
            } => {
                if let Some(value) = initializer {
                    self.expression(value, depth + 1)?
                }
                self.bind(name, statement.span)?;
            }
            S::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                self.expression(initializer, depth + 1)?;
                self.pattern(pattern, depth + 1)?;
            }
            S::Assign { target, value } | S::CompoundAssign { target, value, .. } => {
                self.expression(target, depth + 1)?;
                self.expression(value, depth + 1)?;
            }
            S::Expression(value) | S::Yield(value) => self.expression(value, depth + 1)?,
            S::Return(value) => {
                if let Some(value) = value {
                    self.expression(value, depth + 1)?
                }
            }
            S::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expression(condition, depth + 1)?;
                self.block(then_body, depth + 1)?;
                if let Some(body) = else_body {
                    self.block(body, depth + 1)?
                }
            }
            S::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.expression(value, depth + 1)?;
                self.pattern_block(pattern, then_body, depth + 1)?;
                if let Some(body) = else_body {
                    self.block(body, depth + 1)?
                }
            }
            S::While { condition, body } => {
                self.expression(condition, depth + 1)?;
                self.block(body, depth + 1)?;
            }
            S::WhileLet {
                pattern,
                value,
                body,
            } => {
                self.expression(value, depth + 1)?;
                self.pattern_block(pattern, body, depth + 1)?;
            }
            S::ForEach {
                pattern,
                iterator,
                body,
            } => {
                self.expression(iterator, depth + 1)?;
                self.pattern_block(pattern, body, depth + 1)?;
            }
            S::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.expression(start, depth + 1)?;
                self.expression(end, depth + 1)?;
                let before = self.shadowed;
                self.bind(name, statement.span)?;
                self.block(body, depth + 1)?;
                self.shadowed = before;
            }
            S::With { name, value, body } => {
                self.expression(value, depth + 1)?;
                let before = self.shadowed;
                self.bind(name, statement.span)?;
                self.block(body, depth + 1)?;
                self.shadowed = before;
            }
            S::Match { value, arms } => {
                self.expression(value, depth + 1)?;
                for arm in arms {
                    self.pattern_block(&arm.pattern, &arm.body, depth + 1)?
                }
            }
            S::Block(body) | S::Unsafe(body) | S::Loop(body) | S::Defer(body) => {
                self.block(body, depth + 1)?
            }
            S::CompileTime(_) | S::Break | S::Continue => {}
            S::Unsupported(_) => {
                return Err(Diagnostic::unsupported(
                    Phase::Types,
                    "capture first-use cannot omit an original statement",
                    Some(statement.span),
                ));
            }
        }
        Ok(())
    }
    fn expression(&mut self, expression: &SemanticOriginalExpr, depth: usize) -> R<()> {
        if self.found.is_some() {
            return Ok(());
        }
        self.visit(expression.span, depth)?;
        use CoreExprKind as E;
        match &expression.kind {
            E::Name(name) => {
                if self.shadowed == 0 {
                    self.bind(name, expression.span)?;
                    if self.shadowed != 0 {
                        self.found = Some(expression.id);
                        self.shadowed = 0;
                    }
                }
            }
            E::ReceiverMember(_) => {
                return Err(Diagnostic::unsupported(
                    Phase::Types,
                    "capture first-use requires its separate receiver head",
                    Some(expression.span),
                ));
            }
            E::MakeDynBorrow { source, .. } | E::MakeDynOwned { source, .. } => {
                self.expression(source, depth + 1)?
            }
            E::Ascribed { value, .. }
            | E::NumericConvert { value, .. }
            | E::IntegerConvert { value, .. }
            | E::Unary { operand: value, .. }
            | E::Group(value)
            | E::Spread(value)
            | E::Try(value) => self.expression(value, depth + 1)?,
            E::Binary { left, right, .. }
            | E::Pipeline {
                input: left,
                target: right,
            }
            | E::Range {
                start: left,
                end: right,
            } => {
                self.expression(left, depth + 1)?;
                self.expression(right, depth + 1)?;
            }
            E::TypeApply { callee, .. } | E::GenericApply { callee, .. } => {
                self.expression(callee, depth + 1)?
            }
            E::Call { callee, arguments }
            | E::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expression(callee, depth + 1)?;
                for value in arguments {
                    self.expression(value, depth + 1)?
                }
            }
            E::Tuple(values) | E::Array(values) => {
                for value in values {
                    self.expression(value, depth + 1)?
                }
            }
            E::Index { object, index } => {
                self.expression(object, depth + 1)?;
                self.expression(index, depth + 1)?;
            }
            E::Slice { object, start, end } => {
                self.expression(object, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expression(value, depth + 1)?
                }
            }
            E::Member { object, .. } => self.expression(object, depth + 1)?,
            E::Record { fields, .. } | E::RecordConstructor { fields, .. } => {
                for (_, value) in fields {
                    self.expression(value, depth + 1)?
                }
            }
            E::RecordUpdate { base, fields, .. } => {
                self.expression(base, depth + 1)?;
                for (_, value) in fields {
                    self.expression(value, depth + 1)?
                }
            }
            E::Sequence { bindings, value } => {
                let before = self.shadowed;
                for (name, initializer) in bindings {
                    self.expression(initializer, depth + 1)?;
                    self.bind(name, initializer.span)?;
                }
                self.expression(value, depth + 1)?;
                self.shadowed = before;
            }
            E::Closure {
                parameters, body, ..
            } => {
                let before = self.shadowed;
                for parameter in parameters {
                    self.bind(&parameter.name, parameter.span)?
                }
                self.block(body, depth + 1)?;
                self.shadowed = before;
            }
            E::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, depth + 1)?;
                self.expression(then_value, depth + 1)?;
                self.expression(else_value, depth + 1)?;
            }
            E::CompileTime(_)
            | E::Splice(_)
            | E::CompileTimeBlock(_)
            | E::SyntaxQuote { .. }
            | E::SyntaxIsland { .. }
            | E::Literal(_)
            | E::Integer(_)
            | E::Float(_)
            | E::Bool(_)
            | E::String(_)
            | E::Unit
            | E::Character(_) => {}
            E::Unsupported(_) => {
                return Err(Diagnostic::unsupported(
                    Phase::Types,
                    "capture first-use cannot omit an original expression",
                    Some(expression.span),
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn find(
    definition: &SemanticDefinition,
    name: &str,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<zil_base::NodeId>> {
    let SemanticDefinitionBody::FunctionWithBody {
        parameters,
        body: SemanticOriginalFunctionBody::Defined(body),
        ..
    } = &definition.body
    else {
        return Ok(None);
    };
    let mut search = Search {
        name,
        shadowed: 0,
        found: None,
        limits,
        left,
    };
    for parameter in parameters {
        search.bind(&parameter.name, parameter.span)?
    }
    search.block(body, 0)?;
    Ok(search.found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::NodeId;

    fn expression(id: u32, kind: SemanticOriginalExprKind) -> SemanticOriginalExpr {
        CoreExpr {
            id: NodeId(id),
            kind,
            span: Span::default(),
        }
    }
    fn name(id: u32) -> SemanticOriginalExpr {
        expression(id, CoreExprKind::Name("captured".into()))
    }
    fn first(value: &SemanticOriginalExpr) -> Option<NodeId> {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let mut search = Search {
            name: "captured",
            shadowed: 0,
            found: None,
            limits: &limits,
            left: &mut left,
        };
        search.expression(value, 0).unwrap();
        search.found
    }

    #[test]
    fn nested_parameter_shadow_and_ct_stage_do_not_hide_the_later_outer_use() {
        let shadowed = expression(
            2,
            CoreExprKind::Closure {
                parameters: vec![CoreClosureParameter {
                    name: "captured".into(),
                    ty: None,
                    mode: PassingMode::Infer,
                    span: Span::default(),
                }],
                return_type: None,
                body: CoreBlock {
                    statements: vec![CoreStatement {
                        id: NodeId(3),
                        kind: CoreStatementKind::Return(Some(name(4))),
                        span: Span::default(),
                    }],
                    span: Span::default(),
                },
                move_capture: false,
            },
        );
        let staged = expression(5, CoreExprKind::CompileTime(Box::new(name(6))));
        let value = expression(1, CoreExprKind::Tuple(vec![shadowed, staged, name(7)]));
        assert_eq!(first(&value), Some(NodeId(7)));
    }

    #[test]
    fn sequence_initializer_precedes_its_shadowing_binding() {
        let value = expression(
            1,
            CoreExprKind::Sequence {
                bindings: vec![("captured".into(), name(2))],
                value: Box::new(name(3)),
            },
        );
        assert_eq!(first(&value), Some(NodeId(2)));
    }
}
