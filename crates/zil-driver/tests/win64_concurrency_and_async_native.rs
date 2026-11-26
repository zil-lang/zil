//! Windows x64 W3 concurrency and async runtime verification suite.
//!
//! Covers:
//! - Win32 `SRWLOCK` (`Mutex<T>` / affine `Guard` / `mutex_scope` contention + owned Drop)
//! - UCRT `_beginthreadex` + Win32 `WaitForSingleObject` / `CloseHandle` (`thread_scope`)
//! - Win32 Fibers (`CreateFiberEx` with `FIBER_FLAG_FLOAT_SWITCH`, `SwitchToFiber`,
//!   `DeleteFiber`, `@__zil_ensure_fiber`) for `Task` yield, `f64` FP preservation,
//!   `PendingExit` single-`Drop`, nested child task cancel, 40-round fiber stress,
//!   and cross-thread `Task` migration across `thread_scope`
//! - `AsyncRegistration` 5-state CAS + late-notification rejection + drain with a
//!   real Win32 `_beginthreadex` background callback thread
//! - Paired concurrency/async safety negatives rejected before `Codegen`

use std::path::{Path, PathBuf};
use zil_backend_llvm::{NativeBackend, emit_verified_body_for_target};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::body_pipeline::verify_body_program;
use zil_driver::{CompileOptions, Stage, compile_source};

#[cfg(target_os = "windows")]
use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use zil_driver::{CodegenOptions, CompilationSession, Optimization};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

const WIN64_MUTEX_OWNED_RESOURCE_PROGRAM: &str = r#"
extern "C" fn win32_obs_drop(tag: i32);
extern "C" fn win32_obs_total() -> i32;

resource Vault {
    val: i32;
}

impl Drop for Vault {
    fn drop(self: Self) {
        unsafe { win32_obs_drop(self.val) }
    }
}

package SharedBox {
    manages bytes: ByteBuffer;
    pub count: i32;
}

fn worker(m: Mutex<SharedBox>) -> i32 {
    var g = mutex_lock(m)
    let p = mutex_value(&mut g)
    p.count = p.count + 15
    let out = p.count
    mutex_unlock(g)
    return out
}

fn main() -> i32 {
    var local_val: i32 = 0
    match mutex_create(Vault { val: 11 }) {
        .ok(mv) => {
            var gv = mutex_lock(mv)
            let pv = mutex_value(&mut gv)
            pv.val = pv.val + 11
            local_val = pv.val
            mutex_unlock(gv)
        }
        .err(_) => { local_val = 0 }
    }

    var sum: i32 = 0
    let buf = bytes_new()
    match mutex_create(SharedBox { bytes: give buf, count: 7 }) {
        .ok(m) => {
            let (ok, r1, r2) = thread_scope(worker, m, () -> i32 => 5)
            if ok {
                sum = r1 + r2
            }
        }
        .err(_) => { sum = 0 }
    }

    var drops: i32 = 0
    unsafe { drops = win32_obs_total() }
    // local_val = 22, Vault drop adds 22 -> drops = 22,
    // worker sets SharedBox.count = 22 and returns 22, parent returns 5 -> sum = 27.
    // Total = 22 + 27 = 49 (plus drops == 22 -> verify drops == local_val, return 49).
    if drops != local_val {
        return 1
    }
    return sum + drops
}
"#;

const WIN64_MUTEX_SRWLOCK_CONTENTION_PROGRAM: &str = r#"
fn hammer(cell: Mutex<i32>) -> i32 {
    var i: i32 = 0
    while i < 10000 {
        var g = mutex_lock(cell)
        let p = mutex_value(&mut g)
        *p = *p + 1
        mutex_unlock(g)
        i = i + 1
    }
    return 0
}

fn main() -> MutexCreateError!i32 {
    let cell = mutex_create(0)?
    let (created, child, parent) = thread_scope(hammer, cell, () -> i32 => hammer(cell))
    if !created {
        return 1
    }
    let total = mutex_with(cell, (v: &mut i32) -> i32 => *v)
    if total != 20000 || child != 0 || parent != 0 {
        return 2
    }
    return 0
}
"#;

const WIN64_TASK_FIBER_FP_CANCEL_AND_STRESS_PROGRAM: &str = r#"
extern "C" fn win32_obs_drop(tag: i32);
extern "C" fn win32_obs_total() -> i32;
extern "C" fn win32_obs_count() -> i32;
extern "C" fn win32_obs_reset();

resource FiberToken {
    tag: i32;
}

impl Drop for FiberToken {
    fn drop(self: Self) {
        unsafe { win32_obs_drop(self.tag) }
    }
}

fn fp_worker(seed: f64) -> f64 {
    var acc: f64 = seed + 1.25
    task_yield()
    acc = acc * 2.0
    task_yield()
    acc = acc + 3.5
    task_yield()
    return acc
}

fn cancel_owned_worker(tag: i32) -> i32 {
    let tok = FiberToken { tag: tag }
    task_yield()
    return tok.tag + 100
}

fn inner_child(tag: i32) -> i32 {
    let tok = FiberToken { tag: tag }
    task_yield()
    task_yield()
    return tok.tag
}

fn outer_parent() -> i32 {
    let outer_tok = FiberToken { tag: 11 }
    match task_create(inner_child, 13) {
        .ok(child) => {
            var c = give child
            match task_poll(&mut c) {
                .ok(_) => {}
                .err(_) => {}
            }
            task_yield()
            task_request_cancel(&mut c)
            loop {
                match task_poll_close(&mut c) {
                    Poll.Pending() => { continue }
                    Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give c)
        }
        .err(_) => {}
    }
    return outer_tok.tag
}

fn stress_worker(n: i32) -> i32 {
    task_yield()
    return n + 1
}

fn main() -> i32 {
    // 1. Verify f64 register preservation across 3 SwitchToFiber yields:
    //    ((2.5 + 1.25) * 2.0) + 3.5 = 11.0
    var fp_ok: i32 = 0
    match task_create(fp_worker, 2.5) {
        .ok(t) => {
            var h = give t
            loop {
                match task_poll(&mut h) {
                    .ok(p) => {
                        match p {
                            Poll.Pending() => { continue }
                            Poll.Ready(v) => {
                                if v == 11.0 {
                                    fp_ok = 10
                                }
                                break
                            }
                        }
                    }
                    .err(_) => { break }
                }
            }
            task_request_cancel(&mut h)
            loop {
                match task_poll_close(&mut h) {
                    Poll.Pending() => { continue }
                    Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give h)
        }
        .err(_) => {}
    }
    if fp_ok != 10 {
        return 1
    }

    // 2. Verify PendingExit cancellation drops owned FiberToken(7) exactly once.
    unsafe { win32_obs_reset() }
    match task_create(cancel_owned_worker, 7) {
        .ok(t) => {
            var h = give t
            match task_poll(&mut h) {
                .ok(_) => {}
                .err(_) => { return 2 }
            }
            task_request_cancel(&mut h)
            loop {
                match task_poll_close(&mut h) {
                    Poll.Pending() => { continue }
                    Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give h)
        }
        .err(_) => { return 3 }
    }
    var c1: i32 = 0
    var s1: i32 = 0
    unsafe {
        c1 = win32_obs_count()
        s1 = win32_obs_total()
    }
    if c1 != 1 || s1 != 7 {
        return 4
    }

    // 3. Verify nested child Task cancellation while parent is suspended drops
    //    both inner (13) and outer (11) exactly once (count == 2, total == 24).
    unsafe { win32_obs_reset() }
    match task_create(outer_parent) {
        .ok(t) => {
            var h = give t
            match task_poll(&mut h) {
                .ok(_) => {}
                .err(_) => { return 5 }
            }
            task_request_cancel(&mut h)
            loop {
                match task_poll_close(&mut h) {
                    Poll.Pending() => { continue }
                    Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give h)
        }
        .err(_) => { return 6 }
    }
    var c2: i32 = 0
    var s2: i32 = 0
    unsafe {
        c2 = win32_obs_count()
        s2 = win32_obs_total()
    }
    if c2 != 2 || s2 != 24 {
        return 7
    }

    // 4. 40-round fiber create/poll/cancel + create/poll/ready stress loop
    //    (proves DeleteFiber reclaims OS fiber stacks cleanly).
    var round: i32 = 0
    var ready_acc: i32 = 0
    while round < 40 {
        match task_create(stress_worker, round) {
            .ok(t) => {
                var h = give t
                match task_poll(&mut h) {
                    .ok(_) => {}
                    .err(_) => { return 8 }
                }
                if (round % 2) == 0 {
                    match task_poll(&mut h) {
                        .ok(p) => {
                            match p {
                                Poll.Pending() => { return 9 }
                                Poll.Ready(v) => { ready_acc = ready_acc + v }
                            }
                        }
                        .err(_) => { return 10 }
                    }
                }
                task_request_cancel(&mut h)
                loop {
                    match task_poll_close(&mut h) {
                        Poll.Pending() => { continue }
                        Poll.Ready(_) => { break }
                    }
                }
                task_finish_destroy(give h)
            }
            .err(_) => { return 11 }
        }
        round = round + 1
    }
    // Even rounds: 0, 2, ..., 38 -> sum of (2k + 1) for k=0..19 is 20^2 = 400.
    if ready_acc != 400 {
        return 12
    }
    return 0
}
"#;

const WIN64_ASYNC_REG_BG_THREAD_PROGRAM: &str = r#"
extern "C" fn win32_areg_register(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle contract {
    provides(static_callbacks(), async_callback_register());
    effects(ffi);
};
extern "C" fn win32_areg_request_close(token: c_handle) contract {
    provides(static_callbacks(), async_callback_request_close());
    effects(ffi);
};
extern "C" fn win32_areg_poll_drain(token: c_handle) -> i32 contract {
    provides(static_callbacks(), async_callback_poll_drain());
    effects(ffi);
};
extern "C" fn win32_areg_release_bg() contract {
    effects(ffi);
};
extern "C" fn win32_areg_note_drop(code: i32) contract {
    effects(ffi);
};
extern "C" fn win32_areg_verify() -> i32 contract {
    effects(ffi);
};

resource RegMark {
    code: i32;
}

impl Drop for RegMark {
    fn drop(self: Self) {
        unsafe { win32_areg_note_drop(self.code) }
    }
}

fn async_worker(base: i32) -> i32 {
    let mark = RegMark { code: 37 }
    var acc: i32 = base
    let cb = move (delta: i32) -> i32 => {
        acc = acc + delta
        return acc + mark.code
    }
    unsafe {
        var reg = c_register_async(
            win32_areg_register,
            win32_areg_request_close,
            win32_areg_poll_drain,
            cb,
        )
        if !async_registration_valid(reg) {
            return 99
        }
        task_yield()
    }
    return 0
}

fn main() -> std.task.TaskCreateError!i32 {
    var task = task_create(async_worker, 5)?
    // First poll registers async callback on background Win32 thread and yields.
    match task_poll(&mut task) {
        .ok(_) => {}
        .err(_) => {
            task_finish_destroy(give task)
            return 10
        }
    }
    // Second poll resumes past task_yield, initiating async registration close.
    // The background thread holds the queued event until win32_areg_release_bg(),
    // so __zil_areg_drop_frame yields via SwitchToFiber (Poll.Pending).
    match task_poll(&mut task) {
        .ok(p) => {
            match p {
                std.task.Poll.Pending() => {}
                std.task.Poll.Ready(_) => {
                    task_finish_destroy(give task)
                    return 11
                }
            }
        }
        .err(_) => {
            task_finish_destroy(give task)
            return 12
        }
    }
    // Allow background Win32 thread to fire its late callback (must be rejected)
    // and finish draining.
    unsafe { win32_areg_release_bg() }
    // Resume task until __zil_areg_drop_frame finishes draining and drops RegMark once.
    loop {
        match task_poll(&mut task) {
            .ok(p) => {
                match p {
                    std.task.Poll.Pending() => { continue }
                    std.task.Poll.Ready(_) => { break }
                }
            }
            .err(_) => {
                task_finish_destroy(give task)
                return 13
            }
        }
    }
    match task_poll_close(&mut task) {
        std.task.Poll.Pending() => {
            task_finish_destroy(give task)
            return 14
        }
        std.task.Poll.Ready(_) => {}
    }
    task_finish_destroy(give task)
    unsafe { return win32_areg_verify() }
}
"#;

const WIN32_CONCURRENCY_FIXTURES: &[&str] = &[
    "tests/language-readiness/terminal/concurrency-and-registration/sync-probes/mutex-owned-return.zil",
    "tests/language-readiness/terminal/concurrency-and-registration/sync-probes/mutex-protected-owner-thread.zil",
    "tests/language-readiness/terminal/concurrency-and-registration/atomic-scope.zil",
    "tests/language-readiness/terminal/concurrency-and-registration/atomic-contention.zil",
    "tests/language-readiness/terminal/concurrency-and-registration/cas-contention.zil",
    "tests/language-readiness/task-thread-transfer/scalar.zil",
    "tests/language-readiness/terminal/concurrency-and-registration/task-owned-result.zil",
    "tests/language-readiness/terminal/concurrency-and-registration/task-owned-child-created.zil",
    "tests/language-readiness/terminal/concurrency-and-registration/task-owned-child-direct.zil",
    "tests/language-readiness/terminal/concurrency-and-registration/task-unit-ready.zil",
];

#[test]
fn win64_w3_all_sources_verify_and_emit_win32_ir_without_posix() {
    let target = TargetProfile::windows_x64_llvm();
    let inline_cases: &[(&str, &str)] = &[
        (
            "win64_mutex_owned_resource.zil",
            WIN64_MUTEX_OWNED_RESOURCE_PROGRAM,
        ),
        (
            "win64_mutex_srwlock_contention.zil",
            WIN64_MUTEX_SRWLOCK_CONTENTION_PROGRAM,
        ),
        (
            "win64_task_fiber_fp_cancel_stress.zil",
            WIN64_TASK_FIBER_FP_CANCEL_AND_STRESS_PROGRAM,
        ),
        (
            "win64_async_reg_bg_thread.zil",
            WIN64_ASYNC_REG_BG_THREAD_PROGRAM,
        ),
    ];

    let root = repo_root();
    let mut all_cases: Vec<(String, String)> = inline_cases
        .iter()
        .map(|(n, s)| ((*n).to_string(), (*s).to_string()))
        .collect();
    for rel in WIN32_CONCURRENCY_FIXTURES {
        let path = root.join(rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()));
        all_cases.push(((*rel).to_string(), src));
    }

    for (name, src) in &all_cases {
        let verified = verify_body_program(&[(name.as_str(), src.as_str())])
            .unwrap_or_else(|e| panic!("{name} must pass independent zil-check: {e:?}"));
        let ir = emit_verified_body_for_target(&verified, true, &target.triple)
            .unwrap_or_else(|e| panic!("Win64 W3 IR emission failed for {name}: {e:?}"));
        assert!(
            ir.contains("target triple = \"x86_64-pc-windows-msvc\""),
            "expected Win64 MSVC triple in {name}"
        );
        for forbidden in [
            "@pthread_create",
            "@pthread_join",
            "@pthread_mutex_init",
            "@pthread_mutex_destroy",
            "@pthread_mutex_lock",
            "@pthread_mutex_unlock",
            "@getcontext",
            "@makecontext",
            "@swapcontext",
            "@mprotect",
            "@__errno_location",
        ] {
            assert!(
                !ir.contains(forbidden),
                "Win64 W3 IR for {name} must not reference POSIX symbol {forbidden}"
            );
        }
    }
}

#[test]
fn win64_w3_concurrency_safety_negatives_rejected_before_codegen() {
    let negatives: &[(&str, &str)] = &[
        (
            "win64_local_task_transfer_neg.zil",
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
            Poll.Pending() => { continue }
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
                    Poll.Pending() => { continue }
                    Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give rem)
            return 1
        }
        TaskThreadScopeResult.Completed(a, b) => { return a + b }
    }
}
"#,
        ),
        (
            "win64_task_use_after_destroy_neg.zil",
            r#"
fn worker(x: i32) -> i32 {
    task_yield()
    return x + 1
}
fn main() -> TaskCreateError!i32 {
    var t = task_create(worker, 5)?
    task_request_cancel(&mut t)
    loop {
        match task_poll_close(&mut t) {
            Poll.Pending() => { continue }
            Poll.Ready(_) => { break }
        }
    }
    task_finish_destroy(give t)
    match task_poll(&mut t) {
        .ok(_) => { return 0 }
        .err(_) => { return 1 }
    }
}
"#,
        ),
        (
            "win64_mutex_guard_double_unlock_neg.zil",
            r#"
fn main() -> i32 {
    match mutex_create(1) {
        .ok(m) => {
            var g = mutex_lock(m)
            mutex_unlock(g)
            mutex_unlock(g)
            return 0
        }
        .err(_) => { return 1 }
    }
}
"#,
        ),
    ];

    for (name, src) in negatives {
        let source = SourceUnit::new(FileId(1), ModuleId(1), *name, *src);
        let report = compile_source(
            &source,
            &CompileOptions {
                stop_after: Stage::Build,
                target: TargetProfile::windows_x64_llvm(),
                backend: NativeBackend::Clang,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "Win64 W3 negative {name} must be SafeReject, got {:?}",
            report
        );
        assert_ne!(
            report.phase,
            Phase::Codegen,
            "Win64 W3 negative {name} must fail before Codegen, got {:?}",
            report
        );
    }
}

#[cfg(target_os = "windows")]
fn win64_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-win64-w3-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[cfg(target_os = "windows")]
fn compile_and_run_win64_o0_o2(
    dir: &Path,
    entry: &Path,
    extra_c_sources: &[PathBuf],
) -> (i32, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>)> = None;
    for (label, opt) in [("O0", Optimization::O0), ("O2", Optimization::O2)] {
        let exe = dir.join(format!("app_{label}.exe"));
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            entry,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
                backend: NativeBackend::Clang,
                output: Some(exe.clone()),
                extra_c_sources: extra_c_sources.to_vec(),
                codegen: CodegenOptions { units: 1, jobs: 1 },
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "Win64 W3 build failed for {} ({label}): {:?}",
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
            "unexpected stderr for {} ({label}): {}",
            entry.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let result = (out.status.code().unwrap_or(-1), out.stdout);
        if let Some(prev) = &expected {
            assert_eq!(
                &result,
                prev,
                "O0 vs O2 mismatch for {} ({label})",
                entry.display()
            );
        } else {
            expected = Some(result);
        }
    }
    expected.expect("O0 and O2 both executed")
}

#[cfg(target_os = "windows")]
#[test]
fn win64_native_mutex_srwlock_and_atomics_contention() {
    let root = repo_root();
    for (label, rel) in [
        (
            "mutex_owned_return",
            "tests/language-readiness/terminal/concurrency-and-registration/sync-probes/mutex-owned-return.zil",
        ),
        (
            "mutex_protected_owner_thread",
            "tests/language-readiness/terminal/concurrency-and-registration/sync-probes/mutex-protected-owner-thread.zil",
        ),
        (
            "atomic_scope",
            "tests/language-readiness/terminal/concurrency-and-registration/atomic-scope.zil",
        ),
        (
            "atomic_contention",
            "tests/language-readiness/terminal/concurrency-and-registration/atomic-contention.zil",
        ),
        (
            "cas_contention",
            "tests/language-readiness/terminal/concurrency-and-registration/cas-contention.zil",
        ),
    ] {
        let dir = win64_temp_dir(label);
        let (code, _) = compile_and_run_win64_o0_o2(&dir, &root.join(rel), &[]);
        assert_eq!(code, 0, "Win64 W3 fixture {rel} exited with {code}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 20,000 contended Win32 SRWLOCK lock/increment/unlock ops across 2 OS threads.
    let cdir = win64_temp_dir("mutex_srwlock_contention");
    let csrc = cdir.join("srwlock_contention.zil");
    std::fs::write(&csrc, WIN64_MUTEX_SRWLOCK_CONTENTION_PROGRAM)
        .expect("write srwlock_contention.zil");
    let (ccode, _) = compile_and_run_win64_o0_o2(&cdir, &csrc, &[]);
    assert_eq!(
        ccode, 0,
        "Win64 SRWLOCK 20,000-op contention test exited with {ccode}"
    );
    let _ = std::fs::remove_dir_all(&cdir);

    // Owned resource inside Mutex<Vault> + explicit mutex_unlock + ThreadScope.
    let dir = win64_temp_dir("mutex_owned_vault");
    let obs = dir.join("obs.c");
    std::fs::write(
        &obs,
        r#"
#include <stdint.h>
static int32_t g_total = 0;
void win32_obs_drop(int32_t tag) { g_total += tag; }
int32_t win32_obs_total(void) { return g_total; }
"#,
    )
    .expect("write obs.c");
    let src_path = dir.join("mutex_owned.zil");
    std::fs::write(&src_path, WIN64_MUTEX_OWNED_RESOURCE_PROGRAM).expect("write mutex_owned.zil");
    let (code, _) = compile_and_run_win64_o0_o2(&dir, &src_path, &[obs]);
    assert_eq!(
        code, 49,
        "expected exit code 49 from Mutex<Vault> owned resource test"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "windows")]
#[test]
fn win64_native_thread_scope_and_task_cross_thread_transfer() {
    let root = repo_root();
    let dir = win64_temp_dir("task_thread_transfer");
    let entry = root.join("tests/language-readiness/task-thread-transfer/scalar.zil");
    let (code, stdout) = compile_and_run_win64_o0_o2(&dir, &entry, &[]);
    assert_eq!(
        code, 0,
        "Win64 cross-thread Task transfer scalar.zil exited with {code}"
    );
    assert!(stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "windows")]
#[test]
fn win64_native_task_fibers_owned_child_pending_exit_fp_and_stress() {
    let root = repo_root();
    for (label, rel, expected_stdout) in [
        (
            "task_owned_result",
            "tests/language-readiness/terminal/concurrency-and-registration/task-owned-result.zil",
            b"PRA".as_slice(),
        ),
        (
            "task_owned_child_created",
            "tests/language-readiness/terminal/concurrency-and-registration/task-owned-child-created.zil",
            b"PPPRA".as_slice(),
        ),
        (
            "task_owned_child_direct",
            "tests/language-readiness/terminal/concurrency-and-registration/task-owned-child-direct.zil",
            b"BA".as_slice(),
        ),
        (
            "task_unit_ready",
            "tests/language-readiness/terminal/concurrency-and-registration/task-unit-ready.zil",
            b"PR".as_slice(),
        ),
    ] {
        let dir = win64_temp_dir(label);
        let (code, stdout) = compile_and_run_win64_o0_o2(&dir, &root.join(rel), &[]);
        assert_eq!(code, 0, "Win64 Task fixture {rel} exited with {code}");
        assert_eq!(
            stdout.as_slice(),
            expected_stdout,
            "stdout mismatch for {rel}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    let dir = win64_temp_dir("fiber_fp_cancel_stress");
    let obs = dir.join("fiber_obs.c");
    std::fs::write(
        &obs,
        r#"
#include <stdint.h>
static int32_t g_total = 0;
static int32_t g_count = 0;
void win32_obs_reset(void) { g_total = 0; g_count = 0; }
void win32_obs_drop(int32_t tag) { g_total += tag; g_count += 1; }
int32_t win32_obs_total(void) { return g_total; }
int32_t win32_obs_count(void) { return g_count; }
"#,
    )
    .expect("write fiber_obs.c");
    let src_path = dir.join("fiber_stress.zil");
    std::fs::write(&src_path, WIN64_TASK_FIBER_FP_CANCEL_AND_STRESS_PROGRAM)
        .expect("write fiber_stress.zil");
    let (code, _) = compile_and_run_win64_o0_o2(&dir, &src_path, &[obs]);
    assert_eq!(
        code, 0,
        "Win64 Fiber FP/PendingExit/nested-cancel/40-round stress exited with {code}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "windows")]
#[test]
fn win64_native_async_registration_win32_background_thread() {
    let dir = win64_temp_dir("async_reg_bg_thread");
    let host_c = dir.join("win32_areg_host.c");
    std::fs::write(
        &host_c,
        r#"
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <process.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef int64_t (*cb_t)(void *, int32_t);

struct host_state {
    cb_t cb;
    void *ctx;
    HANDLE th;
    volatile LONG live_done;
    volatile LONG close_requested;
    volatile LONG release_bg;
    volatile LONG bg_finished;
    volatile LONG live_ok;
    volatile LONG late_rejected;
    volatile LONG drain_polls;
    volatile LONG drop_count;
    volatile LONG drop_before_drain;
};

static struct host_state g_host;

static unsigned __stdcall bg_worker(void *arg) {
    struct host_state *h = (struct host_state *)arg;
    /* 1. Fire two live callbacks from the background Win32 OS thread.
       base=5, mark.code=37:
       call(10): acc=15 -> 15+37 = 52
       call(20): acc=35 -> 35+37 = 72
    */
    int64_t r1 = h->cb(h->ctx, 10);
    int64_t r2 = h->cb(h->ctx, 20);
    if (r1 == 52 && r2 == 72) {
        InterlockedExchange(&h->live_ok, 1);
    }
    InterlockedExchange(&h->live_done, 1);

    /* 2. Wait until ZIL initiates close AND releases the background thread. */
    while (InterlockedCompareExchange(&h->release_bg, 0, 0) == 0) {
        Sleep(1);
    }

    /* 3. Fire a LATE callback strictly after request_close; the 5-state CAS
       trampoline must reject it with (1LL << 32) without invoking the closure. */
    int64_t late = h->cb(h->ctx, 999);
    if (late == (INT64_C(1) << 32)) {
        InterlockedExchange(&h->late_rejected, 1);
    }

    /* 4. Clear retained pointers and signal drain completion. */
    h->cb = NULL;
    h->ctx = NULL;
    InterlockedExchange(&h->bg_finished, 1);
    return 0;
}

void *win32_areg_register(cb_t cb, void *ctx) {
    memset(&g_host, 0, sizeof(g_host));
    g_host.cb = cb;
    g_host.ctx = ctx;
    uintptr_t h = _beginthreadex(NULL, 0, bg_worker, &g_host, 0, NULL);
    if (h == 0) {
        return NULL;
    }
    g_host.th = (HANDLE)h;
    while (InterlockedCompareExchange(&g_host.live_done, 0, 0) == 0) {
        Sleep(1);
    }
    return &g_host;
}

void win32_areg_request_close(void *token) {
    if (token != &g_host) abort();
    InterlockedExchange(&g_host.close_requested, 1);
}

int32_t win32_areg_poll_drain(void *token) {
    if (token != &g_host) abort();
    InterlockedIncrement(&g_host.drain_polls);
    if (InterlockedCompareExchange(&g_host.bg_finished, 0, 0) == 0) {
        return 0; /* Pending: forces __zil_areg_drop_frame to SwitchToFiber */
    }
    if (g_host.th != NULL) {
        WaitForSingleObject(g_host.th, INFINITE);
        CloseHandle(g_host.th);
        g_host.th = NULL;
    }
    return 1; /* Drained */
}

void win32_areg_release_bg(void) {
    InterlockedExchange(&g_host.release_bg, 1);
    while (InterlockedCompareExchange(&g_host.bg_finished, 0, 0) == 0) {
        Sleep(1);
    }
}

void win32_areg_note_drop(int32_t code) {
    if (code != 37) abort();
    if (InterlockedCompareExchange(&g_host.bg_finished, 0, 0) == 0) {
        InterlockedIncrement(&g_host.drop_before_drain);
    }
    InterlockedIncrement(&g_host.drop_count);
}

int32_t win32_areg_verify(void) {
    if (g_host.live_ok != 1) return 21;
    if (g_host.close_requested != 1) return 22;
    if (g_host.late_rejected != 1) return 23;
    if (g_host.bg_finished != 1) return 24;
    if (g_host.drain_polls < 2) return 25;
    if (g_host.drop_before_drain != 0) return 26;
    if (g_host.drop_count != 1) return 27;
    if (g_host.cb != NULL || g_host.ctx != NULL || g_host.th != NULL) return 28;
    return 0;
}
"#,
    )
    .expect("write win32_areg_host.c");
    let src_path = dir.join("win32_areg.zil");
    std::fs::write(&src_path, WIN64_ASYNC_REG_BG_THREAD_PROGRAM).expect("write win32_areg.zil");
    let (code, _) = compile_and_run_win64_o0_o2(&dir, &src_path, &[host_c]);
    assert_eq!(
        code, 0,
        "Win64 AsyncRegistration background Win32 thread test exited with {code}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
