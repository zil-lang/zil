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
    fn record_layout_life(&mut self, span: Span) -> R<String> {
        let mut index = self.life_scope.len();
        loop {
            self.budget
                .identity_work(self.life_scope.len().saturating_add(32), span)?;
            let name = format!("$constructor_layout_{index}");
            if !self.life_scope.contains(&name) {
                self.budget.text(&name, span)?;
                self.budget
                    .lineage_push(&mut self.life_scope, name.clone(), span)?;
                return Ok(name);
            }
            index = index
                .checked_add(1)
                .ok_or_else(|| error(span, "constructor layout scope exhausted"))?;
        }
    }
    // Only anonymous retained data addresses receive layout placeholders.
    // A Code signature keeps its own real invocation/quantifier frame.
    fn record_layout_type(&mut self, ty: &mut TypeRef, depth: usize) -> R<()> {
        self.budget.node(ty.span, depth)?;
        match &mut ty.kind {
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if lifetime.is_none() {
                    *lifetime = Some(self.record_layout_life(ty.span)?);
                }
                self.record_layout_type(inner, depth + 1)?;
            }
            TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::Array { element: inner, .. } => {
                self.record_layout_type(inner, depth + 1)?
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                for field in fields {
                    self.record_layout_type(field, depth + 1)?;
                }
            }
            TypeKind::Error { error, result } => {
                self.record_layout_type(error, depth + 1)?;
                self.record_layout_type(result, depth + 1)?;
            }
            TypeKind::Named { path, args } if !matches!(path.as_slice(), [name] if matches!(name.as_str(), "Fn" | "FnMut" | "FnOnce")) => {
                for argument in args {
                    if let GenericArg::Type(ty) = argument {
                        self.record_layout_type(ty, depth + 1)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn record_life_used(&mut self, ty: &TypeRef, name: &str, depth: usize) -> R<bool> {
        self.budget.node(ty.span, depth)?;
        Ok(match &ty.kind {
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                lifetime.as_deref() == Some(name)
                    || self.record_life_used(inner, name, depth + 1)?
            }
            TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::Array { element: inner, .. } => {
                self.record_life_used(inner, name, depth + 1)?
            }
            TypeKind::ForLifetime { lifetime, inner } => {
                lifetime != name && self.record_life_used(inner, name, depth + 1)?
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) => {
                let mut used = false;
                for field in fields {
                    used |= self.record_life_used(field, name, depth + 1)?;
                }
                used
            }
            TypeKind::Error { error, result } => {
                self.record_life_used(error, name, depth + 1)?
                    || self.record_life_used(result, name, depth + 1)?
            }
            TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
                let mut used = false;
                for argument in args {
                    used |= match argument {
                        GenericArg::Lifetime(life) => life == name,
                        GenericArg::Type(ty) => self.record_life_used(ty, name, depth + 1)?,
                        _ => false,
                    };
                }
                used
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                let mut used = self.record_life_used(result, name, depth + 1)?;
                for parameter in params {
                    used |= self.record_life_used(parameter, name, depth + 1)?;
                }
                used
            }
            _ => false,
        })
    }
    fn infer_record_layout(
        &mut self,
        template: &str,
        declaration: &Aggregate,
        fields: &mut [RecordField],
        env: &mut Env,
        span: Span,
        depth: usize,
    ) -> R<String> {
        let scope = self.source_unification_scope(env, span)?;
        let variables = declaration
            .generics
            .iter()
            .filter(|formal| !matches!(formal.kind, GenericParamKind::Lifetime))
            .map(|formal| formal.name.clone())
            .collect();
        let mut found = BTreeMap::new();
        let mut seen = BTreeSet::new();
        if fields.len() != declaration.fields.len() {
            return Err(error(span, "inferred record omits or adds fields"));
        }
        // Read-only, nondefaulting lookahead lets every independently typed
        // field constrain the joint vector before unsuffixed literals receive
        // a context. Actual field evaluation below stays in source order.
        for field in fields.iter() {
            self.budget
                .identity_work(declaration.fields.len(), field.span)?;
            let original = declaration
                .fields
                .iter()
                .find(|original| original.name == field.name)
                .ok_or_else(|| {
                    error(
                        field.span,
                        "inferred record changes its declaration field identity",
                    )
                })?;
            let actual = if let Some(value) = &field.value {
                self.value_hint(value, env, depth + 1, false)?
            } else {
                let actual = env.get(&field.name).and_then(Option::as_ref);
                if let Some(actual) = actual {
                    self.budget.ty(actual)?;
                }
                actual.cloned()
            };
            if let Some(actual) = actual {
                let actual = self.original_type(&actual)?;
                self.record_layout_constraint(
                    &original.ty,
                    &actual,
                    &declaration.generics,
                    &mut found,
                    scope.as_ref(),
                    depth + 1,
                )?;
            }
        }
        for field in fields {
            self.budget.node(field.span, depth)?;
            if !seen.insert(field.name.as_str()) {
                return Err(error(field.span, "inferred record repeats a field"));
            }
            self.budget
                .identity_work(declaration.fields.len(), field.span)?;
            let original = declaration
                .fields
                .iter()
                .find(|original| original.name == field.name)
                .ok_or_else(|| {
                    error(
                        field.span,
                        "inferred record changes its declaration field identity",
                    )
                })?;
            let hint = enums::substituted(&original.ty, &found, &mut self.budget)?;
            let hint = (!check::dependent(&hint, &variables)
                && !self.qualified_actual_syntax(&hint, depth + 1)?)
            .then_some(hint);
            let actual = if let Some(value) = &mut field.value {
                match self.expr_expected(value, env, hint.as_ref(), depth + 1)? {
                    Some(actual) => Some(actual),
                    None => self.value_hint(value, env, depth + 1, true)?,
                }
            } else {
                self.budget.text(&field.name, field.span)?;
                let actual = env.get(&field.name).and_then(Option::as_ref);
                if let Some(actual) = actual {
                    self.budget.ty(actual)?;
                }
                actual.cloned()
            }
            .ok_or_else(|| error(field.span, "record field type cannot be inferred"))?;
            let actual = self.original_type(&actual)?;
            self.record_layout_constraint(
                &original.ty,
                &actual,
                &declaration.generics,
                &mut found,
                scope.as_ref(),
                depth + 1,
            )?;
        }
        // Placeholder names below describe only the produced physical domain.
        // The original RecordConstructor retains every real source expression;
        // Types and both kernels independently solve its actual mixed vector.
        let mark = self.life_scope.len();
        let result = (|| {
            let mut arguments = Vec::new();
            for formal in &declaration.generics {
                self.budget.node(formal.span, depth)?;
                arguments.push(match &formal.kind {
                    GenericParamKind::Lifetime => {
                        let mut used = false;
                        for field in &declaration.fields {
                            used |= self.record_life_used(&field.ty, &formal.name, depth + 1)?;
                        }
                        if !used {
                            return Err(error(
                                span,
                                "inferred record has an unconstrained phantom lifetime",
                            ));
                        }
                        GenericArg::Lifetime(self.record_layout_life(span)?)
                    }
                    GenericParamKind::Type(_) => {
                        let mut actual = found.remove(&formal.name).ok_or_else(|| {
                            error(span, "inferred record has an unconstrained Type formal")
                        })?;
                        self.record_layout_type(&mut actual, depth + 1)?;
                        GenericArg::Type(actual)
                    }
                    GenericParamKind::Const(_) => {
                        let actual = found.remove(&formal.name).ok_or_else(|| {
                            error(span, "inferred record has an unconstrained Const formal")
                        })?;
                        let actual = consts::normalize(formal, &actual, &mut self.budget)?;
                        let value = consts::expression(&actual).ok_or_else(|| {
                            error(span, "inferred record Const loses its scalar carrier")
                        })?;
                        self.budget.const_expression(value)?;
                        GenericArg::Const(value.clone())
                    }
                });
            }
            if self.stored_application(template, &arguments, span, depth + 1)? {
                self.stored_nominal(template, &mut arguments, span, depth + 1)
                    .map(|(name, _)| name)
            } else {
                let actuals = declaration
                    .generics
                    .iter()
                    .zip(&arguments)
                    .map(|(formal, actual)| {
                        consts::argument(actual, Some(formal), &mut self.budget)
                    })
                    .collect::<R<Vec<_>>>()?;
                self.instantiate(template, actuals, span)
            }
        })();
        self.life_scope.truncate(mark);
        result
    }
    fn record_layout_constraint(
        &mut self,
        pattern: &TypeRef,
        actual: &TypeRef,
        formals: &[GenericParam],
        found: &mut BTreeMap<String, TypeRef>,
        scope: Option<&Function>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(actual.span, depth)?;
        let pattern = grouped(pattern);
        let actual = grouped(actual);
        let span = actual.span;
        if let TypeKind::Named { path, args } = &pattern.kind
            && path.len() == 1
            && args.is_empty()
            && let Some(formal) = formals.iter().find(|formal| formal.name == path[0])
            && matches!(formal.kind, GenericParamKind::Type(_))
        {
            return unify_scoped(
                pattern,
                actual,
                &BTreeSet::from([formal.name.clone()]),
                found,
                actual.span,
                &mut self.budget,
                scope,
            );
        }
        match (&pattern.kind, &actual.kind) {
            (
                TypeKind::Reference {
                    mutable: requested,
                    inner,
                    ..
                },
                TypeKind::Reference {
                    mutable: available,
                    inner: value,
                    ..
                },
            ) if requested == available => {
                self.record_layout_constraint(inner, value, formals, found, scope, depth + 1)
            }
            (
                TypeKind::Array {
                    element,
                    length: Some(length),
                },
                TypeKind::Array {
                    element: value,
                    length: Some(value_length),
                },
            ) => {
                self.record_layout_constraint(element, value, formals, found, scope, depth + 1)?;
                if let ExprKind::Name(name) = &length.kind
                    && let Some(formal) = formals.iter().find(|formal| {
                        formal.name == *name && matches!(formal.kind, GenericParamKind::Const(_))
                    })
                {
                    let actual = consts::carrier(value_length, &mut self.budget)?;
                    let actual = consts::normalize(formal, &actual, &mut self.budget)?;
                    if let Some(old) = found.get(name) {
                        if type_key_bounded(old, &mut self.budget)?
                            != type_key_bounded(&actual, &mut self.budget)?
                        {
                            return Err(error(
                                value_length.span,
                                "conflicting inferred record Const actuals",
                            ));
                        }
                    } else {
                        found.insert(name.clone(), actual);
                    }
                    Ok(())
                } else if consts::length_key(length, found)?
                    == consts::length_key(value_length, &BTreeMap::new())?
                {
                    Ok(())
                } else {
                    Err(error(
                        actual.span,
                        "inferred record changes an invariant array length",
                    ))
                }
            }
            (
                TypeKind::Named { path, args },
                TypeKind::Named {
                    path: actual_path,
                    args: actual_args,
                },
            ) if path == actual_path && args.len() == actual_args.len() => {
                for (pattern, actual) in args.iter().zip(actual_args) {
                    let name = match pattern {
                        GenericArg::Const(Expr {
                            kind: ExprKind::Name(name),
                            ..
                        }) => Some(name.as_str()),
                        GenericArg::Type(ty) => trait_bounds::plain(ty),
                        _ => None,
                    };
                    if let Some(formal) = name.and_then(|name| {
                        formals.iter().find(|formal| {
                            formal.name == name && matches!(formal.kind, GenericParamKind::Const(_))
                        })
                    }) {
                        let actual = consts::argument(actual, Some(formal), &mut self.budget)?;
                        let actual = consts::normalize(formal, &actual, &mut self.budget)?;
                        if let Some(old) = found.get(&formal.name) {
                            if type_key_bounded(old, &mut self.budget)?
                                != type_key_bounded(&actual, &mut self.budget)?
                            {
                                return Err(error(
                                    span,
                                    "conflicting inferred record Const actuals",
                                ));
                            }
                        } else {
                            found.insert(formal.name.clone(), actual);
                        }
                        continue;
                    }
                    match (pattern, actual) {
                        (GenericArg::Type(pattern), GenericArg::Type(actual)) => self
                            .record_layout_constraint(
                                pattern,
                                actual,
                                formals,
                                found,
                                scope,
                                depth + 1,
                            )?,
                        // Only the layout is selected here. The immutable
                        // original constructor independently solves every Life
                        // and proves the complete nominal class vector.
                        (GenericArg::Lifetime(_), GenericArg::Lifetime(_)) => {}
                        (GenericArg::Const(pattern), GenericArg::Const(actual)) => {
                            self.budget.const_expression(pattern)?;
                            self.budget.const_expression(actual)?;
                            if consts::length_key(pattern, &BTreeMap::new())?
                                != consts::length_key(actual, &BTreeMap::new())?
                            {
                                return Err(error(
                                    span,
                                    "inferred record changes a nested Const actual",
                                ));
                            }
                        }
                        _ => {
                            return Err(error(
                                span,
                                "inferred record changes a nested mixed argument kind",
                            ));
                        }
                    }
                }
                Ok(())
            }
            (TypeKind::Tuple(patterns), TypeKind::Tuple(actuals))
                if patterns.len() == actuals.len() =>
            {
                for (pattern, actual) in patterns.iter().zip(actuals) {
                    self.record_layout_constraint(
                        pattern,
                        actual,
                        formals,
                        found,
                        scope,
                        depth + 1,
                    )?;
                }
                Ok(())
            }
            _ => {
                let variables = formals
                    .iter()
                    .filter(|formal| matches!(formal.kind, GenericParamKind::Type(_)))
                    .map(|formal| formal.name.clone())
                    .collect();
                unify_scoped(
                    pattern,
                    actual,
                    &variables,
                    found,
                    actual.span,
                    &mut self.budget,
                    scope,
                )
            }
        }
    }
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
                (UnaryOp::Borrow | UnaryOp::BorrowMut, Some(mut inner)) => {
                    self.preserve_resource_reference(&mut inner)?;
                    Some(TypeRef {
                        span: e.span,
                        kind: TypeKind::Reference {
                            mutable: *op == UnaryOp::BorrowMut,
                            lifetime: None,
                            inner: Box::new(inner),
                        },
                    })
                }
                (
                    UnaryOp::Dereference,
                    Some(TypeRef {
                        kind: TypeKind::Reference { inner, .. },
                        ..
                    }),
                ) => Some(*inner),
                (UnaryOp::Give, Some(ty)) => self.given_resource_hint(operand, ty, env)?,
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
            let peer_hint = if hint.is_none()
                && check::uncontextual_float(left, &mut self.budget, depth + 1)?
            {
                self.value_hint(right, env, depth + 1, false)?
                    .filter(check::floating)
            } else if hint.is_none()
                && check::uncontextual_integer(left, &mut self.budget, depth + 1)?
            {
                self.value_hint(right, env, depth + 1, false)?
                    .filter(check::integer)
            } else {
                None
            };
            let hint = hint.or(peer_hint.as_ref());
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
            let numeric_hint =
                if expected.is_none() && error_context.is_none() && anonymous_context.is_none() {
                    let first = self.value_hint(then_value, env, depth + 1, false)?;
                    let hint = match first {
                        Some(value) => Some(value),
                        None => self.value_hint(else_value, env, depth + 1, false)?,
                    };
                    hint.filter(|ty| check::integer(ty) || check::floating(ty))
                } else {
                    None
                };
            let expected = expected.or(numeric_hint.as_ref());
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
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Record { ty, base, fields } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let mut source_record = None;
            let mut fields_checked = false;
            if let Some(t) = ty.as_mut()
                && let ExprKind::Name(template) = &t.kind
                && let Some((declaration, _)) = self.record_declaration(template, e.span)?
                && !params(declaration).is_empty()
                && let Some(expected) = context.expected
            {
                // The argument/result context already selected a complete
                // layout. Authenticate its original nominal declaration;
                // its machine name is not lifetime or generic evidence.
                let origin = self.template_origin(declaration)?.ok_or_else(|| {
                    error(
                        t.span,
                        "inferred record lacks its original declaration provenance",
                    )
                })?;
                let origins = self.origins.ok_or_else(|| {
                    error(
                        t.span,
                        "inferred record lacks its original declaration inventory",
                    )
                })?;
                self.budget
                    .identity_work(origins.identities.len(), t.span)?;
                let identity = origins.identities.get(&origin.declaration).ok_or_else(|| {
                    error(
                        t.span,
                        "inferred record original declaration identity is absent",
                    )
                })?;
                let mut physical = self.original_type(expected)?;
                if let TypeKind::Named { path, args } = &physical.kind
                    && path.as_slice() == ["Managed"]
                    && let [GenericArg::Type(payload)] = args.as_slice()
                {
                    self.budget.ty(payload)?;
                    physical = payload.clone();
                }
                self.ty(&mut physical, depth + 1)?;
                if let TypeKind::Named { path, args } = &grouped(&physical).kind
                    && path.len() == 1
                    && args.is_empty()
                {
                    let mut matched = false;
                    for instance in &self.products.storage.instances {
                        self.budget.node(t.span, depth)?;
                        let zil_ir::Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                        else {
                            continue;
                        };
                        let (zil_ir::Type::Record { name, .. }
                        | zil_ir::Type::Package { name, .. }) = storage.as_ref()
                        else {
                            continue;
                        };
                        self.budget.text(name, t.span)?;
                        self.budget.text(&path[0], t.span)?;
                        if name == &path[0] {
                            if instance.declaration.id.module != origin.declaration.module
                                || instance.declaration.id.compiler != origin.declaration.compiler
                                || instance.declaration.id.declaration != identity.declaration
                            {
                                return Err(error(
                                    t.span,
                                    "inferred record context changes its original nominal declaration",
                                ));
                            }
                            matched = true;
                        }
                    }
                    if matched {
                        self.budget.text(&path[0], t.span)?;
                        t.kind = ExprKind::Name(path[0].clone());
                    }
                }
            }
            if base.is_none()
                && let Some(t) = ty.as_mut()
                && let ExprKind::Name(template) = &t.kind
                && let Some((declaration, _)) = self.record_declaration(template, e.span)?
                && let ItemKind::Aggregate(declaration) = &declaration.kind
                && !declaration.generics.is_empty()
            {
                let name = self.infer_record_layout(
                    template,
                    declaration,
                    fields,
                    env,
                    e.span,
                    depth + 1,
                )?;
                t.kind = ExprKind::Name(name);
                fields_checked = true;
            }
            if let Some(t) = ty
                && let ExprKind::TypeApply { callee, arguments } = &mut t.kind
                && let ExprKind::Name(template) = &callee.kind
                && let Some((declaration, stored)) = self.record_declaration(template, e.span)?
            {
                if stored
                    || (self.stored_nominals
                        && self.stored_application(template, arguments, e.span, depth + 1)?)
                {
                    let source = TypeRef {
                        span: t.span,
                        kind: TypeKind::Named {
                            path: vec![template.clone()],
                            args: arguments.to_vec(),
                        },
                    };
                    self.budget.ty(&source)?;
                    source_record = Some(source);
                    let (name, _) = self.stored_nominal(template, arguments, e.span, depth + 1)?;
                    t.kind = ExprKind::Name(name);
                } else {
                    let mut types = Vec::new();
                    let declaration = Some(declaration);
                    for (index, arg) in arguments.iter_mut().enumerate() {
                        let parameter = declaration.and_then(|item| {
                            params(item)
                                .iter()
                                .filter(|p| !matches!(p.kind, GenericParamKind::Lifetime))
                                .nth(index)
                        });
                        if let GenericArg::Type(value) = arg
                            && !parameter
                                .is_some_and(|p| matches!(p.kind, GenericParamKind::Const(_)))
                        {
                            self.ty(value, depth + 1)?;
                            types.push(value.clone());
                        } else {
                            types.push(consts::argument(arg, parameter, &mut self.budget)?);
                        }
                    }
                    t.kind = ExprKind::Name(self.instantiate(template, types, e.span)?);
                }
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
                if fields_checked {
                    break;
                }
                if let Some(value) = &mut field.value {
                    let hint = if let Some(source) = &source_record {
                        self.source_record_field(source, &field.name)?
                    } else {
                        record_type
                            .as_ref()
                            .map(|ty| self.record_field(ty, &field.name))
                            .transpose()?
                            .flatten()
                    };
                    self.expr_expected(value, env, hint.as_ref(), depth + 1)?;
                }
            }
            self.constructed_resource_hint(source_record.or(record_type))?
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
            context,
            params,
            return_type,
            body,
            ..
        } = &mut e.kind
        else {
            unreachable!("expression variant dispatch")
        };
        // An input token is never a production record for this request.
        *context = ClosureContextMarker::default();
        if self.closure_recording.is_some() {
            return Err(unsupported(
                e.span,
                "nested closure source contexts require their complete producer",
            ));
        }
        let result = {
            let mut inner = self.clone_env(env)?;
            inner.closure_depth = inner.closure_depth.checked_add(1).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "closure lexical depth exhausted",
                    Some(e.span),
                )
            })?;
            let mut recording = None;
            for index in 0..params.len() {
                if recording.is_none()
                    && let Some(t) = &params[index].ty
                    && let Err(error) = rich_pointer::reject_fixed_closure(
                        t,
                        &self.products.storage.instances,
                        &mut self.budget,
                    )
                {
                    if error.category != zil_base::ResultCategory::Unsupported
                        || !error
                            .message
                            .contains("genuine enclosing definition context")
                    {
                        return Err(error);
                    }
                    recording = Some(self.start_closure_context(
                        params,
                        return_type,
                        env.closure_depth,
                        e.span,
                    )?);
                }
                let param = &mut params[index];
                if let Some(t) = &mut param.ty {
                    self.ty(t, depth + 1)?;
                }
                inner.insert(param.name.clone(), param.ty.clone());
            }
            if let Some(t) = return_type
                && recording.is_none()
                && let Err(error) = rich_pointer::reject_fixed_closure(
                    t,
                    &self.products.storage.instances,
                    &mut self.budget,
                )
            {
                if error.category != zil_base::ResultCategory::Unsupported
                    || !error
                        .message
                        .contains("genuine enclosing definition context")
                {
                    return Err(error);
                }
                recording = Some(self.start_closure_context(
                    params,
                    return_type,
                    env.closure_depth,
                    e.span,
                )?);
            }
            if let Some(t) = return_type {
                self.ty(t, depth + 1)?;
            }
            let saved = std::mem::replace(&mut self.return_type, return_type.clone());
            // This closure owns a separate result hint. The enclosing stored
            // enum's source clone charge resumes only after its body returns.
            let saved_source_enum_result =
                std::mem::replace(&mut self.source_enum_return_active, false);
            self.closure_recording = recording;
            let body_result = match body {
                ArrowBody::Block(b) => self.block(b, &mut inner, depth + 1),
                ArrowBody::Expression(e) => {
                    let expected = self.return_type.clone();
                    self.expr_expected(e, &mut inner, expected.as_ref(), depth + 1)
                        .map(|_| ())
                }
            };
            let recording = self.closure_recording.take();
            self.return_type = saved;
            self.source_enum_return_active = saved_source_enum_result;
            body_result?;
            if let Some(recording) = recording {
                *context = ClosureContextMarker::untrusted(
                    self.finish_closure_context(recording, e.span)?,
                );
                // The real producer has retained the complete scoped header,
                // including named parent Fixed classes and actual input anchors.
                // Only its physical annotation now follows function-header
                // erasure; the immutable original signature is unchanged.
                for ty in params
                    .iter_mut()
                    .filter_map(|parameter| parameter.ty.as_mut())
                    .chain(return_type.iter_mut())
                {
                    self.budget.ty(ty)?;
                    stored_nominal::erase_stored_type_lifetimes(ty);
                }
            }
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
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprKind::Pipeline { input, target } = &mut e.kind else {
            unreachable!("expression variant dispatch");
        };
        let mut callee = target.as_mut();
        let mut target_depth = depth + 1;
        while matches!(callee.kind, ExprKind::Group(_)) {
            let ExprKind::Group(inner) = &mut callee.kind else {
                unreachable!("grouped pipeline target");
            };
            self.budget.node(inner.span, target_depth)?;
            target_depth += 1;
            callee = inner;
        }
        let name = match &callee.kind {
            ExprKind::Name(name) => Some(name),
            ExprKind::TypeApply { callee, .. } => match &callee.kind {
                ExprKind::Name(name) => Some(name),
                _ => None,
            },
            _ => None,
        };
        if let Some(name) = name {
            self.budget.text(name, callee.span)?;
            if !env.contains_key(name)
                && self.templates.get(name).is_some_and(|item| matches!(item.kind, ItemKind::Function(_)))
            {
                // Reuse ordinary inference on the actual input and full
                // generic target. The outer Pipeline still prepares the left
                // value before evaluating the selected invocation target.
                let span = e.span;
                self.budget.node(span, depth)?;
                self.budget.lineage_bytes(
                    std::mem::size_of::<Expr>() + std::mem::size_of::<Argument>(), span,
                )?;
                let placeholder = || Expr {
                    semantic: Default::default(), span,
                    kind: ExprKind::Literal(Literal::Bool(false)),
                };
                let argument = Argument {
                    spread: false,
                    span: input.span,
                    value: std::mem::replace(input.as_mut(), placeholder()),
                };
                let mut invocation = Expr {
                    semantic: Default::default(), span,
                    kind: ExprKind::Call {
                        callee: Box::new(std::mem::replace(callee, placeholder())),
                        arguments: vec![argument],
                    },
                };
                let result = self.expr_call_normal(&mut invocation, env, context, depth);
                let ExprKind::Call { callee: selected, mut arguments } = invocation.kind else {
                    unreachable!("ordinary static invocation retains its call");
                };
                *callee = *selected;
                **input = arguments.pop().expect("one pipeline input").value;
                return result;
            }
        }
        self.expr(input, env, depth + 1)?;
        self.expr(target, env, depth + 1)?;
        Ok(None)
    }
}
