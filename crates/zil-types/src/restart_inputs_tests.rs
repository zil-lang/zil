use super::*;
use zil_expand::{
    ExpandSession, ProducedProgram,
    source_context::{DeclarationPlacement, SourceIdentityContext},
};
use zil_ir::wire::FunctionEncodingStats;
use zil_syntax::{ParsedSource, attributes::AttributeContext};

fn sources(texts: &[String]) -> Vec<ParsedSource> {
    texts
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let parsed = ParsedSource::parse(
                Arc::new(zil_base::SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("m{i}.zil"),
                    text.as_str(),
                )),
                &Limits::default(),
                &AttributeContext::default(),
            );
            assert!(parsed.parsed().is_ok(), "{:?}", parsed.parsed().diagnostics);
            parsed
        })
        .collect()
}
fn expand(session: &mut ExpandSession, texts: &[String]) -> ProducedProgram {
    session
        .expand_parsed(
            &sources(texts),
            &vec![vec![]; texts.len()],
            &Limits::default(),
            &TargetProfile::default(),
            SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .unwrap()
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0
}
fn prepare(produced: &ProducedProgram, raw: &mut RawInferenceSession) -> PreparedProgram {
    infer_produced_program_for_ct_with_session(produced, &Limits::default(), raw)
        .unwrap()
        .into_prepared()
}
fn load(items: &FunctionItemProducts, key: Fingerprint) -> Option<Arc<[u8]>> {
    items
        .items
        .iter()
        .find(|item| item.content_fingerprint == key)
        .map(|item| item.bytes.clone())
}
fn encode(functions: &FunctionSession) -> FunctionItemProducts {
    functions
        .encode_item_products(
            &TargetProfile::default(),
            &Limits::default(),
            &mut FunctionEncodingStats::default(),
        )
        .unwrap()
}
fn text(count: usize) -> String {
    (0..count)
        .map(|i| format!("fn f{i}(x:i32)->i32 {{if x<10 {{return x+1;}}else{{return x*2;}}}} "))
        .collect()
}

#[test]
fn fused_cached_cfg_copy_matches_original_rebase_and_preserves_raw_owners() {
    let produced = expand(&mut ExpandSession::default(), &[text(3)]);
    let program = check_program(
        produced.program(),
        &TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    for source in &program.module.functions {
        let unchanged = source.clone();
        let length = source.span.end - source.span.start;
        for new in [
            Span::new(FileId(7), 200, 200 + length),
            Span::new(FileId(0), 0, length),
            Span::new(FileId(0), 0, length - 1),
            Span::new(FileId(0), usize::MAX - 1, usize::MAX),
        ] {
            let mut expected = source.clone();
            let success = rebase(&mut expected, source.span, new);
            let actual = relocated_copy(source, source.span, new);
            assert_eq!(actual, success.then_some(expected));
            if let Some(actual) = actual {
                assert!(actual.values.ptr_eq(&source.values));
                assert!(!actual.locals.ptr_eq(&source.locals));
                assert!(!actual.blocks.ptr_eq(&source.blocks));
            }
            assert_eq!(source, &unchanged);
        }
        for wrong_file in [false, true] {
            let mut invalid = source.clone();
            let end = invalid.span.end;
            let instruction = invalid
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.instructions)
                .last()
                .unwrap();
            if wrong_file {
                instruction.span.file = FileId(99);
            } else {
                instruction.span.end = end + 1;
            }
            let mut old = invalid.clone();
            let new = Span::new(FileId(7), 200, 200 + length);
            assert!(!rebase(&mut old, invalid.span, new));
            assert!(relocated_copy(&invalid, invalid.span, new).is_none());
        }
    }
}

#[test]
fn fresh_restart_first_edit_reconstructs_current_inputs_and_consumes_persisted_items() {
    let target = TargetProfile::default();
    let limits = Limits::default();
    let mut texts = vec![text(12); 4];
    let mut producer = ExpandSession::default();
    let mut raw = RawInferenceSession::default();
    let mut functions = FunctionSession::default();
    let mut push = TypesPushSession::default();
    let initial = expand(&mut producer, &texts);
    let prepared = prepare(&initial, &mut raw);
    let first = push
        .apply_with_function_cache(&prepared, &target, &limits, None, &mut functions)
        .unwrap();
    assert_eq!(functions.last_stats().body_key_current_projections, 48);
    let items = encode(&functions);
    let frontend = producer.encode_frontend_products(&limits).unwrap();
    // No successful producer, raw inference, key memo or push Context survives.
    drop((producer, raw, functions, push, initial, prepared, first));
    texts[0] = texts[0].replacen("x+1", "x+111", 1);
    let mut producer = ExpandSession::default();
    assert!(producer.restore_frontend_products(frontend, &limits));
    let mut functions = FunctionSession::default();
    assert!(functions.restore_item_products(
        &items.manifest,
        &mut |key| load(&items, key),
        &target,
        &limits,
    ));
    let current = expand(&mut producer, &texts);
    let mut raw = RawInferenceSession::default();
    let inferred = infer_produced_program_for_ct_with_session(&current, &limits, &mut raw).unwrap();
    assert_eq!(inferred.raw_inference_stats().unwrap().full_runs, 1);
    assert_eq!(
        inferred.raw_inference_stats().unwrap().body_inputs_produced,
        48
    );
    let prepared = inferred.into_prepared();
    let mut push = TypesPushSession::default();
    let update = push
        .apply_with_function_cache(&prepared, &target, &limits, None, &mut functions)
        .unwrap();
    assert_eq!(
        update.program.as_ref(),
        &check_program(current.program(), &target, &limits).unwrap()
    );
    assert_eq!(
        update.stats.context_rebuilds, 1,
        "current context is reconstructed, not deserialized"
    );
    assert_eq!(update.stats.body_checks, 1);
    assert_eq!(update.stats.post_functions_observed, 48);
    assert_eq!(update.stats.post_functions_rechecked, 0);
    let stats = functions.last_stats();
    assert_eq!(stats.body_key_executions, 0);
    assert_eq!(stats.body_key_current_projections, 48);
    assert_eq!(stats.names_current_replays, 48);
    assert_eq!(
        (
            stats.shape_functions_observed,
            stats.shape_functions_checked
        ),
        (48, 0)
    );
    assert_eq!(stats.restored_reused_functions, 47);
    assert_eq!(stats.cfg_same_span_reuses, 0);
    assert_eq!(
        stats.cfg_relocated_functions, 47,
        "canonical raw CFGs still need current absolute placements"
    );
    let mut counts = FunctionEncodingStats::default();
    functions
        .encode_item_products(&target, &limits, &mut counts)
        .unwrap();
    assert_eq!(
        (counts.serialized_functions, counts.reused_functions),
        (1, 47)
    );
    // The fresh current reader graph also reaches the normal sparse consumer.
    texts[1] = texts[1].replacen("x+1", "x+5", 1);
    let next = expand(&mut producer, &texts);
    let prepared = prepare(&next, &mut raw);
    let update = push
        .apply_with_function_cache(&prepared, &target, &limits, None, &mut functions)
        .unwrap();
    assert_eq!(
        update.program.as_ref(),
        &check_program(next.program(), &target, &limits).unwrap()
    );
    assert_eq!(
        (update.stats.sparse_batches, update.stats.body_checks),
        (1, 1)
    );
    assert_eq!(update.stats.post_functions_rechecked, 1);
}

#[test]
fn fresh_current_signature_reads_reject_changed_callee_and_negative_lookups() {
    let target = TargetProfile::default();
    let limits = Limits::default();
    let initial_text = "fn callee(x:i32)->i32=>x+1; fn caller(x:i32)->i32=>callee(x);";
    let initial = expand(&mut ExpandSession::default(), &[initial_text.into()]);
    let mut raw = RawInferenceSession::default();
    let prepared = prepare(&initial, &mut raw);
    let mut functions = FunctionSession::default();
    TypesPushSession::default()
        .apply_with_function_cache(&prepared, &target, &limits, None, &mut functions)
        .unwrap();
    let items = encode(&functions);
    for edit in [
        initial_text.replace("callee(x:i32)", "callee(x:i32,y:i32)"),
        initial_text.replace("=>callee(x)", "=>missing(x)"),
        initial_text.replace("fn callee", "fn renamed"),
    ] {
        let produced = expand(&mut ExpandSession::default(), &[edit]);
        let prepared = prepare(&produced, &mut RawInferenceSession::default());
        let mut restored = FunctionSession::default();
        assert!(restored.restore_item_products(
            &items.manifest,
            &mut |key| load(&items, key),
            &target,
            &limits
        ));
        let actual = TypesPushSession::default().apply_with_function_cache(
            &prepared,
            &target,
            &limits,
            None,
            &mut restored,
        );
        assert_eq!(
            actual.err().unwrap(),
            check_program(produced.program(), &target, &limits)
                .err()
                .unwrap()
        );
    }
    // The successful restored graph must invalidate an unchanged caller when
    // a subsequent same-process request edits only the callee's signature.
    let mut producer = ExpandSession::default();
    let produced = expand(&mut producer, &[initial_text.into()]);
    let mut raw = RawInferenceSession::default();
    let prepared = prepare(&produced, &mut raw);
    let mut functions = FunctionSession::default();
    assert!(functions.restore_item_products(
        &items.manifest,
        &mut |key| load(&items, key),
        &target,
        &limits
    ));
    let mut push = TypesPushSession::default();
    push.apply_with_function_cache(&prepared, &target, &limits, None, &mut functions)
        .unwrap();
    let current = expand(
        &mut producer,
        &[initial_text.replace("callee(x:i32)", "callee(x:i32,y:i32)")],
    );
    let prepared = prepare(&current, &mut raw);
    assert_eq!(
        push.apply_with_function_cache(&prepared, &target, &limits, None, &mut functions)
            .err()
            .unwrap(),
        check_program(current.program(), &target, &limits)
            .err()
            .unwrap(),
    );
}

#[test]
fn current_inputs_do_not_cross_limits_or_ct_materialization_and_preserve_errors() {
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let produced = expand(&mut ExpandSession::default(), &[text(8)]);
    let mut raw = RawInferenceSession::default();
    let prepared = prepare(&produced, &mut raw);
    for (nodes, work, depth) in [
        (0, defaults.max_analysis_steps, defaults.max_call_depth),
        (defaults.max_nodes, 1, defaults.max_call_depth),
        (defaults.max_nodes, defaults.max_analysis_steps, 1),
    ] {
        let limits = Limits {
            max_nodes: nodes,
            max_analysis_steps: work,
            max_call_depth: depth,
            ..defaults.clone()
        };
        assert!(prepared.current_inputs(&limits).is_none());
        let mut functions = FunctionSession::default();
        assert_eq!(
            functions.check_prepared_program(&prepared, &target, &limits),
            check_program(produced.program(), &target, &limits)
        );
        assert_eq!(functions.last_stats().names_current_replays, 0);
    }
    let inferred =
        infer_produced_program_for_ct_with_session(&produced, &defaults, &mut raw).unwrap();
    let materialized = inferred.program().clone();
    let prepared = inferred.after_evaluation(materialized);
    assert!(prepared.current_inputs(&defaults).is_none());
    let mut functions = FunctionSession::default();
    assert_eq!(
        functions.check_prepared_program(&prepared, &target, &defaults),
        check_program(produced.program(), &target, &defaults)
    );
    assert_eq!(functions.last_stats().body_key_current_projections, 0);
}

#[test]
fn fresh_current_observations_preserve_original_aggregate_work_and_error_prefixes() {
    let produced = expand(&mut ExpandSession::default(), &[text(6)]);
    let target = TargetProfile::default();
    let defaults = Limits::default();
    for work in [true, false] {
        let mut low = 0;
        let mut high = if work {
            defaults.max_analysis_steps
        } else {
            defaults.max_nodes
        };
        while low < high {
            let mid = low + (high - low) / 2;
            let mut limits = defaults.clone();
            if work {
                limits.max_analysis_steps = mid;
            } else {
                limits.max_nodes = mid;
            }
            if check_program(produced.program(), &target, &limits).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        for value in [0, 1, low.saturating_sub(1), low, low + 1] {
            let mut limits = defaults.clone();
            if work {
                limits.max_analysis_steps = value;
            } else {
                limits.max_nodes = value;
            }
            let actual = infer_produced_program_for_ct_with_session(
                &produced,
                &limits,
                &mut RawInferenceSession::default(),
            )
            .and_then(|inferred| {
                let prepared = inferred.into_prepared();
                TypesPushSession::default()
                    .apply_with_function_cache(
                        &prepared,
                        &target,
                        &limits,
                        None,
                        &mut FunctionSession::default(),
                    )
                    .map(|update| update.program.as_ref().clone())
            });
            assert_eq!(
                actual,
                check_program(produced.program(), &target, &limits),
                "work={work} value={value}"
            );
        }
    }
}
