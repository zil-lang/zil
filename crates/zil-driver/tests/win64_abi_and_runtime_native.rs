//! Windows x64 MSVC ABI & core runtime verification suite (Win64 W2):
//! - Cross-platform verification of multi-unit Win64 IR linkage, every W2 ZIL
//!   program's independent `zil-check` + MSVC IR emission, and C-ABI safety negatives.
//! - Real native Windows x64 execution tests (O0 & O2):
//!   1. Complete C-ABI scalar/float/NaN/POD-ref & 9-arg CCallback matrix (`tests/integers/ffi.{zil,c}`)
//!   2. 16-arg mixed int/float ABI (4-reg + 32-byte shadow space + stack spill), large closures (>80B),
//!      and 64 KiB local stack frame probing (`__chkstk`)
//!   3. Sync C-host `Registration` (`c_register`) & repo `c_with_callback` fixtures
//!   4. Ordered `AtomicI64`/`AtomicUsize` (all 5 `MemoryOrder`s) & integer/store domain fixtures
//!   5. Multi-unit COFF `.obj` compilation & linking (zero `LNK2005` on TLS/runtime helpers)
//!   6. Deterministic Win32 `HeapAlloc` OOM failure injection with zero-leak & exact Drop cleanup

use zil_backend_llvm::{
    NativeBackend, emit_verified_body_for_target, emit_verified_body_units_for_target,
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::body_pipeline::verify_body_program;
use zil_driver::{CompileOptions, Stage, compile_source};

const MULTI_UNIT_WIN64_SRC: &str = r#"
extern "C" fn zil_glibc_errno_exchange(next: i32) -> i32;

fn step_a(x: i32) -> i32 {
    return x + 10
}

fn step_b(x: i32) -> i32 {
    return step_a(x) * 2
}

fn step_c(x: i32) -> i32 {
    return step_b(x) + 3
}

fn main() -> i32 {
    var prev: i32 = 0
    unsafe {
        let _ = zil_glibc_errno_exchange(7)
        prev = zil_glibc_errno_exchange(0)
    }
    // step_c(5) = ((5 + 10) * 2) + 3 = 33; 33 + 7 = 40
    return step_c(5) + prev
}
"#;

const SHADOW_STACK_PROBE_SRC: &str = r#"
extern "C" fn win64_sum16(
    a0: i64, a1: i64, a2: i64, a3: i64, a4: i64, a5: i64, a6: i64, a7: i64,
    f0: f64, f1: f64, f2: f64, f3: f64, f4: f64, f5: f64, f6: f64, f7: f64,
) -> i64;

extern "C" fn win64_invoke_cb10(
    cb: extern "C" fn(i64, i64, i64, i64, i64, f64, f64, f64, f64, f64) -> i64
) -> i64 contract { provides(static_callbacks()); effects(ffi); };

fn zil_cb10(
    a0: i64, a1: i64, a2: i64, a3: i64, a4: i64,
    f0: f64, f1: f64, f2: f64, f3: f64, f4: f64,
) -> i64 {
    if f0 + f1 + f2 + f3 + f4 != 15.0f64 { return -1 }
    return a0 + a1 + a2 + a3 + a4
}

struct ChunkWords { a: i64; b: i64; c: i64; d: i64; e: i64; f: i64; g: i64; h: i64 }
struct HalfKiloChunk { s: [ChunkWords; 8] }
struct PageChunk { s: [HalfKiloChunk; 8] }
struct LargeStackFrame { p: [PageChunk; 16] }

fn zero_page() -> PageChunk {
    let w = ChunkWords { a: 0, b: 0, c: 0, d: 0, e: 0, f: 0, g: 0, h: 0 }
    let half = HalfKiloChunk { s: [w, w, w, w, w, w, w, w] }
    return PageChunk { s: [half, half, half, half, half, half, half, half] }
}

// Allocate and touch a 64 KiB stack frame across 16 Windows 4 KiB guard pages
// to prove LLVM emits and links MSVC __chkstk stack probes properly.
fn touch_large_stack_frame(seed: i64) -> i64 {
    let z = zero_page()
    var big = LargeStackFrame { p: [z, z, z, z, z, z, z, z, z, z, z, z, z, z, z, z] }
    big.p[0].s[0].s[0].a = seed
    big.p[1].s[0].s[0].a = seed + 1
    big.p[4].s[3].s[2].d = seed + 2
    big.p[8].s[4].s[4].e = seed + 3
    big.p[12].s[6].s[6].g = seed + 4
    big.p[15].s[7].s[7].h = seed + 5
    let s0 = big.p[0].s[0].s[0].a + big.p[1].s[0].s[0].a + big.p[4].s[3].s[2].d
    let s1 = big.p[8].s[4].s[4].e + big.p[12].s[6].s[6].g + big.p[15].s[7].s[7].h
    return s0 + s1
}

fn call_twice<F: FnMut(i64) -> i64>(f: &mut F, x: i64) -> i64 {
    let r1 = f(x)
    let r2 = f(x + 1)
    return r1 + r2
}

fn main() -> i32 {
    var s16: i64 = 0
    var cb_res: i64 = 0
    unsafe {
        s16 = win64_sum16(
            1, 2, 3, 4, 5, 6, 7, 8,
            1.0f64, 2.0f64, 3.0f64, 4.0f64, 5.0f64, 6.0f64, 7.0f64, 8.0f64,
        )
        cb_res = win64_invoke_cb10(c_callback(zil_cb10))
    }
    if s16 != 36 { return 1 }
    if cb_res != 15 { return 2 }

    // 64 KiB stack probe check: 10 + 11 + 12 + 13 + 14 + 15 = 75
    if touch_large_stack_frame(10) != 75 { return 3 }

    // Large closure environment (>80 bytes) mixing owned ByteBuffer + 8 scalars.
    let buf = bytes_new()
    bytes_reserve(buf, 8)
    bytes_push(buf, 7u8)
    let k0: i64 = 10
    let k1: i64 = 20
    let k2: i64 = 30
    let k3: i64 = 40
    let k4: i64 = 50
    let k5: i64 = 60
    let k6: i64 = 70
    let k7: i64 = 80
    var acc: i64 = 0
    var large_cb = move (x: i64) -> i64 => {
        acc = acc + x + k0 + k1 + k2 + k3 + k4 + k5 + k6 + k7 + bytes_len(buf)
        return acc
    }
    let total = call_twice(&mut large_cb, 1)
    if total != 1087 { return 4 }
    return 0
}
"#;

const SYNC_REG_SRC: &str = r#"
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
        let fail_tag = Tag { code: 70 }
        let fail_cb = move (x: i32) -> i32 => x + fail_tag.code
        let bad = c_register(sync_register_fail, sync_close, fail_cb)
        if registration_valid(bad) { return 11 }
        registration_close(give bad)

        let tag = Tag { code: 65 }
        var acc: i32 = 10
        let cb = move (x: i32) -> i32 => {
            acc = acc + x
            return acc + tag.code
        }
        let reg = install_ok(cb)
        if !registration_valid(reg) { return 12 }
        if sync_invoke(5) != 80 { return 13 }
        if sync_invoke(7) != 87 { return 14 }
        registration_close(give reg)
        if sync_closes() != 1 { return 15 }
        if sync_invoke(1) != -1 { return 16 }
        return 0
    }
}
"#;

const ATOMICS_ORDERED_SRC: &str = r#"
fn main() -> i32 {
    let init64: i64 = 10
    let (ok1, r1) = atomic_scope(init64, (cell: &AtomicI64) -> i32 => {
        if atomic_load_ordered(cell, MemoryOrder.Acquire) != 10 { return 1 }
        atomic_store_ordered(cell, 25, MemoryOrder.Release)
        let prev = atomic_fetch_add_ordered(cell, 5, MemoryOrder.AcqRel)
        if prev != 25 { return 2 }
        let ex = atomic_exchange_ordered(cell, 40, MemoryOrder.SeqCst)
        if ex != 30 { return 3 }
        let (obs, won) = atomic_compare_exchange_ordered(
            cell, 40, 99, MemoryOrder.SeqCst, MemoryOrder.Acquire,
        )
        if !won || obs != 40 { return 4 }
        if atomic_load_ordered(cell, MemoryOrder.Relaxed) != 99 { return 5 }
        return 0
    })
    if !ok1 || r1 != 0 { return 10 + r1 }

    let init_u: usize = 42
    let (ok2, r2) = atomic_scope(init_u, (cell: &AtomicUsize) -> i32 => {
        let s: usize = 100
        let d: usize = 23
        let bad: usize = 999
        let one: usize = 1
        atomic_store(cell, s)
        let old = atomic_fetch_add(cell, d)
        let (obs, won) = atomic_compare_exchange(cell, bad, one)
        if old != 100 || obs != 123 || won { return 6 }
        return 0
    })
    if !ok2 || r2 != 0 { return 20 + r2 }
    return 0
}
"#;

const OOM_WORKLOAD_SRC: &str = r#"
extern "C" fn oom_arm(fail_after: i64);
extern "C" fn oom_disarm();
extern "C" fn oom_live_count() -> i64;
extern "C" fn oom_double_frees() -> i64;
extern "C" fn oom_record_drop(n: i64);
extern "C" fn oom_drop_count() -> i64;
extern "C" fn oom_reset_drops();

trait Worker {
    fn work(self: &Self) -> i64;
}

resource Item {
    weight: i64;
}

impl Drop for Item {
    fn drop(self: Self) {
        unsafe { oom_record_drop(self.weight) }
    }
}

package DynItem {
    item: Managed<Item>;
}

impl Worker for DynItem {
    fn work(self: &Self) -> i64 {
        return self.item.weight * 2
    }
}

fn run_workload() -> i64 {
    let buf = bytes_new()
    if !bytes_reserve(buf, 16) { return 0 }
    if !bytes_push(buf, 1u8) { return 0 }
    if !bytes_push(buf, 2u8) { return 0 }

    let st = store_new()
    if !store_reserve(st, 4) { return 0 }
    match store_push(st, Item { weight: 3 }) {
        .ok(_) => {}
        .err(_) => { return 0 }
    }
    match store_push(st, Item { weight: 5 }) {
        .ok(_) => {}
        .err(_) => { return 0 }
    }

    var dyn_val: i64 = 0
    let third = Item { weight: 7 }
    match owned_dyn::<dyn Worker>(DynItem { item: give third }) {
        .ok(obj) => { dyn_val = obj.work() }
        .err(_) => { return 0 }
    }
    return dyn_val + bytes_len(buf) + store_len(st)
}

fn main() -> i32 {
    var k: i64 = 0
    while k < 12 {
        unsafe {
            oom_reset_drops()
            oom_arm(k)
        }
        let res = run_workload()
        var live: i64 = 0
        var df: i64 = 0
        var drops: i64 = 0
        unsafe {
            oom_disarm()
            live = oom_live_count()
            df = oom_double_frees()
            drops = oom_drop_count()
        }
        if live != 0 { return 10 + i32(k) }
        if df != 0 { return 40 + i32(k) }
        // On full success (res == 18), all 3 Items (3 + 5 + 7 = 15) must have been dropped cleanly.
        if res == 18 && drops != 15 { return 70 }
        k = k + 1
    }
    return 0
}
"#;

#[test]
fn win64_all_w2_programs_verify_and_emit_clean_msvc_ir() {
    for (name, src) in [
        ("multi_unit.zil", MULTI_UNIT_WIN64_SRC),
        ("shadow_stack_probe.zil", SHADOW_STACK_PROBE_SRC),
        ("sync_reg.zil", SYNC_REG_SRC),
        ("atomics_ordered.zil", ATOMICS_ORDERED_SRC),
        ("oom_workload.zil", OOM_WORKLOAD_SRC),
    ] {
        let verified = verify_body_program(&[(name, src)])
            .unwrap_or_else(|e| panic!("{name} must pass independent zil-check: {e:?}"));
        let ir = emit_verified_body_for_target(&verified, true, "x86_64-pc-windows-msvc")
            .unwrap_or_else(|e| panic!("{name} Win64 IR emission failed: {e:?}"));
        assert!(ir.contains("target triple = \"x86_64-pc-windows-msvc\""));
        assert!(!ir.contains("@pthread_create"));
        assert!(!ir.contains("@swapcontext"));
    }
}

#[test]
fn win64_multi_unit_ir_defines_runtime_only_in_unit_zero() {
    let verified = verify_body_program(&[("multi_unit.zil", MULTI_UNIT_WIN64_SRC)])
        .expect("multi-unit Win64 program must pass independent zil-check");
    let units = emit_verified_body_units_for_target(&verified, true, 3, "x86_64-pc-windows-msvc")
        .expect("Win64 multi-unit IR emission must succeed");
    assert_eq!(units.len(), 3, "expected 3 distinct IR units");

    assert!(units[0].contains("@__zil_current_task = thread_local global ptr null"));
    assert!(units[0].contains("@__zil_task_cancel_trap = thread_local global i1 false"));
    assert!(units[0].contains("@zil_process_argc = global i32 0"));
    assert!(units[0].contains("@zil_process_argv = global ptr null"));
    assert!(units[0].contains("define void @__zil_reg_drop_frame(ptr %frame)"));
    assert!(units[0].contains("define i32 @main("));

    for (idx, unit) in units.iter().enumerate().skip(1) {
        assert!(
            unit.contains("target triple = \"x86_64-pc-windows-msvc\""),
            "unit {idx} missing Win64 triple"
        );
        assert!(
            unit.contains("@__zil_current_task = external thread_local global ptr"),
            "unit {idx} must declare @__zil_current_task externally"
        );
        assert!(
            unit.contains("@__zil_task_cancel_trap = external thread_local global i1"),
            "unit {idx} must declare @__zil_task_cancel_trap externally"
        );
        assert!(
            unit.contains("@zil_process_argc = external global i32"),
            "unit {idx} must declare @zil_process_argc externally"
        );
        assert!(
            unit.contains("@zil_process_argv = external global ptr"),
            "unit {idx} must declare @zil_process_argv externally"
        );
        assert!(
            unit.contains("declare void @__zil_reg_drop_frame(ptr)"),
            "unit {idx} must declare @__zil_reg_drop_frame externally"
        );
        assert!(
            !unit.contains("define void @__zil_reg_drop_frame"),
            "unit {idx} must not redefine @__zil_reg_drop_frame"
        );
        assert!(
            !unit.contains("define i32 @main("),
            "unit {idx} must not redefine @main"
        );
    }
}

#[test]
fn win64_c_abi_safety_negatives_rejected_before_codegen() {
    let cases = [
        (
            "non_pod_resource_in_extern_c",
            r#"
resource Owned { fd: i32; }
impl Drop for Owned { fn drop(self: Self) { let _ = self.fd } }
extern "C" fn bad_ref(p: &Owned) -> i32 contract { provides(noescape(p)); effects(ffi); };
fn main() -> i32 { return 0 }
"#,
        ),
        (
            "missing_noescape_on_pod_ref",
            r#"
struct Pod { x: i32; }
extern "C" fn bad_escape(p: &Pod) -> i32;
fn main() -> i32 { return 0 }
"#,
        ),
        (
            "capturing_closure_to_c_callback",
            r#"
extern "C" fn take_cb(cb: extern "C" fn(i32) -> i32) -> i32 contract { provides(static_callbacks()); effects(ffi); };
fn main() -> i32 {
    let bias: i32 = 5
    let cb = (x: i32) -> i32 => x + bias
    unsafe { return take_cb(c_callback(cb)) }
}
"#,
        ),
    ];

    for (name, src) in cases {
        let source = SourceUnit::new(FileId(1), ModuleId(1), format!("{name}.zil"), src);
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
            "{name} must be SafeReject on Win64"
        );
        assert_ne!(
            report.phase,
            Phase::Codegen,
            "{name} must be rejected before Codegen, got {:?}",
            report
        );
    }
}

#[cfg(target_os = "windows")]
mod native_windows {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use zil_backend_llvm::{BodyBuildOptions, Optimization, build_verified_body};
    use zil_driver::{CodegenOptions, CompilationSession};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zil-win64-w2-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn build_and_run_o0_o2(
        dir: &Path,
        entry: &Path,
        extra_c_sources: &[PathBuf],
        clang_override: Option<PathBuf>,
    ) -> (i32, String) {
        let mut expected: Option<(i32, String)> = None;
        for (label, opt) in [("o0", Optimization::O0), ("o2", Optimization::O2)] {
            let exe = dir.join(format!("bin-{label}.exe"));
            let mut session = CompilationSession::new();
            let opts = CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
                backend: NativeBackend::Clang,
                target: TargetProfile::windows_x64_llvm(),
                clang: clang_override.clone(),
                output: Some(exe.clone()),
                extra_c_sources: extra_c_sources.to_vec(),
                codegen: CodegenOptions { units: 1, jobs: 1 },
                ..CompileOptions::default()
            };
            let report = session.compile_path(entry, &opts);
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "Win64 build failed for {} ({label}): {:?}",
                entry.display(),
                report.diagnostics
            );
            assert_eq!(report.verification_runs, 1);

            let out = Command::new(&exe)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            assert!(
                stderr.is_empty(),
                "unexpected stderr for {} ({label}): {stderr}",
                entry.display()
            );
            let code = out.status.code().unwrap_or(-1);
            if let Some((prev_code, ref prev_stdout)) = expected {
                assert_eq!(code, prev_code, "O0/O2 exit code mismatch on {label}");
                assert_eq!(&stdout, prev_stdout, "O0/O2 stdout mismatch on {label}");
            } else {
                expected = Some((code, stdout));
            }
        }
        expected.expect("at least one opt level ran")
    }

    #[test]
    fn win64_c_abi_all_scalars_floats_nan_pod_refs_and_9_arg_callbacks() {
        let root = repo_root();
        let ffi_zil = root.join("tests/integers/ffi.zil");
        let ffi_c = root.join("tests/integers/ffi.c");
        let dir = temp_dir("integers-ffi");
        let mut src = std::fs::read_to_string(&ffi_zil).expect("read ffi.zil");
        src.push_str("\nfn main()->i32 { return result() }\n");
        let entry = dir.join("ffi_main.zil");
        std::fs::write(&entry, src).expect("write ffi_main.zil");
        let (code, _) = build_and_run_o0_o2(&dir, &entry, &[ffi_c], None);
        assert_eq!(code, 0, "Win64 C-ABI ffi.zil+ffi.c matrix must exit 0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn win64_16_arg_shadow_space_large_closure_and_64k_stack_probe() {
        let dir = temp_dir("shadow-stack-probe");
        let c_host = dir.join("win64_abi_host.c");
        std::fs::write(
            &c_host,
            r#"
#include <stdint.h>

int64_t win64_sum16(
    int64_t a0, int64_t a1, int64_t a2, int64_t a3,
    int64_t a4, int64_t a5, int64_t a6, int64_t a7,
    double f0, double f1, double f2, double f3,
    double f4, double f5, double f6, double f7
) {
    double fs = f0 + f1 + f2 + f3 + f4 + f5 + f6 + f7;
    if (fs != 36.0) return -1;
    return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7;
}

typedef int64_t (*cb10_t)(
    int64_t, int64_t, int64_t, int64_t, int64_t,
    double, double, double, double, double
);

int64_t win64_invoke_cb10(cb10_t cb) {
    return cb(1, 2, 3, 4, 5, 1.0, 2.0, 3.0, 4.0, 5.0);
}
"#,
        )
        .expect("write win64_abi_host.c");

        let entry = dir.join("main.zil");
        std::fs::write(&entry, SHADOW_STACK_PROBE_SRC).expect("write main.zil");
        let (code, _) = build_and_run_o0_o2(&dir, &entry, &[c_host], None);
        assert_eq!(code, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn win64_sync_registration_and_scoped_callback_with_c_host() {
        let dir = temp_dir("sync-registration");
        let c_host = dir.join("sync_host.c");
        std::fs::write(
            &c_host,
            r#"
#include <stdint.h>

typedef int64_t (*cb_fn)(void *, int32_t);
static cb_fn saved_cb = 0;
static void *saved_ctx = 0;
static int32_t closes = 0;
static int32_t token_slot = 0;

void *sync_register_ok(cb_fn cb, void *ctx) {
    saved_cb = cb;
    saved_ctx = ctx;
    return &token_slot;
}

void *sync_register_fail(cb_fn cb, void *ctx) {
    (void)cb;
    (void)ctx;
    return 0;
}

void sync_close(void *tok) {
    if (tok == &token_slot) {
        closes += 1;
        saved_cb = 0;
        saved_ctx = 0;
    }
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

        let entry = dir.join("main.zil");
        std::fs::write(&entry, SYNC_REG_SRC).expect("write main.zil");
        let (code, stdout) = build_and_run_o0_o2(&dir, &entry, &[c_host], None);
        assert_eq!(code, 0);
        assert_eq!(stdout, "FA", "expected fail drop 'F' then close drop 'A'");
        let _ = std::fs::remove_dir_all(&dir);

        // Also run the repo's foreign-scoped-callback fixtures natively on Windows x64.
        let root = repo_root().join("tests/foreign-scoped-callback/direct");
        let dir1 = temp_dir("scoped-cb-ok");
        let (code1, _) = build_and_run_o0_o2(
            &dir1,
            &root.join("direct.zil"),
            &[root.join("driver.c")],
            None,
        );
        assert_eq!(code1, 0);
        let _ = std::fs::remove_dir_all(&dir1);

        let dir2 = temp_dir("scoped-cb-null");
        let (code2, _) = build_and_run_o0_o2(
            &dir2,
            &root.join("direct-null.zil"),
            &[root.join("driver-null.c")],
            None,
        );
        assert_eq!(code2, 0);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn win64_sync_cell_ordered_atomics_and_integer_domains() {
        let root = repo_root();
        for (name, path) in [
            (
                "integer_widths",
                root.join("tests/fixtures/numeric-text/integer-widths.zil"),
            ),
            (
                "complete_domains",
                root.join("tests/integers/complete-domains.zil"),
            ),
            (
                "store_domains",
                root.join("tests/integers/store-domains.zil"),
            ),
        ] {
            let dir = temp_dir(name);
            let mut src = std::fs::read_to_string(&path).expect("read fixture");
            if !src.contains("fn main(") && src.contains("fn result(") {
                src.push_str("\nfn main()->i32 { return result() }\n");
            }
            let entry = dir.join(format!("{name}.zil"));
            std::fs::write(&entry, src).expect("write fixture");
            let (code, _) = build_and_run_o0_o2(&dir, &entry, &[], None);
            assert_eq!(code, 0, "{name} must exit 0 on Win64");
            let _ = std::fs::remove_dir_all(&dir);
        }

        let dir = temp_dir("atomics");
        let entry = dir.join("atomics.zil");
        std::fs::write(&entry, ATOMICS_ORDERED_SRC).expect("write atomics.zil");
        let (code, _) = build_and_run_o0_o2(&dir, &entry, &[], None);
        assert_eq!(code, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn win64_multi_unit_coff_link_and_execute() {
        let dir = temp_dir("multi-unit-coff");
        let verified = verify_body_program(&[("multi_unit.zil", MULTI_UNIT_WIN64_SRC)])
            .expect("multi-unit Win64 program must verify");
        let units =
            emit_verified_body_units_for_target(&verified, true, 3, "x86_64-pc-windows-msvc")
                .expect("emit 3 Win64 IR units");
        assert_eq!(units.len(), 3);

        for (label, opt_flag) in [("o0", "-O0"), ("o2", "-O2")] {
            let mut objs = Vec::new();
            for (idx, unit_ir) in units.iter().enumerate() {
                let ll = dir.join(format!("u{idx}_{label}.ll"));
                let obj = dir.join(format!("u{idx}_{label}.obj"));
                std::fs::write(&ll, unit_ir).expect("write unit ll");
                let status = Command::new("clang")
                    .args([
                        "--target=x86_64-pc-windows-msvc",
                        "-x",
                        "ir",
                        opt_flag,
                        "-c",
                    ])
                    .arg(&ll)
                    .arg("-o")
                    .arg(&obj)
                    .status()
                    .expect("invoke clang -c");
                assert!(status.success(), "unit {idx} ({label}) compile failed");
                objs.push(obj);
            }
            let exe = dir.join(format!("multi_{label}.exe"));
            let mut link = Command::new("clang");
            link.args(["--target=x86_64-pc-windows-msvc", opt_flag]);
            for obj in &objs {
                link.arg(obj);
            }
            link.arg("-o").arg(&exe).arg("-llegacy_stdio_definitions");
            let out = link.output().expect("link multi-unit COFF exe");
            assert!(
                out.status.success(),
                "multi-unit COFF link ({label}) failed:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let run = Command::new(&exe).output().expect("run multi-unit exe");
            assert_eq!(run.status.code(), Some(40), "expected exit 40 at {label}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn win64_oom_injection_zero_leak_and_drop_cleanup() {
        let dir = temp_dir("oom-injection");
        let alloc_c = dir.join("win64_oom_alloc.c");
        std::fs::write(
            &alloc_c,
            r#"
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <windows.h>

#define MAGIC_LIVE 0x5A494C57494E3634ULL
#define MAGIC_DEAD 0xDEAD5A494C465245ULL

typedef struct {
    uint64_t magic;
    uint64_t tracked_and_size; // bit 63 = tracked, bits 0..62 = payload bytes
} AllocHdr;

static int32_t g_armed = 0;
static int64_t g_fail_after = -1;
static int64_t g_live = 0;
static int64_t g_double_frees = 0;
static int64_t g_drops = 0;

void oom_arm(int64_t fail_after) {
    g_fail_after = fail_after;
    g_armed = 1;
}

void oom_disarm(void) {
    g_armed = 0;
    g_fail_after = -1;
}

int64_t oom_live_count(void) { return g_live; }
int64_t oom_double_frees(void) { return g_double_frees; }
void oom_record_drop(int64_t n) { g_drops += n; }
int64_t oom_drop_count(void) { return g_drops; }
void oom_reset_drops(void) { g_drops = 0; }

static int should_fail(void) {
    if (!g_armed || g_fail_after < 0) return 0;
    if (g_fail_after == 0) return 1;
    g_fail_after -= 1;
    return 0;
}

void *malloc(size_t size) {
    if (should_fail()) return NULL;
    size_t total = sizeof(AllocHdr) + (size ? size : 1);
    AllocHdr *h = (AllocHdr *)HeapAlloc(GetProcessHeap(), 0, total);
    if (!h) return NULL;
    h->magic = MAGIC_LIVE;
    h->tracked_and_size = ((uint64_t)size) | (g_armed ? (1ULL << 63) : 0ULL);
    if (g_armed) g_live += 1;
    return (void *)(h + 1);
}

void *calloc(size_t n, size_t sz) {
    if (should_fail()) return NULL;
    if (sz != 0 && n > ((size_t)-1) / sz) return NULL;
    size_t bytes = n * sz;
    size_t total = sizeof(AllocHdr) + (bytes ? bytes : 1);
    AllocHdr *h = (AllocHdr *)HeapAlloc(GetProcessHeap(), HEAP_ZERO_MEMORY, total);
    if (!h) return NULL;
    h->magic = MAGIC_LIVE;
    h->tracked_and_size = ((uint64_t)bytes) | (g_armed ? (1ULL << 63) : 0ULL);
    if (g_armed) g_live += 1;
    return (void *)(h + 1);
}

void *realloc(void *p, size_t size) {
    if (!p) return malloc(size);
    if (size == 0) {
        free(p);
        return NULL;
    }
    if (should_fail()) return NULL;
    AllocHdr *old_h = ((AllocHdr *)p) - 1;
    if (old_h->magic != MAGIC_LIVE) return NULL;
    uint64_t was_tracked = old_h->tracked_and_size & (1ULL << 63);
    size_t total = sizeof(AllocHdr) + size;
    AllocHdr *new_h = (AllocHdr *)HeapReAlloc(GetProcessHeap(), 0, old_h, total);
    if (!new_h) return NULL;
    new_h->magic = MAGIC_LIVE;
    new_h->tracked_and_size = ((uint64_t)size) | was_tracked;
    return (void *)(new_h + 1);
}

void free(void *p) {
    if (!p) return;
    AllocHdr *h = ((AllocHdr *)p) - 1;
    if (h->magic == MAGIC_DEAD) {
        g_double_frees += 1;
        return;
    }
    if (h->magic != MAGIC_LIVE) return;
    if (h->tracked_and_size & (1ULL << 63)) {
        g_live -= 1;
    }
    h->magic = MAGIC_DEAD;
    HeapFree(GetProcessHeap(), 0, h);
}
"#,
        )
        .expect("write win64_oom_alloc.c");

        let verified = verify_body_program(&[("win64_oom.zil", OOM_WORKLOAD_SRC)])
            .expect("OOM program must verify");

        let clang_wrap = dir.join("clang-force.cmd");
        std::fs::write(&clang_wrap, "@echo off\r\nclang -Wl,/FORCE:MULTIPLE %*\r\n")
            .expect("write clang-force.cmd");

        for (label, opt) in [("o0", Optimization::O0), ("o2", Optimization::O2)] {
            let exe = dir.join(format!("oom_{label}.exe"));
            build_verified_body(
                &verified,
                &BodyBuildOptions {
                    output: exe.clone(),
                    optimization: opt,
                    backend: NativeBackend::Clang,
                    target: TargetProfile::windows_x64_llvm(),
                    windows: None,
                    clang: Some(clang_wrap.clone()),
                    extra_c_sources: vec![alloc_c.clone()],
                    emit_c_main: true,
                    entry: None,
                    codegen_units: 1,
                    codegen_jobs: 1,
                },
            )
            .unwrap_or_else(|e| panic!("Win64 OOM build ({label}) failed: {}", e.message));

            let out = Command::new(&exe)
                .output()
                .unwrap_or_else(|e| panic!("run Win64 OOM exe ({label}): {e}"));
            assert_eq!(
                out.status.code(),
                Some(0),
                "Win64 OOM test ({label}) failed with code {:?}, stderr={}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
