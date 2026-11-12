use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/task-thread-transfer/scalar.zil");
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "task-transfer.zil", text)
}
#[test]
fn actual_transfer_task_source_reaches_both_kernels() {
    let report = compile_source(
        &source(SOURCE),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
