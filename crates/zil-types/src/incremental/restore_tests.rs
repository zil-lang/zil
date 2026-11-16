use super::super::tests::{loader, prepared, products, session};
use super::*;
use zil_ir::wire::{FunctionEncodingStats, WireSegment, raw_scalar_function_id};

fn semantic_hints(
    source: &FunctionSession,
    limits: &Limits,
    relabel: bool,
) -> BTreeMap<FunctionId, RawFunctionEncoding> {
    let target = TargetProfile::default();
    let (mut module, _) =
        zil_ir::wire::decode_module_candidate(&empty_wire(&target, limits).unwrap(), limits)
            .unwrap();
    let mut functions = source
        .entries
        .values()
        .map(|entry| {
            let mut function = entry.canonical().unwrap().as_ref().clone();
            if relabel {
                function.name = format!("@module9:function{}", function.id.0);
            }
            function
        })
        .collect::<Vec<_>>();
    functions.sort_by_key(|function| function.id);
    module.functions = functions.into();
    let plan = CandidatePlan::empty_for(&module);
    let slots: Vec<_> = module.functions.iter().map(|_| OnceLock::new()).collect();
    let inputs: Vec<_> = module.functions.iter().zip(&slots).collect();
    let segments: Vec<WireSegment> = zil_ir::wire::encode_module_candidate_segments(
        &module,
        &plan,
        &inputs,
        limits,
        &mut FunctionEncodingStats::default(),
    )
    .unwrap();
    let bytes: Vec<_> = segments
        .iter()
        .flat_map(|segment| segment.bytes.iter().copied())
        .collect();
    let decoded =
        zil_ir::wire::decode_module_candidate_with_encodings(&bytes, &segments, limits).unwrap();
    decoded
        .encodings
        .unwrap()
        .functions
        .into_iter()
        .map(|encoding| (encoding.decoded_function().unwrap().id, encoding))
        .collect()
}

#[test]
fn shared_semantic_decode_restores_function_items_without_a_second_decoder() {
    let source = session();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let items = products(&source, &limits);
    for relabel in [false, true] {
        let hints = semantic_hints(&source, &limits, relabel);
        let mut restored = FunctionSession::default();
        let mut stats = RawFunctionDecodingStats::default();
        assert!(restored.restore_item_products_with_decoded(
            &items.manifest,
            &mut |key| loader(&items.items, key),
            &mut |bytes| hints.get(&raw_scalar_function_id(bytes)?).cloned(),
            &target,
            &limits,
            &mut stats,
        ));
        assert_eq!((stats.decode_attempts, stats.decoded_functions), (0, 0));
        assert_eq!(stats.reused_decoded_functions, items.items.len());
        assert_eq!(
            stats.relabeled_decoded_functions,
            if relabel { items.items.len() } else { 0 }
        );
        for item in &items.items {
            let entry = restored.entries.get(&item.input_key).unwrap();
            let hint = &hints[&entry.function.id];
            assert_eq!(entry.cost, item.cost);
            assert!(entry.restored);
            assert!(
                entry
                    .function
                    .blocks
                    .ptr_eq(&hint.decoded_function().unwrap().blocks)
            );
            if !relabel {
                assert!(Arc::ptr_eq(
                    &entry.function,
                    hint.decoded_function().unwrap()
                ));
            }
            assert!(Arc::ptr_eq(
                &entry.encoding.get().unwrap().shared_bytes(),
                &item.bytes
            ));
        }
        let actual = restored
            .check_prepared_program(&prepared(1), &target, &limits)
            .unwrap();
        assert_eq!(
            actual,
            check_program(prepared(1).program(), &target, &limits).unwrap()
        );
        assert_eq!(
            restored.last_stats().restored_reused_functions,
            items.items.len()
        );
    }
}

#[test]
fn wrong_or_encoded_only_hint_runs_the_original_decoder_once() {
    let source = session();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let items = products(&source, &limits);
    let hints = semantic_hints(&source, &limits, false);
    let wrong = hints.values().next().unwrap().clone();
    let encoded_only =
        zil_ir::wire::encode_raw_function(wrong.decoded_function().unwrap(), &limits).unwrap();
    for hint in [Some(wrong), Some(encoded_only), None] {
        let mut restored = FunctionSession::default();
        let mut stats = RawFunctionDecodingStats::default();
        assert!(restored.restore_item_products_with_decoded(
            &items.manifest,
            &mut |key| loader(&items.items, key),
            &mut |_| hint.clone(),
            &target,
            &limits,
            &mut stats,
        ));
        assert_eq!(stats.decode_attempts, stats.decoded_functions);
        assert_eq!(
            stats.decoded_functions + stats.reused_decoded_functions,
            items.items.len()
        );
        if hint
            .as_ref()
            .is_none_or(|hint| hint.decoded_function().is_none())
        {
            assert_eq!(stats.decoded_functions, items.items.len());
        } else {
            assert_eq!(stats.reused_decoded_functions, 1);
            assert_eq!(stats.decoded_functions, items.items.len() - 1);
        }
    }
}

#[test]
fn shared_decoding_never_renews_aggregate_quotas_or_installs_partial_entries() {
    let source = session();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let items = products(&source, &defaults);
    let exact_bytes = legacy_bytes(items.items.len(), &target, &defaults).unwrap()
        + items
            .items
            .iter()
            .map(|item| item.bytes.len())
            .sum::<usize>();
    let exact_nodes = items.items.len()
        + items
            .items
            .iter()
            .map(|item| item.wire_nodes)
            .sum::<usize>();
    let exact_limits = Limits {
        max_source_bytes: exact_bytes,
        max_nodes: exact_nodes,
        ..defaults.clone()
    };
    let exact_hints = semantic_hints(&source, &exact_limits, false);
    let mut exact = FunctionSession::default();
    let mut exact_stats = RawFunctionDecodingStats::default();
    assert!(exact.restore_item_products_with_decoded(
        &items.manifest,
        &mut |key| loader(&items.items, key),
        &mut |bytes| exact_hints.get(&raw_scalar_function_id(bytes)?).cloned(),
        &target,
        &exact_limits,
        &mut exact_stats,
    ));
    assert_eq!(exact_stats.reused_decoded_functions, items.items.len());
    assert_eq!(exact_stats.decode_attempts, 0);
    for (bytes, nodes, expected) in [
        (exact_bytes, exact_nodes, true),
        (exact_bytes - 1, exact_nodes, false),
        (exact_bytes, exact_nodes - 1, false),
    ] {
        let limits = Limits {
            max_source_bytes: bytes,
            max_nodes: nodes,
            ..defaults.clone()
        };
        let mut restored = source.checkpoint();
        let before = restored.entries.clone();
        let mut stats = RawFunctionDecodingStats::default();
        let hints = semantic_hints(&source, &defaults, false);
        let actual = restored.restore_item_products_with_decoded(
            &items.manifest,
            &mut |key| loader(&items.items, key),
            &mut |bytes| hints.get(&raw_scalar_function_id(bytes)?).cloned(),
            &target,
            &limits,
            &mut stats,
        );
        assert_eq!(actual, expected);
        // Limits differ from the semantic decode; even valid data must use
        // the original current-limits decoder instead of borrowing its receipt.
        assert_eq!(stats.reused_decoded_functions, 0);
        if !expected {
            assert!(Arc::ptr_eq(&before, &restored.entries));
        }
    }
    let hints = semantic_hints(&source, &defaults, false);
    let mut forged = items.manifest.clone();
    let nodes = u64_at(&forged, HEADER + 80).unwrap();
    forged[HEADER + 80..HEADER + 88].copy_from_slice(&((nodes - 1) as u64).to_le_bytes());
    let mut restored = source.checkpoint();
    let before = restored.entries.clone();
    assert!(!restored.restore_item_products_with_decoded(
        &forged,
        &mut |key| loader(&items.items, key),
        &mut |bytes| hints.get(&raw_scalar_function_id(bytes)?).cloned(),
        &target,
        &defaults,
        &mut RawFunctionDecodingStats::default(),
    ));
    assert!(Arc::ptr_eq(&before, &restored.entries));
    let mut loads = 0;
    let mut partial_stats = RawFunctionDecodingStats::default();
    assert!(!restored.restore_item_products_with_decoded(
        &items.manifest,
        &mut |key| {
            loads += 1;
            (loads == 1).then(|| loader(&items.items, key)).flatten()
        },
        &mut |bytes| hints.get(&raw_scalar_function_id(bytes)?).cloned(),
        &target,
        &defaults,
        &mut partial_stats,
    ));
    assert_eq!(loads, 2);
    assert_eq!(partial_stats.reused_decoded_functions, 1);
    assert!(Arc::ptr_eq(&before, &restored.entries));
}

#[test]
fn shared_decode_keeps_all_six_fees_and_current_context_checks() {
    let source = session();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let items = products(&source, &limits);
    let hints = semantic_hints(&source, &limits, true);
    let mut restored = FunctionSession::default();
    assert!(restored.restore_item_products_with_decoded(
        &items.manifest,
        &mut |key| loader(&items.items, key),
        &mut |bytes| hints.get(&raw_scalar_function_id(bytes)?).cloned(),
        &target,
        &limits,
        &mut RawFunctionDecodingStats::default(),
    ));
    for item in &items.items {
        assert_eq!(restored.entries[&item.input_key].cost, item.cost);
    }
    // A body change rebuilds under the genuine current complete context;
    // a valid raw transport owner never supplies that context or its proof.
    let changed = prepared(2);
    let actual = restored
        .check_prepared_program(&changed, &target, &limits)
        .unwrap();
    assert_eq!(
        actual,
        check_program(changed.program(), &target, &limits).unwrap()
    );
    assert_eq!(restored.last_stats().executed_functions, 1);
    assert_eq!(restored.last_stats().restored_reused_functions, 1);
    // The complete original Limits context remains an input even for a quota
    // this scalar body does not consume. Raw decoder owners cannot retarget it.
    let changed_limits = Limits {
        max_ct_steps: limits.max_ct_steps - 1,
        ..limits
    };
    let actual = restored
        .check_prepared_program(&changed, &target, &changed_limits)
        .unwrap();
    assert_eq!(
        actual,
        check_program(changed.program(), &target, &changed_limits).unwrap()
    );
    assert_eq!(restored.last_stats().executed_functions, items.items.len());
    assert_eq!(restored.last_stats().restored_reused_functions, 0);
}
