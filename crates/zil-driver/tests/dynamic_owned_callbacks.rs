use std::{collections::BTreeMap, fs, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
};

#[test]
fn affine_dynamic_owner_and_program_static_capture_keep_their_distinct_responsibilities() {
    let text = include_str!("../../../tests/fixtures/dyn-context/closed-owner-static-callback.zil");
    check(text, "closed-owner-static-callback");
}

#[test]
fn context_owner_moves_through_a_repeated_callable_and_ordinary_forwarding_helper() {
    check(include_str!("../../../tests/fixtures/dyn-context/captured-owner.zil"), "context-owner-callback");
}

#[test]
fn declared_long_slice_getter_preserves_the_actual_fixed_context_class() {
    let text=include_str!("../../../tests/fixtures/dyn-context/long-slice-getter.zil");
    let project=ProjectSources{root:ModuleId(0),modules:vec![SourceUnit::new(FileId(0),ModuleId(0),"main.zil",text)],paths:BTreeMap::new()};
    let root=std::env::var_os("ZIL_TEST_REPORT_DIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir)
        .join(format!("dyn-long-slice-getter-{}",std::process::id()));
    fs::create_dir_all(&root).unwrap();fs::write(root.join("main.zil"),text).unwrap();
    let report=compile_project(&project,&CompileOptions{stop_after:Stage::Types,emit_dir:Some(root.join("phases")),..CompileOptions::default()});
    fs::write(root.join("compile.json"),report.to_json()).unwrap();
    assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
    assert_eq!(report.verification_runs,0);
    let source=&project.modules[0];let limits=zil_base::Limits::default();
    let syntax=zil_syntax::parse(source).into_result().unwrap();
    let core=zil_expand::expand_with_map_limits_target_context(source,&syntax,&limits,&zil_base::TargetProfile::default(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot{root:source.module_id,aliases:&BTreeMap::new(),declarations:zil_expand::source_context::DeclarationPlacement::Original}).unwrap().core;
    let module=zil_types::check(&core,&zil_base::TargetProfile::default(),&limits).unwrap();
    let bytes=zil_ir::wire::encode_module_candidate(&module,&zil_ir::CandidatePlan::empty_for(&module),&limits).unwrap();
    fs::write(root.join("typed-unverified-empty-plan.wire"),bytes).unwrap();
}

fn check(text: &str, label: &str) {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)],
        paths: BTreeMap::new(),
    };
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "{label}-{}",
            std::process::id()
        ));
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = root.join(format!("{backend:?}-{optimization:?}"));
            fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("program");
            let report = compile_project(
                &project,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
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
            let output = Command::new(executable).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
        }
    }
}
