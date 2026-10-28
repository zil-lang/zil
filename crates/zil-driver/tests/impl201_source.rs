//! Source Store elements keep each environment's real position and responsibility.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "store201-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn compile(case: &Case, source: &str, stage: Stage, optimization: Optimization) -> CompileReport {
    fs::write(case.0.join("main.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: Default::default(),
        },
        &CompileOptions {
            stop_after: stage,
            optimization,
            output: (stage == Stage::Build).then(|| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
    report
}
fn types(label: &str, source: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, Stage::Types, Optimization::O0);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(report.verification_runs, 0);
}
fn native(label: &str, source: &str, expected_stdout: &str, expected_exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Stage::Build, optimization);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(case.0.join("stdout.log"), &output.stdout).unwrap();
        fs::write(case.0.join("stderr.log"), &output.stderr).unwrap();
        fs::write(case.0.join("native.txt"), format!("optimization={optimization:?}\nexit={:?}\nexpected_exit={expected_exit}\nexpected_stdout={expected_stdout:?}\n", output.status.code())).unwrap();
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "{label}: {output:?}"
        );
        assert_eq!(
            output.stdout,
            expected_stdout.as_bytes(),
            "{label}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

const ORIGINAL02: &str = include_str!("impl200_fixtures/02_store_of_callback_state.zil");
const PAIR_MUT: &str = include_str!("impl201_fixtures/01_pair_store_mut.zil");

#[test]
fn original02_types() {
    types("original02-types", ORIGINAL02);
}

#[test]
fn store_field_fnmut_types() {
    types("pair-mut-types", PAIR_MUT);
}

#[test]
fn original02_verify_native() {
    native("original02-native", ORIGINAL02, "", 0);
}

#[test]
fn store_field_fnmut_verify_native() {
    native("pair-mut-native", PAIR_MUT, "1A2BZ", 0);
}
