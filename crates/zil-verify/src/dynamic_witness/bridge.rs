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
