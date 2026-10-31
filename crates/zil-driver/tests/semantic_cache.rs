//! Trusted-local frontend candidates remain raw; every successful request
//! independently analyzes and verifies the current input before native work.
#![cfg(all(feature = "llvm-api", target_os = "linux"))]
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Fingerprint, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CancellationToken, CompilationSession, CompileOptions, CompileReport, DiskCacheStatus,
    NativeBackend, ProjectSources, QueryDisposition, Stage,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil-semantic-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        let d = Self(p);
        d.write(
            "main.zil",
            "import a\nimport b\nfn value()->i32{return a.get()+b.get();}",
        );
        d.write("a.zil", "pub fn get()->i32{return 19;}");
        d.write("b.zil", "pub fn get()->i32{return 23;}");
        d
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }
    fn options(&self) -> CompileOptions {
        CompileOptions {
            semantic_cache: Some(self.0.join("cache")),
            ..Default::default()
        }
    }
    fn project(&self) -> ProjectSources {
        let modules = ["main.zil", "a.zil", "b.zil"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                let path = self.0.join(name);
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy(),
                    fs::read_to_string(&path).unwrap(),
                )
            })
            .collect();
        ProjectSources {
            root: ModuleId(0),
            modules,
            paths: BTreeMap::from([
                (vec!["a".into()], ModuleId(1)),
                (vec!["b".into()], ModuleId(2)),
            ]),
        }
    }
    fn compile(&self, options: &CompileOptions) -> CompileReport {
        CompilationSession::default().compile_project(&self.project(), options)
    }
    fn entries(&self) -> Vec<PathBuf> {
        let mut files = fs::read_dir(self.0.join("cache/semantic-v1"))
            .into_iter()
            .flatten()
            .map(|p| p.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "candidate"))
            .collect::<Vec<_>>();
        files.sort();
        files
    }
    fn build_options(&self) -> CompileOptions {
        CompileOptions {
            stop_after: Stage::Build,
            backend: NativeBackend::LlvmApi,
            entry: Some("value".into()),
            output: Some(self.0.join("program")),
            ..self.options()
        }
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
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    assert_eq!(
        report
            .query_events
            .iter()
            .filter(|e| e.phase == Phase::Verify)
            .count(),
        1
    );
}
fn frontend_hit(report: &CompileReport) {
    success(report);
    assert_eq!(report.disk_cache, DiskCacheStatus::Hit);
    assert!(
        report
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Reused)
    );
    for phase in [Phase::Expand, Phase::Ct] {
        assert!(
            !report.query_events.iter().any(|e| e.phase == phase),
            "{}",
            report.to_json()
        );
    }
    assert!(
        report
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Analyze && e.disposition == QueryDisposition::Executed)
    );
}
fn equivalent(a: &CompileReport, b: &CompileReport) {
    assert_eq!(a.status, b.status);
    assert_eq!(a.phase, b.phase);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.scalar_interfaces, b.scalar_interfaces);
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.verification_runs, b.verification_runs);
}
fn native(report: &CompileReport, value: i32) {
    let out = Command::new(&report.artifacts["executable"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(out.stdout, format!("{value}\n").as_bytes());
    assert!(out.stderr.is_empty());
}

#[test]
fn fresh_sessions_reuse_raw_frontend_and_keep_actual_observer_events_and_native_value() {
    let d = Dir::new();
    let options = d.build_options();
    let cold = d.compile(&options);
    success(&cold);
    native(&cold, 42);
    assert_eq!(cold.disk_cache, DiskCacheStatus::Missing);
    assert_eq!(d.entries().len(), 1);
    let mut session = CompilationSession::default();
    let mut events = Vec::new();
    let warm = session
        .compile_project_with_observer(&d.project(), &options, &mut |e| events.push(e.clone()));
    frontend_hit(&warm);
    native(&warm, 42);
    assert_eq!(events, warm.query_events);
    let mut fresh_options = options.clone();
    fresh_options.semantic_cache = None;
    let fresh = d.compile(&fresh_options);
    equivalent(&warm, &fresh);
    native(&fresh, 42);
}

#[test]
fn each_limit_build_inputs_target_and_source_have_separate_frontend_identity() {
    let d = Dir::new();
    let options = d.options();
    success(&d.compile(&options));
    for index in 0..13 {
        let mut changed = options.clone();
        match index {
            0 => changed.limits.max_source_bytes -= 1,
            1 => changed.limits.max_tokens -= 1,
            2 => changed.limits.max_nodes -= 1,
            3 => changed.limits.max_functions -= 1,
            4 => changed.limits.max_blocks -= 1,
            5 => changed.limits.max_instructions -= 1,
            6 => changed.limits.max_locals -= 1,
            7 => changed.limits.max_values -= 1,
            8 => changed.limits.max_analysis_steps -= 1,
            9 => changed.limits.max_ct_steps -= 1,
            10 => changed.limits.max_call_depth -= 1,
            11 => changed.build_inputs = Fingerprint::of(b"changed build input"),
            12 => changed.target = TargetProfile::windows_x64_llvm(),
            _ => unreachable!(),
        }
        let first = d.compile(&changed);
        success(&first);
        assert_eq!(
            first.disk_cache,
            DiskCacheStatus::Missing,
            "case {index}: {}",
            first.to_json()
        );
        frontend_hit(&d.compile(&changed));
    }
    assert_eq!(d.entries().len(), 14);
    d.write("a.zil", "pub fn get()->i32{return 20;}");
    let edited = d.compile(&options);
    success(&edited);
    assert_eq!(edited.disk_cache, DiskCacheStatus::Missing);
    frontend_hit(&d.compile(&options));
    assert_eq!(d.entries().len(), 15);
}

#[test]
fn truncated_corrupt_and_missing_candidates_rebuild_from_current_source() {
    let d = Dir::new();
    let options = d.options();
    let original = d.compile(&options);
    success(&original);
    let path = d.entries().pop().unwrap();
    let bytes = fs::read(&path).unwrap();
    for malformed in [b"truncated".to_vec(), {
        let mut b = bytes.clone();
        *b.last_mut().unwrap() ^= 1;
        b
    }] {
        fs::write(&path, malformed).unwrap();
        let rebuilt = d.compile(&options);
        success(&rebuilt);
        equivalent(&rebuilt, &original);
        assert_eq!(rebuilt.disk_cache, DiskCacheStatus::Rebuilt);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        frontend_hit(&d.compile(&options));
    }
    fs::remove_file(path).unwrap();
    assert_eq!(d.compile(&options).disk_cache, DiskCacheStatus::Missing);
}

#[test]
fn errors_and_cancellation_do_not_publish_and_observers_match_actual_attempts() {
    let d = Dir::new();
    let options = d.options();
    d.write("a.zil", "pub fn get()->i32{return true;}");
    let error = d.compile(&options);
    assert_eq!(error.status, ResultCategory::SafeReject);
    assert!(d.entries().is_empty());
    let mut fresh = options.clone();
    fresh.semantic_cache = None;
    equivalent(&error, &d.compile(&fresh));
    d.write("a.zil", "pub fn get()->i32{return 19;}");
    for phase in [Phase::Types, Phase::Analyze, Phase::Verify] {
        let mut cancelled = options.clone();
        cancelled.cancellation = CancellationToken::default();
        let mut events = Vec::new();
        let mut session = CompilationSession::default();
        let report = session.compile_project_with_observer(&d.project(), &cancelled, &mut |e| {
            events.push(e.clone());
            if e.phase == phase {
                cancelled.cancellation.cancel();
            }
        });
        assert_eq!(report.status, ResultCategory::Cancelled);
        assert_eq!(events, report.query_events);
        assert_eq!(session.active_query_count(), 0);
        assert!(d.entries().is_empty());
    }
    success(&d.compile(&options));
    let path = d.entries().pop().unwrap();
    let prior = fs::read(&path).unwrap();
    let cancelled = d.options();
    let mut events = Vec::new();
    let report = CompilationSession::default().compile_project_with_observer(
        &d.project(),
        &cancelled,
        &mut |e| {
            events.push(e.clone());
            if e.phase == Phase::Analyze {
                cancelled.cancellation.cancel();
            }
        },
    );
    assert_eq!(report.status, ResultCategory::Cancelled);
    assert_eq!(events, report.query_events);
    assert_eq!(report.verification_runs, 0);
    assert_eq!(fs::read(path).unwrap(), prior);
}

#[test]
fn cached_frontend_link_failure_runs_once_and_preserves_previous_executable() {
    let d = Dir::new();
    let mut options = d.build_options();
    let first = d.compile(&options);
    success(&first);
    native(&first, 42);
    let executable = options.output.as_ref().unwrap();
    let prior = fs::read(executable).unwrap();
    let count = d.0.join("link-calls");
    let fake = d.0.join("fail-link");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\nprintf 'call\\n' >> '{}'\nexit 7\n",
            count.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    options.clang = Some(fake);
    let mut events = Vec::new();
    let failed = CompilationSession::default().compile_project_with_observer(
        &d.project(),
        &options,
        &mut |e| events.push(e.clone()),
    );
    assert_eq!(failed.disk_cache, DiskCacheStatus::Hit);
    assert_ne!(failed.status, ResultCategory::Success);
    assert_eq!(failed.verification_runs, 1);
    assert_eq!(events, failed.query_events);
    assert_eq!(fs::read_to_string(count).unwrap(), "call\n");
    assert_eq!(fs::read(executable).unwrap(), prior);
    assert_eq!(d.entries().len(), 1);
}

#[test]
fn source_and_output_aliases_are_rejected_before_cache_load_or_publication() {
    for hard in [false, true] {
        for output_alias in [false, true] {
            let d = Dir::new();
            let options = if output_alias {
                d.build_options()
            } else {
                d.options()
            };
            success(&d.compile(&options));
            let candidate = d.entries().pop().unwrap();
            let protected = if output_alias {
                d.0.join("program")
            } else {
                d.0.join("a.zil")
            };
            let bytes = fs::read(&protected).unwrap();
            fs::remove_file(&candidate).unwrap();
            if hard {
                fs::hard_link(&protected, &candidate).unwrap();
            } else {
                symlink(&protected, &candidate).unwrap();
            }
            let report = d.compile(&options);
            assert_eq!(
                report.status,
                ResultCategory::InfraError,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 0);
            assert_eq!(
                report.diagnostics[0].code,
                if output_alias {
                    "E-OUTPUT-COLLISION"
                } else {
                    "E-OUTPUT-SOURCE"
                }
            );
            assert_eq!(fs::read(&protected).unwrap(), bytes);
        }
    }
}

#[test]
fn unsupported_cache_modes_and_writable_store_keep_current_source_path() {
    let d = Dir::new();
    for mode in 0..3 {
        let mut options = d.options();
        match mode {
            0 => options.stop_after = Stage::Analyze,
            1 => options.emit_dir = Some(d.0.join("emitted")),
            2 => options.cache_dir = Some(d.0.join("source-cache")),
            _ => unreachable!(),
        }
        let report = d.compile(&options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .query_events
                .iter()
                .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
        );
        assert!(d.entries().is_empty());
    }
    let options = d.options();
    success(&d.compile(&options));
    fs::set_permissions(
        d.0.join("cache/semantic-v1"),
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    let report = d.compile(&options);
    success(&report);
    assert_eq!(report.disk_cache, DiskCacheStatus::Rebuilt);
    assert!(
        report
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
    );
}
