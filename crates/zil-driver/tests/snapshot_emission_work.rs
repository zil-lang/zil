//! Original owning 512-address Package, through default Source limits and the
//! two real native backends. No shape substitution or enlarged budget/stack.
#![cfg(feature = "llvm-api")]
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
static NEXT: AtomicU64 = AtomicU64::new(0);
const PROGRAM: &str = include_str!(
    "../../../tests/fixtures/resource-packages/owned-package-wide-shared-address-array.zil"
);
const FIELD_SNAPSHOT: &str = include_str!(
    "../../../tests/fixtures/resource-packages/owned-package-shared-address-array-snapshot.zil"
);
const SNAPSHOT_FINGERPRINT: &str =
    "59b8aaa8aa292c1e3921cfee638ef03598e1ffe62a3a4402d1e1542cc793ae66";
const FINGERPRINT: &str = "95d6572a129f856a5b073de7cfa2aa9d2b730c0ff44b254fa6f9a3d8830299bb";
struct Temp(PathBuf, bool);
impl Temp {
    fn new() -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR").is_some();
        let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = base.join(format!(
            "snapshot-emission-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, keep)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn native_program(program: &str, fingerprint: &str) {
    let temp = Temp::new();
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "large-package-array-default-work",
        program,
    );
    assert_eq!(source.fingerprint().to_hex(), fingerprint);
    fs::write(temp.0.join("source.zil"), program).unwrap();
    let mut executed = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = temp.0.join(format!("{backend:?}-{optimization:?}"));
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
            fs::write(dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.input_fingerprint.to_hex(), fingerprint);
            assert_eq!(report.verification_runs, 1);
            assert!(!report.commands.is_empty());
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(!native.object_reports.is_empty());
                assert!(native.object_reports.iter().all(|r| {
                    r.contains("\"input_kind\":\"builder\"")
                        && r.contains("\"parse_ns\":0")
                        && r.contains("\"verify_before_after\":true")
                }));
            }
            let executable = &report.artifacts["executable"];
            fs::write(dir.join("execution.command"), format!("{executable:?}\n")).unwrap();
            let output = Command::new(executable).output().unwrap();
            fs::write(dir.join("stdout"), &output.stdout).unwrap();
            fs::write(dir.join("stderr"), &output.stderr).unwrap();
            fs::write(dir.join("status"), format!("{:?}\n", output.status)).unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {output:?}"
            );
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
            executed += 1;
        }
    }
    assert_eq!(executed, 4);
    eprintln!("original owning512 source={fingerprint} actual native executions={executed}");
}

#[test]
fn ordinary_owned_wide_snapshot_runs_both_backends_at_both_optimizations() {
    native_program(PROGRAM, FINGERPRINT);
}
#[test]
fn whole_owned_wide_array_field_snapshot_runs_after_owner_cleanup() {
    native_program(FIELD_SNAPSHOT, SNAPSHOT_FINGERPRINT);
}
