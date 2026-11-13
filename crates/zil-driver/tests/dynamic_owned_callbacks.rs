use std::{collections::BTreeMap, fs, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
};

#[test]
fn affine_dynamic_owner_and_program_static_capture_keep_their_distinct_responsibilities() {
    let text = include_str!("../../../tests/fixtures/dyn-context/closed-owner-static-callback.zil");
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)],
        paths: BTreeMap::new(),
    };
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "closed-owner-static-callback-{}",
            std::process::id()
        ));
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = root.join(format!("{backend:?}-{optimization:?}"));
            fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("program");
            let report = compile_project(
                &project,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let output = Command::new(executable).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
        }
    }
}
