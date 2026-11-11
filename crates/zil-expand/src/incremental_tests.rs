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
