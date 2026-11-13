//! Raw scoped-program transport: decoding preserves claims and grants no proof.
use super::*;

fn definition() -> SemanticModule {
    SemanticModule {
        definitions: vec![SemanticDefinition {
            id: SemanticDefinitionId {
                module: ModuleId(3),
                index: 7,
            },
            identity: LifeDeclarationIdentity {
                package: "sample".into(),
                module_path: vec!["library".into()],
                declaration_path: vec!["make".into()],
                generation_path: vec![2, 5],
            },
            parent: None,
            kind: SemanticDefinitionKind::Function,
            parameters: vec![
                SemanticFormal::Type {
                    requirements: vec![],
                },
                SemanticFormal::Const {
                    carrier: SemanticTypeRecipe::Atom(Type::I32),
                },
                SemanticFormal::Lifetime,
                SemanticFormal::Lifetime,
            ],
            implicit_lifetimes: vec![SemanticImplicitLifetime {
                ordinal: 0,
                occurrence_path: vec![1, 4, 2],
            }],
            requirements: vec![],
            environment: vec![SemanticEnvironmentFormal {
                kind: SemanticFormal::Lifetime,
                source: SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(1),
                    ordinal: 2,
                })),
            }],
            body: SemanticDefinitionBody::Function {
                signature: SemanticSignatureRecipe {
                    bound_lifetimes: 0,
                    invocation_lifetimes: 0,
                    parameters: vec![],
                    result: SemanticTypeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: 0,
                    }),
                    result_mode: PassingMode::Move,
                    call_kind: CallKind::Direct,
                },
                sites: vec![SemanticSiteRecipe {
                    id: SemanticSiteId(11),
                    occurrence_path: vec![0, 2],
                    kind: SemanticSiteRecipeKind::DeclareLocal {
                        expected: SemanticTypeRecipe::Formal(SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: 0,
                        }),
                    },
                }],
            },
        }],
        instances: vec![],
        sites: vec![],
        dyn_scopes: vec![],
    }
}

fn encode(program: &SemanticModule, limits: &Limits) -> WireResult<Vec<u8>> {
    let mut writer = Writer::new(limits);
    writer.semantic_module(program, 0)?;
    Ok(writer.output)
}

fn decode(bytes: &[u8], limits: &Limits) -> WireResult<SemanticModule> {
    let mut reader = Reader::new(bytes, limits)?;
    let result = reader.semantic_module(0)?;
    reader.finish()?;
    Ok(result)
}

#[test]
fn empty_scoped_program_has_four_independent_counts() {
    let raw = SemanticModule {
        definitions: vec![],
        instances: vec![],
        sites: vec![],
        dyn_scopes: vec![],
    };
    let limits = Limits::default();
    let bytes = encode(&raw, &limits).unwrap();
    assert_eq!(bytes, [0u8; 16]);
    assert_eq!(decode(&bytes, &limits).unwrap(), raw);
    for length in 0..bytes.len() {
        assert!(decode(&bytes[..length], &limits).is_err());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode(&trailing, &limits).is_err());
}

#[test]
fn original_mixed_order_implicit_occurrences_and_free_environment_survive() {
    let raw = definition();
    let limits = Limits::default();
    let bytes = encode(&raw, &limits).unwrap();
    assert_eq!(decode(&bytes, &limits).unwrap(), raw);
    assert_eq!(
        encode(&decode(&bytes, &limits).unwrap(), &limits).unwrap(),
        bytes
    );
    // These are untrusted records: the codec must preserve even an unresolved
    // ancestor claim, rather than inventing a runtime scope or erasing it.
    let mut changed = raw.clone();
    changed.definitions[0].implicit_lifetimes[0].occurrence_path[1] += 1;
    assert_ne!(encode(&changed, &limits).unwrap(), bytes);
    changed = raw;
    let SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(formal)) =
        &mut changed.definitions[0].environment[0].source
    else {
        unreachable!()
    };
    formal.ordinal = 3;
    assert_ne!(encode(&changed, &limits).unwrap(), bytes);
}

#[test]
fn scoped_recipe_limits_precede_deep_transport_and_growth() {
    let mut raw = definition();
    let SemanticDefinitionBody::Function { signature, .. } = &mut raw.definitions[0].body else {
        unreachable!()
    };
    for _ in 0..24 {
        signature.result = SemanticTypeRecipe::Managed(Box::new(signature.result.clone()));
    }
    let defaults = Limits::default();
    let bytes = encode(&raw, &defaults).unwrap();
    for limits in [
        Limits {
            max_call_depth: 8,
            ..defaults.clone()
        },
        Limits {
            max_nodes: 1,
            ..defaults.clone()
        },
        Limits {
            max_analysis_steps: 1,
            ..defaults.clone()
        },
    ] {
        for errors in [
            encode(&raw, &limits).unwrap_err(),
            decode(&bytes, &limits).unwrap_err(),
        ] {
            assert_eq!(errors.category, zil_base::ResultCategory::ResourceLimit);
        }
    }
    assert_eq!(decode(&bytes, &defaults).unwrap(), raw);
}
