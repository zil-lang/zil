//! End-to-end native acceptance tests for A1 (`task/foreign-registration-native`):
//! - A1.1 Same-thread `Registration` (`c_register`, `registration_valid`, `registration_close`)
//! - A1.2 Cross-thread `AsyncRegistration` with real pthread races, raw holder,
//!   ClosingBusy interleaving, late-entry rejection, invalid-drain abort, early-destroy abort
//! - A1.3 Task + closure + `AsyncRegistration` composition (Pending -> Ready, cancel, cleanup order)
//! - Source and independent-checker safety negatives (borrowed env, suspending callback, non-C provider)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-a1-reg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn dual_backends() -> Vec<NativeBackend> {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    backends
}

fn build_and_run_c(
    entry: &Path,
    exe: &Path,
    backend: NativeBackend,
    opt: Optimization,
    extra_c_sources: &[PathBuf],
    envs: &[(&str, &str)],
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

    let mut cmd = Command::new(exe);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for &(k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()))
}

#[test]
fn sync_registration_lifecycle_install_fail_post_return_and_unregister() {
    let dir = temp_dir("a1-1-sync");
    let c_host = dir.join("sync_host.c");
    std::fs::write(
        &c_host,
        r#"
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>

typedef int64_t (*cb_t)(void *, int32_t);
static cb_t saved_cb = NULL;
static void *saved_ctx = NULL;
static int closes = 0;

void *sync_register_ok(cb_t cb, void *ctx) {
    saved_cb = cb;
    saved_ctx = ctx;
    return (void *)&saved_cb;
}

void *sync_register_fail(cb_t cb, void *ctx) {
    (void)cb;
    (void)ctx;
    return NULL;
}

void sync_close(void *tok) {
    if (tok != (void *)&saved_cb) abort();
    /* Synchronous provider contract: unregister must clear saved_cb/saved_ctx
       before returning so registration_close can safely free the heap frame. */
    saved_cb = NULL;
    saved_ctx = NULL;
    closes++;
}

int64_t sync_invoke(int32_t arg) {
    if (!saved_cb) return -1;
    return saved_cb(saved_ctx, arg);
}

int32_t sync_closes(void) {
    return closes;
}
"#,
    )
    .expect("write sync_host.c");

    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
extern "C" fn putchar(c: i32) -> i32;
extern "C" fn sync_register_ok(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn sync_register_fail(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn sync_close(tok: c_handle);
extern "C" fn sync_invoke(arg: i32) -> i64;
extern "C" fn sync_closes() -> i32;

resource Tag { code: i32 }
impl Drop for Tag {
    fn drop(self: Self) { unsafe { putchar(.code) } }
}

fn install_ok<F: FnMut(i32) -> i32 + ClosedEnvironment + SyncDrop>(cb: F) -> Managed<Registration<F>> {
    unsafe { return c_register(sync_register_ok, sync_close, cb) }
}

fn main() -> i32 {
    unsafe {
        // 1. Install failure must drop environment immediately once ('F') and return invalid handle.
        let fail_tag = Tag { code: 70 }
        let fail_cb = move (x: i32) -> i32 => x + fail_tag.code
        let bad = c_register(sync_register_fail, sync_close, fail_cb)
        if registration_valid(bad) { return 11 }
        registration_close(give bad)

        // 2. Install succeeds and callback survives after installer returns (stable heap env).
        let tag = Tag { code: 65 }
        var acc: i32 = 10
        let cb = move (x: i32) -> i32 => {
            acc = acc + x
            return acc + tag.code
        }
        let reg = install_ok(cb)
        if !registration_valid(reg) { return 12 }
        let r1 = sync_invoke(5)
        if r1 != 80 { return 13 }
        let r2 = sync_invoke(7)
        if r2 != 87 { return 14 }

        // 3. Explicit close calls C unregister once (clearing saved_cb/saved_ctx) and drops Tag ('A') once.
        registration_close(give reg)
        if sync_closes() != 1 { return 15 }

        // 4. Subsequent host invoke sees unregistered slot (-1) and never touches freed userdata.
        //    (True post-close late-entry rejection on a raw C holder is verified in AsyncRegistration.)
        let after = sync_invoke(1)
        if after != -1 { return 16 }

        return 0
    }
}
"#,
    )
    .expect("write main.zil");

    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("sync-{}-{opt:?}", backend.name()));
            let out = build_and_run_c(&src, &exe, backend, opt, std::slice::from_ref(&c_host), &[]);
            assert_eq!(
                (out.status.code(), out.stdout.as_slice()),
                (Some(0), b"FA".as_slice()),
                "A1.1 sync registration ({backend:?}, {opt:?}) failed: status={:?}, stdout={:?}, stderr={}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn threaded_async_registration_race_raw_holder_and_invalid_drain_abort() {
    let dir = temp_dir("a1-2-threaded");
    let c_host = dir.join("threaded_host.c");
    std::fs::write(
        &c_host,
        r#"
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <stdatomic.h>
#include <pthread.h>
#include <sched.h>

typedef int64_t (*cb_t)(void *, int32_t);
static cb_t g_cb = NULL;
static void *g_ctx = NULL;
static atomic_int g_t1_done = 0;
static atomic_int g_t2_done = 0;
static atomic_int g_t3_ready = 0;
static atomic_int g_closing = 0;
static atomic_int g_release_t3 = 0;
static atomic_int g_raw_holder_done = 0;
static atomic_int g_late_rejected = 0;
static atomic_int g_cb1_result = 0;
static atomic_int g_cb2_result = 0;
static pthread_t g_t1, g_t2, g_t3;

static void *worker_t1(void *arg) {
    (void)arg;
    int64_t r = g_cb(g_ctx, 10);
    atomic_store(&g_cb1_result, (int)r);
    atomic_store_explicit(&g_t1_done, 1, memory_order_relaxed);
    return NULL;
}

static void *worker_t2(void *arg) {
    (void)arg;
    // Relaxed wait orders attempt only; visibility of t1's capture mutation relies on gate release/acquire.
    while (!atomic_load_explicit(&g_t1_done, memory_order_relaxed)) sched_yield();
    int64_t r = g_cb(g_ctx, 20);
    atomic_store(&g_cb2_result, (int)r);
    atomic_store(&g_t2_done, 1);
    return NULL;
}

static void *worker_t3_raw_holder(void *arg) {
    (void)arg;
    // Raw holder keeps userdata before trampoline admission until closing starts.
    cb_t local_cb = g_cb;
    void *local_ctx = g_ctx;
    atomic_store(&g_t3_ready, 1);
    while (!atomic_load(&g_closing)) sched_yield();
    while (!atomic_load(&g_release_t3)) sched_yield();
    int64_t r = local_cb(local_ctx, 30);
    if (r == (INT64_C(1) << 32)) atomic_store(&g_late_rejected, 1);
    atomic_store(&g_raw_holder_done, 1);
    return NULL;
}

void *thr_register(cb_t cb, void *ctx) {
    g_cb = cb;
    g_ctx = ctx;
    if (getenv("ZIL_THR_INSTALL_FAIL")) return NULL;
    if (pthread_create(&g_t1, NULL, worker_t1, NULL) != 0) abort();
    if (pthread_create(&g_t2, NULL, worker_t2, NULL) != 0) abort();
    if (pthread_create(&g_t3, NULL, worker_t3_raw_holder, NULL) != 0) abort();
    while (!atomic_load(&g_t2_done)) sched_yield();
    while (!atomic_load(&g_t3_ready)) sched_yield();
    return (void *)&g_cb;
}

void thr_request_close(void *tok) {
    if (tok != (void *)&g_cb) abort();
    atomic_store(&g_closing, 1);
}

int32_t thr_poll_drain(void *tok) {
    if (tok != (void *)&g_cb) abort();
    if (getenv("ZIL_THR_INVALID_DRAIN")) return 2;
    if (!atomic_load(&g_raw_holder_done)) return 0;
    pthread_join(g_t1, NULL);
    pthread_join(g_t2, NULL);
    pthread_join(g_t3, NULL);
    g_cb = NULL;
    g_ctx = NULL;
    return 1;
}

void host_release_t3(void) {
    atomic_store(&g_release_t3, 1);
}

int32_t host_verify_race_counts(void) {
    if (atomic_load(&g_cb1_result) != 111) return 1;
    if (atomic_load(&g_cb2_result) != 122) return 2;
    if (atomic_load(&g_late_rejected) != 1) return 3;
    return 0;
}
"#,
    )
    .expect("write threaded_host.c");

    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
extern "C" fn putchar(c: i32) -> i32;
extern "C" fn thr_register(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn thr_request_close(tok: c_handle);
extern "C" fn thr_poll_drain(tok: c_handle) -> i32;
extern "C" fn host_release_t3();
extern "C" fn host_verify_race_counts() -> i32;

resource DropMark { code: i32 }
impl Drop for DropMark {
    fn drop(self: Self) { unsafe { putchar(.code) } }
}

fn main() -> i32 {
    unsafe {
        let mark = DropMark { code: 68 }
        var count: i32 = 100
        let cb = move (x: i32) -> i32 => {
            count = count + 1
            return x + count + (mark.code - 68)
        }
        var reg = c_register_async(thr_register, thr_request_close, thr_poll_drain, cb)
        if !async_registration_valid(reg) {
            // Install failure path: env already dropped once ('D'), finish_destroy is a safe no-op.
            async_registration_finish_destroy(give reg)
            return 0
        }

        // Request close while t3 still holds raw userdata (before draining).
        async_registration_request_close(&mut reg)

        // Poll while raw holder t3 is not drained MUST return Pending and must NOT drop `mark` yet.
        match async_registration_poll_close(&mut reg) {
            Poll.Pending() => {},
            Poll.Ready(_) => { return 21 }
        }

        // Release t3 raw holder so it attempts late invocation (rejected) and drains.
        host_release_t3()

        loop {
            match async_registration_poll_close(&mut reg) {
                Poll.Pending() => { continue },
                Poll.Ready(_) => { break }
            }
        }

        async_registration_finish_destroy(give reg)
        return host_verify_race_counts()
    }
}
"#,
    )
    .expect("write main.zil");

    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("thr-{}-{opt:?}", backend.name()));

            // 1. Normal full cross-thread race: OpenBusy -> ClosingBusy -> raw holder late reject -> drain -> 1 drop.
            let out = build_and_run_c(&src, &exe, backend, opt, std::slice::from_ref(&c_host), &[]);
            assert_eq!(
                (out.status.code(), out.stdout.as_slice()),
                (Some(0), b"D".as_slice()),
                "A1.2 threaded race ({backend:?}, {opt:?}) failed: status={:?}, stdout={:?}, stderr={}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );

            // 2. Install failure control: drops env once ('D') and returns invalid handle cleanly.
            let out_fail = build_and_run_c(
                &src,
                &exe,
                backend,
                opt,
                std::slice::from_ref(&c_host),
                &[("ZIL_THR_INSTALL_FAIL", "1")],
            );
            assert_eq!(
                (out_fail.status.code(), out_fail.stdout.as_slice()),
                (Some(0), b"D".as_slice()),
                "A1.2 install fail ({backend:?}, {opt:?}) failed"
            );

            // 3. Invalid drain return value (2): must abort deterministically without dropping env.
            let out_inv = build_and_run_c(
                &src,
                &exe,
                backend,
                opt,
                std::slice::from_ref(&c_host),
                &[("ZIL_THR_INVALID_DRAIN", "1")],
            );
            assert!(
                !out_inv.status.success(),
                "invalid drain tag (2) must abort ({backend:?}, {opt:?})"
            );
        }
    }

    // 4. Early destroy without reaching Closed must abort deterministically.
    let early_src = dir.join("early_destroy.zil");
    std::fs::write(
        &early_src,
        r#"
extern "C" fn thr_register(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn thr_request_close(tok: c_handle);
extern "C" fn thr_poll_drain(tok: c_handle) -> i32;

fn main() -> i32 {
    unsafe {
        let cb = move (x: i32) -> i32 => x + 1
        let reg = c_register_async(thr_register, thr_request_close, thr_poll_drain, cb)
        // Illegal early finish_destroy before poll_close reaches Ready(Closed).
        async_registration_finish_destroy(give reg)
        return 0
    }
}
"#,
    )
    .expect("write early_destroy.zil");

    for backend in dual_backends() {
        let exe = dir.join(format!("early-{}", backend.name()));
        let out = build_and_run_c(
            &early_src,
            &exe,
            backend,
            Optimization::O0,
            std::slice::from_ref(&c_host),
            &[],
        );
        assert!(
            !out.status.success(),
            "early async_registration_finish_destroy must abort ({backend:?})"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn async_registration_with_task_cancel_and_nested_cleanup_order() {
    let dir = temp_dir("a1-3-task-combo");
    let c_host = dir.join("task_reg_host.c");
    std::fs::write(
        &c_host,
        r#"
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>

typedef int64_t (*cb_t)(void *, int32_t);
static cb_t q_cb = NULL;
static void *q_ctx = NULL;
static int q_queued = 0;
static int q_closing = 0;
static int q_polls = 0;

void *q_register(cb_t cb, void *ctx) {
    q_cb = cb;
    q_ctx = ctx;
    if (cb(ctx, 2) != 40) abort();
    q_queued = 1;
    return (void *)&q_cb;
}

void q_request(void *tok) {
    if (tok != (void *)&q_cb) abort();
    q_closing = 1;
    // Synchronous reentrant callback during request_close must be rejected.
    if (q_cb(q_ctx, 99) != (INT64_C(1) << 32)) abort();
}

int32_t q_drain(void *tok) {
    if (tok != (void *)&q_cb || !q_closing) abort();
    q_polls++;
    if (q_queued) return 0;
    q_cb = NULL;
    q_ctx = NULL;
    return 1;
}

void q_pump(void) {
    if (!q_queued || !q_closing) abort();
    if (q_cb(q_ctx, 100) != (INT64_C(1) << 32)) abort();
    q_queued = 0;
}

int32_t q_poll_count(void) { return q_polls; }
"#,
    )
    .expect("write task_reg_host.c");

    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
extern "C" fn putchar(c: i32) -> i32;
extern "C" fn q_register(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn q_request(tok: c_handle);
extern "C" fn q_drain(tok: c_handle) -> i32;
extern "C" fn q_pump();
extern "C" fn q_poll_count() -> i32;

resource Note { code: i32 }
impl Drop for Note { fn drop(self: Self) { unsafe { putchar(.code) } } }

fn worker(seed: i32) -> i32 {
    unsafe {
        let outer = Note { code: 79 } // 'O'
        let note = Note { code: 82 }  // 'R' (owned by AsyncRegistration closure)
        var acc: i32 = seed
        let cb = move (x: i32) -> i32 => {
            acc = acc + x
            return acc + (note.code - 82)
        }
        var reg = c_register_async(q_register, q_request, q_drain, cb)
        putchar(83) // 'S'
        task_yield()
        putchar(88) // 'X' (must not run when cancelled)
        async_registration_request_close(&mut reg)
        loop {
            match async_registration_poll_close(&mut reg) {
                Poll.Pending() => { task_yield(); continue },
                Poll.Ready(_) => { break }
            }
        }
        async_registration_finish_destroy(give reg)
        return outer.code
    }
}

fn main() -> i32 {
    unsafe {
        match task_create(worker, 38) {
            .ok(t) => {
                var task = give t
                // 1. Poll starts worker: installs registration (cb(2)==40), prints 'S', yields.
                match task_poll(&mut task) {
                    .ok(p) => { match p { Poll.Pending() => {}, Poll.Ready(_) => { return 11 } } },
                    .err(_) => { return 12 }
                }
                // 2. Request cancel on task. First poll_close unwinds `reg` -> drain returns 0 -> Pending!
                task_request_cancel(&mut task)
                match task_poll_close(&mut task) {
                    Poll.Pending() => {},
                    Poll.Ready(_) => { return 13 }
                }
                // 3. External host pumps queued event (late callback rejected) and clears queue.
                q_pump()
                // 4. Second poll_close drains `reg` -> drops Note('R') -> drops outer Note('O') -> Ready!
                match task_poll_close(&mut task) {
                    Poll.Ready(_) => {},
                    Poll.Pending() => { return 14 }
                }
                task_finish_destroy(give task)
                if q_poll_count() != 2 { return 15 }
                return 0
            },
            .err(_) => { return 10 }
        }
    }
}
"#,
    )
    .expect("write main.zil");

    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("combo-{}-{opt:?}", backend.name()));
            let out = build_and_run_c(&src, &exe, backend, opt, std::slice::from_ref(&c_host), &[]);
            assert_eq!(
                (out.status.code(), out.stdout.as_slice()),
                (Some(0), b"SRO".as_slice()),
                "A1.3 task+registration combo ({backend:?}, {opt:?}) failed: status={:?}, stdout={:?}, stderr={}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn foreign_registration_safety_negatives_rejected() {
    let dir = temp_dir("a1-negatives");
    let mut session = CompilationSession::new();
    let verify_opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };

    let neg_cases: [(&str, &str); 11] = [
        // 1. Borrowed local environment (violates ClosedEnvironment / holds address).
        (
            "borrowed_env_sync.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn cls(tok: c_handle);
fn main() -> i32 {
    var local: i32 = 1
    let r = &local
    let cb = move (x: i32) -> i32 => x + *r
    let h = unsafe { c_register(reg, cls, cb) }
    registration_close(give h)
    return 0
}
"#,
        ),
        // 2. Suspending callback closure passed to `c_register_async`.
        (
            "suspending_async_cb.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn req(tok: c_handle);
extern "C" fn drn(tok: c_handle) -> i32;
fn main() -> i32 {
    let cb = move (x: i32) -> i32 => {
        task_yield()
        return x
    }
    var h = unsafe { c_register_async(reg, req, drn, cb) }
    async_registration_request_close(&mut h)
    let _ = async_registration_poll_close(&mut h)
    async_registration_finish_destroy(give h)
    return 0
}
"#,
        ),
        // 3. Non-C ZIL function passed as provider callback.
        (
            "non_c_provider.zil",
            r#"
fn fake_reg(cb: i32, ctx: i32) -> i32 { return cb + ctx }
extern "C" fn cls(tok: c_handle);
fn main() -> i32 {
    let cb = move (x: i32) -> i32 => x + 1
    let h = unsafe { c_register(fake_reg, cls, cb) }
    registration_close(give h)
    return 0
}
"#,
        ),
        // 4. Non-Transfer capture (`Managed<Task<i32>>`) in `c_register_async` rejected.
        (
            "non_transfer_task_capture.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn req(tok: c_handle);
extern "C" fn drn(tok: c_handle) -> i32;
fn worker() -> i32 { return 7 }
fn main() -> i32 {
    let t = task_create(worker)
    let cb = move (x: i32) -> i32 => {
        if task_valid(t) { return x + 1 }
        return x
    }
    var h = unsafe { c_register_async(reg, req, drn, cb) }
    async_registration_request_close(&mut h)
    let _ = async_registration_poll_close(&mut h)
    async_registration_finish_destroy(give h)
    return 0
}
"#,
        ),
        // 5. Suspending `Drop` on captured resource in `c_register_async` rejected (SyncDrop).
        (
            "suspending_drop_async_reg.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn req(tok: c_handle);
extern "C" fn drn(tok: c_handle) -> i32;
resource AsyncCleanup { code: i32 }
impl Drop for AsyncCleanup {
    fn drop(self: Self) { task_yield() }
}
fn main() -> i32 {
    let r = AsyncCleanup { code: 1 }
    let cb = move (x: i32) -> i32 => x + r.code
    var h = unsafe { c_register_async(reg, req, drn, cb) }
    async_registration_request_close(&mut h)
    let _ = async_registration_poll_close(&mut h)
    async_registration_finish_destroy(give h)
    return 0
}
"#,
        ),
        // 6. Calling `c_register` outside `unsafe` block must be rejected (`E-FFI-UNSAFE`).
        (
            "safe_call_c_register_rejected.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn cls(tok: c_handle);
fn main() -> i32 {
    let cb = move (x: i32) -> i32 => x + 1
    let h = c_register(reg, cls, cb)
    registration_close(give h)
    return 0
}
"#,
        ),
        // 7. Calling `c_register_async` outside `unsafe` block must be rejected (`E-FFI-UNSAFE`).
        (
            "safe_call_c_register_async_rejected.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn req(tok: c_handle);
extern "C" fn drn(tok: c_handle) -> i32;
fn main() -> i32 {
    let cb = move (x: i32) -> i32 => x + 1
    var h = c_register_async(reg, req, drn, cb)
    async_registration_request_close(&mut h)
    let _ = async_registration_poll_close(&mut h)
    async_registration_finish_destroy(give h)
    return 0
}
"#,
        ),
        // 8. Unknown `extern "C"` call inside `c_register_async` callback rejected (sealed ThreadCallable).
        (
            "thread_callable_ffi_rejected.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn req(tok: c_handle);
extern "C" fn drn(tok: c_handle) -> i32;
extern "C" fn unknown_host_fn(v: i32) -> i32;
fn helper(v: i32) -> i32 { return unsafe { unknown_host_fn(v) } }
fn main() -> i32 {
    let cb = move (x: i32) -> i32 => helper(x)
    var h = unsafe { c_register_async(reg, req, drn, cb) }
    async_registration_request_close(&mut h)
    let _ = async_registration_poll_close(&mut h)
    async_registration_finish_destroy(give h)
    return 0
}
"#,
        ),
        // 9. Indirect function-pointer call inside `c_register_async` callback rejected (sealed ThreadCallable).
        (
            "thread_callable_indirect_rejected.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn req(tok: c_handle);
extern "C" fn drn(tok: c_handle) -> i32;
fn bump(v: i32) -> i32 { return v + 1 }
fn main() -> i32 {
    let fp: fn(i32) -> i32 = bump
    let cb = move (x: i32) -> i32 => fp(x)
    var h = unsafe { c_register_async(reg, req, drn, cb) }
    async_registration_request_close(&mut h)
    let _ = async_registration_poll_close(&mut h)
    async_registration_finish_destroy(give h)
    return 0
}
"#,
        ),
        // 10. Temporary user `Drop` inside `c_register_async` callback rejected (sealed ThreadCallable).
        (
            "thread_callable_temp_user_drop_rejected.zil",
            r#"
extern "C" fn reg(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn req(tok: c_handle);
extern "C" fn drn(tok: c_handle) -> i32;
resource TempDrop { v: i32 }
impl Drop for TempDrop {
    fn drop(self: Self) {}
}
fn main() -> i32 {
    let cb = move (x: i32) -> i32 => {
        let tmp = TempDrop { v: x }
        return tmp.v + 1
    }
    var h = unsafe { c_register_async(reg, req, drn, cb) }
    async_registration_request_close(&mut h)
    let _ = async_registration_poll_close(&mut h)
    async_registration_finish_destroy(give h)
    return 0
}
"#,
        ),
        // 11. Mismatched `extern "C"` provider ABI signature rejected by opspec/zil-check.
        (
            "mismatched_provider_abi_rejected.zil",
            r#"
extern "C" fn bad_reg(x: i32) -> i32;
extern "C" fn cls(tok: c_handle);
fn main() -> i32 {
    let cb = move (x: i32) -> i32 => x + 1
    let h = unsafe { c_register(bad_reg, cls, cb) }
    registration_close(give h)
    return 0
}
"#,
        ),
    ];

    for (name, code) in neg_cases {
        let p = dir.join(name);
        std::fs::write(&p, code).expect("write neg case");
        let report = session.compile_path(&p, &verify_opts);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name} must be SafeReject, got {:?} (diags: {:?})",
            report.status,
            report.diagnostics
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn registration_and_async_registration_oom_drops_env_once() {
    let dir = temp_dir("a1-oom");
    let c_host = dir.join("oom_host.c");
    std::fs::write(
        &c_host,
        r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdlib.h>

typedef int64_t (*cb_t)(void *, int32_t);
static int g_fail_next = 0;

void arm_oom_next_calloc(void) { g_fail_next = 1; }

void *calloc(size_t nmemb, size_t size) {
    if (g_fail_next) {
        g_fail_next = 0;
        return NULL;
    }
    typedef void *(*real_calloc_t)(size_t, size_t);
    static real_calloc_t real_calloc = NULL;
    if (!real_calloc) real_calloc = (real_calloc_t)dlsym(RTLD_NEXT, "calloc");
    return real_calloc(nmemb, size);
}

void *never_register(cb_t cb, void *ctx) { (void)cb; (void)ctx; abort(); }
void never_close(void *tok) { (void)tok; abort(); }
int32_t never_drain(void *tok) { (void)tok; abort(); }
"#,
    )
    .expect("write oom_host.c");

    let src = dir.join("main.zil");
    std::fs::write(
        &src,
        r#"
extern "C" fn putchar(c: i32) -> i32;
extern "C" fn arm_oom_next_calloc();
extern "C" fn never_register(cb: extern "C" fn(c_handle, i32) -> i64, ctx: c_handle) -> c_handle;
extern "C" fn never_close(tok: c_handle);
extern "C" fn never_drain(tok: c_handle) -> i32;

resource DropTag { code: i32 }
impl Drop for DropTag {
    fn drop(self: Self) { unsafe { putchar(.code) } }
}

fn main() -> i32 {
    unsafe {
        // 1. Sync Registration OOM: drops env ('S') once, never calls C register, returns invalid handle.
        let t1 = DropTag { code: 83 }
        let cb1 = move (x: i32) -> i32 => x + t1.code
        arm_oom_next_calloc()
        let r1 = c_register(never_register, never_close, cb1)
        if registration_valid(r1) { return 11 }
        registration_close(give r1)

        // 2. Async Registration OOM: drops env ('A') once, never calls C register, returns invalid handle.
        let t2 = DropTag { code: 65 }
        let cb2 = move (x: i32) -> i32 => x + t2.code
        arm_oom_next_calloc()
        let r2 = c_register_async(never_register, never_close, never_drain, cb2)
        if async_registration_valid(r2) { return 12 }
        async_registration_finish_destroy(give r2)

        return 0
    }
}
"#,
    )
    .expect("write main.zil");

    for backend in dual_backends() {
        for opt in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("oom-{}-{opt:?}", backend.name()));
            let out = build_and_run_c(&src, &exe, backend, opt, std::slice::from_ref(&c_host), &[]);
            assert_eq!(
                (out.status.code(), out.stdout.as_slice()),
                (Some(0), b"SA".as_slice()),
                "A1 OOM ({backend:?}, {opt:?}) failed: status={:?}, stdout={:?}, stderr={}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}
