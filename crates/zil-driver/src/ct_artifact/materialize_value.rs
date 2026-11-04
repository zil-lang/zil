//! Reify active data with its full independently verified type context.
use super::*;
use crate::ct_artifact::execute::value_context::ValueContext;
impl Materializer<'_, '_> {
    fn data_node(&mut self, kind: s::ExprKind, span: Span) -> CompileResult<s::Expr> {
        self.environment.budget.reserve_generated_nodes(1)?;
        self.environment.budget.reserve_analysis_work(1)?;
        self.environment
            .budget
            .reserve_temporary_bytes(std::mem::size_of::<s::Expr>() as u64)?;
        Ok(s::Expr { kind, span })
    }
    fn data_type_expr(&mut self, ty: &s::TypeRef, span: Span) -> CompileResult<s::Expr> {
        let s::TypeKind::Named { path, args } = &ty.kind else {
            return Err(runtime(
                "data constructor needs a complete nominal type",
                span,
            ));
        };
        let mut path = path.iter();
        let first = path
            .next()
            .ok_or_else(|| runtime("empty data type path", span))?;
        self.environment
            .budget
            .reserve_analysis_work(first.len() as u64)?;
        let mut value = self.data_node(s::ExprKind::Name(first.clone()), span)?;
        for name in path {
            self.environment
                .budget
                .reserve_analysis_work(name.len() as u64)?;
            value = self.data_node(
                s::ExprKind::Member {
                    object: Box::new(value),
                    member: s::MemberName::Name(name.clone()),
                },
                span,
            )?;
        }
        if !args.is_empty() {
            self.environment
                .budget
                .reserve_analysis_work(args.len() as u64)?;
            value = self.data_node(
                s::ExprKind::TypeApply {
                    callee: Box::new(value),
                    arguments: args.clone(),
                },
                span,
            )?;
        }
        Ok(value)
    }
    fn data_record_names(
        &mut self,
        symbol: &CtSymbolId,
        count: usize,
        span: Span,
    ) -> CompileResult<Vec<String>> {
        fn fields<'a>(this: &'a Materializer<'_, '_>, symbol: &CtSymbolId, count: usize, span: Span) -> CompileResult<&'a [s::Field]> {
            let (_, parsed) = this.parsed_source(symbol.module)?;
            let item = parsed.module.items.get(symbol.declaration.0 as usize)
                .ok_or_else(|| runtime("returned record declaration is missing", span))?;
            let s::ItemKind::Aggregate(declaration) = &item.kind else {
                return Err(runtime("returned record is not a source declaration", span));
            };
            if declaration.fields.len() != count {
                return Err(runtime("returned record field count differs", span));
            }
            Ok(&declaration.fields)
        }
        let bytes = fields(self, symbol, count, span)?.iter().try_fold(0u64, |bytes, field| {
            bytes.checked_add(field.name.len() as u64)
                .ok_or_else(|| runtime("returned record name storage overflow", span))
        })?;
        self.environment.budget.reserve_analysis_work(bytes.saturating_add(count as u64))?;
        self.environment.budget.reserve_temporary_bytes(bytes.saturating_add((count as u64).saturating_mul(24)))?;
        Ok(fields(self, symbol, count, span)?.iter().map(|field| field.name.clone()).collect())
    }
    pub(super) fn data_expression(
        &mut self,
        module: ModuleId,
        value: &CtValue,
        ty: &CtTy,
        context: &ValueContext,
        span: Span,
        depth: usize,
    ) -> CompileResult<s::Expr> {
        self.depth(depth, span)?;
        self.environment.budget.reserve_analysis_work(1)?;
        let graph = context.type_ref(ty, &mut self.environment.budget)?;
        let source_ty = self.owner_node(&graph, graph.root, module, span, depth + 1)?;
        let kind = match (value, ty) {
            (CtValue::Scalar(c), CtTy::Scalar(_)) => {
                self.environment.budget.reserve_analysis_work(32)?;
                self.environment.budget.reserve_temporary_bytes(32)?;
                match c {
                    zil_ir::Constant::Unit => s::ExprKind::Tuple(Vec::new()),
                    zil_ir::Constant::Bool(v) => s::ExprKind::Literal(s::Literal::Bool(*v)),
                    zil_ir::Constant::U8(v) => {
                        s::ExprKind::Literal(s::Literal::Integer(v.to_string()))
                    }
                    zil_ir::Constant::U16(v) => {
                        s::ExprKind::Literal(s::Literal::Integer(v.to_string()))
                    }
                    zil_ir::Constant::I32(v) => {
                        s::ExprKind::Literal(s::Literal::Integer(v.to_string()))
                    }
                    zil_ir::Constant::I64(v) => {
                        s::ExprKind::Literal(s::Literal::Integer(v.to_string()))
                    }
                    zil_ir::Constant::U64(v) => {
                        s::ExprKind::Literal(s::Literal::Integer(format!("{v}u64")))
                    }
                    zil_ir::Constant::Usize(v) => {
                        s::ExprKind::Literal(s::Literal::Integer(format!("{v}usize")))
                    }
                    zil_ir::Constant::F32(_) | zil_ir::Constant::F64(_) => {
                        float_expression(c, &mut self.environment.budget, span)?.kind
                    }
                    _ => return Err(runtime("CT pointer data cannot enter runtime syntax", span)),
                }
            }
            (CtValue::Tuple(values), CtTy::Tuple(types)) if values.len() == types.len() => {
                let mut fields = Vec::new();
                for (v, t) in values.iter().zip(types) {
                    fields.push(self.data_expression(module, v, t, context, span, depth + 1)?);
                }
                s::ExprKind::Tuple(fields)
            }
            (CtValue::Array(values), CtTy::Array { element, length })
                if values.len() == *length as usize =>
            {
                let mut fields = Vec::new();
                for value in values {
                    fields.push(self.data_expression(
                        module,
                        value,
                        element,
                        context,
                        span,
                        depth + 1,
                    )?);
                }
                s::ExprKind::Array(fields)
            }
            (CtValue::Record { schema, fields }, CtTy::Record(id)) if schema == id => {
                let shape = context
                    .records
                    .get(id.0 as usize)
                    .ok_or_else(|| runtime("returned record schema is missing", span))?;
                let field_names = self.data_record_names(&shape.name, fields.len(), span)?;
                if fields.len() != shape.fields.len() {
                    return Err(runtime("returned record field count differs", span));
                }
                let mut output = Vec::new();
                for ((field, ty), name) in fields.iter().zip(&shape.fields).zip(field_names) {
                    let value =
                        self.data_expression(module, field, ty, context, span, depth + 1)?;
                    output.push(s::RecordField {
                        name,
                        value: Some(value),
                        span,
                    });
                }
                s::ExprKind::Record {
                    ty: Some(Box::new(self.data_type_expr(&source_ty, span)?)),
                    base: None,
                    fields: output,
                }
            }
            (
                CtValue::AnonymousSum {
                    members,
                    active,
                    payload,
                },
                CtTy::AnonymousSum(expected),
            ) => {
                if members != expected {
                    return Err(runtime("returned sum descriptor differs", span));
                }
                let member = expected
                    .get(*active as usize)
                    .ok_or_else(|| runtime("returned sum active member is absent", span))?;
                let payload =
                    self.data_expression(module, payload, member, context, span, depth + 1)?;
                return self.data_node(
                    s::ExprKind::Ascribed {
                        ty: source_ty,
                        value: Box::new(payload),
                    },
                    span,
                );
            }
            _ => {
                return Err(runtime(
                    "CT result has no complete ordinary data reification",
                    span,
                ));
            }
        };
        let value = self.data_node(kind, span)?;
        self.data_node(
            s::ExprKind::Ascribed {
                ty: source_ty,
                value: Box::new(value),
            },
            span,
        )
    }
}
