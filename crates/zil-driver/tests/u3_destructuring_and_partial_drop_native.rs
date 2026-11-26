//! End-to-end dual-backend native execution and safety rejection tests for
//! Batch U3 (L1 Core Usability — G1.4 Mixed Destructuring & Conditional
//! Partial Refill `drop_elab`):
//! 1. Nested reference patterns (`&Struct{..}`, `&mut Enum.Variant(ref mut x)`,
//!    `&(a, b)`, `&.ok(ref v)` / `&.err(ref e)`, `&&val`, nested `ref` patterns).
//! 2. Conditional partial move and conditional/unconditional partial refill
//!    drop elaboration for `Package`, `Record` (`struct`), `Tuple`, and nested
//!    products, verified with exact `Drop` stdout traces and `ByteBuffer`/`Store`.
//! 3. `ErrorUnion` conditional move and partial destructuring cleanup.
//! 4. `SafeReject` safety negatives for illegal moves/mut-borrows through
//!    nested reference patterns and unrefilled conditional partial moves.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-u3-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, src).expect("write source");
    path
}

fn build_and_run_dual(dir: &Path, entry: &Path) -> (i32, String) {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }

    let mut expected: Option<(i32, String)> = None;
    for (label, backend, opt) in configs {
        let exe = dir.join(format!("bin-{label}{}", std::env::consts::EXE_SUFFIX));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
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
        let code = out.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if let Some((prev_code, ref prev_stdout)) = expected {
            assert_eq!(code, prev_code, "exit code mismatch on {label}");
            assert_eq!(stdout, *prev_stdout, "stdout mismatch on {label}");
        } else {
            expected = Some((code, stdout));
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
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "expected `{name}` to be rejected, but it succeeded"
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
fn u3_nested_reference_patterns_native() {
    let dir = temp_dir("nested-ref-patterns");
    let src = r#"
struct Point {
    x: i32
    y: i32
}

enum Entry {
    Val(i32),
    Pair(i32, i32),
    Empty
}

enum Fault {
    Missing(i32)
}

package BufWrap {
    manages buf: ByteBuffer
    tag: i32
}

fn sum_point_ref(r: &Point) -> i32 {
    let &Point { x: a, y: b } = r
    return a + b
}

fn sum_tuple_ref(r: &(i32, i32)) -> i32 {
    let &(a, b) = r
    return a * 10 + b
}

fn bump_entry_mut(r: &mut Entry) -> i32 {
    match r {
        &mut Entry.Val(ref mut x) => {
            *x += 7
            return *x
        },
        &mut Entry.Pair(ref mut a, ref mut b) => {
            *a += 1
            *b += 2
            return *a + *b
        },
        &mut Entry.Empty() => {
            return 0
        }
    }
}

fn inspect_error_union_ref(r: &(Fault!BufWrap)) -> i32 {
    match r {
        &.ok(ref wrap) => {
            return i32(bytes_len(&wrap.buf)) + wrap.tag
        },
        &.err(ref e) => {
            match e {
                &Fault.Missing(code) => {
                    return code
                }
            }
        }
    }
}

fn double_ref_read(r: &(&i32)) -> i32 {
    let &&v = r
    return v
}

fn read_wrap_ref(r: &BufWrap) -> i32 {
    match r {
        &BufWrap { buf: ref b, tag: t } => {
            return i32(bytes_len(b)) + t
        }
    }
}

fn main() -> i32 {
    // 1. `&Struct{..}` in `let` and `match` (including temporary `&p`)
    let p = Point { x: 4, y: 5 }
    let s1 = sum_point_ref(&p)
    var s2: i32 = 0
    match &p {
        &Point { x: ref rx, y: 5 } => { s2 = *rx },
        &Point { x: _, y: _ } => { s2 = 0 }
    }

    // 2. `&(a, b)`
    let tup: (i32, i32) = (2, 3)
    let s3 = sum_tuple_ref(&tup)

    // 3. `&mut Enum.Variant(ref mut x)`
    var e = Entry.Val(10)
    let s4 = bump_entry_mut(&mut e)

    // 4. `&.ok(ref v)` and `&.err(ref e)`
    var buf: ByteBuffer = bytes_new()
    bytes_push(&mut buf, 1)
    bytes_push(&mut buf, 2)
    bytes_push(&mut buf, 3)
    let ok_res: Fault!BufWrap = BufWrap { buf: give buf, tag: 0 }
    let err_res: Fault!BufWrap = Fault.Missing(6)
    let s5 = inspect_error_union_ref(&ok_res)
    let s6 = inspect_error_union_ref(&err_res)

    // 5. `&&v` double deref pattern
    let n: i32 = 8
    let rn: &i32 = &n
    let s7 = double_ref_read(&rn)

    // 6. `&Package { buf: ref b, tag: t }` borrowing managed field without moving
    var buf2: ByteBuffer = bytes_new()
    bytes_push(&mut buf2, 9)
    let wrap = BufWrap { buf: give buf2, tag: 2 }
    let s8 = read_wrap_ref(&wrap) + read_wrap_ref(&wrap)

    // 7. Nested `ref (a, b)` binding mode
    let pair: (i32, i32) = (1, 4)
    let ref (pa, pb) = pair
    let s9 = *pa + *pb

    // Expected: s1=9, s2=4, s3=23, s4=17, s5=3, s6=6, s7=8, s8=6, s9=5 => 81
    return s1 + s2 + s3 + s4 + s5 + s6 + s7 + s8 + s9
}
"#;
    let entry = write_source(&dir, "nested_ref.zil", src);
    let (code, stdout) = build_and_run_dual(&dir, &entry);
    assert_eq!(stdout, "");
    assert_eq!(code, 81);
}

#[test]
fn u3_conditional_partial_move_and_refill_drop_elab_native() {
    let dir = temp_dir("partial-move-refill");
    let src = r#"
extern "C" fn putchar(c: i32) -> i32;

resource Probe {
    code: i32
}

impl Drop for Probe {
    fn drop(self: Self) {
        unsafe { putchar(.code); }
    }
}

package PairPkg {
    manages a: Probe
    manages b: Probe
}

package Single {
    manages p: Probe
}

struct PairRec {
    a: Single
    b: Single
}

package OuterPkg {
    inner: PairPkg
    manages c: Probe
}

// 1. Package conditional partial move WITHOUT refill.
fn pkg_cond_move(cond: bool) {
    let p = PairPkg {
        a: Probe { code: 65 },
        b: Probe { code: 66 }
    }
    if cond {
        let moved = give p.a
        // `moved` ('A') drops here; at function exit only `p.b` ('B') drops.
    }
    unsafe { putchar(124); }
}

// 2. Package conditional partial move + conditional refill (all 4 combinations).
fn pkg_cond_move_cond_refill(do_move: bool, do_refill: bool) {
    var p = PairPkg {
        a: Probe { code: 65 },
        b: Probe { code: 66 }
    }
    if do_move {
        let moved = give p.a
    }
    if do_refill {
        p.a = Probe { code: 67 }
    }
    unsafe { putchar(124); }
}

// 3. Package conditional partial move + UNCONDITIONAL refill.
fn pkg_cond_move_uncond_refill(do_move: bool) -> i32 {
    var p = PairPkg {
        a: Probe { code: 65 },
        b: Probe { code: 66 }
    }
    if do_move {
        let moved = give p.a
    }
    p.a = Probe { code: 67 }
    unsafe { putchar(124); }
    return p.a.code + p.b.code
}

// 4. Record (`struct`) conditional partial move + refill.
fn rec_cond_move(do_move: bool, do_refill: bool) {
    var r = PairRec {
        a: Single { p: Probe { code: 88 } },
        b: Single { p: Probe { code: 89 } }
    }
    if do_move {
        let moved = r.a
    }
    if do_refill {
        r.a = Single { p: Probe { code: 90 } }
    }
    unsafe { putchar(124); }
}

// 5. Tuple conditional partial move + refill.
fn tup_cond_move(do_move: bool, do_refill: bool) {
    var t: (Single, Single) = (
        Single { p: Probe { code: 49 } },
        Single { p: Probe { code: 50 } }
    )
    if do_move {
        let moved = t.0
    }
    if do_refill {
        t.0 = Single { p: Probe { code: 51 } }
    }
    unsafe { putchar(124); }
}

// 6. Nested product conditional partial move (`o.inner.a`).
fn nested_cond_move(do_move: bool) {
    let o = OuterPkg {
        inner: PairPkg {
            a: Probe { code: 97 },
            b: Probe { code: 98 }
        },
        c: Probe { code: 99 }
    }
    if do_move {
        let moved = give o.inner.a
    }
    unsafe { putchar(124); }
}

// 7. Store/ByteBuffer conditional partial move + refill (real heap owners).
package StoreHolder {
    manages items: Store<i64>
    manages buf: ByteBuffer
}

fn store_partial_refill(do_move: bool) -> i32 {
    let s1: Store<i64> = store_new()
    if !store_reserve(s1, 1) { return -1 }
    store_push(s1, 10)
    var b1: ByteBuffer = bytes_new()
    bytes_push(&mut b1, 7)
    var h = StoreHolder { items: give s1, buf: give b1 }
    if do_move {
        let taken = give h.items
        if store_len(taken) != 1 { return -2 }
    }
    let s2: Store<i64> = store_new()
    if !store_reserve(s2, 1) { return -3 }
    store_push(s2, 20)
    h.items = give s2
    return i32(*store_at(h.items, 0)) + i32(bytes_len(&h.buf))
}

fn main() -> i32 {
    // 1. pkg_cond_move:
    //    true  => A|B
    //    false => |AB
    pkg_cond_move(true)
    pkg_cond_move(false)
    unsafe { putchar(59); }

    // 2. pkg_cond_move_cond_refill:
    //    (true,  false) => A|B
    //    (false, false) => |AB
    //    (true,  true)  => A|CB
    //    (false, true)  => A|CB  (old A dropped on refill before '|', then C and B drop after '|')
    pkg_cond_move_cond_refill(true, false)
    pkg_cond_move_cond_refill(false, false)
    pkg_cond_move_cond_refill(true, true)
    pkg_cond_move_cond_refill(false, true)
    unsafe { putchar(59); }

    // 3. pkg_cond_move_uncond_refill:
    //    true  => A|CB
    //    false => A|CB
    let r1 = pkg_cond_move_uncond_refill(true)
    let r2 = pkg_cond_move_uncond_refill(false)
    unsafe { putchar(59); }

    // 4. rec_cond_move:
    //    (true,  false) => X|Y
    //    (false, true)  => X|ZY
    rec_cond_move(true, false)
    rec_cond_move(false, true)
    unsafe { putchar(59); }

    // 5. tup_cond_move:
    //    (true,  false) => 1|2
    //    (false, true)  => 1|32
    tup_cond_move(true, false)
    tup_cond_move(false, true)
    unsafe { putchar(59); }

    // 6. nested_cond_move:
    //    true  => a|bc
    //    false => |abc
    nested_cond_move(true)
    nested_cond_move(false)

    // 7. StoreHolder: both branches return 20 + 1 = 21
    let sh1 = store_partial_refill(true)
    let sh2 = store_partial_refill(false)
    if r1 != 133 { return 2 }
    if r2 != 133 { return 3 }
    if sh1 != 21 { return 4 }
    if sh2 != 21 { return 5 }
    return 0
}
"#;
    let entry = write_source(&dir, "partial_move_refill.zil", src);
    let (code, stdout) = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "A|B|AB;A|B|ABA|CBA|CB;A|CBA|CB;X|YX|ZY;1|21|32;a|bc|abc"
    );
}

#[test]
fn u3_error_union_partial_and_conditional_drop_native() {
    let dir = temp_dir("error-union-drop");
    let src = r#"
extern "C" fn putchar(c: i32) -> i32;

resource Probe {
    code: i32
}

impl Drop for Probe {
    fn drop(self: Self) {
        unsafe { putchar(.code); }
    }
}

package Pair {
    manages a: Probe
    manages b: Probe
}

package ErrOwner {
    manages e: Probe
}

fn make_pair_res(is_ok: bool) -> ErrOwner!Pair {
    if is_ok {
        return Pair {
            a: Probe { code: 65 },
            b: Probe { code: 66 }
        }
    }
    return ErrOwner { e: Probe { code: 69 } }
}

// 1. Partial destructuring of `.ok(Pair { a, b: _ })`, leaving `b` live on `.ok`
//    and `ErrOwner` live on `.err`.
fn partial_ok_destructure(is_ok: bool) {
    let r: ErrOwner!Pair = make_pair_res(is_ok)
    match r {
        .ok(Pair { a: taken_a, b: _ }) => {
            // `taken_a` ('A') drops at end of arm; `r.ok.b` ('B') drops at function exit.
        },
        .err(_) => {
            // `r.err` ('E') drops at function exit.
        }
    }
    unsafe { putchar(124); }
}

package OkOwner {
    manages o: Probe
}

fn make_probe_res(is_ok: bool) -> ErrOwner!OkOwner {
    if is_ok {
        return OkOwner { o: Probe { code: 79 } }
    }
    return ErrOwner { e: Probe { code: 69 } }
}

// 2. Move `.ok(v)` only, leave `.err(_)` to be dropped by `ErrorUnionPartialDrop`.
fn move_ok_only(is_ok: bool) {
    let r: ErrOwner!OkOwner = make_probe_res(is_ok)
    match r {
        .ok(p) => {
            // `p` ('O') drops at end of arm.
        },
        .err(_) => {
            // `r.err` ('E') drops at function exit.
        }
    }
    unsafe { putchar(124); }
}

// 3. Move both `.ok(v)` and `.err(e)` owning payloads.
fn move_both_arms(is_ok: bool) {
    let r: ErrOwner!OkOwner = make_probe_res(is_ok)
    match r {
        .ok(p) => {
            // `p` ('O') drops at end of arm.
        },
        .err(e) => {
            // `e` ('E') drops at end of arm.
        }
    }
    unsafe { putchar(124); }
}

fn main() -> i32 {
    // 1. partial_ok_destructure:
    //    true  => A|B
    //    false => |E
    partial_ok_destructure(true)
    partial_ok_destructure(false)
    unsafe { putchar(59); }

    // 2. move_ok_only:
    //    true  => O|
    //    false => |E
    move_ok_only(true)
    move_ok_only(false)
    unsafe { putchar(59); }

    // 3. move_both_arms:
    //    true  => O|
    //    false => E|
    move_both_arms(true)
    move_both_arms(false)
    return 0
}
"#;
    let entry = write_source(&dir, "error_union_drop.zil", src);
    let (code, stdout) = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 0);
    assert_eq!(stdout, "A|B|E;O||E;O|E|");
}

#[test]
fn u3_safety_negatives_safe_reject() {
    let dir = temp_dir("safety-negatives");

    // Negative 1: Moving a non-Copy field out through a shared `&` nested pattern.
    check_rejected(
        &dir,
        "neg_move_through_shared_ref_pattern",
        r#"
package Holder {
    manages buf: ByteBuffer
}

fn bad(r: &Holder) -> i32 {
    let &Holder { buf: b } = r
    return i32(bytes_len(&b))
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 2: Moving a non-Copy field out through a `&mut` nested pattern.
    check_rejected(
        &dir,
        "neg_move_through_mut_ref_pattern",
        r#"
package Holder {
    manages buf: ByteBuffer
}

fn bad(r: &mut Holder) -> i32 {
    let &mut Holder { buf: b } = r
    return i32(bytes_len(&b))
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 3: Binding `ref mut` through a shared `&` nested pattern.
    check_rejected(
        &dir,
        "neg_ref_mut_through_shared_ref_pattern",
        r#"
enum Entry {
    Val(i32),
    Empty
}

fn bad(r: &Entry) -> i32 {
    match r {
        &Entry.Val(ref mut x) => {
            *x = 99
            return *x
        },
        &Entry.Empty() => {
            return 0
        }
    }
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 4: Matching `&mut` pattern against a shared `&` reference.
    check_rejected(
        &dir,
        "neg_amp_mut_on_shared_ref",
        r#"
struct Point { x: i32, y: i32 }

fn bad(r: &Point) -> i32 {
    let &mut Point { x: a, y: b } = r
    return a + b
}

fn main() -> i32 {
    return 0
}
"#,
        "E-TYPE",
    );

    // Negative 5: Using a conditionally moved-out field without refilling it (`UseOfMaybeUninit`).
    check_rejected(
        &dir,
        "neg_use_maybe_uninit_field",
        r#"
package Pair {
    manages a: ByteBuffer
    manages b: ByteBuffer
}

fn bad(cond: bool) -> i32 {
    let p = Pair { a: bytes_new(), b: bytes_new() }
    if cond {
        let taken = give p.a
    }
    return i32(bytes_len(&p.a))
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );

    // Negative 6: Moving a field out of a `user_drop` type conditionally.
    check_rejected(
        &dir,
        "neg_partial_move_user_drop",
        r#"
package Wrap {
    manages buf: ByteBuffer
}

resource Guard {
    inner: Wrap
}

impl Drop for Guard {
    fn drop(self: Self) {
        let _ = .inner
    }
}

fn bad(cond: bool) -> i32 {
    let g = Guard { inner: Wrap { buf: bytes_new() } }
    if cond {
        let taken = g.inner
    }
    return 0
}

fn main() -> i32 {
    return 0
}
"#,
        "E-SAFETY",
    );
}
