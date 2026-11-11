use super::*;
use zil_expand::{
    ExpandSession, ProducedProgram,
    source_context::{DeclarationPlacement, SourceIdentityContext},
};
use zil_syntax::{ParsedSource, attributes::AttributeContext};

fn parse(index: u32, tail: &str) -> ParsedSource {
    let source = zil_base::SourceUnit::new(
        FileId(index),
        ModuleId(index),
        format!("module{index}.zil"),
        format!("fn f(x:i32)->i32{{if x<10{{return x+{tail};}}else{{return x*2;}}}}"),
    );
    let parsed = ParsedSource::parse(
        Arc::new(source),
        &Limits::default(),
        &AttributeContext::default(),
    );
    assert!(parsed.parsed().is_ok(), "{:?}", parsed.parsed().diagnostics);
    parsed
}

fn expand(session: &mut ExpandSession, parsed: &[ParsedSource]) -> ProducedProgram {
    let aliases = BTreeMap::new();
    let expanded = session
        .expand_parsed(
            parsed,
            &vec![vec![]; parsed.len()],
            &Limits::default(),
            &TargetProfile::default(),
            SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &aliases,
                declarations: DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .expect("plain scalar producer");
    expanded.into_program(ModuleId(0), Fingerprint::ZERO).0
}

fn owners() -> BTreeMap<ModuleId, (String, String)> {
    (0..3)
        .map(|i| (ModuleId(i), ("project".into(), format!("module{i}.zil"))))
        .collect()
}

fn check(
    session: &mut FunctionSession,
    prepared: &PreparedProgram,
    graph: bool,
) -> CompileResult<TypedProgram> {
    if graph {
        session.check_prepared_program_with_owners(
            prepared,
            &TargetProfile::default(),
            &Limits::default(),
            &owners(),
        )
    } else {
        session.check_prepared_program_with_producer_owners(
            prepared,
            &TargetProfile::default(),
            &Limits::default(),
            &owners(),
        )
    }
}

#[test]
fn simple_memo_and_graph_use_the_same_producer_versions_and_content_keys() {
    let mut producer = ExpandSession::default();
    let mut parsed = vec![parse(0, "4"), parse(1, "4"), parse(2, "4")];
    let mut simple = FunctionSession::default();
    let mut graph = FunctionSession::default();
    let mut legacy = FunctionSession::default();
    for (edit, executions, reuses) in [(false, 3, 0), (false, 0, 3), (true, 1, 2)] {
        if edit {
            parsed[1] = parse(1, "5");
        }
        let produced = expand(&mut producer, &parsed);
        let prepared = preparation::infer_produced_program_for_ct(&produced, &Limits::default())
            .unwrap()
            .into_prepared();
        let expected = check_program(
            prepared.program(),
            &TargetProfile::default(),
            &Limits::default(),
        );
        assert_eq!(check(&mut simple, &prepared, false), expected);
        assert_eq!(check(&mut graph, &prepared, true), expected);
        assert_eq!(
            legacy.check_prepared_program(&prepared, &TargetProfile::default(), &Limits::default()),
            expected
        );
        for session in [&simple, &graph] {
            let stats = session.last_stats();
            assert_eq!(
                (stats.body_key_executions, stats.body_key_reuses),
                (executions, reuses)
            );
            assert_eq!(stats.core_function_eq_comparisons, 0);
            assert_eq!(stats.producer_version_reuses, reuses);
            assert!(!stats.cost_profiled);
            assert_eq!(stats.total_ns, 0);
            assert_eq!(
                session.encode_work_products(&TargetProfile::default(), &Limits::default()),
                legacy.encode_work_products(&TargetProfile::default(), &Limits::default())
            );
        }
        assert_eq!(legacy.last_stats().body_key_executions, 3);
        if executions == 0 {
            assert_eq!(graph.last_stats().graph.inventory_visits, 0);
        }
    }
}

#[test]
fn raw_core_and_materialized_evaluation_cannot_keep_producer_authority() {
    let parsed = vec![parse(0, "4"), parse(1, "4"), parse(2, "4")];
    let produced = expand(&mut ExpandSession::default(), &parsed);
    let limits = Limits::default();
    let prepared = preparation::infer_produced_program_for_ct(&produced, &limits)
        .unwrap()
        .into_prepared();
    let mut simple = FunctionSession::default();
    let mut graph = FunctionSession::default();
    check(&mut simple, &prepared, false).unwrap();
    check(&mut graph, &prepared, true).unwrap();
    let mut editable = produced.program().clone();
    Arc::make_mut(&mut editable).source_fingerprint = Fingerprint::of(b"untrusted caller metadata");
    assert_eq!(produced.program().source_fingerprint, Fingerprint::ZERO);
    let raw = infer_shared_program_for_ct(&editable, &limits)
        .unwrap()
        .into_prepared();
    assert!(raw.module_revision(ModuleId(0)).is_none());
    assert_eq!(
        check(&mut simple, &raw, false),
        check(&mut graph, &raw, true)
    );
    assert_eq!(simple.last_stats().body_key_executions, 3);
    assert_eq!(simple.last_stats().producer_version_reuses, 0);
    assert_eq!(graph.last_stats().core_function_eq_comparisons, 3);
    let evaluated = preparation::infer_produced_program_for_ct(&produced, &limits)
        .unwrap()
        .after_evaluation((**produced.program()).clone());
    assert!(evaluated.module_revision(ModuleId(0)).is_none());
    let changed_limits = Limits {
        max_call_depth: limits.max_call_depth + 1,
        ..limits
    };
    simple
        .check_prepared_program_with_producer_owners(
            &prepared,
            &TargetProfile::default(),
            &changed_limits,
            &owners(),
        )
        .unwrap();
    assert_eq!(simple.last_stats().producer_version_reuses, 0);
}

#[test]
fn failure_cannot_publish_partial_versions_and_profiled_costs_are_nested_once() {
    let mut producer = ExpandSession::default();
    let mut parsed = vec![parse(0, "4"), parse(1, "4"), parse(2, "4")];
    let original = parsed.clone();
    let mut simple = FunctionSession::default();
    let mut graph = FunctionSession::default();
    simple.set_cost_profiling(true);
    graph.set_cost_profiling(true);
    for valid in [true, false, true] {
        parsed[1] = if valid {
            original[1].clone()
        } else {
            parse(1, "missing")
        };
        let produced = expand(&mut producer, &parsed);
        let prepared = preparation::infer_produced_program_for_ct(&produced, &Limits::default())
            .unwrap()
            .into_prepared();
        let expected = check_program(
            prepared.program(),
            &TargetProfile::default(),
            &Limits::default(),
        );
        assert_eq!(check(&mut simple, &prepared, false), expected);
        assert_eq!(check(&mut graph, &prepared, true), expected);
        assert_eq!(expected.is_ok(), valid);
        if !valid {
            assert!(simple.producer_memo.entries.is_empty());
            assert!(graph.projections.source.is_none());
        }
        for session in [&simple, &graph] {
            let stats = session.last_stats();
            assert!(stats.cost_profiled);
            assert!(stats.total_ns > 0);
            assert!(
                stats.total_ns
                    >= stats.shape_resolution_ns
                        + stats.context_construction_ns
                        + stats.projection_context_ns
                        + stats.core_validation_ns
                        + stats.inventory_ns
                        + stats.graph_maintenance_ns
                        + stats.evaluator_ns
                        + stats.cfg_reuse_ns
                        + stats.cfg_execute_ns
                        + stats.cfg_store_ns
            );
            if stats.evaluator_ns > 0 {
                assert!(stats.evaluator_ns >= stats.body_key_ns);
            }
        }
    }
    assert_eq!(simple.last_stats().body_key_executions, 3);
    assert_eq!(graph.last_stats().body_key_executions, 3);
}
