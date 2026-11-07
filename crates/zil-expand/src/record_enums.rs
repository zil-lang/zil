//! Preserve named source fields until their real enum declaration is known.
use super::*;
use generics::{Budget, record_enums as labels};

impl<'a> Expander<'a> {
    pub(super) fn enum_meter<T>(
        &mut self,
        operation: impl FnOnce(&mut Budget<'_>) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        let limits = Limits {
            max_analysis_steps: self
                .member_owner_limit
                .saturating_sub(self.member_owner_work)
                .min(self.callable_work),
            max_nodes: self.member_owner_nodes,
            max_call_depth: self.member_owner_depth,
            ..Limits::default()
        };
        let mut meter = Budget::new(&limits);
        let result = operation(&mut meter);
        let remaining = meter.remaining_limits();
        let work = limits.max_analysis_steps - remaining.max_analysis_steps;
        self.member_owner_work += work;
        self.callable_work -= work;
        self.member_owner_nodes = remaining.max_nodes;
        result
    }

    fn source_enum(
        &mut self,
        name: &str,
        span: Span,
    ) -> Result<Option<&'a s::EnumDecl>, Diagnostic> {
        let sources = self.enum_sources;
        self.enum_meter(|meter| {
            for module in sources {
                for item in &module.items {
                    meter.node(item.span, 0)?;
                    if let s::ItemKind::Enum(declaration) = &item.kind {
                        meter.text(name, span)?;
                        meter.text(&declaration.name, item.span)?;
                        if declaration.name == name {
                            return Ok(Some(declaration));
                        }
                    }
                }
            }
            Ok(None)
        })
    }

    pub(super) fn record_enum(
        &mut self,
        span: Span,
        written: Option<&s::Expr>,
        base: Option<&s::Expr>,
        fields: &[s::RecordField],
    ) -> Option<CoreExpr> {
        let written = written?;
        let s::ExprKind::Member {
            object,
            member: s::MemberName::Name(variant),
        } = &written.kind
        else {
            return None;
        };
        let s::ExprKind::Name(name) = &object.kind else {
            return None;
        };
        let result = self.record_enum_inner(span, written, name, variant, base, fields);
        match result {
            Ok(value) => value,
            Err(error) => {
                self.diagnostics.push(error);
                Some(self.expr_node(CoreExprKind::Unit, span))
            }
        }
    }

    fn record_enum_inner(
        &mut self,
        span: Span,
        written: &s::Expr,
        name: &str,
        variant: &str,
        base: Option<&s::Expr>,
        fields: &[s::RecordField],
    ) -> Result<Option<CoreExpr>, Diagnostic> {
        let Some(declaration) = self.source_enum(name, span)? else {
            return Ok(None);
        };
        let (declared, order) = self.enum_meter(|meter| {
            let declared = labels::fields(declaration, variant, span, meter)?;
            if base.is_some() {
                return Err(Diagnostic::unsupported(
                    Phase::Expand,
                    "record enum spread is not implemented",
                    Some(span),
                ));
            }
            let order = labels::order(
                declared,
                fields.iter().map(|f| (f.name.as_str(), f.span)),
                span,
                meter,
            )?;
            for field in declared {
                meter.node(field.span, 0)?;
                meter.node(field.span, 0)?;
                meter.ty(&field.ty)?;
            }
            for _ in 0..4 {
                meter.node(span, 0)?;
            }
            Ok((declared, order))
        })?;
        let mut bindings = Vec::new();
        let mut payload = vec![None; declared.len()];
        for (field, index) in fields.iter().zip(order) {
            let shorthand;
            let value = if let Some(value) = &field.value {
                value
            } else {
                self.enum_meter(|meter| {
                    meter.node(field.span, 0)?;
                    meter.text(&field.name, field.span)
                })?;
                shorthand = s::Expr {
                    kind: s::ExprKind::Name(field.name.clone()),
                    span: field.span,
                };
                &shorthand
            };
            let expected = type_ref(&declared[index].ty);
            self.enum_meter(|meter| meter.node(field.span, 0))?;
            let value = self.expr_expected(value, Some(&expected));
            let value = self.expr_node(
                CoreExprKind::Ascribed {
                    ty: expected,
                    value: Box::new(value),
                },
                field.span,
            );
            let digits = self
                .next_temporary
                .checked_ilog10()
                .map_or(1, |n| n as usize + 1);
            self.enum_meter(|meter| {
                meter.node(field.span, 0)?;
                meter.node(field.span, 0)?;
                for _ in 0..3 {
                    meter.text("$enum_field_", field.span)?;
                    for _ in 0..digits {
                        meter.text("0", field.span)?;
                    }
                }
                Ok(())
            })?;
            let temporary = self.temporary("enum_field");
            self.role(value.id, ExpansionRole::RecordField, vec![span]);
            bindings.push((temporary.clone(), value));
            payload[index] = Some(self.temporary_read(&temporary, field.span));
        }
        let callee = self.expr(written);
        let value = self.expr_node(
            CoreExprKind::Call {
                callee: Box::new(callee),
                arguments: payload
                    .into_iter()
                    .map(|x| x.expect("complete named payload"))
                    .collect(),
            },
            span,
        );
        self.role(value.id, ExpansionRole::RecordResult, Vec::new());
        Ok(Some(self.expr_node(
            CoreExprKind::Sequence {
                bindings,
                value: Box::new(value),
            },
            span,
        )))
    }

    pub(super) fn record_enum_pattern(
        &mut self,
        path: &[String],
        fields: &[s::RecordPatternField],
        span: Span,
    ) -> Result<CorePatternKind, Diagnostic> {
        if path.len() != 2 {
            return Err(Diagnostic::unsupported(
                Phase::Expand,
                "named enum patterns require a qualified variant",
                Some(span),
            ));
        }
        self.enum_meter(|meter| meter.text(&path[0], span))?;
        let name = path[0].clone();
        let declaration = self.source_enum(&name, span)?.ok_or_else(|| {
            Diagnostic::error(
                Phase::Expand,
                "E-ENUM-FIELD",
                Some(span),
                "named pattern requires a declared enum",
            )
        })?;
        let variant = path.last().expect("qualified variant");
        let order = self.enum_meter(|meter| {
            let declared = labels::fields(declaration, variant, span, meter)?;
            meter.text(&name, span)?;
            meter.text(variant, span)?;
            for field in fields {
                meter.node(field.span, 0)?;
                meter.node(field.span, 0)?;
            }
            labels::order(
                declared,
                fields.iter().map(|f| (f.name.as_str(), f.span)),
                span,
                meter,
            )
        })?;
        let mut ordered = vec![None; fields.len()];
        for (field, index) in fields.iter().zip(order) {
            ordered[index] = Some(self.pattern(&field.pattern));
        }
        Ok(CorePatternKind::Variant {
            ty: Type::Named(name),
            variant: variant.clone(),
            fields: ordered
                .into_iter()
                .map(|p| p.expect("complete named pattern"))
                .collect(),
        })
    }
}
