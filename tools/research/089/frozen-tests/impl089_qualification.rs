use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, QueryDisposition, Stage};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "089-qualification.zil", text)
}
fn parse_options() -> CompileOptions {
    CompileOptions {
        stop_after: Stage::Parse,
        ..CompileOptions::default()
    }
}
fn same(a: &CompileReport, b: &CompileReport) {
    assert_eq!(a.status, b.status);
    assert_eq!(a.phase, b.phase);
    assert_eq!(
        format!("{:?}", a.diagnostics),
        format!("{:?}", b.diagnostics)
    );
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.verification_runs, b.verification_runs);
}

#[test]
fn hot_and_cold_parse_budget_boundaries_have_identical_diagnostics() {
    let input = source("#[doc=\"retained\"] fn main()->i32 { let x = (1+2)*(3+4); return x }");
    let mut hot = CompilationSession::with_capacity(4096);
    assert_eq!(
        hot.compile_source(&input, &parse_options()).status,
        ResultCategory::Success
    );
    for field in [0, 1] {
        for n in 0..96 {
            let mut options = parse_options();
            if field == 0 {
                options.limits.max_tokens = n
            } else {
                options.limits.max_nodes = n
            }
            let warm = hot.compile_source(&input, &options);
            let cold = CompilationSession::default().compile_source(&input, &options);
            same(&warm, &cold);
            assert_eq!(hot.active_query_count(), 0);
        }
    }
}

#[test]
fn smaller_successful_budget_reuses_parse_but_not_other_phase_identity() {
    let input = source("fn main()->i32=>0");
    let mut session = CompilationSession::default();
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let cold = session.compile_source(&input, &options);
    assert_eq!(cold.status, ResultCategory::Success);
    let mut smaller = options.clone();
    smaller.limits.max_nodes = 1000;
    smaller.limits.max_tokens = 1000;
    let warm = session.compile_source(&input, &smaller);
    assert_eq!(warm.status, ResultCategory::Success);
    assert_eq!(warm.verification_runs, 1);
    assert!(
        warm.query_events
            .iter()
            .any(|e| e.phase == Phase::Parse && e.disposition == QueryDisposition::Reused)
    );
    assert!(
        warm.query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
    );
    assert_eq!(warm.scalar_interfaces, cold.scalar_interfaces);
}

#[test]
fn failed_then_successful_and_cancelled_then_recovered_requests_do_not_poison_cache() {
    let input = source("fn main()->i32=>0");
    let mut session = CompilationSession::default();
    let mut options = parse_options();
    options.limits = Limits {
        max_nodes: 0,
        ..Limits::default()
    };
    assert_eq!(
        session.compile_source(&input, &options).status,
        ResultCategory::ResourceLimit
    );
    assert_eq!(session.cached_query_count(), 0);
    options.limits = Limits::default();
    assert_eq!(
        session.compile_source(&input, &options).status,
        ResultCategory::Success
    );
    options.cancellation.cancel();
    assert_eq!(
        session.compile_source(&input, &options).status,
        ResultCategory::Cancelled
    );
    assert_eq!(
        session.compile_source(&input, &parse_options()).status,
        ResultCategory::Success
    );
    assert_eq!(session.active_query_count(), 0);
}
