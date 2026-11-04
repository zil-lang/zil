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
    pub(super) fn expr_variant_call(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprExpectedContext {
            expected,
            store_success_context,
            ..
        } = context;
        let ExprKind::Call { callee, arguments } = &mut e.kind else {
            unreachable!("expression variant dispatch")
        };
        let result = {
            let mut trait_signature = None;
            let mut type_qualified = false;
            if self.traits.has_instances
                && let ExprKind::Member {
                    object,
                    member: MemberName::Name(member) | MemberName::Constrained { name: member, .. },
                } = &mut callee.kind
            {
                let mut hint = self.value_hint(object, env, depth + 1, false)?;
                if hint.is_none()
                    && let ExprKind::Name(name) = &object.kind
                    && !env.contains_key(name)
                    && self.nominals.contains_key(name)
                {
                    hint = Some(named(name, object.span));
                    type_qualified = true;
                }
                if let Some(actual) = hint {
                    let actual = self.original_type(&actual)?;
                    if let Some(host) = trait_bounds::plain(projection_shape(&actual))
                        && let Some((target, signature)) = self.traits.concrete_method(
                            host,
                            member,
                            &mut self.budget,
                            callee.span,
                        )?
                    {
                        self.budget.text(&target, callee.span)?;
                        member.clone_from(&target);
                        trait_signature = Some(signature);
                    }
                    if trait_signature.is_none()
                        && member.starts_with("$trait_")
                        && let Some(signature) =
                            self.realized_method(projection_shape(&actual), member, callee.span)?
                    {
                        trait_signature = Some(signature);
                    }
                    if trait_signature.is_none()
                        && !self.traits.recipes.is_empty()
                        && let Some((target, signature, witness)) = self.traits.recipe_method(
                            projection_shape(&actual),
                            member,
                            &mut self.budget,
                            callee.span,
                        )?
                    {
                        self.realize_witness(witness)?;
                        self.budget.text(&target, callee.span)?;
                        member.clone_from(&target);
                        trait_signature = Some(signature);
                    }
                }
            }
            if let Some(name) =
                crate::foreign::bare_name(callee).filter(|name| stores::primitive(name))
            {
                if arguments.len() != stores::arity(name) || arguments.iter().any(|arg| arg.spread)
                {
                    return Err(error(
                        e.span,
                        "Store primitive argument count or shape differs",
                    ));
                }
                let element = if name == "store_new" {
                    expected
                        .or(store_success_context.and_then(stores::carrier))
                        .and_then(stores::element)
                        .cloned()
                        .ok_or_else(|| {
                            error(
                                e.span,
                                "store_new requires an explicit Store element context",
                            )
                        })?
                } else {
                    let store = self
                        .expr_expected(&mut arguments[0].value, env, None, depth + 1)?
                        .ok_or_else(|| {
                            error(e.span, "Store primitive requires a known Store type")
                        })?;
                    stores::element(&store).cloned().ok_or_else(|| {
                        error(e.span, "Store primitive requires a Store description")
                    })?
                };
                self.budget.ty(&element)?;
                if arguments.len() == 2 {
                    let wanted = if name == "store_push" {
                        element.clone()
                    } else {
                        named("i64", e.span)
                    };
                    self.expr_expected(&mut arguments[1].value, env, Some(&wanted), depth + 1)?;
                }
                return Ok(Some(stores::result(name, &element, e.span)));
            }
            if let Some((parameters, result)) =
                crate::foreign::bare_name(callee).and_then(|name| bytes::signature(name, e.span))
            {
                if arguments.len() != parameters.len() || arguments.iter().any(|arg| arg.spread) {
                    return Err(error(
                        e.span,
                        "byte primitive argument count or shape differs",
                    ));
                }
                for (argument, parameter) in arguments.iter_mut().zip(&parameters) {
                    self.expr_expected(&mut argument.value, env, Some(parameter), depth + 1)?;
                }
                self.budget.ty(&result)?;
                return Ok(Some(result));
            }
            if let Some(value) = crate::foreign::literal_constructor(callee, arguments, e.span) {
                let value = value?;
                if let zil_ir::Constant::WideString(units) = &value {
                    self.budget.wide_units(units, e.span)?;
                }
                return Ok(Some(named(&value.ty().name(), e.span)));
            }
            if crate::foreign::bare_name(callee) == Some("c_callback") {
                let [argument] = arguments.as_slice() else {
                    return Err(crate::foreign::callback_error(
                        e.span,
                        "c_callback requires one static symbol",
                    ));
                };
                if argument.spread {
                    return Err(crate::foreign::callback_error(
                        e.span,
                        "C callback constructor does not accept spread arguments",
                    ));
                }
                let Some(target) = crate::foreign::bare_name(&argument.value)
                    .filter(|name| !env.contains_key(*name))
                else {
                    return Err(crate::foreign::callback_error(
                        e.span,
                        "c_callback requires one static symbol",
                    ));
                };
                let Some(function) = self.signatures.get(target) else {
                    return Err(crate::foreign::callback_error(
                        e.span,
                        "unknown callback symbol",
                    ));
                };
                for parameter in &function.params {
                    self.budget.ty(&parameter.ty)?;
                }
                if let Some(result) = &function.return_type {
                    self.budget.ty(result)?;
                }
                return crate::foreign::callback_syntax_type(function, e.span).map(Some);
            }
            if let Some(name) = crate::integer_conversion_name(callee) {
                if arguments.len() != 1 || arguments[0].spread {
                    return Err(crate::conversion_error(
                        e.span,
                        "integer conversion requires exactly one non-spread argument",
                    ));
                }
                self.expr(&mut arguments[0].value, env, depth + 1)?;
                return Ok(Some(named(name, e.span)));
            }
            let expected = expected.or_else(|| {
                let result = stores::call_name(callee)
                    .and_then(|name| self.signatures.get(name))?
                    .return_type
                    .as_ref()?;
                stores::call_context(result, store_success_context?)
            });
            let value_inference_allowed = matches!(&callee.kind, ExprKind::Name(name)
                    if !env.contains_key(name) && self.signatures.get(name)
                        .is_some_and(|f| store_templates::parameter(f).is_some()));
            let mut value_inferred = false;
            let mut hints = self.call_hints(callee, env, expected)?;
            let mut actual = Vec::new();
            let mut contextual_constraints = false;
            for index in 0..arguments.len() {
                let mut hint = hints
                    .as_ref()
                    .map(|h| h.hint(index, &mut self.budget))
                    .transpose()?
                    .flatten();
                if hint.is_none()
                    && hints
                        .as_ref()
                        .map(|h| h.unresolved_context(index, &mut self.budget))
                        .transpose()?
                        .unwrap_or(false)
                {
                    contextual_constraints = true;
                    for union_pass in [false, true] {
                        for (other_index, argument) in arguments.iter().enumerate() {
                            self.budget.node(argument.span, depth + 1)?;
                            if !hints
                                .as_ref()
                                .expect("contextual call hints")
                                .independent_input(other_index, union_pass, &mut self.budget)?
                            {
                                continue;
                            }
                            if let Some(ty) =
                                self.value_hint(&argument.value, env, depth + 1, true)?
                            {
                                let ty = self.original_type(&ty)?;
                                hints
                                    .as_mut()
                                    .expect("contextual call hints")
                                    .independent_constraint(
                                        other_index,
                                        &ty,
                                        argument.span,
                                        &mut self.budget,
                                    )?;
                            }
                        }
                    }
                    hint = hints
                        .as_ref()
                        .map(|h| h.hint(index, &mut self.budget))
                        .transpose()?
                        .flatten();
                }
                if hint.is_none()
                    && let Some(signature) = &trait_signature
                {
                    let skip = usize::from(
                        !type_qualified
                            && signature.params.first().is_some_and(|p| p.name == "self"),
                    );
                    if let Some(parameter) = signature.params.get(index + skip) {
                        self.budget.ty(&parameter.ty)?;
                        hint = Some(parameter.ty.clone());
                    }
                }
                let arg = &mut arguments[index];
                let ty = self.expr_expected(&mut arg.value, env, hint.as_ref(), depth + 1)?;
                let ty = if ty.is_none()
                    && hint
                        .as_ref()
                        .is_some_and(|ty| matches!(grouped(ty).kind, TypeKind::Error { .. }))
                {
                    self.value_hint(&arg.value, env, depth + 1, true)?
                } else {
                    ty
                };
                if value_inference_allowed
                    && let Some(ty) = &ty
                    && self.inferred_state_value(ty, 0)?
                {
                    value_inferred = true;
                    // Concrete C identities belong to Types. A prior
                    // symbolic spelling is not a cross-argument equality
                    // constraint; the new static Call checks all real T's.
                    hints = None;
                }
                if let (Some(hints), Some(ty)) = (&mut hints, &ty) {
                    let ty = self.original_type(ty)?;
                    hints.constrain(index, &ty, arg.span, &mut self.budget)?;
                }
                actual.push(ty);
            }
            let mut explicit = None;
            let target = match &mut callee.kind {
                ExprKind::TypeApply {
                    callee: base,
                    arguments: args,
                } => {
                    let mut types = Vec::new();
                    let declaration = if let ExprKind::Name(name) = &base.kind {
                        self.templates.get(name).copied()
                    } else {
                        None
                    };
                    for (index, arg) in args.iter_mut().enumerate() {
                        let parameter = declaration.and_then(|item| params(item).get(index));
                        if let GenericArg::Type(t) = arg
                            && !parameter
                                .is_some_and(|p| matches!(p.kind, GenericParamKind::Const(_)))
                        {
                            self.ty(t, depth + 1)?;
                            types.push(t.clone());
                        } else {
                            types.push(consts::argument(arg, parameter, &mut self.budget)?);
                        }
                    }
                    explicit = Some(types);
                    match &base.kind {
                        ExprKind::Name(n) if !env.contains_key(n) => Some(n.clone()),
                        _ => None,
                    }
                }
                ExprKind::Name(n) if !env.contains_key(n) => Some(n.clone()),
                _ => None,
            };
            if let Some(target) = target {
                if let Some(template) = self.templates.get(&target).cloned() {
                    let ItemKind::Function(function) = &template.kind else {
                        return Err(error(e.span, "aggregate template is not a callable"));
                    };
                    if let Some(callable) = callable::template(function)? {
                        if explicit.is_some() {
                            return Err(unsupported(
                                e.span,
                                "concrete environment arguments are inferred from the supplied closure",
                            ));
                        }
                        if function.params.len() != actual.len() {
                            return Err(error(e.span, "callable helper arity mismatch"));
                        }
                        if function
                            .generics
                            .iter()
                            .any(|p| !callable.contains_generic(&p.name))
                        {
                            let variables: BTreeSet<_> =
                                type_parameters(function).map(|p| p.name.clone()).collect();
                            let mut found = if contextual_constraints {
                                hints
                                    .as_ref()
                                    .expect("contextual call hints")
                                    .context_constraints(&mut self.budget, e.span)?
                            } else {
                                BTreeMap::new()
                            };
                            if let (Some(result), Some(expected)) =
                                (&function.return_type, expected)
                                && (!self.traits.has_associated
                                    || !trait_bounds::associated::contains(
                                        result,
                                        &mut self.budget,
                                    )?)
                            {
                                let expected = self.original_type(expected)?;
                                unify(
                                    result,
                                    &expected,
                                    &variables,
                                    &mut found,
                                    e.span,
                                    &mut self.budget,
                                )?;
                            }
                            for (index, (parameter, actual)) in
                                function.params.iter().zip(actual).enumerate()
                            {
                                if callable.roles.iter().any(|role| {
                                    role.index == index
                                        && !callable::state_type(&parameter.ty, &role.generic_name)
                                }) {
                                    continue;
                                }
                                let actual = actual.ok_or_else(|| {
                                    error(e.span, "ordinary Copy type argument cannot be inferred")
                                })?;
                                let actual = self.original_type(&actual)?;
                                unify_argument(
                                    &parameter.ty,
                                    &actual,
                                    &variables,
                                    &mut found,
                                    e.span,
                                    &mut self.budget,
                                )?;
                            }
                            let types = function
                                .generics
                                .iter()
                                .filter(|p| !callable.contains_generic(&p.name))
                                .map(|p| {
                                    found.get(&p.name).cloned().ok_or_else(|| {
                                        error(
                                            e.span,
                                            format!(
                                                "Copy type parameter {} has no unique constraint",
                                                p.name
                                            ),
                                        )
                                    })
                                })
                                .collect::<R<Vec<_>>>()?;
                            let instance = self.instantiate(&target, types, e.span)?;
                            callee.kind = ExprKind::Name(instance.clone());
                            if callable.owned_state
                                && let (Some(hints), Some(result)) = (&hints, &function.return_type)
                                && let Some(mut result) =
                                    hints.symbolic_result(result, &callable, &mut self.budget)?
                            {
                                self.ty(&mut result, depth + 1)?;
                                self.note_single_state_value_result(&result, &callable)?;
                                return Ok(Some(result));
                            }
                            return self.signature_result(&instance);
                        }
                        if callable.owned_state
                            && let (Some(hints), Some(result)) = (&hints, &function.return_type)
                            && let Some(mut result) =
                                hints.symbolic_result(result, &callable, &mut self.budget)?
                        {
                            self.ty(&mut result, depth + 1)?;
                            self.note_single_state_value_result(&result, &callable)?;
                            return Ok(Some(result));
                        }
                        return self.signature_result(&target);
                    }
                    if value_inferred {
                        let item = self.templates.get(&target).copied().ok_or_else(|| {
                            error(e.span, "inferred ordinary call has no original template")
                        })?;
                        return self.apply_value_store_template(
                            callee, function, item, &target, &actual, e.span,
                        );
                    }
                    let types = if let Some(types) = explicit {
                        types
                    } else {
                        if function.params.len() != actual.len() {
                            return Err(error(e.span, "generic call arity mismatch"));
                        }
                        let variables = type_parameters(function).map(|p| p.name.clone()).collect();
                        let mut found = if contextual_constraints {
                            hints
                                .as_ref()
                                .expect("contextual call hints")
                                .context_constraints(&mut self.budget, e.span)?
                        } else {
                            BTreeMap::new()
                        };
                        if let (Some(result), Some(expected)) = (&function.return_type, expected)
                            && (!self.traits.has_associated
                                || !trait_bounds::associated::contains(result, &mut self.budget)?)
                        {
                            let expected = self.original_type(expected)?;
                            unify(
                                result,
                                &expected,
                                &variables,
                                &mut found,
                                e.span,
                                &mut self.budget,
                            )?;
                        }
                        for (p, actual) in function.params.iter().zip(actual) {
                            let actual = actual.ok_or_else(|| {
                                error(
                                    e.span,
                                    "generic argument cannot be inferred; use explicit ::<...>",
                                )
                            })?;
                            let actual = self.original_type(&actual)?;
                            unify_argument(
                                &p.ty,
                                &actual,
                                &variables,
                                &mut found,
                                e.span,
                                &mut self.budget,
                            )?;
                        }
                        type_parameters(function)
                            .map(|p| {
                                found.get(&p.name).cloned().ok_or_else(|| {
                                    error(
                                        e.span,
                                        format!(
                                            "type parameter {} has no unique constraint",
                                            p.name
                                        ),
                                    )
                                })
                            })
                            .collect::<R<Vec<_>>>()?
                    };
                    match self.instantiate_inner(&target, types, e.span, true)? {
                        store_templates::Instantiation::Eager(instance) => {
                            callee.kind = ExprKind::Name(instance.clone());
                            self.signature_result(&instance)?
                        }
                        store_templates::Instantiation::Deferred { template, element } => {
                            self.apply_store_template(callee, function, template, element)?
                        }
                    }
                } else {
                    if explicit.is_some() {
                        return Err(error(
                            e.span,
                            "type arguments supplied to a non-generic function",
                        ));
                    }
                    self.signature_result(&target)?
                }
            } else {
                let callee_type = self.expr(callee, env, depth + 1)?;
                if let Some(mut signature) = trait_signature {
                    if let Some(result) = &mut signature.return_type {
                        self.ty(result, depth + 1)?;
                    }
                    signature.return_type
                } else {
                    // A dynamic call still has a statically known complete
                    // signature. Preserve its result for following source
                    // patterns; Types checks the actual call independently.
                    match callee_type.as_ref().map(grouped).map(|ty| &ty.kind) {
                        Some(TypeKind::Function { result, .. }) => {
                            self.budget.ty(result)?;
                            Some(*result.clone())
                        }
                        _ => None,
                    }
                }
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
