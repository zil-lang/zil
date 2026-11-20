//! End-to-end Alpha acceptance integration suite through `zil_driver::CompilationSession`.
//!
//! Unlike component-level corpus tests, every test here runs the complete CLI pipeline:
//! Parse -> CT original declaration/import checks -> Typeck -> Real AST `check_fidelity`
//! -> Solve -> Independent Check -> Native Build (`Clang` and `LlvmApi`, O0/O2, 1/8 units)
//! -> Real ELF execution with observable exit codes and stdout.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::{Phase, ResultCategory};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-alpha-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn build_and_run(
    entry: &Path,
    exe: &Path,
    backend: NativeBackend,
    opt: Optimization,
    units: usize,
    stdin_bytes: &[u8],
) -> (i32, Vec<u8>, usize, usize) {
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Build,
        optimization: opt,
        backend,
        output: Some(exe.to_path_buf()),
        codegen: CodegenOptions {
            units,
            jobs: if units > 1 { 4 } else { 1 },
        },
        ..CompileOptions::default()
    };
    let report = session.compile_path(entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "build failed for {} ({backend:?}, {opt:?}, u{units}): {:?}",
        entry.display(),
        report.diagnostics
    );
    assert_eq!(report.verification_runs, 1);
    let cg = report
        .native_codegen
        .as_ref()
        .expect("native_codegen summary");
    assert_eq!(cg.backend, backend);
    let actual_units = cg.actual_units;
    let obj_reports = cg.object_reports.len();

    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", exe.display()));
    if !stdin_bytes.is_empty() {
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(stdin_bytes)
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait child");
    assert!(
        out.stderr.is_empty(),
        "unexpected stderr from {}: {}",
        exe.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    (
        out.status.code().unwrap_or(-1),
        out.stdout,
        actual_units,
        obj_reports,
    )
}

#[test]
fn state_store_example_dual_backend_o0_o2_and_real_8_unit_execution() {
    let dir = temp_dir("state-store");
    let main_entry = repo_root().join("examples/mainline/zui-language-store-fields/main.zil");
    let field_exit = repo_root().join("examples/mainline/zui-language-store-fields/field_exit.zil");

    let mut configs = vec![
        ("clang-o0-u1", NativeBackend::Clang, Optimization::O0, 1),
        ("clang-o2-u1", NativeBackend::Clang, Optimization::O2, 1),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0-u1", NativeBackend::LlvmApi, Optimization::O0, 1));
        configs.push(("llvmapi-o2-u1", NativeBackend::LlvmApi, Optimization::O2, 1));
        configs.push(("llvmapi-o0-u8", NativeBackend::LlvmApi, Optimization::O0, 8));
        configs.push(("llvmapi-o2-u8", NativeBackend::LlvmApi, Optimization::O2, 8));
    }

    for (label, backend, opt, units) in configs {
        let exe_main = dir.join(format!("main-{label}"));
        let (code0, out0, actual_u, obj_r) =
            build_and_run(&main_entry, &exe_main, backend, opt, units, b"0");
        assert_eq!((code0, out0.as_slice()), (0, b"HZMK".as_slice()), "{label}");
        if backend == NativeBackend::LlvmApi {
            assert_eq!(actual_u, units, "{label}: expected real {units} units");
            assert_eq!(obj_r, units, "{label}: expected {units} object reports");
        }

        let mut child1 = Command::new(&exe_main)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn count-1");
        child1
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"1")
            .expect("write 1");
        let run1 = child1.wait_with_output().expect("wait count-1");
        assert_eq!(
            (run1.status.code(), run1.stdout.as_slice()),
            (Some(0), b"DHZMNK".as_slice()),
            "{label} count-1"
        );

        let exe_fe = dir.join(format!("field-exit-{label}"));
        let (fe_code, fe_out, fe_u, fe_r) =
            build_and_run(&field_exit, &exe_fe, backend, opt, units, b"1");
        assert_eq!(
            (fe_code, fe_out.as_slice()),
            (1, b"DHZ".as_slice()),
            "{label} field_exit"
        );
        if backend == NativeBackend::LlvmApi {
            assert_eq!(fe_u, units, "{label}: field_exit real {units} units");
            assert_eq!(fe_r, units, "{label}: field_exit {units} object reports");
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn safety_negatives_reject_at_exact_phase_and_code_through_full_driver() {
    let root = repo_root();
    let mut session = CompilationSession::new();
    let check_opts = CompileOptions {
        stop_after: Stage::Lower,
        ..CompileOptions::default()
    };

    // 1. Historical unbounded SyncDrop cleanup fixtures -> Phase::Types, E-ORIGINAL-CLEANUP
    for entry in ["main.zil", "field_exit.zil"] {
        let path = root
            .join("tests/fixtures/state-store-without-sync-drop")
            .join(entry);
        let report = session.compile_path(&path, &check_opts);
        assert_eq!(report.status, ResultCategory::SafeReject);
        assert_eq!(report.phase, Phase::Types);
        assert_eq!(report.verification_runs, 0);
        assert!(
            report.diagnostics.iter().any(|d| {
                d.code == "E-ORIGINAL-CLEANUP"
                    && d.message.contains("SyncDrop")
                    && d.notes
                        .iter()
                        .any(|n| n.contains("original scope cleanup binding \"callback\""))
            }),
            "{entry}: unexpected diagnostics {:?}",
            report.diagnostics
        );
    }

    // 2. Rigid-life closure mismatch -> Phase::Types, E-TYPE-MISMATCH (0 false admits)
    let rigid = root.join("tests/fixtures/call-regions/negative-closure-rigid-life.zil");
    let report = session.compile_path(&rigid, &check_opts);
    assert_eq!(report.status, ResultCategory::SafeReject);
    assert_eq!(report.phase, Phase::Types);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TYPE-MISMATCH"),
        "expected E-TYPE-MISMATCH, got {:?}",
        report.diagnostics
    );

    // 3. Task SDK negative controls through full driver (including CT + AST fidelity)
    let worker_neg = root.join(
        "tests/language-readiness/task-owner-transport/invalid-source-contracts/direct-worker-owner-cleanup.zil",
    );
    let report = session.compile_path(&worker_neg, &check_opts);
    assert_eq!(report.status, ResultCategory::SafeReject);
    assert_eq!(report.phase, Phase::Types);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-ORIGINAL-CLEANUP" && d.message.contains("SyncDrop")),
        "expected E-ORIGINAL-CLEANUP SyncDrop, got {:?}",
        report.diagnostics
    );

    let region_neg = root.join(
        "tests/language-readiness/task-owner-transport/invalid-source-contracts/equal-distinct-task-classes.zil",
    );
    let report = session.compile_path(&region_neg, &check_opts);
    assert_eq!(report.status, ResultCategory::SafeReject);
    assert_eq!(report.phase, Phase::Analyze);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-SAFETY-SOLVE" && d.message.contains("RegionEscape")),
        "expected RegionEscape, got {:?}",
        report.diagnostics
    );
}

#[test]
fn stdlib_and_control_programs_execute_natively_on_both_backends_at_o0_and_o2() {
    let dir = temp_dir("stdlib-controls");
    let root = repo_root();
    let cases = [
        (
            "std_store_mut",
            "tests/language-readiness/store-mutation/std-store.zil",
        ),
        (
            "store_mut_direct",
            "tests/language-readiness/store-mutation/direct.zil",
        ),
        (
            "nested_packages",
            "tests/fixtures/nested-store/packages.zil",
        ),
        (
            "static_trait",
            "tests/language-readiness/controls/static-trait-control.zil",
        ),
        (
            "owned_closure",
            "tests/language-readiness/controls/owned-closure-return-control.zil",
        ),
        (
            "mut_store_helper",
            "tests/language-readiness/controls/mutable-store-helper-control.zil",
        ),
        (
            "ct_enum_match",
            "tests/language-readiness/controls/ct-enum-match.zil",
        ),
        (
            "ct_generic",
            "tests/language-readiness/controls/ct-generic.zil",
        ),
    ];

    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }

    for (name, rel) in cases {
        let path = root.join(rel);
        for &backend in &backends {
            for opt in [Optimization::O0, Optimization::O2] {
                let exe = dir.join(format!("{name}-{}-{opt:?}", backend.name()));
                let (code, _, _, _) = build_and_run(&path, &exe, backend, opt, 1, b"");
                assert_eq!(
                    code, 0,
                    "{rel} ({backend:?}, {opt:?}) exited with non-zero status {code}"
                );
            }
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}
