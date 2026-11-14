//! Self comes from the checked original implementation and its actual parent
//! instance. The instance's self_type row is only a claim to compare with it.
use super::*;
impl Eval<'_, '_, '_> {
    pub(super) fn self_claim(
        &mut self,
        scope: SemanticInstanceId,
        claim: &SemanticTypeValue,
    ) -> R<SemanticTypeValue> {
        let instance = self.instance(scope)?;
        let original = self.definition(instance.definition)?;
        let (selector, parent) = match &original.body {
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::InherentImpl { .. }
                | SemanticScopeRecipe::TraitImpl { .. }
                | SemanticScopeRecipe::BuiltinImpl { .. },
            ) => (SemanticScopeRef::Ancestor(0), None),
            SemanticDefinitionBody::Function { .. }
            | SemanticDefinitionBody::FunctionWithBody { .. }
                if original.kind == SemanticDefinitionKind::Method =>
            {
                let parent = instance
                    .parent
                    .ok_or_else(|| bad("method Self lacks its actual implementation parent"))?;
                let parent_instance = self.instance(parent)?;
                let parent_definition = self.definition(parent_instance.definition)?;
                if original.parent != Some(parent_instance.definition)
                    || !matches!(
                        parent_definition.body,
                        SemanticDefinitionBody::Scope(
                            SemanticScopeRecipe::InherentImpl { .. }
                                | SemanticScopeRecipe::TraitImpl { .. }
                                | SemanticScopeRecipe::BuiltinImpl { .. }
                        )
                    )
                {
                    return Err(bad(
                        "method Self does not name its true original implementation parent",
                    ));
                }
                (SemanticScopeRef::Ancestor(1), Some(parent))
            }
            _ => {
                return Err(unknown(
                    "Self requires its complete actual implementation/provider boundary",
                ));
            }
        };
        // Derive without a claimed physical Type hint or a self_type seed.
        let expected = self.ty(
            scope,
            &SemanticTypeRecipe::SelfType { scope: selector },
            None,
            0,
        )?;
        let SemanticTypeValue::Resolved { ty, .. } = &expected else {
            unreachable!()
        };
        let mut actual = self.value(scope, claim, Some(ty), 0)?;
        if let SemanticTypeValue::Core {
            scope: claimed_scope,
            ..
        } = claim
        {
            if *claimed_scope == scope {
                // The claim is already expressed in this method/scope domain.
            } else if Some(*claimed_scope) == parent {
                self.rehome_classes(&mut actual, 1)?;
            } else {
                return Err(bad(
                    "Self claim imports a different actual lexical instance",
                ));
            }
        }
        crate::dyn_metadata::semantic_operand_snapshot(&expected, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&actual, self.limits, self.left, 1)?;
        if expected != actual {
            return Err(bad(
                "Self claim differs from its complete original implementation type and classes",
            ));
        }
        Ok(expected)
    }
}
