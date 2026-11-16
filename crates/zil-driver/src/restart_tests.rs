//! Filesystem restart candidates must produce real current frontend epochs.
use super::*;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "zil-restart-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn fixture(&self, imports: usize) -> (PathBuf, CompileOptions) {
        let mut root = String::from("import unrelated;\n");
        for index in 0..imports {
            root += &format!("import m{index};\n");
            fs::write(self.0.join(format!("m{index}.zil")), module()).unwrap();
        }
        root +=
            "fn first(x:i32)->i32=>x+4; fn second(x:i32)->i32=>x+5; pub fn main()->i32=>m0.f(1);";
        fs::write(self.0.join("unrelated.zil"), module()).unwrap();
        let path = self.0.join("main.zil");
        fs::write(&path, root).unwrap();
        (
            path,
            CompileOptions {
                push_reuse: true,
                profile_compilation: true,
                semantic_cache: Some(self.0.join("cache")),
                ..Default::default()
            },
        )
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn module() -> &'static str {
    "fn private(x:i32)->i32=>x+1; fn other(x:i32)->i32=>x+2; pub fn f(x:i32)->i32=>x+3;"
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn typed(session: &CompilationSession, report: &CompileReport) -> Arc<TypedProgram> {
    session
        .raw
        .values()
        .find_map(|raw| match raw {
            RawSnapshot::ProjectTyped(owner)
                if owner.module.source_fingerprint == report.input_fingerprint =>
            {
                Some(owner.program().clone())
            }
            _ => None,
        })
        .expect("current actual typed owner")
}
fn epochs(session: &CompilationSession, report: &CompileReport, modules: usize) {
    let (key, owner) = session
        .raw
        .iter()
        .find_map(|(key, raw)| match raw {
            RawSnapshot::ProjectTyped(owner)
                if owner.module.source_fingerprint == report.input_fingerprint =>
            {
                Some((key, owner))
            }
            _ => None,
        })
        .unwrap();
    assert!(owner.has_analysis_epoch(key));
    assert!(session.push_types.checkpoint_for(owner.program()).is_some());
    let epoch = session
        .raw
        .values()
        .find_map(|raw| match raw {
            RawSnapshot::Project(expanded)
                if expanded.core.source_fingerprint == report.input_fingerprint =>
            {
                expanded.producer_epoch.as_ref()
            }
            _ => None,
        })
        .expect("real current producer epoch");
    assert_eq!(epoch.inputs.len(), modules);
    assert!(
        epoch
            .raw_inference
            .get()
            .unwrap()
            .checkpoint_for(&epoch.core)
            .is_some()
    );
    assert!(
        epoch
            .plain_source
            .get()
            .unwrap()
            .checkpoint_for(&epoch.core)
            .is_some()
    );
    let mut analyze = key.clone();
    analyze.phase = Phase::Analyze;
    let mut pending = vec![analyze];
    let mut seen = BTreeSet::new();
    let mut phases = BTreeSet::new();
    while let Some(key) = pending.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        let query = session
            .database
            .read(&key)
            .unwrap()
            .expect("completed real dependency");
        phases.insert(key.phase);
        pending.extend(query.dependencies.iter().map(|dep| dep.key.clone()));
    }
    for phase in [
        Phase::Parse,
        Phase::Expand,
        Phase::Ct,
        Phase::Types,
        Phase::Analyze,
    ] {
        assert!(phases.contains(&phase), "missing {phase:?}");
    }
    assert_eq!(session.active_query_count(), 0);
}
fn compare(
    session: &CompilationSession,
    report: &CompileReport,
    path: &Path,
    options: &CompileOptions,
) {
    let mut fresh = CompilationSession::default();
    let expected = fresh.compile_path(
        path,
        &CompileOptions {
            semantic_cache: None,
            push_reuse: false,
            producer_reuse: false,
            ..options.clone()
        },
    );
    assert_eq!(
        report.status,
        expected.status,
        "{}\n{}",
        report.to_json(),
        expected.to_json()
    );
    assert_eq!(report.phase, expected.phase);
    assert_eq!(report.diagnostics, expected.diagnostics);
    assert_eq!(report.scalar_interfaces, expected.scalar_interfaces);
    assert_eq!(report.verification_runs, expected.verification_runs);
    if report.status == ResultCategory::Success {
        assert_eq!(typed(session, report), typed(&fresh, &expected));
    }
    assert_eq!(session.active_query_count(), 0);
}
fn discover(path: &Path, options: &mut CompileOptions) -> (ProjectSources, LoadedProject) {
    let mut project = ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "placeholder", "")],
        paths: BTreeMap::new(),
    };
    let loaded = load_sources(&mut project, path, options, &CompilationSession::default()).unwrap();
    (project, loaded)
}

#[test]
fn disk_restart_at_default_capacity_restores_cfgs_then_pushes_one_private_edit() {
    let dir = TempDir::new();
    let (path, options) = dir.fixture(100);
    let mut writer = CompilationSession::default();
    let cold = writer.compile_path(&path, &options);
    success(&cold);
    assert_eq!(cold.source_files.len(), 102);
    drop(writer); // The restart must not inherit any live producer/checker state.
    let mut session = CompilationSession::default();
    assert_eq!(session.capacity, 32);
    let restart = session.compile_path(&path, &options);
    success(&restart);
    assert_eq!(
        restart.disk_cache,
        DiskCacheStatus::Rebuilt,
        "{}",
        restart.to_json()
    );
    for phase in [Phase::Types, Phase::Analyze] {
        assert!(restart.query_events.contains(&QueryEvent {
            phase,
            disposition: QueryDisposition::Executed
        }));
    }
    epochs(&session, &restart, 102);
    let baseline = typed(&session, &restart);
    assert_eq!(baseline.module.functions.len(), 306);
    let edited_path = dir.0.join("m0.zil");
    let edit = module().replace("x+1", "x+7");
    assert_eq!(edit.len(), module().len());
    fs::write(&edited_path, &edit).unwrap();
    let edited = session.compile_path(&path, &options);
    success(&edited);
    compare(&session, &edited, &path, &options);
    epochs(&session, &edited, 102);
    let work = edited.types_push_work.as_ref().unwrap();
    assert_eq!(
        (work.context_rebuilds, work.sparse_batches, work.body_checks),
        (0, 1, 1)
    );
    assert_eq!(work.retained_functions, 305);
    let ct = edited.ct_plain_work.as_ref().unwrap();
    assert_eq!((ct.body_checks, ct.body_reuses), (1, 305));
    let analysis = edited.initialization_work.as_ref().unwrap();
    assert_eq!(analysis.product_function_queries_executed, 1);
    assert_eq!(analysis.product_function_queries_reused, 305);
    let current = typed(&session, &edited);
    let mut changed = 0;
    for (index, before) in baseline.module.functions.iter().enumerate() {
        let after = &current.module.functions[index];
        let same_owner = Arc::ptr_eq(
            baseline.module.functions.owner(index).unwrap(),
            current.module.functions.owner(index).unwrap(),
        );
        if before == after {
            assert!(same_owner);
        } else {
            assert!(!same_owner);
            changed += 1;
        }
    }
    assert_eq!(changed, 1);

    fs::write(&edited_path, edit.replace("x+7", "missing")).unwrap();
    let error = session.compile_path(&path, &options);
    assert_ne!(error.status, ResultCategory::Success);
    assert_eq!(error.verification_runs, 0);
    compare(&session, &error, &path, &options);
    assert!(!session.raw.values().any(|raw| matches!(raw, RawSnapshot::ProjectTyped(owner) if owner.module.source_fingerprint == error.input_fingerprint)));
    fs::write(&edited_path, edit.replace("x+7", "x+8")).unwrap();
    let recovered = session.compile_path(&path, &options);
    success(&recovered);
    compare(&session, &recovered, &path, &options);
    epochs(&session, &recovered, 102);
}

#[test]
fn push_restart_ignores_whole_candidate_and_still_checks_current_unused_bodies() {
    let dir = TempDir::new();
    let (path, options) = dir.fixture(1);
    let mut writer = CompilationSession::default();
    let cold = writer.compile_path(&path, &options);
    success(&cold);
    let functions = typed(&writer, &cold).module.functions.len();
    drop(writer);
    let whole = options
        .semantic_cache
        .as_ref()
        .unwrap()
        .join("semantic-v7-push");
    let candidate = fs::read_dir(whole)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "candidate")
        })
        .expect("cold whole candidate exists");
    fs::write(candidate, b"unusable whole candidate").unwrap();
    let mut session = CompilationSession::default();
    let restarted = session.compile_path(&path, &options);
    success(&restarted);
    assert_eq!(
        restarted
            .source_ingress
            .as_ref()
            .unwrap()
            .semantic_load_attempts,
        0
    );
    let decoding = restarted.function_cache_decoding.as_ref().unwrap();
    assert_eq!(decoding.decode_attempts, decoding.decoded_functions);
    assert!(decoding.decoded_functions > 0);
    assert_eq!(
        decoding.decoded_functions + decoding.reused_decoded_functions,
        functions
    );
    assert!(decoding.relabeled_decoded_functions <= decoding.reused_decoded_functions);
    assert_eq!(
        session
            .function_types
            .last_stats()
            .restored_reused_functions,
        functions
    );
    epochs(&session, &restarted, 3);
    compare(&session, &restarted, &path, &options);
    drop(session);
    // An unused definition still enters the current producer/checker chain.
    let unused = dir.0.join("unrelated.zil");
    fs::write(&unused, module().replace("x+3", "true")).unwrap();
    let mut consumer = CompilationSession::default();
    let invalid = consumer.compile_path(&path, &options);
    assert_eq!(invalid.phase, Phase::Types);
    assert_ne!(invalid.status, ResultCategory::Success);
    assert_eq!(invalid.verification_runs, 0);
    compare(&consumer, &invalid, &path, &options);
}

#[test]
fn restart_replays_checked_function_fees_then_keeps_sparse_edits() {
    let dir = TempDir::new();
    let (path, options) = dir.fixture(100);
    // Long bodies and the ordinary scalar wrappers all use the same raw CFG
    // protocol; every original fee is still replayed in the real push ledger.
    let body = "let a=x+1; let b=a+2; let c=b+3; let d=c+4; return d;";
    let source = format!(
        "fn private(x:i32)->i32{{{body}}} fn other(x:i32)->i32{{{body}}} pub fn f(x:i32)->i32{{{body}}}"
    );
    for name in std::iter::once("unrelated".to_owned()).chain((0..100).map(|i| format!("m{i}"))) {
        fs::write(dir.0.join(format!("{name}.zil")), &source).unwrap();
    }
    let mut writer = CompilationSession::default();
    success(&writer.compile_path(&path, &options));
    assert!(!writer.function_types.is_empty());
    drop(writer);

    let mut session = CompilationSession::default();
    let restart = session.compile_path(&path, &options);
    success(&restart);
    assert_eq!(restart.disk_cache, DiskCacheStatus::Rebuilt);
    let stats = session.function_types.last_stats();
    assert_eq!(stats.reused_functions, 306);
    assert_eq!(stats.restored_reused_functions, 306);
    assert_eq!(stats.executed_functions, 0);
    epochs(&session, &restart, 102);
    compare(&session, &restart, &path, &options);

    let changed = source.replacen("x+1", "x+7", 1);
    fs::write(dir.0.join("m0.zil"), &changed).unwrap();
    let edited = session.compile_path(&path, &options);
    success(&edited);
    let work = edited.types_push_work.as_ref().unwrap();
    assert_eq!((work.context_rebuilds, work.body_checks), (0, 1));
    assert_eq!(work.retained_functions, 305);
    epochs(&session, &edited, 102);
    compare(&session, &edited, &path, &options);

    // A fresh process editing before its first request has no whole-project
    // disk candidate. The stable function cache still rechecks keys and can
    // reuse the other bodies while building genuine current push context.
    drop(session);
    fs::write(dir.0.join("m0.zil"), changed.replacen("x+7", "x+8", 1)).unwrap();
    let mut next = CompilationSession::default();
    let first_edit = next.compile_path(&path, &options);
    success(&first_edit);
    assert_eq!(first_edit.disk_cache, DiskCacheStatus::Missing);
    let stats = next.function_types.last_stats();
    assert_eq!(stats.reused_functions, 305);
    assert_eq!(stats.executed_functions, 1);
    epochs(&next, &first_edit, 102);
    compare(&next, &first_edit, &path, &options);

    drop(next);
    let valid = changed.replacen("x+7", "x+8", 1);
    fs::write(
        dir.0.join("m0.zil"),
        valid.replacen("return d;", "return true;", 1),
    )
    .unwrap();
    let mut rejected = CompilationSession::default();
    let error = rejected.compile_path(&path, &options);
    assert_eq!(error.phase, Phase::Types);
    assert_eq!(error.verification_runs, 0);
    assert_eq!(error.attempts.len(), 2);
    assert_eq!(error.attempts[0].phase, Phase::Types);
    assert_eq!(
        error.attempts[0].retry_reason.as_deref(),
        Some("rejected_function_work_product")
    );
    assert!(
        error.attempts[0]
            .function_type_work
            .as_ref()
            .unwrap()
            .restored_reused_functions
            > 0
    );
    compare(&rejected, &error, &path, &options);
    fs::write(dir.0.join("m0.zil"), valid).unwrap();
    let recovery = rejected.compile_path(&path, &options);
    success(&recovery);
    compare(&rejected, &recovery, &path, &options);
}

#[test]
fn retained_types_reactivates_analysis_before_replacing_its_evicted_query_slot() {
    let dir = TempDir::new();
    let (path, options) = dir.fixture(1);
    let mut session = CompilationSession::default();
    let cold = session.compile_path(&path, &options);
    success(&cold);
    let analyze = session
        .raw
        .iter()
        .find_map(|(key, raw)| matches!(raw, RawSnapshot::Candidate(_)).then(|| key.clone()))
        .unwrap();
    // A second real project replaces the active Analyze provider while the
    // first project's genuine Types epoch remains in the bounded raw cache.
    let other = dir.0.join("other.zil");
    fs::write(&other, "pub fn main()->i32=>9;").unwrap();
    success(&session.compile_path(&other, &options));
    session.raw.remove(&analyze);
    let restored = session.compile_path(&path, &options);
    success(&restored);
    assert!(restored.types_push_work.is_none());
    let work = restored.initialization_work.as_ref().unwrap();
    assert_eq!(work.shared_program_reuses, 1);
    assert_eq!(work.product_function_queries_executed, 0);
    assert!(restored.query_events.contains(&QueryEvent {
        phase: Phase::Analyze,
        disposition: QueryDisposition::Executed,
    }));
    epochs(&session, &restored, 3);
    compare(&session, &restored, &path, &options);
}

#[test]
fn restart_revalidates_namespace_signature_positions_flags_and_limits() {
    for mutation in ["namespace", "signature", "positions", "flags", "limits"] {
        let dir = TempDir::new();
        let (path, mut options) = dir.fixture(1);
        let mut writer = CompilationSession::default();
        success(&writer.compile_path(&path, &options));
        drop(writer);
        match mutation {
            "namespace" => {
                fs::write(dir.0.join("alt.zil"), module().replace("x+3", "x+9")).unwrap();
                fs::write(
                    &path,
                    fs::read_to_string(&path).unwrap().replace("m0", "alt"),
                )
                .unwrap();
            }
            "signature" => fs::write(
                dir.0.join("m0.zil"),
                module().replace("private(x:i32)->i32", "private(x:i64)->i64"),
            )
            .unwrap(),
            "positions" => fs::write(
                dir.0.join("m0.zil"),
                format!("// current positions\n{}", module()),
            )
            .unwrap(),
            "flags" => options.push_reuse = false,
            "limits" => options.limits.max_blocks = 0,
            _ => unreachable!(),
        }
        let mut session = CompilationSession::default();
        let report = session.compile_path(&path, &options);
        assert_ne!(report.disk_cache, DiskCacheStatus::Hydrated, "{mutation}");
        compare(&session, &report, &path, &options);
        if mutation == "limits" {
            assert_ne!(report.status, ResultCategory::Success);
            assert_eq!(report.verification_runs, 0);
            assert!(
                !session
                    .raw
                    .values()
                    .any(|raw| matches!(raw, RawSnapshot::ProjectTyped(_)))
            );
        } else {
            success(&report);
            assert!(report.query_events.contains(&QueryEvent {
                phase: Phase::Types,
                disposition: QueryDisposition::Executed
            }));
            if options.push_reuse {
                epochs(&session, &report, 3);
            }
        }
    }
}

#[test]
fn wrong_decoded_hydration_values_cannot_supply_results_checks_or_fees() {
    let dir = TempDir::new();
    let (path, options) = dir.fixture(1);
    let mut writer = CompilationSession::default();
    success(&writer.compile_path(&path, &options));
    drop(writer);
    for corruption in ["plan", "typed", "limits"] {
        let mut current_options = options.clone();
        let (project, loaded) = discover(&path, &mut current_options);
        let LoadedProject::Parsed(syntax) = loaded else {
            panic!("push discovery must restore current syntax without eagerly loading Typed/Plan");
        };
        // Explicitly inject the same untrusted whole image to retain the
        // counterexample. Ordinary push restoration no longer reads it.
        let mut pair = Box::new(
            exact_discovered_semantic_profiled(&project, &current_options, None)
                .expect("physical disk candidate for injection"),
        );
        if corruption != "plan" {
            pair.0.module.functions[0].return_type = zil_ir::Type::Bool;
        }
        pair.1.schema_version = u32::MAX;
        pair.1.functions = Default::default();
        if corruption == "limits" {
            // Each body fits alone; the real whole-program ledger must reject.
            let blocks = pair
                .0
                .module
                .functions
                .iter()
                .map(|f| f.blocks.len())
                .sum::<usize>();
            assert!(blocks > 1);
            current_options.limits.max_blocks = blocks - 1;
        }
        // Isolate the untrusted whole-program anchor from the independently
        // valid CFG cache. The separate restart regression tests real CFG hits;
        // this counterexample must still perform all nine current body checks.
        let function_cache = options
            .semantic_cache
            .as_ref()
            .unwrap()
            .join("function-types-v3-push");
        if function_cache.exists() {
            fs::remove_dir_all(function_cache).unwrap();
        }
        let mut session = CompilationSession::default();
        let report = session.compile_project_request(
            &project,
            &current_options,
            &mut |_| {},
            Some(LoadedProject::Hydrating(syntax, pair)),
        );
        compare(&session, &report, &path, &current_options);
        if corruption == "plan" {
            assert_eq!(report.disk_cache, DiskCacheStatus::Hydrated);
        } else {
            assert_ne!(report.disk_cache, DiskCacheStatus::Hydrated);
        }
        if corruption == "limits" {
            assert_ne!(report.status, ResultCategory::Success);
            assert_eq!(report.verification_runs, 0);
            assert!(
                !session
                    .raw
                    .values()
                    .any(|raw| matches!(raw, RawSnapshot::ProjectTyped(_)))
            );
        } else {
            success(&report);
            epochs(&session, &report, 3);
            let mut fresh = CompilationSession::default();
            let expected = fresh.compile_path(
                &path,
                &CompileOptions {
                    semantic_cache: None,
                    ..options.clone()
                },
            );
            success(&expected);
            assert_eq!(report.types_push_work, expected.types_push_work);
            assert_eq!(report.ct_plain_work, expected.ct_plain_work);
            assert_eq!(report.initialization_work, expected.initialization_work);
            assert_eq!(report.types_push_work.as_ref().unwrap().body_checks, 9);
            assert_eq!(
                report
                    .initialization_work
                    .as_ref()
                    .unwrap()
                    .product_function_queries_executed,
                9
            );
        }
    }
}
