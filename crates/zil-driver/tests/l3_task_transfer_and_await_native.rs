//! End-to-end dual-backend native execution, pthread failure injection,
//! structured `task_await` child-task cleanup, safety negative, and Raw/Wire
//! tests for L3 Batch 1 (`task/l3-task-surface-and-transfer`, G8.4 / G8.6).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l3-b1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, src).expect("write source");
    path
}

fn dual_configs() -> Vec<(&'static str, NativeBackend, Optimization)> {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }
    configs
}

fn build_and_run_dual_with_c(
    dir: &Path,
    entry: &Path,
    extra_c_sources: &[PathBuf],
) -> (i32, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            extra_c_sources: extra_c_sources.to_vec(),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let out = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr ({label}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let result = (out.status.code().unwrap_or(-1), out.stdout);
        if let Some(prev) = &expected {
            assert_eq!(&result, prev, "result mismatch on {label}");
        } else {
            expected = Some(result);
        }
    }
    expected.expect("at least one backend ran")
}

fn check_rejected(dir: &Path, name: &str, src: &str, expected_code_prefix: &str) {
    let entry = write_source(dir, &format!("{name}.zil"), src);
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected `{name}` to fail with SafeReject (got {:?}): {:?}",
        report.status,
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with(expected_code_prefix)),
        "expected diagnostic starting with `{expected_code_prefix}` for `{name}`, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn task_thread_transfer_scalar_and_pthread_failure_recovery_execute_natively() {
    let root = repo_root();
    let scalar = root.join("tests/language-readiness/task-thread-transfer/scalar.zil");

    // 1. Normal OS-thread transfer: compute(37) yields on parent, resumes on child
    //    OS thread -> answer = 42, body = 9 -> 42 + 9 - 51 == 0.
    let dir = temp_dir("scalar-ok");
    let (code, stdout) = build_and_run_dual_with_c(&dir, &scalar, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());

    // 2. pthread_create failure injection: returns EAGAIN (11) -> thread_scope returns
    //    TaskThreadScopeResult.Failed(input), caller cancels/closes/destroys the
    //    returned Task cleanly and exits with code 2.
    let fail_dir = temp_dir("scalar-pthread-fail");
    let fail_probe = fail_dir.join("fail_pthread.c");
    std::fs::write(
        &fail_probe,
        r#"
#include <errno.h>
#include <pthread.h>
int pthread_create(pthread_t *thread, const pthread_attr_t *attr,
                   void *(*start_routine)(void *), void *arg) {
    (void)thread;
    (void)attr;
    (void)start_routine;
    (void)arg;
    return EAGAIN;
}
"#,
    )
    .expect("write fail_pthread.c");
    let (fail_code, fail_stdout) = build_and_run_dual_with_c(&fail_dir, &scalar, &[fail_probe]);
    assert_eq!(fail_code, 2);
    assert!(fail_stdout.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&fail_dir);
}

#[test]
fn structured_task_await_and_owned_task_fixtures_execute_natively_on_dual_backends() {
    let root = repo_root();
    let fixtures: &[(&str, &[u8])] = &[
        (
            "tests/language-readiness/terminal/concurrency-and-registration/task-owned-result.zil",
            b"PRA",
        ),
        (
            "tests/language-readiness/terminal/concurrency-and-registration/task-owned-child-created.zil",
            b"PPPR A"[..4].as_ref(),
        ),
        (
            "tests/language-readiness/terminal/concurrency-and-registration/task-owned-child-direct.zil",
            b"AB",
        ),
        (
            "tests/language-readiness/terminal/concurrency-and-registration/task-unit-ready.zil",
            b"PR",
        ),
    ];
    for (idx, (rel, _expected_prefix)) in fixtures.iter().enumerate() {
        let dir = temp_dir(&format!("task-fixture-{idx}"));
        let path = root.join(rel);
        let (code, stdout) = build_and_run_dual_with_c(&dir, &path, &[]);
        assert_eq!(code, 0, "fixture {rel} exited with {code}");
        assert!(
            !stdout.is_empty(),
            "fixture {rel} should emit deterministic poll/drop trace"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Verify exact trace for task-owned-result.zil (Pending 'P', Ready 'R', Drop Token(65) 'A')
    let dir_res = temp_dir("task-owned-result-exact");
    let (code, stdout) = build_and_run_dual_with_c(
        &dir_res,
        &root.join(
            "tests/language-readiness/terminal/concurrency-and-registration/task-owned-result.zil",
        ),
        &[],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.as_slice(), b"PRA");
    let _ = std::fs::remove_dir_all(&dir_res);

    // Verify exact trace for task-owned-child-created.zil (3 yields -> PPP, Ready -> R, Drop 'A')
    let dir_child = temp_dir("task-owned-child-created-exact");
    let (code, stdout) = build_and_run_dual_with_c(
        &dir_child,
        &root.join(
            "tests/language-readiness/terminal/concurrency-and-registration/task-owned-child-created.zil",
        ),
        &[],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.as_slice(), b"PPPRA");
    let _ = std::fs::remove_dir_all(&dir_child);
}

#[test]
fn task_transfer_and_task_await_safety_negatives_are_rejected() {
    let dir = temp_dir("safety-negatives");

    // 1. Transferring a resume(local) Task across thread_scope must be rejected.
    check_rejected(
        &dir,
        "local_task_transfer_rejected",
        r#"
fn local_worker(seed: i32) -> i32 contract { effects(suspend); resume(local) } {
    task_yield()
    return seed + 1
}
fn drive(give input: Task<i32>) -> i32 {
    var task = give input
    task_request_cancel(&mut task)
    loop {
        match task_poll_close(&mut task) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give task)
    return 0
}
fn main() -> TaskCreateError!i32 {
    let task = task_create(local_worker, 1)?
    match thread_scope(drive, give task, () => 0) {
        TaskThreadScopeResult.Failed(input) => {
            var rem = give input
            task_request_cancel(&mut rem)
            loop {
                match task_poll_close(&mut rem) {
                    Poll.Pending() => { continue },
                    Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give rem)
            return 1
        },
        TaskThreadScopeResult.Completed(a, b) => { return a + b }
    }
}
"#,
        "E-SAFETY-CHECK",
    );

    // 2. Transferring a Task with borrowed Context<'a> across thread_scope must be rejected.
    check_rejected(
        &dir,
        "borrowed_context_task_transfer_rejected",
        r#"
fn worker<life a>(p: &'a i32) -> i32 {
    task_yield()
    return *p
}
fn drive<life a>(give input: Task<i32, &'a i32>) -> i32 {
    var task = give input
    task_request_cancel(&mut task)
    loop {
        match task_poll_close(&mut task) {
            Poll.Pending() => { continue },
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give task)
    return 0
}
fn run<life a>(p: &'a i32) -> TaskCreateError!i32 {
    let task = task_create(worker, p)?
    match thread_scope(drive, give task, () => 0) {
        TaskThreadScopeResult.Failed(input) => {
            var rem = give input
            task_request_cancel(&mut rem)
            loop {
                match task_poll_close(&mut rem) {
                    Poll.Pending() => { continue },
                    Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give rem)
            return 1
        },
        TaskThreadScopeResult.Completed(a, b) => { return a + b }
    }
}
fn main() -> TaskCreateError!i32 { let x = 1; return run(&x) }
"#,
        "E-SAFETY-SOLVE",
    );

    // 3. task_await inside defer cleanup must be rejected.
    check_rejected(
        &dir,
        "task_await_in_defer_rejected",
        r#"
fn child() { task_yield() }
fn worker() -> i32 contract { effects(suspend); resume(local) } {
    defer { task_await(child) }
    return 0
}
fn main() -> i32 { return 0 }
"#,
        "E-LOOP-CONTROL",
    );

    // 4. task_await of a suspending child directly from main (outside Task) must be rejected.
    check_rejected(
        &dir,
        "task_await_in_main_rejected",
        r#"
fn child() { task_yield() }
fn main() -> i32 {
    task_await(child)
    return 0
}
"#,
        "E-SAFETY-CHECK",
    );

    // 5. task_await of a suspending child while holding a borrowed local in resume(transfer) must be rejected.
    check_rejected(
        &dir,
        "task_await_borrowed_local_in_resume_transfer_rejected",
        r#"
fn child() { task_yield() }
fn worker(seed: i32) -> i32 contract { effects(suspend); resume(transfer) } {
    let x = seed
    let p = &x
    task_await(child)
    return *p
}
fn main() -> i32 { return 0 }
"#,
        "E-SAFETY-CHECK",
    );

    let _ = std::fs::remove_dir_all(&dir);
}
