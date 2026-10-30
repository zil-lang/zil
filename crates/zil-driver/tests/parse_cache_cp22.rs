#![cfg(target_os = "linux")]
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{Phase, ResultCategory};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, QueryDisposition, Stage, compile_path,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const MAIN: &str = "import a\nimport b\nfn main()->i32=>a.answer()+b.zero()-42\n";
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-cp22-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let case = Self(path);
        case.put("main.zil", MAIN);
        case.put("a.zil", "pub fn answer()->i32=>42\n");
        case.put("b.zil", "pub fn zero()->i32=>0\n");
        case
    }
    fn put(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn path(&self) -> PathBuf {
        self.0.join("main.zil")
    }
    fn options(&self) -> CompileOptions {
        CompileOptions {
            query_cache: Some(self.0.join("cache")),
            ..CompileOptions::default()
        }
    }
    fn run(&self, options: &CompileOptions) -> CompileReport {
        compile_path(&self.path(), options)
    }
    fn entries(&self) -> Vec<PathBuf> {
        fs::read_dir(self.0.join("cache/parse-cp22-v1"))
            .into_iter()
            .flatten()
            .map(|e| e.unwrap().path())
            .collect()
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        } else {
            eprintln!("CP22 failure retained: {}", self.0.display());
        }
    }
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn equivalent(warm: &CompileReport, cold: &CompileReport) {
    assert_eq!(
        warm.status,
        cold.status,
        "{}\n{}",
        warm.to_json(),
        cold.to_json()
    );
    assert_eq!(warm.phase, cold.phase);
    assert_eq!(warm.diagnostics, cold.diagnostics);
    assert_eq!(warm.source_files, cold.source_files);
    assert_eq!(warm.verification_runs, cold.verification_runs);
}
#[test]
fn separate_sessions_reuse_unchanged_modules_after_early_token_count_edit() {
    let case = Case::new();
    let options = case.options();
    let first = case.run(&options);
    success(&first);
    assert_eq!(
        (
            first.parse_cache.hits,
            first.parse_cache.parsed,
            first.parse_cache.stored
        ),
        (0, 3, 3),
        "{}",
        first.to_json()
    );
    let warm = case.run(&options);
    success(&warm);
    assert_eq!(
        (
            warm.parse_cache.hits,
            warm.parse_cache.parsed,
            warm.parse_cache.stored
        ),
        (3, 0, 0)
    );
    case.put("main.zil", &format!("// additional input token\n{MAIN}"));
    let edit = case.run(&options);
    success(&edit);
    assert_eq!(
        (
            edit.parse_cache.hits,
            edit.parse_cache.parsed,
            edit.parse_cache.stored
        ),
        (2, 1, 1),
        "{}",
        edit.to_json()
    );
    let mut cold = options.clone();
    cold.query_cache = None;
    equivalent(&edit, &case.run(&cold));
    assert!(
        edit.query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
    );
}
#[test]
fn session_parse_keys_do_not_depend_on_preceding_module_consumption() {
    let case = Case::new();
    let options = CompileOptions::default();
    let mut session = CompilationSession::with_capacity(128);
    success(&session.compile_path(&case.path(), &options));
    case.put("main.zil", &format!("// another token\n{MAIN}"));
    let second = session.compile_path(&case.path(), &options);
    success(&second);
    assert_eq!(second.parse_cache.session_hits, 2);
    assert_eq!(second.parse_cache.parsed, 1);
    assert_eq!(
        second
            .query_events
            .iter()
            .filter(|e| e.phase == Phase::Parse && e.disposition == QueryDisposition::Reused)
            .count(),
        2
    );
}
#[test]
fn quota_depth_and_source_byte_failures_match_uncached_first_diagnostics() {
    let case = Case::new();
    let options = case.options();
    success(&case.run(&options));
    let mut quota_misses = 0;
    for axis in 0..4 {
        for value in [0, 1, 3, 8, 16, 24, 32, 48, 64, 96, 128, 256] {
            let mut warm = options.clone();
            match axis {
                0 => warm.limits.max_tokens = value,
                1 => warm.limits.max_nodes = value,
                2 => warm.limits.max_call_depth = value,
                _ => warm.limits.max_source_bytes = value,
            };
            let mut cold = warm.clone();
            cold.query_cache = None;
            let a = case.run(&warm);
            let b = case.run(&cold);
            equivalent(&a, &b);
            quota_misses += a.parse_cache.quota_misses;
        }
    }
    assert!(quota_misses > 0);
}
#[test]
fn module_reordering_missing_files_and_ambiguous_paths_use_current_discovery() {
    let case = Case::new();
    let options = case.options();
    success(&case.run(&options));
    case.put(
        "main.zil",
        "import b\nimport a\nfn main()->i32=>a.answer()+b.zero()-42\n",
    );
    let reordered = case.run(&options);
    success(&reordered);
    assert_eq!(reordered.parse_cache.hits, 0);
    assert_eq!(reordered.parse_cache.parsed, 3);
    let mut cold = options.clone();
    cold.query_cache = None;
    equivalent(&reordered, &case.run(&cold));
    assert_eq!(case.run(&options).parse_cache.hits, 3);
    fs::remove_file(case.0.join("a.zil")).unwrap();
    equivalent(&case.run(&options), &case.run(&cold));
    case.put("a.zil", "pub fn answer()->i32=>42\n");
    case.put("a/mod.zil", "pub fn answer()->i32=>42\n");
    let ambiguous = case.run(&options);
    equivalent(&ambiguous, &case.run(&cold));
    assert!(
        ambiguous
            .diagnostics
            .iter()
            .any(|d| d.code == "E-MODULE-AMBIGUOUS")
    );
}
#[test]
fn damaged_entries_fall_back_and_repair_without_skipping_verify() {
    let case = Case::new();
    let options = case.options();
    success(&case.run(&options));
    for path in case.entries() {
        let mut bytes = fs::read(&path).unwrap();
        let end = bytes.len() - 1;
        bytes[end] ^= 1;
        fs::write(path, bytes).unwrap();
    }
    let repaired = case.run(&options);
    success(&repaired);
    assert_eq!(
        (
            repaired.parse_cache.corrupt,
            repaired.parse_cache.parsed,
            repaired.parse_cache.stored
        ),
        (3, 3, 3)
    );
    assert_eq!(case.run(&options).parse_cache.hits, 3);
    let path = case.entries().pop().unwrap();
    fs::write(&path, b"truncated").unwrap();
    let repaired = case.run(&options);
    success(&repaired);
    assert_eq!(repaired.parse_cache.corrupt, 1);
}
#[test]
fn failed_cancelled_or_invalid_artifact_requests_do_not_create_cache_directories() {
    let case = Case::new();
    let options = case.options();
    case.put("a.zil", "pub fn answer( ->");
    assert_ne!(case.run(&options).status, ResultCategory::Success);
    assert!(!case.0.join("cache").exists());
    case.put("a.zil", "pub fn answer()->i32=>true\n");
    assert_ne!(case.run(&options).status, ResultCategory::Success);
    assert!(!case.0.join("cache").exists());
    case.put("a.zil", "pub fn answer()->i32=>42\n");
    let cancelled = options.clone();
    cancelled.cancellation.cancel();
    assert_eq!(case.run(&cancelled).status, ResultCategory::Cancelled);
    assert!(!case.0.join("cache").exists());
    let mut invalid = case.options();
    invalid.stop_after = Stage::Build;
    invalid.output = Some(case.path());
    let result = case.run(&invalid);
    assert_ne!(result.status, ResultCategory::Success);
    assert!(!case.0.join("cache").exists());
    success(&case.run(&case.options()));
}
