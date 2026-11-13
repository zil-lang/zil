use super::*;
use zil_syntax::attributes::AttributeContext;

fn parsed(id: u32, text: &str) -> ParsedSource {
    let source = Arc::new(SourceUnit::new(
        FileId(id),
        ModuleId(id),
        format!("module{id}.zil"),
        text,
    ));
    let parsed = ParsedSource::parse(source, &Limits::default(), &AttributeContext::default());
    assert!(
        parsed.parsed().diagnostics.is_empty(),
        "{:?}",
        parsed.parsed().diagnostics
    );
    parsed
}
fn expand(
    session: &mut ExpandSession,
    input: &[ParsedSource],
    limits: &Limits,
) -> CompileResult<Option<ExpandedProgram>> {
    let aliases = BTreeMap::new();
    session.expand_parsed(
        input,
        &vec![vec![]; input.len()],
        limits,
        &TargetProfile::default(),
        source_context::SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases: &aliases,
            declarations: source_context::DeclarationPlacement::Original,
        },
    )
}
fn fresh(
    input: &[ParsedSource],
    limits: &Limits,
) -> CompileResult<Vec<(ProgramModule, Arc<SourceMap>)>> {
    let sources: Vec<_> = input.iter().map(|p| p.source().clone()).collect();
    let syntax: Vec<_> = input.iter().map(|p| &p.parsed().module).collect();
    expand_program_with_limits_target(
        &sources,
        &syntax,
        &vec![vec![]; input.len()],
        limits,
        &TargetProfile::default(),
    )
    .map(|modules| {
        modules
            .into_iter()
            .map(|(module, map)| (module, Arc::new(map)))
            .collect()
    })
}
fn checked(
    session: &mut ExpandSession,
    input: &[ParsedSource],
    limits: &Limits,
) -> ExpandedProgram {
    let expanded = expand(session, input, limits)
        .unwrap()
        .expect("sealed scalar path");
    assert_eq!(expanded.modules, fresh(input, limits).unwrap());
    expanded
}
#[test]
fn producer_shares_complete_core_and_maps_across_unchanged_modules() {
    let limits = Limits::default();
    let mut input = [
        parsed(0, "fn first(x:i32)->i32=>x+1;"),
        parsed(1, "fn second(x:i32)->i32=>x+2;"),
    ];
    let mut session = ExpandSession::default();
    assert!(session.parsed_scalar(&input[0], &limits));
    let cold = checked(&mut session, &input, &limits);
    assert!(session.parsed_scalar(&input[0], &limits));
    assert!(!session.parsed_scalar(
        &input[0],
        &Limits {
            max_nodes: 0,
            ..limits.clone()
        }
    ));
    assert!(!session.parsed_scalar(&parsed(0, "struct Rich{x:i32}"), &limits));
    let warm = checked(&mut session, &input, &limits);
    for ((before, before_map), (after, after_map)) in cold.modules.iter().zip(&warm.modules) {
        assert!(Arc::ptr_eq(&before.core, &after.core));
        assert!(Arc::ptr_eq(before_map, after_map));
    }
    assert_eq!(session.last_stats().retained_lower_cores, 2);
    assert_eq!(session.last_stats().retained_lower_source_maps, 2);
    assert_eq!(session.last_stats().cloned_source_map_entries, 0);
    input[0] = parsed(0, "fn first(x:i32)->i32=>x+3;");
    let current = checked(&mut session, &input, &limits);
    assert!(!Arc::ptr_eq(
        &warm.modules[0].0.core,
        &current.modules[0].0.core
    ));
    assert!(!Arc::ptr_eq(&warm.modules[0].1, &current.modules[0].1));
    assert!(Arc::ptr_eq(
        &warm.modules[1].0.core,
        &current.modules[1].0.core
    ));
    assert!(Arc::ptr_eq(&warm.modules[1].1, &current.modules[1].1));
    assert_eq!(session.last_stats().retained_lower_cores, 1);
    assert_eq!(session.last_stats().retained_lower_source_maps, 1);
    assert_eq!(session.last_stats().cloned_source_map_entries, 0);

    // The public raw entry keeps its owned SourceMap result and identical data.
    let sources = input.iter().map(|p| p.source().clone()).collect::<Vec<_>>();
    let syntax = input.iter().map(|p| &p.parsed().module).collect::<Vec<_>>();
    let raw: Vec<(ProgramModule, SourceMap)> = expand_program_with_limits_target(
        &sources,
        &syntax,
        &[vec![], vec![]],
        &limits,
        &TargetProfile::default(),
    )
    .unwrap();
    for ((shared, map), (owned, owned_map)) in current.modules.iter().zip(&raw) {
        assert_eq!(shared, owned);
        assert_eq!(map.as_ref(), owned_map);
    }
    let mut edited = current.modules[1].0.clone();
    Arc::make_mut(&mut edited.core).functions[0].name = "raw_edit".into();
    assert_eq!(current.modules[1].0.core.functions[0].name, "second");
}

#[test]
fn producer_wide_private_edit_skips_unchanged_modules_and_preserves_local_spans() {
    let limits = Limits::default();
    let input = [
        parsed(
            0,
            "fn local()->i32=>9; pub fn helper(x:i32)->i32=>x+local();",
        ),
        parsed(1, "pub fn untouched(x:i32)->i32=>x+2;"),
    ];
    let mut session = ExpandSession::default();
    let cold = checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().normalize_modules, 2);
    assert_eq!(session.last_stats().lower_modules, 2);
    let warm = checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().normalize_modules, 0);
    assert_eq!(session.last_stats().restore_modules, 0);
    assert_eq!(session.last_stats().plain_prepare_modules, 0);
    assert_eq!(session.last_stats().plain_lifetimes_modules, 0);
    assert_eq!(session.last_stats().lower_modules, 0);
    assert_eq!(session.last_stats().reused_lower_modules, 2);
    assert!(session.last_stats().receipt_nodes > 0);
    assert_eq!(cold.revisions, warm.revisions);
    let edit = [
        parsed(
            0,
            "fn local()->i32=>10; pub fn helper(x:i32)->i32=>x+local();",
        ),
        input[1].clone(),
    ];
    let current = checked(&mut session, &edit, &limits);
    assert_eq!(session.last_stats().normalize_modules, 1);
    assert_eq!(session.last_stats().restore_modules, 1);
    assert_eq!(session.last_stats().lower_modules, 1);
    assert_eq!(session.last_stats().reused_lower_modules, 1);
    assert_ne!(
        warm.revisions[&ModuleId(0)],
        current.revisions[&ModuleId(0)]
    );
    assert_eq!(
        warm.revisions[&ModuleId(1)],
        current.revisions[&ModuleId(1)]
    );
    assert!(session.last_stats().syntax_clone_receipt_nodes > 0);
    assert_eq!(session.last_stats().cloned_source_map_entries, 0);
    assert_eq!(session.last_stats().total_ns, 0, "profiling disabled");
}
#[test]
fn producer_failure_and_fallback_preserve_last_successful_epoch() {
    let limits = Limits::default();
    let input = [
        parsed(0, "fn helper()->i32=>1;"),
        parsed(1, "fn untouched()->i32=>2;"),
    ];
    let mut session = ExpandSession::default();
    let before = checked(&mut session, &input, &limits);
    let invalid = [
        parsed(0, "fn helper()->i32=>1;fn helper()->i32=>2;"),
        input[1].clone(),
    ];
    let expected = fresh(&invalid, &limits).unwrap_err();
    let actual = expand(&mut session, &invalid, &limits)
        .err()
        .expect("duplicate declaration");
    assert_eq!(actual, expected);
    assert_eq!(
        checked(&mut session, &input, &limits).revisions,
        before.revisions
    );
    for text in [
        "struct Point{x:i32}fn main(){}",
        "resource Item{x:i32}fn main(){}",
        "fn identity<T>(x:T)->T=>x;fn main(){}",
        "trait T{fn f(self:Self);}fn main(){}",
    ] {
        let rich = [parsed(0, text)];
        assert!(
            expand(&mut session, &rich, &limits).unwrap().is_none(),
            "{text}"
        );
        assert_eq!(
            checked(&mut session, &input, &limits).revisions,
            before.revisions
        );
    }
}
#[test]
fn producer_receipts_preserve_ordered_first_budget_diagnostic() {
    let small = [
        parsed(0, "fn first()->i32=>1;"),
        parsed(1, "fn second()->i32=>2;"),
    ];
    let long = format!("fn first()->i32=>1{};", "+1".repeat(15));
    let changed = [parsed(0, &long), small[1].clone()];
    let mut compared = 0;
    let mut reused_before_failure = 0;
    for work in (100..4000).step_by(13) {
        let limits = Limits {
            max_analysis_steps: work,
            ..Limits::default()
        };
        let mut session = ExpandSession::default();
        if !matches!(expand(&mut session, &small, &limits), Ok(Some(_))) {
            continue;
        }
        let expected = fresh(&changed, &limits);
        let actual = expand(&mut session, &changed, &limits);
        match (actual, expected) {
            (Ok(Some(actual)), Ok(expected)) => assert_eq!(actual.modules, expected),
            (Err(actual), Err(expected)) => {
                assert_eq!(actual, expected, "remaining work={work}");
                if session.last_stats().receipt_hits > 0 {
                    reused_before_failure += 1;
                }
            }
            _ => panic!("producer/fresh disagreement at remaining work={work}"),
        }
        compared += 1;
    }
    assert!(compared > 100);
    assert!(
        reused_before_failure > 0,
        "exercise receipt reuse before a later original budget failure"
    );
}
#[test]
fn producer_accepts_existing_scalar_extern_and_unsafe_io() {
    let input = [parsed(
        0,
        "extern \"C\" fn getchar()->i32;extern \"C\" fn putchar(value:i32)->i32;fn main(){unsafe{let value=getchar();putchar(value);}}",
    )];
    let mut session = ExpandSession::default();
    checked(&mut session, &input, &Limits::default());
    checked(&mut session, &input, &Limits::default());
    assert_eq!(session.last_stats().lower_modules, 0);
    assert_eq!(session.last_stats().reused_lower_modules, 1);
}
#[test]
fn producer_profiles_fallback_and_recomputation_without_overlapping_categories() {
    let mut session = ExpandSession::default();
    session.set_cost_profiling(true);
    let rich = [parsed(0, "struct Point{x:i32}fn main(){}")];
    assert!(
        expand(&mut session, &rich, &Limits::default())
            .unwrap()
            .is_none()
    );
    assert!(session.last_stats().context_ns > 0);
    assert!(session.last_stats().total_ns >= session.last_stats().context_ns);
    let input = [parsed(0, "fn f()->i32=>1;")];
    checked(&mut session, &input, &Limits::default());
    let stats = session.last_stats();
    assert!(stats.production_ns > 0 && stats.assembly_ns > 0 && stats.reuse_ns > 0);
    assert!(
        stats.context_ns + stats.production_ns + stats.assembly_ns + stats.reuse_ns
            <= stats.total_ns
    );
}

#[test]
fn producer_duplicate_file_identity_falls_back_before_any_receipt() {
    let first = parsed(0, "fn a(){}");
    let second_source = Arc::new(SourceUnit::new(
        FileId(0),
        ModuleId(1),
        "other.zil",
        format!("fn b()->i32=>1{};", "+1".repeat(15)),
    ));
    let second = ParsedSource::parse(
        second_source,
        &Limits::default(),
        &AttributeContext::default(),
    );
    assert!(second.parsed().diagnostics.is_empty());
    let input = [first, second];
    let limits = Limits {
        max_analysis_steps: 32,
        ..Limits::default()
    };
    let raw = fresh(&input, &limits).unwrap_err();
    assert_eq!(raw[0].category, zil_base::ResultCategory::ResourceLimit);
    let mut session = ExpandSession::default();
    match expand(&mut session, &input, &limits) {
        Ok(None) => {}
        Err(actual) => panic!(
            "file-keyed producer changed raw first diagnostic: producer={actual:?}, raw={raw:?}"
        ),
        Ok(Some(_)) => panic!("duplicate file identities cannot enter the sealed reuse domain"),
    }
    assert_eq!(session.last_stats().receipt_hits, 0);
    assert_eq!(session.last_stats().preflight_modules, 0);
}

#[test]
fn producer_unavailable_scalar_lineage_keeps_raw_outputs_and_exact_budget_boundary() {
    let input = [parsed(
        0,
        "fn emit(value:i32)->i32=>value+1;fn main()->i32=>emit(1);",
    )];
    let aliases = BTreeMap::new();
    let context = source_context::SourceIdentityContext::ClosedSnapshot {
        root: ModuleId(0),
        aliases: &aliases,
        declarations: source_context::DeclarationPlacement::Unavailable,
    };
    let sources = vec![input[0].source().clone()];
    let syntax = vec![&input[0].parsed().module];
    let imports = vec![vec![]];
    let target = TargetProfile::default();
    let raw = |limits: &Limits| {
        expand_program_with_limits_target_context(
            &sources, &syntax, &imports, limits, &target, context,
        )
    };
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        if raw(&limits).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(low > 0);
    let limits = Limits {
        max_analysis_steps: low,
        ..Limits::default()
    };
    let expected = raw(&limits)
        .unwrap()
        .into_iter()
        .map(|(module, map)| (module, Arc::new(map)))
        .collect::<Vec<_>>();
    let mut session = ExpandSession::default();
    let produce = |session: &mut ExpandSession, limits: &Limits| {
        session.expand_parsed(&input, &imports, limits, &target, context)
    };
    let cold = produce(&mut session, &limits)
        .unwrap()
        .expect("scalar unavailable domain");
    assert_eq!(cold.modules, expected);
    let warm = produce(&mut session, &limits).unwrap().unwrap();
    assert_eq!(warm.modules, expected);
    assert_eq!(warm.revisions, cold.revisions);
    assert_eq!(session.last_stats().lower_modules, 0);
    let below = Limits {
        max_analysis_steps: low - 1,
        ..limits.clone()
    };
    let actual = produce(&mut session, &below).err().expect("N-1 rejects");
    assert_eq!(actual, raw(&below).unwrap_err());
    assert_eq!(actual[0].category, zil_base::ResultCategory::ResourceLimit);
    let original = expand(&mut session, &input, &limits).unwrap().unwrap();
    assert_ne!(
        original.revisions, warm.revisions,
        "context marker is a validity input"
    );
    let rows = [];
    let aligned = source_context::SourceIdentityContext::ClosedSnapshot {
        root: ModuleId(0),
        aliases: &aliases,
        declarations: source_context::DeclarationPlacement::Aligned(&rows),
    };
    assert!(
        session
            .expand_parsed(&input, &imports, &limits, &target, aligned)
            .unwrap()
            .is_none()
    );
}

#[test]
fn producer_current_work_and_node_allowances_replay_successful_stage_receipts() {
    let input = [
        parsed(0, "fn local()->i32=>9;pub fn first(x:i32)->i32=>x+local();"),
        parsed(1, "pub fn second(x:i32)->i32=>x+2;"),
        parsed(2, "pub fn third(x:i32)->i32=>x+3;"),
    ];
    let mut session = ExpandSession::default();
    session.set_cost_profiling(true);
    let limits = Limits::default();
    let cold = checked(&mut session, &input, &limits);
    let remaining = Limits {
        max_analysis_steps: limits.max_analysis_steps - 3,
        max_nodes: limits.max_nodes - 5,
        ..limits.clone()
    };
    let warm = checked(&mut session, &input, &remaining);
    assert_eq!(warm.revisions, cold.revisions);
    assert_eq!(session.last_stats().normalize_modules, 0);
    assert_eq!(session.last_stats().lower_modules, 0);
    assert_eq!(session.last_stats().reused_lower_modules, 3);
    assert_eq!(session.last_stats().allowance_work_decrease, 3);
    assert_eq!(session.last_stats().allowance_nodes_decrease, 5);
    assert_eq!(session.last_stats().environment_headers_changed, 0);
    let changed = [
        parsed(
            0,
            "fn local()->i32=>10;pub fn first(x:i32)->i32{let y=x+local();let z=y+1;return z;}",
        ),
        input[1].clone(),
        input[2].clone(),
    ];
    let edit = checked(&mut session, &changed, &remaining);
    assert_eq!(session.last_stats().classified_modules, 1);
    assert_eq!(session.last_stats().normalize_modules, 1);
    assert_eq!(session.last_stats().lower_modules, 1);
    assert_eq!(session.last_stats().reused_lower_modules, 2);
    assert_eq!(edit.revisions[&ModuleId(1)], warm.revisions[&ModuleId(1)]);
    assert_eq!(edit.revisions[&ModuleId(2)], warm.revisions[&ModuleId(2)]);
    let byte_change = Limits {
        max_source_bytes: remaining.max_source_bytes - 1,
        ..remaining
    };
    let invalidated = checked(&mut session, &changed, &byte_change);
    assert_eq!(session.last_stats().env_limit_max_source_bytes_changed, 1);
    assert_eq!(session.last_stats().lower_modules, 3);
    assert_ne!(
        invalidated.revisions[&ModuleId(1)],
        edit.revisions[&ModuleId(1)]
    );
}

#[test]
fn producer_changed_allowances_keep_full_fresh_first_diagnostics() {
    let input = [
        parsed(0, "fn first(x:i32)->i32{let a=x+1;return a+a;}"),
        parsed(1, "fn second(x:i32)->i32=>x+2;"),
    ];
    let mut completed = 0;
    let mut insufficient_receipts = 0;
    for nodes in [false, true] {
        let limit = |amount| {
            if nodes {
                Limits {
                    max_nodes: amount,
                    ..Limits::default()
                }
            } else {
                Limits {
                    max_analysis_steps: amount,
                    ..Limits::default()
                }
            }
        };
        let mut low = 0;
        let mut high = if nodes {
            Limits::default().max_nodes
        } else {
            Limits::default().max_analysis_steps
        };
        while low < high {
            let mid = low + (high - low) / 2;
            if fresh(&input, &limit(mid)).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert!(low > 0);
        for amount in [0, 1, low / 2, low - 1, low, low + 1] {
            let mut session = ExpandSession::default();
            checked(&mut session, &input, &Limits::default());
            let current = limit(amount);
            match (
                expand(&mut session, &input, &current),
                fresh(&input, &current),
            ) {
                (Ok(Some(actual)), Ok(expected)) => assert_eq!(actual.modules, expected),
                (Err(actual), Err(expected)) => {
                    assert_eq!(actual, expected, "nodes={nodes}, amount={amount}")
                }
                _ => panic!("current allowance mismatch nodes={nodes}, amount={amount}"),
            }
            insufficient_receipts += session.last_stats().receipt_budget_misses;
            completed += 1;
        }
    }
    assert_eq!(completed, 12);
    assert!(insufficient_receipts > 0);
}

#[test]
fn producer_lower_demands_fail_closed_before_reusing_a_local_product() {
    let input = [parsed(0, "fn f(x:i32)->i32=>x+1;")];
    let mut session = ExpandSession::default();
    checked(&mut session, &input, &Limits::default());
    let entry = session.entries.get(&ModuleId(0)).unwrap();
    let (_, lower) = entry.lower.as_ref().unwrap();
    assert!(lower.lowering_demand.work > 0 && lower.lowering_demand.nodes > 0);
    let mut entries = session.entries.clone();
    let mut stats = ProducerStats::default();
    let mut scalar_inventory = None;
    let mut reuse = Reuse {
        normalization_environment: Arc::new(session.environment.clone().unwrap()),
        scalar_inventory: &mut scalar_inventory,
        prior_entries: &session.entries,
        item_reuse: false,
        entries: &mut entries,
        files: [(FileId(0), ModuleId(0))].into(),
        stats: &mut stats,
        executed: BTreeSet::new(),
        profiling: false,
    };
    let insufficient_work = Limits {
        max_analysis_steps: lower.lowering_demand.work - 1,
        ..Limits::default()
    };
    assert!(reuse.lower(FileId(0), &insufficient_work).is_none());
    let insufficient_nodes = Limits {
        max_nodes: lower.lowering_demand.nodes - 1,
        ..Limits::default()
    };
    assert!(reuse.lower(FileId(0), &insufficient_nodes).is_none());
    assert_eq!(stats.lower_budget_misses, 2);
    assert_eq!(stats.reused_lower_modules, 0);
}

#[test]
fn producer_failed_lower_source_map_validation_counts_actual_execution() {
    let input: Vec<_> = (0..8).map(|id| parsed(id, "")).collect();
    let mut session = ExpandSession::default();
    checked(&mut session, &input, &Limits::default());
    let limits = Limits {
        max_analysis_steps: input.len(),
        ..Limits::default()
    };
    // Capture/normalization consumes exactly one module node each. The first
    // cached empty map checks all source files and the project debits that work;
    // the next module re-enters real Lower at zero remaining work and fails its
    // original map validation. Cache publication cannot count this execution.
    let actual = expand(&mut session, &input, &limits)
        .err()
        .expect("current map validation rejects");
    assert_eq!(actual, fresh(&input, &limits).unwrap_err());
    assert_eq!(
        actual[0].message,
        "source-map validation work budget exceeded"
    );
    assert_eq!(session.last_stats().reused_lower_modules, 1);
    assert_eq!(session.last_stats().lower_budget_misses, 1);
    assert_eq!(session.last_stats().lower_modules, 1);
    let repaired = checked(&mut session, &input, &Limits::default());
    assert_eq!(repaired.modules.len(), input.len());
    assert_eq!(session.last_stats().lower_modules, 0);
}

#[test]
fn producer_consumes_owned_normalized_items_and_retains_real_restored_products() {
    let input = [
        parsed(0, "fn local()->i32=>9;pub fn first(x:i32)->i32=>x+local();"),
        parsed(1, "pub fn second(x:i32)->i32=>x+2;"),
        parsed(2, "pub fn third(x:i32)->i32=>x+3;"),
    ];
    let mut session = ExpandSession::default();
    let limits = Limits::default();
    let cold = checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().normalized_item_deep_clones, 4);
    assert_eq!(session.last_stats().restored_module_deep_clones, 0);
    assert_eq!(session.last_stats().restored_item_deep_clones, 0);
    assert_eq!(session.last_stats().owned_restore_items, 4);
    assert_eq!(session.last_stats().retained_normalized_items, 0);
    checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().normalized_item_deep_clones, 0);
    assert_eq!(session.last_stats().restored_module_deep_clones, 0);
    assert_eq!(session.last_stats().retained_normalized_items, 4);
    assert_eq!(session.last_stats().retained_restored_modules, 3);
    assert_eq!(session.last_stats().syntax_clone_receipt_nodes, 0);
    assert_eq!(session.last_stats().cloned_source_map_entries, 0);
    let changed = [
        parsed(
            0,
            "fn local()->i32=>10;pub fn first(x:i32)->i32=>x+local();",
        ),
        input[1].clone(),
        input[2].clone(),
    ];
    let current = checked(&mut session, &changed, &limits);
    assert_eq!(session.last_stats().normalized_item_deep_clones, 2);
    assert_eq!(session.last_stats().restored_module_deep_clones, 0);
    assert_eq!(session.last_stats().restored_item_deep_clones, 0);
    assert_eq!(session.last_stats().owned_restore_items, 2);
    assert_eq!(session.last_stats().retained_normalized_items, 2);
    assert_eq!(session.last_stats().retained_restored_modules, 2);
    assert_eq!(
        current.revisions[&ModuleId(1)],
        cold.revisions[&ModuleId(1)]
    );
    assert_eq!(
        current.revisions[&ModuleId(2)],
        cold.revisions[&ModuleId(2)]
    );
}

#[test]
fn producer_retained_body_receipt_shortfall_reenters_complete_restore_input() {
    let current = [
        parsed(0, "fn first()->i32=>10;"),
        parsed(1, "fn second()->i32=>2;"),
    ];
    let cheap = [parsed(0, "fn first()->i32=>9;"), current[1].clone()];
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        if fresh(&current, &limits).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(low > 0);
    let limits = Limits {
        max_analysis_steps: low - 1,
        ..Limits::default()
    };
    let expected = fresh(&current, &limits).unwrap_err();
    let mut session = ExpandSession::default();
    checked(&mut session, &cheap, &limits);
    let actual = expand(&mut session, &current, &limits)
        .err()
        .expect("current original Restore fails");
    assert_eq!(actual, expected);
    assert!(session.last_stats().retained_normalized_items > 0);
    assert_eq!(session.last_stats().restored_item_deep_clones, 0);
    assert!(
        session.last_stats().normalized_materializations > 0,
        "insufficient retained second item reconstructs its genuine normalized body"
    );
    assert!(session.last_stats().receipt_budget_misses > 0);
    assert_eq!(session.last_stats().restored_module_deep_clones, 0);
}

#[test]
fn producer_tracks_unchanged_functions_inside_a_changed_module_without_source_copies() {
    let limits = Limits::default();
    let cold_text = "fn first(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} fn second(x:i32)->i32{if x<10{return x+3;}else{return x*4;}}";
    let mut session = ExpandSession::default();
    let cold = checked(&mut session, &[parsed(0, cold_text)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    let current = checked(
        &mut session,
        &[parsed(0, &cold_text.replace("x+1", "x+2"))],
        &limits,
    )
    .into_program(ModuleId(0), Fingerprint::ZERO)
    .0;
    assert_ne!(
        cold.function_revision(ModuleId(0), "first"),
        current.function_revision(ModuleId(0), "first")
    );
    assert_eq!(
        cold.function_revision(ModuleId(0), "second"),
        current.function_revision(ModuleId(0), "second")
    );
    assert_eq!(
        cold.function_source_fingerprint(ModuleId(0), "second"),
        current.function_source_fingerprint(ModuleId(0), "second")
    );
    assert_eq!(session.last_stats().cloned_source_bytes, 0);

    // Identical text at a different source position is a distinct relocation
    // binding, even when semantic BodyKey projection can later compare equal.
    let moved = checked(
        &mut session,
        &[parsed(0, &format!("\n{cold_text}"))],
        &limits,
    )
    .into_program(ModuleId(0), Fingerprint::ZERO)
    .0;
    assert_ne!(
        cold.function_revision(ModuleId(0), "second"),
        moved.function_revision(ModuleId(0), "second")
    );
    assert_eq!(session.last_stats().cloned_source_bytes, 0);
}

#[test]
fn function_revision_does_not_survive_a_changed_declaration_environment() {
    let limits = Limits::default();
    let mut session = ExpandSession::default();
    let original = "    fn first(x:i32)->i32=>x+1; fn second(x:i32)->i32=>x+2;";
    let cold = checked(&mut session, &[parsed(0, original)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    // Publicness participates in the complete source environment even though
    // the untouched second body and its source span remain byte-identical.
    let current = checked(
        &mut session,
        &[parsed(0, &original.replace("    fn first", "pub fn first"))],
        &limits,
    )
    .into_program(ModuleId(0), Fingerprint::ZERO)
    .0;
    assert_ne!(
        cold.function_revision(ModuleId(0), "second"),
        current.function_revision(ModuleId(0), "second")
    );
}

#[test]
fn relocated_input_keeps_only_its_sealed_relative_version_and_current_binding() {
    let limits = Limits::default();
    let original = "fn first(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} fn second(x:i32)->i32{if x<10{return x+3;}else{return x*4;}}";
    let mut session = ExpandSession::default();
    let cold = checked(&mut session, &[parsed(0, original)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    let wider = original.replace("x+1", "x+117");
    let current = checked(&mut session, &[parsed(0, &wider)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    assert_ne!(
        cold.function_input_revision(ModuleId(0), "first"),
        current.function_input_revision(ModuleId(0), "first")
    );
    assert_eq!(
        cold.function_input_revision(ModuleId(0), "second"),
        current.function_input_revision(ModuleId(0), "second")
    );
    // The existing public revision and fingerprint remain exact current
    // position bindings. Only the new opaque input version survives movement.
    assert_ne!(
        cold.function_revision(ModuleId(0), "second"),
        current.function_revision(ModuleId(0), "second")
    );
    assert_ne!(
        cold.function_source_fingerprint(ModuleId(0), "second"),
        current.function_source_fingerprint(ModuleId(0), "second")
    );
    assert_eq!(session.last_stats().function_input_hashes, 1);
    assert_eq!(session.last_stats().function_input_reuses, 1);
    assert_eq!(session.last_stats().function_input_text_comparisons, 2);
    assert_eq!(session.last_stats().function_input_binding_hashes, 2);
    assert_eq!(session.last_stats().cloned_source_bytes, 0);
    let fresh = checked(&mut ExpandSession::default(), &[parsed(0, &wider)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    for name in ["first", "second"] {
        assert_eq!(
            current.function_source_fingerprint(ModuleId(0), name),
            fresh.function_source_fingerprint(ModuleId(0), name)
        );
    }
    let shifted = checked(&mut session, &[parsed(0, &format!("\n\n{wider}"))], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    for name in ["first", "second"] {
        assert_eq!(
            current.function_input_revision(ModuleId(0), name),
            shifted.function_input_revision(ModuleId(0), name)
        );
        assert_ne!(
            current.function_revision(ModuleId(0), name),
            shifted.function_revision(ModuleId(0), name)
        );
    }
    assert_eq!(session.last_stats().function_input_hashes, 0);
    assert_eq!(session.last_stats().function_input_reuses, 2);
    // Whitespace inside the declaration changes relative diagnostic positions.
    let interior = checked(
        &mut session,
        &[parsed(0, &format!("\n\n{}", wider.replace("x+3", "x + 3")))],
        &limits,
    )
    .into_program(ModuleId(0), Fingerprint::ZERO)
    .0;
    assert_ne!(
        shifted.function_input_revision(ModuleId(0), "second"),
        interior.function_input_revision(ModuleId(0), "second")
    );
    assert_eq!(session.last_stats().function_input_hashes, 1);
}

#[test]
fn relative_input_versions_do_not_survive_declaration_insertion_deletion_or_signature_changes() {
    let limits = Limits::default();
    let original = "fn first(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} fn second(x:i32)->i32{if x<10{return first(x);}else{return x*4;}}";
    for current_text in [
        format!("fn inserted(x:i32)->i32=>x; {original}"),
        original.replace(
            "fn first(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} ",
            "",
        ),
        original
            .replace("first(x:i32)", "first(y:i32)")
            .replace("return x+1", "return y+1")
            .replace("return x*2", "return y*2")
            .replace("if x<10{return y", "if y<10{return y"),
        original.replacen("fn first", "pub fn first", 1),
    ] {
        let mut session = ExpandSession::default();
        let cold = checked(&mut session, &[parsed(0, original)], &limits)
            .into_program(ModuleId(0), Fingerprint::ZERO)
            .0;
        let current = checked(&mut session, &[parsed(0, &current_text)], &limits)
            .into_program(ModuleId(0), Fingerprint::ZERO)
            .0;
        assert_ne!(
            cold.function_input_revision(ModuleId(0), "second"),
            current.function_input_revision(ModuleId(0), "second"),
            "{current_text}"
        );
        assert_eq!(session.last_stats().function_input_reuses, 0);
    }
}

#[test]
fn rewritten_and_captured_inputs_cannot_mint_relative_scalar_versions() {
    // These all parse successfully, but require ordinary complete expansion.
    // A prior successful scalar epoch must not admit their unchanged function
    // text without the constant/CT/instance/capture dependencies being checked.
    let original = "fn first(x:i32)->i32{if x<10{return x+1;}else{return x*2;}}";
    for text in [
        format!("const N:i32=7; {original}"),
        format!("#if(true){{fn chosen()->i32=>7;}} {original}"),
        format!("fn identity<T>(x:T)->T=>x; {original}"),
        format!("trait Read{{fn get(self:&Self)->i32;}} {original}"),
        "fn first(x:i32)->i32{let f=()=>x;return f();}".into(),
    ] {
        let mut session = ExpandSession::default();
        checked(&mut session, &[parsed(0, original)], &Limits::default());
        assert!(
            expand(&mut session, &[parsed(0, &text)], &Limits::default())
                .unwrap()
                .is_none(),
            "{text}"
        );
    }
}

fn checked_import_bindings(
    session: &mut ExpandSession,
    inputs: &[ParsedSource],
) -> ProducedProgram {
    let imports = vec![
        vec![CoreImport {
            alias: "dep".into(),
            module: ModuleId(1),
            span: inputs[0].parsed().module.items[0].span,
        }],
        vec![],
    ];
    let aliases = BTreeMap::from([(vec!["dep".into()], ModuleId(1))]);
    let limits = Limits::default();
    let target = TargetProfile::default();
    let current = session
        .expand_parsed(
            inputs,
            &imports,
            &limits,
            &target,
            source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &aliases,
                declarations: source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .unwrap();
    let sources = inputs.iter().map(ParsedSource::source).collect::<Vec<_>>();
    let syntax = inputs
        .iter()
        .map(|p| &p.parsed().module)
        .collect::<Vec<_>>();
    let fresh =
        expand_program_with_limits_target(&sources, &syntax, &imports, &limits, &target).unwrap();
    assert_eq!(current.modules.len(), fresh.len());
    for ((module, map), (expected, expected_map)) in current.modules.iter().zip(fresh) {
        assert_eq!(*module, expected);
        assert_eq!(map.as_ref(), &expected_map);
    }
    current.into_program(ModuleId(0), Fingerprint::ZERO).0
}

fn import_binding_inputs(prefix: &str) -> [ParsedSource; 2] {
    [
        parsed(
            0,
            &format!(
                "{prefix}import dep; fn caller(x:i32)->i32{{if x<10{{return dep.f(x);}}else{{return x*2;}}}}"
            ),
        ),
        parsed(
            1,
            &format!("{prefix}pub fn f(x:i32)->i32{{if x<10{{return x+3;}}else{{return x*4;}}}}"),
        ),
    ]
}

#[test]
fn moved_import_diagnostics_keep_only_exact_function_input_versions() {
    let mut session = ExpandSession::default();
    session.set_cost_profiling(true);
    let cold = checked_import_bindings(&mut session, &import_binding_inputs(""));
    let current = checked_import_bindings(
        &mut session,
        &import_binding_inputs("// current positions\n\n"),
    );
    for (module, name) in [(ModuleId(0), "caller"), (ModuleId(1), "f")] {
        assert_eq!(
            cold.function_input_revision(module, name),
            current.function_input_revision(module, name)
        );
        assert_ne!(
            cold.function_revision(module, name),
            current.function_revision(module, name)
        );
        assert_ne!(
            cold.function_source_fingerprint(module, name),
            current.function_source_fingerprint(module, name)
        );
    }
    let work = session.last_stats();
    assert_eq!(work.environment_imports_changed, 1);
    assert_eq!(
        (work.function_input_hashes, work.function_input_reuses),
        (0, 2)
    );
    assert_eq!(work.function_input_text_comparisons, 2);
    assert_eq!(work.function_input_binding_hashes, 2);
    // No old position-bearing producer output or stage receipt survives.
    assert_eq!(
        (
            work.normalize_modules,
            work.restore_modules,
            work.lower_modules
        ),
        (2, 2, 2)
    );
    assert_eq!(work.reused_lower_modules, 0);
    assert_eq!(work.retained_normalized_items, 0);
    assert_eq!(work.retained_restored_modules, 0);
}

#[test]
fn function_input_environment_rejects_every_nearby_import_identity_change() {
    let mut session = ExpandSession::default();
    checked_import_bindings(&mut session, &import_binding_inputs(""));
    let previous = session.environment.clone().unwrap();
    let mut relocated = previous.clone();
    relocated.imports[0][0].span.start += 20;
    relocated.imports[0][0].span.end += 20;
    assert!(!previous.compatible(&relocated));
    assert!(previous.function_inputs_compatible(&relocated));
    let mut changes = Vec::new();
    let mut current = relocated.clone();
    current.imports[0][0].alias = "other".into();
    changes.push(current);
    let mut current = relocated.clone();
    current.imports[0][0].module = ModuleId(2);
    changes.push(current);
    let mut current = relocated.clone();
    current.imports[0][0].span.file = FileId(9);
    changes.push(current);
    let mut current = relocated.clone();
    current.imports[0].clear();
    changes.push(current);
    let mut current = relocated.clone();
    current.imports.push(vec![]);
    changes.push(current);
    let mut current = relocated.clone();
    current.sources[0].1 = FileId(9);
    changes.push(current);
    let mut current = relocated.clone();
    current.sources[1].0 = ModuleId(2);
    changes.push(current);
    let mut current = relocated.clone();
    current.sources[1].2.push_str(".renamed");
    changes.push(current);
    let mut current = relocated.clone();
    current.headers[0][1].public = true;
    changes.push(current);
    let mut current = relocated.clone();
    current.context.push_str(":changed");
    changes.push(current);
    let mut current = relocated.clone();
    current.target.checked_arithmetic = !current.target.checked_arithmetic;
    changes.push(current);
    macro_rules! changed_limit {
        ($($field:ident),+ $(,)?) => { $(
            let mut current = relocated.clone();
            current.limits.$field += 1;
            changes.push(current);
        )+ };
    }
    changed_limit!(
        max_source_bytes,
        max_tokens,
        max_functions,
        max_blocks,
        max_instructions,
        max_locals,
        max_values,
        max_ct_steps,
        max_call_depth
    );
    for current in changes {
        assert!(!previous.compatible(&current));
        assert!(
            !previous.function_inputs_compatible(&current),
            "{current:?}"
        );
    }
}

#[test]
fn sparse_changes_bind_actual_epoch_and_share_unmodified_input_tables() {
    let limits = Limits::default();
    let mut session = ExpandSession::default();
    let mut input = [
        parsed(0, "fn first(x:i32)->i32=>x+1;fn sibling(x:i32)->i32=>x+2;"),
        parsed(1, "fn other(x:i32)->i32=>x+3;"),
    ];
    let cold = checked(&mut session, &input, &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    let first_owner = cold.function_owner(ModuleId(0), "first").unwrap().clone();
    let warm = checked(&mut session, &input, &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    assert!(warm.change_batch().changes().is_empty());
    assert_eq!(
        warm.change_batch().previous_epoch(),
        Some(cold.change_batch().epoch())
    );
    assert!(Arc::ptr_eq(
        &cold.functions[&ModuleId(0)],
        &warm.functions[&ModuleId(0)]
    ));
    input[0] = parsed(
        0,
        "fn first(x:i32)->i32=>x+111;fn sibling(x:i32)->i32=>x+2;",
    );
    let edited = checked(&mut session, &input, &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    assert_eq!(
        edited.function_owner(ModuleId(0), "first"),
        Some(&first_owner)
    );
    assert_eq!(
        edited.change_batch().context(),
        warm.change_batch().context()
    );
    assert!(Arc::ptr_eq(
        &warm.functions[&ModuleId(1)],
        &edited.functions[&ModuleId(1)]
    ));
    assert_eq!(edited.change_batch().changed_modules(), [0]);
    assert_eq!(edited.change_batch().changes().len(), 2);
    assert!(edited.change_batch().changes()[0].body_changed());
    assert!(!edited.change_batch().changes()[1].body_changed());
    for change in edited.change_batch().changes() {
        let (module, function) = edited.changed_function(change).unwrap();
        assert_eq!(module.core.module_id, ModuleId(0));
        assert_eq!(
            function.span,
            input[0].parsed().module.items[change.function_index()].span
        );
        assert!(cold.changed_function(change).is_none());
    }
    input[0] = parsed(
        0,
        "fn first(x:i64)->i64=>x+111;fn sibling(x:i32)->i32=>x+2;",
    );
    let interface = checked(&mut session, &input, &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    assert_ne!(
        edited.change_batch().context(),
        interface.change_batch().context()
    );
}

#[test]
fn private_body_projection_keeps_current_raw_node_and_temporary_bindings() {
    let limits = Limits::default();
    let text = "fn identity(x:i32)->i32=>x; fn first(x:i32)->i32=>x+1; fn second(x:i32)->i32 {return x |> identity;} fn third(x:i32)->i32=>x+3;";
    let mut session = ExpandSession::default();
    let cold = checked(&mut session, &[parsed(0, text)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    let current_text = text.replace(
        "fn first(x:i32)->i32=>x+1;",
        "fn first(x:i32)->i32 {return x |> identity;}",
    );
    let current = checked(&mut session, &[parsed(0, &current_text)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    assert_eq!(
        current
            .change_batch()
            .changes()
            .iter()
            .filter(|c| c.body_changed())
            .count(),
        2,
        "both edited function and later global-temporary consumer change"
    );
    assert_eq!(
        cold.function_owner(ModuleId(0), "second"),
        current.function_owner(ModuleId(0), "second")
    );
    assert_ne!(
        cold.function_input_revision(ModuleId(0), "second"),
        current.function_input_revision(ModuleId(0), "second")
    );
    assert_ne!(
        cold.function_body_revision(ModuleId(0), "second"),
        current.function_body_revision(ModuleId(0), "second"),
        "actual module-global temporary names remain body dependencies"
    );
    assert_eq!(
        cold.function_body_revision(ModuleId(0), "third"),
        current.function_body_revision(ModuleId(0), "third")
    );
    assert!(
        current
            .change_batch()
            .changes()
            .iter()
            .filter(|c| c.body_changed())
            .any(|c| current.changed_function(c).unwrap().1.name == "second")
    );
    assert!(
        !current
            .change_batch()
            .changes()
            .iter()
            .filter(|c| c.body_changed())
            .any(|c| current.changed_function(c).unwrap().1.name == "third")
    );
}

#[test]
fn item_lowering_replaces_actual_core_leaf_and_replays_current_source_map() {
    let limits = Limits::default();
    let text = "fn first(x:i32)->i32 {let a=x+1;return a+2;} fn second(x:i32)->i32 {let b=x+3;return b+4;} fn third(x:i32)->i32 {let c=x+5;return c+6;}";
    let mut session = ExpandSession::default();
    let cold = checked(&mut session, &[parsed(0, text)], &limits);
    let old_header = session.environment.as_ref().unwrap().headers[0].0.clone();
    let old_items = session.entries[&ModuleId(0)]
        .shared_restored
        .as_ref()
        .unwrap()
        .items
        .clone();
    assert_eq!(session.last_stats().executed_lower_items, 3);
    let input = [parsed(0, &text.replacen("x+1", "x+7", 1))];
    let changed = checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().reused_lower_items, 2);
    assert!(
        Arc::ptr_eq(
            &old_header,
            &session.environment.as_ref().unwrap().headers[0].0
        ),
        "the complete actual environment consumes the retained immutable header"
    );
    assert_eq!(session.last_stats().executed_lower_items, 1);
    assert!(session.last_stats().copied_lower_map_entries > 0);
    assert_eq!(session.last_stats().normalized_item_deep_clones, 1);
    assert_eq!(session.last_stats().restored_item_deep_clones, 0);
    assert_eq!(session.last_stats().owned_restore_items, 1);
    assert_eq!(session.last_stats().restored_module_deep_clones, 0);
    assert_eq!(session.last_stats().retained_restored_items, 2);
    let current_items = &session.entries[&ModuleId(0)]
        .shared_restored
        .as_ref()
        .unwrap()
        .items;
    for index in [1, 2] {
        assert!(Arc::ptr_eq(&old_items[index], &current_items[index]));
    }
    assert!(!Arc::ptr_eq(&old_items[0], &current_items[0]));
    let old_functions = &cold.modules[0].0.core.functions;
    let current_functions = &changed.modules[0].0.core.functions;
    assert_eq!(current_functions.changed_indices(old_functions), [0]);
    for index in [1, 2] {
        assert!(Arc::ptr_eq(
            old_functions.owner(index).unwrap(),
            current_functions.owner(index).unwrap()
        ));
    }
    // A raw transformation of the current result cannot change the old owner.
    let mut raw = changed.modules[0].0.core.clone();
    Arc::make_mut(&mut raw).functions[1].name = "mutated".into();
    assert_eq!(cold.modules[0].0.core.functions[1].name, "second");
    assert_eq!(changed.modules[0].0.core.functions[1].name, "second");
    let moved = [parsed(
        0,
        &input[0].source().text.replacen("x+7", "x+777", 1),
    )];
    let moved = checked(&mut session, &moved, &limits);
    assert_eq!(
        session.last_stats().reused_lower_items,
        0,
        "absolute Core/source-map positions stay current"
    );
    assert_eq!(session.last_stats().executed_lower_items, 3);
    assert_eq!(
        moved.modules[0]
            .0
            .core
            .functions
            .changed_indices(current_functions),
        [0, 1, 2]
    );
    let inserted = [parsed(
        0,
        &text.replacen("let a=x+1;", "let extra=x+9;let a=x+1;", 1),
    )];
    checked(&mut session, &inserted, &limits);
    assert_eq!(
        session.last_stats().reused_lower_items,
        0,
        "changed NodeId/map-base requires real current lowering"
    );
    assert_eq!(session.last_stats().executed_lower_items, 3);
}

#[test]
fn body_revision_separates_placement_without_hiding_generated_names() {
    let limits = Limits::default();
    let text = "fn first(x:i32)->i32=>x; fn discard(x:i32)->i32 {let _=x;return x;} fn sibling(x:i32)->i32=>x;";
    let mut session = ExpandSession::default();
    let initial = checked(&mut session, &[parsed(0, text)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    let edited = text.replace(
        "fn first(x:i32)->i32=>x;",
        "fn first(x:i32)->i32 {let alias=x;return alias;}",
    );
    let current = checked(&mut session, &[parsed(0, &edited)], &limits)
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    assert_ne!(
        initial.program().modules[0].core.functions[2].id,
        current.program().modules[0].core.functions[2].id
    );
    assert_ne!(
        initial.function_input_revision(ModuleId(0), "sibling"),
        current.function_input_revision(ModuleId(0), "sibling")
    );
    assert_eq!(
        initial.function_body_revision(ModuleId(0), "sibling"),
        current.function_body_revision(ModuleId(0), "sibling")
    );
    assert_eq!(
        initial.function_owner(ModuleId(0), "sibling"),
        current.function_owner(ModuleId(0), "sibling")
    );
    assert_ne!(
        initial.function_body_revision(ModuleId(0), "discard"),
        current.function_body_revision(ModuleId(0), "discard"),
        "actual $discard NodeId-derived Core names belong to body input"
    );
    assert_ne!(
        initial.function_body_revision(ModuleId(0), "first"),
        current.function_body_revision(ModuleId(0), "first")
    );
    assert_eq!(
        initial.change_batch().context(),
        current.change_batch().context()
    );
    assert_eq!(
        initial.change_batch().namespace_context(),
        current.change_batch().namespace_context()
    );
}

#[test]
fn scalar_inventory_is_shared_by_actual_consumers_across_body_and_position_edits() {
    let limits = Limits::default();
    let mut session = ExpandSession::default();
    let mut input = [
        parsed(
            0,
            "pub fn first(x:i32)->i32=>x+1; fn sibling(x:i32)->i32=>x+2;",
        ),
        parsed(1, "fn second(x:i32)->i32=>x+3;"),
    ];
    let cold = checked(&mut session, &input, &limits);
    let inventory = session.scalar_inventory.clone().unwrap();
    for text in [
        "pub fn first(x:i32)->i32=>x+9; fn sibling(x:i32)->i32=>x+2;",
        "pub fn first(x:i32)->i32 {let alias=x;return alias;} fn sibling(x:i32)->i32=>x+2;",
        "\n\n pub fn first(x:i32)->i32 {let alias=x;return alias;} fn sibling(x:i32)->i32=>x+2;",
    ] {
        input[0] = parsed(0, text);
        let current = checked(&mut session, &input, &limits);
        assert!(Arc::ptr_eq(
            &inventory,
            session.scalar_inventory.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(
            &cold.modules[1].0.core,
            &current.modules[1].0.core
        ));
        assert!(Arc::ptr_eq(&cold.modules[1].1, &current.modules[1].1));
    }
    // Real current raw IDs and map positions were compared against fresh above.
    // An actual declaration change replaces the table, even with identical body.
    input[0] = parsed(
        0,
        "fn renamed(x:i32)->i32=>x+9; fn sibling(x:i32)->i32=>x+2;",
    );
    checked(&mut session, &input, &limits);
    assert!(!Arc::ptr_eq(
        &inventory,
        session.scalar_inventory.as_ref().unwrap()
    ));
}

#[test]
fn scalar_inventory_errors_do_not_publish_and_current_budget_keeps_first_error() {
    let limits = Limits::default();
    let mut session = ExpandSession::default();
    let input = [parsed(
        0,
        "fn first(x:i32)->i32=>x+1; fn second(x:i32)->i32=>x+2;",
    )];
    checked(&mut session, &input, &limits);
    let inventory = session.scalar_inventory.clone().unwrap();
    for text in [
        "fn first(x:i32)->i32=>x+1; fn first(x:i32)->i32=>x+2;",
        "fn i32(x:i32)->i32=>x;",
    ] {
        let current = [parsed(0, text)];
        assert_eq!(
            expand(&mut session, &current, &limits).err().unwrap(),
            fresh(&current, &limits).unwrap_err()
        );
        assert!(Arc::ptr_eq(
            &inventory,
            session.scalar_inventory.as_ref().unwrap()
        ));
    }
    // Distinct entry allowances are current ledgers, not input identities.
    // Exercise exact total work N and N-1 with relocated current diagnostics.
    let current = [parsed(
        0,
        "\n fn first(x:i32)->i32=>x+11; fn second(x:i32)->i32=>x+2;",
    )];
    let allowance = |work| Limits {
        max_analysis_steps: work,
        ..limits.clone()
    };
    let mut low = 0;
    let mut high = limits.max_analysis_steps;
    while low < high {
        let mid = low + (high - low) / 2;
        if fresh(&current, &allowance(mid)).is_ok() {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    assert!(low > 0);
    for work in [0, 1, low / 2, low - 1] {
        let actual = expand(&mut session, &current, &allowance(work))
            .err()
            .unwrap();
        assert_eq!(
            actual,
            fresh(&current, &allowance(work)).unwrap_err(),
            "work={work}"
        );
        assert!(Arc::ptr_eq(
            &inventory,
            session.scalar_inventory.as_ref().unwrap()
        ));
    }
    checked(&mut session, &current, &allowance(low));
    assert!(Arc::ptr_eq(
        &inventory,
        session.scalar_inventory.as_ref().unwrap()
    ));
    checked(&mut session, &input, &limits);
    assert!(Arc::ptr_eq(
        &inventory,
        session.scalar_inventory.as_ref().unwrap()
    ));
}

#[test]
fn scalar_inventory_rebinds_real_import_positions_target_and_nonledger_limits() {
    let mut session = ExpandSession::default();
    checked_import_bindings(&mut session, &import_binding_inputs(""));
    let old = session.scalar_inventory.clone().unwrap();
    checked_import_bindings(&mut session, &import_binding_inputs("\n\n"));
    assert!(!Arc::ptr_eq(
        &old,
        session.scalar_inventory.as_ref().unwrap()
    ));

    let input = [parsed(0, "fn f(x:i32)->i32=>x+1;")];
    let mut session = ExpandSession::default();
    let limits = Limits::default();
    checked(&mut session, &input, &limits);
    let old = session.scalar_inventory.clone().unwrap();
    let current_limits = Limits {
        max_values: limits.max_values + 1,
        ..limits
    };
    checked(&mut session, &input, &current_limits);
    assert!(!Arc::ptr_eq(
        &old,
        session.scalar_inventory.as_ref().unwrap()
    ));
    let old = session.scalar_inventory.clone().unwrap();
    let mut target = TargetProfile::default();
    target.checked_arithmetic = !target.checked_arithmetic;
    let aliases = BTreeMap::new();
    let current = session
        .expand_parsed(
            &input,
            &[vec![]],
            &current_limits,
            &target,
            source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &aliases,
                declarations: source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .unwrap();
    let sources = input.iter().map(ParsedSource::source).collect::<Vec<_>>();
    let syntax = input.iter().map(|p| &p.parsed().module).collect::<Vec<_>>();
    let raw =
        expand_program_with_limits_target(&sources, &syntax, &[vec![]], &current_limits, &target)
            .unwrap();
    for ((module, map), (raw_module, raw_map)) in current.modules.iter().zip(raw) {
        assert_eq!(module, &raw_module);
        assert_eq!(map.as_ref(), &raw_map);
    }
    assert!(!Arc::ptr_eq(
        &old,
        session.scalar_inventory.as_ref().unwrap()
    ));
}

#[test]
fn deferred_scalar_stage_misses_materialize_real_normalized_input_once() {
    let input = [parsed(0, "fn f(x:i32)->i32{let y=x+1;return y*2;}")];
    let span = input[0].parsed().module.items[0].span;
    for stage in [
        Stage::Prepare,
        Stage::Lifetimes,
        Stage::Headers,
        Stage::Distribute,
        Stage::Restore,
        Stage::RestoreItem,
    ] {
        let mut session = ExpandSession::default();
        let cold = checked(&mut session, &input, &Limits::default());
        assert_eq!(session.last_stats().owned_restore_items, 1);
        assert_eq!(session.last_stats().restored_item_deep_clones, 0);
        assert!(matches!(
            &session.entries[&ModuleId(0)].normalized[&span.start],
            NormalizedCache::Deferred(_)
        ));
        // RestoreItem is reached only when the whole-module receipt misses.
        let entry = session.entries.get_mut(&ModuleId(0)).unwrap();
        let receipts = Arc::make_mut(&mut entry.receipts);
        let at = if stage == Stage::Restore {
            input[0].parsed().module.span.start
        } else {
            span.start
        };
        receipts.remove(&(stage, at));
        if stage == Stage::RestoreItem {
            receipts.remove(&(Stage::Restore, input[0].parsed().module.span.start));
        }
        let warm = checked(&mut session, &input, &Limits::default());
        assert_eq!(warm.modules, cold.modules);
        let work = session.last_stats();
        let materializes = stage != Stage::Restore;
        assert_eq!(work.normalized_materializations, u64::from(materializes));
        assert_eq!(work.normalized_item_deep_clones, u64::from(materializes));
        assert_eq!(work.restored_item_deep_clones, 0);
        if materializes {
            assert!(work.normalized_reconstruction_nodes > 0);
        }
    }
}

#[test]
fn deferred_scalar_headers_require_explicit_complete_false_outcome() {
    let input = [parsed(0, "fn f(x:i32)->i32=>x+1;")];
    let span = input[0].parsed().module.items[0].span;
    for missing in [true, false] {
        let mut session = ExpandSession::default();
        checked(&mut session, &input, &Limits::default());
        let entry = session.entries.get_mut(&ModuleId(0)).unwrap();
        let outcomes = Arc::make_mut(&mut entry.scalar_headers);
        if missing {
            outcomes.remove(&span.start);
        } else {
            outcomes.get_mut(&span.start).unwrap().has_alias = true;
        }
        checked(&mut session, &input, &Limits::default());
        assert_eq!(session.last_stats().normalized_materializations, 1);
        assert_eq!(session.last_stats().plain_header_items, 1);
        assert!(session.entries[&ModuleId(0)].scalar_headers[&span.start].closed());
    }
}

#[test]
fn transferred_scalar_recipe_rebinds_the_actual_current_parser_owner() {
    let old = [parsed(0, "fn f()->i32=>1;fn g()->i32=>2;")];
    let current = [parsed(0, "fn f()->i32=>3;fn g()->i32=>2;")];
    let mut session = ExpandSession::default();
    checked(&mut session, &old, &Limits::default());
    checked(&mut session, &current, &Limits::default());
    let entry = &session.entries[&ModuleId(0)];
    for (ordinal, item) in current[0].parsed().module.items.iter().enumerate() {
        let NormalizedCache::Deferred(recipe) = &entry.normalized[&item.span.start] else {
            panic!("cold/changed products retain recipes, not complete normalized bodies");
        };
        assert!(recipe.parsed.same_revision(&current[0]));
        assert_eq!(recipe.ordinal, ordinal);
        assert_eq!(recipe.span, item.span);
        assert_eq!(
            recipe.environment.as_ref(),
            session.environment.as_ref().unwrap()
        );
    }
    assert_eq!(session.last_stats().normalized_recipe_hits, 1);
    assert_eq!(session.last_stats().normalized_item_deep_clones, 1);
    assert_eq!(session.last_stats().owned_restore_items, 1);
}

#[test]
fn deferred_scalar_insufficient_stage_receipts_do_not_predebit_actual_visitors() {
    let input = [parsed(0, "fn f(x:i32)->i32=>x+1;")];
    let span = input[0].parsed().module.items[0].span;
    for stage in [
        Stage::Prepare,
        Stage::Lifetimes,
        Stage::Headers,
        Stage::Distribute,
        Stage::RestoreItem,
    ] {
        let mut session = ExpandSession::default();
        checked(&mut session, &input, &Limits::default());
        let entry = session.entries.get_mut(&ModuleId(0)).unwrap();
        let receipts = Arc::make_mut(&mut entry.receipts);
        receipts.insert(
            (stage, span.start),
            Receipt {
                nodes: usize::MAX,
                work: usize::MAX,
            },
        );
        if stage == Stage::RestoreItem {
            receipts.remove(&(Stage::Restore, input[0].parsed().module.span.start));
        }
        checked(&mut session, &input, &Limits::default());
        assert!(session.last_stats().receipt_budget_misses > 0);
        assert_eq!(session.last_stats().normalized_materializations, 1);
        assert_eq!(session.last_stats().restored_item_deep_clones, 0);
    }
}

#[test]
fn deferred_scalar_cross_module_restore_keeps_real_import_and_source_map_checks() {
    let input = import_binding_inputs("");
    let mut session = ExpandSession::default();
    let cold = checked_import_bindings(&mut session, &input);
    let span = input[0]
        .parsed()
        .module
        .items
        .iter()
        .find(|item| matches!(item.kind, s::ItemKind::Function(_)))
        .unwrap()
        .span;
    let entry = session.entries.get_mut(&ModuleId(0)).unwrap();
    let receipts = Arc::make_mut(&mut entry.receipts);
    receipts.remove(&(Stage::Restore, input[0].parsed().module.span.start));
    receipts.remove(&(Stage::RestoreItem, span.start));
    let current = checked_import_bindings(&mut session, &input);
    assert_eq!(cold.program(), current.program());
    assert_eq!(session.last_stats().normalized_materializations, 1);
    assert_eq!(session.last_stats().restored_item_deep_clones, 0);
    let source = input[0].source();
    let restored = session.entries[&ModuleId(0)]
        .shared_restored
        .as_ref()
        .unwrap();
    assert_eq!(restored.imports.len(), 1);
    assert_eq!(restored.imports[0].alias, "dep");
    assert_eq!(restored.span.file, source.file_id);
    let map = &session.entries[&ModuleId(0)]
        .lower
        .as_ref()
        .unwrap()
        .1
        .source_map;
    map.validate_iter(input.iter().map(ParsedSource::source), &Limits::default())
        .unwrap();
}

#[test]
fn deferred_reconstruction_uses_normalize_entry_allowance_not_later_remainder() {
    let input = [parsed(0, "fn f(x:i32)->i32=>x+1;")];
    let limits = Limits::default();
    let span = input[0].parsed().module.items[0].span;
    for stage in [Stage::Lifetimes, Stage::Headers] {
        let mut session = ExpandSession::default();
        let before = checked(&mut session, &input, &limits);
        // Deliberately exhaust the separate generic ledger through Prepare's
        // receipt. This is an internal ledger adversary, not a claim that
        // these fabricated costs describe the fixture. Normalize still has
        // its real entry allowance; only the later actual visitor must fail.
        let entry = session.entries.get_mut(&ModuleId(0)).unwrap();
        let receipts = Arc::make_mut(&mut entry.receipts);
        let lifetime_nodes = if stage == Stage::Headers {
            receipts[&(Stage::Lifetimes, span.start)].nodes
        } else {
            0
        };
        receipts.insert(
            (Stage::Prepare, span.start),
            Receipt {
                nodes: limits.max_nodes - lifetime_nodes,
                work: 0,
            },
        );
        receipts.remove(&(stage, span.start));
        let errors = expand(&mut session, &input, &limits)
            .err()
            .expect("actual later limit");
        let s::ItemKind::Function(function) = &input[0].parsed().module.items[0].kind else {
            unreachable!("function fixture");
        };
        let expected_span = if stage == Stage::Headers {
            span
        } else {
            function.params[0].ty.span
        };
        assert_eq!(
            errors,
            vec![Diagnostic::limit(
                Phase::Expand,
                "generic expansion budget exceeded",
                Some(expected_span)
            )]
        );
        assert_eq!(
            session.last_stats().normalized_materializations,
            1,
            "Normalize reconstructs before the untouched failing later visit"
        );
        assert!(session.last_stats().normalized_reconstruction_nodes > 0);
        assert_eq!(
            session.entries[&ModuleId(0)].revision,
            before.revisions[&ModuleId(0)],
            "failure never publishes tentative normalized/header/restored products"
        );
    }
}
