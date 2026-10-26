//! Public whole-request controls for the private raw syntax/identity channel.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Fingerprint, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, ProjectSources, QueryDisposition, Stage,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    keep: bool,
}
impl Fixture {
    fn new() -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR").is_some();
        let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let root = base.join(format!(
            "zil-vertical-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("math")).unwrap();
        fs::write(root.join("main.zil"), "import math.arithmetic as a\nimport other\nimport math.arithmetic as again\nfn main() -> i32 => a.answer() + other.answer() + again.answer()\n").unwrap();
        fs::write(
            root.join("math/arithmetic.zil"),
            "fn hidden() -> i32 => 20\npub fn answer() -> i32 => hidden()\n",
        )
        .unwrap();
        fs::write(root.join("other.zil"), "pub fn answer() -> i32 => 2\n").unwrap();
        Self { root, keep }
    }
    fn project(&self) -> ProjectSources {
        ProjectSources {
            root: ModuleId(0),
            modules: ["main.zil", "math/arithmetic.zil", "other.zil"]
                .into_iter()
                .enumerate()
                .map(|(i, path)| {
                    let path = fs::canonicalize(self.root.join(path)).unwrap();
                    SourceUnit::new(
                        FileId(i as u32),
                        ModuleId(i as u32),
                        path.to_string_lossy(),
                        fs::read_to_string(&path).unwrap(),
                    )
                })
                .collect(),
            paths: BTreeMap::from([
                (vec!["main".into()], ModuleId(0)),
                (vec!["math".into(), "arithmetic".into()], ModuleId(1)),
                (vec!["other".into()], ModuleId(2)),
            ]),
        }
    }
    fn report(&self, name: &str, report: &CompileReport) {
        fs::write(self.root.join(format!("{name}.json")), report.to_json()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.root);
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
}
fn events(report: &CompileReport) -> Vec<(Phase, QueryDisposition)> {
    report
        .query_events
        .iter()
        .map(|e| (e.phase, e.disposition))
        .collect()
}

#[test]
fn path_discovery_and_explicit_sources_have_identical_stages_and_verified_interfaces() {
    let f = Fixture::new();
    let project = f.project();
    let mut session = CompilationSession::default();
    let cold = session.compile_path(
        &f.root.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(f.root.join("cold")),
            ..CompileOptions::default()
        },
    );
    let supplied = CompilationSession::default().compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(f.root.join("supplied")),
            ..CompileOptions::default()
        },
    );
    let hot = session.compile_path(
        &f.root.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(f.root.join("hot")),
            ..CompileOptions::default()
        },
    );
    for (name, r) in [("cold", &cold), ("supplied", &supplied), ("hot", &hot)] {
        f.report(name, r);
        success(r);
        assert_eq!(r.verification_runs, 1);
    }
    assert_eq!(cold.input_fingerprint, supplied.input_fingerprint);
    assert_eq!(cold.input_fingerprint, hot.input_fingerprint);
    assert_eq!(cold.scalar_interfaces, supplied.scalar_interfaces);
    assert_eq!(cold.scalar_interfaces, hot.scalar_interfaces);
    assert_eq!(events(&cold), events(&supplied));
    for name in [
        "syntax",
        "core",
        "source-map",
        "ct-core",
        "typed",
        "interfaces",
        "candidate",
        "verified-interfaces",
        "verified",
        "lir",
    ] {
        let expected = fs::read(&supplied.artifacts[name]).unwrap();
        assert_eq!(
            fs::read(&cold.artifacts[name]).unwrap(),
            expected,
            "cold {name}"
        );
        assert_eq!(
            fs::read(&hot.artifacts[name]).unwrap(),
            expected,
            "hot {name}"
        );
    }
    assert_eq!(
        events(&hot)
            .iter()
            .filter(|(_, disposition)| *disposition == QueryDisposition::Reused)
            .count(),
        7
    );
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn cumulative_source_and_frontend_quotas_still_reject_before_verify() {
    let f = Fixture::new();
    let p = f.project();
    let total_bytes = p.modules.iter().map(|s| s.text.len()).sum::<usize>();
    let total_tokens = p
        .modules
        .iter()
        .map(|s| zil_syntax::parse(s).tokens.len())
        .sum::<usize>();
    for (label, field, limit) in [
        ("bytes", 0, total_bytes - 1),
        ("tokens", 1, total_tokens - 1),
        ("nodes", 2, total_tokens - 1),
        ("depth", 3, 1),
    ] {
        let mut options = CompileOptions::default();
        match field {
            0 => options.limits.max_source_bytes = limit,
            1 => options.limits.max_tokens = limit,
            2 => options.limits.max_nodes = limit,
            _ => options.limits.max_call_depth = limit,
        }
        let mut path_session = CompilationSession::default();
        let path = path_session.compile_path(&f.root.join("main.zil"), &options);
        let memory = CompilationSession::default().compile_project(&p, &options);
        f.report(&format!("quota-{label}-path"), &path);
        f.report(&format!("quota-{label}-memory"), &memory);
        assert_eq!(
            path.status,
            ResultCategory::ResourceLimit,
            "{}",
            path.to_json()
        );
        assert_eq!(
            memory.status,
            ResultCategory::ResourceLimit,
            "{}",
            memory.to_json()
        );
        assert_eq!(path.verification_runs, 0);
        assert_eq!(memory.verification_runs, 0);
        assert_eq!(path_session.active_query_count(), 0);
    }
    let mut options = CompileOptions {
        stop_after: Stage::Core,
        ..CompileOptions::default()
    };
    options.limits.max_source_bytes = total_bytes;
    options.limits.max_tokens = total_tokens;
    success(&CompilationSession::default().compile_path(&f.root.join("main.zil"), &options));
    success(&CompilationSession::default().compile_project(&p, &options));
}

#[test]
fn syntax_only_stop_does_not_discover_missing_imports_or_publish_failed_loads() {
    let f = Fixture::new();
    fs::write(
        f.root.join("main.zil"),
        "import missing\nfn main() -> i32 => 0\n",
    )
    .unwrap();
    let mut session = CompilationSession::default();
    let parse = session.compile_path(
        &f.root.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Parse,
            ..CompileOptions::default()
        },
    );
    success(&parse);
    assert_eq!(parse.query_events.len(), 1);
    let before = session.cached_query_count();
    let full = session.compile_path(&f.root.join("main.zil"), &CompileOptions::default());
    f.report("missing-import", &full);
    assert_eq!(full.status, ResultCategory::SafeReject);
    assert_eq!(full.diagnostics[0].code, "E-MODULE-NOT-FOUND");
    assert!(full.query_events.is_empty());
    assert_eq!(session.cached_query_count(), before);
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn artifact_collision_and_cancelled_request_never_publish_seeded_syntax() {
    let f = Fixture::new();
    let original = fs::read(f.root.join("math/arithmetic.zil")).unwrap();
    let mut session = CompilationSession::default();
    let collision = session.compile_path(
        &f.root.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(f.root.join("math/arithmetic.zil")),
            ..CompileOptions::default()
        },
    );
    f.report("collision", &collision);
    assert_ne!(collision.status, ResultCategory::Success);
    assert!(collision.query_events.is_empty());
    assert_eq!(session.cached_query_count(), 0);
    assert_eq!(
        fs::read(f.root.join("math/arithmetic.zil")).unwrap(),
        original
    );
    let options = CompileOptions::default();
    options.cancellation.cancel();
    let cancelled = session.compile_path(&f.root.join("main.zil"), &options);
    f.report("cancelled", &cancelled);
    assert_eq!(cancelled.status, ResultCategory::Cancelled);
    assert!(cancelled.query_events.is_empty());
    assert_eq!(session.cached_query_count(), 0);
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn private_edit_and_request_identity_changes_do_not_reuse_stale_analysis() {
    let f = Fixture::new();
    let mut session = CompilationSession::default();
    let initial = session.compile_path(&f.root.join("main.zil"), &CompileOptions::default());
    success(&initial);
    fs::write(
        f.root.join("math/arithmetic.zil"),
        "fn hidden() -> i32 => 21\npub fn answer() -> i32 => hidden()\n",
    )
    .unwrap();
    let changed = session.compile_path(&f.root.join("main.zil"), &CompileOptions::default());
    success(&changed);
    f.report("private-edit", &changed);
    assert_ne!(initial.input_fingerprint, changed.input_fingerprint);
    assert_eq!(
        initial.scalar_interfaces[1].signature_fingerprint,
        changed.scalar_interfaces[1].signature_fingerprint
    );
    assert!(events(&changed).contains(&(Phase::Analyze, QueryDisposition::Executed)));
    for (name, options) in [
        (
            "build-input",
            CompileOptions {
                build_inputs: Fingerprint::of(b"another dependency lock"),
                ..CompileOptions::default()
            },
        ),
        ("target", {
            let mut o = CompileOptions::default();
            o.target.id = "explicit-other-target".into();
            o
        }),
        ("limits", {
            let mut o = CompileOptions::default();
            o.limits.max_analysis_steps -= 1;
            o
        }),
    ] {
        let report = session.compile_path(&f.root.join("main.zil"), &options);
        f.report(name, &report);
        success(&report);
        assert_eq!(report.verification_runs, 1);
        assert!(events(&report).contains(&(Phase::Analyze, QueryDisposition::Executed)));
    }
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn existing_read_only_source_cache_is_read_without_replacing_independent_verification() {
    let f = Fixture::new();
    let options = CompileOptions {
        cache_dir: Some(f.root.join("cache")),
        ..CompileOptions::default()
    };
    let first = CompilationSession::default().compile_path(&f.root.join("main.zil"), &options);
    f.report("disk-first", &first);
    success(&first);
    let mut files = Vec::new();
    let mut pending = vec![options.cache_dir.clone().unwrap()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let metadata = fs::metadata(&path).unwrap();
                let original_permissions = metadata.permissions();
                let mut permissions = original_permissions.clone();
                permissions.set_readonly(true);
                fs::set_permissions(&path, permissions).unwrap();
                files.push((path, metadata.modified().unwrap(), original_permissions));
            }
        }
    }
    assert_eq!(files.len(), 3);
    let second = CompilationSession::default().compile_path(&f.root.join("main.zil"), &options);
    f.report("disk-hit-fresh-session", &second);
    success(&second);
    assert_eq!(second.disk_cache, zil_driver::DiskCacheStatus::Hit);
    assert_eq!(second.verification_runs, 1);
    assert_eq!(second.scalar_interfaces, first.scalar_interfaces);
    assert_eq!(events(&second), events(&first));
    assert_eq!(
        second
            .query_events
            .iter()
            .filter(|e| e.disposition == QueryDisposition::Reused)
            .count(),
        0
    );
    for (path, modified, permissions) in files {
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        fs::set_permissions(path, permissions).unwrap();
    }
}
