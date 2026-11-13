//! The complete numeric text consumer, without changing its source or budget.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project};
#[test]
fn complete_ten_width_source_fits_default_analysis_budget() {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: [
            (
                "main.zil",
                include_str!("../../../tests/fixtures/numeric-text/integer-widths.zil"),
            ),
            (
                "std.int.zil",
                include_str!("../../../tests/fixtures/numeric-text/integer-widths-library.zil"),
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (name, text))| SourceUnit::new(FileId(i as u32), ModuleId(i as u32), name, text))
        .collect(),
        paths: [(vec!["std".into(), "int".into()], ModuleId(1))].into(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Analyze,
            limits: Limits::default(),
            ..CompileOptions::default()
        },
    );
    eprintln!("{}", report.to_json());
    assert_eq!(report.status, ResultCategory::Success);
}
