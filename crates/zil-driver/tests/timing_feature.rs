#![cfg(not(feature = "timings"))]
use std::collections::BTreeMap;
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, ProjectSources, Stage};

#[test]
fn unavailable_requested_timings_reject_all_driver_entry_points() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", "fn main()->i32=>42");
    let options = CompileOptions {
        timings: true,
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![source.clone()],
        paths: BTreeMap::from([(vec![], ModuleId(0))]),
    };
    let mut session = CompilationSession::default();
    for report in [
        session.compile_source(&source, &options),
        session.compile_project(&project, &options),
        session.compile_path(
            std::path::Path::new("not-read-for-unavailable-timings.zil"),
            &options,
        ),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Driver);
        assert_eq!(report.verification_runs, 0);
        assert!(report.timings.is_none());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("--features timings"))
        );
    }
    assert_eq!(session.cached_query_count(), 0);
    let ordinary = session.compile_source(&source, &CompileOptions::default());
    assert_eq!(
        ordinary.status,
        ResultCategory::Success,
        "{}",
        ordinary.to_json()
    );
    assert_eq!(ordinary.verification_runs, 1);
    assert!(ordinary.timings.is_none());
}
