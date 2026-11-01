//! Preserve the original ALQZ obligation while extending only ordinary generic
//! cleanup admission; these are author tests, not an independent review.
use std::{fs, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const IMPLICIT: &str = include_str!("impl237_fixtures/defer_f_implicit.zil");
const EXPLICIT: &str = include_str!("impl237_fixtures/defer_f_explicit.zil");
#[test]
fn original_f_helper_defer_and_implicit_or_explicit_selfcall_execute_alqz() {
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    for (name, text) in [("implicit", IMPLICIT), ("explicit", EXPLICIT)] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(format!(
                    "author237-defer-{name}-{optimization:?}-{}",
                    std::process::id()
                ));
            fs::create_dir_all(&root).unwrap();
            fs::write(root.join("source.zil"), text).unwrap();
            let executable = root.join("program");
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "defer237.zil", text),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(executable.clone()),
                    emit_dir: Some(root.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(root.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let summary = report.native_codegen.unwrap();
            assert_eq!(summary.backend, backend);
            assert!(summary.fallback.is_none());
            let output = Command::new(executable).output().unwrap();
            fs::write(root.join("stdout"), &output.stdout).unwrap();
            fs::write(root.join("stderr"), &output.stderr).unwrap();
            fs::write(root.join("exit.txt"), format!("{:?}", output.status.code())).unwrap();
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(output.stdout, b"ALQZ");
            assert!(output.stderr.is_empty());
        }
    }
}
