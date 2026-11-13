//! Exact original owner lookup for the shared semantic type vocabulary. A
//! successful lookup is metadata only, never type evaluation or a W proof.
use super::*;

pub(super) fn owner<'a>(
    module: &'a TypedModule,
    expected: &SemanticDynOwner,
    requested: SemanticDefinitionId,
    b: &mut Budget,
) -> Check<&'a SemanticDefinition> {
    let semantic = module
        .semantic
        .as_ref()
        .ok_or_else(|| bad("semantic type bridge original program is Unknown"))?;
    b.spend(
        semantic
            .dyn_scopes
            .len()
            .saturating_add(semantic.definitions.len()),
    )?;
    let mut bindings = semantic
        .dyn_scopes
        .iter()
        .filter(|row| &row.owner == expected);
    let binding = bindings
        .next()
        .ok_or_else(|| bad("semantic type bridge has no exact original Dyn owner binding"))?;
    if bindings.next().is_some() || binding.scope != requested {
        return Err(bad(
            "semantic type bridge selects a different or duplicate original owner scope",
        ));
    }
    let mut definitions = semantic
        .definitions
        .iter()
        .filter(|definition| definition.id == requested);
    let definition = definitions
        .next()
        .ok_or_else(|| bad("semantic bridge owner definition is absent"))?;
    if definitions.next().is_some() {
        return Err(bad(
            "semantic bridge owner definition identity is duplicated",
        ));
    }
    match expected {
        SemanticDynOwner::Declaration(id) => match &definition.body {
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::Trait { declaration }
                | SemanticScopeRecipe::InherentImpl { declaration, .. }
                | SemanticScopeRecipe::TraitImpl { declaration, .. },
            ) if declaration == id => {}
            _ => {
                return Err(bad(
                    "semantic bridge declaration binding is not its actual lexical scope",
                ));
            }
        },
        SemanticDynOwner::MethodSignature { .. } | SemanticDynOwner::MethodDefinition(_) => {
            if definition.kind != SemanticDefinitionKind::Method
                || !matches!(definition.body, SemanticDefinitionBody::Function { .. })
            {
                return Err(bad(
                    "semantic bridge method binding is not an original method scope",
                ));
            }
        }
    }
    Ok(definition)
}
