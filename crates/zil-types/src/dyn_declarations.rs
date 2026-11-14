//! Original declaration directories remain separate from runtime admission.
use super::*;

// Validate the complete original directory without requiring object safety or
// a runtime instance. Runtime interface/table admission remains below.
pub(super) fn original_declaration(
    core: &CoreModule,
    declaration: &DynDeclaration,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let spend = |left: &mut usize, count: usize| -> R<()> {
        *left = left.checked_sub(count).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original dyn directory work exceeded", None)
        })?;
        Ok(())
    };
    crate::dyn_metadata::actual_declaration_snapshot(declaration, limits, left, 1)?;
    let semantic = core.semantic.as_ref().ok_or_else(|| {
        unsupported(
            Span::default(),
            "original dyn directory has no lexical semantic source",
        )
    })?;
    spend(
        left,
        semantic
            .dyn_scopes
            .len()
            .saturating_add(semantic.definitions.len()),
    )?;
    let mut scopes = semantic
        .dyn_scopes
        .iter()
        .filter(|row| row.owner == SemanticDynOwner::Declaration(declaration.id));
    let scope = scopes.next().ok_or_else(|| {
        unsupported(
            Span::default(),
            "original dyn declaration has no exact scope binding",
        )
    })?;
    if scopes.next().is_some() {
        return Err(error(
            "E-DYN-DECLARATION",
            Span::default(),
            "original dyn declaration repeats its scope binding",
        ));
    }
    let mut definitions = semantic
        .definitions
        .iter()
        .filter(|row| row.id == scope.scope);
    let original = definitions
        .next()
        .ok_or_else(|| unsupported(Span::default(), "original dyn declaration scope is absent"))?;
    if definitions.next().is_some() {
        return Err(error(
            "E-DYN-DECLARATION",
            Span::default(),
            "original dyn scope definition is ambiguous",
        ));
    }
    let kind = matches!((&original.body, declaration.kind),
        (SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration: id }), DynDeclarationKind::Trait)
        | (SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl { declaration: id, .. }), DynDeclarationKind::Impl)
        | (SemanticDefinitionBody::Scope(SemanticScopeRecipe::InherentImpl { declaration: id, .. }), DynDeclarationKind::Impl) if *id == declaration.id);
    spend(
        left,
        original
            .parameters
            .len()
            .saturating_add(declaration.parameters.len()),
    )?;
    directory_identity_work(&original.identity, left)?;
    if !kind
        || original.identity != declaration.identity
        || original.parameters.len() != declaration.parameters.len()
        || !original
            .parameters
            .iter()
            .zip(&declaration.parameters)
            .all(|(formal, kind)| {
                matches!(
                    (formal, kind),
                    (SemanticFormal::Type { .. }, LifeParameterKind::Type)
                        | (SemanticFormal::Const { .. }, LifeParameterKind::Const(_))
                        | (SemanticFormal::Lifetime, LifeParameterKind::Lifetime)
                )
            })
    {
        return Err(error(
            "E-DYN-DECLARATION",
            Span::default(),
            "dyn directory changes its original declaration identity or mixed formal kinds",
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    let mut name_work = 0usize;
    let mut method_count = 0usize;
    for (ordinal, member) in declaration.members.iter().enumerate() {
        spend(
            left,
            names
                .len()
                .saturating_add(member.name.len())
                .saturating_add(semantic.definitions.len())
                .saturating_add(1),
        )?;
        spend(
            left,
            name_work.saturating_add(names.len().saturating_mul(member.name.len())),
        )?;
        name_work = name_work.saturating_add(member.name.len());
        if !names.insert(member.name.as_str()) {
            return Err(error(
                "E-DYN-DECLARATION",
                Span::default(),
                "original dyn directory repeats a member name",
            ));
        }
        let DynMemberKind::Method(parameters) = &member.kind else {
            continue;
        };
        method_count += 1;
        let mut selected = None;
        for child in &semantic.definitions {
            if child.parent != Some(original.id) || child.kind != SemanticDefinitionKind::Method {
                continue;
            }
            directory_identity_work(&child.identity, left)?;
            directory_identity_work(&original.identity, left)?;
            let prefix = &original.identity.declaration_path;
            let path = &child.identity.declaration_path;
            spend(
                left,
                prefix
                    .len()
                    .saturating_add(path.len())
                    .saturating_add(member.name.len())
                    .saturating_add(32),
            )?;
            if child.identity.package == original.identity.package
                && child.identity.module_path == original.identity.module_path
                && child.identity.generation_path == original.identity.generation_path
                && path.len() == prefix.len().saturating_add(3)
                && path.starts_with(prefix)
                && path[prefix.len()] == "member"
                && path[prefix.len() + 1] == ordinal.to_string()
                && path[prefix.len() + 2] == member.name
            {
                if selected.replace(child).is_some() {
                    return Err(error(
                        "E-DYN-DECLARATION",
                        Span::default(),
                        "original dyn method identity is ambiguous",
                    ));
                }
            }
        }
        let child = selected.ok_or_else(|| {
            unsupported(
                Span::default(),
                "dyn member has no complete original method definition",
            )
        })?;
        spend(
            left,
            child.parameters.len().saturating_add(parameters.len()),
        )?;
        if child.parameters.len() != parameters.len()
            || !child
                .parameters
                .iter()
                .zip(parameters)
                .all(|(formal, kind)| {
                    matches!(
                        (formal, kind),
                        (SemanticFormal::Type { .. }, LifeParameterKind::Type)
                            | (SemanticFormal::Const { .. }, LifeParameterKind::Const(_))
                            | (SemanticFormal::Lifetime, LifeParameterKind::Lifetime)
                    )
                })
        {
            return Err(error(
                "E-DYN-DECLARATION",
                Span::default(),
                "dyn method directory changes its original mixed formals",
            ));
        }
        let signature = match &child.body {
            SemanticDefinitionBody::Function { signature, .. }
            | SemanticDefinitionBody::FunctionWithBody { signature, .. } => signature,
            _ => {
                return Err(error(
                    "E-DYN-DECLARATION",
                    Span::default(),
                    "dyn method has no original signature",
                ));
            }
        };
        let mut found = false;
        let mut has_signature = false;
        let mut has_definition = false;
        if let Some(rows) = &declaration.method_signatures {
            spend(left, rows.len())?;
            for row in rows.iter().filter(|row| row.member as usize == ordinal) {
                if found {
                    return Err(error(
                        "E-DYN-DECLARATION",
                        Span::default(),
                        "dyn method signature directory repeats a member",
                    ));
                }
                directory_signature(&row.signature, child.id, signature, limits, left)?;
                directory_method_scope(
                    semantic,
                    SemanticDynOwner::MethodSignature {
                        declaration: declaration.id,
                        member: row.member,
                    },
                    child.id,
                    left,
                )?;
                found = true;
                has_signature = true;
            }
        }
        if let Some(rows) = &declaration.method_definitions {
            spend(left, rows.len())?;
            let mut defined = false;
            for row in rows.iter().filter(|row| row.member as usize == ordinal) {
                directory_identity_work(&child.identity, left)?;
                if defined || row.identity != child.identity {
                    return Err(error(
                        "E-DYN-DECLARATION",
                        Span::default(),
                        "dyn method definition directory changes its original identity",
                    ));
                }
                directory_signature(&row.signature, child.id, signature, limits, left)?;
                directory_method_scope(
                    semantic,
                    SemanticDynOwner::MethodDefinition(row.id),
                    child.id,
                    left,
                )?;
                defined = true;
                found = true;
                has_definition = true;
            }
        }
        if !found
            || declaration.kind == DynDeclarationKind::Trait && !has_signature
            || declaration.kind == DynDeclarationKind::Impl && !has_definition
        {
            return Err(unsupported(
                Span::default(),
                "dyn method directory omits its complete original signature",
            ));
        }
    }
    spend(left, semantic.definitions.len())?;
    if semantic
        .definitions
        .iter()
        .filter(|child| {
            child.parent == Some(original.id) && child.kind == SemanticDefinitionKind::Method
        })
        .count()
        != method_count
    {
        return Err(error(
            "E-DYN-DECLARATION",
            Span::default(),
            "dyn directory omits an original method",
        ));
    }
    for member in declaration
        .method_signatures
        .iter()
        .flatten()
        .map(|row| row.member)
        .chain(
            declaration
                .method_definitions
                .iter()
                .flatten()
                .map(|row| row.member),
        )
    {
        spend(left, 1)?;
        if declaration
            .members
            .get(member as usize)
            .is_none_or(|row| !matches!(row.kind, DynMemberKind::Method(_)))
        {
            return Err(error(
                "E-DYN-DECLARATION",
                Span::default(),
                "dyn method row selects another original member kind",
            ));
        }
    }
    if declaration.associated_definitions.is_none() && declaration.members.len() != method_count {
        return Err(unsupported(
            Span::default(),
            "original dyn associated directory is incomplete",
        ));
    }
    if let Some(rows) = &declaration.associated_definitions {
        let mut seen = std::collections::BTreeSet::new();
        for row in rows {
            spend(left, seen.len().saturating_add(1))?;
            let member = declaration
                .members
                .get(row.member as usize)
                .ok_or_else(|| {
                    error(
                        "E-DYN-DECLARATION",
                        Span::default(),
                        "dyn associated row selects an absent member",
                    )
                })?;
            if !seen.insert(row.member)
                || !matches!(
                    (&member.kind, &row.carrier, &row.value),
                    (
                        DynMemberKind::Type,
                        None,
                        None | Some(DynAssociatedValueRecipe::Type(_))
                    ) | (
                        DynMemberKind::Constant,
                        Some(_),
                        None | Some(DynAssociatedValueRecipe::Constant { .. })
                    )
                )
            {
                return Err(error(
                    "E-DYN-DECLARATION",
                    Span::default(),
                    "dyn associated directory changes the original member kind",
                ));
            }
        }
        if seen.len() != declaration.members.len().saturating_sub(method_count) {
            return Err(error(
                "E-DYN-DECLARATION",
                Span::default(),
                "dyn associated directory omits an original member",
            ));
        }
    }
    Ok(())
}

fn directory_method_scope(
    semantic: &SemanticModule,
    owner: SemanticDynOwner,
    scope: SemanticDefinitionId,
    left: &mut usize,
) -> R<()> {
    *left = left.checked_sub(semantic.dyn_scopes.len()).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "original dyn method binding work exceeded",
            None,
        )
    })?;
    let mut rows = semantic.dyn_scopes.iter().filter(|row| row.owner == owner);
    if rows.next().is_none_or(|row| row.scope != scope) || rows.next().is_some() {
        return Err(error(
            "E-DYN-DECLARATION",
            Span::default(),
            "dyn method directory has no exact original scope binding",
        ));
    }
    Ok(())
}

fn directory_identity_work(identity: &LifeDeclarationIdentity, left: &mut usize) -> R<()> {
    let work = identity
        .package
        .len()
        .saturating_add(identity.generation_path.len())
        .saturating_add(identity.module_path.len())
        .saturating_add(identity.declaration_path.len());
    *left = left.checked_sub(work).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original dyn identity work exceeded", None)
    })?;
    for part in identity
        .module_path
        .iter()
        .chain(&identity.declaration_path)
    {
        *left = left.checked_sub(part.len()).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original dyn identity text work exceeded",
                None,
            )
        })?;
    }
    Ok(())
}

fn directory_signature(
    recipe: &DynSignatureRecipe,
    scope: SemanticDefinitionId,
    original: &SemanticSignatureRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(original, limits, left, 1)?;
    if recipe.parameters.len() != original.parameters.len() || recipe.result_mode != original.result_mode || recipe.call_kind != original.call_kind
        || recipe.invocation_binder_count != original.invocation_lifetimes
        || !recipe.parameters.iter().zip(&original.parameters).all(|(row, parameter)| row.mode == parameter.mode && matches!(&row.ty, DynTypePattern::Semantic { scope: owner, ty } if *owner == scope && ty.as_ref() == &parameter.ty))
        || !matches!(&recipe.result, DynTypePattern::Semantic { scope: owner, ty } if *owner == scope && ty.as_ref() == &original.result)
    { return Err(error("E-DYN-DECLARATION", Span::default(), "dyn method directory changes its full original signature")); }
    Ok(())
}
