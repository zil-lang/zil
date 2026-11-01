//! Real filesystem discovery candidates remain raw and all current files/imports
//! and Verify are checked on every request.
#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::symlink,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::ResultCategory;
use zil_driver::{CompilationSession, CompileOptions, CompileReport, DiskCacheStatus};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-discovery-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("sub")).unwrap();
        fs::write(
            path.join("main.zil"),
            "import leaf\nimport sub.util\nfn main()->i32{return leaf.answer()+util.answer();}",
        )
        .unwrap();
        fs::write(path.join("leaf.zil"), "pub fn answer()->i32=>19").unwrap();
        fs::write(path.join("sub/util.zil"), "pub fn answer()->i32=>23").unwrap();
        Self(path)
    }
    fn options(&self) -> CompileOptions {
        CompileOptions {
            semantic_cache: Some(self.0.join("cache")),
            ..Default::default()
        }
    }
    fn compile(&self, options: &CompileOptions) -> CompileReport {
        CompilationSession::default().compile_path(&self.0.join("main.zil"), options)
    }
    fn receipt(&self) -> PathBuf {
        let files = fs::read_dir(self.0.join("cache/discovery-v1"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1);
        files.into_iter().next().unwrap()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    assert_eq!(report.source_files.len(), 3);
}
fn same_source_result(a: &CompileReport, b: &CompileReport) {
    assert_eq!(a.status, b.status);
    assert_eq!(a.phase, b.phase);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.scalar_interfaces, b.scalar_interfaces);
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.source_files, b.source_files);
}

#[test]
fn current_multifile_reads_and_edits_keep_exact_source_results_and_verify() {
    let dir = Dir::new();
    let options = dir.options();
    let cold = dir.compile(&options);
    success(&cold);
    let receipt = dir.receipt();
    let old = fs::read(&receipt).unwrap();
    let warm = dir.compile(&options);
    success(&warm);
    assert_eq!(warm.disk_cache, DiskCacheStatus::Hit);
    same_source_result(&cold, &warm);
    fs::write(dir.0.join("sub/util.zil"), "pub fn answer()->i32=>24").unwrap();
    let edited = dir.compile(&options);
    success(&edited);
    assert_ne!(edited.input_fingerprint, cold.input_fingerprint);
    assert_ne!(fs::read(&receipt).unwrap(), old);
    let mut fresh_options = options.clone();
    fresh_options.semantic_cache = None;
    same_source_result(&edited, &dir.compile(&fresh_options));
    let edited_warm = dir.compile(&options);
    success(&edited_warm);
    assert_eq!(edited_warm.disk_cache, DiskCacheStatus::Hit);
    same_source_result(&edited, &edited_warm);
}

#[test]
fn old_graph_never_hides_new_ambiguity_missing_entries_or_symlink_escape() {
    let dir = Dir::new();
    let options = dir.options();
    success(&dir.compile(&options));
    let receipt = dir.receipt();
    let original = fs::read(&receipt).unwrap();
    fs::create_dir(dir.0.join("leaf")).unwrap();
    fs::write(dir.0.join("leaf/mod.zil"), "pub fn answer()->i32=>19").unwrap();
    let ambiguous = dir.compile(&options);
    assert!(
        ambiguous
            .diagnostics
            .iter()
            .any(|error| error.code == "E-MODULE-AMBIGUOUS")
    );
    fs::remove_dir_all(dir.0.join("leaf")).unwrap();
    fs::remove_file(dir.0.join("leaf.zil")).unwrap();
    let missing = dir.compile(&options);
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|error| error.code == "E-MODULE-NOT-FOUND")
    );
    let outside = Dir::new();
    symlink(outside.0.join("leaf.zil"), dir.0.join("leaf.zil")).unwrap();
    let escape = dir.compile(&options);
    assert!(
        escape
            .diagnostics
            .iter()
            .any(|error| error.code == "E-MODULE-PATH")
    );
    for actual in [&ambiguous, &missing, &escape] {
        assert_eq!(actual.verification_runs, 0);
    }
    let mut fresh = options.clone();
    fresh.semantic_cache = None;
    same_source_result(&escape, &dir.compile(&fresh));
    assert_eq!(
        fs::read(&receipt).unwrap(),
        original,
        "failed requests never update receipts"
    );
}

#[test]
fn corrupted_receipts_low_limits_and_cancelled_requests_use_original_boundaries() {
    let dir = Dir::new();
    let options = dir.options();
    success(&dir.compile(&options));
    let receipt = dir.receipt();
    let original = fs::read(&receipt).unwrap();
    fs::write(&receipt, b"truncated").unwrap();
    success(&dir.compile(&options));
    assert_eq!(fs::read(&receipt).unwrap(), original);
    let mut low = options.clone();
    low.limits.max_tokens = 8;
    let actual = dir.compile(&low);
    low.semantic_cache = None;
    same_source_result(&actual, &dir.compile(&low));
    assert_eq!(actual.status, ResultCategory::ResourceLimit);
    let cancelled = dir.options();
    cancelled.cancellation.cancel();
    assert_eq!(dir.compile(&cancelled).status, ResultCategory::Cancelled);
    assert_eq!(fs::read(&receipt).unwrap(), original);
}

#[test]
fn receipt_publication_cannot_overwrite_a_dependency_alias() {
    let dir = Dir::new();
    let options = dir.options();
    success(&dir.compile(&options));
    let receipt = dir.receipt();
    fs::remove_file(&receipt).unwrap();
    let dependency = dir.0.join("leaf.zil");
    let source = fs::read(&dependency).unwrap();
    fs::hard_link(&dependency, &receipt).unwrap();
    success(&dir.compile(&options));
    assert_eq!(fs::read(&dependency).unwrap(), source);
    assert_eq!(fs::read(&receipt).unwrap(), source);
}
