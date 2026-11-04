use super::*;
use cfg::{Builder, Value};
pub(super) fn scalar_constant(c: &Constant) -> CtTy {
    CtTy::Scalar(match c {
        Constant::Unit => CtScalarTy::Unit,
        Constant::Bool(_) => CtScalarTy::Bool,
        Constant::U8(_) => CtScalarTy::U8,
        Constant::U16(_) => CtScalarTy::U16,
        Constant::I32(_) => CtScalarTy::I32,
        Constant::I64(_) => CtScalarTy::I64,
        Constant::F32(_) => CtScalarTy::F32,
        Constant::F64(_) => CtScalarTy::F64,
        Constant::U64(_) => CtScalarTy::U64,
        Constant::Usize(_) => CtScalarTy::Usize,
        _ => unreachable!(),
    })
}
fn runtime_scalar(ty: Option<&CtTy>) -> Option<zil_ir::Type> {
    Some(match ty? {
        CtTy::Scalar(CtScalarTy::U8) => zil_ir::Type::U8,
        CtTy::Scalar(CtScalarTy::U16) => zil_ir::Type::U16,
        CtTy::Scalar(CtScalarTy::I32) => zil_ir::Type::I32,
        CtTy::Scalar(CtScalarTy::I64) => zil_ir::Type::I64,
        CtTy::Scalar(CtScalarTy::F32) => zil_ir::Type::F32,
        CtTy::Scalar(CtScalarTy::F64) => zil_ir::Type::F64,
        CtTy::Scalar(CtScalarTy::U64) => zil_ir::Type::U64,
        CtTy::Scalar(CtScalarTy::Usize) => zil_ir::Type::Usize,
        _ => return None,
    })
}
fn binary(op: s::BinaryOp) -> Option<BinaryOp> {
    Some(match op {
        s::BinaryOp::Add => BinaryOp::Add,
        s::BinaryOp::Subtract => BinaryOp::Subtract,
        s::BinaryOp::Multiply => BinaryOp::Multiply,
        s::BinaryOp::Divide => BinaryOp::Divide,
        s::BinaryOp::Remainder => BinaryOp::Remainder,
        s::BinaryOp::AddWrapping => BinaryOp::AddWrapping,
        s::BinaryOp::SubtractWrapping => BinaryOp::SubtractWrapping,
        s::BinaryOp::MultiplyWrapping => BinaryOp::MultiplyWrapping,
        s::BinaryOp::ShiftLeft => BinaryOp::ShiftLeft,
        s::BinaryOp::ShiftRight => BinaryOp::ShiftRight,
        s::BinaryOp::BitAnd => BinaryOp::BitAnd,
        s::BinaryOp::BitOr => BinaryOp::BitOr,
        s::BinaryOp::BitXor => BinaryOp::BitXor,
        s::BinaryOp::Equal => BinaryOp::Equal,
        s::BinaryOp::NotEqual => BinaryOp::NotEqual,
        s::BinaryOp::Less => BinaryOp::Less,
        s::BinaryOp::LessEqual => BinaryOp::LessEqual,
        s::BinaryOp::Greater => BinaryOp::Greater,
        s::BinaryOp::GreaterEqual => BinaryOp::GreaterEqual,
        _ => return None,
    })
}
fn text(spelling: &str, span: Span) -> R<String> {
    let Some(s) = spelling.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return Err(error(span, "malformed CT Text literal"));
    };
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next() {
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('0') => '\0',
            Some('"') => '"',
            Some('\\') => '\\',
            _ => return Err(error(span, "unsupported Text literal escape")),
        });
    }
    Ok(out)
}
impl Builder<'_, '_> {
    fn source_path(&mut self, expression: &s::Expr, depth: usize) -> R<Option<Vec<String>>> {
        self.compiler.budget.depth(depth, expression.span)?;
        match &expression.kind {
            s::ExprKind::Name(name) => {
                self.compiler.budget.text(name, expression.span)?;
                self.compiler.budget.node(expression.span)?;
                Ok(Some(vec![name.clone()]))
            }
            s::ExprKind::Member {
                object,
                member: s::MemberName::Name(name),
            } => {
                let Some(mut path) = self.source_path(object, depth + 1)? else {
                    return Ok(None);
                };
                self.compiler.budget.text(name, expression.span)?;
                self.compiler.budget.node(expression.span)?;
                path.push(name.clone());
                Ok(Some(path))
            }
            s::ExprKind::Group(inner) => self.source_path(inner, depth + 1),
            _ => Ok(None),
        }
    }

    pub(super) fn expr(&mut self, e: &s::Expr, expected: Option<&CtTy>, depth: usize) -> R<Value> {
        if let Some(CtTy::AnonymousSum(members)) = expected
            && !matches!(e.kind, s::ExprKind::If { .. } | s::ExprKind::MetaBlock(_))
        {
            let value = self.expr_raw(e, None, depth)?;
            self.compiler.budget.ty(&value.ty, 0, e.span)?;
            let expected = expected.expect("sum context");
            self.compiler.budget.ty(expected, 0, e.span)?;
            if &value.ty == expected {
                return Ok(value);
            }
            let mut active = None;
            for (index, member) in members.iter().enumerate() {
                self.compiler.budget.ty(member, 0, e.span)?;
                self.compiler.budget.ty(&value.ty, 0, e.span)?;
                if member == &value.ty && active.replace(index as u32).is_some() {
                    return Err(error(e.span, "ambiguous CT sum injection"));
                }
            }
            let active = active
                .ok_or_else(|| error(e.span, "CT sum injection requires an exact member type"))?;
            for member in members {
                self.compiler.budget.copy_ty(member, 0, e.span)?;
            }
            self.compiler.budget.copy_ty(expected, 0, e.span)?;
            return self.emit(
                CtInstructionKind::MakeAnonymousSum {
                    members: members.clone(),
                    active,
                    payload: value.id,
                },
                expected.clone(),
                e.span,
            );
        }
        self.expr_raw(e, expected, depth)
    }
    fn expr_raw(&mut self, e: &s::Expr, expected: Option<&CtTy>, depth: usize) -> R<Value> {
        self.compiler.budget.depth(depth, e.span)?;
        if let Some(ty) = expected {
            self.compiler.budget.ty(ty, 0, e.span)?;
        }
        let span = e.span;
        let value = match &e.kind {
            s::ExprKind::Ascribed { ty, value } => {
                let ty = self.compiler.ty(self.module, ty, depth + 1)?;
                self.expr(value, Some(&ty), depth + 1)?
            }
            s::ExprKind::Literal(s::Literal::Bool(v)) => self.constant(
                CtConstant::Scalar(Constant::Bool(*v)),
                CtTy::Scalar(CtScalarTy::Bool),
                span,
            )?,
            s::ExprKind::Literal(s::Literal::Integer(spelling)) => {
                self.compiler.budget.text(spelling, span)?;
                let c = zil_types::parse_integer_literal(
                    spelling,
                    runtime_scalar(expected).as_ref(),
                    false,
                    span,
                )?;
                let ty = scalar_constant(&c);
                self.constant(CtConstant::Scalar(c), ty, span)?
            }
            s::ExprKind::Literal(s::Literal::Float(spelling)) => {
                let work = if spelling.starts_with("@bits:") {
                    spelling.len()
                } else {
                    zil_base::numeric::decimal_work(spelling.len())
                };
                self.compiler.budget.work(work, span)?;
                let c = zil_types::parse_float_literal(
                    spelling,
                    runtime_scalar(expected).as_ref(),
                    span,
                )?;
                let ty = scalar_constant(&c);
                self.constant(CtConstant::Scalar(c), ty, span)?
            }
            s::ExprKind::Literal(s::Literal::String(spelling)) => {
                self.compiler.budget.text(spelling, span)?;
                self.constant(CtConstant::Text(text(spelling, span)?), CtTy::Text, span)?
            }
            s::ExprKind::Name(name) => {
                if let Some(value) = self.lookup(name, span)? {
                    value
                } else if let Some(graph) = self.compiler.metadata_binding(name, span)? {
                    self.metadata_type_value(&graph, graph.root, depth + 1, span)?
                } else if let Some(ty) = self.compiler.binding(name, span)? {
                    self.bound_type_value(&ty, span, depth + 1)?
                } else if let Some(t) = types::scalar(name) {
                    self.emit(CtInstructionKind::TypePrimitive(t), CtTy::Type, span)?
                } else if let Some(t) = match name.as_str() {
                    "ByteBuffer" => Some(CtBuiltinType::ByteBuffer),
                    "ForeignHandle" => Some(CtBuiltinType::ForeignHandle),
                    "CWideString" => Some(CtBuiltinType::CWideString),
                    _ => None,
                } {
                    self.emit(CtInstructionKind::TypeBuiltin(t), CtTy::Type, span)?
                } else {
                    self.type_name(std::slice::from_ref(name), span, depth)?
                }
            }
            s::ExprKind::Group(inner) | s::ExprKind::MetaEval(inner) => {
                self.expr(inner, expected, depth + 1)?
            }
            s::ExprKind::Tuple(fields) => {
                if fields.is_empty() {
                    self.constant(
                        CtConstant::Scalar(Constant::Unit),
                        CtTy::Scalar(CtScalarTy::Unit),
                        span,
                    )?
                } else {
                    let mut values = Vec::new();
                    let mut tys = Vec::new();
                    for (i, field) in fields.iter().enumerate() {
                        self.compiler.budget.node(field.span)?;
                        let target = match expected {
                            Some(CtTy::Tuple(ts)) => ts.get(i),
                            _ => None,
                        };
                        let value = self.expr(field, target, depth + 1)?;
                        values.push(value.id);
                        tys.push(value.ty);
                    }
                    self.emit(CtInstructionKind::MakeTuple(values), CtTy::Tuple(tys), span)?
                }
            }
            s::ExprKind::Array(fields) => {
                let target = match expected {
                    Some(CtTy::Array { element, .. }) | Some(CtTy::List(element)) => {
                        Some(element.as_ref())
                    }
                    _ => None,
                };
                let mut values = Vec::new();
                let mut element = target.cloned();
                for field in fields {
                    self.compiler.budget.node(field.span)?;
                    let value = self.expr(field, element.as_ref(), depth + 1)?;
                    if element.is_none() {
                        element = Some(value.ty.clone());
                    }
                    values.push(value.id);
                }
                let element = element.ok_or_else(|| {
                    error(span, "empty CT array/list needs an explicit element type")
                })?;
                if matches!(expected, Some(CtTy::List(_))) {
                    self.emit(
                        CtInstructionKind::MakeList(values),
                        CtTy::List(Box::new(element)),
                        span,
                    )?
                } else {
                    self.emit(
                        CtInstructionKind::MakeArray(values),
                        CtTy::Array {
                            element: Box::new(element),
                            length: fields.len() as u32,
                        },
                        span,
                    )?
                }
            }
            s::ExprKind::Unary {
                op: s::UnaryOp::Negate,
                operand,
            } if matches!(operand.kind, s::ExprKind::Literal(s::Literal::Integer(_))) => {
                let s::ExprKind::Literal(s::Literal::Integer(spelling)) = &operand.kind else {
                    unreachable!()
                };
                self.compiler.budget.text(spelling, span)?;
                let c = zil_types::parse_integer_literal(
                    spelling,
                    runtime_scalar(expected).as_ref(),
                    true,
                    span,
                )?;
                let ty = scalar_constant(&c);
                self.constant(CtConstant::Scalar(c), ty, span)?
            }
            s::ExprKind::Unary { op, operand } => {
                let op = match op {
                    s::UnaryOp::Not => UnaryOp::Not,
                    s::UnaryOp::Negate => UnaryOp::Negate,
                    s::UnaryOp::BitNot => UnaryOp::BitNot,
                    _ => {
                        return Err(error(
                            span,
                            "CT values have no runtime borrowing, dereference or resource transfer",
                        ));
                    }
                };
                let value = self.expr(operand, expected, depth + 1)?;
                if self.compiler.is_symbolic(&value.ty) {
                    return Err(error(span, "Copy bound does not grant unary operations"));
                }
                self.emit(
                    CtInstructionKind::Unary {
                        op,
                        operand: value.id,
                    },
                    value.ty,
                    span,
                )?
            }
            s::ExprKind::Binary { op, left, right }
                if matches!(op, s::BinaryOp::And | s::BinaryOp::Or) =>
            {
                let constant = s::Expr {
                    kind: s::ExprKind::Literal(s::Literal::Bool(*op == s::BinaryOp::Or)),
                    span,
                };
                if *op == s::BinaryOp::And {
                    self.expr_if(
                        left,
                        right,
                        &constant,
                        Some(&CtTy::Scalar(CtScalarTy::Bool)),
                        depth + 1,
                        span,
                    )?
                } else {
                    self.expr_if(
                        left,
                        &constant,
                        right,
                        Some(&CtTy::Scalar(CtScalarTy::Bool)),
                        depth + 1,
                        span,
                    )?
                }
            }
            s::ExprKind::Binary { op, left, right } => {
                let op = binary(*op).ok_or_else(|| {
                    error(span, "range is only valid as an ordinary for-loop iterable")
                })?;
                let compare = matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                );
                let lhs_expected = if compare { None } else { expected };
                let hint = if lhs_expected.is_none() {
                    match self.operand_hint(left, depth + 1)? {
                        Some(ty) => Some(ty),
                        None => self.operand_hint(right, depth + 1)?,
                    }
                } else {
                    None
                };
                let lhs = self.expr(left, lhs_expected.or(hint.as_ref()), depth + 1)?;
                let rhs = self.expr(right, Some(&lhs.ty), depth + 1)?;
                let integer = matches!(
                    lhs.ty,
                    CtTy::Scalar(
                        CtScalarTy::U8
                            | CtScalarTy::U16
                            | CtScalarTy::U64
                            | CtScalarTy::Usize
                            | CtScalarTy::I32
                            | CtScalarTy::I64
                    )
                );
                let equality = matches!(op, BinaryOp::Equal | BinaryOp::NotEqual);
                let floating = matches!(lhs.ty, CtTy::Scalar(CtScalarTy::F32 | CtScalarTy::F64));
                let float_operator = matches!(
                    op,
                    BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide
                ) || compare;
                if !integer
                    && !(floating && float_operator)
                    && !(equality
                        && matches!(
                            lhs.ty,
                            CtTy::Scalar(CtScalarTy::Bool) | CtTy::Type | CtTy::Ident | CtTy::Text
                        ))
                {
                    return Err(error(
                        span,
                        "CT operator is not defined for these actual data operands",
                    ));
                }
                let result = if compare {
                    CtTy::Scalar(CtScalarTy::Bool)
                } else {
                    lhs.ty
                };
                self.emit(
                    CtInstructionKind::Binary {
                        op,
                        left: lhs.id,
                        right: rhs.id,
                    },
                    result,
                    span,
                )?
            }
            s::ExprKind::Call { callee, arguments } => {
                if matches!(&callee.kind, s::ExprKind::Name(name) if name == "thread_scope") {
                    return Err(vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-FORBIDDEN",
                        Some(e.span),
                        "structured threads cannot execute in CT",
                    )]);
                }
                self.call(callee, arguments, expected, depth + 1, span)?
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => self.expr_if(condition, then_value, else_value, expected, depth + 1, span)?,
            s::ExprKind::Index { object, index } => {
                let object = self.expr(object, None, depth + 1)?;
                let element = match &object.ty {
                    CtTy::Array { element, .. } | CtTy::List(element) => element.as_ref().clone(),
                    _ => return Err(error(span, "CT dynamic indexing needs an Array or List")),
                };
                let index = self.expr(index, None, depth + 1)?;
                self.emit(
                    CtInstructionKind::Extract {
                        aggregate: object.id,
                        index: index.id,
                    },
                    element,
                    span,
                )?
            }
            s::ExprKind::Member { object, member } => {
                if let Some(path) = self.source_path(e, depth + 1)?
                    && path.len() >= 2
                    && !self.has_local(&path[0], span)?
                {
                    if let Some(value) = self.enum_value(&path, &[], depth + 1, span)? {
                        value
                    } else {
                        self.type_name(&path, span, depth + 1)?
                    }
                } else {
                    let value = self.expr(object, None, depth + 1)?;
                    self.field(value, member, span)?
                }
            }
            s::ExprKind::Record { ty, base, fields } => {
                if base.is_some() {
                    return Err(error(
                        span,
                        "CT record update requires explicit pure field replacement",
                    ));
                }
                let target = if let Some(t) = ty {
                    let (nominal, arguments) = match &t.kind {
                        s::ExprKind::TypeApply { callee, arguments } => {
                            for arg in arguments {
                                match arg {
                                    s::GenericArg::Lifetime(name) => {
                                        self.compiler.budget.text(name, t.span)?
                                    }
                                    s::GenericArg::Type(actual) => {
                                        self.compiler.source_type_cost(actual, depth + 1)?
                                    }
                                    s::GenericArg::Const(value) => {
                                        self.compiler.source_expression_copy(value, depth + 1)?
                                    }
                                }
                            }
                            (callee.as_ref(), arguments.clone())
                        }
                        _ => (t.as_ref(), Vec::new()),
                    };
                    let path = self.source_path(nominal, depth + 1)?.ok_or_else(|| {
                        error(t.span, "CT record constructor needs a nominal declaration")
                    })?;
                    if path.len() == 1 && self.compiler.binding(&path[0], t.span)?.is_some() {
                        return Err(error(
                            t.span,
                            "Copy bound does not grant a record constructor",
                        ));
                    }
                    let t = s::TypeRef {
                        kind: s::TypeKind::Named {
                            path,
                            args: arguments,
                        },
                        span: t.span,
                    };
                    self.compiler.ty(self.module, &t, depth + 1)?
                } else {
                    expected.cloned().ok_or_else(|| {
                        error(span, "CT record needs its actual expected nominal type")
                    })?
                };
                if self.compiler.is_symbolic(&target) {
                    return Err(error(
                        span,
                        "Copy bound does not grant a contextual record constructor",
                    ));
                }
                let CtTy::Record(id) = target else {
                    return Err(error(span, "CT record constructor requires a pure struct"));
                };
                let (owner, decl) = self.compiler.record_sources[id.0 as usize];
                for ty in &self.compiler.records[id.0 as usize].fields {
                    self.compiler.budget.ty(ty, 0, span)?;
                }
                let types = self.compiler.records[id.0 as usize].fields.clone();
                let mut values = Vec::new();
                if fields.len() != decl.fields.len() {
                    return Err(error(
                        span,
                        "CT record requires every actual field exactly once",
                    ));
                }
                for (field, ty) in decl.fields.iter().zip(types) {
                    if owner != self.module && !field.public {
                        return Err(error(
                            span,
                            "CT record constructor cannot initialize private foreign fields",
                        ));
                    }
                    self.compiler.budget.work(fields.len(), span)?;
                    let mut found = fields.iter().filter(|f| f.name == field.name);
                    let actual = found
                        .next()
                        .ok_or_else(|| error(span, "CT record field is missing"))?;
                    if found.next().is_some() {
                        return Err(error(actual.span, "duplicate CT record field"));
                    }
                    let shorthand = s::Expr {
                        kind: s::ExprKind::Name(actual.name.clone()),
                        span: actual.span,
                    };
                    let value = self.expr(
                        actual.value.as_ref().unwrap_or(&shorthand),
                        Some(&ty),
                        depth + 1,
                    )?;
                    self.compiler.budget.node(span)?;
                    values.push(value.id);
                }
                self.emit(
                    CtInstructionKind::MakeRecord {
                        schema: id,
                        fields: values,
                    },
                    CtTy::Record(id),
                    span,
                )?
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                let names = self.source_path(callee, depth + 1)?.ok_or_else(|| {
                    error(span, "Type application needs an actual nominal declaration")
                })?;
                if self.has_local(&names[0], span)? {
                    return Err(error(
                        span,
                        "value binding shadows a nominal Type application",
                    ));
                }
                if names.len() == 1 && self.compiler.binding(&names[0], span)?.is_some() {
                    return Err(error(
                        span,
                        "Copy type binder is not a generic nominal constructor",
                    ));
                }
                let (owner, item) = self.compiler.named_item(self.module, &names, span)?;
                let nominal = self.compiler.symbol(owner, item.span)?;
                let mut ids = Vec::new();
                for argument in arguments {
                    self.compiler.budget.node(span)?;
                    let value = match argument {
                        s::GenericArg::Lifetime(_) => {
                            return Err(error(
                                span,
                                "nominal lifetime CT argument requires a closed numeric scope",
                            ));
                        }
                        s::GenericArg::Type(ty) => self.type_ref(self.module, ty, depth + 1)?,
                        s::GenericArg::Const(e) => self.expr(e, None, depth + 1)?,
                    };
                    ids.push(value.id);
                }
                self.emit(
                    CtInstructionKind::TypeApply {
                        nominal,
                        arguments: ids,
                    },
                    CtTy::Type,
                    span,
                )?
            }
            s::ExprKind::MetaBlock(body) => self.meta_block(body, expected, depth + 1)?,
            s::ExprKind::Quote { category, template } => {
                self.quote(*category, template, depth + 1, span)?
            }
            _ => {
                return Err(error(
                    span,
                    "expression is not a closed pure CT data operation",
                ));
            }
        };
        self.expect(value, expected, span)
    }
    fn enum_value(
        &mut self,
        path: &[String],
        arguments: &[s::Argument],
        depth: usize,
        span: Span,
    ) -> R<Option<Value>> {
        if path.len() < 2 {
            return Ok(None);
        }
        let (_, item) = match self
            .compiler
            .named_item(self.module, &path[..path.len() - 1], span)
        {
            Ok(item) => item,
            Err(errors)
                if errors
                    .iter()
                    .any(|d| d.category == zil_base::ResultCategory::ResourceLimit) =>
            {
                return Err(errors);
            }
            Err(_) => return Ok(None),
        };
        if !matches!(item.kind, s::ItemKind::Enum(_) | s::ItemKind::TypeAlias(_)) {
            return Ok(None);
        }
        for part in &path[..path.len() - 1] {
            self.compiler.budget.text(part, span)?;
        }
        let ty = s::TypeRef {
            kind: s::TypeKind::Named {
                path: path[..path.len() - 1].to_vec(),
                args: Vec::new(),
            },
            span,
        };
        let resolved = self.compiler.ty(self.module, &ty, depth + 1)?;
        let CtTy::Enum(schema) = resolved else {
            return Ok(None);
        };
        let (_, enumeration) = self.compiler.enum_sources[schema.0 as usize];
        let name = path.last().unwrap();
        let mut tag = None;
        for (index, variant) in enumeration.variants.iter().enumerate() {
            self.compiler
                .budget
                .work(name.len().saturating_add(1), span)?;
            if variant.name == *name {
                tag = Some(index);
                break;
            }
        }
        let tag = tag.ok_or_else(|| error(span, "CT enum constructor variant is absent"))?;
        for ty in &self.compiler.enums[schema.0 as usize].variants[tag] {
            self.compiler.budget.ty(ty, 0, span)?;
        }
        let expected = self.compiler.enums[schema.0 as usize].variants[tag].clone();
        if expected.len() != arguments.len() {
            return Err(error(span, "CT enum constructor field count differs"));
        }
        let mut fields = Vec::new();
        for (argument, ty) in arguments.iter().zip(expected) {
            self.compiler.budget.node(argument.span)?;
            if argument.spread {
                return Err(error(
                    argument.span,
                    "CT enum field cannot use an unexpanded spread",
                ));
            }
            fields.push(self.expr(&argument.value, Some(&ty), depth + 1)?.id);
        }
        self.emit(
            CtInstructionKind::MakeVariant {
                schema,
                tag: tag as u32,
                fields,
            },
            CtTy::Enum(schema),
            span,
        )
        .map(Some)
    }
    fn type_name(&mut self, path: &[String], span: Span, depth: usize) -> R<Value> {
        if path.len() == 1
            && let Some(graph) = self.compiler.metadata_binding(&path[0], span)?
        {
            return self.metadata_type_value(&graph, graph.root, depth + 1, span);
        }
        if path.len() == 1
            && let Some(ty) = self.compiler.binding(&path[0], span)?
        {
            return self.bound_type_value(&ty, span, depth + 1);
        }
        let (owner, item) = self.compiler.named_item(self.module, path, span)?;
        if let s::ItemKind::TypeAlias(alias) = &item.kind {
            let actual = alias
                .value
                .as_ref()
                .ok_or_else(|| error(span, "CT alias has no actual definition"))?;
            self.compiler.enter_alias(owner, item.span, span)?;
            let result = self.source_alias_type_ref(owner, actual, depth + 1);
            self.compiler.active_aliases.pop();
            return result;
        }
        let symbol = self.compiler.symbol(owner, item.span)?;
        self.emit(CtInstructionKind::TypeName(symbol), CtTy::Type, span)
    }
    pub(super) fn type_ref(&mut self, module: usize, ty: &s::TypeRef, depth: usize) -> R<Value> {
        self.compiler.budget.depth(depth, ty.span)?;
        match &ty.kind {
            s::TypeKind::Named { path, args } if args.is_empty() => {
                if path.len() == 1 {
                    if let Some(graph) = self.compiler.metadata_binding(&path[0], ty.span)? {
                        return self.metadata_type_value(&graph, graph.root, depth + 1, ty.span);
                    }
                    if let Some(actual) = self.compiler.binding(&path[0], ty.span)? {
                        return self.bound_type_value(&actual, ty.span, depth + 1);
                    }
                    if let Some(t) = types::scalar(&path[0]) {
                        return self.emit(CtInstructionKind::TypePrimitive(t), CtTy::Type, ty.span);
                    }
                    if let Some(t) = match path[0].as_str() {
                        "ByteBuffer" => Some(CtBuiltinType::ByteBuffer),
                        "ForeignHandle" => Some(CtBuiltinType::ForeignHandle),
                        "CWideString" => Some(CtBuiltinType::CWideString),
                        _ => None,
                    } {
                        return self.emit(CtInstructionKind::TypeBuiltin(t), CtTy::Type, ty.span);
                    }
                }
                let old = self.module;
                self.module = module;
                let result = self.type_name(path, ty.span, depth + 1);
                self.module = old;
                result
            }
            s::TypeKind::Named { path, args } => {
                if path.len() == 1 && self.compiler.binding(&path[0], ty.span)?.is_some() {
                    return Err(error(ty.span, "CT type binder takes no type arguments"));
                }
                let (owner, item) = self.compiler.named_item(module, path, ty.span)?;
                let nominal = self.compiler.symbol(owner, item.span)?;
                let mut arguments = Vec::new();
                for arg in args {
                    let value = match arg {
                        s::GenericArg::Lifetime(_) => {
                            return Err(error(
                                ty.span,
                                "nominal lifetime CT argument requires a closed numeric scope",
                            ));
                        }
                        s::GenericArg::Type(t) => self.type_ref(module, t, depth + 1)?,
                        s::GenericArg::Const(e) => self.expr(e, None, depth + 1)?,
                    };
                    self.compiler.budget.node(ty.span)?;
                    arguments.push(value.id);
                }
                self.emit(
                    CtInstructionKind::TypeApply { nominal, arguments },
                    CtTy::Type,
                    ty.span,
                )
            }
            s::TypeKind::Group(inner) => self.type_ref(module, inner, depth + 1),
            s::TypeKind::Tuple(fields) | s::TypeKind::Union(fields) => {
                let mut arguments = Vec::new();
                for field in fields {
                    let v = self.type_ref(module, field, depth + 1)?;
                    self.compiler.budget.node(ty.span)?;
                    arguments.push(v.id);
                }
                self.emit(
                    CtInstructionKind::TypeConstruct {
                        constructor: if matches!(ty.kind, s::TypeKind::Union(_)) {
                            CtTypeConstructor::AnonymousSum
                        } else {
                            CtTypeConstructor::Tuple
                        },
                        arguments,
                    },
                    CtTy::Type,
                    ty.span,
                )
            }
            s::TypeKind::Reference { mutable, inner, .. } => {
                let v = self.type_ref(module, inner, depth + 1)?;
                self.emit(
                    CtInstructionKind::TypeConstruct {
                        constructor: CtTypeConstructor::Address { mutable: *mutable },
                        arguments: vec![v.id],
                    },
                    CtTy::Type,
                    ty.span,
                )
            }
            s::TypeKind::Array { element, length } => {
                let v = self.type_ref(module, element, depth + 1)?;
                let (constructor, arguments) = if let Some(n) = length {
                    let n = self.expr(n, None, depth + 1)?;
                    (CtTypeConstructor::Array, vec![v.id, n.id])
                } else {
                    (CtTypeConstructor::Slice, vec![v.id])
                };
                self.emit(
                    CtInstructionKind::TypeConstruct {
                        constructor,
                        arguments,
                    },
                    CtTy::Type,
                    ty.span,
                )
            }
            s::TypeKind::ForLifetime { .. } => {
                let mut inner = ty;
                let mut names = Vec::new();
                while let s::TypeKind::ForLifetime {
                    lifetime,
                    inner: next,
                } = &inner.kind
                {
                    self.compiler.budget.node(inner.span)?;
                    self.compiler
                        .budget
                        .work(lifetime.len().saturating_add(1), ty.span)?;
                    names.push(lifetime.as_str());
                    inner = next;
                }
                let s::TypeKind::Function { params, result } = &inner.kind else {
                    return Err(error(
                        ty.span,
                        "higher-rank Type constructor requires a function signature",
                    ));
                };
                self.compiler.budget.work(
                    names.len().saturating_add(
                        params
                            .len()
                            .saturating_add(1)
                            .saturating_mul(names.len().saturating_add(1)),
                    ),
                    ty.span,
                )?;
                let life = super::source_relation(&names, params, result);
                let mut arguments = Vec::new();
                for parameter in params {
                    arguments.push(self.type_ref(module, parameter, depth + 1)?.id);
                }
                arguments.push(self.type_ref(module, result, depth + 1)?.id);
                let constructor = if life.binder_count == 0 {
                    CtTypeConstructor::FunctionPointer
                } else {
                    CtTypeConstructor::BorrowedFunctionPointer { life }
                };
                self.emit(
                    CtInstructionKind::TypeConstruct {
                        constructor,
                        arguments,
                    },
                    CtTy::Type,
                    ty.span,
                )
            }
            s::TypeKind::Function { params, result } => {
                let mut arguments = Vec::new();
                for t in params {
                    let v = self.type_ref(module, t, depth + 1)?;
                    self.compiler.budget.node(ty.span)?;
                    arguments.push(v.id);
                }
                let v = self.type_ref(module, result, depth + 1)?;
                arguments.push(v.id);
                self.emit(
                    CtInstructionKind::TypeConstruct {
                        constructor: CtTypeConstructor::FunctionPointer,
                        arguments,
                    },
                    CtTy::Type,
                    ty.span,
                )
            }
            s::TypeKind::Error { error, result } => {
                let a = self.type_ref(module, error, depth + 1)?;
                let b = self.type_ref(module, result, depth + 1)?;
                self.emit(
                    CtInstructionKind::TypeConstruct {
                        constructor: CtTypeConstructor::ErrorUnion,
                        arguments: vec![a.id, b.id],
                    },
                    CtTy::Type,
                    ty.span,
                )
            }
            s::TypeKind::MetaEval(e) => self.expr(e, Some(&CtTy::Type), depth + 1),
            s::TypeKind::MetaBlock(body) => self.meta_block(body, Some(&CtTy::Type), depth + 1),
            _ => Err(error(ty.span, "unsupported Type data constructor")),
        }
    }
    fn field(&mut self, value: Value, field: &s::MemberName, span: Span) -> R<Value> {
        if let s::MemberName::Name(name) = field {
            let projection = match (&value.ty, name.as_str()) {
                (CtTy::ShapeInfo, "fields") => Some((
                    CtMetaProjection::ShapeFields,
                    CtTy::List(Box::new(CtTy::FieldInfo)),
                )),
                (CtTy::ShapeInfo, "complete") => Some((
                    CtMetaProjection::ShapeComplete,
                    CtTy::Scalar(CtScalarTy::Bool),
                )),
                (CtTy::FieldInfo, "name") => Some((CtMetaProjection::FieldName, CtTy::Ident)),
                (CtTy::FieldInfo, "ty") => Some((CtMetaProjection::FieldType, CtTy::Type)),
                (CtTy::FieldInfo, "managed") => Some((
                    CtMetaProjection::FieldManaged,
                    CtTy::Scalar(CtScalarTy::Bool),
                )),
                (CtTy::ImplInfo, "for_type") => Some((CtMetaProjection::ImplType, CtTy::Type)),
                (CtTy::LayoutInfo, "size") => {
                    Some((CtMetaProjection::LayoutSize, CtTy::Scalar(CtScalarTy::I64)))
                }
                (CtTy::LayoutInfo, "alignment") => Some((
                    CtMetaProjection::LayoutAlignment,
                    CtTy::Scalar(CtScalarTy::I64),
                )),
                (CtTy::LayoutInfo, "field_offsets") => Some((
                    CtMetaProjection::LayoutFieldOffsets,
                    CtTy::List(Box::new(CtTy::Scalar(CtScalarTy::I64))),
                )),
                _ => None,
            };
            if let Some((projection, ty)) = projection {
                return self.emit(
                    CtInstructionKind::MetaProject {
                        value: value.id,
                        projection,
                    },
                    ty,
                    span,
                );
            }
        }
        let (index, ty) = match (&value.ty, field) {
            (CtTy::Tuple(fields), s::MemberName::TupleIndex(index)) => (
                *index,
                fields
                    .get(*index as usize)
                    .cloned()
                    .ok_or_else(|| error(span, "CT tuple field is absent"))?,
            ),
            (CtTy::Record(id), s::MemberName::Name(name))
                if !self.compiler.is_symbolic(&value.ty) =>
            {
                let (owner, decl) = self.compiler.record_sources[id.0 as usize];
                let mut found = None;
                for (index, field) in decl.fields.iter().enumerate() {
                    self.compiler.budget.work(1, span)?;
                    if field.name == *name {
                        found = Some(index);
                        break;
                    }
                }
                let i = found.ok_or_else(|| error(span, "CT struct field is absent"))?;
                if owner != self.module && !decl.fields[i].public {
                    return Err(error(
                        span,
                        "CT struct field is private to its actual declaring module",
                    ));
                }
                (
                    i as u32,
                    self.compiler.records[id.0 as usize].fields[i].clone(),
                )
            }
            _ => {
                return Err(error(
                    span,
                    "CT field projection requires an actual pure struct or tuple",
                ));
            }
        };
        self.emit(
            CtInstructionKind::ExtractField {
                aggregate: value.id,
                index,
            },
            ty,
            span,
        )
    }
    fn call(
        &mut self,
        callee: &s::Expr,
        arguments: &[s::Argument],
        expected: Option<&CtTy>,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        let (callee, explicit) = match &callee.kind {
            s::ExprKind::TypeApply { callee, arguments } => (callee.as_ref(), Some(arguments)),
            _ => (callee, None),
        };
        let Some(path) = self.source_path(callee, depth + 1)? else {
            return Err(error(
                span,
                "CT call requires a concrete ordinary function or imported standard operation",
            ));
        };
        if self.has_local(&path[0], span)? {
            return Err(error(
                span,
                "CT data binding shadows the function/module name and is not callable",
            ));
        }
        for argument in arguments {
            self.compiler.budget.work(1, argument.span)?;
            if argument.spread {
                return Err(error(
                    argument.span,
                    "CT function call cannot use unexpanded spread arguments",
                ));
            }
        }
        if path.len() == 2 {
            for import in self.compiler.modules[self.module].imports {
                self.compiler.budget.work(1, span)?;
                if import.alias == path[0]
                    && import.target
                        != ImportTarget::Module(self.compiler.modules[self.module].syntax.id)
                    && matches!(import.target, ImportTarget::Meta | ImportTarget::Syntax)
                {
                    if explicit.is_some() {
                        return Err(error(
                            span,
                            "CT standard operation takes no explicit type arguments",
                        ));
                    }
                    return self.builtin(import.target, &path[1], arguments, depth, span);
                }
            }
        }
        if path.len() == 1
            && let Some(target) = types::scalar(&path[0])
            && !matches!(target, CtScalarTy::Unit | CtScalarTy::Bool)
        {
            if explicit.is_some() {
                return Err(error(span, "CT scalar conversion takes no type arguments"));
            }
            if arguments.len() != 1 {
                return Err(error(span, "integer conversion takes exactly one argument"));
            }
            let value = self.expr(&arguments[0].value, None, depth)?;
            if !matches!(
                value.ty,
                CtTy::Scalar(
                    CtScalarTy::U8
                        | CtScalarTy::U16
                        | CtScalarTy::U64
                        | CtScalarTy::Usize
                        | CtScalarTy::I32
                        | CtScalarTy::I64
                        | CtScalarTy::F32
                        | CtScalarTy::F64
                )
            ) {
                return Err(error(
                    span,
                    "integer conversion requires a concrete integer value",
                ));
            }
            let kind = if matches!(target, CtScalarTy::F32 | CtScalarTy::F64)
                || matches!(value.ty, CtTy::Scalar(CtScalarTy::F32 | CtScalarTy::F64))
            {
                CtInstructionKind::ConvertNumber {
                    value: value.id,
                    target,
                }
            } else {
                CtInstructionKind::ConvertInteger {
                    value: value.id,
                    target,
                }
            };
            return self.emit(kind, CtTy::Scalar(target), span);
        }
        if let Some(value) = self.enum_value(&path, arguments, depth, span)? {
            return Ok(value);
        }
        let declaration = self.compiler.function(self.module, &path, span)?;
        if !self.compiler.declarations[declaration]
            .function
            .generics
            .is_empty()
            || explicit.is_some()
        {
            return self.generic_call(
                declaration,
                explicit.map(Vec::as_slice),
                arguments,
                expected,
                depth + 1,
                span,
            );
        }
        let (types, result) = self.compiler.header(declaration)?;
        if types.len() != arguments.len() {
            return Err(error(span, "CT function actual argument count differs"));
        }
        let mut actual = Vec::new();
        for (arg, ty) in arguments.iter().zip(types) {
            self.compiler.budget.node(arg.span)?;
            actual.push(self.expr(&arg.value, Some(&ty), depth)?.id);
        }
        let function = self.compiler.enqueue(declaration)?;
        if result == CtTy::Scalar(CtScalarTy::Unit) {
            self.op(
                CtInstructionKind::Call {
                    function,
                    arguments: actual,
                },
                span,
            )?;
            self.constant(CtConstant::Scalar(Constant::Unit), result, span)
        } else {
            self.emit(
                CtInstructionKind::Call {
                    function,
                    arguments: actual,
                },
                result,
                span,
            )
        }
    }

    fn generic_call(
        &mut self,
        declaration: usize,
        explicit: Option<&[s::GenericArg]>,
        arguments: &[s::Argument],
        expected: Option<&CtTy>,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        if !self.compiler.checking_generics {
            return self.metadata_generic_call(
                declaration,
                explicit,
                arguments,
                expected,
                depth,
                span,
            );
        }
        let d = self.compiler.declarations[declaration];
        let names =
            self.compiler
                .generic_names(&d.function.generics, &d.function.where_clause, span)?;
        if names.is_empty() {
            return Err(error(
                span,
                "nongeneric CT function takes no type arguments",
            ));
        }
        if arguments.len() != d.function.params.len() {
            return Err(error(span, "CT function actual argument count differs"));
        }
        self.compiler.budget.work(names.len(), span)?;
        self.compiler.budget.node(span)?;
        let mut slots = vec![None; names.len()];
        if let Some(explicit) = explicit {
            if explicit.len() != names.len() {
                return Err(error(span, "CT generic type argument count differs"));
            }
            for (slot, argument) in slots.iter_mut().zip(explicit) {
                let s::GenericArg::Type(ty) = argument else {
                    return Err(error(span, "CT generic value arguments are unsupported"));
                };
                *slot = Some(self.compiler.ty(self.module, ty, depth + 1)?);
            }
        }
        if let (Some(pattern), Some(actual)) = (&d.function.return_type, expected) {
            self.compiler
                .infer(d.module, pattern, actual, &names, &mut slots, depth + 1)?;
        }
        let mut actual = Vec::new();
        let mut actual_types = Vec::new();
        for (i, argument) in arguments.iter().enumerate() {
            self.compiler.budget.node(argument.span)?;
            let hint = self.compiler.partial_type(
                d.module,
                &d.function.params[i].ty,
                &names,
                &slots,
                depth + 1,
            )?;
            let value = self.expr(&argument.value, hint.as_ref(), depth + 1)?;
            self.compiler.infer(
                d.module,
                &d.function.params[i].ty,
                &value.ty,
                &names,
                &mut slots,
                depth + 1,
            )?;
            actual.push(value.id);
            actual_types.push(value.ty);
        }
        let concrete = slots
            .into_iter()
            .map(|ty| ty.ok_or_else(|| error(span, "CT generic type argument cannot be inferred")))
            .collect::<R<Vec<_>>>()?;
        let (parameters, result) = self.compiler.instance_header(declaration, &concrete)?;
        for (actual, parameter) in actual_types.iter().zip(&parameters) {
            self.compiler.budget.ty(actual, 0, span)?;
            self.compiler.budget.ty(parameter, 0, span)?;
            if actual != parameter {
                return Err(error(span, "CT generic actual argument type differs"));
            }
        }
        let function = self.compiler.enqueue_instance(declaration, &concrete)?;
        if result == CtTy::Scalar(CtScalarTy::Unit) {
            self.op(
                CtInstructionKind::Call {
                    function,
                    arguments: actual,
                },
                span,
            )?;
            self.constant(CtConstant::Scalar(Constant::Unit), result, span)
        } else {
            self.emit(
                CtInstructionKind::Call {
                    function,
                    arguments: actual,
                },
                result,
                span,
            )
        }
    }

    fn bound_type_value(&mut self, ty: &CtTy, span: Span, depth: usize) -> R<Value> {
        self.compiler.budget.depth(depth, span)?;
        if !self.compiler.checking_generics
            && matches!(ty, CtTy::Record(_) | CtTy::Enum(_) | CtTy::AnonymousSum(_))
        {
            let graph = self.compiler.metadata_data(ty, span, depth + 1)?;
            return self.metadata_type_value(&graph, graph.root, depth + 1, span);
        }

        // Symbolic bodies are checked but never verified, published or run.
        if self.compiler.is_symbolic(ty) {
            return self.emit(
                CtInstructionKind::TypePrimitive(CtScalarTy::Unit),
                CtTy::Type,
                span,
            );
        }
        match ty {
            CtTy::Scalar(ty) => self.emit(CtInstructionKind::TypePrimitive(*ty), CtTy::Type, span),
            CtTy::Record(id) => {
                let (owner, declaration) = self.compiler.record_sources[id.0 as usize];
                self.compiler
                    .budget
                    .work(self.compiler.modules[owner].syntax.items.len(), span)?;
                let item = self.compiler.modules[owner].syntax.items.iter().find(|item| matches!(&item.kind,s::ItemKind::Aggregate(a) if std::ptr::eq(a,declaration)))
                    .ok_or_else(|| error(span,"CT generic record declaration is absent"))?;
                let symbol = self.compiler.symbol(owner, item.span)?;
                let args = &self.compiler.record_arguments[id.0 as usize];
                for ty in args {
                    self.compiler.budget.copy_ty(ty, 0, span)?;
                }
                let args = args.clone();
                if args.is_empty() {
                    self.emit(CtInstructionKind::TypeName(symbol), CtTy::Type, span)
                } else {
                    let mut arguments = Vec::new();
                    for arg in &args {
                        self.compiler.budget.node(span)?;
                        arguments.push(self.bound_type_value(arg, span, depth + 1)?.id);
                    }
                    self.emit(
                        CtInstructionKind::TypeApply {
                            nominal: symbol,
                            arguments,
                        },
                        CtTy::Type,
                        span,
                    )
                }
            }
            CtTy::Enum(id) => {
                let symbol = self.compiler.enums[id.0 as usize].name.clone();
                self.emit(CtInstructionKind::TypeName(symbol), CtTy::Type, span)
            }
            CtTy::Tuple(fields) => {
                let mut arguments = Vec::new();
                for field in fields {
                    self.compiler.budget.node(span)?;
                    arguments.push(self.bound_type_value(field, span, depth + 1)?.id);
                }
                self.emit(
                    CtInstructionKind::TypeConstruct {
                        constructor: CtTypeConstructor::Tuple,
                        arguments,
                    },
                    CtTy::Type,
                    span,
                )
            }
            CtTy::Array { element, length } => {
                let element = self.bound_type_value(element, span, depth + 1)?;
                let length = i32::try_from(*length)
                    .map_err(|_| error(span, "CT generic array length exceeds target i32"))?;
                let length = self.constant(
                    CtConstant::Scalar(Constant::I32(length)),
                    CtTy::Scalar(CtScalarTy::I32),
                    span,
                )?;
                self.emit(
                    CtInstructionKind::TypeConstruct {
                        constructor: CtTypeConstructor::Array,
                        arguments: vec![element.id, length.id],
                    },
                    CtTy::Type,
                    span,
                )
            }
            _ => Err(error(
                span,
                "CT generic bound does not describe an ordinary type",
            )),
        }
    }
}
