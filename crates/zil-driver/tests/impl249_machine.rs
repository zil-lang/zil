//! Source-derived owning callback ABI and Once cleanup at both LLVM levels.
use std::{fs, path::Path, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

const EVENT_REPLY: &str =
    include_str!("../../../evidence/planning/owned-callback-finalR/event-reply/source.zil");
const EVENT_COPY_RESULT: &str =
    include_str!("../../../evidence/planning/owned-callback-finalR/event-copy-result/source.zil");
const OWNED_RESULT: &str =
    include_str!("../../../evidence/planning/owned-callback-finalR/owned-result/source.zil");

#[test]
fn impl249_concrete_owned_event_and_result_execute_o0_o2() {
    let report_root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("impl249-{}", std::process::id()));
    for (name, source) in [
        ("event-reply", EVENT_REPLY),
        ("event-copy-result", EVENT_COPY_RESULT),
        ("owned-result", OWNED_RESULT),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = report_root.join(format!("{name}-{optimization:?}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("main.zil"), source).unwrap();
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source),
                &CompileOptions {
                    backend: NativeBackend::Clang,
                    optimization,
                    stop_after: Stage::Build,
                    emit_dir: Some(dir.join("phases")),
                    output: Some(dir.join("program")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let program = &report.artifacts["executable"];
            let actual = Command::new(Path::new(program)).output().unwrap();
            fs::write(dir.join("stdout"), &actual.stdout).unwrap();
            fs::write(dir.join("stderr"), &actual.stderr).unwrap();
            fs::write(dir.join("exit"), format!("{:?}\n", actual.status.code())).unwrap();
            assert_eq!(
                actual.status.code(),
                Some(0),
                "{name}/{optimization:?}: {actual:?}"
            );
            assert!(actual.stdout.is_empty());
            assert!(actual.stderr.is_empty());
        }
    }
}
