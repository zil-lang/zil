//! A result boundary is selected by original operation provenance. Hidden
//! recipes retain their callee lexical domain until the actual call is checked.
use super::*;

#[derive(Clone)]
pub(crate) struct CalleeResultBoundary {
    pub caller: SemanticDefinitionId,
    pub call: SemanticSiteId,
    pub callee: SemanticDefinitionId,
    pub actuals: Vec<SemanticActualRecipe>,
    /// Complete all-return result in the original callee definition domain.
    pub result: SemanticTypeRecipe,
}

impl Check<'_, '_> {
    pub(super) fn alias_result_origin(
        &mut self,
        owner: SemanticDefinitionId,
        target: Query,
        source: &Query,
        depth: usize,
    ) -> R<()> {
        self.pay(source.copy_work().saturating_add(target.copy_work()), depth)?;
        let Some(origin) = self
            .proof
            .call_result_origins
            .get(&(owner, source.clone()))
            .copied()
        else {
            return Ok(());
        };
        if let Some(previous) = self.proof.call_result_origins.get(&(owner, target.clone())) {
            if *previous != origin {
                return Err(pending(
                    "original result alias requires its complete control-flow boundary family",
                ));
            }
        }
        self.pay(1, depth)?;
        self.proof
            .call_result_origins
            .insert((owner, target), origin);
        Ok(())
    }

    pub(super) fn expression_result_origin(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        // This receipt describes a whole opaque value. Aggregates or projections
        // need their own complete component boundary, and cannot borrow a
        // same-shaped call elsewhere in the body.
        if !matches!(result, SemanticTypeRecipe::Opaque { .. }) {
            return Ok(());
        }
        let target = Query::Expression(expression.id);
        match &expression.kind {
            CoreExprKind::Call { .. } => {
                let Some(site) =
                    self.site(owner, SemanticOriginalNode::Expression(expression.id))?
                else {
                    return Err(bad(
                        "original opaque result call loses its actual operation site",
                    ));
                };
                if self.proof.call_targets.contains_key(&(owner, site.id)) {
                    self.pay(1, depth)?;
                    self.proof
                        .call_result_origins
                        .insert((owner, target), (owner, site.id));
                }
            }
            CoreExprKind::Name(name) => {
                self.pay(name.len(), depth)?;
                let binding = variables
                    .get(name)
                    .ok_or_else(|| bad("original opaque alias loses its lexical binder"))?;
                // Crossing a closure boundary must use its sealed whole Type
                // environment, never a caller same-numbered operation.
                if binding.origin_owner == owner {
                    self.pay(binding.origin.copy_work(), depth)?;
                    let origin = binding.origin.clone();
                    self.alias_result_origin(owner, target, &origin, depth + 1)?;
                }
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.alias_result_origin(owner, target, &Query::Expression(value.id), depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Proof {
    pub(in crate::semantics) fn call_result_boundary(
        &self,
        current: SemanticDefinitionId,
        owner: SemanticDefinitionId,
        source: &SemanticOriginalTypeSource,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<CalleeResultBoundary>> {
        // An ancestor query is interpreted only after the consumer has selected
        // that real ancestor instance. Sealed closure environments remain on
        // their existing checked path.
        if current != owner {
            return Ok(None);
        }
        let source_work = match source {
            SemanticOriginalTypeSource::BindingProjection { pattern, .. } => {
                pattern.len().saturating_add(1)
            }
            SemanticOriginalTypeSource::NameAt { name, .. } => name.len().saturating_add(1),
            _ => 1,
        };
        *left = left
            .checked_sub(source_work.saturating_add(4))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original result boundary work exceeded", None)
            })?;
        let query = Query::from(source.clone());
        let Some(&(caller, call)) = self.call_result_origins.get(&(owner, query)) else {
            return Ok(None);
        };
        if caller != owner || !self.complete.contains(&caller) {
            return Err(bad(
                "original result boundary escapes its proved caller domain",
            ));
        }
        let callee = *self
            .call_targets
            .get(&(caller, call))
            .ok_or_else(|| bad("original result boundary has no real callee binding"))?;
        if !self.complete.contains(&callee) {
            return Err(bad(
                "original result boundary callee has no complete body proof",
            ));
        }
        let signature = self
            .direct_calls
            .get(&(caller, call))
            .or_else(|| {
                self.method_calls
                    .get(&(caller, call))
                    .map(|call| &call.signature)
            })
            .ok_or_else(|| bad("original result boundary lacks its full solved call scheme"))?;
        let SemanticTypeRecipe::Opaque { definition, .. } = &signature.result else {
            return Err(bad(
                "original opaque result boundary changes its call result constructor",
            ));
        };
        if *definition != callee {
            return Err(pending(
                "original opaque result needs its complete provider definition boundary",
            ));
        }
        let actuals = self
            .calls
            .get(&(caller, call))
            .ok_or_else(|| bad("original result boundary omits its full mixed call actuals"))?;
        let result = self.types.get(&(callee, Query::Result)).ok_or_else(|| {
            bad("original result boundary lacks its complete all-return equation")
        })?;
        *left = left
            .checked_sub(actuals.len().saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original result boundary actual work exceeded",
                    None,
                )
            })?;
        for actual in actuals {
            crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(result, limits, left, 1)?;
        Ok(Some(CalleeResultBoundary {
            caller,
            call,
            callee,
            actuals: actuals.clone(),
            result: result.clone(),
        }))
    }
}
