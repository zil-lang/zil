//! Linear proposals consumed only after the real Core node has been minted.
//! A token is a request-local address, never a provenance/capability proof.
use super::*;
use zil_base::NodeId;

#[derive(Clone)]
pub(crate) struct Actual {
    pub(crate) recipe: SemanticActualRecipe,
    /// Actual argument ordinal, not a same-shaped expression found later.
    pub(crate) inferred_argument: Option<u32>,
}
#[derive(Clone)]
pub(crate) enum Action {
    Construct(SemanticTypeRecipe),
    TypeUse(SemanticTypeRecipe),
    DeclareLocal(SemanticTypeRecipe),
    Call {
        target: SemanticFunctionTarget,
        actuals: Vec<Actual>,
    },
    DeferredCall {definition:SemanticDefinitionId,actuals:Vec<Actual>},
    CallableCall,
    Closure {
        environment: Vec<SemanticActualRecipe>,
    },
}
#[derive(Clone)]
pub(crate) struct Event {
    pub(crate) recipe: SemanticSiteId,
    pub(crate) role: SemanticEmissionRole,
    pub(crate) action: Action,
}
struct Pending {
    instance: SemanticInstanceId,
    creator: Option<NodeId>,
    events: Vec<Event>,
}
#[derive(Default)]
pub(crate) struct Ledger {
    entries: Vec<Option<Pending>>,
    emitted: Vec<(SemanticInstanceId, NodeId, u32, SemanticEmissionRole)>,
    owners: Vec<OriginalOwner>,
    templates: Vec<OriginalTemplate>,
}
struct OriginalTemplate {
    definition: SemanticDefinitionId,
    events: Vec<Event>,
}
// The namespace is private to this request. It distinguishes an immutable
// original occurrence from a linear actual production; neither is authority.
const TEMPLATE: u32 = 1 << 31;
struct OriginalOwner {
    instance: SemanticInstanceId,
    definition: SemanticDefinitionId,
    creator: Option<NodeId>,
}
fn unavailable(message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-SOURCE-SEMANTIC-EMISSION", None, message)
}
fn deferred(
    scope: SemanticInstanceId,
    recipe: SemanticTypeRecipe,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<SemanticTypeValue> {
    budget.lineage_bytes(std::mem::size_of::<SemanticTypeRecipe>(), span)?;
    Ok(SemanticTypeValue::Core {
        scope,
        source: SemanticCoreTypeSource::OriginalRecipe(Box::new(recipe)),
    })
}
impl Ledger {
    pub(crate) fn deferred_call(&self,marker:u32)->Option<SemanticDefinitionId>{
        self.entries.get(marker as usize).and_then(Option::as_ref)?.events.iter().find_map(|event|if let Action::DeferredCall{definition,..}=&event.action{Some(*definition)}else{None})
    }
    pub(crate) fn resolve_call(&mut self,marker:u32,definition:SemanticDefinitionId,target:SemanticFunctionTarget,budget:&mut Budget<'_>,span:Span)->R<()> {
        budget.node(span,0)?;
        let pending=self.entries.get_mut(marker as usize).and_then(Option::as_mut).ok_or_else(||unavailable("actual static call event is absent"))?;
        let event=pending.events.iter_mut().find(|event|matches!(event.action,Action::DeferredCall{..})).ok_or_else(||unavailable("actual static call has no original source recipe"))?;
        let Action::DeferredCall{definition:expected,..}=&event.action else{unreachable!()};
        if *expected!=definition{return Err(unavailable("actual Core static callee has a different original declaration"));}
        let Action::DeferredCall{actuals,..}=std::mem::replace(&mut event.action,Action::CallableCall) else{unreachable!()};
        event.action=Action::Call{target,actuals};Ok(())
    }
    pub(crate) fn actual_role(&self, marker: u32) -> Option<SemanticEmissionRole> {
        self.entries
            .get(marker as usize)
            .and_then(Option::as_ref)
            .and_then(|row| row.events.first())
            .map(|event| event.role)
    }
    pub(crate) fn actual_owner(&self, marker: u32) -> Option<SemanticInstanceId> {
        self.entries
            .get(marker as usize)
            .and_then(Option::as_ref)
            .map(|row| row.instance)
    }
    pub(crate) fn template_site(
        &self,
        marker: s::SemanticContextMarker,
    ) -> Option<(SemanticDefinitionId, SemanticSiteId)> {
        let raw = marker.raw_index()?;
        if raw & TEMPLATE == 0 {
            return None;
        };
        let row = self.templates.get((raw & !TEMPLATE) as usize)?;
        Some((row.definition, row.events.first()?.recipe))
    }
    pub(crate) fn is_template(&self, marker: s::SemanticContextMarker) -> bool {
        marker.raw_index().is_some_and(|raw| {
            raw & TEMPLATE != 0 && self.templates.get((raw & !TEMPLATE) as usize).is_some()
        })
    }
    fn validate_bundle(events: &[Event], budget: &mut Budget<'_>, span: Span) -> R<()> {
        if events.is_empty() {
            return Err(unavailable("semantic production has an empty event bundle"));
        }
        for (index, event) in events.iter().enumerate() {
            budget.identity_work(index.saturating_add(1), span)?;
            let role = match &event.action {
                Action::Construct(_) => SemanticEmissionRole::Construct,
                Action::TypeUse(_) => SemanticEmissionRole::Ascription,
                Action::DeclareLocal(_) => SemanticEmissionRole::Binding,
                Action::Call { .. } | Action::DeferredCall{..} | Action::CallableCall => SemanticEmissionRole::Call,
                Action::Closure { .. } => SemanticEmissionRole::Closure,
            };
            if role != event.role || events[..index].iter().any(|old| old.role == event.role) {
                return Err(unavailable(
                    "semantic bundle repeats a role or mismatches its actual action",
                ));
            }
        }
        Ok(())
    }
    /// Retain one original syntax occurrence as an immutable recipe. Generic
    /// template clones retain this token until the actual specialization is
    /// known; they do not share a consumable production event.
    pub(crate) fn template(
        &mut self,
        definition: SemanticDefinitionId,
        events: Vec<Event>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<s::SemanticContextMarker> {
        Self::validate_bundle(&events, budget, span)?;
        let index = u32::try_from(self.templates.len())
            .map_err(|_| unavailable("semantic template namespace overflow"))?;
        if index >= TEMPLATE - 1 {
            return Err(unavailable(
                "semantic template token uses the reserved sentinel",
            ));
        }
        if self.templates.len() == self.templates.capacity() {
            let growth = self.templates.capacity().max(4);
            budget.identity_work(self.templates.len(), span)?;
            budget.lineage_bytes(
                growth
                    .checked_mul(std::mem::size_of::<OriginalTemplate>())
                    .ok_or_else(|| unavailable("semantic template allocation overflow"))?,
                span,
            )?;
            self.templates
                .try_reserve_exact(growth)
                .map_err(|_| unavailable("semantic template allocation failed"))?;
        }
        self.templates.push(OriginalTemplate { definition, events });
        Ok(s::SemanticContextMarker::untrusted(index | TEMPLATE))
    }
    pub(crate) fn instantiate_template(
        &mut self,
        marker: s::SemanticContextMarker,
        instance: SemanticInstanceId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<s::SemanticContextMarker> {
        budget.identity_work(self.owners.len().saturating_add(1), span)?;
        let raw = marker
            .raw_index()
            .filter(|raw| raw & TEMPLATE != 0)
            .ok_or_else(|| {
                unavailable("semantic specialization requires an actual original template token")
            })?;
        let template = self
            .templates
            .get((raw & !TEMPLATE) as usize)
            .ok_or_else(|| unavailable("semantic original occurrence is absent"))?;
        let owner = self
            .owners
            .iter()
            .find(|owner| owner.instance == instance)
            .ok_or_else(|| unavailable("semantic specialization has no original owner"))?;
        if owner.definition != template.definition || owner.creator.is_some() {
            return Err(unavailable(
                "semantic original occurrence belongs to another definition or completed specialization",
            ));
        }
        budget.lineage_bytes(
            template
                .events
                .len()
                .checked_mul(std::mem::size_of::<Event>())
                .ok_or_else(|| unavailable("semantic event clone allocation overflow"))?,
            span,
        )?;
        for event in &template.events {
            budget.node(span, 0)?;
            match &event.action {
                Action::Construct(ty) | Action::TypeUse(ty) | Action::DeclareLocal(ty) => {
                    prepay_recipe(ty, budget, span, 1)?
                }
                Action::Call { target, actuals } => {
                    prepay_recipe(target, budget, span, 1)?;
                    budget.lineage_bytes(
                        actuals
                            .len()
                            .checked_mul(std::mem::size_of::<Actual>())
                            .ok_or_else(|| {
                                unavailable("semantic actual clone allocation overflow")
                            })?,
                        span,
                    )?;
                    for actual in actuals {
                        prepay_recipe(&actual.recipe, budget, span, 1)?;
                    }
                }
                Action::DeferredCall{definition,actuals}=> {
                    prepay_recipe(definition,budget,span,1)?;
                    budget.lineage_bytes(actuals.len()*std::mem::size_of::<Actual>(),span)?;
                    for actual in actuals {prepay_recipe(&actual.recipe,budget,span,1)?;}
                }
                Action::DeferredCall{..}=>return Err(unavailable("static source call has not bound its actual Core callee")),
            Action::CallableCall => {}
                Action::Closure { environment } => {
                    budget.lineage_bytes(
                        environment
                            .len()
                            .checked_mul(std::mem::size_of::<SemanticActualRecipe>())
                            .ok_or_else(|| {
                                unavailable("semantic environment clone allocation overflow")
                            })?,
                        span,
                    )?;
                    for actual in environment {
                        prepay_recipe(actual, budget, span, 1)?;
                    }
                }
            }
        }
        let events = template.events.clone();
        self.record_original(instance, events, budget, span)
    }
    /// Record the actual specialization owner before Core exists. The original
    /// definition comes from the aligned source/Origins ledger, never a guessed
    /// future Core function number. Body-free scopes use their separate scope
    /// producer and do not register a fictitious creator here.
    pub(crate) fn original_owner(
        &mut self,
        instance: SemanticInstanceId,
        definition: SemanticDefinitionId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        budget.identity_work(self.owners.len().saturating_add(1), span)?;
        if self.owners.iter().any(|owner| owner.instance == instance) {
            return Err(unavailable(
                "semantic specialization owner is already registered",
            ));
        }
        if self.owners.len() == self.owners.capacity() {
            let growth = self.owners.capacity().max(4);
            budget.lineage_bytes(
                growth
                    .checked_mul(std::mem::size_of::<OriginalOwner>())
                    .ok_or_else(|| unavailable("semantic owner allocation overflow"))?,
                span,
            )?;
            self.owners
                .try_reserve_exact(growth)
                .map_err(|_| unavailable("semantic owner allocation failed"))?;
        }
        self.owners.push(OriginalOwner {
            instance,
            definition,
            creator: None,
        });
        Ok(())
    }
    pub(crate) fn record_original(
        &mut self,
        instance: SemanticInstanceId,
        events: Vec<Event>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<s::SemanticContextMarker> {
        budget.identity_work(self.owners.len().saturating_add(1), span)?;
        let owner = self
            .owners
            .iter()
            .find(|owner| owner.instance == instance)
            .ok_or_else(|| {
                unavailable("semantic event has no actual original specialization owner")
            })?;
        if owner.creator.is_some() {
            return Err(unavailable(
                "original specialization cannot append events after Core emission began",
            ));
        }
        self.record_bundle_inner(instance, None, events, budget, span)
    }
    /// Bind only at the actual CoreFunction mint, with the original identity
    /// carried by that same pending item. Neither a marker nor a source span
    /// selects which emitted function is its creator. Different semantic
    /// instances may share a real Core template: Types specializes callable
    /// helpers only after it knows the whole actual F/entry. That later binding
    /// must use complete actuals, never the first equal Core target.
    pub(crate) fn bind_creator(
        &mut self,
        instance: SemanticInstanceId,
        definition: SemanticDefinitionId,
        creator: NodeId,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<()> {
        budget.identity_work(
            self.owners
                .len()
                .saturating_add(self.entries.len())
                .saturating_add(1),
            span,
        )?;
        let owner = self
            .owners
            .iter()
            .position(|owner| owner.instance == instance)
            .ok_or_else(|| {
                unavailable("actual Core creator has no original specialization owner")
            })?;
        if self.owners[owner].definition != definition || self.owners[owner].creator.is_some() {
            return Err(unavailable(
                "actual Core creator reuses or changes its original specialization identity",
            ));
        }
        if self
            .entries
            .iter()
            .flatten()
            .any(|pending| pending.instance == instance && pending.creator.is_some())
        {
            return Err(unavailable(
                "original semantic event was bound before its actual creator",
            ));
        }
        // Validate the complete update before publishing any creator binding.
        self.owners[owner].creator = Some(creator);
        for pending in self.entries.iter_mut().flatten() {
            if pending.instance == instance {
                pending.creator = Some(creator);
            }
        }
        Ok(())
    }
    pub(crate) fn record(
        &mut self,
        instance: SemanticInstanceId,
        creator: NodeId,
        recipe: SemanticSiteId,
        role: SemanticEmissionRole,
        action: Action,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<s::SemanticContextMarker> {
        budget.lineage_bytes(std::mem::size_of::<Event>(), span)?;
        self.record_bundle(
            instance,
            creator,
            vec![Event {
                recipe,
                role,
                action,
            }],
            budget,
            span,
        )
    }
    pub(crate) fn record_bundle(
        &mut self,
        instance: SemanticInstanceId,
        creator: NodeId,
        events: Vec<Event>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<s::SemanticContextMarker> {
        self.record_bundle_inner(instance, Some(creator), events, budget, span)
    }
    fn record_bundle_inner(
        &mut self,
        instance: SemanticInstanceId,
        creator: Option<NodeId>,
        events: Vec<Event>,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<s::SemanticContextMarker> {
        budget.node(span, 0)?;
        Self::validate_bundle(&events, budget, span)?;
        let token = u32::try_from(self.entries.len())
            .map_err(|_| unavailable("source semantic token overflow"))?;
        if token >= TEMPLATE {
            return Err(unavailable(
                "source semantic token uses the absent sentinel",
            ));
        }
        if self.entries.len() == self.entries.capacity() {
            let growth = self.entries.capacity().max(4);
            budget.identity_work(self.entries.len(), span)?;
            budget.lineage_bytes(
                growth
                    .checked_mul(std::mem::size_of::<Option<Pending>>())
                    .ok_or_else(|| unavailable("semantic ledger allocation overflow"))?,
                span,
            )?;
            self.entries
                .try_reserve_exact(growth)
                .map_err(|_| unavailable("semantic ledger allocation failed"))?;
        }
        self.entries.push(Some(Pending {
            instance,
            creator,
            events,
        }));
        Ok(s::SemanticContextMarker::untrusted(token))
    }
    fn take(
        &mut self,
        marker: s::SemanticContextMarker,
        instance: SemanticInstanceId,
        creator: NodeId,
        node: NodeId,
        occurrence: u32,
        role: SemanticEmissionRole,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Event> {
        let token = marker
            .raw_index()
            .ok_or_else(|| unavailable("actual emission has no source semantic production"))?;
        budget.identity_work(
            self.entries
                .len()
                .saturating_add(self.emitted.len())
                .saturating_add(1),
            span,
        )?;
        let pending = self
            .entries
            .get(token as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| {
                unavailable("source semantic production is absent or already consumed")
            })?;
        if pending.instance != instance || pending.creator != Some(creator) {
            return Err(unavailable(
                "source semantic production belongs to another actual owner or role",
            ));
        }
        budget.identity_work(pending.events.len(), span)?;
        let event = pending
            .events
            .iter()
            .position(|event| event.role == role)
            .ok_or_else(|| {
                unavailable("source semantic bundle has no unconsumed event for this actual role")
            })?;
        if self
            .emitted
            .iter()
            .any(|old| *old == (instance, node, occurrence, role))
        {
            return Err(unavailable(
                "source semantic actual emission identity is reused",
            ));
        }
        if self.emitted.len() == self.emitted.capacity() {
            let growth = self.emitted.capacity().max(4);
            budget.lineage_bytes(
                growth
                    .checked_mul(std::mem::size_of::<(
                        SemanticInstanceId,
                        NodeId,
                        u32,
                        SemanticEmissionRole,
                    )>())
                    .ok_or_else(|| unavailable("emission inventory allocation overflow"))?,
                span,
            )?;
            self.emitted
                .try_reserve_exact(growth)
                .map_err(|_| unavailable("emission inventory allocation failed"))?;
        }
        self.emitted.push((instance, node, occurrence, role));
        let pending = self.entries[token as usize]
            .as_mut()
            .expect("actual source production checked");
        let result = pending.events.remove(event);
        if pending.events.is_empty() {
            self.entries[token as usize] = None;
        }
        Ok(result)
    }
    pub(crate) fn expression(
        &mut self,
        marker: s::SemanticContextMarker,
        instance: SemanticInstanceId,
        creator: NodeId,
        expression: &CoreExpr,
        occurrence: u32,
        role: SemanticEmissionRole,
        budget: &mut Budget<'_>,
    ) -> R<SemanticSite> {
        let pending = self.take(
            marker,
            instance,
            creator,
            expression.id,
            occurrence,
            role,
            budget,
            expression.span,
        )?;
        let result = SemanticOperand::CoreExpression {
            expression: expression.id,
            occurrence,
        };
        let action = match pending.action {
            Action::Construct(recipe) => SemanticSiteAction::Construct {
                result,
                value: deferred(instance, recipe, budget, expression.span)?,
            },
            Action::TypeUse(recipe) => SemanticSiteAction::TypeUse {
                subject: result,
                expected: deferred(instance, recipe, budget, expression.span)?,
            },
            Action::DeclareLocal(_) => {
                return Err(unavailable(
                    "local declaration must consume an actual statement, not read an occupant",
                ));
            }
            Action::Call { target, actuals } => {
                let CoreExprKind::Call { arguments, .. } = &expression.kind else {
                    return Err(unavailable("static semantic call has no actual Core Call"));
                };
                budget.lineage_bytes(
                    actuals
                        .len()
                        .checked_mul(std::mem::size_of::<SemanticActual>())
                        .ok_or_else(|| unavailable("call actual allocation overflow"))?,
                    expression.span,
                )?;
                let mut output = Vec::with_capacity(actuals.len());
                for actual in actuals {
                    budget.node(expression.span, 0)?;
                    if let Some(index) = actual.inferred_argument {
                        let SemanticActualRecipe::Type(recipe) = actual.recipe else {
                            return Err(unavailable(
                                "inferred expression type requires a complete original Type recipe",
                            ));
                        };
                        let argument = arguments.get(index as usize).ok_or_else(|| {
                            unavailable("semantic actual argument ordinal is outside the real Call")
                        })?;
                        budget.lineage_bytes(
                            std::mem::size_of::<SemanticTypeRecipe>(),
                            expression.span,
                        )?;
                        output.push(SemanticActual::Type(SemanticTypeValue::Core {
                            scope: instance,
                            source: SemanticCoreTypeSource::Expression {
                                module: instance.module,
                                function: creator,
                                expression: argument.id,
                                occurrence,
                                recipe: Box::new(recipe),
                            },
                        }));
                    } else {
                        budget.lineage_bytes(
                            std::mem::size_of::<SemanticActualRecipe>(),
                            expression.span,
                        )?;
                        output.push(SemanticActual::CoreRecipe {
                            scope: instance,
                            recipe: Box::new(actual.recipe),
                        });
                    }
                }
                SemanticSiteAction::Call {
                    target,
                    actuals: output,
                }
            }
            Action::CallableCall => {
                let CoreExprKind::Call { callee, .. } = &expression.kind else {
                    return Err(unavailable("callable site has no actual Core callee"));
                };
                SemanticSiteAction::CallableCall {
                    callee: SemanticOperand::CoreExpression {
                        expression: callee.id,
                        occurrence,
                    },
                }
            }
            Action::Closure { environment } => {
                if !matches!(expression.kind, CoreExprKind::Closure { .. }) {
                    return Err(unavailable(
                        "closure semantic site has no actual closure production",
                    ));
                }
                budget.lineage_bytes(
                    environment
                        .len()
                        .checked_mul(
                            std::mem::size_of::<SemanticActual>()
                                + std::mem::size_of::<SemanticActualRecipe>(),
                        )
                        .ok_or_else(|| unavailable("ghost environment allocation overflow"))?,
                    expression.span,
                )?;
                let environment = environment
                    .into_iter()
                    .map(|recipe| SemanticActual::CoreRecipe {
                        scope: instance,
                        recipe: Box::new(recipe),
                    })
                    .collect();
                SemanticSiteAction::Closure {
                    result,
                    closure: SemanticClosureTarget::Core {
                        module: instance.module,
                        creator,
                        expression: expression.id,
                        occurrence,
                    },
                    environment,
                }
            }
        };
        Ok(SemanticSite {
            instance,
            recipe: pending.recipe,
            at: SemanticSiteAddress::Core {
                expression: expression.id,
                occurrence,
                role,
            },
            action,
        })
    }
    pub(crate) fn statement(
        &mut self,
        marker: s::SemanticContextMarker,
        instance: SemanticInstanceId,
        creator: NodeId,
        statement: &CoreStatement,
        occurrence: u32,
        budget: &mut Budget<'_>,
    ) -> R<SemanticSite> {
        let role = SemanticEmissionRole::Binding;
        let pending = self.take(
            marker,
            instance,
            creator,
            statement.id,
            occurrence,
            role,
            budget,
            statement.span,
        )?;
        let Action::DeclareLocal(recipe) = pending.action else {
            return Err(unavailable(
                "statement semantic token is not a declaration constraint",
            ));
        };
        if !matches!(statement.kind, CoreStatementKind::Binding { .. }) {
            return Err(unavailable(
                "declaration semantic site has no actual Core binding",
            ));
        }
        Ok(SemanticSite {
            instance,
            recipe: pending.recipe,
            at: SemanticSiteAddress::Core {
                expression: statement.id,
                occurrence,
                role,
            },
            action: SemanticSiteAction::DeclareLocal {
                local: SemanticLocalTarget::Core {
                    statement: statement.id,
                    occurrence,
                },
                expected: deferred(instance, recipe, budget, statement.span)?,
            },
        })
    }
    pub(crate) fn finish(&self, budget: &mut Budget<'_>, span: Span) -> R<()> {
        budget.identity_work(self.entries.len().saturating_add(self.owners.len()), span)?;
        if self.owners.iter().any(|owner| owner.creator.is_none()) {
            return Err(unavailable(
                "original specialization has no actual emitted Core creator",
            ));
        }
        if self.entries.iter().any(Option::is_some) {
            return Err(unavailable(
                "source semantic production has no actual emitted node",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn instance(index: u32) -> SemanticInstanceId {
        SemanticInstanceId {
            module: ModuleId(0),
            index,
        }
    }
    fn tuple() -> CoreExpr {
        CoreExpr {
            id: NodeId(12),
            span: Span::default(),
            kind: CoreExprKind::Tuple(Vec::new()),
        }
    }
    fn event(recipe: u32, role: SemanticEmissionRole, action: Action) -> Event {
        Event {
            recipe: SemanticSiteId(recipe),
            role,
            action,
        }
    }
    #[test]
    fn one_actual_node_consumes_each_recorded_role_once_and_requires_the_full_bundle() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut ledger = Ledger::default();
        let marker = ledger
            .record_bundle(
                instance(0),
                NodeId(4),
                vec![
                    event(
                        0,
                        SemanticEmissionRole::Construct,
                        Action::Construct(SemanticTypeRecipe::Tuple(Vec::new())),
                    ),
                    event(
                        1,
                        SemanticEmissionRole::Ascription,
                        Action::TypeUse(SemanticTypeRecipe::Tuple(Vec::new())),
                    ),
                ],
                &mut budget,
                Span::default(),
            )
            .unwrap();
        assert!(
            ledger
                .expression(
                    marker,
                    instance(1),
                    NodeId(4),
                    &tuple(),
                    0,
                    SemanticEmissionRole::Construct,
                    &mut budget
                )
                .is_err()
        );
        let first = ledger
            .expression(
                marker,
                instance(0),
                NodeId(4),
                &tuple(),
                0,
                SemanticEmissionRole::Construct,
                &mut budget,
            )
            .unwrap();
        assert_eq!(first.recipe, SemanticSiteId(0));
        assert!(ledger.finish(&mut budget, Span::default()).is_err());
        assert!(
            ledger
                .expression(
                    marker,
                    instance(0),
                    NodeId(4),
                    &tuple(),
                    0,
                    SemanticEmissionRole::Construct,
                    &mut budget
                )
                .is_err()
        );
        let second = ledger
            .expression(
                marker,
                instance(0),
                NodeId(4),
                &tuple(),
                0,
                SemanticEmissionRole::Ascription,
                &mut budget,
            )
            .unwrap();
        assert_eq!(second.recipe, SemanticSiteId(1));
        ledger.finish(&mut budget, Span::default()).unwrap();
    }
    #[test]
    fn inferred_actual_points_to_the_real_argument_and_uninitialized_binding_is_not_read() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut ledger = Ledger::default();
        let marker = ledger
            .record(
                instance(0),
                NodeId(4),
                SemanticSiteId(0),
                SemanticEmissionRole::Call,
                Action::Call {
                    target: SemanticFunctionTarget::Core {
                        module: ModuleId(0),
                        function: NodeId(8),
                    },
                    actuals: vec![Actual {
                        recipe: SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(Type::I32)),
                        inferred_argument: Some(0),
                    }],
                },
                &mut budget,
                Span::default(),
            )
            .unwrap();
        let expression = CoreExpr {
            id: NodeId(15),
            span: Span::default(),
            kind: CoreExprKind::Call {
                callee: Box::new(CoreExpr {
                    id: NodeId(16),
                    span: Span::default(),
                    kind: CoreExprKind::Name("actual".into()),
                }),
                arguments: vec![CoreExpr {
                    id: NodeId(17),
                    span: Span::default(),
                    kind: CoreExprKind::Integer("42".into()),
                }],
            },
        };
        let site = ledger
            .expression(
                marker,
                instance(0),
                NodeId(4),
                &expression,
                0,
                SemanticEmissionRole::Call,
                &mut budget,
            )
            .unwrap();
        let SemanticSiteAction::Call { actuals, .. } = site.action else {
            panic!()
        };
        assert!(matches!(
            &actuals[0],
            SemanticActual::Type(SemanticTypeValue::Core {
                source: SemanticCoreTypeSource::Expression {
                    expression: NodeId(17),
                    ..
                },
                ..
            })
        ));
        let marker = ledger
            .record(
                instance(0),
                NodeId(4),
                SemanticSiteId(1),
                SemanticEmissionRole::Binding,
                Action::DeclareLocal(SemanticTypeRecipe::Atom(Type::I32)),
                &mut budget,
                Span::default(),
            )
            .unwrap();
        let statement = CoreStatement {
            id: NodeId(18),
            span: Span::default(),
            kind: CoreStatementKind::Binding {
                name: "uninit".into(),
                mutable: true,
                ty: Some(Type::I32),
                initializer: None,
            },
        };
        let site = ledger
            .statement(marker, instance(0), NodeId(4), &statement, 0, &mut budget)
            .unwrap();
        assert!(matches!(
            site.action,
            SemanticSiteAction::DeclareLocal {
                local: SemanticLocalTarget::Core {
                    statement: NodeId(18),
                    ..
                },
                ..
            }
        ));
        ledger.finish(&mut budget, Span::default()).unwrap();
    }
    #[test]
    fn original_owner_binds_only_the_actual_creator_before_consumption() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut ledger = Ledger::default();
        let definition = SemanticDefinitionId {
            module: ModuleId(0),
            index: 7,
        };
        ledger
            .original_owner(instance(0), definition, &mut budget, Span::default())
            .unwrap();
        let marker = ledger
            .record_original(
                instance(0),
                vec![event(
                    0,
                    SemanticEmissionRole::Ascription,
                    Action::TypeUse(SemanticTypeRecipe::Atom(Type::I32)),
                )],
                &mut budget,
                Span::default(),
            )
            .unwrap();
        assert!(
            ledger
                .expression(
                    marker,
                    instance(0),
                    NodeId(4),
                    &tuple(),
                    0,
                    SemanticEmissionRole::Ascription,
                    &mut budget
                )
                .is_err()
        );
        assert!(
            ledger
                .bind_creator(
                    instance(0),
                    SemanticDefinitionId {
                        index: 8,
                        ..definition
                    },
                    NodeId(4),
                    &mut budget,
                    Span::default()
                )
                .is_err()
        );
        ledger
            .bind_creator(
                instance(0),
                definition,
                NodeId(4),
                &mut budget,
                Span::default(),
            )
            .unwrap();
        assert!(
            ledger
                .bind_creator(
                    instance(0),
                    definition,
                    NodeId(4),
                    &mut budget,
                    Span::default()
                )
                .is_err()
        );
        ledger
            .expression(
                marker,
                instance(0),
                NodeId(4),
                &tuple(),
                0,
                SemanticEmissionRole::Ascription,
                &mut budget,
            )
            .unwrap();
        ledger.finish(&mut budget, Span::default()).unwrap();
    }
    #[test]
    fn callable_and_closure_events_select_actual_operands_not_declaration_numbers() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut ledger = Ledger::default();
        let marker = ledger
            .record(
                instance(0),
                NodeId(4),
                SemanticSiteId(0),
                SemanticEmissionRole::Call,
                Action::CallableCall,
                &mut budget,
                Span::default(),
            )
            .unwrap();
        let expression = CoreExpr {
            id: NodeId(15),
            span: Span::default(),
            kind: CoreExprKind::Call {
                callee: Box::new(tuple()),
                arguments: Vec::new(),
            },
        };
        let site = ledger
            .expression(
                marker,
                instance(0),
                NodeId(4),
                &expression,
                0,
                SemanticEmissionRole::Call,
                &mut budget,
            )
            .unwrap();
        assert!(matches!(
            site.action,
            SemanticSiteAction::CallableCall {
                callee: SemanticOperand::CoreExpression {
                    expression: NodeId(12),
                    occurrence: 0
                }
            }
        ));
        let marker = ledger
            .record(
                instance(0),
                NodeId(4),
                SemanticSiteId(1),
                SemanticEmissionRole::Closure,
                Action::Closure {
                    environment: vec![SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(
                        SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: 3,
                        },
                    ))],
                },
                &mut budget,
                Span::default(),
            )
            .unwrap();
        let expression = CoreExpr {
            id: NodeId(16),
            span: Span::default(),
            kind: CoreExprKind::Closure {
                move_capture: false,
                parameters: Vec::new(),
                return_type: None,
                body: CoreBlock {
                    statements: Vec::new(),
                    span: Span::default(),
                },
            },
        };
        let site = ledger
            .expression(
                marker,
                instance(0),
                NodeId(4),
                &expression,
                0,
                SemanticEmissionRole::Closure,
                &mut budget,
            )
            .unwrap();
        let SemanticSiteAction::Closure {
            closure,
            environment,
            ..
        } = site.action
        else {
            panic!()
        };
        assert!(matches!(
            closure,
            SemanticClosureTarget::Core {
                creator: NodeId(4),
                expression: NodeId(16),
                ..
            }
        ));
        assert!(
            matches!(&environment[0], SemanticActual::CoreRecipe { scope, recipe } if *scope == instance(0) && matches!(recipe.as_ref(), SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(SemanticFormalRef { scope: SemanticScopeRef::Ancestor(0), ordinal: 3 }))))
        );
        ledger.finish(&mut budget, Span::default()).unwrap();
    }
    #[test]
    fn repeated_template_clones_mint_independent_actual_events_in_the_true_owner() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let mut ledger = Ledger::default();
        let definition = SemanticDefinitionId {
            module: ModuleId(0),
            index: 7,
        };
        let original = ledger
            .template(
                definition,
                vec![event(
                    0,
                    SemanticEmissionRole::Ascription,
                    Action::TypeUse(SemanticTypeRecipe::Tuple(Vec::new())),
                )],
                &mut budget,
                Span::default(),
            )
            .unwrap();
        ledger
            .original_owner(instance(0), definition, &mut budget, Span::default())
            .unwrap();
        ledger
            .original_owner(instance(1), definition, &mut budget, Span::default())
            .unwrap();
        ledger
            .original_owner(
                instance(2),
                SemanticDefinitionId {
                    index: 8,
                    ..definition
                },
                &mut budget,
                Span::default(),
            )
            .unwrap();
        assert!(
            ledger
                .instantiate_template(original, instance(2), &mut budget, Span::default())
                .is_err()
        );
        let first = ledger
            .instantiate_template(original, instance(0), &mut budget, Span::default())
            .unwrap();
        let second = ledger
            .instantiate_template(original, instance(1), &mut budget, Span::default())
            .unwrap();
        assert_ne!(first.raw_index(), second.raw_index());
        assert_ne!(first.raw_index(), original.raw_index());
        ledger
            .bind_creator(
                instance(0),
                definition,
                NodeId(4),
                &mut budget,
                Span::default(),
            )
            .unwrap();
        ledger
            .bind_creator(
                instance(1),
                definition,
                NodeId(5),
                &mut budget,
                Span::default(),
            )
            .unwrap();
        ledger
            .bind_creator(
                instance(2),
                SemanticDefinitionId {
                    index: 8,
                    ..definition
                },
                NodeId(6),
                &mut budget,
                Span::default(),
            )
            .unwrap();
        assert!(
            ledger
                .expression(
                    original,
                    instance(0),
                    NodeId(4),
                    &tuple(),
                    0,
                    SemanticEmissionRole::Ascription,
                    &mut budget
                )
                .is_err()
        );
        assert!(
            ledger
                .expression(
                    first,
                    instance(1),
                    NodeId(5),
                    &tuple(),
                    0,
                    SemanticEmissionRole::Ascription,
                    &mut budget
                )
                .is_err()
        );
        ledger
            .expression(
                first,
                instance(0),
                NodeId(4),
                &tuple(),
                0,
                SemanticEmissionRole::Ascription,
                &mut budget,
            )
            .unwrap();
        let second_expression = CoreExpr {
            id: NodeId(13),
            ..tuple()
        };
        ledger
            .expression(
                second,
                instance(1),
                NodeId(5),
                &second_expression,
                0,
                SemanticEmissionRole::Ascription,
                &mut budget,
            )
            .unwrap();
        ledger.finish(&mut budget, Span::default()).unwrap();
    }
}
