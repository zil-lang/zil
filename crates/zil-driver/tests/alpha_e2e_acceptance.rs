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

fn build_and_run_raw(
    entry: &Path,
    exe: &Path,
    backend: NativeBackend,
    opt: Optimization,
) -> std::process::Output {
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Build,
        optimization: opt,
        backend,
        output: Some(exe.to_path_buf()),
        codegen: CodegenOptions { units: 1, jobs: 1 },
        ..CompileOptions::default()
    };
    let report = session.compile_path(entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "build failed for {} ({backend:?}, {opt:?}): {:?}",
        entry.display(),
        report.diagnostics
    );
    assert_eq!(report.verification_runs, 1);

    Command::new(exe)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()))
}

fn dual_backends() -> Vec<NativeBackend> {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    backends
}

#[test]
fn task_cancel_pending_unwinds_resources_and_closures_in_reverse_order() {
    let dir = temp_dir("task-cancel-unwind");
    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
extern "C" fn putchar(value: i32) -> i32;

resource Guard { code: i32 }
impl Drop for Guard { fn drop(self: Self) { unsafe { putchar(.code) } } }

fn worker(arg: i32) -> i32 {
    let g1 = Guard { code: 49 }
    let g2 = Guard { code: 50 }
    let cap = Guard { code: 51 }
    let cls = move () -> i32 => cap.code + arg
    unsafe { putchar(83) }
    task_yield()
    unsafe { putchar(88) }
    return cls() + g1.code + g2.code
}

fn main() -> i32 {
    let created = task_create(worker, 1)
    match created {
        .ok(t) => {
            var task = give t
            match task_poll(&mut task) {
                .ok(v) => { match v { Poll.Pending() => {}, Poll.Ready(_) => { return 11 } } },
                .err(_) => { return 12 }
            }
            task_request_cancel(&mut task)
            match task_poll_close(&mut task) {
                Poll.Ready(_) => {},
                Poll.Pending() => { return 13 }
            }
            task_finish_destroy(give task)
            return 0
        },
        .err(_) => { return 10 }
    }
}
"#,
    )
    .expect("write source");

    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("bin-{}-{opt:?}", backend.name()));
            let out = build_and_run_raw(&src, &exe, backend, opt);
            assert_eq!(
                out.status.code(),
                Some(0),
                "({backend:?}, {opt:?}) failed: status={:?}, stderr={}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
            // 'S' before yield, then reverse drop order: cls(cap='3'), g2='2', g1='1'; no 'X'.
            assert_eq!(
                out.stdout.as_slice(),
                b"S321",
                "({backend:?}, {opt:?}) unexpected drop trace: {:?}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn task_cancel_pending_propagates_pending_and_cleans_nested_task() {
    let dir = temp_dir("task-nested-pending-close");
    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
extern "C" fn putchar(value: i32) -> i32;

resource Guard { code: i32 }
impl Drop for Guard { fn drop(self: Self) { unsafe { putchar(.code) } } }

resource YieldingGuard { code: i32 }
impl Drop for YieldingGuard {
    fn drop(self: Self) {
        unsafe { putchar(.code) }
        task_yield()
        unsafe { putchar(.code + 32) }
    }
}

fn child_worker(x: i32) -> i32 {
    let cg = YieldingGuard { code: 67 }
    task_yield()
    unsafe { putchar(88) }
    return cg.code + x
}

fn parent_worker(x: i32) -> i32 {
    let pg = Guard { code: 80 }
    let created = task_create(child_worker, x)
    match created {
        .ok(t) => {
            var child = give t
            match task_poll(&mut child) {
                .ok(v) => { match v { Poll.Pending() => {}, Poll.Ready(_) => { return -2 } } },
                .err(_) => { return -3 }
            }
            task_yield()
            unsafe { putchar(89) }
            match task_poll_close(&mut child) {
                Poll.Ready(_) => {},
                Poll.Pending() => {}
            }
            task_finish_destroy(give child)
            return pg.code
        },
        .err(_) => { return -1 }
    }
}

fn main() -> i32 {
    let created = task_create(parent_worker, 7)
    match created {
        .ok(t) => {
            var parent = give t
            match task_poll(&mut parent) {
                .ok(v) => { match v { Poll.Pending() => {}, Poll.Ready(_) => { return 11 } } },
                .err(_) => { return 12 }
            }
            task_request_cancel(&mut parent)
            match task_poll_close(&mut parent) {
                Poll.Pending() => {},
                Poll.Ready(_) => { return 13 }
            }
            unsafe { putchar(77) }
            match task_poll_close(&mut parent) {
                Poll.Ready(_) => {},
                Poll.Pending() => { return 14 }
            }
            task_finish_destroy(give parent)
            return 0
        },
        .err(_) => { return 10 }
    }
}
"#,
    )
    .expect("write source");

    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("bin-{}-{opt:?}", backend.name()));
            let out = build_and_run_raw(&src, &exe, backend, opt);
            assert_eq!(
                out.status.code(),
                Some(0),
                "({backend:?}, {opt:?}) failed: status={:?}, stderr={}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
            // Child starts YieldingGuard drop ('C') and yields -> parent task_poll_close returns Pending
            // -> main prints 'M' -> second task_poll_close resumes child drop ('c'), frees child,
            // drops parent Guard ('P'), returns Ready. No 'X' or 'Y'.
            assert_eq!(
                out.stdout.as_slice(),
                b"CMcP",
                "({backend:?}, {opt:?}) unexpected nested cancel trace: {:?}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn task_finish_destroy_aborts_if_not_closed() {
    use std::os::unix::process::ExitStatusExt;

    let dir = temp_dir("task-destroy-abort");
    let cases = [
        (
            "pending",
            r#"
fn worker(x: i32) -> i32 {
    task_yield()
    return x + 1
}

fn main() -> i32 {
    let created = task_create(worker, 5)
    match created {
        .ok(t) => {
            var task = give t
            match task_poll(&mut task) {
                .ok(v) => { match v { Poll.Pending() => {}, Poll.Ready(_) => { return 11 } } },
                .err(_) => { return 12 }
            }
            // Deliberately skip task_poll_close: must abort!
            task_finish_destroy(give task)
            return 0
        },
        .err(_) => { return 10 }
    }
}
"#,
        ),
        (
            "unstarted",
            r#"
fn worker(x: i32) -> i32 {
    return x + 1
}

fn main() -> i32 {
    let created = task_create(worker, 5)
    match created {
        .ok(t) => {
            var task = give t
            // Deliberately skip task_poll_close on unstarted task: must abort!
            task_finish_destroy(give task)
            return 0
        },
        .err(_) => { return 10 }
    }
}
"#,
        ),
    ];

    for (case_name, code) in cases {
        let src = dir.join(format!("{case_name}.zil"));
        std::fs::write(&src, code).expect("write source");
        for backend in dual_backends() {
            for opt in [Optimization::O0, Optimization::O2] {
                let exe = dir.join(format!("{case_name}-{}-{opt:?}", backend.name()));
                let out = build_and_run_raw(&src, &exe, backend, opt);
                assert_eq!(
                    out.status.signal(),
                    Some(6), // SIGABRT
                    "{case_name} ({backend:?}, {opt:?}) expected SIGABRT(6), got status={:?}",
                    out.status
                );
            }
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn task_stack_overflow_hits_guard_page_safely() {
    use std::os::unix::process::ExitStatusExt;

    let dir = temp_dir("task-guard-page");
    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
fn burn_stack(depth: i32) -> i32 {
    if depth <= 0 {
        return 1
    }
    let left = burn_stack(depth - 1)
    let right = burn_stack(depth - 1)
    return left + right
}

fn worker(depth: i32) -> i32 {
    return burn_stack(depth)
}

fn main() -> i32 {
    let created = task_create(worker, 5000)
    match created {
        .ok(t) => {
            var task = give t
            match task_poll(&mut task) {
                .ok(v) => { match v { Poll.Ready(res) => { return res }, Poll.Pending() => { return 11 } } },
                .err(_) => { return 12 }
            }
        },
        .err(_) => { return 10 }
    }
}
"#,
    )
    .expect("write source");

    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("bin-{}-{opt:?}", backend.name()));
            let out = build_and_run_raw(&src, &exe, backend, opt);
            assert_eq!(
                out.status.signal(),
                Some(11), // SIGSEGV on PROT_NONE guard page
                "({backend:?}, {opt:?}) expected SIGSEGV(11) from guard page, got status={:?}",
                out.status
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn task_concurrent_polling_on_two_os_threads_isolated_tls() {
    let dir = temp_dir("task-tls-threads");
    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
fn step_worker(seed: i32) -> i32 {
    var acc = 0
    var i = 0
    while i < 40 {
        acc += seed + i
        task_yield()
        i += 1
    }
    return acc
}

fn run_task(seed: i32) -> i32 {
    let created = task_create(step_worker, seed)
    match created {
        .ok(t) => {
            var task = give t
            var result = -1
            var done = false
            while !done {
                match task_poll(&mut task) {
                    .ok(v) => {
                        match v {
                            Poll.Pending() => {},
                            Poll.Ready(res) => {
                                result = res
                                done = true
                            }
                        }
                    },
                    .err(_) => { return -11 }
                }
            }
            match task_poll_close(&mut task) {
                Poll.Ready(_) => {},
                Poll.Pending() => { return -12 }
            }
            task_finish_destroy(give task)
            return result
        },
        .err(_) => { return -10 }
    }
}

fn main() -> i32 {
    let (created, child_res, main_res) = thread_scope(run_task, 100, () -> i32 => run_task(200))
    if !created { return 1 }
    // sum_{i=0..39} (100 + i) = 4000 + 780 = 4780
    if child_res != 4780 { return 2 }
    // sum_{i=0..39} (200 + i) = 8000 + 780 = 8780
    if main_res != 8780 { return 3 }
    return 0
}
"#,
    )
    .expect("write source");

    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("bin-{}-{opt:?}", backend.name()));
            let out = build_and_run_raw(&src, &exe, backend, opt);
            assert_eq!(
                out.status.code(),
                Some(0),
                "({backend:?}, {opt:?}) failed: status={:?}, stderr={}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}
