//! Actual impl materializations retain lexical scope requests from the selected
//! original method call. These records do not solve or authenticate a provider.
use super::*;
use std::collections::BTreeSet;
pub(super) struct Application {
    pub id: SemanticInstanceId,
    pub definition: SemanticDefinitionId,
    pub parameters: usize,
}
impl Program {
    /// Bind this specific default emission to the selected original impl.
    /// Siblings on the same Self may still have different trait arguments.
    pub(crate) fn default_method_template(
        &mut self,
        original: s::SemanticContextMarker,
        implementation: SemanticDefinitionId,
        selected: Option<SemanticInstanceId>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<s::SemanticContextMarker> {
        let raw = original
            .raw_index()
            .filter(|raw| raw & TEMPLATE != 0)
            .ok_or_else(|| error(span, "default method has no source template"))?;
        let template = self
            .templates
            .get((raw & !TEMPLATE) as usize)
            .ok_or_else(|| error(span, "default method source template is absent"))?;
        let method = template.definition;
        if !template.parameters.is_empty() {
            return Err(unsupported(
                span,
                "generic default method requires its complete method substitution",
            ));
        }
        let id = self.inherited_method_scope(method, implementation, selected, budget, span)?;
        let index = self.templates.len();
        let marker = u32::try_from(index)
            .ok()
            .filter(|index| *index < TEMPLATE)
            .ok_or_else(|| error(span, "default method template space exhausted"))?;
        budget.lineage_push(
            &mut self.templates,
            FunctionTemplate {
                definition: method,
                parameters: Vec::new(),
            },
            span,
        )?;
        budget.lineage_bytes(
            12 * std::mem::size_of::<(usize, SemanticInstanceId)>(),
            span,
        )?;
        self.default_scopes.insert(index, id);
        Ok(s::SemanticContextMarker::untrusted(TEMPLATE | marker))
    }

    fn inherited_method_scope(
        &mut self,
        method: SemanticDefinitionId,
        implementation: SemanticDefinitionId,
        selected: Option<SemanticInstanceId>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<SemanticInstanceId> {
        let implementation_scope = if let Some(selected) = selected {
            budget.identity_work(self.scope_applications.len(), span)?;
            if !self
                .scope_applications
                .iter()
                .any(|scope| scope.id == selected && scope.definition == implementation)
            {
                return Err(error(
                    span,
                    "selected default method scope names another original implementation",
                ));
            }
            selected
        } else {
            self.closed_lexical_scope(implementation, budget, 0)?
        };
        let find = |id| {
            self.modules
                .iter()
                .flat_map(|(_, module)| &module.definitions)
                .find(|row| row.id == id)
        };
        budget.identity_work(
            self.modules
                .iter()
                .map(|(_, module)| module.definitions.len())
                .sum::<usize>()
                .saturating_mul(3),
            span,
        )?;
        let parent = find(method)
            .and_then(|row| row.parent)
            .ok_or_else(|| error(span, "default method has no original trait scope"))?;
        let trait_scope =
            find(parent).ok_or_else(|| error(span, "default trait scope is absent"))?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) =
            &trait_scope.body
        else {
            return Err(error(
                span,
                "default body belongs to another original scope",
            ));
        };
        let provider = find(implementation)
            .ok_or_else(|| error(span, "default provider has no original impl"))?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
            trait_application,
            self_type,
            ..
        }) = &provider.body
        else {
            return Err(error(
                span,
                "default provider is not its original trait impl",
            ));
        };
        if declaration != &trait_application.declaration
            || trait_scope.parameters.len() != trait_application.arguments.len()
        {
            return Err(error(
                span,
                "default provider changes trait identity or complete argument arity",
            ));
        }
        let mut actuals = Vec::new();
        for argument in &trait_application.arguments {
            prepay_recipe(argument, budget, span, 0)?;
            budget.lineage_push(
                &mut actuals,
                SemanticActual::CoreRecipe {
                    scope: implementation_scope,
                    recipe: Box::new(argument.clone()),
                },
                span,
            )?;
        }
        prepay_recipe(self_type, budget, span, 0)?;
        let self_type = SemanticTypeValue::Core {
            scope: implementation_scope,
            source: SemanticCoreTypeSource::OriginalRecipe(Box::new(self_type.clone())),
        };
        let ancestor = trait_scope
            .parent
            .map(|id| self.closed_lexical_scope(id, budget, 0))
            .transpose()?;
        let id = SemanticInstanceId {
            module: parent.module,
            index: self.next_instance,
        };
        self.next_instance = self
            .next_instance
            .checked_add(1)
            .ok_or_else(|| error(span, "default lexical scope instance id exhausted"))?;
        let module = self
            .modules
            .iter_mut()
            .find(|(module, _)| *module == parent.module)
            .ok_or_else(|| error(span, "default trait module is absent"))?;
        budget.lineage_push(
            &mut module.1.instances,
            SemanticInstance {
                id,
                definition: parent,
                target: SemanticInstanceTarget::Scope,
                lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
                parent: ancestor,
                actuals,
                header_embedding: Vec::new(),
                environment: None,
                self_type: Some(self_type),
            },
            span,
        )?;
        Ok(id)
    }

    fn add_impl_application(
        &mut self,
        definition: SemanticDefinitionId,
        parameters: usize,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<SemanticInstanceId> {
        let id = SemanticInstanceId {
            module: definition.module,
            index: self.next_instance,
        };
        self.next_instance = self
            .next_instance
            .checked_add(1)
            .ok_or_else(|| error(span, "impl scope instance namespace exhausted"))?;
        budget.lineage_push(
            &mut self.scope_applications,
            Application {
                id,
                definition,
                parameters,
            },
            span,
        )?;
        Ok(id)
    }

    fn specialization_definition(
        &self,
        definition: SemanticDefinitionId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<&SemanticDefinition> {
        budget.identity_work(self.modules.len(), span)?;
        let (_, module) = self
            .modules
            .iter()
            .find(|(id, _)| *id == definition.module)
            .ok_or_else(|| error(span, "specialization original module is absent"))?;
        budget.identity_work(module.definitions.len(), span)?;
        module
            .definitions
            .iter()
            .find(|row| row.id == definition)
            .ok_or_else(|| error(span, "specialization original declaration is absent"))
    }

    pub(super) fn bind_specialized_parent(
        &mut self,
        instance: SemanticInstanceId,
        method: SemanticDefinitionId,
        implementation: SemanticDefinitionId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<SemanticInstanceId> {
        budget.identity_work(
            self.provider_parents.len().checked_ilog2().unwrap_or(0) as usize + 1,
            span,
        )?;
        if let Some(&(previous, request)) = self.provider_parents.get(&instance) {
            if implementation != previous {
                return Err(error(
                    span,
                    "one emitted method selected different original implementations",
                ));
            }
            return Ok(request);
        }
        let original = self.specialization_definition(implementation, budget, span)?;
        if !matches!(
            original.body,
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl { .. })
        ) {
            return Err(error(
                span,
                "selected provider has no original trait implementation",
            ));
        }
        let parameters = original.parameters.len();
        let selected = if parameters == 0 {
            None
        } else {
            Some(self.add_impl_application(implementation, parameters, budget, span)?)
        };
        let original_parent = self
            .specialization_definition(method, budget, span)?
            .parent
            .ok_or_else(|| error(span, "specialized method lost its original lexical parent"))?;
        let lexical_parent = if original_parent == implementation {
            match selected {
                Some(scope) => scope,
                None => self.closed_lexical_scope(implementation, budget, 0)?,
            }
        } else {
            self.inherited_method_scope(method, implementation, selected, budget, span)?
        };
        budget.lineage_bytes(
            12 * std::mem::size_of::<(SemanticInstanceId, (SemanticDefinitionId, SemanticInstanceId))>()
                + 12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(),
            span,
        )?;
        if self.scope_owners.insert(instance, lexical_parent).is_some() {
            return Err(error(
                span,
                "specialized method was attached to different lexical parents",
            ));
        }
        let request = selected.unwrap_or(instance);
        self.provider_parents.insert(instance, (implementation, request));
        Ok(request)
    }

    pub(super) fn record_provider_sibling_scope(
        &mut self,
        ordinary: SemanticInstanceId,
        provider: SemanticInstanceId,
        implementation: SemanticDefinitionId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        budget.identity_work(self.scope_applications.len(), span)?;
        let mut matching = self.scope_applications.iter().filter(|scope| {
            (scope.id == ordinary || scope.id == provider) && scope.definition == implementation
        });
        if ordinary == provider || matching.next().is_none() || matching.next().is_none() {
            return Err(error(span, "provider and ordinary scope do not share one original impl"));
        }
        budget.identity_work(
            self.provider_sibling_scopes.len().checked_ilog2().unwrap_or(0) as usize + 1,
            span,
        )?;
        budget.lineage_bytes(12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(), span)?;
        self.provider_sibling_scopes.entry(ordinary).or_insert(provider);
        Ok(())
    }

    pub(super) fn reuse_uncalled_impl_scopes(
        &mut self,
        budget: &mut Budget<'_>,
    ) -> R<BTreeSet<SemanticInstanceId>> {
        let mut members = BTreeSet::new();
        if self.provider_sibling_scopes.is_empty() {
            return Ok(members);
        }
        let span = Span::default();
        let mut called = BTreeSet::new();
        for request in &self.scope_requests {
            budget.identity_work(called.len().saturating_add(1).ilog2() as usize + 1, span)?;
            budget.lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
            called.insert(request.callee);
        }
        let mut replacements = std::mem::take(&mut self.provider_sibling_scopes);
        budget.identity_work(replacements.len().saturating_mul(called.len().saturating_add(1).ilog2() as usize + 1), span)?;
        replacements.retain(|scope, _| !called.contains(scope));
        if replacements.is_empty() {
            return Ok(members);
        }
        // These ordinary members were physically emitted but have no direct
        // call. Their exact witness already has a real provider call, whose
        // implementation scope can own those members without a new request.
        let lookup = replacements.len().ilog2() as usize + 1;
        budget.identity_work(self.scope_owners.len().saturating_mul(lookup), span)?;
        for (member, scope) in &mut self.scope_owners {
            if let Some(replacement) = replacements.get(scope) {
                *scope = *replacement;
                budget.lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
                members.insert(*member);
            }
        }
        budget.identity_work(self.modules.len(), span)?;
        for (_, module) in &mut self.modules {
            budget.identity_work(module.instances.len().saturating_mul(lookup), span)?;
            for row in &mut module.instances {
                if matches!(row.target, SemanticInstanceTarget::Scope)
                    && let Some(SemanticTypeValue::Core { scope, .. }) = &row.self_type
                    && replacements.contains_key(scope)
                {
                    budget.identity_work(row.actuals.len().saturating_add(3).saturating_mul(lookup), span)?;
                    requests::instance(row, &replacements);
                    budget.lineage_bytes(12 * std::mem::size_of::<SemanticInstanceId>(), span)?;
                    members.insert(row.id);
                }
            }
        }
        budget.identity_work(self.scope_applications.len().saturating_mul(lookup), span)?;
        self.scope_applications.retain(|scope| !replacements.contains_key(&scope.id));
        Ok(members)
    }

    pub(crate) fn realize_impl_scope(
        &mut self,
        implementation: &mut s::ImplBlock,
        mut definition: Option<SemanticDefinitionId>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<SemanticInstanceId>> {
        let mut has_emitted_method = definition.is_some();
        for member in &implementation.members {
            budget.node(member.span, 0)?;
            let s::ItemKind::Function(function) = &member.kind else {
                continue;
            };
            let Some(raw) = function.semantic.raw_index() else {
                continue;
            };
            has_emitted_method |= function.generics.is_empty();
            if raw & TEMPLATE == 0 {
                return Err(error(
                    member.span,
                    "impl materialization reused a completed method owner",
                ));
            }
            let method = self
                .templates
                .get((raw & !TEMPLATE) as usize)
                .ok_or_else(|| error(member.span, "impl method original template is absent"))?
                .definition;
            let original = self
                .modules
                .iter()
                .flat_map(|(_, m)| &m.definitions)
                .find(|row| row.id == method)
                .ok_or_else(|| error(member.span, "impl method original definition is absent"))?;
            let parent = original
                .parent
                .ok_or_else(|| error(member.span, "impl method lost its actual lexical parent"))?;
            if definition.replace(parent).is_some_and(|old| old != parent) {
                return Err(error(
                    member.span,
                    "one actual impl contains methods from different lexical scopes",
                ));
            }
        }
        let Some(definition) = definition else {
            return Ok(None);
        };
        let original = self
            .modules
            .iter()
            .flat_map(|(_, m)| &m.definitions)
            .find(|row| row.id == definition)
            .ok_or_else(|| error(span, "impl original scope is absent"))?;
        if !matches!(
            original.body,
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::TraitImpl { .. }
                    | SemanticScopeRecipe::InherentImpl { .. }
                    | SemanticScopeRecipe::BuiltinImpl { .. }
            )
        ) {
            return Err(error(
                span,
                "actual impl scope does not name its original implementation",
            ));
        }
        if original.parameters.is_empty() || !has_emitted_method {
            return Ok(None);
        }
        let id = self.add_impl_application(definition, original.parameters.len(), budget, span)?;
        for member in &mut implementation.members {
            let s::ItemKind::Function(function) = &mut member.kind else {
                continue;
            };
            // Generic members are lifted as separate providers. The witness
            // supplies their original impl scope, but only the selected
            // provider specialization can mint a function owner.
            if !function.generics.is_empty() || function.semantic.raw_index().is_none() {
                continue;
            }
            self.ensure_function(function, budget)?;
            let marker = function
                .semantic
                .raw_index()
                .ok_or_else(|| error(member.span, "actual impl method lost its owner"))?;
            let instance = self
                .instances
                .get(marker as usize)
                .ok_or_else(|| error(member.span, "actual impl method instance is absent"))?
                .id;
            budget.lineage_bytes(
                12 * std::mem::size_of::<(SemanticInstanceId, SemanticInstanceId)>(),
                span,
            )?;
            if self.scope_owners.insert(instance, id).is_some() {
                return Err(error(span, "actual impl method was attached to two scopes"));
            }
        }
        Ok(Some(id))
    }
    pub(crate) fn record_scope_call(
        &mut self,
        marker: s::SemanticContextMarker,
        scope: SemanticInstanceId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        let Some(raw) = marker.raw_index() else {
            return Err(error(
                span,
                "selected impl call lacks its original operation token",
            ));
        };
        let (caller, call) = self.ledger.actual_call_site(raw).ok_or_else(|| {
            error(
                span,
                "selected impl receiver call lacks its actual original caller/site",
            )
        })?;
        budget.identity_work(self.scope_requests.len(), span)?;
        if let Some(previous) = self
            .scope_requests
            .iter()
            .find(|row| row.caller == caller && row.call == call)
        {
            if previous.callee != scope {
                return Err(error(
                    span,
                    "one actual original receiver call selected conflicting lexical scopes",
                ));
            }
            return Ok(());
        }
        budget.lineage_push(
            &mut self.scope_requests,
            requests::Request {
                caller,
                call,
                callee: scope,
            },
            span,
        )
    }
    pub(super) fn publish_impl_scopes(&mut self, budget: &mut Budget<'_>) -> R<()> {
        let span = Span::default();
        // Scope actuals are populated by the actual call requests, not by the
        // physical Source substitution or a self-referential Formal recipe.
        let count = self.scope_applications.len();
        for index in 0..count {
            let application = &self.scope_applications[index];
            let id = application.id;
            let definition = application.definition;
            let original = self
                .modules
                .iter()
                .flat_map(|(_, m)| &m.definitions)
                .find(|row| row.id == definition)
                .ok_or_else(|| error(span, "original impl scope disappeared"))?;
            let parent = original.parent;
            let self_type = match &original.body {
                SemanticDefinitionBody::Scope(
                    SemanticScopeRecipe::TraitImpl { self_type, .. }
                    | SemanticScopeRecipe::InherentImpl { self_type, .. }
                    | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
                ) => {
                    prepay_recipe(self_type, budget, span, 0)?;
                    self_type.clone()
                }
                _ => return Err(error(span, "actual impl lexical scope changed kind")),
            };
            let parent = parent
                .map(|parent| self.closed_lexical_scope(parent, budget, 0))
                .transpose()?;
            let row = SemanticInstance {
                id,
                definition,
                target: SemanticInstanceTarget::Scope,
                lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
                parent,
                actuals: Vec::new(),
                header_embedding: Vec::new(),
                environment: None,
                self_type: Some(SemanticTypeValue::Core {
                    scope: id,
                    source: SemanticCoreTypeSource::OriginalRecipe(Box::new(self_type)),
                }),
            };
            let target = self
                .modules
                .iter_mut()
                .find(|(m, _)| *m == definition.module)
                .ok_or_else(|| error(span, "actual impl scope module is absent"))?;
            budget.lineage_push(&mut target.1.instances, row, span)?;
        }
        Ok(())
    }
}
