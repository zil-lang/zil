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
) -> CompileResult<Vec<(ProgramModule, SourceMap)>> {
    let sources: Vec<_> = input.iter().map(|p| p.source().clone()).collect();
    let syntax: Vec<_> = input.iter().map(|p| &p.parsed().module).collect();
    expand_program_with_limits_target(
        &sources,
        &syntax,
        &vec![vec![]; input.len()],
        limits,
        &TargetProfile::default(),
    )
}
fn assert_complete_modules(
    actual: &[(ProgramModule, ProducedSourceMap)],
    expected: &[(ProgramModule, SourceMap)],
) {
    assert_eq!(actual.len(), expected.len());
    for ((actual, map), (expected, original_map)) in actual.iter().zip(expected) {
        assert_eq!(actual, expected);
        assert_eq!(&**map, original_map);
    }
}
fn checked(
    session: &mut ExpandSession,
    input: &[ParsedSource],
    limits: &Limits,
) -> ExpandedProgram {
    let expanded = expand(session, input, limits)
        .unwrap()
        .expect("sealed scalar path");
    assert_complete_modules(&expanded.modules, &fresh(input, limits).unwrap());
    expanded
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
    assert!(session.last_stats().cloned_source_map_entries > 0);
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
            (Ok(Some(actual)), Ok(expected)) => assert_complete_modules(&actual.modules, &expected),
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
    let expected = raw(&limits).unwrap();
    let mut session = ExpandSession::default();
    let produce = |session: &mut ExpandSession, limits: &Limits| {
        session.expand_parsed(&input, &imports, limits, &target, context)
    };
    let cold = produce(&mut session, &limits)
        .unwrap()
        .expect("scalar unavailable domain");
    assert_complete_modules(&cold.modules, &expected);
    let warm = produce(&mut session, &limits).unwrap().unwrap();
    assert_complete_modules(&warm.modules, &expected);
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
                (Ok(Some(actual)), Ok(expected)) => {
                    assert_complete_modules(&actual.modules, &expected)
                }
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
    let mut reuse = Reuse {
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
fn producer_retains_complete_ast_products_without_warm_deep_clones() {
    let input = [
        parsed(0, "fn local()->i32=>9;pub fn first(x:i32)->i32=>x+local();"),
        parsed(1, "pub fn second(x:i32)->i32=>x+2;"),
        parsed(2, "pub fn third(x:i32)->i32=>x+3;"),
    ];
    let mut session = ExpandSession::default();
    let limits = Limits::default();
    let cold = checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().normalized_item_deep_clones, 4);
    assert_eq!(session.last_stats().restored_module_deep_clones, 3);
    assert_eq!(session.last_stats().retained_normalized_items, 0);
    checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().normalized_item_deep_clones, 0);
    assert_eq!(session.last_stats().restored_module_deep_clones, 0);
    assert_eq!(session.last_stats().retained_normalized_items, 4);
    assert_eq!(session.last_stats().retained_restored_modules, 3);
    assert_eq!(session.last_stats().syntax_clone_receipt_nodes, 0);
    assert_eq!(session.last_stats().cloned_source_map_entries, 0);
    assert_eq!(
        session.last_stats().core_module_deep_clones,
        3,
        "public Core Vec assembly still copies each module"
    );
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
    assert_eq!(session.last_stats().restored_module_deep_clones, 1);
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
    assert_eq!(
        session.last_stats().restored_module_deep_clones,
        2,
        "changed first module and insufficient retained second module both copy complete normalized bodies"
    );
    assert!(session.last_stats().receipt_budget_misses > 0);
}

#[test]
fn producer_shared_maps_equal_complete_raw_values_and_count_only_real_copies() {
    let input = [
        parsed(0, "fn first()->i32=>1;"),
        parsed(1, "fn second(x:i32)->i32=>x+2;"),
    ];
    let limits = Limits::default();
    let original = fresh(&input, &limits).unwrap();
    let entry_count: u64 = original
        .iter()
        .map(|(_, map)| map.entries.len() as u64)
        .sum();
    let mut session = ExpandSession::default();
    let cold = checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().shared_maps, 2);
    assert_eq!(session.last_stats().shared_entries, entry_count);
    assert_eq!(session.last_stats().cloned_source_map_entries, entry_count);
    assert_eq!(session.last_stats().source_map_deep_clones, 2);
    assert_eq!(session.last_stats().core_module_deep_clones, 2);
    let cold_maps: Vec<_> = cold.modules.iter().map(|(_, map)| map).collect();
    let expected_maps: Vec<_> = original.iter().map(|(_, map)| map).collect();
    assert_eq!(format!("{cold_maps:?}"), format!("{expected_maps:?}"));
    assert_eq!(format!("{cold_maps:#?}"), format!("{expected_maps:#?}"));
    checked(&mut session, &input, &limits);
    assert_eq!(session.last_stats().cloned_source_map_entries, 0);
    assert_eq!(session.last_stats().source_map_deep_clones, 0);
    assert_eq!(session.last_stats().shared_maps, 2);
    assert_eq!(session.last_stats().shared_entries, entry_count);
    assert_eq!(session.last_stats().core_module_deep_clones, 2);
    let changed = [parsed(0, "fn first()->i32=>10;"), input[1].clone()];
    checked(&mut session, &changed, &limits);
    assert_eq!(session.last_stats().source_map_deep_clones, 1);
    assert_eq!(
        session.last_stats().cloned_source_map_entries,
        fresh(&changed, &limits).unwrap()[0].1.entries.len() as u64
    );
    assert_eq!(session.last_stats().shared_maps, 2);
    assert_eq!(session.last_stats().core_module_deep_clones, 2);
    let owned: ProducedSourceMap = original[0].1.clone().into();
    assert_eq!(&*owned, &original[0].1);
    assert_eq!(format!("{owned:#?}"), format!("{:#?}", original[0].1));
}

#[test]
fn producer_retained_map_does_not_retain_replaced_cached_core() {
    let input = [parsed(0, "fn first()->i32=>1;")];
    let limits = Limits::default();
    let expected = fresh(&input, &limits).unwrap().remove(0).1;
    let mut session = ExpandSession::default();
    let expanded = checked(&mut session, &input, &limits);
    let weak = Arc::downgrade(&session.entries[&ModuleId(0)].lower.as_ref().unwrap().1);
    let (program, mut maps) = expanded.into_program(ModuleId(0), Fingerprint::ZERO);
    let retained_map = maps.remove(0);
    drop(program);
    drop(maps);
    // An actual source replacement drops the old CachedLower owner. The map
    // alone must not keep its Core alive, while its own value remains usable.
    checked(&mut session, &[parsed(0, "fn first()->i32=>10;")], &limits);
    assert!(
        weak.upgrade().is_none(),
        "historical SourceMap kept the old Core cache alive"
    );
    drop(session);
    assert_eq!(&*retained_map, &expected);
    assert_eq!(format!("{retained_map:#?}"), format!("{expected:#?}"));
}
