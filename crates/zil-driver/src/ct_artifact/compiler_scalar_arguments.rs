//! Closed scalar nominal actuals retain their declared mathematical domain.
use super::*;
impl Compiler<'_> {
    pub(super) fn scalar_binding(&mut self, name: &str, span: Span) -> R<Option<Constant>> {
        for (actual, value) in &self.scalar_bindings {
            self.budget
                .work(name.len().min(actual.len()).saturating_add(1), span)?;
            if actual == name {
                self.budget.work(32, span)?;
                return Ok(Some(value.clone()));
            }
        }
        Ok(None)
    }
    pub(super) fn nominal_parameters(
        &mut self,
        parameters: &[s::GenericParam],
        span: Span,
    ) -> R<()> {
        for (i, parameter) in parameters.iter().enumerate() {
            self.budget.node(parameter.span)?;
            self.budget.text(&parameter.name, parameter.span)?;
            for prior in &parameters[..i] {
                self.budget.work(
                    prior.name.len().min(parameter.name.len()).saturating_add(1),
                    span,
                )?;
                if prior.name == parameter.name {
                    return Err(error(span, "duplicate nominal generic binder"));
                }
            }
            match &parameter.kind {
                s::GenericParamKind::Type(_) => {
                    self.generic_names(std::slice::from_ref(parameter), &[], span)?;
                }
                s::GenericParamKind::Const(ty) => {
                    self.scalar_argument_type(ty, span)?;
                }
                s::GenericParamKind::Lifetime => {
                    return Err(error(
                        span,
                        "nominal lifetime actual requires an explicit numeric scope",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn scalar_argument_type(&mut self, ty: &s::TypeRef, span: Span) -> R<CtScalarTy> {
        self.budget.node(span)?;
        let mut owner = None;
        for (index, module) in self.modules.iter().enumerate() {
            self.budget.work(1, span)?;
            if module.source.file_id == ty.span.file {
                owner = Some(index);
                break;
            }
        }
        let owner =
            owner.ok_or_else(|| error(span, "scalar declaration has no frozen source module"))?;
        // A declaration domain cannot capture its caller's generic binders.
        let old_data = std::mem::take(&mut self.bindings);
        let old_meta = std::mem::take(&mut self.metadata_bindings);
        let old_scalar = std::mem::take(&mut self.scalar_bindings);
        let result = self.ty(owner, ty, 0);
        self.bindings = old_data;
        self.metadata_bindings = old_meta;
        self.scalar_bindings = old_scalar;
        match result? {
            CtTy::Scalar(
                s @ (CtScalarTy::Bool
                | CtScalarTy::U8
                | CtScalarTy::U16
                | CtScalarTy::U32
                | CtScalarTy::U64
                | CtScalarTy::Usize
                | CtScalarTy::I32
                | CtScalarTy::I8
                | CtScalarTy::I16
                | CtScalarTy::Isize
                | CtScalarTy::I64),
            ) => Ok(s),
            _ => Err(error(
                span,
                "nominal scalar parameter is outside the ordinary const profile",
            )),
        }
    }
    pub(super) fn source_scalar_argument(
        &mut self,
        parameter: &s::GenericParam,
        arg: &s::GenericArg,
        span: Span,
    ) -> R<Constant> {
        let s::GenericParamKind::Const(ty) = &parameter.kind else {
            return Err(error(
                span,
                "scalar actual requires its declared const parameter",
            ));
        };
        let target = self.scalar_argument_type(ty, span)?;
        let value = match arg {
            s::GenericArg::AssociatedBinding(binding) => {
                return Err(error(
                    binding.span,
                    "associated equation is a trait premise, not a scalar actual",
                ));
            }
            s::GenericArg::Lifetime(_) => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "CT4 lifetime actual is not a scalar argument",
                    Some(span),
                )]);
            }
            s::GenericArg::Type(ty) => {
                let s::TypeKind::Named { path, args } = &ty.kind else {
                    return Err(error(span, "nominal scalar actual is not a value binder"));
                };
                if path.len() != 1 || !args.is_empty() {
                    return Err(error(span, "nominal scalar actual is not a value binder"));
                }
                self.scalar_binding(&path[0], span)?
                    .ok_or_else(|| error(span, "nominal scalar value binder is absent"))?
            }
            s::GenericArg::Const(expression) => {
                self.budget.node(expression.span)?;
                match &expression.kind {
                    s::ExprKind::Name(name) => self
                        .scalar_binding(name, span)?
                        .ok_or_else(|| error(span, "nominal scalar value binder is absent"))?,
                    s::ExprKind::Literal(s::Literal::Bool(value)) if target == CtScalarTy::Bool => {
                        Constant::Bool(*value)
                    }
                    s::ExprKind::Literal(s::Literal::Integer(text))
                        if target != CtScalarTy::Bool =>
                    {
                        self.budget.text(text, span)?;
                        let runtime = match target {
                            CtScalarTy::U8 => zil_ir::Type::U8,
                            CtScalarTy::U16 => zil_ir::Type::U16,
                            CtScalarTy::U32 => zil_ir::Type::U32,
                            CtScalarTy::I32 => zil_ir::Type::I32,
                            CtScalarTy::I8 => zil_ir::Type::I8,
                            CtScalarTy::I16 => zil_ir::Type::I16,
                            CtScalarTy::Isize => zil_ir::Type::Isize,
                            CtScalarTy::I64 => zil_ir::Type::I64,
                            CtScalarTy::U64 => zil_ir::Type::U64,
                            CtScalarTy::Usize => zil_ir::Type::Usize,
                            _ => unreachable!(),
                        };
                        zil_types::parse_integer_literal(text, Some(&runtime), false, span)?
                    }
                    _ => {
                        return Err(error(
                            span,
                            "nominal scalar actual needs completed verified CT evaluation",
                        ));
                    }
                }
            }
        };
        if expr::scalar_constant(&value) != CtTy::Scalar(target) {
            return Err(error(
                span,
                "nominal scalar actual differs from its declared domain",
            ));
        }
        Ok(value)
    }
    pub(super) fn source_expression_copy(&mut self, expression: &s::Expr, depth: usize) -> R<()> {
        struct Copy<'a, 'b>(&'a mut Budget<'b>);
        impl zil_expand::ct_selection::Visitor for Copy<'_, '_> {
            fn node(&mut self, span: Span, depth: usize) -> R<()> {
                self.0.depth(depth, span)?;
                self.0.node(span)?;
                Budget::take(
                    &mut self.0.bytes,
                    std::mem::size_of::<s::Expr>(),
                    span,
                    "CT source scalar AST copy bytes exhausted",
                )
            }
            fn text(&mut self, text: &str, span: Span) -> R<()> {
                self.0.text(text, span)
            }
        }
        zil_expand::ct_selection::walk_expression(&mut Copy(&mut self.budget), expression, depth)
    }
}
