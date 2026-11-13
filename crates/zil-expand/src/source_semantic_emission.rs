//! Linear proposals consumed only after the real Core node has been minted.
//! A token is a request-local address, never a provenance/capability proof.
use super::*;
use zil_base::NodeId;

pub(crate) struct Actual {
    pub(crate) recipe: SemanticActualRecipe,
    /// Actual argument ordinal, not a same-shaped expression found later.
    pub(crate) inferred_argument: Option<u32>,
}
pub(crate) enum Action {
    Construct(SemanticTypeRecipe),
    TypeUse(SemanticTypeRecipe),
    DeclareLocal(SemanticTypeRecipe),
    Call {
        target: SemanticFunctionTarget,
        actuals: Vec<Actual>,
    },
    CallableCall,
    Closure {
        environment: Vec<SemanticActualRecipe>,
    },
}
struct Pending {
    instance: SemanticInstanceId,
    creator: NodeId,
    recipe: SemanticSiteId,
    role: SemanticEmissionRole,
    action: Action,
}
#[derive(Default)]
pub(crate) struct Ledger {
    entries: Vec<Option<Pending>>,
    emitted: Vec<(SemanticInstanceId, NodeId, u32, SemanticEmissionRole)>,
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
        budget.node(span, 0)?;
        let token = u32::try_from(self.entries.len())
            .map_err(|_| unavailable("source semantic token overflow"))?;
        if token == u32::MAX {
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
            recipe,
            role,
            action,
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
    ) -> R<Pending> {
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
        if pending.instance != instance || pending.creator != creator || pending.role != role {
            return Err(unavailable(
                "source semantic production belongs to another actual owner or role",
            ));
        }
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
        Ok(self.entries[token as usize]
            .take()
            .expect("actual source production checked"))
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
                                expression: argument.value.id,
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
        budget.identity_work(self.entries.len(), span)?;
        if self.entries.iter().any(Option::is_some) {
            return Err(unavailable(
                "source semantic production has no actual emitted node",
            ));
        }
        Ok(())
    }
}
