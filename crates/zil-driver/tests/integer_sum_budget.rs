//! The complete numeric text consumer, without changing its source or budget.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
};
fn project() -> ProjectSources {
    ProjectSources {
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
    }
}
#[test]
fn complete_ten_width_source_fits_default_analysis_budget() {
    let report = compile_project(
        &project(),
        &CompileOptions {
            stop_after: Stage::Analyze,
            limits: Limits::default(),
            ..CompileOptions::default()
        },
    );
    eprintln!("{}", report.to_json());
    assert_eq!(report.status, ResultCategory::Success);
}

#[test]
fn complete_ten_width_matrix_executes_with_default_budget_on_both_backends() {
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../out/runs/ownership-public/integer-matrix-native")
                .join(format!(
                    "{}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
            std::fs::create_dir_all(&dir).unwrap();
            let report = compile_project(
                &project(),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            std::fs::write(dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let summary = report.native_codegen.as_ref().unwrap();
            assert_eq!(summary.backend, backend);
            assert!(summary.fallback.is_none());
            let output = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            std::fs::write(dir.join("stdout"), &output.stdout).unwrap();
            std::fs::write(dir.join("stderr"), &output.stderr).unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {output:?}"
            );
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
        }
    }
}
