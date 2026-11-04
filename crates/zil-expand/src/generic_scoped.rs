//! Symbolic checking for the structured foreign callback primitive.
use super::*;
impl Check<'_, '_, '_> {
    pub(super) fn registration_call(
        &mut self,
        name: &str,
        arguments: &[Argument],
        state: &mut State,
        span: Span,
    ) -> R<TypeRef> {
        let asynchronous = name == "c_register_async" || name.starts_with("async_registration_");
        if matches!(name, "c_register" | "c_register_async") {
            let (providers, callback): (Vec<_>, _) = match arguments {
                [register, close, callback] if !asynchronous => (
                    vec![
                        (register, zil_ir::ScopedCallbackRole::Register),
                        (close, zil_ir::ScopedCallbackRole::Close),
                    ],
                    callback,
                ),
                [register, close, drain, callback] if asynchronous => (
                    vec![
                        (register, zil_ir::ScopedCallbackRole::AsyncRegister),
                        (close, zil_ir::ScopedCallbackRole::AsyncRequestClose),
                        (drain, zil_ir::ScopedCallbackRole::AsyncPollDrain),
                    ],
                    callback,
                ),
                _ => {
                    return Err(error(
                        span,
                        "registration constructor requires ordered actual providers and F",
                    ));
                }
            };
            if !self.unsafe_scope || arguments.iter().any(|a| a.spread) {
                return Err(error(
                    span,
                    "c_register requires explicit unsafe and ordered arguments",
                ));
            }
            for (argument, role) in providers {
                self.tick(argument.span)?;
                let name = super::super::super::foreign::bare_name(&argument.value)
                    .filter(|n| !state.locals.contains_key(*n))
                    .ok_or_else(|| {
                        error(
                            argument.span,
                            "registration roles require actual static foreign symbols",
                        )
                    })?;
                self.budget.text(name, argument.span)?;
                let (_, function) = self
                    .foreign_signatures
                    .get(name)
                    .ok_or_else(|| error(argument.span, "registration foreign role absent"))?;
                for clause in &function.contract {
                    self.budget.node(clause.span, 0)?;
                    for term in &clause.terms {
                        for part in &term.path {
                            self.budget.text(part, clause.span)?;
                        }
                    }
                }
                let (_, static_callbacks, actual) =
                    super::super::super::foreign::noescape_contract(function)?;
                if actual != Some(role) || !static_callbacks {
                    return Err(error(
                        argument.span,
                        "registration role differs from actual foreign declaration",
                    ));
                }
            }
            let ty = self.expr(&callback.value, state, None, false)?;
            let bound = self
                .callable
                .as_ref()
                .and_then(|b| {
                    b.bindings()
                        .find(|b| callable::parameter_mode(&ty, &b.generic_name) == Some(None))
                })
                .ok_or_else(|| {
                    error(
                        callback.span,
                        "generic registration requires its actual declared concrete callable input",
                    )
                })?;
            if !bound.closed_environment
                || bound.kind != zil_ir::CallableKind::FnMut
                || bound.parameters.len() != 1
            {
                return Err(error(
                    callback.span,
                    "registration requires explicit FnMut(i32)->i32 + ClosedEnvironment",
                ));
            }
            same(&bound.parameters[0], &named("i32", span), span, self.budget)?;
            same(&bound.result, &named("i32", span), span, self.budget)?;
            self.budget.ty(&ty)?;
            Ok(TypeRef {
                span,
                kind: TypeKind::Named {
                    path: vec![if asynchronous {
                        "AsyncRegistration".into()
                    } else {
                        "Registration".into()
                    }],
                    args: vec![GenericArg::Type(ty)],
                },
            })
        } else {
            let [argument] = arguments else {
                return Err(error(
                    span,
                    "registration operation requires one actual carrier",
                ));
            };
            if argument.spread {
                return Err(error(span, "registration operation does not accept spread"));
            }
            if asynchronous
                && matches!(
                    name,
                    "async_registration_request_close" | "async_registration_poll_close"
                )
            {
                let ExprKind::Unary {
                    op: UnaryOp::BorrowMut,
                    operand,
                } = &argument.value.kind
                else {
                    return Err(error(
                        span,
                        "async request/poll require exclusive complete local owning head",
                    ));
                };
                self.tick(argument.span)?;
                let ty = self.expr(operand, state, None, true)?;
                if callable::registration_environment(&ty).is_none()
                    || !matches!(&group(&ty).kind,TypeKind::Named{path,..} if path.as_slice()==["AsyncRegistration"])
                {
                    return Err(error(span, "async close requires actual pollable owner"));
                }
                let ExprKind::Name(local) = &operand.kind else {
                    return Err(error(
                        span,
                        "async close requires complete local owning head",
                    ));
                };
                if !state.locals.get(local).is_some_and(|l| l.owned_resource) {
                    return Err(error(span, "async close cannot advance a descriptor"));
                }
                return Ok(if name == "async_registration_poll_close" {
                    TypeRef {
                        span,
                        kind: TypeKind::Named {
                            path: vec!["std".into(), "task".into(), "Poll".into()],
                            args: vec![GenericArg::Type(unit(span))],
                        },
                    }
                } else {
                    unit(span)
                });
            }
            let owns = self.owned_resource_expression(&argument.value, state)?;
            let valid = matches!(name, "registration_valid" | "async_registration_valid");
            let ty = self.expr(&argument.value, state, None, valid)?;
            if matches!(&group(&ty).kind,TypeKind::Named{path,..} if (path.as_slice()==["AsyncRegistration"]) != asynchronous)
            {
                return Err(error(
                    span,
                    "sync and pollable registration carriers are distinct",
                ));
            }
            if callable::registration_environment(&ty).is_none() || (!valid && !owns) {
                return Err(error(
                    span,
                    "registration validity reads descriptor; close requires actual give owner",
                ));
            }
            Ok(if valid {
                named("bool", span)
            } else {
                unit(span)
            })
        }
    }

    pub(super) fn scoped_copy_local(&mut self, id: usize, span: Span) -> R<bool> {
        if self.scoped_copy_closures.is_empty() {
            return Ok(false);
        }
        self.tick(span)?;
        Ok(self
            .factory_values
            .get(&id)
            .is_some_and(|origin| self.scoped_copy_closures.contains(origin)))
    }
    /// Admission is tied to a lexical body source. Concrete provenance and all
    /// other uses are checked again after specialization by the helper kernel.
    pub(super) fn scoped_body_closure(&mut self, span: Span) -> R<bool> {
        if self.scoped_body_closures.is_none() {
            let mut candidates = BTreeSet::new();
            match self.scoped_source {
                FunctionBody::Block(block) => {
                    self.scoped_body_sources(block, &BTreeMap::new(), &mut candidates, 0)?
                }
                FunctionBody::Expression(expression) => {
                    self.scoped_body_source(expression, &BTreeMap::new(), &mut candidates)?
                }
                FunctionBody::Declaration => {}
            }
            self.scoped_body_closures = Some(candidates);
        }
        self.tick(span)?;
        Ok(self.scoped_body_closures.as_ref().unwrap().contains(&span))
    }
    fn scoped_body_source(
        &mut self,
        expression: &Expr,
        bindings: &BTreeMap<String, Span>,
        candidates: &mut BTreeSet<Span>,
    ) -> R<()> {
        self.tick(expression.span)?;
        if let ExprKind::Call {
            callee, arguments, ..
        } = &expression.kind
            && super::super::super::foreign::bare_name(callee) == Some("c_with_callback")
            && let Some(body) = arguments.get(3)
        {
            self.tick(body.span)?;
            let source = match &body.value.kind {
                ExprKind::Closure { .. } => Some(body.value.span),
                ExprKind::Name(name) => {
                    self.budget.text(name, body.span)?;
                    bindings.get(name).copied()
                }
                _ => None,
            };
            if let Some(source) = source {
                candidates.insert(source);
            }
        }
        Ok(())
    }
    fn scoped_body_sources(
        &mut self,
        block: &Block,
        outer: &BTreeMap<String, Span>,
        candidates: &mut BTreeSet<Span>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(block.span, depth)?;
        for name in outer.keys() {
            self.budget.text(name, block.span)?;
        }
        let mut bindings = outer.clone();
        for statement in &block.statements {
            self.budget.node(statement.span, depth + 1)?;
            match &statement.kind {
                StmtKind::Binding { pattern, value, .. } => {
                    self.scoped_body_source(value, &bindings, candidates)?;
                    if let PatternKind::Name(name) = &pattern.kind {
                        self.budget.text(name, pattern.span)?;
                        if matches!(value.kind, ExprKind::Closure { .. }) {
                            bindings.insert(name.clone(), value.span);
                        } else {
                            bindings.remove(name);
                        }
                    }
                }
                StmtKind::Return(Some(value)) | StmtKind::Expression(value) => {
                    self.scoped_body_source(value, &bindings, candidates)?
                }
                StmtKind::Unsafe(body)
                | StmtKind::Loop(body)
                | StmtKind::While { body, .. }
                | StmtKind::For { body, .. } => {
                    self.scoped_body_sources(body, &bindings, candidates, depth + 1)?
                }
                StmtKind::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.scoped_body_sources(then_body, &bindings, candidates, depth + 1)?;
                    if let Some(ElseBranch::Block(body)) = else_body {
                        self.scoped_body_sources(body, &bindings, candidates, depth + 1)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(super) fn scoped_callback_block(&mut self, block: &Block, depth: usize) -> R<bool> {
        self.budget.node(block.span, depth)?;
        for statement in &block.statements {
            self.budget.node(statement.span, depth + 1)?;
            match &statement.kind {
                StmtKind::Return(Some(e))
                | StmtKind::Expression(e)
                | StmtKind::Binding { value: e, .. }
                    if matches!(&e.kind,ExprKind::Call{callee,..} if matches!(super::super::super::foreign::bare_name(callee),Some("c_with_callback"|"c_register"))) =>
                {
                    return Ok(true);
                }
                StmtKind::Unsafe(body)
                | StmtKind::Loop(body)
                | StmtKind::While { body, .. }
                | StmtKind::For { body, .. } => {
                    if self.scoped_callback_block(body, depth + 1)? {
                        return Ok(true);
                    }
                }
                StmtKind::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    if self.scoped_callback_block(then_body, depth + 1)? {
                        return Ok(true);
                    }
                    if let Some(ElseBranch::Block(body)) = else_body
                        && self.scoped_callback_block(body, depth + 1)?
                    {
                        return Ok(true);
                    }
                }
                _ => {}
            }
        }
        Ok(false)
    }
    pub(super) fn scoped_callback_call(
        &mut self,
        arguments: &[Argument],
        state: &mut State,
        span: Span,
    ) -> R<TypeRef> {
        let [register, close, callback, body] = arguments else {
            return Err(error(
                span,
                "c_with_callback requires four ordered arguments",
            ));
        };
        if arguments.iter().any(|a| a.spread) {
            return Err(error(
                span,
                "c_with_callback does not accept spread arguments",
            ));
        }
        if !self.unsafe_scope {
            return Err(error(
                span,
                "c_with_callback requires an explicit unsafe block",
            ));
        }
        for (argument, role) in [
            (register, zil_ir::ScopedCallbackRole::Register),
            (close, zil_ir::ScopedCallbackRole::Close),
        ] {
            self.tick(argument.span)?;
            let name = super::super::super::foreign::bare_name(&argument.value)
                .filter(|n| !state.locals.contains_key(*n))
                .ok_or_else(|| {
                    error(
                        argument.span,
                        "scoped callback role requires a static foreign declaration symbol",
                    )
                })?;
            self.budget.text(name, argument.span)?;
            let (_, function) = self.foreign_signatures.get(name).ok_or_else(|| {
                error(
                    argument.span,
                    "scoped callback role has no actual foreign declaration",
                )
            })?;
            for clause in &function.contract {
                self.budget.node(clause.span, 0)?;
                for term in &clause.terms {
                    for part in &term.path {
                        self.budget.text(part, clause.span)?;
                    }
                }
            }
            let (_, static_callbacks, actual) =
                super::super::super::foreign::noescape_contract(function)?;
            if actual != Some(role) || !static_callbacks {
                return Err(error(
                    argument.span,
                    "scoped callback role differs from its actual foreign declaration",
                ));
            }
        }
        let callback_type = self.expr(&callback.value, state, None, true)?;
        let TypeKind::Reference {
            mutable: true,
            inner,
            ..
        } = &group(&callback_type).kind
        else {
            return Err(error(
                callback.span,
                "scoped callback requires an exclusive callable address",
            ));
        };
        let bound = self
            .callable
            .as_ref()
            .and_then(|bounds| {
                bounds.bindings().find(|bound| {
                    callable::parameter_mode(inner, &bound.generic_name) == Some(None)
                })
            })
            .ok_or_else(|| {
                error(
                    callback.span,
                    "scoped callback must have a declared callable bound",
                )
            })?;
        if !matches!(
            bound.kind,
            zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut
        ) || bound.parameters.len() != 1
        {
            return Err(error(
                callback.span,
                "scoped callback requires Fn or FnMut(i32)->i32",
            ));
        }
        same(&bound.parameters[0], &named("i32", span), span, self.budget)?;
        same(&bound.result, &named("i32", span), span, self.budget)?;
        let body_type = self.expr(&body.value, state, None, false)?;
        if let Some((_, parameters, result)) = callable::opaque_signature(&body_type) {
            if !parameters.is_empty() {
                return Err(error(body.span, "scoped body requires ()->i32"));
            }
            same(result, &named("i32", span), span, self.budget)?;
            return Ok(TypeRef {
                span,
                kind: TypeKind::Tuple(vec![named("bool", span), named("i32", span)]),
            });
        }
        let bound = self
            .callable
            .as_ref()
            .and_then(|bounds| {
                bounds.bindings().find(|bound| {
                    callable::parameter_mode(&body_type, &bound.generic_name) == Some(None)
                })
            })
            .ok_or_else(|| error(body.span, "scoped body must have an owned callable bound"))?;
        if !bound.parameters.is_empty() {
            return Err(error(body.span, "scoped body requires ()->i32"));
        }
        same(&bound.result, &named("i32", span), span, self.budget)?;
        Ok(TypeRef {
            span,
            kind: TypeKind::Tuple(vec![named("bool", span), named("i32", span)]),
        })
    }
}
