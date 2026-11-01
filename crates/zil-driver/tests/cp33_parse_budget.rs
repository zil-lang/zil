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
const MAIN: &str = "import a\nimport b\nfn main()->i32 { return a.adjust(b.value()); }\n";
const A: &str = "pub fn adjust(x:i32)->i32 { return x+1; }\n";
const B: &str = "import c\npub fn value()->i32 { return c.value(); }\n";
const C: &str = "pub fn value()->i32 { return 2; }\n";
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-cp33-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let case = Self(path);
        for (name, text) in [("main.zil", MAIN), ("a.zil", A), ("b.zil", B), ("c.zil", C)] {
            case.put(name, text);
        }
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
    fn project(&self) -> ProjectSources {
        let mut paths = BTreeMap::new();
        let modules = ["main", "a", "b", "c"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                let id = i as u32;
                paths.insert(vec![name.into()], ModuleId(id));
                let path = self.0.join(format!("{name}.zil"));
                SourceUnit::new(
                    FileId(id),
                    ModuleId(id),
                    path.to_string_lossy(),
                    fs::read_to_string(&path).unwrap(),
                )
            })
            .collect();
        ProjectSources {
            root: ModuleId(0),
            modules,
            paths,
        }
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        } else {
            eprintln!("CP33 failed fixture retained: {}", self.0.display());
        }
    }
}
fn options() -> CompileOptions {
    CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
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
fn equivalent(warm: &CompileReport, fresh: &CompileReport) {
    assert_eq!(
        warm.status,
        fresh.status,
        "{}\n{}",
        warm.to_json(),
        fresh.to_json()
    );
    assert_eq!(warm.phase, fresh.phase);
    assert_eq!(warm.diagnostics, fresh.diagnostics);
    assert_eq!(warm.source_files, fresh.source_files);
    assert_eq!(warm.input_fingerprint, fresh.input_fingerprint);
    assert_eq!(warm.scalar_interfaces, fresh.scalar_interfaces);
    assert_eq!(warm.verification_runs, fresh.verification_runs);
}
fn parse_counts(report: &CompileReport) -> (usize, usize) {
    let events = report
        .query_events
        .iter()
        .filter(|e| e.phase == Phase::Parse);
    (
        events
            .clone()
            .filter(|e| e.disposition == QueryDisposition::Executed)
            .count(),
        events
            .filter(|e| e.disposition == QueryDisposition::Reused)
            .count(),
    )
}
fn fresh(case: &Case, options: &CompileOptions) -> CompileReport {
    CompilationSession::default().compile_path(&case.path(), options)
}

#[test]
fn preceding_trivia_and_public_signature_edits_only_parse_changed_files() {
    let case = Case::new();
    let options = options();
    let mut session = CompilationSession::with_capacity(128);
    let initial = session.compile_path(&case.path(), &options);
    success(&initial);
    assert_eq!(parse_counts(&initial), (4, 0));
    case.put(
        "main.zil",
        &format!("// additional consumed trivia\n{MAIN}"),
    );
    let comment = session.compile_path(&case.path(), &options);
    success(&comment);
    equivalent(&comment, &fresh(&case, &options));
    assert_eq!(parse_counts(&comment), (1, 3));
    case.put(
        "main.zil",
        "import a\nimport b\nfn main()->i32 { return a.adjust(b.value(),3); }\n",
    );
    case.put(
        "a.zil",
        "pub fn adjust(x:i32,extra:i32)->i32 { return x+extra; }\n",
    );
    let public = session.compile_path(&case.path(), &options);
    success(&public);
    equivalent(&public, &fresh(&case, &options));
    assert_eq!(parse_counts(&public), (2, 2));
    case.put("main.zil", MAIN);
    case.put("a.zil", A);
    let restore = session.compile_path(&case.path(), &options);
    success(&restore);
    equivalent(&restore, &fresh(&case, &options));
    assert_eq!(parse_counts(&restore), (0, 4));
    assert_eq!(session.active_query_count(), 0);
}

#[test]
fn current_project_quota_and_depth_failures_match_fresh_first_diagnostics() {
    let case = Case::new();
    let mut session = CompilationSession::with_capacity(512);
    success(&session.compile_path(&case.path(), &options()));
    let mut failures = 0;
    for axis in 0..4 {
        for value in [0, 1, 3, 8, 16, 24, 32, 48, 64, 96, 128, 256] {
            let mut current = options();
            match axis {
                0 => current.limits.max_tokens = value,
                1 => current.limits.max_nodes = value,
                2 => current.limits.max_call_depth = value,
                _ => current.limits.max_source_bytes = value,
            }
            let warm = session.compile_path(&case.path(), &current);
            equivalent(&warm, &fresh(&case, &current));
            failures += usize::from(warm.status != ResultCategory::Success);
            assert_eq!(session.active_query_count(), 0);
        }
    }
    assert!(failures > 0);
}

#[test]
fn single_source_receipts_fall_back_for_attribute_nodes_and_eof_tokens() {
    let case = Case::new();
    for text in [
        "",
        "#[doc=\"a\"]\n#[doc=\"b\"]\n#[doc=\"c\"]\nfn f(){}",
        "fn f()->i32=>1+2*3",
    ] {
        let source = SourceUnit::new(FileId(9), ModuleId(11), case.path().to_string_lossy(), text);
        let full = zil_syntax::parse(&source);
        assert!(full.is_ok());
        let mut session = CompilationSession::with_capacity(256);
        let base = CompileOptions {
            stop_after: Stage::Parse,
            ..CompileOptions::default()
        };
        assert_eq!(
            session.compile_source(&source, &base).status,
            ResultCategory::Success
        );
        let mut limits = Vec::new();
        for nodes in 0..=full.usage.nodes_required + 1 {
            let mut current = base.clone();
            current.limits.max_nodes = nodes;
            limits.push(current);
        }
        for tokens in [
            0,
            full.tokens.len() - 1,
            full.tokens.len(),
            full.tokens.len() + 1,
        ] {
            let mut current = base.clone();
            current.limits.max_tokens = tokens;
            limits.push(current);
        }
        for current in limits {
            let warm = session.compile_source(&source, &current);
            let fresh = CompilationSession::default().compile_source(&source, &current);
            equivalent(&warm, &fresh);
            assert_eq!(session.active_query_count(), 0);
        }
    }
}

#[test]
fn source_position_context_and_nonreceipt_limits_stay_in_identity() {
    let case = Case::new();
    let source = SourceUnit::new(
        FileId(3),
        ModuleId(5),
        case.path().to_string_lossy(),
        "fn f()->i32=>1\n",
    );
    let options = CompileOptions {
        stop_after: Stage::Parse,
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::with_capacity(128);
    assert_eq!(
        parse_counts(&session.compile_source(&source, &options)),
        (1, 0)
    );
    let mut relaxed = options.clone();
    relaxed.limits.max_tokens += 1;
    relaxed.limits.max_nodes += 1;
    assert_eq!(
        parse_counts(&session.compile_source(&source, &relaxed)),
        (0, 1)
    );
    for axis in 0..7 {
        let mut unit = source.clone();
        let mut current = options.clone();
        match axis {
            0 => unit.file_id = FileId(4),
            1 => unit.module_id = ModuleId(6),
            2 => unit.path.push_str(".other"),
            3 => unit.text.push_str("// changed source\n"),
            4 => current.limits.max_call_depth += 1,
            5 => current.limits.max_source_bytes -= 1,
            _ => current.build_inputs = Fingerprint::of(b"different context"),
        }
        let report = session.compile_source(&unit, &current);
        equivalent(
            &report,
            &CompilationSession::default().compile_source(&unit, &current),
        );
        assert_eq!(parse_counts(&report), (1, 0), "axis {axis}");
    }
}

#[test]
fn current_import_resolution_and_failed_edits_never_use_stale_results() {
    let case = Case::new();
    let options = options();
    let mut session = CompilationSession::with_capacity(128);
    success(&session.compile_path(&case.path(), &options));
    case.put(
        "main.zil",
        "import b\nimport a\nfn main()->i32 { return a.adjust(b.value()); }\n",
    );
    equivalent(
        &session.compile_path(&case.path(), &options),
        &fresh(&case, &options),
    );
    fs::remove_file(case.0.join("c.zil")).unwrap();
    equivalent(
        &session.compile_path(&case.path(), &options),
        &fresh(&case, &options),
    );
    case.put("c.zil", C);
    case.put("c/mod.zil", C);
    let ambiguous = session.compile_path(&case.path(), &options);
    equivalent(&ambiguous, &fresh(&case, &options));
    assert!(
        ambiguous
            .diagnostics
            .iter()
            .any(|d| d.code == "E-MODULE-AMBIGUOUS")
    );
    fs::remove_file(case.0.join("c/mod.zil")).unwrap();
    for bad in [
        "pub fn adjust( ->",
        "pub fn adjust(x:i32)->i32 { return true; }\n",
    ] {
        case.put("a.zil", bad);
        let failure = session.compile_path(&case.path(), &options);
        assert_ne!(failure.status, ResultCategory::Success);
        equivalent(&failure, &fresh(&case, &options));
        assert_eq!(session.active_query_count(), 0);
    }
    case.put("a.zil", A);
    success(&session.compile_path(&case.path(), &options));
}

#[test]
fn cancellation_and_small_capacity_keep_fresh_verification_and_query_cleanup() {
    let case = Case::new();
    let project = case.project();
    let mut session = CompilationSession::with_capacity(8);
    success(&session.compile_project(&project, &options()));
    let cancel_run = |session: &mut CompilationSession| {
        let current = options();
        let cancellation = current.cancellation.clone();
        session.compile_project_with_observer(&project, &current, &mut |event| {
            if event.phase == Phase::Parse {
                cancellation.cancel();
            }
        })
    };
    let cancelled = cancel_run(&mut session);
    equivalent(
        &cancelled,
        &cancel_run(&mut CompilationSession::with_capacity(8)),
    );
    assert_eq!(cancelled.status, ResultCategory::Cancelled);
    assert_eq!(cancelled.verification_runs, 0);
    assert_eq!(session.active_query_count(), 0);
    for prefix in ["// first\n", "// second changed trivia\n", "", "// final\n"] {
        case.put("main.zil", &format!("{prefix}{MAIN}"));
        let report = session.compile_path(&case.path(), &options());
        success(&report);
        equivalent(&report, &fresh(&case, &options()));
        assert_eq!(session.active_query_count(), 0);
    }
}
