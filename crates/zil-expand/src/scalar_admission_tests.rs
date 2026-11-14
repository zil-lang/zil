use super::*;
use source_context::{DeclarationPlacement, SourceIdentityContext};
use zil_syntax::attributes::AttributeContext;

type Output = Vec<(ProgramModule, Arc<SourceMap>)>;
type FixtureInputs = (
    Vec<ParsedSource>,
    Vec<Vec<CoreImport>>,
    BTreeMap<Vec<String>, ModuleId>,
);

fn parsed(id: u32, text: &str) -> ParsedSource {
    let input = ParsedSource::parse(
        Arc::new(SourceUnit::new(
            FileId(id),
            ModuleId(id),
            format!("m{id}.zil"),
            text,
        )),
        &Limits::default(),
        &AttributeContext::default(),
    );
    assert!(input.parsed().is_ok(), "{:?}", input.parsed().diagnostics);
    input
}

fn context(aliases: &BTreeMap<Vec<String>, ModuleId>) -> SourceIdentityContext<'_> {
    SourceIdentityContext::ClosedSnapshot {
        root: ModuleId(0),
        aliases,
        declarations: DeclarationPlacement::Original,
    }
}

fn raw(
    inputs: &[ParsedSource],
    imports: &[Vec<CoreImport>],
    limits: &Limits,
) -> CompileResult<Output> {
    let sources = inputs.iter().map(ParsedSource::source).collect::<Vec<_>>();
    let modules = inputs
        .iter()
        .map(|p| &p.parsed().module)
        .collect::<Vec<_>>();
    expand_program_with_limits_target(
        &sources,
        &modules,
        imports,
        limits,
        &TargetProfile::default(),
    )
    .map(|modules| {
        modules
            .into_iter()
            .map(|(m, map)| (m, Arc::new(map)))
            .collect()
    })
}

fn run(
    session: &mut ExpandSession,
    inputs: &[ParsedSource],
    imports: &[Vec<CoreImport>],
    aliases: &BTreeMap<Vec<String>, ModuleId>,
    limits: &Limits,
) -> CompileResult<Output> {
    for input in inputs {
        session.parsed_scalar(input, limits);
    }
    let (origins, remaining) = session.capture_original_parsed(inputs, context(aliases), limits)?;
    assert!(
        origins.is_none(),
        "this helper exercises the plain scalar chain"
    );
    let result = session.expand_parsed(
        inputs,
        imports,
        &remaining,
        &TargetProfile::default(),
        SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases,
            // This is the existing unchanged materializer's conservative marker.
            declarations: DeclarationPlacement::Unavailable,
        },
    )?;
    match result {
        Some(program) => Ok(program.modules),
        None => raw(inputs, imports, &remaining),
    }
}

fn fixture(prefix: &str) -> FixtureInputs {
    let inputs = vec![
        parsed(
            0,
            &format!(
                "{prefix}import dep; fn main(x:i32)->i32{{let y=x+1;if y<10{{return dep.f(y);}}else{{return y*2;}}}}"
            ),
        ),
        parsed(
            1,
            &format!(
                "{prefix}pub fn f(x:i32)->i32=>x+3; fn unused(x:i32)->i32{{if x<2{{return x+4;}}else{{return x*5;}}}}"
            ),
        ),
    ];
    let imports = vec![
        vec![CoreImport {
            alias: "dep".into(),
            module: ModuleId(1),
            span: inputs[0].parsed().module.items[0].span,
        }],
        vec![],
    ];
    (
        inputs,
        imports,
        BTreeMap::from([(vec!["dep".into()], ModuleId(1))]),
    )
}

#[test]
fn fused_scalar_chain_executes_less_work_and_preserves_full_outputs_and_maps() {
    let (inputs, imports, aliases) = fixture("");
    let limits = Limits::default();
    let mut session = ExpandSession::default();
    let cold = run(&mut session, &inputs, &imports, &aliases, &limits).unwrap();
    assert_eq!(cold, raw(&inputs, &imports, &limits).unwrap());
    assert_eq!(session.last_stats().admission_classifier_hits, 2);
    assert_eq!(
        session.last_stats().classifier_nodes,
        0,
        "classification already ran in parsed_scalar"
    );
    assert_eq!(session.last_stats().fused_distributed_items, 3);
    let fused_nodes = session.last_stats().visited_nodes;
    for entry in session.entries.values() {
        assert!(
            !entry
                .receipts
                .keys()
                .any(|(stage, _)| *stage == Stage::Distribute)
        );
    }
    let warm = run(&mut session, &inputs, &imports, &aliases, &limits).unwrap();
    assert_eq!(cold, warm);
    assert_eq!(session.last_stats().normalize_modules, 0);
    assert_eq!(session.last_stats().normalized_materializations, 0);
    for ((before, before_map), (after, after_map)) in cold.iter().zip(&warm) {
        assert!(Arc::ptr_eq(&before.core, &after.core));
        assert!(Arc::ptr_eq(before_map, after_map));
    }
    // No capture admission: the same public parsed API retains its complete
    // old Distribute visitor and real receipts, even after a fused success.
    session
        .expand_parsed(
            &inputs,
            &imports,
            &limits,
            &TargetProfile::default(),
            context(&aliases),
        )
        .unwrap()
        .unwrap();
    assert_eq!(session.last_stats().fused_distributed_items, 0);
    let old_distribute_nodes: usize = session
        .entries
        .values()
        .flat_map(|entry| entry.receipts.iter())
        .filter(|((stage, _), _)| *stage == Stage::Distribute)
        .map(|(_, r)| r.nodes)
        .sum();
    assert!(old_distribute_nodes > 0);
    assert_eq!(
        session.last_stats().visited_nodes,
        fused_nodes + old_distribute_nodes as u64
    );
    assert_eq!(
        run(&mut session, &inputs, &imports, &aliases, &limits).unwrap(),
        cold
    );
    assert_eq!(session.last_stats().fused_distributed_items, 3);
}

#[test]
fn scalar_capture_requires_actual_classification_and_exact_current_parser_owner() {
    let input = [parsed(0, "fn f(x:i32)->i32=>x+1;")];
    let aliases = BTreeMap::new();
    let limits = Limits::default();
    let session = ExpandSession::default();
    let sources = input.iter().map(ParsedSource::source).collect::<Vec<_>>();
    let modules = input.iter().map(|p| &p.parsed().module).collect::<Vec<_>>();
    let original =
        source_context::capture_original(&sources, &modules, context(&aliases), &limits).unwrap();
    // No proof means the public wrapper executes original discovery, with its
    // original exact demand rather than accepting a caller-supplied no-CT bit.
    assert_eq!(
        session
            .capture_original_parsed(&input, context(&aliases), &limits)
            .unwrap(),
        original
    );
    assert!(session.parsed_scalar(&input[0], &limits));
    let fused = session
        .capture_original_parsed(&input, context(&aliases), &limits)
        .unwrap();
    assert!(fused.0.is_none());
    assert_eq!(fused.1.max_nodes, limits.max_nodes - 1);
    assert!(fused.1.max_nodes > original.1.max_nodes);
    assert!(fused.1.max_analysis_steps > original.1.max_analysis_steps);
    // Byte-identical reparsing is a different immutable parser owner.
    let reparsed = [parsed(0, &input[0].source().text)];
    assert_eq!(
        session
            .capture_original_parsed(&reparsed, context(&aliases), &limits)
            .unwrap(),
        original
    );
    // A COW raw tree is not the parser's tree and cannot supply a fact.
    let mut forged = input[0].parsed_shared();
    Arc::make_mut(&mut forged).module.items.clear();
    assert!(!input[0].parsed().module.items.is_empty());
    assert!(session.parsed_scalar(&reparsed[0], &limits));
    let shallow = Limits {
        max_call_depth: 0,
        ..limits.clone()
    };
    assert_eq!(
        session.capture_original_parsed(&reparsed, context(&aliases), &shallow),
        source_context::capture_original(&sources, &modules, context(&aliases), &shallow)
    );
}

#[test]
fn rich_ct_and_unused_branch_inputs_take_the_real_discovery_fallback() {
    let aliases = BTreeMap::new();
    let limits = Limits::default();
    for text in [
        "struct View<life a>{p:&'a i64} fn main(){}",
        "fn generic<T>(x:T)->T=>x; fn main(){}",
        "fn unused()->i32{if true{return 1;}else{return #(2);}} fn main(){}",
        "fn unused()->i32=>task_create(worker); fn worker()->i32=>1; fn main(){}",
    ] {
        let input = [parsed(0, text)];
        let session = ExpandSession::default();
        assert!(!session.parsed_scalar(&input[0], &limits), "{text}");
        let sources = input.iter().map(ParsedSource::source).collect::<Vec<_>>();
        let modules = input.iter().map(|p| &p.parsed().module).collect::<Vec<_>>();
        assert_eq!(
            session.capture_original_parsed(&input, context(&aliases), &limits),
            source_context::capture_original(&sources, &modules, context(&aliases), &limits),
            "fallback must execute discovery/inventory, not report Unsupported as success: {text}"
        );
    }
}

#[test]
fn capture_admission_is_single_use_and_binds_current_context_and_complete_owners() {
    let inputs = [parsed(0, "fn f()->i32=>1;")];
    let aliases = BTreeMap::new();
    let limits = Limits::default();
    for change_owner in [false, true] {
        let mut session = ExpandSession::default();
        assert!(session.parsed_scalar(&inputs[0], &limits));
        let (_, remaining) = session
            .capture_original_parsed(&inputs, context(&aliases), &limits)
            .unwrap();
        let reparsed = [parsed(0, &inputs[0].source().text)];
        let changed = BTreeMap::from([(vec!["other".into()], ModuleId(0))]);
        let current_inputs = if change_owner { &reparsed } else { &inputs };
        let current_aliases = if change_owner { &aliases } else { &changed };
        session
            .expand_parsed(
                current_inputs,
                &[vec![]],
                &remaining,
                &TargetProfile::default(),
                context(current_aliases),
            )
            .unwrap()
            .unwrap();
        assert_eq!(session.last_stats().fused_distributed_items, 0);
        assert!(!session.environment.as_ref().unwrap().fused_distribution);
        // A mismatching attempt consumes rather than retaining the old fact.
        session
            .expand_parsed(
                &inputs,
                &[vec![]],
                &limits,
                &TargetProfile::default(),
                context(&aliases),
            )
            .unwrap()
            .unwrap();
        assert_eq!(session.last_stats().fused_distributed_items, 0);
    }
}

#[test]
fn fused_scalar_new_work_and_node_boundaries_match_fresh_warm_failure_and_retry() {
    let (inputs, imports, aliases) = fixture("");
    let defaults = Limits::default();
    let mut seed = ExpandSession::default();
    let expected = run(&mut seed, &inputs, &imports, &aliases, &defaults).unwrap();
    let epoch = seed.epoch.clone();
    for nodes in [true, false] {
        let allowance = |amount| {
            if nodes {
                Limits {
                    max_nodes: amount,
                    ..defaults.clone()
                }
            } else {
                Limits {
                    max_analysis_steps: amount,
                    ..defaults.clone()
                }
            }
        };
        let (mut low, mut high) = (
            0,
            if nodes {
                defaults.max_nodes
            } else {
                defaults.max_analysis_steps
            },
        );
        while low < high {
            let mid = low + (high - low) / 2;
            if run(
                &mut ExpandSession::default(),
                &inputs,
                &imports,
                &aliases,
                &allowance(mid),
            )
            .is_ok()
            {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert!(low > 0);
        assert!(
            run(
                &mut ExpandSession::default(),
                &inputs,
                &imports,
                &aliases,
                &allowance(low - 1),
            )
            .is_err(),
            "the measured fused N-1 must reject"
        );
        assert_eq!(
            run(
                &mut ExpandSession::default(),
                &inputs,
                &imports,
                &aliases,
                &allowance(low)
            )
            .unwrap(),
            expected
        );
        for amount in [0, 1, low / 2, low - 1, low, low + 1] {
            let limits = allowance(amount);
            let fresh = run(
                &mut ExpandSession::default(),
                &inputs,
                &imports,
                &aliases,
                &limits,
            );
            let mut warm = seed.checkpoint();
            let actual = run(&mut warm, &inputs, &imports, &aliases, &limits);
            match (actual, fresh) {
                (Ok(actual), Ok(fresh)) => assert_eq!(actual, fresh),
                (Err(actual), Err(fresh)) => {
                    assert_eq!(actual, fresh, "nodes={nodes}, allowance={amount}");
                    assert!(!actual.is_empty());
                    assert!(
                        actual
                            .iter()
                            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
                    );
                    assert_eq!(warm.epoch, epoch, "failed tentative work cannot publish");
                }
                _ => panic!("fused fresh/warm disagreement: nodes={nodes}, allowance={amount}"),
            }
            assert_eq!(
                run(&mut warm, &inputs, &imports, &aliases, &defaults).unwrap(),
                expected
            );
        }
    }
}

#[test]
fn fused_scalar_placement_and_current_import_changes_do_not_reuse_stale_maps() {
    let (inputs, imports, aliases) = fixture("");
    let limits = Limits::default();
    let mut session = ExpandSession::default();
    run(&mut session, &inputs, &imports, &aliases, &limits).unwrap();
    let (moved, moved_imports, aliases) = fixture("// actual current source positions\n\n");
    let current = run(&mut session, &moved, &moved_imports, &aliases, &limits).unwrap();
    assert_eq!(current, raw(&moved, &moved_imports, &limits).unwrap());
    assert!(session.last_stats().rebound_frontend_modules > 0);
    assert_eq!(session.last_stats().rebound_source_map_validations, 2);
    for (_, map) in &current {
        map.validate_iter(moved.iter().map(ParsedSource::source), &limits)
            .unwrap();
    }
    // Expand preserves unresolved ordinary value calls and raw import edges;
    // Types checks absent import owners (E-MODULE-NOT-FOUND). Changing the
    // target must still invalidate prior products and agree with fresh/raw
    // expansion, never authorize the old target from Capture's no-CT facts.
    let mut changed_imports = moved_imports.clone();
    changed_imports[0][0].module = ModuleId(99);
    let actual = run(&mut session, &moved, &changed_imports, &aliases, &limits)
        .expect("Expand preserves the unresolved import for Types");
    assert_eq!(
        session.environment.as_ref().unwrap().imports,
        changed_imports
    );
    assert_eq!(session.last_stats().lower_modules, moved.len() as u64);
    assert_eq!(session.last_stats().reused_lower_modules, 0);
    assert_eq!(actual[0].0.imports, changed_imports[0]);
    assert_ne!(actual, current, "the old import target must not escape");
    assert!(!Arc::ptr_eq(&actual[0].0.core, &current[0].0.core));
    let fresh = run(
        &mut ExpandSession::default(),
        &moved,
        &changed_imports,
        &aliases,
        &limits,
    )
    .expect("fresh Expand also preserves the unresolved import");
    assert_eq!(actual, fresh);
    assert_eq!(actual, raw(&moved, &changed_imports, &limits).unwrap());
    for (_, map) in &actual {
        map.validate_iter(moved.iter().map(ParsedSource::source), &limits)
            .unwrap();
    }
    assert_eq!(
        run(&mut session, &moved, &changed_imports, &aliases, &limits).unwrap(),
        actual
    );
    assert_eq!(
        run(&mut session, &moved, &moved_imports, &aliases, &limits).unwrap(),
        current
    );
}
