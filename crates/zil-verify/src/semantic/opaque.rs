//! Instantiation of an already universally checked opaque result. Only its
//! explicit actuals cross the boundary; hidden names remain in the callee's
//! real lexical domain. Actual creator seals and memory proofs stay pending.
use super::*;

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn opaque_type(&self, module: &TypedModule, owner: SemanticDefinitionId, arguments: &[SemanticActualRecipe], hidden: &SemanticTypeRecipe, caller: &EvalEnv, external: &mut impl ExternalProofs, active: &mut ActiveQueries, b: &mut Budget) -> Check<CheckedType> {
        let definition = self.definition(module, owner, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else { return Err(bad("opaque owner omits its complete original function")); };
        if !matches!(&signature.result, SemanticTypeRecipe::Opaque { definition: actual, hidden: original, .. } if *actual == owner && original.as_ref() == hidden)
            || !matches!(hidden, SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result })
            || arguments.len() != definition.parameters.len() {
            return Err(bad("opaque result changes its defining role or complete mixed actuals"));
        }
        b.spend(arguments.len())?;
        let mut actuals = Vec::new();
        for (formal, recipe) in definition.parameters.iter().zip(arguments) {
            let actual = self.evaluate_actual(module, recipe, caller, external, active, b)?;
            if !matches!((formal, &actual), (SemanticFormal::Type { .. }, CheckedActual::Type(_)) | (SemanticFormal::Lifetime, CheckedActual::Life(_)) | (SemanticFormal::Const { .. }, CheckedActual::Const { .. })) {
                return Err(bad("opaque argument changes its original full-ordinal kind"));
            }
            actuals.push(actual);
        }
        let mut environment = caller.clone();
        b.spend(caller.lexical.len())?;
        environment.lexical = if let Some(parent) = definition.parent {
            let index = caller.lexical.iter().position(|row| row.definition == parent).ok_or_else(|| unsupported("opaque result cannot reconstruct its true lexical parent", None))?;
            caller.lexical[index..].to_vec()
        } else { vec![] };
        let mut sealed = Vec::new();
        for formal in &definition.environment {
            sealed.push(self.evaluate_actual(module, &formal.source, &environment, external, active, b)?);
        }
        let key = match caller.lexical.iter().find(|row| row.definition == owner && row.actuals == actuals && row.environment == sealed) {
            Some(row) => row.key,
            None => ScopeKey::Witness { definition: owner, frame: active.fresh(b)? },
        };
        environment.lexical.insert(0, LexicalFrame { definition: owner, key, actuals, environment: sealed, self_type: None });
        for formal in &definition.parameters {
            if let SemanticFormal::Type { requirements } = formal {
                self.evaluate_requirements(module, requirements, &environment, RequirementMode::ActualObligation, external, active, b)?;
            }
        }
        let recipe = self.original.borrow().as_ref().ok_or_else(|| bad("opaque result has no independent original proof"))?.opaque_result_recipe(module, owner, &environment, b)?;
        self.evaluate_type(module, &recipe, &environment, external, active, b)
    }
}
