use super::*;

use zil_base::{Fingerprint, FingerprintBuilder, Limits, ModuleId, NodeId, Span};

fn program() -> CtArtifactProgram {
    CtArtifactProgram {
        dynamic_catalogue: Default::default(),
        ambient_life_scopes: Vec::new(),
        schema_version: CT_ARTIFACT_SCHEMA_VERSION,
        target_pointer_width: 64,
        source_fingerprint: Fingerprint::ZERO,
        functions: vec![],
        entry: CtFunctionId(0),
        constants: vec![],
        record_schemas: vec![],
        enum_schemas: vec![],
        templates: vec![],
        input_tokens: CtInputTable {
            invocation_fingerprint: Fingerprint::ZERO,
            tokens: vec![],
        },
        snapshot: CtSnapshotRequest {
            stage: None,
            fingerprint: Fingerprint::ZERO,
            ready: CtReadyStage::Tokens,
            target_fingerprint: None,
            requested_queries: vec![],
        },
        budget: CtDeclaredBudget {
            max_steps: 1,
            max_call_depth: 1,
            max_live_cells: 1,
            max_temporary_bytes: 1,
            max_generated_nodes: 1,
            max_instances: 1,
            max_analysis_work: 1,
        },
    }
}

#[test]
fn original_protocols_survive_reification_wire_lifting_and_bind_content_identity() {
    use crate::*;
    // No Type DAG nodes are needed for these raw tuple recipes. Transport
    // preserves protocol claims; it does not certify their applicability.
    struct NoTypes;
    macro_rules! meter {
        () => {
            type Error = ();
            fn node(&mut self, _: usize) -> Result<(), ()> {
                Ok(())
            }
            fn rows(&mut self, _: usize, _: usize, _: usize) -> Result<(), ()> {
                Ok(())
            }
            fn text(&mut self, value: &str, _: usize) -> Result<String, ()> {
                Ok(value.to_owned())
            }
            fn constant(&mut self, value: &Constant, _: usize) -> Result<Constant, ()> {
                Ok(value.clone())
            }
        };
    }
    impl CtDynamicReifier for NoTypes {
        meter!();
        fn store_type(&mut self, _: CtDynamicTypeNode, _: usize) -> Result<CtDynamicTypeId, ()> {
            unreachable!()
        }
    }
    impl CtDynamicLifter for NoTypes {
        meter!();
        fn load_type(&mut self, _: CtDynamicTypeId, _: usize) -> Result<Type, ()> {
            unreachable!()
        }
    }
    let unit = SemanticTypeRecipe::Tuple(vec![]);
    let original = SemanticModule {
        definitions: vec![SemanticDefinition {
            id: SemanticDefinitionId::default(),
            identity: LifeDeclarationIdentity {
                package: "raw".into(),
                module_path: vec![],
                declaration_path: vec!["closed".into()],
                generation_path: vec![],
            },
            parent: None,
            kind: SemanticDefinitionKind::Function,
            parameters: vec![],
            implicit_lifetimes: vec![],
            requirements: vec![],
            environment: vec![],
            body: SemanticDefinitionBody::FunctionWithBody {
                signature: SemanticSignatureRecipe {
                    bound_lifetimes: 0,
                    invocation_lifetimes: 0,
                    parameters: vec![SemanticParameterRecipe {
                        ty: unit.clone(),
                        mode: PassingMode::Infer,
                    }],
                    result: unit.clone(),
                    result_mode: PassingMode::Infer,
                    call_kind: CallKind::Direct,
                },
                protocol_contracts: vec![FunctionProtocolContract {
                    protocol: ProtocolKind::AsyncRegistrationV1,
                    parameter: 0,
                    entry: vec![ProtocolState::Closing, ProtocolState::Closed],
                    exits: vec![ProtocolTransition {
                        input: ProtocolState::Closing,
                        exit: ProtocolExit::PollReady,
                        output: vec![ProtocolState::Closed],
                    }],
                }],
                sites: vec![],
                parameters: vec![CoreClosureParameter {
                    name: "owner".into(),
                    ty: Some(unit),
                    mode: PassingMode::Infer,
                    span: Span::default(),
                }],
                receiver_parameter: None,
                names: vec![],
                site_nodes: vec![],
                body: SemanticOriginalFunctionBody::Declaration,
            },
        }],
        instances: vec![],
        sites: vec![],
        dyn_scopes: vec![],
    };
    let mut raw = program();
    raw.dynamic_catalogue.semantic =
        Some(reify_semantic_module(&original, &mut NoTypes, 0).unwrap());
    let limits = Limits::default();
    let bytes = encode_program(&raw, &limits).unwrap();
    let decoded = decode_program(&bytes, &limits).unwrap();
    assert_eq!(decoded, raw);
    assert_eq!(
        lift_semantic_module(
            decoded.dynamic_catalogue.semantic.as_ref().unwrap(),
            &mut NoTypes,
            0
        )
        .unwrap(),
        original
    );
    let fingerprint = program_fingerprint(&raw, &limits).unwrap();
    let CtSemanticDefinitionBody::FunctionWithBody {
        protocol_contracts, ..
    } = &mut raw.dynamic_catalogue.semantic.as_mut().unwrap().definitions[0].body
    else {
        unreachable!()
    };
    protocol_contracts[0].exits[0].output = vec![CtProtocolState::Closing];
    assert_ne!(program_fingerprint(&raw, &limits).unwrap(), fingerprint);
    let mut stale = bytes;
    stale[8..12].copy_from_slice(&9u32.to_le_bytes());
    resign(&mut stale, CT_ARTIFACT_FINGERPRINT_DOMAIN);
    assert_eq!(decode_program(&stale, &limits), Err(WireError::BadVersion));
}

#[test]
fn ct6_contextual_type_instruction_transports_the_complete_graph() {
    let limits = Limits::default();
    let graph = CtTypeRef {
        scoped_catalogue: None,
        nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
        root: CtTypeNodeId(0),
        ambient_classes: vec![CtAmbientClassBinding {
            owner: CtTypeNodeId(0),
            class: 3,
            source: CtAmbientLifeReference {
                scope: CtAmbientScopeId(7),
                parameter: 4,
            },
        }],
    };
    // Transport deliberately preserves untrusted claims. This is not a Type
    // admission test: independent source and graph validation must reject it.
    let mut raw = program();
    raw.functions.push(CtFunction {
        id: CtFunctionId(0),
        definition: CtSymbolId {
            module: ModuleId(0),
            declaration: NodeId(0),
            generation_path: None,
        },
        parameters: vec![],
        parameter_types: vec![],
        result_type: CtTy::Type,
        locals: vec![],
        values: vec![CtTy::Type],
        blocks: vec![CtBlock {
            id: CtBlockId(0),
            instructions: vec![CtInstruction {
                result: Some(CtValueId(0)),
                kind: CtInstructionKind::TypeInContext { ty: graph },
                span: Span::default(),
            }],
            terminator: CtTerminator::Return(Some(CtValueId(0))),
        }],
        entry: CtBlockId(0),
        span: Span::default(),
    });
    let bytes = encode_program(&raw, &limits).unwrap();
    assert_eq!(&bytes[..8], b"ZILCTP02");
    assert_eq!(decode_program(&bytes, &limits).unwrap(), raw);
    let mut stale = bytes;
    stale[8..12].copy_from_slice(&5u32.to_le_bytes());
    resign(&mut stale, CT_ARTIFACT_FINGERPRINT_DOMAIN);
    assert_eq!(decode_program(&stale, &limits), Err(WireError::BadVersion));
}

#[test]
fn rich_callable_and_ambient_scope_claims_are_lossless_bounded_raw_data() {
    use crate::{
        FunctionLifeContract, LifeTypeProjection, SignatureLifeClass, SignatureLifeClassBinding,
        StorageBase, StorageBinder, StorageLifeContract,
    };
    let limits = Limits::default();
    let flat = FunctionLifeContract {
        binder_count: 2,
        parameter_lives: vec![Some(0), Some(1)],
        result_life: Some(0),
    };
    let mut rich = StorageLifeContract::from_flat(flat.clone()).unwrap();
    assert_eq!(rich.to_flat(2), Some(flat));
    let mut reordered = rich.clone();
    reordered.bindings.swap(0, 1);
    assert!(reordered.to_flat(2).is_none());
    let mut reordered = rich.clone();
    reordered.bindings.swap(1, 2);
    assert!(reordered.to_flat(2).is_none());
    rich.classes.push(SignatureLifeClassBinding {
        position: SignatureLifeClass {
            base: StorageBase::Parameter(0),
            path: vec![LifeTypeProjection::Pointee],
            class: 0,
        },
        binder: StorageBinder::Invocation(1),
    });
    assert!(rich.to_flat(2).is_none());
    let source = CtAmbientLifeReference {
        scope: CtAmbientScopeId(0),
        parameter: 2,
    };
    let graph = CtTypeRef {
        scoped_catalogue: None,
        nodes: vec![
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Address {
                mutable: false,
                inner: CtTypeNodeId(0),
            },
            CtTypeNode::BorrowedFunctionPointer {
                parameters: vec![CtTypeNodeId(1), CtTypeNodeId(1)],
                result: CtTypeNodeId(1),
                life: rich,
            },
        ],
        root: CtTypeNodeId(2),
        // This deliberately unverified owner mapping must survive transport;
        // only the independent contextual verifier may reject or admit it.
        ambient_classes: vec![CtAmbientClassBinding {
            owner: CtTypeNodeId(2),
            class: 0,
            source,
        }],
    };
    let value = CtValue::Type(graph.clone());
    let bytes = encode_value(&value, &limits).unwrap();
    assert_eq!(decode_value(&bytes, &limits), Ok(value.clone()));
    let mut p = program();
    p.ambient_life_scopes.push(CtAmbientLifeScope {
        enclosing: None,
        declaration: CtSymbolId {
            module: ModuleId(4),
            declaration: NodeId(7),
            generation_path: None,
        },
        binder_site: vec![3, 0],
        instance: CtScopeInstance::Instantiated(vec![
            CtScopeActual::Type(graph.clone()),
            CtScopeActual::Scalar(Constant::I64(5)),
            CtScopeActual::Lifetime(source),
        ]),
    });
    let bytes = encode_program(&p, &limits).unwrap();
    assert_eq!(decode_program(&bytes, &limits), Ok(p.clone()));
    for n in 0..5 {
        let mut changed = p.clone();
        let scope = &mut changed.ambient_life_scopes[0];
        match n {
            0 => scope.declaration.declaration = NodeId(8),
            1 => scope.binder_site[0] = 4,
            2 => scope.instance = CtScopeInstance::Definition,
            3 => {
                let CtScopeInstance::Instantiated(actuals) = &mut scope.instance else {
                    unreachable!()
                };
                actuals.swap(0, 1);
            }
            _ => {
                let CtScopeInstance::Instantiated(actuals) = &mut scope.instance else {
                    unreachable!()
                };
                actuals[2] = CtScopeActual::Lifetime(CtAmbientLifeReference {
                    parameter: 1,
                    ..source
                });
            }
        }
        assert_ne!(
            program_fingerprint(&changed, &limits),
            program_fingerprint(&p, &limits)
        );
    }
    for n in 0..6 {
        let mut changed = graph.clone();
        match n {
            0 => changed.ambient_classes[0].owner = CtTypeNodeId(1),
            1 => changed.ambient_classes[0].class = 1,
            2 => changed.ambient_classes[0].source.scope = CtAmbientScopeId(1),
            3 => changed.ambient_classes[0].source.parameter = 1,
            4 => changed.ambient_classes.clear(),
            _ => {
                let CtTypeNode::BorrowedFunctionPointer { life, .. } = &mut changed.nodes[2] else {
                    unreachable!()
                };
                life.classes[0].binder = StorageBinder::Invocation(0);
            }
        }
        assert_ne!(
            encode_value(&CtValue::Type(changed), &limits),
            encode_value(&value, &limits)
        );
    }
    // Same raw index is not a contextual semantic key. These intentionally
    // different scopes leave the standalone value bytes unchanged.
    let mut other_scope = p.clone();
    other_scope.ambient_life_scopes[0].declaration.module = ModuleId(5);
    assert_ne!(
        program_fingerprint(&other_scope, &limits),
        program_fingerprint(&p, &limits)
    );
    assert_eq!(
        encode_value(&value, &limits),
        encode_value(&CtValue::Type(graph), &limits)
    );
    let minimum = (0..limits.max_nodes)
        .find(|&n| {
            encode_program(
                &p,
                &Limits {
                    max_nodes: n,
                    ..limits.clone()
                },
            )
            .is_ok()
        })
        .unwrap();
    assert!(minimum > 0);
    let exact = Limits {
        max_nodes: minimum,
        ..limits.clone()
    };
    let low = Limits {
        max_nodes: minimum - 1,
        ..limits
    };
    assert_eq!(encode_program(&p, &exact), Ok(bytes.clone()));
    assert!(encode_program(&p, &low).is_err());
    assert_eq!(encode_program(&p, &exact), Ok(bytes.clone()));
    assert!(decode_program(&bytes, &low).is_err());
}
fn resign(bytes: &mut [u8], domain: &[u8]) {
    let end = bytes.len() - 32;
    let mut hash = FingerprintBuilder::new();
    hash.update(domain);
    hash.update(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash.digest().0);
}

#[test]
fn roundtrip_program_value_registry_and_domain_fingerprints() {
    let limits = Limits::default();
    let p = program();
    let bytes = encode_program(&p, &limits).unwrap();
    assert_eq!(decode_program(&bytes, &limits), Ok(p.clone()));
    assert_eq!(
        program_fingerprint(&p, &limits).unwrap().0,
        bytes[bytes.len() - 32..]
    );
    let mut changed = p;
    changed.budget.max_steps += 1;
    assert_ne!(
        program_fingerprint(&changed, &limits),
        program_fingerprint(&program(), &limits)
    );
    let function = CtFunction {
        id: CtFunctionId(0),
        definition: CtSymbolId {
            module: ModuleId(0),
            declaration: NodeId(1),
            generation_path: None,
        },
        parameters: vec![],
        parameter_types: vec![],
        result_type: CtTy::Scalar(CtScalarTy::Unit),
        locals: vec![],
        values: vec![],
        blocks: vec![],
        entry: CtBlockId(0),
        span: Span::default(),
    };
    assert_ne!(
        function_fingerprint(&function, &limits).unwrap(),
        program_fingerprint(&program(), &limits).unwrap()
    );

    let value = CtValue::List(vec![
        CtValue::Scalar(Constant::I64(7)),
        CtValue::Text("é".into()),
    ]);
    let bytes = encode_value(&value, &limits).unwrap();
    assert_eq!(decode_value(&bytes, &limits), Ok(value));
    let registry = CtLockedProviderRegistry {
        schema_version: CT_ARTIFACT_SCHEMA_VERSION,
        entries: vec![],
        fingerprint: Fingerprint::ZERO,
    };
    let bytes = encode_registry(&registry, &limits).unwrap();
    assert_eq!(decode_registry(&bytes, &limits), Ok(registry));
}

#[test]
fn rejects_bad_version_truncation_trailing_and_digest() {
    let limits = Limits::default();
    let good = encode_program(&program(), &limits).unwrap();
    for version in [1, CT_ARTIFACT_SCHEMA_VERSION + 1] {
        let mut bad = good.clone();
        bad[8..12].copy_from_slice(&version.to_le_bytes());
        resign(&mut bad, CT_ARTIFACT_FINGERPRINT_DOMAIN);
        assert_eq!(decode_program(&bad, &limits), Err(WireError::BadVersion));
    }
    let mut bad = good.clone();
    bad.pop();
    assert_eq!(
        decode_program(&bad, &limits),
        Err(WireError::BadFingerprint)
    );
    let mut bad = good.clone();
    bad.push(0);
    assert_eq!(
        decode_program(&bad, &limits),
        Err(WireError::BadFingerprint)
    );
    let mut bad = good.clone();
    bad[20] ^= 1;
    assert_eq!(
        decode_program(&bad, &limits),
        Err(WireError::BadFingerprint)
    );

    let mut bad = good.clone();
    bad.insert(bad.len() - 32, 0);
    resign(&mut bad, CT_ARTIFACT_FINGERPRINT_DOMAIN);
    assert_eq!(decode_program(&bad, &limits), Err(WireError::Trailing));

    let mut wrong = program();
    wrong.schema_version += 1;
    assert_eq!(encode_program(&wrong, &limits), Err(WireError::BadVersion));
}

#[test]
fn exact_byte_budget_and_depth_limit() {
    let good = encode_value(&CtValue::Scalar(Constant::Bool(true)), &Limits::default()).unwrap();
    let mut limits = Limits {
        max_source_bytes: good.len(),
        ..Limits::default()
    };
    assert!(decode_value(&good, &limits).is_ok());
    limits.max_source_bytes -= 1;
    assert_eq!(decode_value(&good, &limits), Err(WireError::Limit));
    let value = CtValue::List(vec![CtValue::List(vec![CtValue::Scalar(Constant::I32(1))])]);
    let nested = encode_value(&value, &Limits::default()).unwrap();
    let limits = Limits {
        max_call_depth: 2,
        ..Limits::default()
    };
    assert_eq!(decode_value(&nested, &limits), Err(WireError::Limit));
}

#[test]
fn rejects_noncanonical_bool_and_oversized_sequence_before_allocation() {
    let limits = Limits::default();
    let good = encode_value(&CtValue::Scalar(Constant::Bool(true)), &limits).unwrap();
    let mut bad = good.clone();
    // Envelope 12 bytes, CtValue::Scalar tag u8, Constant::Bool tag u8.
    bad[14] = 2;
    resign(&mut bad, VALUE_DOMAIN);
    assert_eq!(decode_value(&bad, &limits), Err(WireError::NonCanonical));

    let good = encode_value(&CtValue::List(vec![]), &limits).unwrap();
    let mut bad = good.clone();
    bad[13..17].copy_from_slice(&u32::MAX.to_le_bytes());
    resign(&mut bad, VALUE_DOMAIN);
    assert_eq!(decode_value(&bad, &limits), Err(WireError::Limit));
}

#[test]
fn rejects_unknown_tag_and_invalid_utf8_with_valid_digest() {
    let limits = Limits::default();
    let mut bad = encode_value(&CtValue::Scalar(Constant::Unit), &limits).unwrap();
    bad[12] = 255;
    resign(&mut bad, VALUE_DOMAIN);
    assert_eq!(decode_value(&bad, &limits), Err(WireError::UnknownTag));

    let mut bad = encode_value(&CtValue::Text("x".into()), &limits).unwrap();
    bad[17] = 255;
    resign(&mut bad, VALUE_DOMAIN);
    assert_eq!(decode_value(&bad, &limits), Err(WireError::InvalidUtf8));

    let mut bad = encode_value(&CtValue::Scalar(Constant::Bool(true)), &limits).unwrap();
    bad.remove(14);
    resign(&mut bad, VALUE_DOMAIN);
    assert_eq!(decode_value(&bad, &limits), Err(WireError::Truncated));
}

#[test]
fn fingerprint_work_is_charged_at_exact_boundary() {
    let value = CtValue::Scalar(Constant::Bool(true));
    let mut limits = Limits::default();
    let (good, encoded_usage) =
        encode_value_with_usage(&value, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    limits.max_analysis_steps = encoded_usage.work;
    assert!(encode_value(&value, &limits).is_ok());
    limits.max_analysis_steps -= 1;
    assert_eq!(encode_value(&value, &limits), Err(WireError::Limit));

    let (_, decoded_usage) = decode_value_with_usage(
        &good,
        &Limits::default(),
        &mut WireBudget::from_limits(&Limits::default()),
    )
    .unwrap();
    limits.max_analysis_steps = decoded_usage.work;
    assert_eq!(decode_value(&good, &limits), Ok(value));
    limits.max_analysis_steps -= 1;
    assert_eq!(decode_value(&good, &limits), Err(WireError::Limit));
}

#[test]
fn cumulative_wire_usage_charges_failed_work_without_publishing() {
    let limits = Limits::default();
    let value = CtValue::Scalar(Constant::Bool(true));
    let mut remaining = WireBudget::from_limits(&limits);
    let (bytes, usage) = encode_value_with_usage(&value, &limits, &mut remaining).unwrap();
    assert_eq!(usage.bytes, bytes.len());
    assert!(usage.allocation > bytes.len()); // output plus charged continuation
    assert_eq!(remaining.bytes, limits.max_source_bytes - bytes.len());
    let after_first = remaining;
    let (_, second) = encode_value_with_usage(&value, &limits, &mut remaining).unwrap();
    assert_eq!(usage, second);
    assert_eq!(remaining.bytes, after_first.bytes - bytes.len());

    let mut exact = WireBudget::from_limits(&limits);
    exact.bytes = bytes.len();
    exact.allocation = usage.allocation;
    exact.work = usage.work;
    assert!(encode_value_with_usage(&value, &limits, &mut exact).is_ok());
    assert_eq!((exact.bytes, exact.allocation, exact.work), (0, 0, 0));
    let exhausted = exact;
    assert_eq!(
        encode_value_with_usage(&value, &limits, &mut exact),
        Err(WireError::Limit)
    );
    assert_eq!(exact, exhausted);

    let mut decoded = WireBudget::from_limits(&limits);
    let (_, decode_usage) =
        decode_value_with_usage(&bytes, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    decoded.bytes = bytes.len();
    decoded.work = decode_usage.work;
    let (_, used) = decode_value_with_usage(&bytes, &limits, &mut decoded).unwrap();
    assert_eq!(used.bytes, bytes.len());
    assert_eq!((decoded.bytes, decoded.work), (0, 0));
    let mut below = WireBudget::from_limits(&limits);
    below.work = decode_usage.work - 1;
    let before = below;
    assert_eq!(
        decode_value_with_usage(&bytes, &limits, &mut below),
        Err(WireError::Limit)
    );
    assert!(below.work < before.work);
}

#[test]
fn repeated_bad_digest_and_schema_consume_request_budget() {
    let limits = Limits::default();
    let p = program();
    let mut bad = encode_program(&p, &limits).unwrap();
    bad[20] ^= 1;
    let mut remaining = WireBudget::from_limits(&limits);
    let initial = remaining;
    assert_eq!(
        decode_program_with_usage(&bad, &limits, &mut remaining),
        Err(WireError::BadFingerprint)
    );
    let once = remaining;
    assert!(once.work < initial.work && once.bytes < initial.bytes);
    assert_eq!(initial.bytes - once.bytes, bad.len());
    assert_eq!(
        decode_program_with_usage(&bad, &limits, &mut remaining),
        Err(WireError::BadFingerprint)
    );
    assert_eq!(initial.work - once.work, once.work - remaining.work);

    let mut bad = encode_program(&p, &limits).unwrap();
    // Envelope schema is valid; inner raw program schema is stale.
    bad[12] = 1;
    resign(&mut bad, CT_ARTIFACT_FINGERPRINT_DOMAIN);
    let prior = remaining;
    assert_eq!(
        decode_program_with_usage(&bad, &limits, &mut remaining),
        Err(WireError::BadVersion)
    );
    assert!(remaining.work < prior.work);
    let prior = remaining;
    assert_eq!(
        decode_program_with_usage(&bad, &limits, &mut remaining),
        Err(WireError::BadVersion)
    );
    assert!(remaining.work < prior.work);

    let mut tiny = WireBudget::from_limits(&limits);
    tiny.work = 1;
    assert_eq!(
        decode_program_with_usage(&bad, &limits, &mut tiny),
        Err(WireError::Limit)
    );
    assert_eq!(tiny.work, 0);

    let mut local = limits.clone();
    local.max_analysis_steps = 0;
    let mut large_remaining = WireBudget::from_limits(&limits);
    assert_eq!(
        decode_program_with_usage(&bad, &local, &mut large_remaining),
        Err(WireError::Limit)
    );
    assert_eq!(large_remaining.work, limits.max_analysis_steps);
}

#[test]
fn span_encoder_rejects_reversed_bounds() {
    let mut p = program();
    p.functions.push(CtFunction {
        id: CtFunctionId(0),
        definition: CtSymbolId {
            module: ModuleId(0),
            declaration: NodeId(1),
            generation_path: None,
        },
        parameters: vec![],
        parameter_types: vec![],
        result_type: CtTy::Scalar(CtScalarTy::Unit),
        locals: vec![],
        values: vec![],
        blocks: vec![],
        entry: CtBlockId(0),
        span: Span {
            file: zil_base::FileId(0),
            start: 2,
            end: 1,
        },
    });
    assert_eq!(
        encode_program(&p, &Limits::default()),
        Err(WireError::NonCanonical)
    );
}

#[test]
fn large_scalar_sequence_uses_bounded_charged_growth() {
    let limits = Limits::default();
    let values = CtValue::List(
        (0..1024)
            .map(|_| CtValue::Scalar(Constant::I32(7)))
            .collect(),
    );
    let (encoded, usage) =
        encode_value_with_usage(&values, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    assert!(
        usage.growths < 32,
        "{} allocations for {} bytes",
        usage.growths,
        encoded.len()
    );
    assert!(usage.work >= encoded.len() * 2);
    assert!(usage.allocation >= encoded.len());
    assert_eq!(decode_value(&encoded, &limits), Ok(values));
}

#[test]
fn scalar_and_recursive_nodes_charge_exact_boundaries() {
    let value = CtValue::Scalar(Constant::Bool(true));
    let limits = Limits::default();
    let (encoded, usage) =
        encode_value_with_usage(&value, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    assert!(usage.nodes >= 2);
    let mut exact = limits.clone();
    exact.max_nodes = usage.nodes;
    assert!(encode_value(&value, &exact).is_ok());
    exact.max_nodes -= 1;
    assert_eq!(encode_value(&value, &exact), Err(WireError::Limit));
    let mut zero = limits.clone();
    zero.max_nodes = 0;
    assert_eq!(decode_value(&encoded, &zero), Err(WireError::Limit));

    let (_, decoded) =
        decode_value_with_usage(&encoded, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    let mut exact = limits.clone();
    exact.max_nodes = decoded.nodes;
    assert_eq!(decode_value(&encoded, &exact), Ok(value));
    exact.max_nodes -= 1;
    assert_eq!(decode_value(&encoded, &exact), Err(WireError::Limit));

    let nested = CtValue::List(vec![CtValue::List(vec![CtValue::Scalar(Constant::I32(1))])]);
    let (_, nested_usage) =
        encode_value_with_usage(&nested, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    assert!(nested_usage.nodes > usage.nodes);
}

#[test]
fn legal_240_level_value_survives_two_megabyte_worker_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut value = CtValue::Scalar(Constant::I32(9));
            for _ in 0..240 {
                value = CtValue::List(vec![value]);
            }
            let bytes = encode_value(&value, &Limits::default()).unwrap();
            let mut decoded = decode_value(&bytes, &Limits::default()).unwrap();
            for _ in 0..240 {
                let CtValue::List(mut items) = decoded else {
                    panic!("lost list level")
                };
                assert_eq!(items.len(), 1);
                decoded = items.pop().unwrap();
            }
            assert!(matches!(decoded, CtValue::Scalar(Constant::I32(9))));
            drop(value);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn legal_240_level_ct_type_survives_two_megabyte_worker_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut ty = CtTy::Scalar(CtScalarTy::I32);
            for _ in 0..240 {
                ty = CtTy::List(Box::new(ty));
            }
            let mut p = program();
            p.functions.push(CtFunction {
                id: CtFunctionId(0),
                definition: CtSymbolId {
                    module: ModuleId(0),
                    declaration: NodeId(1),
                    generation_path: None,
                },
                parameters: vec![],
                parameter_types: vec![],
                result_type: ty,
                locals: vec![],
                values: vec![],
                blocks: vec![],
                entry: CtBlockId(0),
                span: Span::default(),
            });
            let bytes = encode_program(&p, &Limits::default()).unwrap();
            let mut decoded = decode_program(&bytes, &Limits::default())
                .unwrap()
                .functions
                .pop()
                .unwrap()
                .result_type;
            for _ in 0..240 {
                let CtTy::List(inner) = decoded else {
                    panic!("lost type level")
                };
                decoded = *inner;
            }
            assert_eq!(decoded, CtTy::Scalar(CtScalarTy::I32));
            drop(p);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn over_depth_value_rejects_before_worker_stack_growth() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut value = CtValue::Scalar(Constant::I32(1));
            for _ in 0..256 {
                value = CtValue::List(vec![value]);
            }
            let generous = Limits {
                max_call_depth: 512,
                ..Limits::default()
            };
            let bytes = encode_value(&value, &generous).unwrap();
            assert_eq!(
                encode_value(&value, &Limits::default()),
                Err(WireError::Limit)
            );
            assert_eq!(
                decode_value(&bytes, &Limits::default()),
                Err(WireError::Limit)
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn over_depth_type_rejects_before_worker_stack_growth() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut ty = CtTy::Scalar(CtScalarTy::I32);
            for _ in 0..256 {
                ty = CtTy::List(Box::new(ty));
            }
            let mut p = program();
            p.functions.push(CtFunction {
                id: CtFunctionId(0),
                definition: CtSymbolId {
                    module: ModuleId(0),
                    declaration: NodeId(1),
                    generation_path: None,
                },
                parameters: vec![],
                parameter_types: vec![],
                result_type: ty,
                locals: vec![],
                values: vec![],
                blocks: vec![],
                entry: CtBlockId(0),
                span: Span::default(),
            });
            let generous = Limits {
                max_call_depth: 512,
                ..Limits::default()
            };
            let bytes = encode_program(&p, &generous).unwrap();
            assert_eq!(
                encode_program(&p, &Limits::default()),
                Err(WireError::Limit)
            );
            assert_eq!(
                decode_program(&bytes, &Limits::default()),
                Err(WireError::Limit)
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn iterative_recursive_variants_preserve_historical_bytes_and_explicit_context_migration() {
    // Preserve original schema1 bytes as old-domain rejection controls.
    // Re-signed bytes below test unchanged payload layout only; they grant no
    // artifact/source authentication or migration permission.
    let limits = Limits::default();
    let scalar = CtValue::Scalar(Constant::I32(3));
    let values = [
        CtValue::Tuple(vec![scalar.clone(), CtValue::List(vec![scalar.clone()])]),
        CtValue::Array(vec![scalar.clone(), CtValue::Text("a".into())]),
        CtValue::Record {
            schema: CtRecordSchemaId(7),
            fields: vec![scalar.clone()],
        },
        CtValue::Variant {
            schema: CtEnumSchemaId(8),
            tag: 2,
            fields: vec![CtValue::Array(vec![scalar])],
        },
    ];
    for (index, value) in values.into_iter().enumerate() {
        let fresh = encode_value(&value, &limits).unwrap();
        let prior = [
            include_bytes!("fixtures/tuple-list.wire").as_slice(),
            include_bytes!("fixtures/array-text.wire").as_slice(),
            include_bytes!("fixtures/record.wire").as_slice(),
            include_bytes!("fixtures/variant-array.wire").as_slice(),
        ][index];
        assert_eq!(decode_value(prior, &limits), Err(WireError::BadFingerprint));
        let mut current = prior.to_vec();
        current[..8].copy_from_slice(VALUE_MAGIC);
        current[8..12].copy_from_slice(&CT_ARTIFACT_SCHEMA_VERSION.to_le_bytes());
        resign(&mut current, VALUE_DOMAIN);
        assert_eq!(fresh, current);
        assert_eq!(decode_value(&current, &limits), Ok(value));
    }
    let mut p = program();
    let ty = CtTy::Tuple(vec![
        CtTy::Array {
            element: Box::new(CtTy::Scalar(CtScalarTy::I32)),
            length: 4,
        },
        CtTy::List(Box::new(CtTy::Record(CtRecordSchemaId(7)))),
    ]);
    p.functions.push(CtFunction {
        id: CtFunctionId(0),
        definition: CtSymbolId {
            module: ModuleId(0),
            declaration: NodeId(1),
            generation_path: None,
        },
        parameters: vec![],
        parameter_types: vec![],
        result_type: ty,
        locals: vec![],
        values: vec![],
        blocks: vec![],
        entry: CtBlockId(0),
        span: Span::default(),
    });
    let fresh = encode_program(&p, &limits).unwrap();
    let prior = include_bytes!("fixtures/typed-program.wire").as_slice();
    let mut stale = prior.to_vec();
    resign(&mut stale, CT_ARTIFACT_FINGERPRINT_DOMAIN);
    assert_eq!(decode_program(&stale, &limits), Err(WireError::BadVersion));
    // Schema2 adds only the explicitly frozen target width after the inner
    // schema; all pre-iterative recursive payload bytes remain the old golden.
    let mut current = prior.to_vec();
    current[..8].copy_from_slice(PROGRAM_MAGIC);
    current[8..12].copy_from_slice(&CT_ARTIFACT_SCHEMA_VERSION.to_le_bytes());
    current[12..16].copy_from_slice(&CT_ARTIFACT_SCHEMA_VERSION.to_le_bytes());
    current.splice(16..16, 64u16.to_le_bytes());
    // CT5 adds an empty artifact-wide ambient scope table after source identity.
    current.splice(50..50, 0u32.to_le_bytes());
    // CT9 adds an absent dynamic catalogue after that table, and an absent
    // stage identity at the start of the snapshot. The historical fixture
    // remains unchanged. Its trailing fingerprint is 32 bytes; its declared
    // budget is six u64s plus one u32, and its empty snapshot is a fingerprint,
    // a stage tag, an absent target and an empty query count (38 bytes).
    current.insert(54, 0);
    let snapshot_start = current.len() - 32 - (6 * 8 + 4) - 38;
    current.insert(snapshot_start, 0);
    resign(&mut current, CT_ARTIFACT_FINGERPRINT_DOMAIN);
    assert_eq!(fresh, current);
    assert_eq!(decode_program(&current, &limits), Ok(p));
}

#[test]
fn attempt_is_inside_local_limit_at_exact_work_boundary() {
    let base = Limits::default();
    let value = CtValue::Scalar(Constant::Bool(true));
    let (bytes, encoded) =
        encode_value_with_usage(&value, &base, &mut WireBudget::from_limits(&base)).unwrap();
    let mut exact = base.clone();
    exact.max_analysis_steps = encoded.work;
    let mut remaining = WireBudget::from_limits(&base);
    assert!(encode_value_with_usage(&value, &exact, &mut remaining).is_ok());
    exact.max_analysis_steps -= 1;
    let mut remaining = WireBudget::from_limits(&base);
    assert_eq!(
        encode_value_with_usage(&value, &exact, &mut remaining),
        Err(WireError::Limit)
    );
    assert!(remaining.work < base.max_analysis_steps);

    let (_, decoded) =
        decode_value_with_usage(&bytes, &base, &mut WireBudget::from_limits(&base)).unwrap();
    exact.max_analysis_steps = decoded.work;
    let mut remaining = WireBudget::from_limits(&base);
    assert!(decode_value_with_usage(&bytes, &exact, &mut remaining).is_ok());
    exact.max_analysis_steps -= 1;
    let mut remaining = WireBudget::from_limits(&base);
    assert_eq!(
        decode_value_with_usage(&bytes, &exact, &mut remaining),
        Err(WireError::Limit)
    );
}

#[test]
fn empty_collection_and_nested_frames_pay_continuation_growth() {
    let limits = Limits::default();
    let empty = CtValue::List(vec![]);
    let (encoded, encode_usage) =
        encode_value_with_usage(&empty, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    assert!(encode_usage.allocation > encoded.len());
    let (_, decode_usage) =
        decode_value_with_usage(&encoded, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    assert!(decode_usage.allocation > 0);
    assert!(decode_usage.work > encoded.len());
    let mut exact = WireBudget::from_limits(&limits);
    exact.allocation = decode_usage.allocation;
    assert!(decode_value_with_usage(&encoded, &limits, &mut exact).is_ok());
    assert_eq!(exact.allocation, 0);
    let mut below = WireBudget::from_limits(&limits);
    below.allocation = decode_usage.allocation - 1;
    assert_eq!(
        decode_value_with_usage(&encoded, &limits, &mut below),
        Err(WireError::Limit)
    );

    let mut nested = empty;
    for _ in 0..40 {
        nested = CtValue::List(vec![nested]);
    }
    let encoded = encode_value(&nested, &limits).unwrap();
    let (_, nested_usage) =
        decode_value_with_usage(&encoded, &limits, &mut WireBudget::from_limits(&limits)).unwrap();
    assert!(nested_usage.allocation > decode_usage.allocation * 20);
    let mut exact = WireBudget::from_limits(&limits);
    exact.allocation = nested_usage.allocation;
    assert!(decode_value_with_usage(&encoded, &limits, &mut exact).is_ok());
    let mut below = WireBudget::from_limits(&limits);
    below.allocation = nested_usage.allocation - 1;
    assert_eq!(
        decode_value_with_usage(&encoded, &limits, &mut below),
        Err(WireError::Limit)
    );
}

#[test]
fn borrowed_identity_domains_and_shared_budget_are_checked() {
    let limits = Limits::default();
    let tree = CtTokenTree {
        nodes: vec![],
        roots: vec![],
        origins: vec![],
    };
    let mut budget = WireBudget::from_limits(&limits);
    let before = budget;
    let (tokens, usage) = token_tree_fingerprint_with_usage(&tree, &limits, &mut budget).unwrap();
    assert_eq!(before.work - budget.work, usage.work);
    assert_eq!(before.allocation - budget.allocation, usage.allocation);
    let (value, _) =
        value_fingerprint_with_usage(&CtValue::TokenTree(tree.clone()), &limits, &mut budget)
            .unwrap();
    assert_ne!(tokens, value);
    let mut exact = WireBudget::from_limits(&limits);
    exact.work = usage.work;
    assert_eq!(
        token_tree_fingerprint_with_usage(&tree, &limits, &mut exact)
            .unwrap()
            .0,
        tokens
    );
    assert_eq!(exact.work, 0);
    assert_eq!(
        token_tree_fingerprint_with_usage(&tree, &limits, &mut exact),
        Err(WireError::Limit)
    );
    let mut below = WireBudget::from_limits(&limits);
    below.work = usage.work - 1;
    assert_eq!(
        token_tree_fingerprint_with_usage(&tree, &limits, &mut below),
        Err(WireError::Limit)
    );
}

#[test]
fn ct_dynamic_atoms_have_independent_bytes_and_a_real_type_dag_holder() {
    use crate::{StorageBase, StoragePosition, StoredBorrowMode};
    let position = StoragePosition {
        base: StorageBase::Parameter(3),
        path: vec![],
    };
    let atoms = vec![
        CtStorageSource::Exact(position.clone()),
        CtStorageSource::PlaceAddress {
            place: position.clone(),
            mode: StoredBorrowMode::ExclusiveAddress,
        },
        CtStorageSource::OwnedView {
            head: position.clone(),
            leaf: CtDynamicTypeId(0),
            mode: StoredBorrowMode::SharedAddress,
        },
        CtStorageSource::ReborrowedLeaf {
            source: position.clone(),
            head: position.clone(),
        },
        CtStorageSource::Static {
            leaf: CtDynamicTypeId(0),
            mode: StoredBorrowMode::ReadOnlyDescription,
        },
    ];
    let raw_position = [0, 3, 0, 0, 0, 0, 0, 0, 0];
    let mut expected = vec![];
    expected.push(0);
    expected.extend(raw_position);
    expected.push(1);
    expected.extend(raw_position);
    expected.push(1);
    expected.push(2);
    expected.extend(raw_position);
    expected.extend([0, 0, 0, 0, 0]);
    expected.push(3);
    expected.extend(raw_position);
    expected.extend(raw_position);
    expected.extend([4, 0, 0, 0, 0, 2]);
    let limits = Limits::default();
    let mut encoder = Encoder::new(WireBudget::from_limits(&limits));
    for atom in &atoms {
        atom.encode(&mut encoder).unwrap();
    }
    assert_eq!(encoder.out, expected);
    let mut decoder = Decoder::new(&expected, WireBudget::from_limits(&limits)).unwrap();
    for atom in &atoms {
        assert_eq!(CtStorageSource::decode(&mut decoder).unwrap(), *atom);
    }
    decoder.finish().unwrap();
    let mut raw = program();
    let absent = program_fingerprint(&raw, &limits).unwrap();
    raw.dynamic_catalogue = Some(CtDynamicCatalogue::default());
    assert_ne!(absent, program_fingerprint(&raw, &limits).unwrap());
    raw.dynamic_catalogue
        .as_mut()
        .unwrap()
        .types
        .push(CtDynamicTypeNode::I32);
    raw.dynamic_catalogue
        .as_mut()
        .unwrap()
        .function_sources
        .push(CtFunctionSourceBindings {
            module: ModuleId(7),
            source: NodeId(13),
            retained_return_sources: Some(vec![CtStorageSourceBinding {
                position,
                sources: atoms,
            }]),
        });
    let bytes = encode_program(&raw, &limits).unwrap();
    assert_eq!(decode_program(&bytes, &limits).unwrap(), raw);
    let key = program_fingerprint(&raw, &limits).unwrap();
    raw.dynamic_catalogue.as_mut().unwrap().types[0] = CtDynamicTypeNode::U32;
    assert_ne!(key, program_fingerprint(&raw, &limits).unwrap());
}
#[test]
fn ct_dynamic_equality_recipes_preserve_complete_carriers_and_expression_trees() {
    use crate::{LifeDeclarationId, LifeDeclarationIdentity};
    let mut raw = program();
    raw.dynamic_catalogue = Some(CtDynamicCatalogue::default());
    raw.dynamic_catalogue
        .as_mut()
        .unwrap()
        .types
        .push(CtDynamicTypeNode::Usize);
    raw.dynamic_catalogue
        .as_mut()
        .unwrap()
        .declarations
        .push(CtDynDeclaration {
            implicit_lifetimes: None,
            id: LifeDeclarationId {
                module: ModuleId(2),
                declaration: NodeId(9),
                compiler: false,
            },
            identity: LifeDeclarationIdentity {
                package: "actual".into(),
                module_path: vec!["module".into()],
                declaration_path: vec!["Trait".into()],
                generation_path: vec![],
            },
            kind: CtDynDeclarationKind::Trait,
            parameters: vec![
                CtLifeParameterKind::Type,
                CtLifeParameterKind::Const(CtDynamicTypeId(0)),
            ],
            members: vec![],
            direct_supertraits: Some(vec![]),
            requirements: Some(vec![
                CtDynObligation::TypeEqual {
                    left: CtDynTypePattern::Parameter(0),
                    right: CtDynTypePattern::SelfType,
                },
                CtDynObligation::ConstEqual {
                    ty: CtDynTypePattern::Concrete(CtDynamicTypeId(0)),
                    left: CtDynConstPattern::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(CtDynConstPattern::Value(Constant::Usize(20))),
                        right: Box::new(CtDynConstPattern::Value(Constant::Usize(22))),
                        ty: Box::new(CtDynTypePattern::Concrete(CtDynamicTypeId(0))),
                    },
                    right: CtDynConstPattern::IntegerLiteral("42".into()),
                },
            ]),
            associated_definitions: Some(vec![]),
            method_signatures: Some(vec![]),
            method_definitions: Some(vec![]),
            self_pattern: None,
            implemented_trait: None,
        });
    let limits = Limits::default();
    let bytes = encode_program(&raw, &limits).unwrap();
    assert_eq!(decode_program(&bytes, &limits).unwrap(), raw);
    let key = program_fingerprint(&raw, &limits).unwrap();
    let CtDynObligation::ConstEqual { right, .. } =
        &mut raw.dynamic_catalogue.as_mut().unwrap().declarations[0]
            .requirements
            .as_mut()
            .unwrap()[1]
    else {
        unreachable!()
    };
    *right = CtDynConstPattern::IntegerLiteral("43".into());
    assert_ne!(key, program_fingerprint(&raw, &limits).unwrap());
    let mut pattern = CtDynTypePattern::SelfType;
    for _ in 0..112 {
        pattern = CtDynTypePattern::Slice(Box::new(pattern));
    }
    raw.dynamic_catalogue.as_mut().unwrap().declarations[0].self_pattern = Some(pattern);
    let bytes = encode_program(&raw, &limits).unwrap();
    assert_eq!(decode_program(&bytes, &limits).unwrap(), raw);
    let mut pattern = CtDynTypePattern::SelfType;
    for _ in 0..130 {
        pattern = CtDynTypePattern::Slice(Box::new(pattern));
    }
    raw.dynamic_catalogue.as_mut().unwrap().declarations[0].self_pattern = Some(pattern);
    assert_eq!(encode_program(&raw, &limits), Err(WireError::Limit));
}

#[test]
fn ct_dynamic_constant_occurrences_are_real_catalogue_edges_and_identity() {
    let mut raw = program();
    raw.dynamic_catalogue = Some(CtDynamicCatalogue::default());
    raw.dynamic_catalogue.as_mut().unwrap().types = vec![
        CtDynamicTypeNode::I32,
        CtDynamicTypeNode::Address {
            mutable: false,
            inner: Box::new(CtDynamicTypeId(0)),
        },
        CtDynamicTypeNode::BorrowedFunctionPointer {
            parameters: vec![],
            result: Box::new(CtDynamicTypeId(1)),
            life: crate::StorageLifeContract {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
                bindings: vec![],
                static_bindings: vec![crate::StoragePosition {
                    base: crate::StorageBase::Result,
                    path: vec![],
                }],
                classes: vec![],
            },
        },
    ];
    let limits = Limits::default();
    let bytes = encode_program(&raw, &limits).unwrap();
    assert_eq!(decode_program(&bytes, &limits).unwrap(), raw);
    let key = program_fingerprint(&raw, &limits).unwrap();
    let CtDynamicTypeNode::BorrowedFunctionPointer { life, .. } =
        &mut raw.dynamic_catalogue.as_mut().unwrap().types[2]
    else {
        unreachable!()
    };
    life.static_bindings.clear();
    assert_ne!(key, program_fingerprint(&raw, &limits).unwrap());
    assert_ne!(bytes, encode_program(&raw, &limits).unwrap());
}
#[test]
fn ct_dynamic_wire_work_is_exact_n_minus_one_and_fresh() {
    let mut raw = program();
    raw.dynamic_catalogue = Some(CtDynamicCatalogue::default());
    raw.dynamic_catalogue
        .as_mut()
        .unwrap()
        .types
        .push(CtDynamicTypeNode::F64);
    raw.dynamic_catalogue
        .as_mut()
        .unwrap()
        .function_sources
        .push(CtFunctionSourceBindings {
            module: ModuleId(0),
            source: NodeId(1),
            retained_return_sources: Some(vec![CtStorageSourceBinding {
                position: crate::StoragePosition {
                    base: crate::StorageBase::Result,
                    path: vec![],
                },
                sources: vec![CtStorageSource::Static {
                    leaf: CtDynamicTypeId(0),
                    mode: crate::StoredBorrowMode::SharedAddress,
                }],
            }]),
        });
    let limits = Limits::default();
    let generous = WireBudget::from_limits(&limits);
    let mut measure = generous;
    let (bytes, usage) = encode_program_with_usage(&raw, &limits, &mut measure).unwrap();
    for work in [usage.work - 1, usage.work] {
        let mut budget = WireBudget { work, ..generous };
        let result = encode_program_with_usage(&raw, &limits, &mut budget);
        assert_eq!(result.is_ok(), work == usage.work);
    }
    let mut measure = generous;
    let (_, usage) = decode_program_with_usage(&bytes, &limits, &mut measure).unwrap();
    let mut low = WireBudget {
        work: usage.work - 1,
        ..generous
    };
    assert_eq!(
        decode_program_with_usage(&bytes, &limits, &mut low),
        Err(WireError::Limit)
    );
    assert!(decode_program_with_usage(&bytes, &limits, &mut low).is_err());
    let mut exact = WireBudget {
        work: usage.work,
        ..generous
    };
    assert_eq!(
        decode_program_with_usage(&bytes, &limits, &mut exact)
            .unwrap()
            .0,
        raw
    );
    let mut fresh = WireBudget {
        work: usage.work,
        ..generous
    };
    assert_eq!(
        decode_program_with_usage(&bytes, &limits, &mut fresh)
            .unwrap()
            .0,
        raw
    );
}

#[test]
fn ct_dynamic_unused_lexical_scope_and_header_embedding_have_independent_bytes() {
    use crate::StorageBinder;
    let instance = CtDynFunctionInstance {
        scope: CtDynInstantiationScope {
            fixed_binder_count: 2,
            invocation_binder_count: 0,
        },
        header_embedding: vec![],
        definition: CtDynDefinitionId {
            module: ModuleId(0),
            source: NodeId(7),
        },
        receiver: CtDynTypeValue {
            ty: CtDynamicTypeId(0),
            life: crate::StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes: vec![],
        },
        declaration_actuals: vec![
            CtDynGenericActual::Life(StorageBinder::Fixed(0)),
            CtDynGenericActual::Life(StorageBinder::Fixed(1)),
        ],
        method_actuals: vec![],
    };
    let mut expected = vec![];
    for word in [2u32, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 2] {
        expected.extend(word.to_le_bytes());
    }
    for class in [0u32, 1] {
        expected.extend([3, 0]);
        expected.extend(class.to_le_bytes());
    }
    expected.extend(0u32.to_le_bytes());
    let limits = Limits::default();
    let mut encoder = Encoder::new(WireBudget::from_limits(&limits));
    instance.encode(&mut encoder).unwrap();
    assert_eq!(encoder.out, expected);
    let mut decoder = Decoder::new(&expected, WireBudget::from_limits(&limits)).unwrap();
    assert_eq!(
        CtDynFunctionInstance::decode(&mut decoder).unwrap(),
        instance
    );
    decoder.finish().unwrap();
    let mut raw = program();
    raw.dynamic_catalogue = Some(CtDynamicCatalogue::default());
    let catalogue = raw.dynamic_catalogue.as_mut().unwrap();
    catalogue.types.push(CtDynamicTypeNode::I32);
    catalogue
        .function_instances
        .push(CtDynFunctionInstanceRecord {
            module: ModuleId(0),
            source: NodeId(7),
            instance,
        });
    let key = program_fingerprint(&raw, &limits).unwrap();
    raw.dynamic_catalogue.as_mut().unwrap().function_instances[0]
        .instance
        .header_embedding
        .push(CtDynLifeEmbedding {
            from: StorageBinder::Fixed(0),
            to: StorageBinder::Fixed(1),
        });
    assert_ne!(key, program_fingerprint(&raw, &limits).unwrap());
    let bytes = encode_program(&raw, &limits).unwrap();
    assert_eq!(decode_program(&bytes, &limits).unwrap(), raw);
}
#[test]
fn ct_dynamic_description_implicit_occurrences_preserve_kind_path_and_unknown() {
    let pattern = CtDynTypePattern::Desc {
        lifetime: Some(CtDynLifePattern::ImplicitDeclaration(0)),
        inner: Box::new(CtDynTypePattern::Concrete(CtDynamicTypeId(3))),
    };
    let limits = Limits::default();
    let mut encoder = Encoder::new(WireBudget::from_limits(&limits));
    pattern.encode(&mut encoder).unwrap();
    let mut expected = vec![11, 1, 4];
    expected.extend(0u32.to_le_bytes());
    expected.push(3);
    expected.extend(3u32.to_le_bytes());
    assert_eq!(encoder.out, expected);
    let mut decoder = Decoder::new(&expected, WireBudget::from_limits(&limits)).unwrap();
    assert_eq!(CtDynTypePattern::decode(&mut decoder).unwrap(), pattern);
    decoder.finish().unwrap();
    let occurrence = CtDynImplicitLifeDeclaration {
        occurrences: vec![CtDynImplicitLifeOccurrence {
            root: CtDynImplicitLifeRoot::SelfPattern,
            path: vec![0],
            kind: CtDynImplicitLifeKind::Description,
        }],
    };
    let mut encoder = Encoder::new(WireBudget::from_limits(&limits));
    occurrence.encode(&mut encoder).unwrap();
    let mut expected = 1u32.to_le_bytes().to_vec();
    expected.push(0);
    expected.extend(1u32.to_le_bytes());
    expected.extend(0u32.to_le_bytes());
    expected.push(1);
    assert_eq!(encoder.out, expected);
    let mut changed = occurrence.clone();
    changed.occurrences[0].kind = CtDynImplicitLifeKind::Address;
    let mut other = Encoder::new(WireBudget::from_limits(&limits));
    changed.encode(&mut other).unwrap();
    assert_ne!(encoder.out, other.out);
    let unknown = CtDynTypePattern::Desc {
        lifetime: None,
        inner: Box::new(CtDynTypePattern::Concrete(CtDynamicTypeId(3))),
    };
    let mut other = Encoder::new(WireBudget::from_limits(&limits));
    unknown.encode(&mut other).unwrap();
    assert_ne!(other.out, pattern_bytes(&pattern, &limits));
    fn pattern_bytes(pattern: &CtDynTypePattern, limits: &Limits) -> Vec<u8> {
        let mut encoder = Encoder::new(WireBudget::from_limits(limits));
        pattern.encode(&mut encoder).unwrap();
        encoder.out
    }
}

fn stage_identity_fixture() -> CtStageIdentity {
    CtStageIdentity {
        source_fingerprint: Fingerprint([0x11; 32]),
        epoch: 0x0807_0605_0403_0201,
        committed_generation_fingerprint: Fingerprint([0x22; 32]),
        declaration_inventory_fingerprint: Fingerprint([0x33; 32]),
    }
}
fn stage_identity_bytes() -> Vec<u8> {
    let mut bytes = vec![0x11; 32];
    bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    bytes.extend_from_slice(&[0x22; 32]);
    bytes.extend_from_slice(&[0x33; 32]);
    bytes
}
fn scoped_type_fixture() -> CtTypeRef {
    CtTypeRef {
        scoped_catalogue: Some(CtTypeCatalogue {
            stage: stage_identity_fixture(),
            types: vec![],
            interfaces: vec![],
        }),
        // Deliberately unverified references: transport must preserve them,
        // while semantic admission belongs to the independent stage verifier.
        nodes: vec![
            CtTypeNode::ScopedDyn {
                descriptor: CtDynamicTypeId(7),
                context: Some(CtTypeNodeId(1)),
                classes: vec![CtLifetimeArgument {
                    owner: CtTypeNodeId(1),
                    binder: crate::StorageBinder::Invocation(6),
                }],
            },
            CtTypeNode::ScopedTask {
                descriptor: CtDynamicTypeId(8),
                result: CtTypeNodeId(0),
                context: None,
                classes: vec![],
            },
        ],
        root: CtTypeNodeId(1),
        ambient_classes: vec![],
    }
}
#[test]
fn ct_stage_and_scoped_type_have_an_independent_byte_oracle() {
    let limits = Limits::default();
    let generous = WireBudget::from_limits(&limits);
    let mut encoder = Encoder::new(generous);
    stage_identity_fixture().encode(&mut encoder).unwrap();
    assert_eq!(encoder.out, stage_identity_bytes());
    let mut expected = vec![1]; // Some catalogue, including its own stage.
    expected.extend(stage_identity_bytes());
    let push_u32 = |bytes: &mut Vec<u8>, value: u32| bytes.extend(value.to_le_bytes());
    push_u32(&mut expected, 0); // descriptor inventory
    push_u32(&mut expected, 0); // interface inventory
    push_u32(&mut expected, 2); // physical Type node namespace
    expected.push(17);
    push_u32(&mut expected, 7);
    expected.push(1);
    push_u32(&mut expected, 1);
    push_u32(&mut expected, 1);
    push_u32(&mut expected, 1);
    expected.push(1); // Invocation binder, not Fixed.
    push_u32(&mut expected, 6);
    expected.push(18);
    push_u32(&mut expected, 8);
    push_u32(&mut expected, 0);
    expected.push(0);
    push_u32(&mut expected, 0);
    push_u32(&mut expected, 1);
    push_u32(&mut expected, 0);
    let graph = scoped_type_fixture();
    let mut encoder = Encoder::new(generous);
    graph.encode(&mut encoder).unwrap();
    assert_eq!(encoder.out, expected);
    let mut decoder = Decoder::new(&expected, generous).unwrap();
    assert_eq!(CtTypeRef::decode(&mut decoder).unwrap(), graph);
    decoder.finish().unwrap();
    for end in 0..expected.len() {
        let mut decoder = Decoder::new(&expected[..end], generous).unwrap();
        assert!(CtTypeRef::decode(&mut decoder).is_err(), "truncation {end}");
    }
    let mut malformed = expected;
    malformed[0] = 2;
    let mut decoder = Decoder::new(&malformed, generous).unwrap();
    assert_eq!(
        CtTypeRef::decode(&mut decoder),
        Err(WireError::NonCanonical)
    );
}
#[test]
fn ct_stage_requests_and_scoped_catalogues_are_fingerprinted_and_bounded() {
    let limits = Limits::default();
    let generous = WireBudget::from_limits(&limits);
    let mut raw = program();
    raw.snapshot.stage = Some(stage_identity_fixture());
    raw.dynamic_catalogue = Some(CtDynamicCatalogue {
        stage: Some(stage_identity_fixture()),
        facet_requests: vec![CtStageFacetRequest {
            subject: CtStageSubject::Type(scoped_type_fixture()),
            facet: CtStageFacet::Contract,
            content_fingerprint: Some(Fingerprint([0x44; 32])),
            body_source: Some(CtBodySourceRef {
                declaration: CtSymbolId {
                    module: ModuleId(2),
                    declaration: NodeId(3),
                    generation_path: None,
                },
                body_fingerprint: Fingerprint([0x55; 32]),
            }),
        }],
        ..CtDynamicCatalogue::default()
    });
    let key = program_fingerprint(&raw, &limits).unwrap();
    for mutation in 0..12 {
        let mut changed = raw.clone();
        let catalogue = changed.dynamic_catalogue.as_mut().unwrap();
        match mutation {
            0 => changed.snapshot.stage = None,
            1 => catalogue.stage = None,
            2 => catalogue.stage.as_mut().unwrap().epoch += 1,
            3 => catalogue.stage.as_mut().unwrap().source_fingerprint = Fingerprint::ZERO,
            4 => {
                catalogue
                    .stage
                    .as_mut()
                    .unwrap()
                    .committed_generation_fingerprint = Fingerprint::ZERO
            }
            5 => {
                catalogue
                    .stage
                    .as_mut()
                    .unwrap()
                    .declaration_inventory_fingerprint = Fingerprint::ZERO
            }
            6 => catalogue.facet_requests[0].facet = CtStageFacet::Layout,
            7 => catalogue.facet_requests[0].content_fingerprint = None,
            8 => {
                catalogue.facet_requests[0]
                    .body_source
                    .as_mut()
                    .unwrap()
                    .body_fingerprint = Fingerprint::ZERO
            }
            9 => {
                catalogue.facet_requests[0]
                    .body_source
                    .as_mut()
                    .unwrap()
                    .declaration
                    .declaration = NodeId(4)
            }
            10 | 11 => {
                let CtStageSubject::Type(ty) = &mut catalogue.facet_requests[0].subject else {
                    panic!()
                };
                if mutation == 10 {
                    ty.scoped_catalogue.as_mut().unwrap().stage.epoch += 1;
                } else {
                    ty.scoped_catalogue
                        .as_mut()
                        .unwrap()
                        .types
                        .push(CtDynamicTypeNode::I32);
                }
            }
            _ => unreachable!(),
        }
        assert_ne!(
            program_fingerprint(&changed, &limits).unwrap(),
            key,
            "mutation {mutation}"
        );
    }
    let mut remaining = generous;
    let (bytes, _) = encode_program_with_usage(&raw, &limits, &mut remaining).unwrap();
    let mut remaining = generous;
    let (_, usage) = decode_program_with_usage(&bytes, &limits, &mut remaining).unwrap();
    for dimension in 0..3 {
        let mut exact = generous;
        match dimension {
            0 => exact.work = usage.work,
            1 => exact.nodes = usage.nodes,
            _ => exact.allocation = usage.allocation,
        }
        let mut low = exact;
        match dimension {
            0 => low.work -= 1,
            1 => low.nodes -= 1,
            _ => low.allocation -= 1,
        }
        assert_eq!(
            decode_program_with_usage(&bytes, &limits, &mut low),
            Err(WireError::Limit)
        );
        assert!(decode_program_with_usage(&bytes, &limits, &mut low).is_err());
        let mut fresh = exact;
        assert_eq!(
            decode_program_with_usage(&bytes, &limits, &mut exact)
                .unwrap()
                .0,
            raw
        );
        assert_eq!(
            decode_program_with_usage(&bytes, &limits, &mut fresh)
                .unwrap()
                .0,
            raw
        );
    }
}
