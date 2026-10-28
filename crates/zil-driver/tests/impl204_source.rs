//! Real source controls for finite owned leaf-Store captures.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, ProjectSources, Stage, compile_project};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const ORIGINALS: [&str; 4] = [
    include_str!("impl204_fixtures/00_package_owned_control.zil"),
    include_str!("impl204_fixtures/01_direct_store_capture.zil"),
    include_str!("impl204_fixtures/02_original201_package_store_capture.zil"),
    include_str!("impl204_fixtures/03_mutating_store_capture.zil"),
];

fn compile(label: &str, source: &str, stage: Stage) -> (PathBuf, CompileReport) {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = root.join(format!(
        "capture204-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("source.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: Default::default(),
        },
        &CompileOptions {
            stop_after: stage,
            emit_dir: Some(directory.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(directory.join("compile.json"), report.to_json()).unwrap();
    (directory, report)
}

fn through_types(label: &str, source: &str) {
    let (_, report) = compile(label, source, Stage::Types);
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn original_complete_package_control_reaches_types() {
    through_types("original-control", ORIGINALS[0]);
}

#[test]
fn original_direct_store_capture_reaches_types() {
    through_types("original-direct", ORIGINALS[1]);
}

#[test]
fn original_package_store_capture_reaches_types() {
    through_types("original-package", ORIGINALS[2]);
}

#[test]
fn original_mutating_store_capture_reaches_types() {
    through_types("original-mutating", ORIGINALS[3]);
}
