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
    let (declaration, formals, original_identity) = match expected {
        SemanticDynOwner::Declaration(id)
        | SemanticDynOwner::MethodSignature {
            declaration: id, ..
        } => {
            b.spend(module.dyn_declarations.len())?;
            let mut rows = module.dyn_declarations.iter().filter(|d| d.id == *id);
            let declaration = rows
                .next()
                .ok_or_else(|| bad("semantic bridge has no actual original Dyn declaration"))?;
            if rows.next().is_some() {
                return Err(bad("semantic bridge original Dyn declaration is ambiguous"));
            }
            match expected {
                SemanticDynOwner::Declaration(_) => (
                    declaration,
                    &declaration.parameters,
                    Some(&declaration.identity),
                ),
                SemanticDynOwner::MethodSignature { member, .. } => {
                    let Some(DynMemberDeclaration {
                        kind: DynMemberKind::Method(formals),
                        ..
                    }) = declaration.members.get(*member as usize)
                    else {
                        return Err(bad(
                            "semantic bridge selects a different original member kind or ordinal",
                        ));
                    };
                    if !declaration
                        .method_signatures
                        .as_ref()
                        .is_some_and(|rows| rows.iter().any(|row| row.member == *member))
                    {
                        return Err(bad("semantic bridge original method signature is absent"));
                    }
                    (declaration, formals, None)
                }
                _ => unreachable!(),
            }
        }
        SemanticDynOwner::MethodDefinition(id) => {
            let mut selected = None;
            for declaration in &module.dyn_declarations {
                let rows = declaration
                    .method_definitions
                    .as_ref()
                    .ok_or_else(|| bad("semantic bridge original definitions are Unknown"))?;
                b.spend(rows.len().saturating_add(1))?;
                for row in rows.iter().filter(|row| row.id == *id) {
                    let Some(DynMemberDeclaration {
                        kind: DynMemberKind::Method(formals),
                        ..
                    }) = declaration.members.get(row.member as usize)
                    else {
                        return Err(bad(
                            "semantic bridge definition selects a nonmethod original member",
                        ));
                    };
                    if selected
                        .replace((declaration, formals, Some(&row.identity)))
                        .is_some()
                    {
                        return Err(bad(
                            "semantic bridge original method definition is ambiguous",
                        ));
                    }
                }
            }
            selected.ok_or_else(|| bad("semantic bridge original method definition is absent"))?
        }
    };
    if let Some(identity) = original_identity {
        for identity in [identity, &definition.identity] {
            b.spend(
                identity
                    .package
                    .len()
                    .saturating_add(identity.module_path.iter().map(String::len).sum::<usize>())
                    .saturating_add(
                        identity
                            .declaration_path
                            .iter()
                            .map(String::len)
                            .sum::<usize>(),
                    )
                    .saturating_add(identity.module_path.len())
                    .saturating_add(identity.declaration_path.len())
                    .saturating_add(identity.generation_path.len()),
            )?;
        }
        if identity != &definition.identity {
            return Err(bad(
                "semantic bridge changes the full original declaration identity",
            ));
        }
    }
    b.spend(formals.len().saturating_add(definition.parameters.len()))?;
    if formals.len() != definition.parameters.len()
        || !formals.iter().zip(&definition.parameters).all(|(a, z)| {
            matches!(
                (a, z),
                (LifeParameterKind::Type, SemanticFormal::Type { .. })
                    | (LifeParameterKind::Lifetime, SemanticFormal::Lifetime)
                    | (LifeParameterKind::Const(_), SemanticFormal::Const { .. })
            )
        })
    {
        return Err(bad(
            "semantic bridge changes complete original mixed formal order",
        ));
    }
    if !matches!(expected, SemanticDynOwner::Declaration(_)) {
        b.spend(semantic.dyn_scopes.len())?;
        let mut parents = semantic
            .dyn_scopes
            .iter()
            .filter(|binding| binding.owner == SemanticDynOwner::Declaration(declaration.id));
        let parent = parents
            .next()
            .ok_or_else(|| bad("semantic method bridge lacks its actual original owner parent"))?;
        if parents.next().is_some() || definition.parent != Some(parent.scope) {
            return Err(bad(
                "semantic method bridge changes its actual lexical parent",
            ));
        }
    }
    Ok(definition)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_scope_is_selected_from_actual_owner_not_its_self_asserted_scope() {
        let (mut module, _) = crate::tests::baseline();
        let decl = |index| LifeDeclarationId {
            module: module.module_id,
            declaration: zil_base::NodeId(index),
            compiler: false,
        };
        let scope = |index| SemanticDefinitionId {
            module: module.module_id,
            index,
        };
        let definition = |index, declaration| SemanticDefinition {
            id: scope(index),
            identity: LifeDeclarationIdentity {
                package: "bridge".into(),
                module_path: vec![],
                declaration_path: vec![format!("Scope{index}")],
                generation_path: vec![],
            },
            parent: None,
            kind: SemanticDefinitionKind::Scope,
            parameters: vec![],
            implicit_lifetimes: vec![],
            requirements: vec![],
            environment: vec![],
            body: SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }),
        };
        let wanted = SemanticDynOwner::Declaration(decl(80));
        module.semantic = Some(SemanticModule {
            definitions: vec![definition(0, decl(80)), definition(1, decl(81))],
            instances: vec![],
            sites: vec![],
            dyn_scopes: vec![
                SemanticDynScopeBinding {
                    owner: wanted.clone(),
                    scope: scope(0),
                },
                SemanticDynScopeBinding {
                    owner: SemanticDynOwner::Declaration(decl(81)),
                    scope: scope(1),
                },
            ],
        });
        module.dyn_declarations = (0..2)
            .map(|index| DynDeclaration {
                implicit_lifetimes: Some(vec![]),
                direct_supertraits: Some(vec![]),
                requirements: Some(vec![]),
                associated_definitions: Some(vec![]),
                method_signatures: Some(vec![]),
                method_definitions: Some(vec![]),
                self_pattern: None,
                implemented_trait: None,
                id: decl(80 + index),
                identity: definition(index, decl(80 + index)).identity,
                kind: DynDeclarationKind::Trait,
                parameters: vec![],
                members: vec![],
            })
            .collect();
        let limits = Limits::default();
        assert_eq!(
            owner(&module, &wanted, scope(0), &mut Budget::new(&limits))
                .unwrap()
                .id,
            scope(0)
        );
        assert!(owner(&module, &wanted, scope(1), &mut Budget::new(&limits)).is_err());
        let mut wrong = module.clone();
        wrong.semantic.as_mut().unwrap().dyn_scopes[0].scope = scope(1);
        assert!(owner(&wrong, &wanted, scope(1), &mut Budget::new(&limits)).is_err());
        let mut duplicated = module.clone();
        duplicated
            .semantic
            .as_mut()
            .unwrap()
            .dyn_scopes
            .push(SemanticDynScopeBinding {
                owner: wanted.clone(),
                scope: scope(0),
            });
        assert!(owner(&duplicated, &wanted, scope(0), &mut Budget::new(&limits)).is_err());
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(
            owner(&decoded, &wanted, scope(0), &mut Budget::new(&limits))
                .unwrap()
                .id,
            scope(0)
        );
        assert!(owner(&decoded, &wanted, scope(1), &mut Budget::new(&limits)).is_err());
    }
}
