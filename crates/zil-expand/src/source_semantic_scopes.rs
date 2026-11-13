//! Actual impl materializations retain lexical scope requests from the selected
//! original method call. These records do not solve or authenticate a provider.
use super::*;
pub(super) struct Application {
    pub id: SemanticInstanceId,
    pub definition: SemanticDefinitionId,
    pub parameters: usize,
}
impl Program {
    pub(crate) fn realize_impl_scope(
        &mut self,
        implementation: &mut s::ImplBlock,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<SemanticInstanceId>> {
        let mut definition = None;
        for member in &implementation.members {
            budget.node(member.span, 0)?;
            let s::ItemKind::Function(function) = &member.kind else {
                continue;
            };
            let Some(raw) = function.semantic.raw_index() else {
                continue;
            };
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
        if original.parameters.is_empty() {
            return Ok(None);
        }
        let parameters = original.parameters.len();
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
        for member in &mut implementation.members {
            let s::ItemKind::Function(function) = &mut member.kind else {
                continue;
            };
            if function.semantic.raw_index().is_none() {
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
