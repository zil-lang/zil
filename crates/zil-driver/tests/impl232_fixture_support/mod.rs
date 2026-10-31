//! Raw receipts for the three explicitly authorized slice232 fixture migrations.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{Limits, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, NativeBackend, Optimization, Stage, compile_source,
};
use zil_ir::TypedModule;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory(label: &str, source: &SourceUnit) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "migration232-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("source.zil"), &source.text).unwrap();
    path
}

pub fn verified(label: &str, source: &SourceUnit, module: &TypedModule) {
    let path = directory(label, source);
    fs::write(path.join("typed.txt"), format!("{module:#?}")).unwrap();
    let plan = zil_analyze::analyze(module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &Limits::default()).unwrap();
    fs::write(path.join("candidate.wire"), &wire).unwrap();
    zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    let report = compile_source(
        source,
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}

pub fn native(label: &str, source: &SourceUnit, exit: i32) {
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = directory(label, source);
        let report = compile_source(
            source,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                backend,
                output: Some(path.join("program")),
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
        let actual = report.native_codegen.as_ref().unwrap();
        assert_eq!(actual.backend, backend);
        assert!(actual.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            assert!(!actual.object_reports.is_empty());
            assert!(
                actual
                    .object_reports
                    .iter()
                    .all(|r| r.contains("\"input_kind\":\"builder\""))
            );
        }
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(path.join("stdout"), &output.stdout).unwrap();
        fs::write(path.join("stderr"), &output.stderr).unwrap();
        fs::write(path.join("exit.txt"), format!("{:?}", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert!(output.stdout.is_empty(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

pub fn rejected(
    label: &str,
    source: &SourceUnit,
    category: ResultCategory,
    phase: Phase,
    code: &str,
) -> CompileReport {
    let path = directory(label, source);
    let report = compile_source(
        source,
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == phase && d.code == code),
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.verification_runs,
        usize::from(phase == Phase::Verify)
    );
    assert!(!report.artifacts.contains_key("executable"));
    report
}
