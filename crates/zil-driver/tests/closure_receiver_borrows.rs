//! Whole Source, raw candidate, decoded candidate and real machine consumers.
use std::{fs, process::Command};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
fn accepted(name: &str, text: &str) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-receiver-{}-{name}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let report = compile_source(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}
macro_rules! consumer {
    ($test:ident, $source:literal) => {
        #[test]
        fn $test() {
            accepted(
                $source,
                include_str!(concat!(
                    "../../../tests/fixtures/returned-owning-holders/",
                    $source,
                    ".zil"
                )),
            );
        }
    };
}
consumer!(
    captured_mutable_holder_updates_original_storage,
    "captured-mutable-holder"
);
consumer!(
    captured_mutable_holder_survives_borrowed_helpers,
    "captured-mutable-holder-helper"
);
consumer!(
    same_entry_receivers_keep_distinct_holders,
    "captured-mutable-holder-two-receivers"
);
consumer!(
    readonly_exclusive_capture_reborrows_without_copying_environment,
    "exclusive-capture-readonly-fn"
);
consumer!(
    call_before_return_keeps_actual_holder_update,
    "captured-mutable-holder-call-before-return"
);
