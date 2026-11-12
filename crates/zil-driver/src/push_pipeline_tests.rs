//! Real driver consumers, compared with a fresh complete legacy compilation.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-push-pipeline-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn project(&self, private: &str) -> ProjectSources {
        let mut main = "import io\n".to_owned();
        for module in 0..20 {
            main += &format!("import m{module}\n");
        }
        main += "fn value(x:i32)->i32{var y=x;";
        for module in 0..20 {
            main += &format!("y=m{module}.value(y);");
        }
        main += "return y;}";
        let io = (0..4)
            .map(|n| format!("extern \"C\" fn external{n}(x:i32)->i32;\n"))
            .collect::<String>();
        let mut texts = vec![("main".to_owned(), main), ("io".to_owned(), io)];
        for module in 0..20 {
            let mut text = String::new();
            for function in 0..20 {
                text += &format!("fn h{function}(x:i32)->i32{{var y=x;");
                for step in 0..16 {
                    text += &format!("y=y*3+{};", 11 + step);
                }
                text += "return y;}\n";
            }
            text += "pub fn value(x:i32)->i32{var y=x;";
            for function in 0..20 {
                text += &format!("y=h{function}(y);");
            }
            text += "return y;}\n";
            if module == 0 {
                text = text.replacen("y=y*3+11;", private, 1);
            }
            texts.push((format!("m{module}"), text));
        }
        let mut paths = BTreeMap::new();
        let modules = texts
            .into_iter()
            .enumerate()
            .map(|(index, (name, text))| {
                let path = self.0.join(format!("{name}.zil"));
                fs::write(&path, &text).unwrap();
                if index != 0 {
                    paths.insert(vec![name], ModuleId(index as u32));
                }
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    path.to_string_lossy(),
                    text,
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
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn current<'a>(session: &'a CompilationSession, report: &CompileReport) -> &'a TypedProgram {
    session
        .raw
        .values()
        .find_map(|raw| match raw {
            RawSnapshot::ProjectTyped(program)
                if program.module.source_fingerprint == report.input_fingerprint =>
            {
                Some(program.as_ref())
            }
            _ => None,
        })
        .expect("current actual typed product")
}
fn compare(
    session: &CompilationSession,
    report: &CompileReport,
    project: &ProjectSources,
    options: &CompileOptions,
) {
    let mut fresh = CompilationSession::default();
    let expected = fresh.compile_project(
        project,
        &CompileOptions {
            push_reuse: false,
            producer_reuse: false,
            semantic_cache: None,
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
    assert_eq!(session.active_query_count(), 0);
    if report.status == ResultCategory::Success {
        assert_eq!(current(session, report), current(&fresh, &expected));
    }
}
#[test]
fn complete_driver_pushes_one_private_body_through_current_verify_and_lower() {
    let dir = Directory::new();
    // The public API flag alone includes producer reuse; the CLI is not needed
    // to fill a second hidden prerequisite.
    let options = CompileOptions {
        push_reuse: true,
        stop_after: Stage::Lower,
        ..Default::default()
    };
    let mut session = CompilationSession::with_capacity(256);
    let initial = dir.project("y=y*3+11;");
    let cold = session.compile_project(&initial, &options);
    compare(&session, &cold, &initial, &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    assert_eq!(current(&session, &cold).module.functions.len(), 425);
    assert_eq!(cold.types_push_work.as_ref().unwrap().context_rebuilds, 1);
    let edited = dir.project("y=y*3+12;");
    let changed = session.compile_project(&edited, &options);
    compare(&session, &changed, &edited, &options);
    assert_eq!(
        changed.types_push_work.as_ref().unwrap().body_checks,
        1,
        "{}",
        changed.to_json()
    );
    assert_eq!(changed.types_push_work.as_ref().unwrap().sparse_batches, 1);
    let inference = changed.raw_inference_work.as_ref().unwrap();
    assert_eq!(inference.closure_bodies_checked, 1);
    assert_eq!(inference.closure_bodies_reused, 424);
    assert_eq!(inference.name_functions_checked, 1);
    assert_eq!(inference.name_functions_reused, 424);
    assert_eq!(inference.dependency_bodies_checked, 1);
    assert_eq!(inference.dependency_bodies_reused, 424);
    assert_eq!(
        changed
            .initialization_work
            .unwrap()
            .product_function_queries_executed,
        1,
        "{}",
        changed.to_json()
    );
    assert_eq!(changed.verification_runs, 1);
    assert_eq!(
        changed
            .verification_work
            .unwrap()
            .raw_inventory_functions_checked,
        1
    );
    assert_eq!(changed.lowering_work.unwrap().functions_checked, 1);
    assert_eq!(changed.lowering_work.unwrap().functions_reused, 424);
    let unchanged = session.compile_project(&edited, &options);
    compare(&session, &unchanged, &edited, &options);
    assert_eq!(unchanged.verification_work.unwrap().full_receipts_reused, 1);
    assert_eq!(unchanged.lowering_work.unwrap().functions_checked, 0);
    let bad = dir.project("y=missing(y);");
    let rejected = session.compile_project(&bad, &options);
    assert_ne!(rejected.status, ResultCategory::Success);
    compare(&session, &rejected, &bad, &options);
    let recovered = dir.project("y=y*3+13;");
    let recovered_report = session.compile_project(&recovered, &options);
    compare(&session, &recovered_report, &recovered, &options);
    assert_eq!(recovered_report.status, ResultCategory::Success);
}
#[test]
fn retained_parent_restores_actual_push_consumers_after_broad_history() {
    let dir = Directory::new();
    let options = CompileOptions {
        push_reuse: true,
        stop_after: Stage::Lower,
        ..Default::default()
    };
    let mut session = CompilationSession::with_capacity(256);
    let cold = dir.project("y=y*3+11;");
    let first = session.compile_project(&cold, &options);
    compare(&session, &first, &cold, &options);
    let epoch = session
        .raw
        .values()
        .find_map(|raw| match raw {
            RawSnapshot::ProjectTyped(owner)
                if owner.module.source_fingerprint == first.input_fingerprint =>
            {
                owner.epoch_weak()
            }
            _ => None,
        })
        .expect("bounded current push owner checkpoint");
    let mut broad = cold.clone();
    for source in &mut broad.modules {
        source.text = source.text.replace("y=y*3+11;", "y=y*3+21;");
        fs::write(&source.path, &source.text).unwrap();
    }
    let broad_report = session.compile_project(&broad, &options);
    compare(&session, &broad_report, &broad, &options);
    assert_eq!(
        broad_report.types_push_work.as_ref().unwrap().body_checks,
        400
    );
    let restored = dir.project("y=y*3+11;");
    let parent = session.compile_project(&restored, &options);
    compare(&session, &parent, &restored, &options);
    assert!(parent.types_push_work.is_none(), "{}", parent.to_json());
    assert!(parent.initialization_work.is_none(), "{}", parent.to_json());
    let private = dir.project("y=y*3+12;");
    let report = session.compile_project(&private, &options);
    compare(&session, &report, &private, &options);
    assert_eq!(report.types_push_work.as_ref().unwrap().context_rebuilds, 0);
    assert_eq!(report.types_push_work.as_ref().unwrap().body_checks, 1);
    assert_eq!(
        report
            .initialization_work
            .unwrap()
            .product_function_queries_executed,
        1
    );
    assert_eq!(report.lowering_work.unwrap().functions_checked, 1);
    let inference = report.raw_inference_work.as_ref().unwrap();
    assert_eq!(inference.closure_bodies_checked, 1);
    assert_eq!(inference.name_functions_checked, 1);
    assert_eq!(inference.dependency_bodies_checked, 1);
    session.clear();
    assert!(
        epoch.upgrade().is_none(),
        "checkpoint lifetime follows the existing bounded raw owner"
    );
}

#[test]
fn semantic_product_restore_rechecks_current_inputs_and_isolated_identity_domain() {
    let dir = Directory::new();
    let project = dir.project("y=y*3+11;");
    let options = CompileOptions {
        push_reuse: true,
        semantic_cache: Some(dir.0.join("cache")),
        ..Default::default()
    };
    let mut producer = CompilationSession::with_capacity(256);
    let first = producer.compile_project(&project, &options);
    assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
    assert!(dir.0.join("cache/semantic-v7-push").is_dir());
    let mut restored = CompilationSession::with_capacity(256);
    let second = restored.compile_project(&project, &options);
    compare(&restored, &second, &project, &options);
    assert_eq!(second.verification_runs, 1);
    assert_eq!(second.verification_work.unwrap().full_receipts_reused, 0);
    let legacy = CompileOptions {
        push_reuse: false,
        ..options
    };
    let mut ordinary = CompilationSession::default();
    let plain = ordinary.compile_project(&project, &legacy);
    assert_eq!(plain.status, ResultCategory::Success, "{}", plain.to_json());
    assert_eq!(plain.verification_runs, 1);
    assert!(plain.types_push_work.is_none());
}
