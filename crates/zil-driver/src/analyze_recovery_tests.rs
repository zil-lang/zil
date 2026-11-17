use super::*;
use crate::session::RawSnapshot;
use crate::{CompilationSession, ProjectSources};
use zil_base::ResultCategory;

struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-analyze-restart-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn project(&self, value: i32) -> ProjectSources {
        self.project_text(&format!(
            "fn private()->i32{{return {value};}}\npub fn value()->i32{{return private();}}"
        ))
    }
    fn project_text(&self, left: &str) -> ProjectSources {
        let modules = [("main", "import left\nfn main()->i32{return left.value()+right();}\nfn right()->i32{return 23;}"), ("left", left)]
            .into_iter().enumerate().map(|(index, (name, text))| {
                let path = self.0.join(format!("{name}.zil")); fs::write(&path, text).unwrap();
                SourceUnit::new(FileId(index as u32), ModuleId(index as u32), path.to_string_lossy(), text)
            }).collect();
        ProjectSources {
            root: ModuleId(0),
            modules,
            paths: BTreeMap::from([(vec!["left".into()], ModuleId(1))]),
        }
    }
    fn options(&self) -> CompileOptions {
        CompileOptions {
            push_reuse: true,
            semantic_cache: Some(self.0.join("semantic")),
            profile_compilation: true,
            ..CompileOptions::default()
        }
    }
    fn manifest(&self, options: &CompileOptions) -> PathBuf {
        assert!(
            options
                .semantic_cache
                .as_ref()
                .unwrap()
                .starts_with(&self.0)
        );
        let compiler = COMPILER
            .get_or_init(|| zil_backend_llvm::installed_compiler_identity().ok())
            .as_ref()
            .unwrap();
        options
            .semantic_cache
            .as_ref()
            .unwrap()
            .join("analyze-recovery-v1")
            .join(compiler.to_hex())
            .join("current.manifest")
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn success(report: &crate::CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn reference(project: &ProjectSources, options: &CompileOptions, report: &crate::CompileReport) {
    let mut original = options.clone();
    original.semantic_cache = None;
    original.push_reuse = false;
    let expected = CompilationSession::default().compile_project(project, &original);
    assert_eq!(report.status, expected.status);
    assert_eq!(report.phase, expected.phase);
    assert_eq!(report.diagnostics, expected.diagnostics);
    assert_eq!(report.scalar_interfaces, expected.scalar_interfaces);
}
fn typed(session: &CompilationSession) -> Arc<TypedProgram> {
    session
        .raw
        .values()
        .find_map(|raw| match raw {
            RawSnapshot::ProjectTyped(owner) => Some(owner.program().clone()),
            _ => None,
        })
        .expect("actual current Source/Types product")
}
fn payload(manifest: &Path) -> Vec<u8> {
    let bytes = fs::read(manifest).unwrap();
    assert_eq!(&bytes[..8], MANIFEST);
    let digest = Fingerprint(bytes[8..40].try_into().unwrap());
    fs::read(
        manifest
            .parent()
            .unwrap()
            .join(format!("{}.raw", digest.to_hex())),
    )
    .unwrap()
}

#[test]
fn source_restart_private_edit_restores_raw_analyze_and_current_verify() {
    let case = Case::new();
    let options = case.options();
    let source = case.project(19);
    assert!(!options.semantic_cache.as_ref().unwrap().exists());
    let mut cold = CompilationSession::default();
    success(&cold.compile_project(&source, &options));
    let manifest = case.manifest(&options);
    assert!(manifest.is_file());
    let old = fs::read(&manifest).unwrap();
    drop(cold);
    let current = case.project(20);
    let mut restarted = CompilationSession::default();
    let report = restarted.compile_project(&current, &options);
    success(&report);
    reference(&current, &options, &report);
    let stats = report.initialization_work.unwrap();
    assert_eq!(stats.product_function_queries_executed, 1);
    assert_eq!(stats.product_admission_functions_checked, 1);
    assert!(stats.product_function_queries_reused >= 3);
    assert_eq!(stats.product_global_obligations_reused, 1);
    assert_ne!(old, fs::read(&manifest).unwrap());
    assert_eq!(
        fs::read_dir(manifest.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "raw"))
            .count(),
        1
    );
}

#[test]
fn source_restart_bad_bytes_caps_and_template_references_are_optional_misses() {
    let case = Case::new();
    let options = case.options();
    let project = case.project(19);
    let mut producer = CompilationSession::default();
    success(&producer.compile_project(&project, &options));
    let program = typed(&producer);
    let context = RawAnalysisBase::context_for(&program, &options.limits).unwrap();
    let manifest = case.manifest(&options);
    let bytes = payload(&manifest);
    let products = producer.function_types.last_products();
    let restored = decode(&bytes, &context, &options, &program, products).unwrap();
    assert_eq!(restored.rows.len(), program.module.functions.len());
    for length in [0, 8, bytes.len() - 1] {
        assert!(decode(&bytes[..length], &context, &options, &program, products).is_none());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode(&trailing, &context, &options, &program, products).is_none());
    let mut cursor = Input {
        bytes: &bytes,
        nodes: options.limits.max_nodes,
    };
    cursor.take(8).unwrap();
    cursor.blob().unwrap();
    cursor.cost().unwrap();
    let templates = cursor.word().unwrap();
    for _ in 0..templates {
        cursor.blob().unwrap();
        cursor.blob().unwrap();
    }
    cursor.word().unwrap();
    let offset = bytes.len() - cursor.bytes.len();
    let mut noncanonical = bytes.clone();
    noncanonical[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
    assert!(decode(&noncanonical, &context, &options, &program, products).is_none());
    let mut low = options.clone();
    low.limits.max_source_bytes = bytes.len() - 1;
    assert!(decode(&bytes, &context, &low, &program, products).is_none());
    let mut wrong_context = context.clone();
    wrong_context[0] ^= 1;
    assert!(decode(&bytes, &wrong_context, &options, &program, products).is_none());
    let mut broken_manifest = fs::read(&manifest).unwrap();
    broken_manifest[8] ^= 1;
    fs::write(&manifest, broken_manifest).unwrap();
    drop(producer);
    let edited = case.project(20);
    let mut restarted = CompilationSession::default();
    let report = restarted.compile_project(&edited, &options);
    success(&report);
    reference(&edited, &options, &report);
    assert_eq!(
        report
            .initialization_work
            .unwrap()
            .product_global_obligations_executed,
        1
    );
}

#[test]
fn source_restart_failure_cancel_default_off_and_source_collision_keep_successful_base() {
    let case = Case::new();
    let options = case.options();
    let project = case.project(19);
    success(&CompilationSession::default().compile_project(&project, &options));
    let manifest = case.manifest(&options);
    let prior = fs::read(&manifest).unwrap();
    let invalid = case.project_text("pub fn value()->i32{return absent_current_name;}");
    let failed = CompilationSession::default().compile_project(&invalid, &options);
    assert_ne!(failed.status, ResultCategory::Success);
    reference(&invalid, &options, &failed);
    assert_eq!(fs::read(&manifest).unwrap(), prior);
    let current = case.project(21);
    let mut canceled = options.clone();
    canceled.cancellation = CancellationToken::default();
    canceled.cancellation.cancel();
    assert_ne!(
        CompilationSession::default()
            .compile_project(&current, &canceled)
            .status,
        ResultCategory::Success
    );
    assert_eq!(fs::read(&manifest).unwrap(), prior);
    let mut disabled = options.clone();
    disabled.push_reuse = false;
    success(&CompilationSession::default().compile_project(&current, &disabled));
    assert_eq!(fs::read(&manifest).unwrap(), prior);
    let mut collision = options.clone();
    collision.semantic_cache = Some(case.0.join("main.zil"));
    let source_bytes = fs::read(case.0.join("main.zil")).unwrap();
    let plan = Plan::project(&current.modules[0], &current.modules, &options).unwrap();
    assert!(open(&collision, &plan, true).is_none());
    assert_eq!(fs::read(case.0.join("main.zil")).unwrap(), source_bytes);
    let recovery = CompilationSession::default().compile_project(&current, &options);
    success(&recovery);
    assert_eq!(
        recovery
            .initialization_work
            .unwrap()
            .product_function_queries_executed,
        1
    );
}

#[test]
fn raw_plan_fragments_keep_all_rows_and_reject_trailing_tags_and_node_exhaustion() {
    let plan = Arc::new(zil_ir::FunctionPlan {
        function: FunctionId(7),
        blocks: vec![zil_ir::BlockPlan {
            block: zil_ir::BlockId(0),
            initialized_in: vec![zil_ir::LocalId(0)],
            initialized_out: vec![zil_ir::LocalId(0), zil_ir::LocalId(1)],
        }]
        .into(),
        holds: vec![zil_ir::ValueHold {
            value: zil_ir::ValueId(0),
            roots: vec![zil_ir::HoldRoot::Static],
        }]
        .into(),
    });
    let limits = Limits::default();
    let encoded = zil_ir::wire::encode_raw_function_plan(plan.clone(), &limits).unwrap();
    let (decoded, association) = zil_ir::wire::decode_raw_function_plan_shared_owner(
        encoded.shared_bytes(),
        &limits,
        limits.max_nodes,
    )
    .unwrap();
    assert_eq!(decoded, plan);
    assert_eq!(association.node_cost(), encoded.node_cost());
    assert!(association.matches_limits(&limits));
    assert!(
        zil_ir::wire::decode_raw_function_plan_shared_owner(
            encoded.shared_bytes(),
            &limits,
            encoded.node_cost() - 1
        )
        .is_err()
    );
    let mut bad = encoded.shared_bytes().to_vec();
    bad.push(0);
    assert!(
        zil_ir::wire::decode_raw_function_plan_shared_owner(bad.into(), &limits, limits.max_nodes)
            .is_err()
    );
    let mut bad = encoded.shared_bytes().to_vec();
    *bad.last_mut().unwrap() = 255;
    assert!(
        zil_ir::wire::decode_raw_function_plan_shared_owner(bad.into(), &limits, limits.max_nodes)
            .is_err()
    );
}

#[cfg(all(feature = "llvm-api", target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires ZIL_TEST_NATIVE_LINK_PROFILE; actual restart, native output and guarded failure recovery"]
fn native_restart_raw_analyze_private_edit_and_failed_publication_recover() {
    use std::process::Command;
    let case = Case::new();
    let mut options = case.options();
    options.stop_after = Stage::Build;
    options.backend = NativeBackend::LlvmApi;
    options.optimization = Optimization::O2;
    options.output = Some(case.0.join("program"));
    options.native_link_profile = Some(PathBuf::from(
        std::env::var_os("ZIL_TEST_NATIVE_LINK_PROFILE").unwrap(),
    ));
    options.closed_executable = true;
    options.incremental_o2 = true;
    options.bounded_class_imports = true;
    let source = case.project(19);
    let mut cold = CompilationSession::default();
    success(&cold.compile_project(&source, &options));
    assert_eq!(
        Command::new(options.output.as_ref().unwrap())
            .status()
            .unwrap()
            .code(),
        Some(42)
    );
    drop(cold);
    let manifest = case.manifest(&options);
    let prior = fs::read(&manifest).unwrap();
    let current = case.project(20);
    let mut failure_options = options.clone();
    failure_options.native_link_profile = Some(case.0.join("absent-native-profile.json"));
    let mut restart = CompilationSession::default();
    assert_ne!(
        restart.compile_project(&current, &failure_options).status,
        ResultCategory::Success
    );
    assert_eq!(fs::read(&manifest).unwrap(), prior);
    let report = restart.compile_project(&current, &options);
    success(&report);
    assert_eq!(
        report
            .initialization_work
            .unwrap()
            .product_function_queries_executed,
        1
    );
    assert_eq!(
        Command::new(options.output.as_ref().unwrap())
            .status()
            .unwrap()
            .code(),
        Some(43)
    );
    assert!(report.native_codegen.is_some());
    let successful = fs::read(&manifest).unwrap();
    let invalid = case.project_text(
        "pub fn value()->i32{let x=1;var p=&x;if true{let inner=2;p=&inner;}return *p;}",
    );
    assert_ne!(
        restart.compile_project(&invalid, &options).status,
        ResultCategory::Success
    );
    assert_eq!(fs::read(&manifest).unwrap(), successful);
    drop(restart);
    let recovery = case.project(21);
    let report = CompilationSession::default().compile_project(&recovery, &options);
    success(&report);
    assert_eq!(
        report
            .initialization_work
            .unwrap()
            .product_function_queries_executed,
        1
    );
    assert_eq!(
        Command::new(options.output.as_ref().unwrap())
            .status()
            .unwrap()
            .code(),
        Some(44)
    );
}

#[test]
fn source_restart_with_foreign_input_restores_analysis_instead_of_silently_missing() {
    let case = Case::new();
    let options = case.options();
    let project = |value| {
        let mut source = case.project_text(&format!(
            "fn private(x:i32)->i32{{return x+{value};}}\npub fn value(x:i32)->i32{{return private(x);}}"));
        let main = "import left\nextern \"C\" fn getchar()->i32;\nfn main()->i32{unsafe{let x=getchar();return left.value(x)+right(x);}}\nfn right(x:i32)->i32{return x*23;}";
        let path = case.0.join("main.zil");
        fs::write(&path, main).unwrap();
        source.modules[0] = SourceUnit::new(FileId(0), ModuleId(0), path.to_string_lossy(), main);
        source
    };
    let mut cold = CompilationSession::default();
    success(&cold.compile_project(&project(19), &options));
    let count = typed(&cold).module.functions.len();
    assert!(count >= 5);
    drop(cold);
    let current = project(20);
    let report = CompilationSession::default().compile_project(&current, &options);
    success(&report);
    reference(&current, &options, &report);
    let stats = report.initialization_work.unwrap();
    assert_eq!(stats.raw_functions_restored, count);
    assert_eq!(stats.product_function_queries_executed, 1);
    assert_eq!(stats.product_function_queries_reused, count - 1);
}
