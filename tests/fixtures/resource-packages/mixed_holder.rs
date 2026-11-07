//! A real mixed Package keeps resource responsibility and an external loan.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

const PROGRAM: &str = include_str!("mixed-owned-borrowed-package.zil");

fn source() -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "mixed-owned-borrowed-package.zil", PROGRAM)
}

#[test]
fn mixed_package_source_raw_and_wire() {
    let source = source();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let report = compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1);

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
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn mixed_package_native_both_optimizations() {
    for optimization in [Optimization::O0, Optimization::O2] {
        let output = std::env::temp_dir().join(format!(
            "zil-mixed-package-{}-{optimization:?}",
            std::process::id()
        ));
        let report = compile_source(
            &source(),
            &CompileOptions {
                stop_after: Stage::Build,
                backend: NativeBackend::LlvmApi,
                optimization,
                entry: Some("main".into()),
                output: Some(output.clone()),
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
        assert_eq!(report.verification_runs, 1);
        let status = std::process::Command::new(&report.artifacts["executable"])
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(42));
        std::fs::remove_file(output).unwrap();
    }
}
