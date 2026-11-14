//! Original member identity lookups. These validate raw source links only;
//! they do not choose a provider or establish any semantic capability.
use super::*;

fn identity_work(value: &LifeDeclarationIdentity, b: &mut Budget) -> CompileResult<()> {
    b.spend(
        value
            .package
            .len()
            .saturating_add(value.generation_path.len())
            .saturating_add(1),
        WORK,
    )?;
    for part in value.module_path.iter().chain(&value.declaration_path) {
        b.spend(part.len().saturating_add(1), WORK)?;
    }
    Ok(())
}
fn mixed(
    parameters: &[LifeParameterKind],
    formals: &[SemanticFormal],
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(parameters.len().saturating_add(formals.len()), WORK)?;
    if parameters.len() != formals.len()
        || parameters.iter().zip(formals).any(|(raw, original)| {
            !matches!(
                (raw, original),
                (LifeParameterKind::Type, SemanticFormal::Type { .. })
                    | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime)
                    | (LifeParameterKind::Const(_), SemanticFormal::Const { .. })
            )
        })
    {
        return Err(fail(
            "original member changes its complete mixed formal order or kinds",
        ));
    }
    Ok(())
}
fn scan_binding(
    program: &SemanticModule,
    owner: &SemanticDynOwner,
    found: &mut Option<SemanticDefinitionId>,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(program.dyn_scopes.len(), WORK)?;
    let mut local = None;
    for row in &program.dyn_scopes {
        if &row.owner != owner {
            continue;
        }
        if local.replace(row.scope).is_some() {
            return Err(fail(
                "original Dyn owner repeats a scope binding in one input",
            ));
        }
        if found.is_some_and(|old| old != row.scope) {
            return Err(fail("original Dyn owner has conflicting source scopes"));
        }
        *found = Some(row.scope);
    }
    Ok(())
}
fn binding(
    program: &SemanticModule,
    owner: &SemanticDynOwner,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<SemanticDefinitionId> {
    let mut found = None;
    scan_binding(program, owner, &mut found, b)?;
    if let Some(source) = inventory.source {
        for module in &source.modules {
            b.spend(1, WORK)?;
            if let Some(original) = &module.core.semantic {
                scan_binding(original, owner, &mut found, b)?;
            }
        }
    }
    found.ok_or_else(|| unknown("original member has no exact source scope binding"))
}

pub(super) fn declaration_scope(
    program: &SemanticModule,
    declaration: LifeDeclarationId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<SemanticDefinitionId> {
    let id = binding(
        program,
        &SemanticDynOwner::Declaration(declaration),
        inventory,
        b,
    )?;
    let original = inventory.definition(id, b)?;
    let raw = inventory.original_dyn(declaration, b)?;
    identity_work(&original.identity, b)?;
    identity_work(&raw.identity, b)?;
    if original.kind != SemanticDefinitionKind::Scope || original.identity != raw.identity {
        return Err(fail(
            "original declaration scope changes its complete identity or kind",
        ));
    }
    let valid = match (&original.body, raw.kind) {
        (
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait {
                declaration: actual,
            }),
            DynDeclarationKind::Trait,
        ) => *actual == declaration,
        (
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::TraitImpl {
                    declaration: actual,
                    ..
                }
                | SemanticScopeRecipe::InherentImpl {
                    declaration: actual,
                    ..
                },
            ),
            DynDeclarationKind::Impl,
        ) => *actual == declaration,
        _ => false,
    };
    if !valid {
        return Err(fail(
            "original declaration binding selects another authored scope",
        ));
    }
    mixed(&raw.parameters, &original.parameters, b)?;
    Ok(id)
}
fn method_header(
    program: &SemanticModule,
    declaration: LifeDeclarationId,
    member: u32,
    scope: SemanticDefinitionId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    let raw = inventory.original_dyn(declaration, b)?;
    b.spend(raw.members.len(), WORK)?;
    let Some(DynMemberDeclaration {
        kind: DynMemberKind::Method(parameters),
        ..
    }) = raw.members.get(member as usize)
    else {
        return Err(fail(
            "original method binding names a nonmethod or out-of-range member",
        ));
    };
    let parent = declaration_scope(program, declaration, inventory, b)?;
    let method = inventory.definition(scope, b)?;
    identity_work(&method.identity, b)?;
    let member_name = &raw.members[member as usize].name;
    b.spend(member_name.len(), WORK)?;
    if method.identity.declaration_path.last() != Some(member_name) {
        return Err(fail(
            "original member name disagrees with its exact source identity",
        ));
    }
    if method.parent != Some(parent)
        || method.kind != SemanticDefinitionKind::Method
        || !matches!(method.body, SemanticDefinitionBody::FunctionWithBody { .. })
    {
        return Err(fail(
            "original method scope has another parent, kind, or missing authored body inventory",
        ));
    }
    mixed(parameters, &method.parameters, b)
}
fn one_definition<'a>(
    raw: &'a DynDeclaration,
    id: DynDefinitionId,
    b: &mut Budget,
) -> CompileResult<&'a DynDefinitionDeclaration> {
    let rows = raw
        .method_definitions
        .as_ref()
        .ok_or_else(|| unknown("original method definitions are incomplete"))?;
    b.spend(rows.len(), WORK)?;
    let mut found = None;
    for row in rows {
        if row.id == id && found.replace(row).is_some() {
            return Err(fail("original method definition identity is repeated"));
        }
    }
    found.ok_or_else(|| fail("original method definition does not belong to its declared owner"))
}
fn definition_owner(
    id: DynDefinitionId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<LifeDeclarationId> {
    let mut owner = None;
    let mut scan = |rows: &[DynDeclaration], b: &mut Budget| -> CompileResult<()> {
        for raw in rows {
            b.spend(1, WORK)?;
            let Some(definitions) = &raw.method_definitions else {
                continue;
            };
            b.spend(definitions.len(), WORK)?;
            for definition in definitions {
                if definition.id == id {
                    if owner.is_some_and(|old| old != raw.id) {
                        return Err(fail(
                            "original method definition belongs to multiple declarations",
                        ));
                    }
                    owner = Some(raw.id);
                }
            }
        }
        Ok(())
    };
    scan(&inventory.module.dyn_declarations, b)?;
    if let Some(source) = inventory.source {
        for module in &source.modules {
            scan(&module.core.dyn_declarations, b)?;
        }
    }
    owner.ok_or_else(|| fail("original method definition has no source declaration owner"))
}
pub(super) fn definition_scope(
    program: &SemanticModule,
    id: DynDefinitionId,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<SemanticDefinitionId> {
    let declaration = definition_owner(id, inventory, b)?;
    let raw = inventory.original_dyn(declaration, b)?;
    let definition = one_definition(raw, id, b)?;
    let scope = binding(
        program,
        &SemanticDynOwner::MethodDefinition(id),
        inventory,
        b,
    )?;
    method_header(program, declaration, definition.member, scope, inventory, b)?;
    let original = inventory.definition(scope, b)?;
    identity_work(&definition.identity, b)?;
    identity_work(&original.identity, b)?;
    if definition.identity != original.identity
        || !matches!(
            &original.body,
            SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            }
        )
    {
        return Err(fail(
            "original method definition changes identity or lacks its real implementation body",
        ));
    }
    if !matches!(
        (raw.kind, definition.scope),
        (DynDeclarationKind::Trait, DynDefinitionScope::TraitDefault)
            | (DynDeclarationKind::Impl, DynDefinitionScope::ImplOverride)
    ) {
        return Err(fail(
            "original method definition changes default versus override ownership",
        ));
    }
    if raw.kind == DynDeclarationKind::Trait {
        let signature = member_scope(program, declaration, definition.member, inventory, b)?;
        if signature != scope {
            return Err(fail(
                "original trait default body was rebound to another method signature",
            ));
        }
    }
    Ok(scope)
}
pub(super) fn member_scope(
    program: &SemanticModule,
    declaration: LifeDeclarationId,
    member: u32,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<SemanticDefinitionId> {
    let raw = inventory.original_dyn(declaration, b)?;
    if raw.kind == DynDeclarationKind::Impl {
        let rows = raw
            .method_definitions
            .as_ref()
            .ok_or_else(|| unknown("original implementation method directory is incomplete"))?;
        b.spend(rows.len(), WORK)?;
        let mut found = None;
        for row in rows {
            if row.member == member && found.replace(row.id).is_some() {
                return Err(fail("original implementation repeats a member body"));
            }
        }
        return definition_scope(
            program,
            found.ok_or_else(|| fail("original implementation omits the selected member body"))?,
            inventory,
            b,
        );
    }
    let scope = binding(
        program,
        &SemanticDynOwner::MethodSignature {
            declaration,
            member,
        },
        inventory,
        b,
    )?;
    method_header(program, declaration, member, scope, inventory, b)?;
    let rows = raw
        .method_signatures
        .as_ref()
        .ok_or_else(|| unknown("original trait method signature directory is incomplete"))?;
    b.spend(rows.len(), WORK)?;
    let mut found = None;
    for row in rows {
        if row.member == member && found.replace(row).is_some() {
            return Err(fail("original trait repeats a member signature"));
        }
    }
    let row = found.ok_or_else(|| fail("original trait omits the selected method signature"))?;
    let original = inventory.definition(scope, b)?;
    let SemanticDefinitionBody::FunctionWithBody {
        signature, body, ..
    } = &original.body
    else {
        return Err(fail("original member has no complete function inventory"));
    };
    b.spend(
        signature
            .parameters
            .len()
            .saturating_add(row.signature.parameters.len()),
        WORK,
    )?;
    if signature.parameters.len() != row.signature.parameters.len()
        || signature.call_kind != row.signature.call_kind
        || signature.result_mode != row.signature.result_mode
        || signature.invocation_lifetimes != row.signature.invocation_binder_count
        || signature
            .parameters
            .iter()
            .zip(&row.signature.parameters)
            .any(|(a, b)| a.mode != b.mode)
    {
        return Err(fail(
            "original method signature changes its parameter modes or invocation header",
        ));
    }
    if matches!(body, SemanticOriginalFunctionBody::Declaration) != row.default_definition.is_none()
    {
        return Err(fail(
            "original trait default-body selector disagrees with its authored body",
        ));
    }
    Ok(scope)
}
fn validate_input(
    input: &SemanticModule,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(input.dyn_scopes.len(), WORK)?;
    let mut owners = Vec::new();
    for row in &input.dyn_scopes {
        b.spend(owners.len().saturating_add(1), WORK)?;
        if owners.contains(&row.owner) {
            return Err(fail("original Dyn input repeats an owner binding"));
        }
        owners
            .try_reserve(1)
            .map_err(|_| resource("original member identity inventory allocation failed"))?;
        owners.push(row.owner.clone());
        let expected = match row.owner {
            SemanticDynOwner::Declaration(id) => declaration_scope(program, id, inventory, b)?,
            SemanticDynOwner::MethodSignature {
                declaration,
                member,
            } => member_scope(program, declaration, member, inventory, b)?,
            SemanticDynOwner::MethodDefinition(id) => definition_scope(program, id, inventory, b)?,
        };
        if expected != row.scope {
            return Err(fail(
                "original Dyn binding was redirected to another source member",
            ));
        }
    }
    Ok(())
}
pub(super) fn validate(
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    validate_input(program, program, inventory, b)?;
    if let Some(source) = inventory.source {
        for module in &source.modules {
            if let Some(input) = &module.core.semantic {
                validate_input(input, program, inventory, b)?;
            }
        }
    }
    // Reverse coverage is required for every complete user declaration. Sealed
    // builtin rows without user recipes remain the sealed checker's concern.
    let mut seen = BTreeSet::new();
    let mut scan = |rows: &[DynDeclaration], b: &mut Budget| -> CompileResult<()> {
        for raw in rows {
            b.spend(seen.len().saturating_add(1), WORK)?;
            if !seen.insert(raw.id) {
                continue;
            }
            if raw.associated_definitions.is_none()
                && raw.method_signatures.is_none()
                && raw.method_definitions.is_none()
            {
                continue;
            }
            declaration_scope(program, raw.id, inventory, b)?;
            b.spend(raw.members.len(), WORK)?;
            for (member, row) in raw.members.iter().enumerate() {
                if matches!(row.kind, DynMemberKind::Method(_)) {
                    member_scope(program, raw.id, member as u32, inventory, b)?;
                }
            }
            if let Some(definitions) = &raw.method_definitions {
                for row in definitions {
                    definition_scope(program, row.id, inventory, b)?;
                }
            }
        }
        Ok(())
    };
    scan(&inventory.module.dyn_declarations, b)?;
    if let Some(source) = inventory.source {
        for module in &source.modules {
            scan(&module.core.dyn_declarations, b)?;
        }
    }
    Ok(())
}
