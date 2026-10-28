//! Source admission is separate from resource proof and native execution.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const INPUTS: &[(&str, &str)] = &[
    (
        "original120",
        include_str!("impl206_fixtures/00_original120.zil"),
    ),
    (
        "child-only",
        include_str!("impl206_fixtures/00_child_only.zil"),
    ),
    (
        "mixed-created",
        include_str!("impl206_fixtures/01_mixed_child_created.zil"),
    ),
    (
        "mixed-incoming",
        include_str!("impl206_fixtures/01_mixed_child_incoming.zil"),
    ),
    (
        "whole-created",
        include_str!("impl206_fixtures/02_whole_transfer_twice_created.zil"),
    ),
    (
        "whole-incoming",
        include_str!("impl206_fixtures/02_whole_transfer_twice_incoming.zil"),
    ),
    (
        "wrappers-created",
        include_str!("impl206_fixtures/03_child_only_wrappers_created.zil"),
    ),
    (
        "wrappers-incoming",
        include_str!("impl206_fixtures/03_child_only_wrappers_incoming.zil"),
    ),
    (
        "ordinary-calls",
        include_str!("impl206_fixtures/04_unknown_ordinary_calls_do_not_create.zil"),
    ),
];

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "product206-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn original_sources_reach_types_without_claiming_freshness_or_execution() {
    for (label, text) in INPUTS {
        let dir = directory(label);
        fs::write(dir.join("source.zil"), text).unwrap();
        let source = SourceUnit::new(FileId(0), ModuleId(0), "product206.zil", *text);
        let report = compile_source(
            &source,
            &CompileOptions {
                stop_after: Stage::Types,
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}
