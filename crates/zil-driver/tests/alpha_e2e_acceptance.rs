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
    build_and_run_raw_with_c(entry, exe, backend, opt, &[])
}

fn build_and_run_raw_with_c(
    entry: &Path,
    exe: &Path,
    backend: NativeBackend,
    opt: Optimization,
    extra_c_sources: &[PathBuf],
) -> std::process::Output {
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Build,
        optimization: opt,
        backend,
        output: Some(exe.to_path_buf()),
        extra_c_sources: extra_c_sources.to_vec(),
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
    let dir = temp_dir("task-guard-page");
    let probe_c = dir.join("guard_probe.c");
    std::fs::write(
        &probe_c,
        r#"
#define _GNU_SOURCE
#include <errno.h>
#include <signal.h>
#include <stdint.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>

static volatile uintptr_t g_guard_addr = 0;
static volatile size_t g_guard_len = 0;
static volatile int g_fail_next_guard = 0;
static unsigned char g_altstack[65536];

void probe_fail_next_mprotect(void) {
    g_fail_next_guard = 1;
}

int mprotect(void *addr, size_t len, int prot) {
    if (prot == PROT_NONE) {
        if (g_fail_next_guard) {
            g_fail_next_guard = 0;
            errno = ENOMEM;
            return -1;
        }
        g_guard_addr = (uintptr_t)addr;
        g_guard_len = len;
    }
    return (int)syscall(SYS_mprotect, addr, len, prot);
}

static void segv_handler(int sig, siginfo_t *si, void *ucontext) {
    (void)ucontext;
    uintptr_t fault = (uintptr_t)si->si_addr;
    uintptr_t base = g_guard_addr;
    size_t len = g_guard_len;
    if (sig == SIGSEGV &&
        si->si_code == SEGV_ACCERR &&
        base != 0 &&
        fault >= base &&
        fault < base + len) {
        _exit(77);
    }
    _exit(99);
}

void probe_install_segv_handler(void) {
    stack_t ss;
    ss.ss_sp = g_altstack;
    ss.ss_size = sizeof(g_altstack);
    ss.ss_flags = 0;
    if (sigaltstack(&ss, NULL) != 0) {
        _exit(91);
    }
    struct sigaction sa;
    sa.sa_sigaction = segv_handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    if (sigaction(SIGSEGV, &sa, NULL) != 0) {
        _exit(92);
    }
}
"#,
    )
    .expect("write guard_probe.c");

    // Subtest A: Deep recursion overflows into the exact 4 KiB PROT_NONE guard page
    // and triggers SIGSEGV with si_code == SEGV_ACCERR and fault address in [guard, guard+4096).
    let overflow_src = dir.join("overflow.zil");
    std::fs::write(
        &overflow_src,
        r#"
extern "C" fn probe_install_segv_handler() -> ();

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
    unsafe { probe_install_segv_handler() }
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
    .expect("write overflow.zil");

    // Subtest B: If mprotect fails when installing the guard page, task_poll MUST refuse
    // to start the worker, return err(TaskPollState.Invalid), and allow clean close + destroy.
    let fail_src = dir.join("mprotect_fail.zil");
    std::fs::write(
        &fail_src,
        r#"
extern "C" fn probe_fail_next_mprotect() -> ();
extern "C" fn putchar(value: i32) -> i32;

fn worker(x: i32) -> i32 {
    // Must NEVER execute when guard page mprotect fails!
    unsafe { putchar(88) }
    return x + 1
}

fn main() -> i32 {
    let created = task_create(worker, 5)
    match created {
        .ok(t) => {
            var task = give t
            unsafe { probe_fail_next_mprotect() }
            match task_poll(&mut task) {
                .ok(_) => { return 21 },
                .err(st) => {
                    match st {
                        TaskPollState.Invalid() => {},
                        _ => { return 22 }
                    }
                }
            }
            match task_poll_close(&mut task) {
                Poll.Ready(_) => {},
                Poll.Pending() => { return 23 }
            }
            task_finish_destroy(give task)
            return 0
        },
        .err(_) => { return 20 }
    }
}
"#,
    )
    .expect("write mprotect_fail.zil");

    let extra = vec![probe_c];
    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe_ov = dir.join(format!("overflow-{}-{opt:?}", backend.name()));
            let out_ov = build_and_run_raw_with_c(&overflow_src, &exe_ov, backend, opt, &extra);
            assert_eq!(
                out_ov.status.code(),
                Some(77),
                "overflow ({backend:?}, {opt:?}) expected exit 77 (SEGV_ACCERR inside guard page), got status={:?}",
                out_ov.status
            );

            let exe_mf = dir.join(format!("mprotect-fail-{}-{opt:?}", backend.name()));
            let out_mf = build_and_run_raw_with_c(&fail_src, &exe_mf, backend, opt, &extra);
            assert_eq!(
                out_mf.status.code(),
                Some(0),
                "mprotect_fail ({backend:?}, {opt:?}) expected exit 0, got status={:?}, stderr={}",
                out_mf.status,
                String::from_utf8_lossy(&out_mf.stderr)
            );
            assert!(
                out_mf.stdout.is_empty(),
                "mprotect_fail ({backend:?}, {opt:?}) worker must not execute: stdout={:?}",
                String::from_utf8_lossy(&out_mf.stdout)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn task_concurrent_polling_on_two_os_threads_isolated_tls() {
    let dir = temp_dir("task-tls-threads");
    let probe_c = dir.join("barrier_probe.c");
    std::fs::write(
        &probe_c,
        r#"
#define _POSIX_C_SOURCE 200809L
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>

static pthread_barrier_t g_bar;
static volatile unsigned long g_tid0 = 0;
static volatile unsigned long g_tid1 = 0;

void probe_barrier_init(void) {
    if (pthread_barrier_init(&g_bar, NULL, 2) != 0) {
        abort();
    }
    g_tid0 = 0;
    g_tid1 = 0;
}

void probe_barrier_wait(void) {
    int rc = pthread_barrier_wait(&g_bar);
    if (rc != 0 && rc != PTHREAD_BARRIER_SERIAL_THREAD) {
        abort();
    }
}

void probe_record_tid(int slot) {
    unsigned long self = (unsigned long)pthread_self();
    if (slot == 0) {
        g_tid0 = self;
    } else {
        g_tid1 = self;
    }
}

int probe_tids_distinct(void) {
    return (g_tid0 != 0 && g_tid1 != 0 && g_tid0 != g_tid1) ? 1 : 0;
}
"#,
    )
    .expect("write barrier_probe.c");

    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
extern "C" fn probe_barrier_init() -> ();
extern "C" fn probe_barrier_wait() -> ();
extern "C" fn probe_record_tid(slot: i32) -> ();
extern "C" fn probe_tids_distinct() -> i32;

fn step_worker(seed: i32) -> i32 {
    var acc = 0
    var i = 0
    while i < 40 {
        // Real 2-thread pthread_barrier_wait inside active Task frames:
        // guarantees both OS threads are simultaneously alive and executing Tasks.
        unsafe { probe_barrier_wait() }
        acc += seed + i
        task_yield()
        i += 1
    }
    return acc
}

fn run_task(seed: i32) -> i32 {
    if seed == 100 {
        unsafe { probe_record_tid(0) }
    } else {
        unsafe { probe_record_tid(1) }
    }
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
    unsafe { probe_barrier_init() }
    let (created, child_res, main_res) = thread_scope(run_task, 100, () -> i32 => run_task(200))
    if !created { return 1 }
    var distinct = 0
    unsafe { distinct = probe_tids_distinct() }
    if distinct != 1 { return 4 }
    // sum_{i=0..39} (100 + i) = 4000 + 780 = 4780
    if child_res != 4780 { return 2 }
    // sum_{i=0..39} (200 + i) = 8000 + 780 = 8780
    if main_res != 8780 { return 3 }
    return 0
}
"#,
    )
    .expect("write source");

    let mutex_fixture = repo_root().join("tests/fixtures/threading/mutex-worker-helpers.zil");
    let extra = vec![probe_c];
    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("bin-{}-{opt:?}", backend.name()));
            let out = build_and_run_raw_with_c(&src, &exe, backend, opt, &extra);
            assert_eq!(
                out.status.code(),
                Some(0),
                "tls-threads ({backend:?}, {opt:?}) failed: status={:?}, stderr={}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );

            let exe_m = dir.join(format!("mutex-{}-{opt:?}", backend.name()));
            let out_m = build_and_run_raw(&mutex_fixture, &exe_m, backend, opt);
            assert_eq!(
                out_m.status.code(),
                Some(0),
                "mutex-worker-helpers ({backend:?}, {opt:?}) failed: status={:?}, stderr={}",
                out_m.status,
                String::from_utf8_lossy(&out_m.stderr)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn mutex_destroy_failure_aborts_without_freeing() {
    use std::os::unix::process::ExitStatusExt;

    let dir = temp_dir("mutex-destroy-fail");
    let probe_c = dir.join("mutex_destroy_probe.c");
    std::fs::write(
        &probe_c,
        r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <pthread.h>
#include <stddef.h>
#include <unistd.h>

static volatile void *g_failed_mutex = NULL;

int pthread_mutex_destroy(pthread_mutex_t *m) {
    g_failed_mutex = (void *)m;
    return EBUSY;
}

void free(void *p) {
    if (p != NULL && p == g_failed_mutex) {
        const char msg[] = "FREED_AFTER_DESTROY_FAIL";
        ssize_t ignored = write(1, msg, sizeof(msg) - 1);
        (void)ignored;
        _exit(99);
    }
    void (*real_free)(void *) = (void (*)(void *))dlsym(RTLD_NEXT, "free");
    if (real_free != NULL) {
        real_free(p);
    }
}
"#,
    )
    .expect("write mutex_destroy_probe.c");

    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
fn entry() -> MutexCreateError!i32 {
    let cell = mutex_create(42)?
    var guard = mutex_lock(cell)
    let value = mutex_value(&mut guard)
    *value += 1
    return 0
}

fn main() -> i32 {
    match entry() {
        .ok(v) => { return v },
        .err(_) => { return 10 }
    }
}
"#,
    )
    .expect("write main.zil");

    let extra = vec![probe_c];
    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("bin-{}-{opt:?}", backend.name()));
            let out = build_and_run_raw_with_c(&src, &exe, backend, opt, &extra);
            assert_eq!(
                out.status.signal(),
                Some(6), // SIGABRT
                "({backend:?}, {opt:?}) expected SIGABRT(6) on pthread_mutex_destroy failure, got status={:?}, stdout={:?}",
                out.status,
                String::from_utf8_lossy(&out.stdout)
            );
            assert!(
                out.stdout.is_empty(),
                "({backend:?}, {opt:?}) backing memory must not be freed after destroy failure: stdout={:?}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn everyday_const_defer_slice_and_collection_for_in_execute_natively() {
    let dir = temp_dir("batch1a-everyday");
    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
extern "C" fn putchar(value: i32) -> i32;

const BASE: usize = 2
const LEN: usize = BASE * 2 + 1
const BONUS: i32 = 10

resource Tag { code: i32 }
impl Drop for Tag { fn drop(self: Self) { unsafe { putchar(.code) } } }

struct Counter { cur: i32, limit: i32 }
trait Next { fn next(self: &mut Self) -> Option<i32>; }
impl Next for Counter {
    fn next() -> Option<i32> {
        if .cur >= .limit {
            return Option.None()
        }
        let v = .cur
        .cur += 1
        return Option.Some(v)
    }
}

fn test_defer(early: bool) -> i32 {
    let t1 = Tag { code: 65 } // 'A'
    defer { unsafe { putchar(66) } } // 'B'
    let t2 = Tag { code: 67 } // 'C'
    defer { unsafe { putchar(68) } } // 'D'
    if early {
        return t1.code + t2.code
    }
    unsafe { putchar(69) } // 'E'
    return 0
}

fn main() -> i32 {
    // 1. Const array length + bare slice expressions + for..in over array & slice
    let arr: [i32; LEN] = [1, 2, 3, 4, 5]
    var arr_sum = 0
    for x in arr {
        arr_sum += x
    }
    if arr_sum != 15 { return 1 }

    let mid = arr[1..4]
    var mid_sum = 0
    for x in mid {
        mid_sum += x
    }
    if mid_sum != 9 { return 2 }

    let head = arr[..2]
    let tail = arr[3..]
    let full = arr[..]
    if head.len() != 2 || tail.len() != 2 || full.len() != 5 { return 3 }

    // 2. for..in over Managed<OwnedStore<i32>> (non-consuming)
    let store: Store<i32> = store_new()
    if !store_reserve(store, 3) { return 4 }
    store_push(store, 10)
    store_push(store, 20)
    store_push(store, BONUS)
    var store_sum = 0
    for x in store {
        store_sum += x
    }
    if store_sum != 40 || store_len(store) != 3 { return 7 }

    // 3. for..in over custom iterator with .next() -> Option<i32>
    let counter = Counter { cur: 1, limit: 5 }
    var it_sum = 0
    for x in counter {
        it_sum += x
    }
    if it_sum != 10 { return 8 }

    // 4. defer LIFO interleaved with resource Drop on early return and normal exit
    let r = test_defer(true)
    if r != 132 { return 9 }
    let r2 = test_defer(false)
    if r2 != 0 { return 10 }
    return 0
}
"#,
    )
    .expect("write main.zil");

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
            // Early return: D, C, B, A. Normal exit: E, D, C, B, A.
            assert_eq!(
                out.stdout.as_slice(),
                b"DCBAEDCBA",
                "({backend:?}, {opt:?}) unexpected defer/drop order: {:?}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}
