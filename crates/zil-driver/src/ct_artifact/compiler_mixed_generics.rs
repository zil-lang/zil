//! Complete explicit actuals for an original free CT factory. The declaration
//! check remains a separate, discarded symbolic CFG; actuals never prove it.
use super::*;
use cfg::{Builder, Value};

impl Compiler<'_> {
    pub(super) fn has_ambient_use(&mut self) -> R<bool> {
        if self.source_instance_calls.is_empty() {
            return Ok(true);
        }
        // Scalar/closed-Type calls can retain a full nested scope chain in the
        // private source ledger without publishing any ambient graph reference.
        // Remove the whole unused inventory together, never relabel a live row.
        self.budget
            .work(self.ambient_life_scopes.len(), Span::default())?;
        if !self.source_life_bindings.is_empty()
            && self
                .ambient_life_scopes
                .iter()
                .any(|scope| matches!(scope.instance, CtScopeInstance::Instantiated(_)))
        {
            return Ok(true);
        }
        for (arguments, context) in self
            .records
            .iter()
            .map(|s| (&s.arguments, &s.life_context))
            .chain(self.enums.iter().map(|s| (&s.arguments, &s.life_context)))
        {
            self.budget.work(arguments.len() + 1, Span::default())?;
            if context
                .as_ref()
                .is_some_and(|graph| !graph.ambient_classes.is_empty())
            {
                return Ok(true);
            }
            if arguments.iter().any(|arg| matches!(arg, CtSchemaArgument::Type(graph) if !graph.ambient_classes.is_empty())) { return Ok(true); }
        }
        for template in &self.templates {
            self.budget
                .work(template.syntax.nodes.len(), Span::default())?;
            for node in &template.syntax.nodes {
                if matches!(node, CtSyntaxNode::Token(CtToken { name: Some(CtIdent { member_owner_type: Some(graph), .. }), .. }) if !graph.ambient_classes.is_empty())
                {
                    return Ok(true);
                }
            }
        }
        if !self.source_type_sites.is_empty() {
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn mixed_parameters(parameters: &[s::GenericParam]) -> bool {
        parameters.iter().any(|p| {
            matches!(
                p.kind,
                s::GenericParamKind::Const(_) | s::GenericParamKind::Lifetime
            )
        })
    }

    pub(super) fn source_call_scope(
        &mut self,
        span: Span,
    ) -> R<(CtAmbientScopeId, CtAmbientLifeScope)> {
        let scope = self
            .source_life_scope
            .as_ref()
            .ok_or_else(|| error(span, "CT factory call lacks authentic caller scope"))?;
        let first = if let Some((name, _)) = scope.binders.first() {
            self.budget.text(name, span)?;
            Some(name.clone())
        } else {
            None
        };
        let existing = if let Some(first) = first {
            Some(self.ambient_actual(&first, span)?.scope)
        } else {
            None
        };
        let scope = self.source_life_scope.as_ref().unwrap();
        self.budget.work(32, span)?;
        if let CtScopeInstance::Instantiated(actuals) = &scope.instance {
            Self::actual_cost(&mut self.budget, actuals, span)?;
        }
        let raw = CtAmbientLifeScope {
            declaration: scope.definition.clone(),
            enclosing: scope.enclosing,
            binder_site: Vec::new(),
            instance: scope.instance.clone(),
        };
        let id = if let Some(id) = existing {
            id
        } else {
            self.budget.work(self.ambient_life_scopes.len(), span)?;
            for old in &self.ambient_life_scopes {
                self.budget.scope_comparison(old, span)?;
            }
            self.budget.scope_comparison(&raw, span)?;
            let index =
                if let Some(index) = self.ambient_life_scopes.iter().position(|old| old == &raw) {
                    index
                } else {
                    self.budget.node(span)?;
                    Budget::take(
                        &mut self.budget.bytes,
                        std::mem::size_of::<CtAmbientLifeScope>(),
                        span,
                        "CT caller context scope bytes exhausted",
                    )?;
                    if let CtScopeInstance::Instantiated(actuals) = &raw.instance {
                        Self::actual_cost(&mut self.budget, actuals, span)?;
                    }
                    self.ambient_life_scopes.push(raw.clone());
                    self.ambient_life_scopes.len() - 1
                };
            CtAmbientScopeId(
                u32::try_from(index)
                    .map_err(|_| error(span, "CT caller context scope ID overflow"))?,
            )
        };
        Ok((id, raw))
    }

    fn mixed_actual_cost(&mut self, actuals: &[CtScopeActual], span: Span) -> R<()> {
        Self::actual_cost(&mut self.budget, actuals, span)
    }

    pub(super) fn actual_cost(
        budget: &mut Budget<'_>,
        actuals: &[CtScopeActual],
        span: Span,
    ) -> R<()> {
        budget.work(actuals.len(), span)?;
        Budget::take(
            &mut budget.bytes,
            actuals
                .len()
                .saturating_mul(std::mem::size_of::<CtScopeActual>()),
            span,
            "CT full actual vector bytes exhausted",
        )?;
        for actual in actuals {
            if let CtScopeActual::Type(graph) = actual {
                budget.metadata(graph, span)?;
            }
        }
        Ok(())
    }

    pub(super) fn ambient_actual(&mut self, name: &str, span: Span) -> R<CtAmbientLifeReference> {
        let scope = self.source_life_scope.as_ref().ok_or_else(|| {
            error(
                span,
                "CT life actual requires an authentic caller life scope",
            )
        })?;
        let mut parameter = None;
        for (bound, ordinal) in &scope.binders {
            self.budget.work(bound.len().min(name.len()) + 1, span)?;
            if bound == name {
                parameter = Some(*ordinal);
            }
        }
        if parameter.is_none() {
            let mut parent = scope.enclosing.or(self.active_host);
            let mut depth = 0usize;
            while let Some(id) = parent {
                self.budget.depth(depth, span)?;
                self.budget.work(32, span)?;
                let host = self
                    .ambient_life_scopes
                    .get(id.0 as usize)
                    .ok_or_else(|| error(span, "CT actual lexical host is outside inventory"))?;
                let definition = host.declaration.clone();
                parent = host.enclosing;
                let (_, item) = self.metadata_declaration(&definition, span)?;
                let parameters = match &item.kind {
                    s::ItemKind::Aggregate(host) => &host.generics,
                    _ => {
                        return Err(error(
                            span,
                            "CT actual lexical host is not its original aggregate",
                        ));
                    }
                };
                for (ordinal, bound) in parameters.iter().enumerate() {
                    self.budget
                        .work(bound.name.len().min(name.len()) + 1, span)?;
                    if bound.name == name {
                        if !matches!(bound.kind, s::GenericParamKind::Lifetime) {
                            return Err(error(
                                span,
                                "CT actual nearest host parameter is not a life",
                            ));
                        }
                        return Ok(CtAmbientLifeReference {
                            scope: id,
                            parameter: u32::try_from(ordinal)
                                .map_err(|_| error(span, "CT actual host ordinal overflow"))?,
                        });
                    }
                }
                depth += 1;
            }
        }
        let parameter = parameter.ok_or_else(|| {
            error(
                span,
                "CT life actual is not declared by its authentic lexical owner",
            )
        })?;
        let definition = scope.definition.clone();
        // Pay nested metadata before copying the actual source scope.
        match &scope.instance {
            CtScopeInstance::Definition => self.budget.work(32, span)?,
            CtScopeInstance::Instantiated(actuals) => {
                Self::actual_cost(&mut self.budget, actuals, span)?;
            }
        }
        let instance = self.source_life_scope.as_ref().unwrap().instance.clone();
        let raw = CtAmbientLifeScope {
            enclosing: scope.enclosing,
            declaration: definition,
            binder_site: Vec::new(),
            instance,
        };
        self.budget
            .work(self.ambient_life_scopes.len().saturating_mul(32), span)?;
        if matches!(raw.instance, CtScopeInstance::Instantiated(_)) {
            for candidate in &self.ambient_life_scopes {
                if candidate.declaration == raw.declaration
                    && matches!(candidate.instance, CtScopeInstance::Instantiated(_))
                {
                    self.budget.scope_comparison(candidate, span)?;
                    self.budget.scope_comparison(&raw, span)?;
                }
            }
        }
        let scope = if let Some(id) = self.ambient_life_scopes.iter().position(|s| s == &raw) {
            id
        } else {
            self.budget.node(span)?;
            Budget::take(
                &mut self.budget.bytes,
                std::mem::size_of::<CtAmbientLifeScope>(),
                span,
                "CT caller scope table bytes exhausted",
            )?;
            self.ambient_life_scopes.push(raw);
            self.ambient_life_scopes.len() - 1
        };
        Ok(CtAmbientLifeReference {
            scope: CtAmbientScopeId(
                u32::try_from(scope).map_err(|_| error(span, "CT caller scope ID overflow"))?,
            ),
            parameter,
        })
    }

    pub(super) fn bind_mixed_instance(
        &mut self,
        declaration: usize,
        actuals: &[CtScopeActual],
    ) -> R<()> {
        let d = self.declarations[declaration];
        if d.function.generics.len() != actuals.len() {
            return Err(error(d.item.span, "CT full actual vector arity differs"));
        }
        self.bindings.clear();
        self.metadata_bindings.clear();
        self.scalar_bindings.clear();
        self.bind_host_instance(declaration)?;
        for (parameter, actual) in d.function.generics.iter().zip(actuals) {
            self.budget.text(&parameter.name, parameter.span)?;
            match (&parameter.kind, actual) {
                (s::GenericParamKind::Type(_), CtScopeActual::Type(graph)) => {
                    self.generic_names(std::slice::from_ref(parameter), &[], parameter.span)?;
                    self.metadata_copy(graph, 0, parameter.span)?;
                    self.budget.metadata(graph, parameter.span)?;
                    self.metadata_bindings
                        .push((parameter.name.clone(), graph.clone()));
                }
                (s::GenericParamKind::Const(ty), CtScopeActual::Scalar(value)) => {
                    let domain = self.scalar_argument_type(ty, parameter.span)?;
                    if expr::scalar_constant(value) != CtTy::Scalar(domain) {
                        return Err(error(
                            parameter.span,
                            "CT full scalar actual domain differs",
                        ));
                    }
                    self.budget.work(32, parameter.span)?;
                    self.scalar_bindings
                        .push((parameter.name.clone(), value.clone()));
                }
                (s::GenericParamKind::Lifetime, CtScopeActual::Lifetime(reference)) => {
                    let caller = self
                        .ambient_life_scopes
                        .get(reference.scope.0 as usize)
                        .ok_or_else(|| {
                            error(parameter.span, "CT full life actual has no caller scope")
                        })?;
                    if caller.declaration.generation_path.is_some() {
                        return Err(error(
                            parameter.span,
                            "CT full life actual has generated caller",
                        ));
                    }
                }
                _ => {
                    return Err(error(
                        parameter.span,
                        "CT full actual kind differs from original binder",
                    ));
                }
            }
        }
        let definition = self.item_symbol(d.module, d.item)?;
        self.source_life_scope = self.source_life_scope(d.module, &definition, d.function.span)?;
        self.mixed_actual_cost(actuals, d.item.span)?;
        self.source_life_scope
            .as_mut()
            .ok_or_else(|| error(d.item.span, "CT factory has no original life binder"))?
            .instance = CtScopeInstance::Instantiated(actuals.to_vec());
        Ok(())
    }

    fn mixed_header(
        &mut self,
        declaration: usize,
        actuals: &[CtScopeActual],
    ) -> R<(Vec<CtTy>, CtTy)> {
        let data = std::mem::take(&mut self.bindings);
        let meta = std::mem::take(&mut self.metadata_bindings);
        let scalar = std::mem::take(&mut self.scalar_bindings);
        let scope = self.source_life_scope.take();
        let source_function = self.current_source_function.take();
        let result = self
            .bind_mixed_instance(declaration, actuals)
            .and_then(|()| self.header(declaration));
        self.bindings = data;
        self.metadata_bindings = meta;
        self.scalar_bindings = scalar;
        self.source_life_scope = scope;
        self.current_source_function = source_function;
        result
    }

    fn enqueue_mixed_instance(
        &mut self,
        declaration: usize,
        actuals: &[CtScopeActual],
    ) -> R<CtFunctionId> {
        let span = self.declarations[declaration].item.span;
        self.ensure_generic_definition(declaration)?;
        for i in 0..self.mixed_selected.len() {
            self.budget.work(32, span)?;
            self.mixed_actual_cost(actuals, span)?;
            Self::actual_cost(&mut self.budget, &self.mixed_selected[i].1, span)?;
            let (old, arguments, enclosing, function) = &self.mixed_selected[i];
            if *old == declaration && arguments == actuals && *enclosing == self.active_host {
                return Ok(*function);
            }
        }
        self.budget.node(span)?;
        Budget::take(
            &mut self.budget.functions,
            1,
            span,
            "CT full instance budget exhausted",
        )?;
        self.mixed_actual_cost(actuals, span)?;
        self.mixed_actual_cost(actuals, span)?;
        let id = CtFunctionId(self.functions.len() as u32);
        self.functions.push(None);
        self.mixed_selected
            .push((declaration, actuals.to_vec(), self.active_host, id));
        self.pending.push_back((
            declaration,
            PendingTypes::Mixed(actuals.to_vec(), self.active_host),
            id,
        ));
        Ok(id)
    }
}

impl Builder<'_, '_> {
    pub(super) fn contextual_metadata_call(
        &mut self,
        declaration: usize,
        graphs: Vec<CtTypeRef>,
        values: Vec<CtValueId>,
        actual_types: &[CtTy],
        expected: Option<&CtTy>,
        span: Span,
    ) -> R<Value> {
        let d = self.compiler.declarations[declaration];
        let (caller_scope_id, caller_scope) = self.compiler.source_call_scope(span)?;
        Budget::take(
            &mut self.compiler.budget.bytes,
            graphs
                .len()
                .saturating_mul(std::mem::size_of::<CtScopeActual>()),
            span,
            "CT contextual Type actual vector backing exhausted",
        )?;
        let actuals = graphs
            .into_iter()
            .map(CtScopeActual::Type)
            .collect::<Vec<_>>();
        let (parameters, result) = self.compiler.mixed_header(declaration, &actuals)?;
        for (actual, expected) in actual_types.iter().zip(&parameters) {
            self.compiler.budget.ty(actual, 0, span)?;
            self.compiler.budget.ty(expected, 0, span)?;
            if actual != expected {
                return Err(error(span, "CT contextual generic argument type differs"));
            }
        }
        let function = self
            .compiler
            .enqueue_mixed_instance(declaration, &actuals)?;
        self.compiler.mixed_actual_cost(&actuals, span)?;
        self.compiler.budget.node(span)?;
        Budget::take(
            &mut self.compiler.budget.bytes,
            std::mem::size_of::<SourceInstanceCall>(),
            span,
            "CT contextual call source ledger exhausted",
        )?;
        let callee = self.compiler.item_symbol(d.module, d.item)?;
        let site = self.compiler.current_expression_site(span)?;
        self.compiler
            .source_instance_calls
            .push(SourceInstanceCall {
                caller: self.function_id(),
                caller_definition: self.definition.clone(),
                caller_scope,
                caller_scope_id,
                callee,
                instance: function,
                actuals,
                enclosing: self.compiler.active_host,
                site,
            });
        if result == CtTy::Scalar(CtScalarTy::Unit) {
            self.op(
                CtInstructionKind::Call {
                    function,
                    arguments: values,
                },
                span,
            )?;
            self.constant(CtConstant::Scalar(Constant::Unit), result, span)
        } else {
            let value = self.emit(
                CtInstructionKind::Call {
                    function,
                    arguments: values,
                },
                result,
                span,
            )?;
            self.relocate_record_result(value, expected, span)
        }
    }

    pub(super) fn mixed_generic_call(
        &mut self,
        declaration: usize,
        explicit: Option<&[s::GenericArg]>,
        arguments: &[s::Argument],
        expected: Option<&CtTy>,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        let d = self.compiler.declarations[declaration];
        let explicit = match explicit {
            Some(actuals) => actuals,
            None if d.function.generics.is_empty() => &[],
            None => {
                return Err(error(
                    span,
                    "CT full instance requires all explicit actuals",
                ));
            }
        };
        if explicit.len() != d.function.generics.len() || arguments.len() != d.function.params.len()
        {
            return Err(error(span, "CT full instance actual arity differs"));
        }
        // item_symbol uses actual immutable AST placement, never Span/name.
        self.compiler.item_symbol(d.module, d.item)?;
        self.compiler.ensure_generic_definition(declaration)?;
        let (caller_scope_id, caller_scope) = self.compiler.source_call_scope(span)?;
        let mut actuals = Vec::new();
        for (parameter, actual) in d.function.generics.iter().zip(explicit) {
            self.compiler.budget.node(span)?;
            actuals.push(match (&parameter.kind, actual) {
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                    let graph = self.compiler.metadata_type(self.module, ty, depth + 1)?;
                    self.compiler.metadata_copy(&graph, depth + 1, span)?;
                    CtScopeActual::Type(graph)
                }
                (s::GenericParamKind::Const(_), actual) => CtScopeActual::Scalar(
                    self.compiler
                        .source_scalar_argument(parameter, actual, span)?,
                ),
                (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                    CtScopeActual::Lifetime(self.compiler.ambient_actual(name, span)?)
                }
                _ => {
                    return Err(error(
                        span,
                        "CT explicit actual kind differs from full binder ordinal",
                    ));
                }
            });
        }
        let (types, result) = self.compiler.mixed_header(declaration, &actuals)?;
        let mut values = Vec::new();
        for (argument, ty) in arguments.iter().zip(types) {
            self.compiler.budget.node(argument.span)?;
            values.push(self.expr(&argument.value, Some(&ty), depth + 1)?.id);
        }
        let function = self
            .compiler
            .enqueue_mixed_instance(declaration, &actuals)?;
        self.compiler.mixed_actual_cost(&actuals, span)?;
        self.compiler.budget.node(span)?;
        Budget::take(
            &mut self.compiler.budget.bytes,
            std::mem::size_of::<SourceInstanceCall>(),
            span,
            "CT actual call ledger bytes exhausted",
        )?;
        let callee = self.compiler.item_symbol(d.module, d.item)?;
        let site = self.compiler.current_expression_site(span)?;
        self.compiler
            .source_instance_calls
            .push(SourceInstanceCall {
                caller: self.function_id(),
                caller_definition: self.definition.clone(),
                caller_scope,
                caller_scope_id,
                callee,
                instance: function,
                actuals,
                enclosing: self.compiler.active_host,
                site,
            });
        if result == CtTy::Scalar(CtScalarTy::Unit) {
            self.op(
                CtInstructionKind::Call {
                    function,
                    arguments: values,
                },
                span,
            )?;
            self.constant(CtConstant::Scalar(Constant::Unit), result, span)
        } else {
            let value = self.emit(
                CtInstructionKind::Call {
                    function,
                    arguments: values,
                },
                result,
                span,
            )?;
            self.relocate_record_result(value, expected, span)
        }
    }
}
