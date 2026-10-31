use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Fingerprint, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, ProjectSources, QueryDisposition, Stage,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case {
    dir: PathBuf,
    project: ProjectSources,
}
impl Case {
    fn new(n: usize) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "zil-frontend-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let mut main = (1..n).map(|i| format!("import m{i}\n")).collect::<String>();
        main += "fn main()->i32{return m1.value();}\n";
        let mut paths = BTreeMap::new();
        let modules = (0..n)
            .map(|i| {
                paths.insert(vec![format!("m{i}")], ModuleId(i as u32));
                let path = dir.join(format!("m{i}.zil"));
                let text = if i == 0 {
                    main.clone()
                } else {
                    format!("pub fn value()->i32{{return {i};}}\n")
                };
                fs::write(&path, &text).unwrap();
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy(),
                    text,
                )
            })
            .collect();
        Self {
            dir,
            project: ProjectSources {
                root: ModuleId(0),
                modules,
                paths,
            },
        }
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn successful(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn equivalent(a: &CompileReport, b: &CompileReport) {
    assert_eq!(a.status, b.status, "{}\n{}", a.to_json(), b.to_json());
    assert_eq!(a.phase, b.phase);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.source_files, b.source_files);
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.scalar_interfaces, b.scalar_interfaces);
    assert_eq!(a.verification_runs, b.verification_runs);
}
fn parses(report: &CompileReport, disposition: QueryDisposition) -> usize {
    report
        .query_events
        .iter()
        .filter(|e| e.phase == Phase::Parse && e.disposition == disposition)
        .count()
}
#[test]
fn same_complete_input_reuses_frontend_after_parse_eviction_and_verifies_now() {
    for capacity in [8, 32] {
        let case = Case::new(128);
        let options = CompileOptions::default();
        let mut session = CompilationSession::with_capacity(capacity);
        let cold = session.compile_project(&case.project, &options);
        successful(&cold);
        assert_eq!(parses(&cold, QueryDisposition::Executed), 128);
        for _ in 0..4 {
            let warm = session.compile_project(&case.project, &options);
            successful(&warm);
            equivalent(&warm, &cold);
            assert_eq!(parses(&warm, QueryDisposition::Executed), 0);
            assert_eq!(parses(&warm, QueryDisposition::Reused), 128);
            assert_eq!(
                warm.query_events
                    .iter()
                    .map(|e| e.phase)
                    .collect::<Vec<_>>(),
                cold.query_events
                    .iter()
                    .map(|e| e.phase)
                    .collect::<Vec<_>>()
            );
            assert!(
                warm.query_events.iter().any(
                    |e| e.phase == Phase::Verify && e.disposition == QueryDisposition::Executed
                )
            );
            assert!(session.cached_query_count() <= capacity);
            assert_eq!(session.active_query_count(), 0);
        }
    }
}
#[test]
fn changed_source_identity_rules_limits_and_target_match_fresh() {
    let case = Case::new(40);
    let options = CompileOptions::default();
    for change in 0..17 {
        let mut session = CompilationSession::with_capacity(8);
        successful(&session.compile_project(&case.project, &options));
        let mut project = case.project.clone();
        let mut current = options.clone();
        match change {
            0 => project.modules[1].text = "pub fn value()->i32{return 7;}".into(),
            1 => project.modules[1].text = "pub fn value()->i32{return true;}".into(),
            2 => project.modules[1].text = "pub fn value( {".into(),
            3 => project.modules[1].file_id = FileId(1001),
            4 => project.modules[1].path = case.dir.join("renamed.zil").to_string_lossy().into(),
            5 => {
                project.modules[1].module_id = ModuleId(1001);
                project.paths.insert(vec!["m1".into()], ModuleId(1001));
            }
            6 => project.modules.swap(0, 1),
            7 => {
                project.paths.insert(vec!["extra".into()], ModuleId(1));
            }
            8 => project.root = ModuleId(1),
            9 => current.limits.max_tokens = 0,
            10 => current.limits.max_nodes = 0,
            11 => current.limits.max_source_bytes = 0,
            12 => current.limits.max_functions = 0,
            13 => current.limits.max_analysis_steps = 0,
            14 => current.build_inputs = Fingerprint::of(b"another-build-input"),
            15 => current.target = TargetProfile::windows_x64_llvm(),
            16 => current.limits.max_nodes += 1,
            _ => unreachable!(),
        }
        let warm = session.compile_project(&project, &current);
        let fresh = CompilationSession::default().compile_project(&project, &current);
        equivalent(&warm, &fresh);
        if warm.status == ResultCategory::Success {
            assert!(warm.query_events.iter().any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed));
        }
        assert_eq!(session.active_query_count(), 0);
        assert!(session.cached_query_count() <= 8);
    }
}
#[test]
fn replayed_phase_order_keeps_observer_cancellation_and_recovery() {
    let case = Case::new(40);
    for phase in [
        Phase::Parse,
        Phase::Expand,
        Phase::Ct,
        Phase::Types,
        Phase::Analyze,
        Phase::Verify,
        Phase::Lower,
    ] {
        let mut session = CompilationSession::with_capacity(8);
        successful(&session.compile_project(&case.project, &CompileOptions::default()));
        let cancelled = |session: &mut CompilationSession| {
            let options = CompileOptions {
                stop_after: Stage::Lower,
                ..CompileOptions::default()
            };
            let mut seen = 0;
            session.compile_project_with_observer(&case.project, &options, &mut |event| {
                if event.phase == phase {
                    seen += 1;
                    if seen == if phase == Phase::Parse { 17 } else { 1 } {
                        options.cancellation.cancel();
                    }
                }
            })
        };
        let warm = cancelled(&mut session);
        let fresh = cancelled(&mut CompilationSession::default());
        equivalent(&warm, &fresh);
        assert_eq!(warm.status, ResultCategory::Cancelled);
        assert_eq!(warm.phase, phase);
        assert_eq!(session.active_query_count(), 0);
        successful(&session.compile_project(&case.project, &CompileOptions::default()));
    }
}
#[test]
fn stage_stops_artifacts_disk_sources_and_filesystem_discovery_keep_original_path() {
    let case = Case::new(40);
    for stage in [Stage::Parse, Stage::Core, Stage::Types, Stage::Analyze] {
        let mut session = CompilationSession::with_capacity(8);
        successful(&session.compile_project(&case.project, &CompileOptions::default()));
        let options = CompileOptions {
            stop_after: stage,
            ..CompileOptions::default()
        };
        let warm = session.compile_project(&case.project, &options);
        equivalent(
            &warm,
            &CompilationSession::default().compile_project(&case.project, &options),
        );
        assert_eq!(warm.verification_runs, 0);
        assert_eq!(
            parses(&warm, QueryDisposition::Executed) == 0,
            stage >= Stage::Types
        );
    }
    for disk in [false, true] {
        let mut session = CompilationSession::with_capacity(8);
        successful(&session.compile_project(&case.project, &CompileOptions::default()));
        let mut options = CompileOptions::default();
        if disk {
            options.cache_dir = Some(case.dir.join("sources"));
        } else {
            options.emit_dir = Some(case.dir.join("artifacts"));
        }
        let report = session.compile_project(&case.project, &options);
        successful(&report);
        assert!(parses(&report, QueryDisposition::Executed) > 0);
        if !disk {
            let recorded = report
                .artifacts
                .iter()
                .map(|(name, path)| (name.clone(), fs::read(path).unwrap()))
                .collect::<BTreeMap<_, _>>();
            let fresh = CompilationSession::default().compile_project(&case.project, &options);
            equivalent(&report, &fresh);
            for (name, path) in fresh.artifacts {
                assert_eq!(recorded[&name], fs::read(path).unwrap());
            }
        }
    }
    let mut session = CompilationSession::with_capacity(8);
    for _ in 0..2 {
        let report = session.compile_path(&case.dir.join("m0.zil"), &CompileOptions::default());
        successful(&report);
        assert!(parses(&report, QueryDisposition::Executed) > 0);
    }
    fs::create_dir(case.dir.join("m1")).unwrap();
    fs::write(
        case.dir.join("m1/mod.zil"),
        "pub fn value()->i32{return 1;}",
    )
    .unwrap();
    let warm = session.compile_path(&case.dir.join("m0.zil"), &CompileOptions::default());
    let fresh = CompilationSession::default()
        .compile_path(&case.dir.join("m0.zil"), &CompileOptions::default());
    equivalent(&warm, &fresh);
    assert_eq!(warm.diagnostics[0].code, "E-MODULE-AMBIGUOUS");
}

#[test]
#[cfg(feature = "llvm-api")]
fn reused_frontend_builds_current_native_output_and_preserves_link_cancellation() {
    use zil_driver::{NativeBackend, Optimization};
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(40);
        let options = CompileOptions {
            stop_after: Stage::Build,
            backend: NativeBackend::LlvmApi,
            optimization,
            output: Some(case.dir.join("program")),
            object_cache: Some(case.dir.join("objects")),
            ..CompileOptions::default()
        };
        let mut session = CompilationSession::with_capacity(8);
        for warm in [false, true] {
            let report = session.compile_project(&case.project, &options);
            successful(&report);
            assert_eq!(parses(&report, QueryDisposition::Executed) == 0, warm);
            if warm {
                assert!(report.native_codegen.unwrap().object_cache.hits > 0);
            }
            assert_eq!(
                std::process::Command::new(case.dir.join("program"))
                    .status()
                    .unwrap()
                    .code(),
                Some(1)
            );
        }
        let before = fs::read(case.dir.join("program")).unwrap();
        let cancelled = options.clone();
        let report =
            session.compile_project_with_observer(&case.project, &cancelled, &mut |event| {
                if event.phase == Phase::Link {
                    cancelled.cancellation.cancel();
                }
            });
        assert_eq!(report.status, ResultCategory::Cancelled);
        assert_eq!(report.phase, Phase::Link);
        assert_eq!(fs::read(case.dir.join("program")).unwrap(), before);
        let mut edited = case.project.clone();
        edited.modules[1].text = "pub fn value()->i32{return 7;}".into();
        let mut current = options.clone();
        current.cancellation = Default::default();
        successful(&session.compile_project(&edited, &current));
        assert_eq!(
            std::process::Command::new(case.dir.join("program"))
                .status()
                .unwrap()
                .code(),
            Some(7)
        );
        current.output = Some(case.dir.join("m1.zil"));
        let rejected = session.compile_project(&edited, &current);
        equivalent(
            &rejected,
            &CompilationSession::default().compile_project(&edited, &current),
        );
        assert_ne!(rejected.status, ResultCategory::Success);
        assert_eq!(
            fs::read_to_string(case.dir.join("m1.zil")).unwrap(),
            case.project.modules[1].text
        );
    }
}

#[test]
#[cfg(feature = "llvm-api")]
fn real_resources_generics_ct_and_products_reuse_evicted_frontends() {
    use std::{
        io::Write,
        path::Path,
        process::{Command, Stdio},
    };
    use zil_driver::{NativeBackend, Optimization};
    // Build explicit source graphs using the real syntax parser, including
    // only reachable imports. No test-supplied IR enters the driver.
    fn explicit(path: &Path) -> ProjectSources {
        let path = path.canonicalize().unwrap();
        let root_dir = path.parent().unwrap().to_owned();
        let mut files = vec![path.clone()];
        let mut ids = BTreeMap::from([(path, ModuleId(0))]);
        let mut sources = Vec::new();
        let mut paths = BTreeMap::new();
        let mut index = 0;
        while index < files.len() {
            let path = &files[index];
            let source = SourceUnit::new(
                FileId(index as u32),
                ModuleId(index as u32),
                path.to_string_lossy(),
                fs::read_to_string(path).unwrap(),
            );
            let parsed = zil_syntax::parse(&source);
            assert!(parsed.diagnostics.is_empty());
            for item in parsed.module.items {
                if let zil_syntax::ast::ItemKind::Import(import) = item.kind {
                    let mut imported = root_dir.clone();
                    for segment in &import.path {
                        imported.push(segment);
                    }
                    let imported = imported.with_extension("zil").canonicalize().unwrap();
                    let next = ModuleId(files.len() as u32);
                    let id = *ids.entry(imported.clone()).or_insert_with(|| {
                        files.push(imported);
                        next
                    });
                    paths.insert(import.path, id);
                }
            }
            sources.push(source);
            index += 1;
        }
        ProjectSources {
            root: ModuleId(0),
            modules: sources,
            paths,
        }
    }
    fn execute(path: &Path, input: &[u8]) -> (Option<i32>, Vec<u8>, Vec<u8>) {
        let mut process = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        process.stdin.take().unwrap().write_all(input).unwrap();
        let result = process.wait_with_output().unwrap();
        (result.status.code(), result.stdout, result.stderr)
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline");
    for (name, input) in [
        ("byte_io/main.zil", b"hello, Zil!\n\0\x80".as_slice()),
        ("resource_closures/main.zil", b"D"),
        ("generic_store178/main.zil", b"2"),
        ("mixed_capture182/main.zil", b"2"),
        ("ct_selection219/main.zil", b"2"),
        ("ct_types228/arrays_defaults.zil", b""),
        ("where_copy229/main.zil", b""),
        ("shared_product224/main.zil", b""),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let mut case = Case::new(2);
            let path = root.join(name);
            case.project = explicit(&path);
            // Evict the actual consumers' Parse owners, even for small source
            // graphs, so this control necessarily exercises the new reuse.
            for i in 100..112 {
                case.project.modules.push(SourceUnit::new(
                    FileId(i),
                    ModuleId(i),
                    case.dir.join(format!("padding{i}.zil")).to_string_lossy(),
                    format!("fn padding{i}()->i32{{return 0;}}"),
                ));
            }
            let oracle_options = CompileOptions {
                stop_after: Stage::Build,
                backend: NativeBackend::Clang,
                optimization,
                output: Some(case.dir.join("oracle")),
                ..CompileOptions::default()
            };
            successful(&CompilationSession::default().compile_path(&path, &oracle_options));
            let expected = execute(&case.dir.join("oracle"), input);
            assert_eq!(
                expected.0,
                Some(if name.starts_with("ct_selection") {
                    2
                } else {
                    0
                })
            );
            let options = CompileOptions {
                backend: NativeBackend::LlvmApi,
                output: Some(case.dir.join("program")),
                object_cache: Some(case.dir.join("objects")),
                ..oracle_options
            };
            let mut session = CompilationSession::with_capacity(8);
            for warm in [false, true] {
                let report = session.compile_project(&case.project, &options);
                successful(&report);
                assert_eq!(parses(&report, QueryDisposition::Executed) == 0, warm);
                assert_eq!(
                    execute(&case.dir.join("program"), input),
                    expected,
                    "{name} {optimization:?}"
                );
            }
        }
    }
}
