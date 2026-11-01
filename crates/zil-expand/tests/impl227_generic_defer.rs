use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
fn expand(text: &str, limits: &Limits) -> Result<zil_ir::CoreModule, Vec<zil_base::Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "generic-defer227.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand_with_map_limits(&source, &syntax, limits).map(|v| v.core)
}
const UNUSED: &str = "extern \"C\" fn putchar(n:i32)->i32; fn unused<T:Copy>(value:T){defer{unsafe{putchar(81)}}} fn main(){}";
#[test]
fn original_joint_sources_reach_core_without_erasing_their_defer() {
    for text in [
        include_str!("../../zil-driver/tests/impl237_fixtures/defer_f_implicit.zil"),
        include_str!("../../zil-driver/tests/impl237_fixtures/defer_f_explicit.zil"),
    ] {
        let core = expand(text, &Limits::default()).unwrap();
        assert!(core.functions.iter().any(|f| {
            f.body
                .statements
                .iter()
                .any(|s| matches!(s.kind, zil_ir::CoreStatementKind::Defer(_)))
        }));
    }
    expand(UNUSED, &Limits::default()).unwrap();
}
#[test]
fn unused_cleanup_bodies_cannot_escape_their_control_boundary_or_gain_callable_permission() {
    for body in [
        "return",
        "defer{}",
        "break",
        "continue",
        "let value=fail()?",
    ] {
        let text = format!(
            "fn fail()->void!i32{{return ()}} fn unused<T:Copy>(value:T){{while true{{defer{{{body}}};break}}}}fn main(){{}}"
        );
        let errors = expand(&text, &Limits::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Expand && d.code.starts_with("E-DEFER-")),
            "{body}: {errors:#?}"
        );
    }
    for text in [
        "fn unused<F:FnOnce()->i32>(f:F){defer{f();f()}}fn main(){}",
        "fn unused<F:FnOnce()->i32>(f:&F){defer{f()}}fn main(){}",
        "fn unused<F:FnMut()->i32>(f:&F){defer{f()}}fn main(){}",
        "fn unused<F:FnOnce()->i32>(f:F){defer{f()}f()}fn main(){}",
        "fn unused<F:Fn()->i32>(f:F){defer{f()}let moved=f}fn main(){}",
    ] {
        assert!(expand(text, &Limits::default()).is_err(), "{text}");
    }
    expand("fn unused<T:Copy>(value:T){defer{var n=0;while n<3{n+=1;if n<2{continue}break}}}fn main(){}",&Limits::default()).unwrap();
}
#[test]
fn symbolic_cleanup_growth_is_prepaid_and_has_a_fresh_exact_work_boundary() {
    let full = expand(UNUSED, &Limits::default()).unwrap();
    let probe = |work| {
        expand(
            UNUSED,
            &Limits {
                max_analysis_steps: work,
                ..Limits::default()
            },
        )
    };
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        match probe(mid) {
            Ok(_) => high = mid,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit)
                );
                low = mid;
            }
        }
    }
    assert_eq!(probe(high).unwrap(), full);
    assert!(
        probe(high - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(probe(high).unwrap(), full);
    eprintln!("generic_defer227_work_n={high}");
}
