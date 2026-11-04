use super::*;

use zil_base::{Fingerprint, FingerprintBuilder, Limits, ModuleId, NodeId, Span};

fn program() -> CtArtifactProgram {
    CtArtifactProgram {
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
    let mut bad = good.clone();
    bad[8] = 2;
    resign(&mut bad, CT_ARTIFACT_FINGERPRINT_DOMAIN);
    assert_eq!(decode_program(&bad, &limits), Err(WireError::BadVersion));
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
    resign(&mut bad, b"zil/ct-value/v1");
    assert_eq!(decode_value(&bad, &limits), Err(WireError::NonCanonical));

    let good = encode_value(&CtValue::List(vec![]), &limits).unwrap();
    let mut bad = good.clone();
    bad[13..17].copy_from_slice(&u32::MAX.to_le_bytes());
    resign(&mut bad, b"zil/ct-value/v1");
    assert_eq!(decode_value(&bad, &limits), Err(WireError::Limit));
}

#[test]
fn rejects_unknown_tag_and_invalid_utf8_with_valid_digest() {
    let limits = Limits::default();
    let mut bad = encode_value(&CtValue::Scalar(Constant::Unit), &limits).unwrap();
    bad[12] = 255;
    resign(&mut bad, b"zil/ct-value/v1");
    assert_eq!(decode_value(&bad, &limits), Err(WireError::UnknownTag));

    let mut bad = encode_value(&CtValue::Text("x".into()), &limits).unwrap();
    bad[17] = 255;
    resign(&mut bad, b"zil/ct-value/v1");
    assert_eq!(decode_value(&bad, &limits), Err(WireError::InvalidUtf8));

    let mut bad = encode_value(&CtValue::Scalar(Constant::Bool(true)), &limits).unwrap();
    bad.remove(14);
    resign(&mut bad, b"zil/ct-value/v1");
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
    bad[12] = 2;
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
fn iterative_recursive_variants_preserve_schema_one_bytes() {
    // Frozen bytes captured from the pre-iterative schema1 codec; no duplicate encoder is compiled.
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
        assert_eq!(fresh, prior);
        assert_eq!(decode_value(prior, &limits), Ok(value));
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
    assert_eq!(fresh, prior);
    assert_eq!(decode_program(prior, &limits), Ok(p));
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
