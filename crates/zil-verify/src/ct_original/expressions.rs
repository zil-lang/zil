use super::*;

impl Checker<'_, '_, '_> {
    pub(super) fn expression(
        &mut self,
        env: &mut Env,
        expr: &s::Expr,
        expected: Option<&Ty>,
        depth: usize,
    ) -> R<Value> {
        self.step(expr.span, depth)?;
        let span = expr.span;
        let value = match &expr.kind {
            s::ExprKind::Literal(literal) => self.literal(literal, expected, false, span)?,
            s::ExprKind::Name(name) => {
                self.name_value(env, std::slice::from_ref(name), depth + 1, span)?
            }
            s::ExprKind::ReceiverMember(name) => {
                let receiver = env
                    .value("self")
                    .ok_or_else(|| bad(span, "original CT receiver field has no self parameter"))?;
                let receiver = self.duplicate_type(&receiver.value.ty, span)?;
                Value::typed(self.field(env, &receiver, name, depth + 1, span)?)
            }
            s::ExprKind::Group(inner) | s::ExprKind::MetaEval(inner) => {
                self.expression(env, inner, expected, depth + 1)?
            }
            s::ExprKind::Ascribed { ty, value } => {
                let ty = self.ty(env, ty, false, depth + 1)?;
                self.expression(env, value, Some(&ty), depth + 1)?
            }
            s::ExprKind::Tuple(fields) => {
                let context = match expected {
                    Some(Ty::Tuple(fields)) => Some(fields),
                    _ => None,
                };
                if context.is_some_and(|types| types.len() != fields.len()) {
                    return Err(bad(span, "original CT tuple arity differs from its type"));
                }
                let mut types = self.vector(fields.len(), span)?;
                for (i, field) in fields.iter().enumerate() {
                    types.push(
                        self.expression(env, field, context.and_then(|ts| ts.get(i)), depth + 1)?
                            .ty,
                    );
                }
                Value::typed(if types.is_empty() {
                    unit()
                } else {
                    Ty::Tuple(types)
                })
            }
            s::ExprKind::Array(fields) => {
                let context = match expected {
                    Some(Ty::Array(inner, _) | Ty::List(inner)) => Some(inner.as_ref()),
                    _ => None,
                };
                let mut element = context
                    .map(|ty| self.duplicate_type(ty, span))
                    .transpose()?;
                for field in fields {
                    let value = self.expression(env, field, element.as_ref(), depth + 1)?;
                    if element.is_none() {
                        element = Some(value.ty);
                    }
                }
                let element = element
                    .ok_or_else(|| bad(span, "empty CT array has no original element type"))?;
                self.alloc(std::mem::size_of::<Ty>(), span)?;
                Value::typed(if matches!(expected, Some(Ty::List(_))) {
                    Ty::List(Box::new(element))
                } else {
                    Ty::Array(
                        Box::new(element),
                        Constant {
                            carrier: Scalar::Usize,
                            term: Term::Integer(fields.len() as i128),
                        },
                    )
                })
            }
            s::ExprKind::Unary {
                op: s::UnaryOp::Negate,
                operand,
            } if matches!(
                operand.kind,
                s::ExprKind::Literal(s::Literal::Integer(_) | s::Literal::Float(_))
            ) =>
            {
                let s::ExprKind::Literal(literal) = &operand.kind else {
                    unreachable!()
                };
                self.literal(literal, expected, true, span)?
            }
            s::ExprKind::Unary { op, operand } => {
                let mut value = self.expression(env, operand, expected, depth + 1)?;
                let valid = match op {
                    s::UnaryOp::Not => value.ty == bool_ty(),
                    s::UnaryOp::Negate => number(&value.ty),
                    s::UnaryOp::BitNot => integer(&value.ty),
                    _ => false,
                };
                if !valid {
                    return Err(bad(
                        span,
                        "original CT unary operation has no scalar capability in this Γ",
                    ));
                }
                if let Some(constant) = value.constant.take() {
                    let carrier = constant.carrier;
                    let term = match (&constant.term, op) {
                        (Term::Integer(n), s::UnaryOp::Negate) => Term::Integer(
                            n.checked_neg()
                                .ok_or_else(|| bad(span, "CT scalar negation overflows"))?,
                        ),
                        (Term::Integer(n), s::UnaryOp::BitNot) => Term::Integer(!n),
                        (Term::Bool(v), s::UnaryOp::Not) => Term::Bool(!v),
                        _ => {
                            self.alloc(std::mem::size_of::<Constant>(), span)?;
                            Term::Unary(*op, Box::new(constant))
                        }
                    };
                    value.constant = Some(Constant { carrier, term });
                }
                value
            }
            s::ExprKind::Binary { op, left, right } => {
                if *op == s::BinaryOp::Range {
                    return Err(bad(
                        span,
                        "CT range is valid only as an original for iterable",
                    ));
                }
                let comparison = matches!(
                    op,
                    s::BinaryOp::Equal
                        | s::BinaryOp::NotEqual
                        | s::BinaryOp::Less
                        | s::BinaryOp::LessEqual
                        | s::BinaryOp::Greater
                        | s::BinaryOp::GreaterEqual
                );
                let hint = if comparison { None } else { expected };
                let left_hint = self.operand_hint(env, left, depth + 1)?;
                let right_hint = self.operand_hint(env, right, depth + 1)?;
                let left = self.expression(
                    env,
                    left,
                    left_hint.as_ref().or(right_hint.as_ref()).or(hint),
                    depth + 1,
                )?;
                let right = self.expression(env, right, Some(&left.ty), depth + 1)?;
                self.same(&left.ty, &right.ty, span)?;
                let valid = match op {
                    s::BinaryOp::And | s::BinaryOp::Or => left.ty == bool_ty(),
                    s::BinaryOp::Equal | s::BinaryOp::NotEqual => {
                        number(&left.ty)
                            || matches!(
                                left.ty,
                                Ty::Scalar(Scalar::Bool)
                                    | Ty::Meta(Meta::Type | Meta::Ident | Meta::Text)
                            )
                    }
                    s::BinaryOp::Less
                    | s::BinaryOp::LessEqual
                    | s::BinaryOp::Greater
                    | s::BinaryOp::GreaterEqual => number(&left.ty),
                    s::BinaryOp::Add
                    | s::BinaryOp::Subtract
                    | s::BinaryOp::Multiply
                    | s::BinaryOp::Divide
                    | s::BinaryOp::Remainder => number(&left.ty),
                    _ => integer(&left.ty),
                };
                if !valid {
                    return Err(bad(
                        span,
                        "original CT binary operator is not justified by its own scalar carrier",
                    ));
                }
                let ty = if comparison { bool_ty() } else { left.ty };
                let mut result = Value::typed(ty);
                if let (Some(left), Some(right)) = (left.constant, right.constant) {
                    let Ty::Scalar(carrier) = result.ty else {
                        return Err(bad(span, "CT constant result has no scalar carrier"));
                    };
                    let term = if let Some(term) = self.constant_binary(*op, &left, &right, span)? {
                        term
                    } else {
                        self.alloc(std::mem::size_of::<Constant>().saturating_mul(2), span)?;
                        Term::Binary(*op, Box::new(left), Box::new(right))
                    };
                    result.constant = Some(Constant { carrier, term });
                }
                result
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(env, condition, Some(&bool_ty()), depth + 1)?;
                let mut yes_env = self.environment_copy(env, span)?;
                let yes = self.expression(&mut yes_env, then_value, expected, depth + 1)?;
                let mut no_env = self.environment_copy(env, span)?;
                let no = self.expression(&mut no_env, else_value, Some(&yes.ty), depth + 1)?;
                let mut result = Value::typed(yes.ty);
                if yes.represented == no.represented {
                    result.represented = yes.represented;
                }
                if yes.constant == no.constant {
                    result.constant = yes.constant;
                }
                result
            }
            s::ExprKind::Index { object, index } => {
                let object = self.expression(env, object, None, depth + 1)?;
                let index = self.expression(env, index, None, depth + 1)?;
                if !integer(&index.ty) {
                    return Err(bad(span, "original CT index has a noninteger carrier"));
                }
                let ty = match object.ty {
                    Ty::Array(inner, _) | Ty::List(inner) => *inner,
                    Ty::Tuple(fields) => {
                        let Some(Constant {
                            term: Term::Integer(index),
                            ..
                        }) = index.constant
                        else {
                            return Err(bad(
                                span,
                                "CT tuple extraction requires a constant ordinal",
                            ));
                        };
                        fields
                            .into_iter()
                            .nth(
                                usize::try_from(index)
                                    .map_err(|_| bad(span, "negative CT tuple index"))?,
                            )
                            .ok_or_else(|| bad(span, "CT tuple index is out of range"))?
                    }
                    _ => {
                        return Err(bad(
                            span,
                            "CT indexing requires an original Array, List, or Tuple",
                        ));
                    }
                };
                Value::typed(ty)
            }
            s::ExprKind::Member { object, member } => {
                if let Some(path) = self.path(expr, depth + 1)? {
                    let root = &path[0];
                    if env.value(root).is_none() && env.actual(root).is_none() {
                        if path.len() > 1 {
                            if let Some((owner, nominal)) =
                                self.enum_owner(env, &path[..path.len() - 1], depth + 1, span)?
                            {
                                let value = self.variant(
                                    env,
                                    owner,
                                    path.last().unwrap(),
                                    None,
                                    &[],
                                    nominal.as_ref().or(expected),
                                    depth + 1,
                                    span,
                                )?;
                                return self.finish_value(value, expected, span);
                            }
                        }
                        let value = self.name_value(env, &path, depth + 1, span)?;
                        return self.finish_value(value, expected, span);
                    }
                }
                let object = self.expression(env, object, None, depth + 1)?;
                let ty = match member {
                    s::MemberName::TupleIndex(index) => match &object.ty {
                        Ty::Tuple(fields) => {
                            let ty = fields
                                .get(*index as usize)
                                .ok_or_else(|| bad(span, "CT tuple member ordinal is absent"))?;
                            self.duplicate_type(ty, span)?
                        }
                        _ => return Err(bad(span, "CT tuple member needs a tuple receiver")),
                    },
                    s::MemberName::Name(name) => {
                        self.field(env, &object.ty, name, depth + 1, span)?
                    }
                    s::MemberName::Constrained { name, owner } => {
                        let expected = self.ty(env, owner, false, depth + 1)?;
                        self.same(&object.ty, &expected, span)?;
                        self.field(env, &object.ty, name, depth + 1, span)?
                    }
                };
                Value::typed(ty)
            }
            s::ExprKind::Call { callee, arguments } => {
                self.call(env, callee, arguments, expected, depth + 1, span)?
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                let path = self.path(callee, depth + 1)?.ok_or_else(|| {
                    bad(
                        span,
                        "CT nominal Type application needs an original declaration path",
                    )
                })?;
                if env.value(&path[0]).is_some() || env.actual(&path[0]).is_some() {
                    return Err(bad(
                        span,
                        "a local or formal shadows the CT type constructor",
                    ));
                }
                let id = self.resolve(env, &path, span)?;
                if !matches!(
                    self.declarations[id].item.kind,
                    s::ItemKind::Aggregate(_) | s::ItemKind::Enum(_) | s::ItemKind::TypeAlias(_)
                ) {
                    return Err(bad(
                        span,
                        "CT Type application selects a non-type declaration",
                    ));
                }
                Value::type_value(self.named_type(env, &path, arguments, true, span, depth + 1)?)
            }
            s::ExprKind::Record { ty, base, fields } => {
                let nominal = match ty {
                    Some(ty) => self
                        .expression(env, ty, Some(&Ty::Meta(Meta::Type)), depth + 1)?
                        .represented
                        .ok_or_else(|| {
                            bad(span, "CT record constructor has no original nominal Type")
                        })?,
                    None => self.duplicate_type(
                        expected.ok_or_else(|| {
                            bad(
                                span,
                                "CT record initializer has no original nominal context",
                            )
                        })?,
                        span,
                    )?,
                };
                let Ty::Nominal(id, actuals) = &nominal else {
                    return Err(bad(span, "CT record initializer is not a nominal product"));
                };
                let row = self.declarations[*id];
                let s::ItemKind::Aggregate(aggregate) = &row.item.kind else {
                    return Err(bad(
                        span,
                        "CT record constructor selects a nonproduct nominal",
                    ));
                };
                self.copy_type(&nominal, span, depth + 1)?;
                let mut target = self.instantiate(*id, actuals, depth + 1, span)?;
                if let Some(base) = base {
                    self.expression(env, base, Some(&nominal), depth + 1)?;
                }
                let mut names = BTreeSet::new();
                for field in fields {
                    self.step(field.span, depth)?;
                    if !names.insert(field.name.as_str()) {
                        return Err(bad(
                            field.span,
                            "CT record repeats an original field initializer",
                        ));
                    }
                    let declared = aggregate
                        .fields
                        .iter()
                        .find(|row| row.name == field.name)
                        .ok_or_else(|| bad(field.span, "CT record names no original field"))?;
                    if row.module != env.module && !declared.public {
                        return Err(bad(field.span, "CT record accesses a private source field"));
                    }
                    let ty = self.ty(&mut target, &declared.ty, false, depth + 1)?;
                    if let Some(value) = &field.value {
                        self.expression(env, value, Some(&ty), depth + 1)?;
                    } else {
                        let value = self.name_value(
                            env,
                            std::slice::from_ref(&field.name),
                            depth + 1,
                            field.span,
                        )?;
                        self.same(&value.ty, &ty, field.span)?;
                    }
                }
                if base.is_none() {
                    for field in &aggregate.fields {
                        if !names.contains(field.name.as_str()) && field.default.is_none() {
                            return Err(bad(span, "CT record initializer omits an original field"));
                        }
                    }
                }
                Value::typed(nominal)
            }
            s::ExprKind::Quote { category, template } => {
                self.quote_tree(&template.tree, template.holes.len(), depth + 1)?;
                for hole in &template.holes {
                    let expected = match hole.kind {
                        s::QuoteHoleKind::Expr => Ty::Meta(Meta::Syntax(Category::Expr)),
                        s::QuoteHoleKind::Type => Ty::Meta(Meta::Syntax(Category::Type)),
                        s::QuoteHoleKind::StmtList => Ty::Meta(Meta::Syntax(Category::StmtList)),
                        s::QuoteHoleKind::ItemList => Ty::Meta(Meta::Syntax(Category::ItemList)),
                        s::QuoteHoleKind::Ident { .. } => Ty::Meta(Meta::Ident),
                    };
                    self.expression(env, &hole.value, Some(&expected), depth + 1)?;
                }
                Value::typed(Ty::Meta(Meta::Syntax(match category {
                    s::SyntaxCategory::Expr => Category::Expr,
                    s::SyntaxCategory::Type => Category::Type,
                    s::SyntaxCategory::StmtList => Category::StmtList,
                    s::SyntaxCategory::ItemList => Category::ItemList,
                })))
            }
            s::ExprKind::MetaBlock(block) => self.meta_block(env, block, expected, depth + 1)?,
            // Raw token islands are invocation syntax, never an executable CT
            // value. They remain checked at the Source expansion boundary.
            _ => {
                return Err(bad(
                    span,
                    "original expression cannot execute in the pure CT language",
                ));
            }
        };
        self.finish_value(value, expected, span)
    }
    fn finish_value(&mut self, value: Value, expected: Option<&Ty>, span: Span) -> R<Value> {
        if let Some(expected) = expected {
            if let Ty::Sum(members) = expected {
                if &value.ty != expected {
                    let matches = members.iter().filter(|member| **member == value.ty).count();
                    if matches == 1 {
                        return Ok(Value::typed(self.duplicate_type(expected, span)?));
                    }
                }
            }
            self.same(&value.ty, expected, span)?;
        }
        Ok(value)
    }
    pub(super) fn path(&mut self, expr: &s::Expr, depth: usize) -> R<Option<Vec<String>>> {
        self.step(expr.span, depth)?;
        match &expr.kind {
            s::ExprKind::Name(name) => {
                self.alloc(name.len() + std::mem::size_of::<String>(), expr.span)?;
                Ok(Some(vec![name.clone()]))
            }
            s::ExprKind::Group(inner) => self.path(inner, depth + 1),
            s::ExprKind::Member {
                object,
                member: s::MemberName::Name(name),
            } => {
                let Some(mut path) = self.path(object, depth + 1)? else {
                    return Ok(None);
                };
                self.alloc(name.len(), expr.span)?;
                self.append(&mut path, name.clone(), expr.span)?;
                Ok(Some(path))
            }
            _ => Ok(None),
        }
    }
    fn name_value(&mut self, env: &mut Env, path: &[String], depth: usize, span: Span) -> R<Value> {
        if path.len() == 1 {
            if let Some(binding) = env.value(&path[0]) {
                self.type_work(&binding.value.ty, span, depth + 1)?;
                return self.duplicate_value(&binding.value, span);
            }
            if let Some(actual) = env.actual(&path[0]) {
                return match actual {
                    Actual::Type(ty) => Ok(Value::type_value(self.duplicate_type(ty, span)?)),
                    Actual::Const(value) => Ok(Value {
                        ty: Ty::Scalar(value.carrier),
                        constant: Some(self.duplicate_constant(value, span)?),
                        represented: None,
                    }),
                    Actual::Life(_) => Err(bad(span, "a CT Life formal is not a value or Type")),
                };
            }
            if let Some(scalar) = scalar(&path[0]) {
                return Ok(Value::type_value(Ty::Scalar(scalar)));
            }
            if matches!(
                path[0].as_str(),
                "ByteBuffer" | "ForeignHandle" | "CWideString"
            ) {
                return Ok(Value::type_value(self.named_type(
                    env,
                    path,
                    &[],
                    true,
                    span,
                    depth + 1,
                )?));
            }
        }
        let id = self.resolve(env, path, span)?;
        let row = self.declarations[id];
        match &row.item.kind {
            s::ItemKind::Aggregate(_) | s::ItemKind::Enum(_) | s::ItemKind::TypeAlias(_) => Ok(
                Value::type_value(self.named_type(env, path, &[], true, span, depth + 1)?),
            ),
            s::ItemKind::Const(constant) => {
                if !self.constants.insert(id) {
                    return Err(bad(span, "cyclic original CT constant"));
                }
                let result = (|| {
                    let mut target = self.environment(id, depth + 1)?;
                    let expected = constant
                        .ty
                        .as_ref()
                        .map(|ty| self.ty(&mut target, ty, false, depth + 1))
                        .transpose()?;
                    self.expression(
                        &mut target,
                        constant
                            .value
                            .as_ref()
                            .ok_or_else(|| bad(span, "CT constant has no original definition"))?,
                        expected.as_ref(),
                        depth + 1,
                    )
                })();
                self.constants.remove(&id);
                result
            }
            _ => Err(bad(
                span,
                "original CT declaration is not a data or Type value",
            )),
        }
    }
    pub(super) fn operand_hint(
        &mut self,
        env: &Env,
        expr: &s::Expr,
        depth: usize,
    ) -> R<Option<Ty>> {
        self.step(expr.span, depth)?;
        Ok(match &expr.kind {
            s::ExprKind::Name(name) => {
                if let Some(binding) = env.value(name) {
                    Some(self.duplicate_type(&binding.value.ty, expr.span)?)
                } else {
                    match env.actual(name) {
                        Some(Actual::Const(c)) => Some(Ty::Scalar(c.carrier)),
                        _ => None,
                    }
                }
            }
            s::ExprKind::Group(inner) | s::ExprKind::Unary { operand: inner, .. } => {
                return self.operand_hint(env, inner, depth + 1);
            }
            s::ExprKind::Literal(s::Literal::Integer(text) | s::Literal::Float(text)) => {
                self.suffix(text).map(Ty::Scalar)
            }
            s::ExprKind::Call { .. }
            | s::ExprKind::Member { .. }
            | s::ExprKind::Ascribed { .. } => {
                let mut environment = self.environment_copy(env, expr.span)?;
                Some(self.expression(&mut environment, expr, None, depth + 1)?.ty)
            }
            _ => None,
        })
    }
    fn suffix(&self, text: &str) -> Option<Scalar> {
        [
            "isize", "usize", "i64", "i32", "i16", "i8", "u64", "u32", "u16", "u8", "f64", "f32",
        ]
        .into_iter()
        .find(|suffix| text.ends_with(suffix))
        .and_then(scalar)
    }
    pub(super) fn literal(
        &mut self,
        literal: &s::Literal,
        expected: Option<&Ty>,
        negative: bool,
        span: Span,
    ) -> R<Value> {
        match literal {
            s::Literal::Bool(value) => Ok(Value {
                ty: bool_ty(),
                constant: Some(Constant {
                    carrier: Scalar::Bool,
                    term: Term::Bool(*value),
                }),
                represented: None,
            }),
            s::Literal::String(text) => {
                self.work(text.len(), span)?;
                Ok(Value::typed(Ty::Meta(Meta::Text)))
            }
            s::Literal::Integer(text) | s::Literal::Float(text) => {
                self.work(text.len().saturating_mul(2).saturating_add(1), span)?;
                let floating = matches!(literal, s::Literal::Float(_));
                let context = match expected {
                    Some(Ty::Scalar(s)) if !matches!(s, Scalar::Bool | Scalar::Unit) => Some(*s),
                    _ => None,
                };
                let carrier = self
                    .suffix(text)
                    .or_else(|| {
                        if text.starts_with("@bits:f32:") {
                            Some(Scalar::F32)
                        } else if text.starts_with("@bits:f64:") {
                            Some(Scalar::F64)
                        } else {
                            None
                        }
                    })
                    .or(context)
                    .unwrap_or(if floating { Scalar::F64 } else { Scalar::I32 });
                if floating != matches!(carrier, Scalar::F32 | Scalar::F64) {
                    return Err(bad(
                        span,
                        "CT literal has an incompatible original scalar carrier",
                    ));
                }
                let term = if floating {
                    use zil_base::numeric::{FloatFormat, FloatValue};
                    let format = if carrier == Scalar::F32 {
                        FloatFormat::F32
                    } else {
                        FloatFormat::F64
                    };
                    self.work(zil_base::numeric::decimal_work(text.len()), span)?;
                    let value = if text.starts_with("@bits:") {
                        FloatValue::from_generated_spelling(text)
                    } else {
                        let spelling = text
                            .strip_suffix(if carrier == Scalar::F32 { "f32" } else { "f64" })
                            .unwrap_or(text);
                        FloatValue::from_decimal(spelling, format)
                    }
                    .map_err(|_| {
                        bad(
                            span,
                            "original CT floating literal is outside its exact finite domain",
                        )
                    })?;
                    if value.format() != format {
                        return Err(bad(span, "original CT generated float has another width"));
                    }
                    let value = if negative { value.negate() } else { value };
                    self.alloc(26, span)?;
                    Term::Literal(value.generated_spelling())
                } else {
                    let mut digits = text.as_str();
                    for suffix in [
                        "isize", "usize", "i64", "i32", "i16", "i8", "u64", "u32", "u16", "u8",
                    ] {
                        if let Some(prefix) = digits.strip_suffix(suffix) {
                            digits = prefix;
                            break;
                        }
                    }
                    self.alloc(digits.len(), span)?;
                    let digits = digits.replace('_', "");
                    let (radix, digits) = if let Some(s) = digits.strip_prefix("0x") {
                        (16, s)
                    } else if let Some(s) = digits.strip_prefix("0b") {
                        (2, s)
                    } else if let Some(s) = digits.strip_prefix("0o") {
                        (8, s)
                    } else {
                        (10, digits.as_str())
                    };
                    let mut value = i128::from_str_radix(digits, radix).map_err(|_| {
                        bad(
                            span,
                            "CT integer literal is outside its finite scalar domain",
                        )
                    })?;
                    if negative {
                        value = value
                            .checked_neg()
                            .ok_or_else(|| bad(span, "CT integer literal negation overflows"))?;
                    }
                    let (signed, bits) = match carrier {
                        Scalar::I8 => (true, 8),
                        Scalar::I16 => (true, 16),
                        Scalar::I32 => (true, 32),
                        Scalar::I64 => (true, 64),
                        Scalar::Isize => (true, self.target.pointer_width as u32),
                        Scalar::U8 => (false, 8),
                        Scalar::U16 => (false, 16),
                        Scalar::U32 => (false, 32),
                        Scalar::U64 => (false, 64),
                        Scalar::Usize => (false, self.target.pointer_width as u32),
                        _ => return Err(bad(span, "CT integer literal has nonscalar carrier")),
                    };
                    if (signed && !(-(1i128 << (bits - 1))..(1i128 << (bits - 1))).contains(&value))
                        || (!signed && !(0..(1i128 << bits)).contains(&value))
                    {
                        return Err(bad(
                            span,
                            "CT integer literal overflows its original carrier",
                        ));
                    }
                    Term::Integer(value)
                };
                Ok(Value {
                    ty: Ty::Scalar(carrier),
                    constant: Some(Constant { carrier, term }),
                    represented: None,
                })
            }
            s::Literal::Char(_) => Err(bad(span, "character literal is not a pure CT scalar")),
        }
    }
    fn constant_binary(
        &mut self,
        op: s::BinaryOp,
        left: &Constant,
        right: &Constant,
        span: Span,
    ) -> R<Option<Term>> {
        self.work(1, span)?;
        use s::BinaryOp::*;
        if let (Term::Integer(a), Term::Integer(b)) = (&left.term, &right.term) {
            let integer = match op {
                Add => a.checked_add(*b),
                Subtract => a.checked_sub(*b),
                Multiply => a.checked_mul(*b),
                Divide => a.checked_div(*b),
                Remainder => a.checked_rem(*b),
                BitAnd => Some(a & b),
                BitOr => Some(a | b),
                BitXor => Some(a ^ b),
                ShiftLeft => u32::try_from(*b).ok().and_then(|b| a.checked_shl(b)),
                ShiftRight => u32::try_from(*b).ok().and_then(|b| a.checked_shr(b)),
                Equal => return Ok(Some(Term::Bool(a == b))),
                NotEqual => return Ok(Some(Term::Bool(a != b))),
                Less => return Ok(Some(Term::Bool(a < b))),
                LessEqual => return Ok(Some(Term::Bool(a <= b))),
                Greater => return Ok(Some(Term::Bool(a > b))),
                GreaterEqual => return Ok(Some(Term::Bool(a >= b))),
                _ => return Ok(None),
            };
            return Ok(Some(Term::Integer(integer.ok_or_else(|| {
                bad(span, "closed original CT constant operation is invalid")
            })?)));
        }
        if let (Term::Bool(a), Term::Bool(b)) = (&left.term, &right.term) {
            return Ok(match op {
                And => Some(Term::Bool(*a && *b)),
                Or => Some(Term::Bool(*a || *b)),
                Equal => Some(Term::Bool(a == b)),
                NotEqual => Some(Term::Bool(a != b)),
                _ => None,
            });
        }
        Ok(None)
    }
    pub(super) fn field(
        &mut self,
        env: &Env,
        ty: &Ty,
        name: &str,
        depth: usize,
        span: Span,
    ) -> R<Ty> {
        self.step(span, depth)?;
        if matches!(
            (ty, name),
            (Ty::Meta(Meta::Shape), "fields") | (Ty::Meta(Meta::Layout), "field_offsets")
        ) {
            self.alloc(std::mem::size_of::<Ty>(), span)?;
        }
        if let Some(ty) = self.meta_field(ty, name) {
            return Ok(ty);
        }
        if name == "len" && matches!(ty, Ty::Array(..) | Ty::List(_) | Ty::Tuple(_)) {
            return Ok(Ty::Scalar(Scalar::I64));
        }
        let Ty::Nominal(id, actuals) = ty else {
            return Err(bad(
                span,
                "CT field receiver is not an original nominal product",
            ));
        };
        let row = self.declarations[*id];
        let s::ItemKind::Aggregate(aggregate) = &row.item.kind else {
            return Err(bad(span, "CT field receiver selects an enum or nonproduct"));
        };
        let field = aggregate
            .fields
            .iter()
            .find(|row| row.name == name)
            .ok_or_else(|| bad(span, "CT field is absent from its original declaration"))?;
        if row.module != env.module && !field.public {
            return Err(bad(
                span,
                "CT field is private to its actual defining module",
            ));
        }
        let mut target = self.instantiate(*id, actuals, depth + 1, span)?;
        self.ty(&mut target, &field.ty, false, depth + 1)
    }
    fn quote_tree(&mut self, tree: &s::QuoteTokenTree, holes: usize, depth: usize) -> R<()> {
        self.step(tree.span, depth)?;
        for entry in &tree.children {
            match entry {
                s::QuoteTokenTreeEntry::Token { text, span } => {
                    self.step(*span, depth + 1)?;
                    self.work(text.len(), *span)?;
                }
                s::QuoteTokenTreeEntry::Tree(tree) => self.quote_tree(tree, holes, depth + 1)?,
                s::QuoteTokenTreeEntry::Hole { index, span } => {
                    self.step(*span, depth + 1)?;
                    if *index as usize >= holes {
                        return Err(bad(
                            *span,
                            "original CT quote hole selects no actual payload",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
