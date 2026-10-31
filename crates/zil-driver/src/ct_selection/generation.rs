//! Finite source generation. VM results remain syntax, never checked authority.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct ForTrace {
    pub module: ModuleId,
    pub iterable: Span,
    pub binding: String,
    pub iterations: usize,
    pub dependencies: Vec<Dependency>,
    pub steps: u64,
    pub nodes: usize,
}

pub(super) struct Binding {
    pub name: String,
    pub value: s::Expr,
    pub scope: usize,
}

fn unsupported(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Ct, message, Some(span))]
}

pub(super) fn integer(value: &s::Expr) -> Option<(i128, &'static str)> {
    let s::ExprKind::Literal(s::Literal::Integer(text)) = &value.kind else {
        return None;
    };
    for suffix in ["u8", "u16", "i32", "i64"] {
        if let Some(number) = text.strip_suffix(suffix) {
            return number.parse().ok().map(|number| (number, suffix));
        }
    }
    None
}

fn scalar(value: CoreExpr, budget: &mut Budget) -> CompileResult<s::Expr> {
    let span = value.span;
    budget.node(span, 0)?;
    let kind = match value.kind {
        CoreExprKind::Unit => s::ExprKind::Tuple(Vec::new()),
        CoreExprKind::Bool(value) => s::ExprKind::Literal(s::Literal::Bool(value)),
        CoreExprKind::Integer(value) => {
            budget.text(&value, span)?;
            s::ExprKind::Literal(s::Literal::Integer(value))
        }
        _ => {
            return Err(unsupported(
                span,
                "#for elements require independently evaluated Unit, bool or integer scalars",
            ));
        }
    };
    Ok(s::Expr { kind, span })
}

impl materialize::Materializer<'_, '_> {
    /// The wrapper intentionally supplies no expected type. The ordinary raw
    /// compiler infers the array shape and the independent CT gate checks it.
    fn array_query(
        &mut self,
        expression: &s::Expr,
    ) -> CompileResult<(Vec<s::Expr>, Vec<Dependency>)> {
        let dependencies =
            self.context
                .dependencies(self.module, expression, &self.locals, self.budget)?;
        let cost = syntax::expression_cost(expression, &self.budget.remaining)?;
        self.budget.copy(cost, expression.span)?;
        self.budget.copy(self.context.cost, expression.span)?;
        self.budget.spend(8, 32, expression.span)?;
        let (raw, next) =
            syntax::expression_with_next_id(self.module, expression, &self.budget.remaining)?;
        let span = expression.span;
        let wrapper = next.0.checked_add(3).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "#for query identity range exhausted",
                Some(span),
            )]
        })?;
        let query = CoreModule {
            schema_version: SCHEMA_VERSION,
            module_id: self.module,
            source_fingerprint: self.context.program.source_fingerprint,
            functions: vec![CoreFunction {
                id: next,
                name: "$generation222".into(),
                span,
                parameters: Vec::new(),
                return_type: Type::Unit,
                receiver: None,
                result_mode: PassingMode::Infer,
                foreign: None,
                callable_return: None,
                type_parameters: Vec::new(),
                state_parameters: Vec::new(),
                callable_parameters: Vec::new(),
                body: CoreBlock {
                    span,
                    statements: vec![CoreStatement {
                        id: NodeId(wrapper - 2),
                        span,
                        kind: CoreStatementKind::Expression(CoreExpr {
                            id: NodeId(wrapper - 1),
                            span,
                            kind: CoreExprKind::CompileTime(Box::new(raw)),
                        }),
                    }],
                },
            }],
            aggregates: Vec::new(),
            enums: Vec::new(),
            unsupported: Vec::new(),
        };
        let compiler = super::super::ct_program::Compiler {
            program: &self.context.program,
            module: self.module,
            target: self.target,
        };
        let context =
            zil_ct::SourceProgramContext::new(&self.context.program, &self.budget.remaining)?;
        let (evaluated, usage) = zil_ct::evaluate_source_constants_with_compiler(
            &query,
            self.target,
            &self.budget.remaining,
            Some(&context),
            &compiler,
        )?;
        self.budget.execute(usage, span)?;
        let mut functions = evaluated.functions.into_iter();
        let Some(function) = functions.next().filter(|_| functions.next().is_none()) else {
            return Err(forbidden(
                span,
                "#for query produced an invalid function count",
            ));
        };
        let mut statements = function.body.statements.into_iter();
        let Some(statement) = statements.next().filter(|_| statements.next().is_none()) else {
            return Err(forbidden(
                span,
                "#for query produced an invalid statement count",
            ));
        };
        let CoreStatementKind::Expression(value) = statement.kind else {
            return Err(forbidden(span, "#for query must produce a scalar array"));
        };
        let CoreExprKind::Ascribed {
            value,
            ty: Type::Array { .. },
        } = value.kind
        else {
            return Err(forbidden(
                span,
                "#for query must produce a typed scalar array",
            ));
        };
        let CoreExprKind::Array(values) = value.kind else {
            return Err(forbidden(span, "#for query must reify an array value"));
        };
        self.budget.spend(values.len(), values.len(), span)?;
        let values = values
            .into_iter()
            .map(|value| scalar(value, self.budget))
            .collect::<CompileResult<Vec<_>>>()?;
        Ok((values, dependencies))
    }

    fn values(
        &mut self,
        binding: &str,
        iterable: &mut s::Expr,
        depth: usize,
    ) -> CompileResult<Vec<s::Expr>> {
        // Only an enclosing generation frame can substitute here. A receiving
        // runtime binding still goes through the ordinary forbidden-name gate.
        if !self.iterations.is_empty() {
            self.expression(iterable, depth + 1)?;
        }
        let before = self.budget.usage();
        let span = iterable.span;
        let mut inner = &*iterable;
        while let s::ExprKind::Group(value) = &inner.kind {
            self.budget.node(inner.span, depth)?;
            inner = value;
        }
        let (values, dependencies) = match &inner.kind {
            s::ExprKind::Array(_) => self.array_query(inner)?,
            s::ExprKind::Binary {
                op: s::BinaryOp::Range,
                left,
                right,
            } => {
                // Both endpoints are ordinary expressions in one homogeneous
                // CT array. This preserves target integer inference and traps.
                let left_cost = syntax::expression_cost(left, &self.budget.remaining)?;
                self.budget.copy(left_cost, left.span)?;
                let right_cost = syntax::expression_cost(right, &self.budget.remaining)?;
                self.budget.copy(right_cost, right.span)?;
                self.budget.node(span, depth)?;
                let endpoints = s::Expr {
                    span,
                    kind: s::ExprKind::Array(vec![(**left).clone(), (**right).clone()]),
                };
                let (endpoints, dependencies) = self.array_query(&endpoints)?;
                let [start, end] = endpoints.as_slice() else {
                    return Err(forbidden(span, "#for range requires exactly two endpoints"));
                };
                let (Some((start, suffix)), Some((end, end_suffix))) =
                    (integer(start), integer(end))
                else {
                    return Err(unsupported(span, "#for range endpoints must be integers"));
                };
                if suffix != end_suffix {
                    return Err(forbidden(
                        span,
                        "#for range endpoint integer types disagree",
                    ));
                }
                let count = usize::try_from((end - start).max(0)).map_err(|_| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "#for range exceeds generation budget",
                        Some(span),
                    )]
                })?;
                // Reserve the full materialized value vector before allocating
                // it; arithmetic below only enumerates already verified values.
                self.budget.spend(count, count.saturating_mul(32), span)?;
                let values = (0..count)
                    .map(|offset| s::Expr {
                        span,
                        kind: s::ExprKind::Literal(s::Literal::Integer(format!(
                            "{}{suffix}",
                            start + offset as i128
                        ))),
                    })
                    .collect();
                (values, dependencies)
            }
            _ => {
                return Err(unsupported(
                    span,
                    "#for requires a finite integer range or explicit scalar array",
                ));
            }
        };
        self.budget.node(span, depth)?;
        self.budget.text(binding, span)?;
        let after = self.budget.usage();
        self.for_trace.push(ForTrace {
            module: self.module,
            iterable: span,
            binding: binding.into(),
            iterations: values.len(),
            dependencies,
            steps: after.steps - before.steps,
            nodes: after.nodes - before.nodes,
        });
        Ok(values)
    }

    fn enter_iteration(&mut self, binding: &str, value: s::Expr) -> CompileResult<u64> {
        let span = value.span;
        self.budget
            .spend(2, binding.len().saturating_add(2), span)?;
        let serial = self.next_generation;
        self.next_generation = serial.checked_add(1).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "#for identity budget exhausted",
                Some(span),
            )]
        })?;
        self.iterations.push(Binding {
            name: binding.into(),
            value,
            scope: self.locals.len(),
        });
        self.locals.push(Vec::new());
        Ok(serial)
    }

    pub(super) fn for_items(
        &mut self,
        binding: &str,
        iterable: &mut s::Expr,
        items: &[s::Item],
        depth: usize,
    ) -> CompileResult<Vec<s::Item>> {
        let values = self.values(binding, iterable, depth)?;
        let mut output = Vec::new();
        for value in values {
            for item in items {
                let cost = syntax::item_cost(item, &self.budget.remaining)?;
                self.budget.copy(cost, item.span)?;
            }
            self.budget.spend(items.len(), items.len(), iterable.span)?;
            let mut selected = items.to_vec();
            let serial = self.enter_iteration(binding, value)?;
            let result = self.items(&mut selected, depth + 1, true);
            self.locals.pop();
            self.iterations.pop();
            result?;
            super::hygiene::items(
                &mut selected,
                self.module,
                serial,
                self.budget,
                &self.context.program,
            )?;
            for item in selected {
                self.budget.node(item.span, depth + 1)?;
                output.push(item);
            }
        }
        Ok(output)
    }

    pub(super) fn for_statements(
        &mut self,
        binding: &str,
        iterable: &mut s::Expr,
        body: &s::Block,
        depth: usize,
    ) -> CompileResult<Vec<s::Stmt>> {
        let values = self.values(binding, iterable, depth)?;
        let mut output = Vec::new();
        for value in values {
            let cost = syntax::block_cost(body, &self.budget.remaining)?;
            self.budget.copy(cost, body.span)?;
            let mut selected = body.clone();
            let serial = self.enter_iteration(binding, value)?;
            let result = self.statements(&mut selected.statements, depth + 1);
            self.locals.pop();
            self.iterations.pop();
            result?;
            super::hygiene::statements(&mut selected.statements, self.module, serial, self.budget)?;
            for statement in selected.statements {
                self.budget.node(statement.span, depth + 1)?;
                output.push(statement);
            }
        }
        Ok(output)
    }

    pub(super) fn substitute(&mut self, expression: &mut s::Expr) -> CompileResult<()> {
        let s::ExprKind::Name(name) = &expression.kind else {
            return Ok(());
        };
        for binding in self.iterations.iter().rev() {
            self.budget.text(name, expression.span)?;
            if binding.name != *name {
                continue;
            }
            for scope in &self.locals[binding.scope..] {
                for local in scope {
                    self.budget.text(name, expression.span)?;
                    if local == name {
                        return Ok(());
                    }
                }
            }
            let cost = syntax::expression_cost(&binding.value, &self.budget.remaining)?;
            self.budget.copy(cost, expression.span)?;
            let mut value = binding.value.clone();
            value.span = expression.span;
            *expression = value;
            return Ok(());
        }
        Ok(())
    }
}
