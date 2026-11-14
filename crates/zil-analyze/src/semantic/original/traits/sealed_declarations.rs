//! Legacy raw builtin-impl identities are not user trait declarations. Their
//! full original obligations live in the exact BuiltinImpl and its methods;
//! recognizing this link does not establish a structural capability.
use super::*;

pub(super) fn validate(
    raw: &DynDeclaration,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<bool> {
    let mut owner = None;
    for definition in inventory.definitions.values() {
        b.spend(1, WORK)?;
        if !matches!(
            definition.body,
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl { .. })
        ) {
            continue;
        }
        crate::dyn_metadata::recipe(*definition, b)?;
        crate::dyn_metadata::recipe_declaration(raw, b)?;
        if definition.identity == raw.identity && owner.replace(*definition).is_some() {
            return Err(fail(
                "raw builtin implementation has multiple exact original owners",
            ));
        }
    }
    let Some(owner) = owner else {
        return Ok(false);
    };
    if raw.kind != DynDeclarationKind::Impl
        || raw.implicit_lifetimes.is_some()
        || raw.direct_supertraits.is_some()
        || raw.requirements.is_some()
        || raw.associated_definitions.is_some()
        || raw.method_signatures.is_some()
        || raw.method_definitions.is_some()
        || raw.self_pattern.is_some()
        || raw.implemented_trait.is_some()
        || raw.parameters.len() != owner.parameters.len()
    {
        return Err(fail(
            "raw builtin identity changes its original declaration representation",
        ));
    }
    for (kind, formal) in raw.parameters.iter().zip(&owner.parameters) {
        b.spend(1, WORK)?;
        if !matches!(
            (kind, formal),
            (LifeParameterKind::Type, SemanticFormal::Type { .. })
                | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime)
                | (LifeParameterKind::Const(_), SemanticFormal::Const { .. })
        ) {
            return Err(fail(
                "raw builtin identity changes the original mixed formal kinds",
            ));
        }
    }
    let mut seen = BTreeSet::new();
    for member in &raw.members {
        b.spend(member.name.len().saturating_add(seen.len() + 1), WORK)?;
        if !seen.insert(member.name.as_str()) {
            return Err(fail("raw builtin identity repeats a member"));
        }
        let DynMemberKind::Method(parameters) = &member.kind else {
            return Err(unknown(
                "raw builtin associated member requires its complete original recipe",
            ));
        };
        let mut method = None;
        for definition in inventory.definitions.values() {
            b.spend(1, WORK)?;
            if definition.parent == Some(owner.id)
                && definition.identity.declaration_path.last() == Some(&member.name)
            {
                if method.replace(*definition).is_some() {
                    return Err(fail(
                        "raw builtin member has multiple exact original methods",
                    ));
                }
            }
        }
        let method =
            method.ok_or_else(|| fail("raw builtin member has no exact original method"))?;
        if method.kind != SemanticDefinitionKind::Method
            || !matches!(method.body, SemanticDefinitionBody::FunctionWithBody { .. })
            || parameters.len() != method.parameters.len()
        {
            return Err(fail(
                "raw builtin member changes its original method kind or mixed arity",
            ));
        }
        for (kind, formal) in parameters.iter().zip(&method.parameters) {
            b.spend(1, WORK)?;
            if !matches!(
                (kind, formal),
                (LifeParameterKind::Type, SemanticFormal::Type { .. })
                    | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime)
                    | (LifeParameterKind::Const(_), SemanticFormal::Const { .. })
            ) {
                return Err(fail("raw builtin method changes its mixed formal kinds"));
            }
        }
    }
    for method in inventory.definitions.values() {
        b.spend(1, WORK)?;
        if method.parent == Some(owner.id) && method.kind == SemanticDefinitionKind::Method {
            let name = method
                .identity
                .declaration_path
                .last()
                .ok_or_else(|| fail("original builtin method has no member identity"))?;
            b.spend(name.len().saturating_add(seen.len()), WORK)?;
            if !seen.contains(name.as_str()) {
                return Err(fail("raw builtin identity omits an original method"));
            }
        }
    }
    Ok(true)
}
