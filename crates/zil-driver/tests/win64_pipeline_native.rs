//! Windows x64 target pipeline bringup tests (cross-target IR/COFF checks on
//! all hosts + real native `.exe` compilation and execution on Windows x64).

use zil_backend_llvm::{NativeBackend, emit_verified_body_for_target};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::body_pipeline::verify_body_program;
use zil_driver::{CompileOptions, Stage, compile_source};

const CORE_WIN64_PROGRAM: &str = r#"
extern "C" fn putchar(c: i32) -> i32;
extern "C" fn zil_glibc_errno_exchange(next: i32) -> i32;

trait Scorer {
    fn score(self: &Self) -> i32;
}

struct Counter {
    base: i32;
}

impl Scorer for Counter {
    fn score(self: &Self) -> i32 {
        return self.base * 2
    }
}

fn eval_dyn(item: &OwnedDyn<dyn Scorer>) -> i32 {
    return item.score()
}

fn identity<T>(x: T) -> T {
    return x
}

resource Token {
    id: i32;
}

impl Drop for Token {
    fn drop(self: Self) {
        let _ = self.id
    }
}

enum ErrKind {
    Bad;
}

fn maybe_compute(fail: bool) -> ErrKind!i32 {
    let t = Token { id: 4 }
    if fail {
        return ErrKind.Bad
    }
    return t.id + 6
}

fn main() -> i32 {
    var s: i32 = 0
    match owned_dyn::<dyn Scorer>(Counter { base: 7 }) {
        .ok(obj) => { s = eval_dyn(&obj) }
        .err(_) => { s = 0 }
    }
    let three: i32 = 3
    let id_val = identity(three)

    let buf = bytes_new()
    bytes_reserve(buf, 8)
    bytes_push(buf, u8(4))
    bytes_push(buf, u8(6))
    let view = bytes_view(buf)
    let bsum = i32(view[0]) + i32(view[1])

    let st = store_new()
    store_reserve(st, 4)
    store_push(st, Token { id: 1 })
    store_push(st, Token { id: 2 })
    let slen = i32(store_len(st))

    let add_one = (n: i32) -> i32 => { return n + 1 }
    var ok_val: i32 = 0
    match maybe_compute(false) {
        .ok(v) => { ok_val = v }
        .err(_) => { ok_val = 0 }
    }
    var err_val: i32 = 0
    match maybe_compute(true) {
        .ok(v) => { err_val = v }
        .err(_) => { err_val = 2 }
    }

    unsafe {
        let _ = putchar(65)
        let _ = zil_glibc_errno_exchange(0)
    }

    // s=14, id_val=3, bsum=10, slen=2, add_one(0)=1, ok_val=10, err_val=2 => 42
    return s + id_val + bsum + slen + add_one(0) + ok_val + err_val
}
"#;

const WIN64_EXEC_PROGRAM: &str = r#"
extern "C" fn win64_obs_drop(v: i32);
extern "C" fn win64_obs_total() -> i32;
extern "C" fn zil_glibc_errno_exchange(next: i32) -> i32;

trait Value {
    fn value(self: &Self) -> i32;
}

struct Boxed {
    n: i32;
}

impl Value for Boxed {
    fn value(self: &Self) -> i32 {
        return self.n + 5
    }
}

fn read_dyn(v: &OwnedDyn<dyn Value>) -> i32 {
    return v.value()
}

resource DropTag {
    tag: i32;
}

impl Drop for DropTag {
    fn drop(self: Self) {
        unsafe { win64_obs_drop(self.tag) }
    }
}

enum Fail {
    Boom;
}

fn scoped_cleanup(should_fail: bool) -> Fail!i32 {
    let a = DropTag { tag: 7 }
    if should_fail {
        return Fail.Boom
    }
    return a.tag
}

fn main() -> i32 {
    var d: i32 = 0
    match owned_dyn::<dyn Value>(Boxed { n: 10 }) {
        .ok(obj) => { d = read_dyn(&obj) }
        .err(_) => { d = 0 }
    }

    let buf = bytes_new()
    bytes_reserve(buf, 4)
    bytes_push(buf, u8(3))
    bytes_push(buf, u8(4))
    let view = bytes_view(buf)
    let bsum = i32(view[0]) + i32(view[1])

    let st = store_new()
    store_reserve(st, 4)
    store_push(st, DropTag { tag: 2 })
    store_push(st, DropTag { tag: 3 })
    store_pop(st)

    var r1: i32 = 0
    match scoped_cleanup(true) {
        .ok(v) => { r1 = v }
        .err(_) => { r1 = 1 }
    }
    var r2: i32 = 0
    match scoped_cleanup(false) {
        .ok(v) => { r2 = v }
        .err(_) => { r2 = 0 }
    }

    var errno_prev: i32 = 0
    unsafe {
        let _ = zil_glibc_errno_exchange(11)
        errno_prev = zil_glibc_errno_exchange(0)
    }

    var drops: i32 = 0
    unsafe {
        drops = win64_obs_total()
    }

    // d=15, bsum=7, r1=1, r2=7, errno_prev=11, drops=3(pop)+7(err)+7(ok)=17
    // Total = 15 + 7 + 1 + 7 + 11 + 17 = 58
    return d + bsum + r1 + r2 + errno_prev + drops
}
"#;

#[test]
fn win64_ir_targets_msvc_and_omits_posix_symbols() {
    let target = TargetProfile::windows_x64_llvm();
    for (name, src) in [
        ("win64_core.zil", CORE_WIN64_PROGRAM),
        ("win64_exec.zil", WIN64_EXEC_PROGRAM),
    ] {
        let verified = verify_body_program(&[(name, src)])
            .unwrap_or_else(|e| panic!("{name} must pass independent zil-check: {e:?}"));
        let ir = emit_verified_body_for_target(&verified, true, &target.triple)
            .unwrap_or_else(|e| panic!("Win64 IR emission failed for {name}: {e:?}"));

        assert!(
            ir.contains("target triple = \"x86_64-pc-windows-msvc\""),
            "expected Win64 MSVC target triple in emitted IR for {name}"
        );
        for forbidden in [
            "@getcontext",
            "@makecontext",
            "@swapcontext",
            "@mprotect",
            "@pthread_create",
            "@pthread_join",
            "@pthread_mutex_init",
            "@pthread_mutex_destroy",
            "@pthread_mutex_lock",
            "@pthread_mutex_unlock",
            "@__errno_location",
        ] {
            assert!(
                !ir.contains(forbidden),
                "Win64 IR for {name} must not contain Linux POSIX symbol {forbidden}"
            );
        }
        assert!(
            ir.contains("declare ptr @_errno()"),
            "expected Windows UCRT @_errno declaration for {name}"
        );
    }
}

#[test]
fn win64_concurrency_and_tasks_emit_win32_runtime_without_posix() {
    let concurrency_src = r#"
fn worker(m: Mutex<i32>) -> i32 {
    var g = mutex_lock(m)
    let p = mutex_value(&mut g)
    *p = *p + 3
    return *p
}

fn task_body(x: i32) -> i32 {
    task_yield()
    return x + 1
}

fn main() -> i32 {
    var acc: i32 = 0
    match mutex_create(4) {
        .ok(m) => {
            let (ok, r1, r2) = thread_scope(worker, m, () -> i32 => 5)
            if ok {
                acc = r1 + r2
            }
        }
        .err(_) => { acc = 0 }
    }
    match task_create(task_body, acc) {
        .ok(t) => {
            var handle = give t
            match task_poll(&mut handle) {
                .ok(p) => {
                    match p {
                        Poll.Pending() => {}
                        Poll.Ready(v) => { acc = v }
                    }
                }
                .err(_) => {}
            }
            match task_poll(&mut handle) {
                .ok(p) => {
                    match p {
                        Poll.Pending() => {}
                        Poll.Ready(v) => { acc = v }
                    }
                }
                .err(_) => {}
            }
        }
        .err(_) => {}
    }
    return acc
}
"#;
    let target = TargetProfile::windows_x64_llvm();
    let verified = verify_body_program(&[("win64_concurrency.zil", concurrency_src)])
        .expect("Win64 W3 concurrency/task source must pass independent zil-check");
    let ir = emit_verified_body_for_target(&verified, true, &target.triple)
        .expect("Win64 W3 concurrency/task IR emission must succeed");
    assert!(ir.contains("target triple = \"x86_64-pc-windows-msvc\""));
    for expected in [
        "@InitializeSRWLock",
        "@AcquireSRWLockExclusive",
        "@ReleaseSRWLockExclusive",
        "@_beginthreadex",
        "@WaitForSingleObject",
        "@CloseHandle",
        "@ConvertThreadToFiber",
        "@CreateFiberEx",
        "@SwitchToFiber",
        "@DeleteFiber",
        "@__zil_ensure_fiber",
        "@__zil_current_fiber",
    ] {
        assert!(
            ir.contains(expected),
            "Win64 W3 IR must contain Win32 concurrency/fiber symbol {expected}"
        );
    }
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
    ] {
        assert!(
            !ir.contains(forbidden),
            "Win64 W3 IR must not contain Linux POSIX symbol {forbidden}"
        );
    }
}

#[test]
fn win64_safety_negative_rejected_before_codegen() {
    let use_after_move = r#"
resource Handle {
    fd: i32;
}

impl Drop for Handle {
    fn drop(self: Self) {
        let _ = self.fd
    }
}

fn consume(h: Handle) -> i32 {
    return h.fd
}

fn main() -> i32 {
    let h = Handle { fd: 9 }
    let a = consume(give h)
    let b = consume(give h)
    return a + b
}
"#;
    let source = SourceUnit::new(FileId(1), ModuleId(1), "win64_neg.zil", use_after_move);
    let options = CompileOptions {
        stop_after: Stage::Build,
        target: TargetProfile::windows_x64_llvm(),
        backend: NativeBackend::Clang,
        ..CompileOptions::default()
    };
    let report = compile_source(&source, &options);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "use-after-move must be SafeReject on Windows target too"
    );
    assert_ne!(
        report.phase,
        zil_base::Phase::Codegen,
        "safety violation must be rejected before Codegen, got {:?}",
        report
    );
}

#[cfg(target_os = "windows")]
#[test]
fn win64_native_o0_and_o2_execute_core_language_and_c_observer() {
    use std::path::PathBuf;
    use std::process::Command;
    use zil_backend_llvm::{BodyBuildOptions, Optimization, build_verified_body};

    let dir = std::env::temp_dir().join(format!("zil-win64-native-w1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let c_path = dir.join("win64_observer.c");
    std::fs::write(
        &c_path,
        r#"
#include <stdint.h>
static int32_t g_sum = 0;
void win64_obs_drop(int32_t v) { g_sum += v; }
int32_t win64_obs_total(void) { return g_sum; }
"#,
    )
    .expect("write win64_observer.c");

    let verified = verify_body_program(&[("win64_exec.zil", WIN64_EXEC_PROGRAM)])
        .expect("Win64 native execution program must verify");

    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }

    for (label, backend, opt) in configs {
        let exe: PathBuf = dir.join(format!("win64_exec_{label}.exe"));
        build_verified_body(
            &verified,
            &BodyBuildOptions {
                output: exe.clone(),
                optimization: opt,
                backend,
                target: TargetProfile::windows_x64_llvm(),
                windows: None,
                clang: None,
                extra_c_sources: vec![c_path.clone()],
                emit_c_main: true,
                entry: None,
                codegen_units: 1,
                codegen_jobs: 1,
            },
        )
        .unwrap_or_else(|e| panic!("Win64 native build ({label}) failed: {}", e.message));

        let output = Command::new(&exe)
            .output()
            .unwrap_or_else(|e| panic!("failed to run Win64 exe ({label}): {e}"));
        assert_eq!(
            output.status.code(),
            Some(58),
            "expected Win64 exit code 58 at {label}, stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Also exercise custom --entry printing via MSVC UCRT printf (legacy_stdio_definitions).
    let custom_exe = dir.join("win64_custom_entry.exe");
    let custom_verified = verify_body_program(&[(
        "win64_entry.zil",
        "fn helper() -> i64 { return 123456789 }\nfn main() -> i32 { return 0 }\n",
    )])
    .expect("custom entry program must verify");
    build_verified_body(
        &custom_verified,
        &BodyBuildOptions {
            output: custom_exe.clone(),
            optimization: Optimization::O2,
            backend: NativeBackend::Clang,
            target: TargetProfile::windows_x64_llvm(),
            windows: None,
            clang: None,
            extra_c_sources: Vec::new(),
            emit_c_main: true,
            entry: Some("helper".into()),
            codegen_units: 1,
            codegen_jobs: 1,
        },
    )
    .expect("Win64 custom --entry build must succeed with MSVC UCRT printf");
    let out = Command::new(&custom_exe)
        .output()
        .expect("run Win64 custom entry exe");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "123456789");

    let _ = std::fs::remove_dir_all(&dir);
}
