//! Recursive source variants keep their own small ordinary host frames.
//! This is a mechanical extraction: node work, hints and evaluation stay ordered.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct ExprExpectedContext<'a> {
    pub(super) expected: Option<&'a TypeRef>,
    pub(super) error_context: Option<&'a TypeRef>,
    pub(super) anonymous_context: Option<&'a TypeRef>,
    pub(super) store_success_context: Option<&'a TypeRef>,
}
impl Engine<'_> {
    pub(super) fn expr_variant_ascribed(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Ascribed { ty, value } = &mut e.kind else {
            unreachable!("ascription dispatch")
        };
        self.ty(ty, depth + 1)?;
        self.expr_expected(value, env, Some(ty), depth + 1)?;
        self.budget.ty(ty)?;
        Ok(Some(ty.clone()))
    }

    pub(super) fn expr_constant_type(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        if self.traits.has_constants
            && let ExprKind::Member {
                object,
                member: MemberName::Name(member),
            } = &e.kind
            && let Some(host) = trait_bounds::constants::host_expression(
                object,
                &self.nominals,
                |name| !env.contains_key(name),
                &mut self.budget,
            )?
            && let Some(value) = self.traits.recipe_constant(
                &host,
                member,
                &trait_bounds::Bounds::new(),
                &BTreeSet::new(),
                &mut self.budget,
                e.span,
            )?
        {
            *e = value;
        }
        if trait_bounds::constants::parts(e).is_some() {
            let (value, mut ty) = self.traits.constant_value(e, &mut self.budget)?;
            *e = value;
            self.ty(&mut ty, depth + 1)?;
            return Ok(Some(ty));
        }
        Ok(None)
    }
    pub(super) fn expr_variant_unary(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprExpectedContext { expected, .. } = context;
        let ExprKind::Unary { op, operand } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let hint = if matches!(op, UnaryOp::Negate | UnaryOp::BitNot) {
                expected
            } else {
                None
            };
            let ty = self.expr_expected(operand, env, hint, depth + 1)?;
            match (*op, ty) {
                (UnaryOp::Borrow | UnaryOp::BorrowMut, Some(inner)) => Some(TypeRef {
                    span: e.span,
                    kind: TypeKind::Reference {
                        mutable: *op == UnaryOp::BorrowMut,
                        lifetime: None,
                        inner: Box::new(inner),
                    },
                }),
                (
                    UnaryOp::Dereference,
                    Some(TypeRef {
                        kind: TypeKind::Reference { inner, .. },
                        ..
                    }),
                ) => Some(*inner),
                (_, ty) => ty,
            }
        };
        Ok(result)
    }
    pub(super) fn expr_variant_binary(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprExpectedContext { expected, .. } = context;
        let ExprKind::Binary { op, left, right } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let hint = expected.filter(|_| {
                !matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                )
            });
            let float_hint = if hint.is_none()
                && check::uncontextual_float(left, &mut self.budget, depth + 1)?
            {
                self.value_hint(right, env, depth + 1, false)?
                    .filter(check::floating)
            } else {
                None
            };
            let hint = hint.or(float_hint.as_ref());
            let left_type = self.expr_expected(left, env, hint, depth + 1)?;
            self.expr_expected(right, env, hint.or(left_type.as_ref()), depth + 1)?;
            if matches!(
                op,
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or
            ) {
                Some(named("bool", e.span))
            } else {
                left_type
            }
        };
        Ok(result)
    }
    pub(super) fn expr_variant_tuple(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprExpectedContext { expected, .. } = context;
        let ExprKind::Tuple(values) = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let mut types = Vec::new();
            let mut known = true;
            let hints = expected.and_then(|t| match &grouped(t).kind {
                TypeKind::Tuple(ts) => Some(ts.as_slice()),
                _ => None,
            });
            for (index, value) in values.iter_mut().enumerate() {
                if let Some(ty) =
                    self.expr_expected(value, env, hints.and_then(|ts| ts.get(index)), depth + 1)?
                {
                    types.push(ty);
                } else {
                    known = false;
                }
            }
            known.then_some(TypeRef {
                kind: TypeKind::Tuple(types),
                span: e.span,
            })
        };
        Ok(result)
    }
    pub(super) fn expr_variant_array(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprExpectedContext { expected, .. } = context;
        let ExprKind::Array(values) = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let mut element = expected
                .and_then(|ty| match &grouped(ty).kind {
                    TypeKind::Array { element, .. } => Some(element.as_ref()),
                    _ => None,
                })
                .cloned();
            for value in values.iter_mut() {
                let actual = self.expr_expected(value, env, element.as_ref(), depth + 1)?;
                if element.is_none() {
                    element = actual;
                }
            }
            element
                .map(|element| array_type(element, values.len(), e.span))
                .transpose()?
        };
        Ok(result)
    }
    pub(super) fn expr_variant_member(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Member { object, member } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            if let MemberName::Constrained { owner, .. } = member {
                self.ty(owner, depth + 1)?;
            }
            let object_type = self.expr(object, env, depth + 1)?;
            match (object_type.as_ref().map(projection_shape), member) {
                (
                    Some(TypeRef {
                        kind: TypeKind::Tuple(fields),
                        ..
                    }),
                    MemberName::TupleIndex(i),
                ) => fields.get(*i as usize).cloned(),
                (
                    Some(ty),
                    MemberName::Name(member) | MemberName::Constrained { name: member, .. },
                ) => self.record_field(ty, member)?,
                _ => None,
            }
        };
        Ok(result)
    }
    pub(super) fn expr_variant_index(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Index { object, index } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let object_type = self.expr(object, env, depth + 1)?;
            self.expr(index, env, depth + 1)?;
            match object_type.as_ref().map(projection_shape) {
                Some(TypeRef {
                    kind: TypeKind::Array { element, .. },
                    ..
                }) => {
                    self.budget.ty(element)?;
                    Some(*element.clone())
                }
                _ => None,
            }
        };
        Ok(result)
    }
    pub(super) fn expr_variant_slice(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Slice { object, start, end } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let object_type = self.expr(object, env, depth + 1)?;
            for value in [start, end].into_iter().flatten() {
                self.expr(value, env, depth + 1)?;
            }
            match object_type.as_ref().map(projection_shape) {
                Some(TypeRef {
                    kind: TypeKind::Array { element, .. },
                    ..
                }) => {
                    self.budget.ty(element)?;
                    Some(TypeRef {
                        span: e.span,
                        kind: TypeKind::Array {
                            element: element.clone(),
                            length: None,
                        },
                    })
                }
                _ => None,
            }
        };
        Ok(result)
    }
    pub(super) fn expr_variant_if(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprExpectedContext {
            expected,
            error_context,
            anonymous_context,
            ..
        } = context;
        let ExprKind::If {
            condition,
            then_value,
            else_value,
        } = &mut e.kind
        else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            self.expr(condition, env, depth + 1)?;
            let ty = self.expr_expected(then_value, env, expected.or(error_context), depth + 1)?;
            // The erased E!R expectation does not make its first payload
            // the expected nominal type of a distinct enum alternative.
            // Preserve the old same-nominal/context-free path exactly.
            let else_constructor = match &else_value.kind {
                ExprKind::Call { callee, .. } => callee.as_ref(),
                _ => else_value.as_ref(),
            };
            let alternative = if let (Some(union), Some(first), Some((name, _, _))) = (
                error_context,
                ty.as_ref(),
                enums::constructor(else_constructor),
            ) && matches!(&grouped(first).kind,TypeKind::Named{path,..} if path.as_slice()!=[name])
                && let TypeKind::Error { error, result } = &grouped(union).kind
            {
                [error.as_ref(),result.as_ref()].into_iter().find(|payload|
                        matches!(&grouped(payload).kind,TypeKind::Named{path,..} if path.as_slice()==[name]))
                        .or(error_context)
            } else {
                anonymous_context.or(expected).or(ty.as_ref())
            };
            self.expr_expected(else_value, env, alternative, depth + 1)?;
            ty
        };
        Ok(result)
    }
    pub(super) fn expr_variant_record(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Record { ty, base, fields } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            if let Some(t) = ty
                && let ExprKind::TypeApply { callee, arguments } = &mut t.kind
                && let ExprKind::Name(template) = &callee.kind
                && matches!(
                    self.templates.get(template).map(|x| &x.kind),
                    Some(ItemKind::Aggregate(_))
                )
            {
                let mut types = Vec::new();
                for arg in arguments {
                    let GenericArg::Type(value) = arg else {
                        return Err(unsupported(e.span, "const generic argument"));
                    };
                    self.ty(value, depth + 1)?;
                    types.push(value.clone());
                }
                t.kind = ExprKind::Name(self.instantiate(template, types, e.span)?);
            }
            let explicit_type = ty.as_ref().and_then(|t| match &t.kind {
                ExprKind::Name(n) => Some(named(n, t.span)),
                _ => None,
            });
            let base_type = if let Some(base) = base {
                self.expr_expected(base, env, explicit_type.as_ref(), depth + 1)?
            } else {
                None
            };
            let record_type = explicit_type.or(base_type);
            for field in fields {
                if let Some(value) = &mut field.value {
                    let hint = record_type
                        .as_ref()
                        .map(|ty| self.record_field(ty, &field.name))
                        .transpose()?
                        .flatten();
                    self.expr_expected(value, env, hint.as_ref(), depth + 1)?;
                }
            }
            record_type
        };
        Ok(result)
    }
    pub(super) fn expr_variant_closure(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } = &mut e.kind
        else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let mut inner = self.clone_env(env)?;
            for param in params {
                if let Some(t) = &mut param.ty {
                    self.ty(t, depth + 1)?;
                }
                inner.insert(param.name.clone(), param.ty.clone());
            }
            if let Some(t) = return_type {
                self.ty(t, depth + 1)?;
            }
            let saved = std::mem::replace(&mut self.return_type, return_type.clone());
            match body {
                ArrowBody::Block(b) => self.block(b, &mut inner, depth + 1)?,
                ArrowBody::Expression(e) => {
                    let expected = self.return_type.clone();
                    self.expr_expected(e, &mut inner, expected.as_ref(), depth + 1)?;
                }
            }
            self.return_type = saved;
            None
        };
        Ok(result)
    }
    pub(super) fn expr_variant_meta_block(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::MetaBlock(block) = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            self.block(block, env, depth + 1)?;
            None
        };
        Ok(result)
    }
    pub(super) fn expr_variant_pipeline(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Pipeline { input, target } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            self.expr(input, env, depth + 1)?;
            self.expr(target, env, depth + 1)?;
            None
        };
        Ok(result)
    }
}
