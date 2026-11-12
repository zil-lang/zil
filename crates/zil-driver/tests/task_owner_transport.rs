use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/task-owner-transport/returned-task-fnonce.zil");
#[test]
fn returned_fnonce_moves_the_actual_live_task_and_retains_its_parent() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "returned-task-fnonce.zil", SOURCE);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &zil_base::Limits::default(),
    )
    .unwrap();
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../out/runs/task-owner-factory-typed.txt"),
        format!("{typed:#?}"),
    )
    .unwrap();
    let report = compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../out/runs/task-owner-factory-first.json"),
        report.to_json(),
    )
    .unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}
