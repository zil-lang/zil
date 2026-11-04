//! Closed associated equations use the same verified source CT compiler as
//! other CT artifacts. Symbolic host equations stay in their own checking lane.
use super::*;
use zil_expand::ct_selection::Visitor;

pub(super) fn scalar(
    mut ty: &s::TypeRef,
    budget: &mut zil_ct::CtExecutionBudget,
    maximum_depth: usize,
) -> CompileResult<Option<CtTy>> {
    for depth in 0..=maximum_depth {
        budget.reserve_analysis_work(1)?;
        match &ty.kind {
            s::TypeKind::Group(inner) if depth < maximum_depth => ty = inner,
            s::TypeKind::Group(_) => {
                return Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "associated constant type depth exhausted",
                    Some(ty.span),
                )]);
            }
            s::TypeKind::Named { path, args } if args.is_empty() && path.len() == 1 => {
                budget.reserve_analysis_work(path[0].len() as u64)?;
                return Ok(compiler::scalar_type(&path[0]).map(CtTy::Scalar));
            }
            _ => return Ok(None),
        }
    }
    unreachable!()
}
fn literal(
    mut value: &s::Expr,
    budget: &mut zil_ct::CtExecutionBudget,
    maximum_depth: usize,
) -> CompileResult<bool> {
    for depth in 0..=maximum_depth {
        budget.reserve_analysis_work(1)?;
        match &value.kind {
            s::ExprKind::Literal(_) => return Ok(true),
            s::ExprKind::Group(inner) if depth < maximum_depth => value = inner,
            s::ExprKind::Group(_) => {
                return Err(vec![Diagnostic::limit(
                    Phase::Ct,
                    "associated constant value depth exhausted",
                    Some(value.span),
                )]);
            }
            s::ExprKind::Unary {
                op: s::UnaryOp::Negate,
                operand,
            } => return Ok(matches!(operand.kind, s::ExprKind::Literal(_))),
            _ => return Ok(false),
        }
    }
    unreachable!()
}
pub(super) struct Dependencies<'a, 'b> {
    pub(super) parameters: &'a [s::GenericParam],
    pub(super) budget: &'b mut zil_ct::CtExecutionBudget,
    pub(super) symbolic: bool,
    pub(super) self_dependent: bool,
}
impl Dependencies<'_, '_> {
    fn name(&mut self, name: &str) -> CompileResult<()> {
        self.budget
            .reserve_analysis_work(name.len().min(4) as u64 + 1)?;
        self.symbolic |= name == "Self";
        self.self_dependent |= name == "Self";
        for parameter in self.parameters {
            self.budget
                .reserve_analysis_work(name.len().min(parameter.name.len()) as u64 + 1)?;
            self.symbolic |= name == parameter.name;
        }
        Ok(())
    }
}
impl Visitor for Dependencies<'_, '_> {
    fn node(&mut self, _span: Span, _depth: usize) -> CompileResult<()> {
        self.budget.reserve_analysis_work(1)
    }
    fn text(&mut self, text: &str, _span: Span) -> CompileResult<()> {
        self.budget.reserve_analysis_work(text.len() as u64)
    }
    fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
        if let s::TypeKind::Named { path, .. } = &ty.kind
            && let Some(name) = path.first()
        {
            self.name(name)?;
        }
        zil_expand::ct_selection::walk_type(self, ty, depth)
    }
    fn expression(&mut self, expression: &s::Expr, depth: usize) -> CompileResult<()> {
        if let s::ExprKind::Name(name) = &expression.kind {
            self.name(name)?;
        }
        zil_expand::ct_selection::walk_expression(self, expression, depth)
    }
}
impl Materializer<'_, '_> {
    pub(super) fn associated_constants(
        &mut self,
        module: ModuleId,
        members: &mut [s::Item],
        parameters: &[s::GenericParam],
        inherited: &CtSymbolId,
        defer_self: bool,
    ) -> CompileResult<()> {
        for (ordinal, item) in members.iter_mut().enumerate() {
            self.environment.budget.reserve_analysis_work(1)?;
            let s::ItemKind::Const(constant) = &mut item.kind else {
                continue;
            };
            let Some(expression) = &mut constant.value else {
                continue;
            };
            if literal(expression, &mut self.environment.budget, self.max_depth)? {
                continue;
            }
            let Some(ty) = &constant.ty else { continue };
            let Some(expected) = scalar(ty, &mut self.environment.budget, self.max_depth)? else {
                continue;
            };
            // Cost/depth preflight precedes the dependency walk, compiler work
            // and output allocation. No symbolic binder is replaced by a value.
            let limits = self.environment.remaining_limits_for_input();
            let cost = zil_expand::ct_selection::expression_cost(expression, &limits)?;
            self.environment
                .budget
                .reserve_analysis_work(cost.work as u64)?;
            let mut dependencies = Dependencies {
                parameters,
                budget: &mut self.environment.budget,
                symbolic: false,
                self_dependent: false,
            };
            dependencies.expression(expression, 0)?;
            if dependencies.symbolic {
                if dependencies.self_dependent && !defer_self {
                    self.environment
                        .budget
                        .reserve_analysis_work(self.self_constant_sources.len() as u64 + 1)?;
                    if self.generated_context
                        || inherited.generation_path.is_some()
                        || !self.self_constant_sources.contains(&(
                            module,
                            inherited.declaration.0 as usize,
                            ordinal,
                        ))
                    {
                        return Err(runtime(
                            "Self-dependent associated equation has no original-source CT plan",
                            expression.span,
                        ));
                    }
                }
                continue;
            }
            let owner = self.owner(module, item.span, Some(inherited))?;
            let value = self.expression_value(module, &owner, expression, Some(expected))?;
            let CtValue::Scalar(value) = value else {
                return Err(category(
                    "associated constant scalar receiving type differs",
                    expression.span,
                ));
            };
            let span = expression.span;
            // Scalar decimal output is at most 25 bytes on admitted 32/64-bit
            // targets; prepay formatting before allocating its String.
            self.environment.budget.reserve_analysis_work(26)?;
            self.environment.budget.reserve_temporary_bytes(26)?;
            let spelling = match &value {
                zil_ir::Constant::Unit => {
                    return Err(runtime(
                        "associated unit constants require their ordinary value representation",
                        span,
                    ));
                }
                zil_ir::Constant::Bool(value) => {
                    self.environment.budget.reserve_generated_nodes(1)?;
                    expression.kind = s::ExprKind::Literal(s::Literal::Bool(*value));
                    self.environment.budget.release_temporary_bytes(26);
                    self.environment.budget.release_temporary_bytes(16);
                    continue;
                }
                zil_ir::Constant::U8(value) => format!("{value}u8"),
                zil_ir::Constant::U16(value) => format!("{value}u16"),
                zil_ir::Constant::I32(value) => format!("{value}i32"),
                zil_ir::Constant::I64(value) => format!("{value}i64"),
                zil_ir::Constant::U64(value) => format!("{value}u64"),
                zil_ir::Constant::Usize(value) => format!("{value}usize"),
                zil_ir::Constant::F32(_) | zil_ir::Constant::F64(_) => {
                    self.environment.budget.release_temporary_bytes(26);
                    *expression = float_expression(&value, &mut self.environment.budget, span)?;
                    self.environment.budget.release_temporary_bytes(16);
                    continue;
                }
                _ => {
                    return Err(runtime(
                        "associated constant CT result is not an ordinary scalar",
                        span,
                    ));
                }
            };
            self.environment
                .budget
                .release_temporary_bytes(26 - spelling.len() as u64);
            self.environment.budget.reserve_generated_nodes(1)?;
            expression.kind = s::ExprKind::Literal(s::Literal::Integer(spelling));
            self.environment.budget.release_temporary_bytes(16);
        }
        Ok(())
    }
}
