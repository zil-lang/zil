//! Experimental composition of query identities, closure methods and FIFO lifecycle.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
const RECORDS: &str = include_str!("../../../examples/mainline/growing_inbox/queue.zil");

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn sources(main: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "queue.zil", RECORDS),
        ],
        paths: BTreeMap::from([(vec!["queue".into()], ModuleId(1))]),
    }
}
fn check(main: &str) -> CompileReport {
    compile_project(&sources(main), &CompileOptions::default())
}
fn native(main: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "queue-layout153-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("main.zil"), main).unwrap();
        fs::write(dir.join("queue.zil"), RECORDS).unwrap();
        let executable = dir.join("program");
        let report = compile_project(
            &sources(main),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
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
        let result = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &result.stdout).unwrap();
        fs::write(dir.join("stderr"), &result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
    }
}
#[test]
fn queue_callback_trait_layout_output_close_and_cancel_compose() {
    native(include_str!(
        "../../../examples/mainline/growing_inbox/main.zil"
    ));
}
#[test]
fn late_use_of_output_window_still_reaches_independent_verify() {
    let r = check(
        "import queue\nfn main()->i32 {let b=bytes_new();bytes_push(b,1u8);let view=bytes_view(b);var q=queue.make(0,10);q(1);bytes_push(b,2u8);return i32(view.len())}",
    );
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert!(
        r.diagnostics.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        r.to_json()
    );
}
