//! Borrowed original equations must retain their owner and lexical transport.
use super::*;

fn id(index: u32) -> SemanticDefinitionId {
    SemanticDefinitionId {
        module: zil_base::ModuleId(0),
        index,
    }
}
fn module() -> SemanticModule {
    SemanticModule {
        definitions: vec![],
        instances: vec![],
        sites: vec![],
        dyn_scopes: vec![],
    }
}
fn address(ordinal: u32) -> SemanticTypeRecipe {
    SemanticTypeRecipe::Address {
        mutable: false,
        lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        })),
        inner: Box::new(SemanticTypeRecipe::Atom(Type::I32)),
    }
}
fn child(parent: SemanticDefinitionId, source: &SemanticOriginalTypeSource) -> SemanticDefinition {
    SemanticDefinition {
        id: id(1),
        identity: LifeDeclarationIdentity {
            package: "query-view-test".into(),
            module_path: vec![],
            declaration_path: vec!["child".into()],
            generation_path: vec![],
        },
        parent: Some(parent),
        kind: SemanticDefinitionKind::Closure,
        parameters: vec![],
        implicit_lifetimes: vec![],
        requirements: vec![],
        environment: vec![SemanticEnvironmentFormal {
            kind: SemanticFormal::Type {
                requirements: vec![],
            },
            source: SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: source.clone(),
            }),
        }],
        body: SemanticDefinitionBody::Function {
            signature: SemanticSignatureRecipe {
                bound_lifetimes: 0,
                invocation_lifetimes: 0,
                parameters: vec![],
                result: SemanticTypeRecipe::Atom(Type::Unit),
                result_mode: PassingMode::Infer,
                call_kind: CallKind::Direct,
            },
            sites: vec![],
        },
    }
}

#[test]
fn borrowed_query_keeps_exact_owner_and_checked_environment() {
    let source = SemanticOriginalTypeSource::Expression(NodeId(7));
    let mut program = module();
    program.definitions.push(child(id(0), &source));
    let mut proof = Proof::default();
    proof.complete.extend([id(0), id(1)]);
    proof
        .types
        .insert((id(0), Query::Expression(NodeId(7))), address(0));
    proof
        .types
        .insert((id(1), Query::Expression(NodeId(7))), address(1));
    let limits = Limits::default();
    let mut left = limits.max_analysis_steps;
    for owner in [id(0), id(1)] {
        let borrowed = proof
            .type_recipe_view(&program, owner, owner, &source, &limits, &mut left)
            .unwrap();
        let owned = proof
            .type_recipe(&program, owner, owner, &source, &limits, &mut left)
            .unwrap();
        assert_eq!(borrowed.as_ref(), &address(owner.index));
        assert_eq!(borrowed.as_ref(), &owned);
    }
    assert_eq!(
        proof
            .type_recipe_view(&program, id(1), id(0), &source, &limits, &mut left)
            .unwrap()
            .as_ref(),
        &SemanticTypeRecipe::Environment {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0
        },
    );
    // A same-shaped parent equation never substitutes for a missing capture.
    program.definitions[0].environment.clear();
    assert!(
        proof
            .type_recipe_view(&program, id(1), id(0), &source, &limits, &mut left)
            .is_err()
    );
    program.definitions[0] = child(id(2), &source);
    assert!(
        proof
            .type_recipe_view(&program, id(1), id(0), &source, &limits, &mut left)
            .is_err()
    );
    proof.complete.remove(&id(0));
    assert!(
        proof
            .type_recipe_view(&program, id(0), id(0), &source, &limits, &mut left)
            .is_err()
    );
}

#[test]
fn borrowed_constructor_still_requires_a_unique_complete_payload_equation() {
    let owner = id(0);
    let node = NodeId(7);
    let source = SemanticOriginalTypeSource::Expression(node);
    let mut proof = Proof::default();
    proof.complete.insert(owner);
    proof.types.insert(
        (owner, Query::Expression(node)),
        SemanticTypeRecipe::ErrorUnion {
            error: Box::new(SemanticTypeRecipe::Atom(Type::I32)),
            result: Box::new(address(0)),
        },
    );
    proof.injections.insert(
        (owner, node),
        Injection {
            is_error: false,
            payload: address(0),
        },
    );
    let limits = Limits::default();
    let mut left = limits.max_analysis_steps;
    assert_eq!(
        proof
            .constructor_type_recipe(&module(), owner, owner, &source, &limits, &mut left)
            .unwrap()
            .as_ref(),
        &address(0)
    );
    proof.injections.get_mut(&(owner, node)).unwrap().payload = address(1);
    assert!(
        proof
            .constructor_type_recipe(&module(), owner, owner, &source, &limits, &mut left)
            .is_err()
    );
    proof.injections.get_mut(&(owner, node)).unwrap().payload = address(0);
    proof.types.insert(
        (owner, Query::Expression(node)),
        SemanticTypeRecipe::ErrorUnion {
            error: Box::new(address(0)),
            result: Box::new(address(0)),
        },
    );
    assert!(
        proof
            .constructor_type_recipe(&module(), owner, owner, &source, &limits, &mut left)
            .is_err()
    );
}
