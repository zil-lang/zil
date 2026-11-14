//! An opaque result keeps its defining identity and ordered actuals. Its
//! hidden term is checked in that definition's domain, never the caller's
//! same-numbered Result, Implicit or local namespace.
use super::*;

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn opaque_type(&mut self, module: &TypedModule, context: &CheckedContext, owner: SemanticDefinitionId, arguments: &[SemanticActualRecipe], hidden: &SemanticTypeRecipe, caller: &Env, active: &mut ActiveQueries, b: &mut Budget) -> Check<Ty> {
        let original = context.definition(module, owner, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &original.body else { return Err(bad("original opaque owner has no complete original function")); };
        if !matches!(&signature.result, SemanticTypeRecipe::Opaque { definition, hidden: declared, .. } if *definition == owner && declared.as_ref() == hidden)
            || !matches!(hidden, SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result })
            || original.parameters.len() != arguments.len() {
            return Err(unsupported("original opaque result lacks its exact defining result role and full ordered arguments", None));
        }
        b.spend(arguments.len())?;
        let mut actuals = Vec::new();
        for (formal, recipe) in original.parameters.iter().zip(arguments) {
            let actual = self.actual(module, context, recipe, caller, active, b)?;
            if !matches!((formal, &actual), (SemanticFormal::Type { .. }, Arg::Type(_)) | (SemanticFormal::Lifetime, Arg::Life(_)) | (SemanticFormal::Const { .. }, Arg::Const(_))) {
                return Err(bad("original opaque argument changes its complete formal kind"));
            }
            actuals.push(actual);
        }
        Ok(Ty::Opaque(owner, actuals))
    }

    pub(super) fn opaque_hidden(&mut self, module: &TypedModule, context: &CheckedContext, owner: SemanticDefinitionId, arguments: &[Arg], active: &mut ActiveQueries, b: &mut Budget) -> Check<Ty> {
        if !self.checked.contains(&owner) {
            if self.active.contains(&owner) { return Err(unsupported("recursive opaque result has no completed original role proof", None)); }
            self.definition(module, context, owner, active, b)?;
        }
        let original = context.definition(module, owner, b)?;
        if original.parameters.len() != arguments.len() { return Err(bad("opaque result omits a complete original argument")); }
        let hidden = self.resolved(self.results.get(&owner).ok_or_else(|| bad("opaque owner has no checked result role"))?, b)?;
        if usage::has_inference(&hidden, &self.substitutions, b)? { return Err(bad("opaque owner result remains unresolved after its original body")); }
        let mut selected = self.environment(module, context, owner, active, b)?;
        selected.lexical[0].actuals = self.resolve_arguments(arguments, 0, b)?;
        self.apply_type(&hidden, &selected, b)
    }

    pub(super) fn opaque_callable(&mut self, module: &TypedModule, context: &CheckedContext, owner: SemanticDefinitionId, arguments: &[Arg], active: &mut ActiveQueries, b: &mut Budget) -> Check<Option<Prototype>> {
        // A declared result capability is usable only after all original
        // returns have established it. It never yields TypeEqual or LifeEqual.
        self.opaque_hidden(module, context, owner, arguments, active, b)?;
        let original = context.definition(module, owner, b)?;
        let mut selected = self.environment(module, context, owner, active, b)?;
        selected.lexical[0].actuals = self.resolve_arguments(arguments, 0, b)?;
        let mut result = None;
        for row in &original.requirements {
            if let SemanticRequirementRecipe::Callable { ty: SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result }, kind, signature } = row {
                if result.is_some() { return Err(unsupported("opaque Callable overload requires its exact original role selection", None)); }
                let rigid = self.environment(module, context, owner, active, b)?;
                let mut prototype = self.signature(module, context, signature, &rigid, active, b)?;
                prototype.callable_kind = Some(*kind);
                result = Some(self.instantiate_prototype(&prototype, Some(owner), &selected, b)?);
            }
        }
        Ok(result)
    }

    pub(super) fn check_opaque_result(&mut self, module: &TypedModule, context: &CheckedContext, owner: SemanticDefinitionId, env: &Env, active: &mut ActiveQueries, b: &mut Budget) -> Check<()> {
        let original = context.definition(module, owner, b)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &original.body else { return Ok(()); };
        if !matches!(&signature.result, SemanticTypeRecipe::Opaque { definition, .. } if *definition == owner) { return Ok(()); }
        let hidden = self.results.get(&owner).ok_or_else(|| bad("opaque result has no original role"))?;
        if usage::has_inference(hidden, &self.substitutions, b)? { return Err(bad("opaque result is not determined by every original return")); }
        for row in &original.requirements {
            if matches!(row, SemanticRequirementRecipe::Callable { ty: SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result }, .. }) {
                self.obligation(module, context, row, env, env, active, b)?;
            }
        }
        Ok(())
    }
}
