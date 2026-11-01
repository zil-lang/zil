//! Source cleanup actions use real closure capture and executable invocation.
use super::*;

impl Builder<'_, '_> {
    pub(super) fn register_defer(&mut self, body: &CoreBlock, span: Span) -> R<()> {
        if self.defer_body {
            return Err(error(
                "E-DEFER-NESTED",
                span,
                "nested defer is not supported",
            ));
        }
        self.validate_defer_block(body, 0, 0, true)?;
        let (value, ty) = self.closure_expression(
            closures::Input {
                defer_action: true,
                move_capture: false,
                parameters: &[],
                result: Some(&Type::Unit),
                body,
                span,
            },
            None,
        )?;
        // This is ordinary lexical storage, not a full-expression temporary.
        // Reserving a temporary would move registration storage before earlier
        // exits and would let finish_resource_temps clean the action too soon.
        let name = format!("$defer{}", self.function.locals.len());
        let local = self.local(&name, ty, false, span)?;
        self.store(local, value, span)?;
        self.emit(
            InstructionKind::RegisterDefer {
                scope: *self.scope_ids.last().expect("active cleanup scope"),
                local,
            },
            None,
            span,
        )?;
        self.closures
            .snapshot(self.defer_locals.len().saturating_add(1), span)?;
        self.defer_locals.insert(local);
        Ok(())
    }

    pub(super) fn invoke_defer(&mut self, local: LocalId, span: Span) -> R<()> {
        let ty = self.function.locals[local.0 as usize].ty.clone();
        let callee = self.value(InstructionKind::TakeLocal { local }, ty, span)?;
        self.emit(
            InstructionKind::CallClosureOnce {
                callee,
                arguments: vec![],
            },
            None,
            span,
        )?;
        Ok(())
    }

    fn defer_visit(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "defer body traversal depth exceeded",
                Some(span),
            ));
        }
        self.closures.snapshot(1, span)
    }

    fn validate_defer_block(
        &mut self,
        body: &CoreBlock,
        depth: usize,
        loops: usize,
        cleanup: bool,
    ) -> R<()> {
        self.defer_visit(depth, body.span)?;
        for statement in &body.statements {
            self.defer_visit(depth + 1, statement.span)?;
            match &statement.kind {
                CoreStatementKind::Defer(_) => {
                    return Err(error(
                        "E-DEFER-NESTED",
                        statement.span,
                        "nested defer is not supported",
                    ));
                }
                CoreStatementKind::Return(_) if cleanup => {
                    return Err(error(
                        "E-DEFER-CONTROL",
                        statement.span,
                        "return cannot leave a defer cleanup body",
                    ));
                }
                CoreStatementKind::Break | CoreStatementKind::Continue if cleanup && loops == 0 => {
                    return Err(error(
                        "E-DEFER-CONTROL",
                        statement.span,
                        "loop control cannot cross a defer cleanup boundary",
                    ));
                }
                CoreStatementKind::Binding {
                    initializer: Some(value),
                    ..
                } => {
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                }
                CoreStatementKind::Assign { target, value } => {
                    self.validate_defer_expr(target, depth + 1, cleanup)?;
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                }
                CoreStatementKind::Expression(value) | CoreStatementKind::Return(Some(value)) => {
                    self.validate_defer_expr(value, depth + 1, cleanup)?
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.validate_defer_expr(condition, depth + 1, cleanup)?;
                    self.validate_defer_block(then_body, depth + 1, loops, cleanup)?;
                    if let Some(body) = else_body {
                        self.validate_defer_block(body, depth + 1, loops, cleanup)?;
                    }
                }
                CoreStatementKind::IfLet {
                    value,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                    self.validate_defer_block(then_body, depth + 1, loops, cleanup)?;
                    if let Some(body) = else_body {
                        self.validate_defer_block(body, depth + 1, loops, cleanup)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.validate_defer_expr(condition, depth + 1, cleanup)?;
                    self.validate_defer_block(body, depth + 1, loops + 1, cleanup)?;
                }
                CoreStatementKind::WhileLet { value, body, .. } => {
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                    self.validate_defer_block(body, depth + 1, loops + 1, cleanup)?;
                }
                CoreStatementKind::ForRange {
                    start, end, body, ..
                } => {
                    self.validate_defer_expr(start, depth + 1, cleanup)?;
                    self.validate_defer_expr(end, depth + 1, cleanup)?;
                    self.validate_defer_block(body, depth + 1, loops + 1, cleanup)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                    for arm in arms {
                        self.validate_defer_block(&arm.body, depth + 1, loops, cleanup)?;
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.validate_defer_block(body, depth + 1, loops, cleanup)?
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_defer_expr(&mut self, value: &CoreExpr, depth: usize, cleanup: bool) -> R<()> {
        self.defer_visit(depth, value.span)?;
        match &value.kind {
            CoreExprKind::Try(_) if cleanup => {
                return Err(error(
                    "E-DEFER-CONTROL",
                    value.span,
                    "a defer body cannot propagate an error outside its cleanup boundary",
                ));
            }
            CoreExprKind::Closure { body, .. } => {
                self.validate_defer_block(body, depth + 1, 0, false)?
            }
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::TypeApply { callee: value, .. }
            | CoreExprKind::Try(value)
            | CoreExprKind::CompileTime(value)
            | CoreExprKind::Unary { operand: value, .. } => {
                self.validate_defer_expr(value, depth + 1, cleanup)?
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                self.validate_defer_expr(left, depth + 1, cleanup)?;
                self.validate_defer_expr(right, depth + 1, cleanup)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                self.validate_defer_expr(callee, depth + 1, cleanup)?;
                for argument in arguments {
                    self.validate_defer_expr(argument, depth + 1, cleanup)?;
                }
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for value in values {
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                }
            }
            CoreExprKind::Member { object, .. } => {
                self.validate_defer_expr(object, depth + 1, cleanup)?
            }
            CoreExprKind::Slice { object, start, end } => {
                self.validate_defer_expr(object, depth + 1, cleanup)?;
                for endpoint in [start, end].into_iter().flatten() {
                    self.validate_defer_expr(endpoint, depth + 1, cleanup)?;
                }
            }
            CoreExprKind::Record { fields, .. } => {
                for (_, value) in fields {
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.validate_defer_expr(base, depth + 1, cleanup)?;
                for (_, value) in fields {
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                for (_, value) in bindings {
                    self.validate_defer_expr(value, depth + 1, cleanup)?;
                }
                self.validate_defer_expr(value, depth + 1, cleanup)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.validate_defer_expr(condition, depth + 1, cleanup)?;
                self.validate_defer_expr(then_value, depth + 1, cleanup)?;
                self.validate_defer_expr(else_value, depth + 1, cleanup)?;
            }
            _ => {}
        }
        Ok(())
    }
}
