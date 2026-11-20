//! Lexical traversal over the source body shared by the original kernels.
//! It proves no type, original environment correspondence or transfer.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamedPlaceRoot<'a> {
    pub node: NodeId,
    pub name: &'a str,
}

/// Find the named source occurrence behind a syntactically exclusive place
/// use. The caller still proves the binder, address mode and actual permission.
pub fn exclusive_place_root<Annotation, E>(
    mut expression: &CoreExpr<Annotation>,
    mut visit: impl FnMut(usize, Span) -> Result<(), E>,
) -> Result<Option<NamedPlaceRoot<'_>>, E> {
    use CoreExprKind as X;
    let mut depth = 0usize;
    loop {
        visit(depth, expression.span)?;
        expression = match &expression.kind {
            X::Name(name) => {
                return Ok(Some(NamedPlaceRoot {
                    node: expression.id,
                    name,
                }));
            }
            X::Group(base)
            | X::Ascribed { value: base, .. }
            | X::Sequence { value: base, .. }
            | X::Unary {
                op: UnaryOp::BorrowMut | UnaryOp::Dereference,
                operand: base,
            }
            | X::Member { object: base, .. }
            | X::Index { object: base, .. }
            | X::Slice { object: base, .. } => base,
            X::Unary {
                op:
                    UnaryOp::Borrow | UnaryOp::Give | UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                ..
            }
            | X::ReceiverMember(_)
            | X::MakeDynBorrow { .. }
            | X::MakeDynOwned { .. }
            | X::CallDynSlot { .. }
            | X::Literal(_)
            | X::Integer(_)
            | X::Float(_)
            | X::Bool(_)
            | X::String(_)
            | X::Character(_)
            | X::Unit
            | X::NumericConvert { .. }
            | X::IntegerConvert { .. }
            | X::Binary { .. }
            | X::TypeApply { .. }
            | X::GenericApply { .. }
            | X::Call { .. }
            | X::Tuple(_)
            | X::Array(_)
            | X::Record { .. }
            | X::RecordUpdate { .. }
            | X::RecordConstructor { .. }
            | X::CompileTime(_)
            | X::CompileTimeBlock(_)
            | X::Try(_)
            | X::Closure { .. }
            | X::If { .. }
            | X::Unsupported(_)
            | X::Spread(_)
            | X::Range { .. }
            | X::Pipeline { .. }
            | X::Splice(_)
            | X::SyntaxQuote { .. }
            | X::SyntaxIsland { .. } => return Ok(None),
        };
        depth = depth.saturating_add(1);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CaptureUse {
    /// The first name occurrence owned by this declaration. Children have
    /// their own node-to-site directory even when they capture the same root.
    First,
    Give,
}

struct Search<'a, Charge, Failure> {
    name: &'a str,
    shadowed: usize,
    found: Option<NodeId>,
    mode: CaptureUse,
    charge: Charge,
    failure: Failure,
}

impl<E, Charge, Failure> Search<'_, Charge, Failure>
where
    Charge: FnMut(usize, usize, Span) -> Result<(), E>,
    Failure: FnMut(&'static str, Span) -> E,
{
    fn visit(&mut self, span: Span, depth: usize) -> Result<(), E> {
        (self.charge)(1, depth, span)
    }

    fn matches(&mut self, name: &str, span: Span) -> Result<bool, E> {
        (self.charge)(
            name.len().saturating_add(self.name.len()).saturating_add(1),
            0,
            span,
        )?;
        Ok(name == self.name)
    }

    fn bind(&mut self, name: &str, span: Span) -> Result<(), E> {
        if self.matches(name, span)? {
            self.shadowed += 1;
        }
        Ok(())
    }
    fn pattern(&mut self, pattern: &SemanticOriginalPattern, depth: usize) -> Result<(), E> {
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
    fn block(&mut self, block: &SemanticOriginalBlock, depth: usize) -> Result<(), E> {
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
    ) -> Result<(), E> {
        let before = self.shadowed;
        self.pattern(pattern, depth + 1)?;
        self.block(body, depth + 1)?;
        self.shadowed = before;
        Ok(())
    }
    fn statement(&mut self, statement: &SemanticOriginalStatement, depth: usize) -> Result<(), E> {
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
                return Err((self.failure)(
                    "capture first-use cannot omit an original statement",
                    statement.span,
                ));
            }
        }
        Ok(())
    }
    fn expression(&mut self, expression: &SemanticOriginalExpr, depth: usize) -> Result<(), E> {
        if self.found.is_some() {
            return Ok(());
        }
        self.visit(expression.span, depth)?;
        use CoreExprKind as K;
        match &expression.kind {
            K::Name(name) => {
                if self.mode == CaptureUse::First
                    && self.shadowed == 0
                    && self.matches(name, expression.span)?
                {
                    self.found = Some(expression.id);
                }
            }
            K::Unary {
                op: UnaryOp::Give,
                operand,
            } if self.mode == CaptureUse::Give => {
                let mut source = operand.as_ref();
                while let K::Group(inner) = &source.kind {
                    self.visit(source.span, depth + 1)?;
                    source = inner;
                }
                self.visit(source.span, depth + 1)?;
                if let K::Name(name) = &source.kind
                    && self.shadowed == 0
                    && self.matches(name, source.span)?
                {
                    self.found = Some(source.id);
                }
            }
            K::ReceiverMember(_) if self.mode == CaptureUse::First => {
                return Err((self.failure)(
                    "capture first-use requires its separate receiver head",
                    expression.span,
                ));
            }
            K::ReceiverMember(_) => {}
            K::MakeDynBorrow { source, .. } | K::MakeDynOwned { source, .. } => {
                self.expression(source, depth + 1)?
            }
            K::Ascribed { value, .. }
            | K::NumericConvert { value, .. }
            | K::IntegerConvert { value, .. }
            | K::Unary { operand: value, .. }
            | K::Group(value)
            | K::Spread(value)
            | K::Try(value) => self.expression(value, depth + 1)?,
            K::Binary { left, right, .. }
            | K::Pipeline {
                input: left,
                target: right,
            }
            | K::Range {
                start: left,
                end: right,
            } => {
                self.expression(left, depth + 1)?;
                self.expression(right, depth + 1)?;
            }
            K::TypeApply { callee, .. } | K::GenericApply { callee, .. } => {
                self.expression(callee, depth + 1)?
            }
            K::Call { callee, arguments }
            | K::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expression(callee, depth + 1)?;
                for value in arguments {
                    self.expression(value, depth + 1)?
                }
            }
            K::Tuple(values) | K::Array(values) => {
                for value in values {
                    self.expression(value, depth + 1)?
                }
            }
            K::Index { object, index } => {
                self.expression(object, depth + 1)?;
                self.expression(index, depth + 1)?;
            }
            K::Slice { object, start, end } => {
                self.expression(object, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expression(value, depth + 1)?
                }
            }
            K::Member { object, .. } => self.expression(object, depth + 1)?,
            K::Record { fields, .. } | K::RecordConstructor { fields, .. } => {
                for (_, value) in fields {
                    self.expression(value, depth + 1)?
                }
            }
            K::RecordUpdate { base, fields, .. } => {
                self.expression(base, depth + 1)?;
                for (_, value) in fields {
                    self.expression(value, depth + 1)?
                }
            }
            K::Sequence { bindings, value } => {
                let before = self.shadowed;
                for (name, initializer) in bindings {
                    self.expression(initializer, depth + 1)?;
                    self.bind(name, initializer.span)?;
                }
                self.expression(value, depth + 1)?;
                self.shadowed = before;
            }
            K::Closure { .. } if self.mode == CaptureUse::First => {}
            K::Closure {
                parameters, body, ..
            } => {
                let before = self.shadowed;
                for parameter in parameters {
                    self.bind(&parameter.name, parameter.span)?
                }
                self.block(body, depth + 1)?;
                self.shadowed = before;
            }
            K::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, depth + 1)?;
                self.expression(then_value, depth + 1)?;
                self.expression(else_value, depth + 1)?;
            }
            K::CompileTime(_)
            | K::Splice(_)
            | K::CompileTimeBlock(_)
            | K::SyntaxQuote { .. }
            | K::SyntaxIsland { .. }
            | K::Literal(_)
            | K::Integer(_)
            | K::Float(_)
            | K::Bool(_)
            | K::String(_)
            | K::Unit
            | K::Character(_) => {}
            K::Unsupported(_) => {
                return Err((self.failure)(
                    "capture first-use cannot omit an original expression",
                    expression.span,
                ));
            }
        }
        Ok(())
    }
}

/// The exact lexical occurrence of a candidate name in a complete original body.
/// Each kernel must prove the selected name's actual binder and value category.
pub fn original_capture_use<E>(
    definition: &SemanticDefinition,
    name: &str,
    mode: CaptureUse,
    charge: impl FnMut(usize, usize, Span) -> Result<(), E>,
    failure: impl FnMut(&'static str, Span) -> E,
) -> Result<Option<NodeId>, E> {
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
        mode,
        charge,
        failure,
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
    fn selected(value: &SemanticOriginalExpr, mode: CaptureUse) -> Option<NodeId> {
        let mut search = Search {
            name: "captured",
            shadowed: 0,
            found: None,
            mode,
            charge: |_, _, _| Ok::<(), &str>(()),
            failure: |message, _| message,
        };
        search.expression(value, 0).unwrap();
        search.found
    }

    #[test]
    fn exclusive_place_syntax_selects_the_actual_name_occurrence() {
        let borrow = |op| {
            expression(
                1,
                CoreExprKind::Unary {
                    op,
                    operand: Box::new(expression(2, CoreExprKind::Group(Box::new(name(3))))),
                },
            )
        };
        let exclusive = borrow(UnaryOp::BorrowMut);
        let mut visited = Vec::new();
        assert_eq!(
            exclusive_place_root(&exclusive, |depth, _| {
                visited.push(depth);
                Ok::<(), ()>(())
            }),
            Ok(Some(NamedPlaceRoot {
                node: NodeId(3),
                name: "captured"
            }))
        );
        assert_eq!(visited, [0, 1, 2]);
        for value in [
            borrow(UnaryOp::Borrow),
            borrow(UnaryOp::Give),
            expression(4, CoreExprKind::String("\"static\"".into())),
        ] {
            assert_eq!(
                exclusive_place_root(&value, |_, _| Ok::<(), ()>(())),
                Ok(None)
            );
        }
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
        assert_eq!(selected(&value, CaptureUse::First), Some(NodeId(7)));
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
        assert_eq!(selected(&value, CaptureUse::First), Some(NodeId(2)));
    }

    #[test]
    fn nested_free_name_is_owned_by_the_child_even_when_the_parent_reads_it_later() {
        let child = expression(
            2,
            CoreExprKind::Closure {
                parameters: vec![],
                return_type: None,
                move_capture: false,
                body: CoreBlock {
                    statements: vec![CoreStatement {
                        id: NodeId(3),
                        kind: CoreStatementKind::Return(Some(name(4))),
                        span: Span::default(),
                    }],
                    span: Span::default(),
                },
            },
        );
        assert_eq!(selected(&child, CaptureUse::First), None);
        let value = expression(1, CoreExprKind::Tuple(vec![child, name(7)]));
        assert_eq!(selected(&value, CaptureUse::First), Some(NodeId(7)));
    }

    #[test]
    fn give_selects_the_free_name_and_keeps_other_reads_distinct() {
        let give = |id, operand| {
            expression(
                id,
                CoreExprKind::Unary {
                    op: UnaryOp::Give,
                    operand: Box::new(operand),
                },
            )
        };
        let shadowed = expression(
            3,
            CoreExprKind::Sequence {
                bindings: vec![("captured".into(), expression(4, CoreExprKind::Unit))],
                value: Box::new(give(5, name(6))),
            },
        );
        let observed = name(2);
        let transferred = give(7, expression(8, CoreExprKind::Group(Box::new(name(9)))));
        let tuple = expression(
            1,
            CoreExprKind::Tuple(vec![observed, shadowed, transferred]),
        );
        assert_eq!(selected(&tuple, CaptureUse::First), Some(NodeId(2)));
        assert_eq!(selected(&tuple, CaptureUse::Give), Some(NodeId(9)));
    }
}
