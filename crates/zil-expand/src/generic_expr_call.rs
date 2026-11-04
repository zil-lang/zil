//! Sequential call phases keep receiver lookup and completed instantiation
//! locals out of the recursive argument-typing host frame. Work order is fixed.
use super::expr_variants::ExprExpectedContext;
use super::*;
struct CallMethod {
    signature: Option<Function>,
    type_qualified: bool,
}
struct CallArguments {
    method: CallMethod,
    hints: Option<enums::CallHints>,
    actual: Vec<Option<TypeRef>>,
    contextual_constraints: bool,
    value_inference_allowed: bool,
    value_inferred: bool,
}
impl Engine<'_> {
    fn expr_call_method(&mut self, e: &mut Expr, env: &mut Env, depth: usize) -> R<CallMethod> {
        self.method_call(e, env, depth)?;
        let ExprKind::Call { callee, .. } = &mut e.kind else {
            unreachable!("call dispatch")
        };
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
                    && let Some((target, signature)) =
                        self.traits
                            .concrete_method(host, member, &mut self.budget, callee.span)?
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

        Ok(CallMethod {
            signature: trait_signature,
            type_qualified,
        })
    }
    fn expr_call_primitive(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<Option<TypeRef>>> {
        let ExprKind::Call { callee, arguments } = &mut e.kind else {
            unreachable!("call dispatch")
        };
        if let Some(name)=crate::foreign::bare_name(callee).filter(|name| matches!(*name,"task_create"|"task_poll"|"task_poll_close"|"task_request_cancel"|"task_finish_destroy"|"task_valid")) {
            let name=name.to_owned();
            if name=="task_create" {
                let Some(first)=arguments.first() else {return Err(error(e.span,"task_create requires a static worker"));};
                let worker=stores::call_name(&first.value).ok_or_else(||error(first.span,"task_create worker must be static"))?;
                let signature=self.signatures.get(worker).ok_or_else(||error(first.span,"task_create worker is absent"))?;
                self.budget.const_function(signature)?;
                let signature=(*signature).clone();
                if arguments.len()!=signature.params.len()+1 {return Err(error(e.span,"task_create argument count differs from worker"));}
                for (argument,parameter) in arguments[1..].iter_mut().zip(&signature.params) {self.expr_expected(&mut argument.value,env,Some(&parameter.ty),depth+1)?;}
                let result=signature.return_type.clone().unwrap_or_else(||named("void",e.span));
                let mut result=TypeRef {span:e.span,kind:TypeKind::Error {error:Box::new(named(options::TASK_CREATE_ERROR,e.span)),result:Box::new(TypeRef {span:e.span,kind:TypeKind::Named {path:vec!["Task".into()],args:vec![GenericArg::Type(result)]}})}};
                self.ty(&mut result,depth+1)?;
                return Ok(Some(Some(result)));
            }
            if arguments.len()==1 {
                if let Some(actual)=self.expr_expected(&mut arguments[0].value,env,None,depth+1)? {
                    let mut task=&actual;
                    while let TypeKind::Reference{inner,..}|TypeKind::Group(inner)=&task.kind {task=inner;}
                    if let TypeKind::Named{path,args}=&task.kind && path.as_slice()==["Task"] && let [GenericArg::Type(payload)]=args.as_slice() {
                        self.budget.ty(payload)?;
                        let mut result=match name.as_str() {
                            "task_poll"=>TypeRef {span:e.span,kind:TypeKind::Error {error:Box::new(named(options::TASK_POLL_STATE,e.span)),result:Box::new(TypeRef {span:e.span,kind:TypeKind::Named {path:vec![options::TASK_POLL.into()],args:vec![GenericArg::Type(payload.clone())]}})}},
                            "task_poll_close"=>TypeRef {span:e.span,kind:TypeKind::Named {path:vec![options::TASK_POLL.into()],args:vec![GenericArg::Type(named("void",e.span))]}},
                            "task_valid"=>named("bool",e.span),
                            _=>named("void",e.span),
                        };
                        self.ty(&mut result,depth+1)?;
                        return Ok(Some(Some(result)));
                    }
                }
            }
            // Compatibility TaskI32 calls retain the existing signature path.
            return Ok(Some(None));
        }
        if let Some(name) = crate::foreign::bare_name(callee).filter(|name| stores::primitive(name))
        {
            return self.expr_call_store(arguments, e.span, env, context, name, depth);
        }
        if let Some((parameters, result)) =
            crate::foreign::bare_name(callee).and_then(|name| bytes::signature(name, e.span))
        {
            return self.expr_call_bytes(e, env, context, parameters, result, depth);
        }
        if let Some(value) = crate::foreign::literal_constructor(callee, arguments, e.span) {
            let value = value?;
            if let zil_ir::Constant::WideString(units) = &value {
                self.budget.wide_units(units, e.span)?;
            }
            return Ok(Some(Some(named(&value.ty().name(), e.span))));
        }
        if crate::foreign::bare_name(callee) == Some("c_callback") {
            return self.expr_call_callback(e, env, context, depth);
        }
        if let Some(name) = crate::integer_conversion_name(callee) {
            return self.expr_call_conversion(arguments, e.span, env, name, depth);
        }

        Ok(None)
    }
    pub(super) fn expr_call_normal(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let mut state = CallArguments {
            method: self.expr_call_method(e, env, depth)?,
            hints: None,
            actual: Vec::new(),
            contextual_constraints: false,
            value_inference_allowed: false,
            value_inferred: false,
        };
        if let Some(result) = self.expr_call_primitive(e, env, context, depth)? {
            return Ok(result);
        }
        let ExprExpectedContext {
            expected,
            store_success_context,
            ..
        } = context;
        let ExprKind::Call { callee, .. } = &mut e.kind else {
            unreachable!("call dispatch")
        };
        let expected = expected.or_else(|| {
            let result = stores::call_name(callee)
                .and_then(|name| self.signatures.get(name))?
                .return_type
                .as_ref()?;
            stores::call_context(result, store_success_context?)
        });

        let context = ExprExpectedContext {
            expected,
            ..context
        };
        self.expr_call_arguments(e, env, context, &mut state, depth)?;
        self.expr_call_finish(e, env, context, state, depth)
    }
    fn expr_call_arguments(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        state: &mut CallArguments,
        depth: usize,
    ) -> R<()> {
        let ExprExpectedContext { expected, .. } = context;
        let ExprKind::Call { callee, arguments } = &mut e.kind else {
            unreachable!("call dispatch")
        };
        state.value_inference_allowed = matches!(&callee.kind, ExprKind::Name(name) if !env.contains_key(name) && self.signatures.get(name).is_some_and(|f|store_templates::parameter(f).is_some()));
        state.hints = self.call_hints(callee, env, expected)?;
        for index in 0..arguments.len() {
            let hint = self.expr_call_argument_hint(arguments, env, state, index, depth)?;
            let arg = &mut arguments[index];
            let ty = self.expr_expected(&mut arg.value, env, hint.as_ref(), depth + 1)?;

            self.expr_call_argument_result(arg, env, (hint.as_ref(), ty), state, index, depth)?;
        }
        Ok(())
    }
    fn expr_call_argument_hint(
        &mut self,
        arguments: &[Argument],
        env: &mut Env,
        state: &mut CallArguments,
        index: usize,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let mut hint = state
            .hints
            .as_ref()
            .map(|h| h.hint(index, &mut self.budget))
            .transpose()?
            .flatten();
        if hint.is_none()
            && state
                .hints
                .as_ref()
                .map(|h| h.unresolved_context(index, &mut self.budget))
                .transpose()?
                .unwrap_or(false)
        {
            state.contextual_constraints = true;
            for union_pass in [false, true] {
                for (other_index, argument) in arguments.iter().enumerate() {
                    self.budget.node(argument.span, depth + 1)?;
                    if !state
                        .hints
                        .as_ref()
                        .expect("contextual call hints")
                        .independent_input(other_index, union_pass, &mut self.budget)?
                    {
                        continue;
                    }
                    if let Some(ty) = self.value_hint(&argument.value, env, depth + 1, true)? {
                        let ty = self.original_type(&ty)?;
                        state
                            .hints
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
            hint = state
                .hints
                .as_ref()
                .map(|h| h.hint(index, &mut self.budget))
                .transpose()?
                .flatten();
        }
        if hint.is_none()
            && let Some(signature) = &state.method.signature
        {
            let skip = usize::from(
                !state.method.type_qualified
                    && signature.params.first().is_some_and(|p| p.name == "self"),
            );
            if let Some(parameter) = signature.params.get(index + skip) {
                self.budget.ty(&parameter.ty)?;
                hint = Some(parameter.ty.clone());
            }
        }

        Ok(hint)
    }
    fn expr_call_argument_result(
        &mut self,
        arg: &Argument,
        env: &mut Env,
        typed_argument: (Option<&TypeRef>, Option<TypeRef>),
        state: &mut CallArguments,
        index: usize,
        depth: usize,
    ) -> R<()> {
        let (hint, ty) = typed_argument;
        let ty = if ty.is_none()
            && hint
                .as_ref()
                .is_some_and(|ty| matches!(grouped(ty).kind, TypeKind::Error { .. }))
        {
            self.value_hint(&arg.value, env, depth + 1, true)?
        } else {
            ty
        };
        if state.value_inference_allowed
            && let Some(ty) = &ty
            && self.inferred_state_value(ty, 0)?
        {
            state.value_inferred = true;
            // Concrete C identities belong to Types. A prior
            // symbolic spelling is not a cross-argument equality
            // constraint; the new static Call checks all real T's.
            state.hints = None;
        }
        if let (Some(hints), Some(ty)) = (&mut state.hints, &ty) {
            let ty = self.original_type(ty)?;
            hints.constrain(index, &ty, arg.span, &mut self.budget)?;
        }
        state.actual.push(ty);

        Ok(())
    }
    fn expr_call_finish(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        state: CallArguments,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let ExprExpectedContext { expected, .. } = context;
        let CallArguments {
            method:
                CallMethod {
                    signature: trait_signature,
                    ..
                },
            hints,
            actual,
            mut contextual_constraints,
            value_inferred,
            ..
        } = state;
        let ExprKind::Call { callee, .. } = &mut e.kind else {
            unreachable!("call dispatch")
        };
        let result = {
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
                        let parameter = declaration.and_then(|item| {
                            params(item)
                                .iter()
                                .filter(|parameter| {
                                    !matches!(parameter.kind, GenericParamKind::Lifetime)
                                })
                                .nth(index)
                        });
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
                    let provider_prefix = if let ExprKind::Name(name) = &base.kind {
                        self.provider(name, e.span)?
                            && declaration.is_some_and(|item| {
                                types.len()
                                    < params(item)
                                        .iter()
                                        .filter(|p| !matches!(p.kind, GenericParamKind::Lifetime))
                                        .count()
                            })
                    } else {
                        false
                    };
                    explicit = (!provider_prefix).then_some(types);
                    contextual_constraints |= provider_prefix;
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
                    if let Some(callable) = callable::template(function, &mut self.budget)? {
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
                        consts::parameters(function)
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
    fn expr_call_store(
        &mut self,
        arguments: &mut [Argument],
        span: Span,
        env: &mut Env,
        context: ExprExpectedContext<'_>,
        name: &str,
        depth: usize,
    ) -> R<Option<Option<TypeRef>>> {
        let ExprExpectedContext {
            expected,
            store_success_context,
            ..
        } = context;

        if arguments.len() != stores::arity(name) || arguments.iter().any(|arg| arg.spread) {
            return Err(error(
                span,
                "Store primitive argument count or shape differs",
            ));
        }
        let element = if name == "store_new" {
            expected
                .or(store_success_context.and_then(stores::carrier))
                .and_then(stores::element)
                .cloned()
                .ok_or_else(|| {
                    error(span, "store_new requires an explicit Store element context")
                })?
        } else {
            let store = self
                .expr_expected(&mut arguments[0].value, env, None, depth + 1)?
                .ok_or_else(|| error(span, "Store primitive requires a known Store type"))?;
            stores::element(&store)
                .cloned()
                .ok_or_else(|| error(span, "Store primitive requires a Store description"))?
        };
        self.budget.ty(&element)?;
        if arguments.len() == 2 {
            let wanted = if name == "store_push" {
                element.clone()
            } else {
                named("i64", span)
            };
            self.expr_expected(&mut arguments[1].value, env, Some(&wanted), depth + 1)?;
        }
        Ok(Some(Some(stores::result(name, &element, span))))
    }
    fn expr_call_bytes(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        parameters: Vec<TypeRef>,
        result: TypeRef,
        depth: usize,
    ) -> R<Option<Option<TypeRef>>> {
        let ExprKind::Call { arguments, .. } = &mut e.kind else {
            unreachable!("call dispatch")
        };

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
        Ok(Some(Some(result)))
    }
    fn expr_call_callback(
        &mut self,
        e: &mut Expr,
        env: &mut Env,
        _context: ExprExpectedContext<'_>,
        _depth: usize,
    ) -> R<Option<Option<TypeRef>>> {
        let ExprKind::Call { arguments, .. } = &mut e.kind else {
            unreachable!("call dispatch")
        };

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
        let Some(target) =
            crate::foreign::bare_name(&argument.value).filter(|name| !env.contains_key(*name))
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
        (crate::foreign::callback_syntax_type(function, e.span).map(Some)).map(Some)
    }
    fn expr_call_conversion(
        &mut self,
        arguments: &mut [Argument],
        span: Span,
        env: &mut Env,
        name: &str,
        depth: usize,
    ) -> R<Option<Option<TypeRef>>> {
        if arguments.len() != 1 || arguments[0].spread {
            return Err(crate::conversion_error(
                span,
                "integer conversion requires exactly one non-spread argument",
            ));
        }
        self.expr(&mut arguments[0].value, env, depth + 1)?;
        Ok(Some(Some(named(name, span))))
    }
}
