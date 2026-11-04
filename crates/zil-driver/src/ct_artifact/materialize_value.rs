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
            self.data_type_copy(ty)?;
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
    fn data_type_copy(&mut self, ty: &s::TypeRef) -> CompileResult<()> {
        struct Copy<'a> {
            budget: &'a mut zil_ct::CtExecutionBudget,
            max_depth: usize,
        }
        impl zil_expand::ct_selection::Visitor for Copy<'_> {
            fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
                if depth > self.max_depth {
                    return Err(vec![Diagnostic::limit(
                        Phase::Ct,
                        "data type clone nesting exceeds budget",
                        Some(span),
                    )]);
                }
                self.budget.reserve_analysis_work(1)?;
                self.budget.reserve_generated_nodes(1)?;
                self.budget.reserve_temporary_bytes(256)
            }
            fn text(&mut self, text: &str, _: Span) -> CompileResult<()> {
                self.budget.reserve_analysis_work(text.len() as u64)?;
                self.budget.reserve_temporary_bytes(text.len() as u64)
            }
        }
        zil_expand::ct_selection::walk_type(
            &mut Copy {
                budget: &mut self.environment.budget,
                max_depth: self.max_depth,
            },
            ty,
            0,
        )
    }
    fn data_source_type(
        &mut self,
        context: &ValueContext,
        ty: &CtTy,
        module: ModuleId,
        span: Span,
        depth: usize,
    ) -> CompileResult<s::TypeRef> {
        let before = self.environment.budget.live_temporary_bytes();
        let graph = context.type_with_scopes(ty, &mut self.environment.budget);
        let graph_bytes = self
            .environment
            .budget
            .live_temporary_bytes()
            .saturating_sub(before);
        let result = (|| {
            let (graph,scopes) = graph?;
            // Pay the newly generated AST's complete storage independently of
            // the intermediate graph, which is released immediately below.
            let mut name_bytes = 0u64;
            for node in &graph.nodes {
                self.environment.budget.reserve_analysis_work(1)?;
                if let CtTypeNode::Nominal { symbol, .. } = node {
                    let (_, parsed) = self.parsed_source(symbol.module)?;
                    let item = parsed
                        .module
                        .items
                        .get(symbol.declaration.0 as usize)
                        .ok_or_else(|| runtime("data type declaration is missing", span))?;
                    let name = match &item.kind {
                        s::ItemKind::Aggregate(a) => &a.name,
                        s::ItemKind::Enum(e) => &e.name,
                        _ => return Err(runtime("data type has no nominal declaration", span)),
                    };
                    name_bytes = name_bytes.saturating_add(name.len() as u64);
                }
            }
            self.environment.budget.reserve_analysis_work(name_bytes)?;
            self.environment.budget.reserve_temporary_bytes(
                name_bytes.saturating_add((graph.nodes.len() as u64).saturating_mul(256)),
            )?;
            let old=std::mem::replace(&mut self.data_source_scopes,scopes);
            let result=self.owner_node(&graph, graph.root, module, span, depth + 1);
            self.data_source_scopes=old;
            result
        })();
        self.environment.budget.release_temporary_bytes(graph_bytes);
        result
    }
    fn data_record_names(
        &mut self,
        symbol: &CtSymbolId,
        count: usize,
        span: Span,
    ) -> CompileResult<Vec<String>> {
        fn fields<'a>(
            this: &'a Materializer<'_, '_>,
            symbol: &CtSymbolId,
            count: usize,
            span: Span,
        ) -> CompileResult<&'a [s::Field]> {
            let (_, parsed) = this.parsed_source(symbol.module)?;
            let item = parsed
                .module
                .items
                .get(symbol.declaration.0 as usize)
                .ok_or_else(|| runtime("returned record declaration is missing", span))?;
            let s::ItemKind::Aggregate(declaration) = &item.kind else {
                return Err(runtime("returned record is not a source declaration", span));
            };
            if declaration.fields.len() != count {
                return Err(runtime("returned record field count differs", span));
            }
            Ok(&declaration.fields)
        }
        let bytes = fields(self, symbol, count, span)?
            .iter()
            .try_fold(0u64, |bytes, field| {
                bytes
                    .checked_add(field.name.len() as u64)
                    .ok_or_else(|| runtime("returned record name storage overflow", span))
            })?;
        self.environment
            .budget
            .reserve_analysis_work(bytes.saturating_add(count as u64))?;
        self.environment
            .budget
            .reserve_temporary_bytes(bytes.saturating_add((count as u64).saturating_mul(24)))?;
        Ok(fields(self, symbol, count, span)?
            .iter()
            .map(|field| field.name.clone())
            .collect())
    }
    fn data_variant_names(
        &mut self,
        symbol: &CtSymbolId,
        tag: u32,
        count: usize,
        span: Span,
    ) -> CompileResult<(String, Option<Vec<String>>)> {
        fn variant<'a>(
            this: &'a Materializer<'_, '_>,
            symbol: &CtSymbolId,
            tag: u32,
            span: Span,
        ) -> CompileResult<&'a s::Variant> {
            let (_, parsed) = this.parsed_source(symbol.module)?;
            let item = parsed
                .module
                .items
                .get(symbol.declaration.0 as usize)
                .ok_or_else(|| runtime("returned enum declaration is missing", span))?;
            let s::ItemKind::Enum(declaration) = &item.kind else {
                return Err(runtime(
                    "returned enum is not a source enum declaration",
                    span,
                ));
            };
            declaration
                .variants
                .get(tag as usize)
                .ok_or_else(|| runtime("returned enum variant is missing", span))
        }
        let value = variant(self, symbol, tag, span)?;
        let (arity, bytes) = match &value.fields {
            s::VariantFields::Unit => (0, 0),
            s::VariantFields::Tuple(fields) => (fields.len(), 0),
            s::VariantFields::Record(fields) => (
                fields.len(),
                fields.iter().map(|f| f.name.len() as u64).sum::<u64>(),
            ),
        };
        if arity != count {
            return Err(runtime("returned enum field count differs", span));
        }
        let bytes = bytes.saturating_add(value.name.len() as u64);
        self.environment
            .budget
            .reserve_analysis_work(bytes.saturating_add(count as u64))?;
        self.environment
            .budget
            .reserve_temporary_bytes(bytes.saturating_add((count as u64).saturating_mul(24)))?;
        let value = variant(self, symbol, tag, span)?;
        Ok((
            value.name.clone(),
            match &value.fields {
                s::VariantFields::Record(fields) => {
                    Some(fields.iter().map(|f| f.name.clone()).collect())
                }
                _ => None,
            },
        ))
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
        let source_ty = self.data_source_type(context, ty, module, span, depth + 1)?;
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
                CtValue::Variant {
                    schema,
                    tag,
                    fields,
                },
                CtTy::Enum(id),
            ) if schema == id => {
                let shape = context
                    .enums
                    .get(id.0 as usize)
                    .ok_or_else(|| runtime("returned enum schema is missing", span))?;
                let types = shape
                    .variants
                    .get(*tag as usize)
                    .ok_or_else(|| runtime("returned enum tag is missing", span))?;
                if fields.len() != types.len() {
                    return Err(runtime("returned enum payload count differs", span));
                }
                let (name, names) =
                    self.data_variant_names(&shape.name, *tag, fields.len(), span)?;
                let owner = self.data_type_expr(&source_ty, span)?;
                let callee = self.data_node(
                    s::ExprKind::Member {
                        object: Box::new(owner),
                        member: s::MemberName::Name(name),
                    },
                    span,
                )?;
                if let Some(names) = names {
                    let mut output = Vec::new();
                    for ((field, ty), name) in fields.iter().zip(types).zip(names) {
                        let value =
                            self.data_expression(module, field, ty, context, span, depth + 1)?;
                        output.push(s::RecordField {
                            name,
                            value: Some(value),
                            span,
                        });
                    }
                    s::ExprKind::Record {
                        ty: Some(Box::new(callee)),
                        base: None,
                        fields: output,
                    }
                } else if fields.is_empty() {
                    callee.kind
                } else {
                    let mut arguments = Vec::new();
                    for (field, ty) in fields.iter().zip(types) {
                        let value =
                            self.data_expression(module, field, ty, context, span, depth + 1)?;
                        arguments.push(s::Argument {
                            value,
                            spread: false,
                            span,
                        });
                    }
                    s::ExprKind::Call {
                        callee: Box::new(callee),
                        arguments,
                    }
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
                for member in members.iter().chain(expected) {
                    crate::ct_artifact::execute::value_context::ty_cost(
                        member,
                        &mut self.environment.budget,
                    )?;
                }
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
