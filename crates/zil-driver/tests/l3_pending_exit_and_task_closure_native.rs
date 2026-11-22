//! Dual-backend (Clang O0/O2 + LLVM C-API O0/O2) native execution tests for
//! Batch L3-C (G8.4/G8.6 closure):
//! - `PendingExit` cancelled owned-result cleanup (`done_canc`),
//! - `task_create` and `task_create_owned` OOM failure-injection cleanup/recovery (`tc_fail`),
//! - Selective per-field drop on partially-moved enum variants (`inline-task-enum*`),
//! - Inline task package cancel/ready cleanup (`inline-task-package-*`),
//! - Enum task result (`enum-result.zil` + `frame-observer.c`),
//! - Safety negatives for Direct default-Drop of unclosed external `Task<T>`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::{Phase, ResultCategory};
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
    let dir = std::env::temp_dir().join(format!("zil-l3-c-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
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

fn make_wrap_clang(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("clang-wrap.sh");
    std::fs::write(
        &script,
        "#!/usr/bin/env bash\nexec clang -Wl,--wrap=calloc -Wl,--wrap=realloc -Wl,--wrap=free \"$@\"\n",
    )
    .expect("write clang wrapper");
    let mut perms = std::fs::metadata(&script)
        .expect("stat wrapper")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).expect("chmod wrapper");
    script
}

fn build_and_run_dual_env(
    dir: &Path,
    entry: &Path,
    extra_c_sources: &[PathBuf],
    use_wrap_clang: bool,
    envs: &[(&str, &str)],
) -> (i32, Vec<u8>) {
    let clang_opt = if use_wrap_clang {
        Some(make_wrap_clang(dir))
    } else {
        None
    };
    let mut expected: Option<(i32, Vec<u8>)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            clang: clang_opt.clone(),
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

        let mut cmd = Command::new(&exe);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr for {} ({label}): {}",
            entry.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let result = (out.status.code().unwrap_or(-1), out.stdout);
        if let Some(prev) = &expected {
            assert_eq!(
                &result,
                prev,
                "backend/opt mismatch for {} on {label}",
                entry.display()
            );
        } else {
            expected = Some(result);
        }
    }
    expected.expect("at least one backend ran")
}

#[test]
fn l3_inline_task_enum_and_selective_variants_native() {
    let root = repo_root().join("tests/language-readiness/task-aggregate-results");
    let observer = root.join("inline-task-enum-observer.c");

    for (name, file) in [
        ("inline_task_enum", "inline-task-enum.zil"),
        (
            "inline_task_enum_selective",
            "inline-task-enum-selective.zil",
        ),
        (
            "inline_task_enum_selective_failure",
            "inline-task-enum-selective-failure.zil",
        ),
    ] {
        let dir = temp_dir(name);
        let (code, stdout) = build_and_run_dual_env(
            &dir,
            &root.join(file),
            std::slice::from_ref(&observer),
            true,
            &[],
        );
        assert_eq!(code, 0, "{file} exited with {code}");
        let text = String::from_utf8_lossy(&stdout);
        assert!(
            text.starts_with("AB selected child closed"),
            "unexpected stdout for {file}: {text:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn l3_inline_task_package_cancel_and_ready_native() {
    let root = repo_root().join("tests/language-readiness/task-aggregate-results");
    let observer = root.join("inline-task-enum-observer.c");

    for (name, file) in [
        (
            "inline_task_package_cancel",
            "inline-task-package-cancel.zil",
        ),
        ("inline_task_package_ready", "inline-task-package-ready.zil"),
    ] {
        let dir = temp_dir(name);
        let (code, stdout) = build_and_run_dual_env(
            &dir,
            &root.join(file),
            std::slice::from_ref(&observer),
            true,
            &[],
        );
        assert_eq!(code, 0, "{file} exited with {code}");
        let text = String::from_utf8_lossy(&stdout);
        assert!(
            text.starts_with("AB selected child closed"),
            "unexpected stdout for {file}: {text:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn l3_enum_result_with_frame_observer_native() {
    let root = repo_root().join("tests/language-readiness/task-aggregate-results");
    let observer = root.join("frame-observer.c");
    let dir = temp_dir("enum_result");
    let (code, stdout) =
        build_and_run_dual_env(&dir, &root.join("enum-result.zil"), &[observer], true, &[]);
    assert_eq!(code, 0, "enum-result.zil exited with {code}");
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        "PRA",
        "expected Pending ('P'), Ready ('R'), Token drop ('A')"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn l3_pending_exit_cancelled_owned_result_dropped_once_native() {
    // Case 1: Cancelled at task_yield() BEFORE constructing the owned result (Trap(Cancel)).
    // Must NOT run ResultToken::drop on dummy zeroinitializer.
    {
        let dir = temp_dir("pending_exit_canc_before_result");
        let entry = dir.join("main.zil");
        let src = r#"
extern "C" fn putchar(code: i32) -> i32;

resource ResultToken { code: i32 }
impl Drop for ResultToken {
    fn drop(self: Self) {
        unsafe { putchar(.code); }
    }
}

fn worker() -> Managed<ResultToken> {
    task_yield();
    let tok = ResultToken { code: 82 };
    return give tok;
}

fn main() -> TaskCreateError!i32 {
    var task = task_create(worker)?;
    match task_poll(&mut task) {
        .err(_) => { return 90; }
        .ok(p) => {
            match p {
                Poll.Pending() => { unsafe { putchar(80); } }
                Poll.Ready(_) => { return 91; }
            }
        }
    }
    task_request_cancel(&mut task);
    loop {
        match task_poll_close(&mut task) {
            Poll.Pending() => { continue; }
            Poll.Ready(_) => { break; }
        }
    }
    task_finish_destroy(give task);
    unsafe { putchar(68); }
    return 0;
}
"#;
        std::fs::write(&entry, src).expect("write cancel-before-result test");
        let (code, stdout) = build_and_run_dual_env(&dir, &entry, &[], false, &[]);
        assert_eq!(
            code, 0,
            "pending_exit_canc_before_result exited with {code}"
        );
        assert_eq!(
            String::from_utf8_lossy(&stdout),
            "PD",
            "expected no bogus drop when cancelled via Trap(Cancel) before constructing result"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Case 2: Cancelled during PendingExit lexical cleanup (Drop of local guard yields)
    // AFTER preparing the owned result: worker finishes via normal Return while st == 4,
    // so done_canc must drop the prepared ResultToken exactly once.
    {
        let dir = temp_dir("pending_exit_canc_result");
        let entry = dir.join("main.zil");
        let src = r#"
extern "C" fn putchar(code: i32) -> i32;

resource ResultToken { code: i32 }
impl Drop for ResultToken {
    fn drop(self: Self) {
        unsafe { putchar(.code); }
    }
}

resource PendingGuard { code: i32 }
impl Drop for PendingGuard {
    fn drop(self: Self) {
        task_yield();
        unsafe { putchar(.code); }
    }
}

fn worker() -> Managed<ResultToken> {
    let guard = PendingGuard { code: 71 };
    let tok = ResultToken { code: 82 };
    return give tok;
}

fn main() -> TaskCreateError!i32 {
    var task = task_create(worker)?;
    match task_poll(&mut task) {
        .err(_) => { return 90; }
        .ok(p) => {
            match p {
                Poll.Pending() => { unsafe { putchar(80); } }
                Poll.Ready(_) => { return 91; }
            }
        }
    }
    task_request_cancel(&mut task);
    loop {
        match task_poll_close(&mut task) {
            Poll.Pending() => { continue; }
            Poll.Ready(_) => { break; }
        }
    }
    task_finish_destroy(give task);
    unsafe { putchar(68); }
    return 0;
}
"#;
        std::fs::write(&entry, src).expect("write pending_exit test");
        let (code, stdout) = build_and_run_dual_env(&dir, &entry, &[], false, &[]);
        assert_eq!(code, 0, "pending_exit_canc_result exited with {code}");
        // 'P' (Pending during PendingGuard::drop yield), 'R' (ResultToken dropped once during done_canc), 'D' (Done).
        // PendingGuard::drop exits via Trap(Cancel) when resumed under cancel, skipping putchar('G'),
        // while worker finishes its pending return with the prepared ResultToken -> done_canc drops 'R' once.
        let out_str = String::from_utf8_lossy(&stdout);
        assert!(
            out_str == "PRD" || out_str == "PGRD",
            "expected cancelled owned result to be dropped exactly once before destroy, got {out_str:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn l3_task_create_and_owned_oom_cleanup_native() {
    let root = repo_root().join("tests/language-readiness/task-aggregate-results");
    let observer = root.join("frame-observer.c");

    // 1. `task_create` OOM: moved owned arg must be dropped exactly once on calloc == NULL.
    {
        let dir = temp_dir("task_create_oom");
        let entry = dir.join("main.zil");
        let src = r#"
extern "C" fn putchar(code: i32) -> i32;

resource ArgToken { code: i32 }
impl Drop for ArgToken {
    fn drop(self: Self) {
        unsafe { putchar(.code); }
    }
}

fn worker(give tok: ArgToken) -> i32 {
    task_yield();
    return tok.code;
}

fn main() -> i32 {
    let tok = ArgToken { code: 79 };
    match task_create(worker, give tok) {
        .err(_) => {
            unsafe { putchar(69); }
            return 0;
        }
        .ok(t) => {
            var task = give t;
            task_request_cancel(&mut task);
            loop {
                match task_poll_close(&mut task) {
                    Poll.Pending() => { continue; }
                    Poll.Ready(_) => { break; }
                }
            }
            task_finish_destroy(give task);
            return 1;
        }
    }
}
"#;
        std::fs::write(&entry, src).expect("write task_create_oom test");
        let (code, stdout) = build_and_run_dual_env(
            &dir,
            &entry,
            std::slice::from_ref(&observer),
            true,
            &[("ZIL_TASK_AGGREGATE_FAIL", "1")],
        );
        assert_eq!(code, 0, "task_create OOM test exited with {code}");
        // 'O' (ArgToken dropped once in tc_fail), 'E' (.err branch entered).
        assert_eq!(
            String::from_utf8_lossy(&stdout),
            "OE",
            "expected moved owned arg to be dropped once on task_create OOM"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 2. `task_create_owned` OOM: moved owned arg is returned in failure.inputs (NOT dropped in tc_fail),
    //    caller observes its value and drops it once at scope end.
    {
        let dir = temp_dir("task_create_owned_oom");
        let entry = dir.join("main.zil");
        let src = r#"
extern "C" fn putchar(code: i32) -> i32;

resource ArgToken { code: i32 }
impl Drop for ArgToken {
    fn drop(self: Self) {
        unsafe { putchar(.code); }
    }
}

fn worker(give tok: ArgToken) -> i32 {
    task_yield();
    return tok.code;
}

fn main() -> i32 {
    let tok = ArgToken { code: 84 };
    match task_create_owned(worker, give tok) {
        .err(failure) => {
            let (recovered,) = failure.inputs;
            unsafe { putchar(82); }
            if recovered.code != 84 {
                return 2;
            }
            return 0;
        }
        .ok(t) => {
            var task = give t;
            task_request_cancel(&mut task);
            loop {
                match task_poll_close(&mut task) {
                    Poll.Pending() => { continue; }
                    Poll.Ready(_) => { break; }
                }
            }
            task_finish_destroy(give task);
            return 1;
        }
    }
}
"#;
        std::fs::write(&entry, src).expect("write task_create_owned_oom test");
        let (code, stdout) = build_and_run_dual_env(
            &dir,
            &entry,
            &[observer],
            true,
            &[("ZIL_TASK_AGGREGATE_FAIL", "1")],
        );
        assert_eq!(code, 0, "task_create_owned OOM test exited with {code}");
        // 'R' (recovered in .err arm before drop), 'T' (recovered ArgToken dropped once on return).
        assert_eq!(
            String::from_utf8_lossy(&stdout),
            "RT",
            "expected task_create_owned OOM to recover owned arg in failure.inputs and drop it once"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn l3_direct_default_drop_of_unclosed_task_safety_negatives() {
    let cases = [
        (
            "direct_unclosed_task_param",
            r#"
fn parent(give child: Task<i32>) -> i32 {
    return 0;
}
fn main() -> i32 { return 0; }
"#,
        ),
        (
            "direct_unclosed_task_in_package_param",
            r#"
package Holder { manages child: Task<i32> }
fn parent(holder: Holder) -> i32 {
    return 0;
}
fn main() -> i32 { return 0; }
"#,
        ),
        (
            "direct_uncalled_callable_retaining_task_param",
            r#"
fn keep(give original: Task<i32>) -> impl FnMut() -> i32 {
    var task = give original;
    return move() -> i32 => {
        match task_poll(&mut task) {
            .ok(p) => { match p { Poll.Pending() => { return -1; }, Poll.Ready(v) => { return v; } } },
            .err(_) => { return -2; }
        }
    };
}
fn parent(give original: Task<i32>) -> i32 {
    var cb = keep(give original);
    return 0;
}
fn main() -> i32 { return 0; }
"#,
        ),
    ];

    for (name, src) in cases {
        let dir = temp_dir(name);
        let entry = dir.join("neg.zil");
        std::fs::write(&entry, src).expect("write negative test");
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &entry,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name} must SafeReject, got {:?}: {:?}",
            report.status,
            report.diagnostics
        );
        assert!(
            report.diagnostics.iter().any(|d| d.phase == Phase::Types
                && d.message
                    .contains("Closed fact or a Suspend execution root")),
            "{name} expected direct default-Drop diagnostic, got {:?}",
            report.diagnostics
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
