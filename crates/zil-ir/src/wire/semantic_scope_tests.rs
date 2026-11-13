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
fn sealed_impls_and_bounds_preserve_distinct_original_identities() {
    use SemanticBuiltinTrait::*;
    let mut wires = std::collections::BTreeSet::new();
    let mut fingerprints = std::collections::BTreeSet::new();
    let limits = Limits::default();
    for trait_kind in [
        Drop,
        Copy,
        Value,
        SyncDrop,
        StoreElement,
        Transfer,
        Share,
        ThreadCallable,
        ClosedEnvironment,
        Send,
        Sync,
    ] {
        let mut raw = definition();
        let ty = SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0,
        });
        raw.definitions[0].kind = SemanticDefinitionKind::Scope;
        raw.definitions[0].body = SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl {
            self_type: ty.clone(),
            trait_kind,
        });
        raw.definitions[0].requirements =
            vec![SemanticRequirementRecipe::Builtin { ty, trait_kind }];
        // Transport retains even an unproved or inapplicable impl. Only the
        // independent semantic consumers may establish its actual legality.
        let wire = encode(&raw, &limits).unwrap();
        assert_eq!(decode(&wire, &limits).unwrap(), raw);
        assert!(wires.insert(wire));
        let mut fingerprint_bytes = Vec::new();
        crate::encode_semantic_module(&mut fingerprint_bytes, &raw);
        assert!(fingerprints.insert(fingerprint_bytes));
    }
}

#[test]
fn original_body_and_parameter_bindings_survive_without_a_typed_instance() {
    let mut raw = definition();
    let span = Span::default();
    let ty = SemanticTypeRecipe::Atom(Type::I32);
    let SemanticDefinitionBody::Function { mut signature, .. } = raw.definitions[0].body.clone()
    else {
        unreachable!()
    };
    signature.parameters = vec![
        SemanticParameterRecipe {
            ty: ty.clone(),
            mode: PassingMode::Infer
        };
        2
    ];
    signature.result = ty.clone();
    raw.definitions[0].body = SemanticDefinitionBody::FunctionWithBody {
        signature,
        sites: vec![],
        parameters: ["x", "y"]
            .into_iter()
            .map(|name| CoreClosureParameter {
                name: name.into(),
                ty: Some(ty.clone()),
                mode: PassingMode::Infer,
                span,
            })
            .collect(),
        receiver_parameter: None,
        site_nodes: vec![],
        body: SemanticOriginalFunctionBody::Defined(CoreBlock {
            statements: vec![CoreStatement {
                id: NodeId(9),
                kind: CoreStatementKind::Return(Some(CoreExpr {
                    id: NodeId(10),
                    kind: CoreExprKind::Name("x".into()),
                    span,
                })),
                span,
            }],
            span,
        }),
    };
    assert!(raw.instances.is_empty());
    let limits = Limits::default();
    let bytes = encode(&raw, &limits).unwrap();
    assert_eq!(decode(&bytes, &limits).unwrap(), raw);
    let mut fingerprint = Vec::new();
    crate::encode_semantic_module(&mut fingerprint, &raw);
    let SemanticDefinitionBody::FunctionWithBody { parameters, .. } = &mut raw.definitions[0].body
    else {
        unreachable!()
    };
    parameters.swap(0, 1);
    assert_ne!(encode(&raw, &limits).unwrap(), bytes);
    let mut changed_fingerprint = Vec::new();
    crate::encode_semantic_module(&mut changed_fingerprint, &raw);
    assert_ne!(changed_fingerprint, fingerprint);
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
            max_source_bytes: bytes.len() - 1,
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

#[test]
fn core_type_actual_preserves_the_exact_emission_and_lexical_instance() {
    let mut raw = definition();
    let id = SemanticInstanceId {
        module: ModuleId(3),
        index: 9,
    };
    raw.instances.push(SemanticInstance {
        id,
        definition: raw.definitions[0].id,
        target: SemanticInstanceTarget::Function(SemanticFunctionTarget::Core {
            module: ModuleId(3),
            function: NodeId(17),
        }),
        lifetime_instantiation: SemanticLifetimeInstantiation::Rigid,
        parent: None,
        actuals: vec![SemanticActual::Type(SemanticTypeValue::Core {
            scope: id,
            source: SemanticCoreTypeSource::Expression {
                module: ModuleId(3),
                function: NodeId(17),
                expression: NodeId(23),
                occurrence: 2,
                recipe: Box::new(SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: 0,
                })),
            },
        })],
        header_embedding: vec![],
        environment: None,
        self_type: None,
    });
    // Keep a generic const's original carrier and literal unevaluated, rather
    // than fabricating an i32 Constant before the actual T is known.
    raw.instances[0].actuals.push(SemanticActual::CoreRecipe {
        scope: id,
        recipe: Box::new(SemanticActualRecipe::Const {
            carrier: SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: 0,
            }),
            value: SemanticConstRecipe::IntegerLiteral("7".into()),
        }),
    });
    raw.instances[0]
        .actuals
        .push(SemanticActual::Life(SemanticClassExpr::Environment {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0,
        }));
    let limits = Limits::default();
    let bytes = encode(&raw, &limits).unwrap();
    assert_eq!(decode(&bytes, &limits).unwrap(), raw);
    let SemanticActual::Type(SemanticTypeValue::Core { scope, .. }) =
        &mut raw.instances[0].actuals[0]
    else {
        unreachable!()
    };
    scope.index += 1;
    assert_ne!(encode(&raw, &limits).unwrap(), bytes);
    let SemanticActual::Type(SemanticTypeValue::Core { scope, source }) =
        &mut raw.instances[0].actuals[0]
    else {
        unreachable!()
    };
    scope.index -= 1;
    let SemanticCoreTypeSource::Expression { occurrence, .. } = source else {
        unreachable!()
    };
    *occurrence += 1;
    assert_ne!(encode(&raw, &limits).unwrap(), bytes);
}
