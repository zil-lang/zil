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

fn parse_functions(text: &str) -> ParsedSource {
    let source = Arc::new(zil_base::SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "module0.zil",
        text,
    ));
    let parsed = ParsedSource::parse(source, &Limits::default(), &AttributeContext::default());
    assert!(parsed.parsed().is_ok(), "{:?}", parsed.parsed().diagnostics);
    parsed
}

#[test]
fn a_private_edit_propagates_actual_cfg_products_inside_one_module() {
    let original = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} fn g(x:i32)->i32{if x<10{return x+3;}else{return x*4;}}";
    let mut producer = ExpandSession::default();
    let mut simple = FunctionSession::default();
    let mut graph = FunctionSession::default();
    let mut prior_g = None;
    for (text, executions, changed) in [
        (original.to_owned(), 2, 2),
        (original.to_owned(), 0, 0),
        (original.replace("x+1", "x+2"), 1, 1),
    ] {
        let produced = expand(&mut producer, &[parse_functions(&text)]);
        let prepared = preparation::infer_produced_program_for_ct(&produced, &Limits::default())
            .unwrap()
            .into_prepared();
        let expected = check_program(
            prepared.program(),
            &TargetProfile::default(),
            &Limits::default(),
        )
        .unwrap();
        for (session, use_graph) in [(&mut simple, false), (&mut graph, true)] {
            let actual = check(session, &prepared, use_graph).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(session.last_stats().body_key_executions, executions);
            assert_eq!(session.last_stats().executed_functions, executions);
            assert_eq!(session.last_products().len(), actual.module.functions.len());
            assert_eq!(session.last_changes().changed.len(), changed);
            assert_eq!(
                session.last_changes().changed.len() + session.last_changes().reused.len(),
                2
            );
            let g = session
                .last_products()
                .iter()
                .find(|p| p.name == "g")
                .unwrap();
            assert!(g.input_key.is_some());
            assert!(g.function.is_some());
            if let Some(previous) = &prior_g
                && !use_graph
            {
                assert!(Arc::ptr_eq(previous, g.function.as_ref().unwrap()));
            }
            if !use_graph {
                prior_g = g.function.clone();
            }
        }
    }
}

#[test]
fn equal_reexecution_stops_result_changes_but_relocation_stays_explicit() {
    let original = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}}";
    let mut producer = ExpandSession::default();
    let mut session = FunctionSession::default();
    let mut prior = None;
    for text in [original.to_owned(), format!("\n{original}")] {
        let produced = expand(&mut producer, &[parse_functions(&text)]);
        let prepared = preparation::infer_produced_program_for_ct(&produced, &Limits::default())
            .unwrap()
            .into_prepared();
        let result = check(&mut session, &prepared, false).unwrap();
        assert_eq!(
            result,
            check_program(
                prepared.program(),
                &TargetProfile::default(),
                &Limits::default()
            )
            .unwrap()
        );
        let product = &session.last_products()[0];
        if let Some(fingerprint) = prior {
            assert_eq!(product.result_fingerprint, fingerprint);
            assert!(session.last_changes().changed.is_empty());
            assert_eq!(session.last_changes().relocated, [product.id]);
        }
        prior = Some(product.result_fingerprint);
    }
}

#[test]
fn failed_request_cannot_publish_cfg_products_or_change_sets() {
    let original = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}}";
    let mut producer = ExpandSession::default();
    let mut session = FunctionSession::default();
    let produced = expand(&mut producer, &[parse_functions(original)]);
    let prepared = preparation::infer_produced_program_for_ct(&produced, &Limits::default())
        .unwrap()
        .into_prepared();
    check(&mut session, &prepared, false).unwrap();
    let old = session.last_products()[0].result_fingerprint;
    let old_function = session.last_products()[0].function.clone().unwrap();
    let rejected = expand(
        &mut producer,
        &[parse_functions(&original.replace("x+1", "absent_name"))],
    );
    let prepared = preparation::infer_produced_program_for_ct(&rejected, &Limits::default())
        .unwrap()
        .into_prepared();
    assert!(check(&mut session, &prepared, false).is_err());
    assert_eq!(session.last_products()[0].result_fingerprint, old);
    assert!(Arc::ptr_eq(
        session.last_products()[0].function.as_ref().unwrap(),
        &old_function
    ));
    assert_eq!(session.last_changes().changed.len(), 1);
}

#[test]
fn same_span_cfg_hits_share_all_body_vectors_and_position_moves_copy_on_write() {
    let original = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}}";
    let mut producer = ExpandSession::default();
    let mut session = FunctionSession::default();
    let produced = expand(&mut producer, &[parse_functions(original)]);
    let prepared = preparation::infer_produced_program_for_ct(&produced, &Limits::default())
        .unwrap()
        .into_prepared();
    let cold = check(&mut session, &prepared, false).unwrap();
    let warm = check(&mut session, &prepared, false).unwrap();
    assert_eq!(session.last_stats().cfg_same_span_reuses, 1);
    assert_eq!(session.last_stats().cfg_relocated_functions, 0);
    let left = &cold.module.functions[0];
    let right = &warm.module.functions[0];
    assert!(SharedVec::ptr_eq(&left.locals, &right.locals));
    assert!(SharedVec::ptr_eq(&left.values, &right.values));
    assert!(SharedVec::ptr_eq(&left.blocks, &right.blocks));
    let product = &session.last_products()[0];
    let current = product.function.as_ref().unwrap();
    assert_eq!(current.as_ref(), right);
    assert!(SharedVec::ptr_eq(&right.blocks, &current.blocks));

    let shifted = expand(&mut producer, &[parse_functions(&format!("\n{original}"))]);
    let prepared = preparation::infer_produced_program_for_ct(&shifted, &Limits::default())
        .unwrap()
        .into_prepared();
    let moved = check(&mut session, &prepared, false).unwrap();
    assert_eq!(session.last_stats().cfg_relocated_functions, 1);
    let moved_again = check(&mut session, &prepared, false).unwrap();
    assert_eq!(session.last_stats().cfg_same_span_reuses, 1);
    assert_eq!(session.last_stats().cfg_relocated_functions, 0);
    assert_eq!(
        moved,
        check_program(
            prepared.program(),
            &TargetProfile::default(),
            &Limits::default()
        )
        .unwrap()
    );
    assert!(!SharedVec::ptr_eq(
        &left.locals,
        &moved.module.functions[0].locals
    ));
    assert!(!SharedVec::ptr_eq(
        &left.blocks,
        &moved.module.functions[0].blocks
    ));
    assert!(SharedVec::ptr_eq(
        &left.values,
        &moved.module.functions[0].values
    ));
    assert!(SharedVec::ptr_eq(
        &moved.module.functions[0].locals,
        &moved_again.module.functions[0].locals
    ));
    assert!(SharedVec::ptr_eq(
        &moved.module.functions[0].blocks,
        &moved_again.module.functions[0].blocks
    ));
    assert_eq!(cold.module.functions[0].span, left.span);
}

#[test]
fn canonical_products_are_sealed_to_the_current_raw_result_not_public_fields() {
    let original = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}}";
    let mut producer = ExpandSession::default();
    let mut session = FunctionSession::default();
    let produced = expand(&mut producer, &[parse_functions(original)]);
    let prepared = preparation::infer_produced_program_for_ct(&produced, &Limits::default())
        .unwrap()
        .into_prepared();
    let cold = check(&mut session, &prepared, false).unwrap();
    let mut product = session.last_products()[0].clone();
    let actual = &cold.module.functions[0];
    let identity = product.canonical_identity(actual).unwrap();
    product.result_fingerprint = Fingerprint::ZERO;
    assert_eq!(product.canonical_identity(actual), Some(identity));
    let mut forged = actual.clone();
    forged.return_type = Type::Unit;
    product.function = Some(Arc::new(forged.clone()));
    assert!(product.canonical_identity(&forged).is_none());
    assert!(product.canonical_function(&forged).is_none());
    let produced = expand(&mut producer, &[parse_functions(&format!("\n{original}"))]);
    let prepared = preparation::infer_produced_program_for_ct(&produced, &Limits::default())
        .unwrap()
        .into_prepared();
    let moved = check(&mut session, &prepared, false).unwrap();
    let current = &moved.module.functions[0];
    assert!(product.canonical_identity(current).is_none());
    let current_product = &session.last_products()[0];
    assert_eq!(current_product.canonical_identity(current), Some(identity));
    let canonical = current_product.canonical_function(current).unwrap();
    assert_eq!(
        canonical.span,
        Span::new(FileId(0), 0, current.span.end - current.span.start)
    );
    assert_eq!(canonical.name, current.name);
}

#[test]
fn unchanged_relative_inputs_skip_bodykey_walks_but_current_cfg_positions_and_wire_match_fresh() {
    let original = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} fn g(x:i32)->i32{if x<10{return x+3;}else{return x*4;}}";
    for use_graph in [false, true] {
        let mut producer = ExpandSession::default();
        let mut session = FunctionSession::default();
        let wider = original.replace("x+1", "x+117");
        for (text, keys, relocated) in [
            (original.to_owned(), (2, 0), 0),
            (wider.clone(), (1, 1), 1),
            (format!("\n\n{wider}"), (0, 2), 2),
            (format!("\n\n{}", wider.replace("x+3", "x + 3")), (1, 1), 0),
        ] {
            let produced = expand(&mut producer, &[parse_functions(&text)]);
            let prepared =
                preparation::infer_produced_program_for_ct(&produced, &Limits::default())
                    .unwrap()
                    .into_prepared();
            let actual = check(&mut session, &prepared, use_graph).unwrap();
            let mut fresh = FunctionSession::default();
            let expected = fresh
                .check_prepared_program(&prepared, &TargetProfile::default(), &Limits::default())
                .unwrap();
            assert_eq!(actual, expected);
            let stats = session.last_stats();
            assert_eq!((stats.body_key_executions, stats.body_key_reuses), keys);
            assert_eq!(stats.cfg_relocated_functions, relocated);
            assert_eq!(stats.core_function_eq_comparisons, 0);
            assert_eq!(
                session.encode_work_products(&TargetProfile::default(), &Limits::default()),
                fresh.encode_work_products(&TargetProfile::default(), &Limits::default()),
            );
            for product in session.last_products() {
                let current = actual
                    .module
                    .functions
                    .iter()
                    .find(|f| f.id == product.id)
                    .unwrap();
                assert_eq!(product.source_span, current.span);
                assert_eq!(product.function.as_deref(), Some(current));
            }
        }
    }
}

#[test]
fn raw_or_rewritten_core_cannot_borrow_a_relative_input_version() {
    let original = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} fn g(x:i32)->i32{if x<10{return x+3;}else{return x*4;}}";
    let produced = expand(&mut ExpandSession::default(), &[parse_functions(original)]);
    let limits = Limits::default();
    let prepared = preparation::infer_produced_program_for_ct(&produced, &limits)
        .unwrap()
        .into_prepared();
    assert!(prepared.function_input_revision(ModuleId(0), "g").is_some());
    let mut raw = (**produced.program()).clone();
    // Deliberately preserve caller-visible source fingerprints and every span.
    // Changing the actual raw body still destroys the private producer route.
    let mut other = (**expand(
        &mut ExpandSession::default(),
        &[parse_functions(&original.replace("x+3", "x+9"))],
    )
    .program())
    .clone();
    Arc::make_mut(&mut other.modules[0].core).source_fingerprint =
        raw.modules[0].core.source_fingerprint;
    raw.modules[0].core = other.modules.remove(0).core;
    let raw = infer_program_for_ct(&raw, &limits).unwrap().into_prepared();
    let evaluated = preparation::infer_produced_program_for_ct(&produced, &limits)
        .unwrap()
        .after_evaluation((**produced.program()).clone());
    for current in [&raw, &evaluated] {
        assert!(current.function_input_revision(ModuleId(0), "g").is_none());
        for graph in [false, true] {
            let mut session = FunctionSession::default();
            check(&mut session, &prepared, graph).unwrap();
            let actual = check(&mut session, current, graph);
            assert_eq!(
                actual,
                check_program(current.program(), &TargetProfile::default(), &limits)
            );
            assert_eq!(session.last_stats().producer_version_reuses, 0);
        }
    }
}
