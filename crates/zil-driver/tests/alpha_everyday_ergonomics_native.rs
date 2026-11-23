//! Dual-backend native acceptance tests for everyday Alpha safety and ergonomics:
//! - Runtime bounds checks on array/slice indexing and sub-slicing
//! - `Option<T>` `?` propagation and `.is_some()`/`.is_none()`/`.unwrap()`/`.unwrap_or()`
//! - `ErrorUnion` (`E!T`) `.is_ok()`/`.is_err()`/`.unwrap()`/`.unwrap_or()`
//! - `match` guards with fallthrough and exhaustiveness rejection
//! - Record pattern field shorthand (`Point { x, y }`) and trailing rest (`Point { x, .. }`)
//! - Inclusive ranges (`for i in a..=b`) including upper-bound max without overflow

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zil-alpha-ergonomics-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn backend_configs() -> Vec<(&'static str, NativeBackend, Optimization)> {
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

fn compile_and_run_status(
    dir: &Path,
    label: &str,
    src: &str,
    backend: NativeBackend,
    opt: Optimization,
) -> ExitStatus {
    let entry = dir.join(format!("{label}.zil"));
    let exe = dir.join(format!("{label}.bin"));
    std::fs::write(&entry, src).expect("write source");

    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Build,
        optimization: opt,
        backend,
        output: Some(exe.clone()),
        codegen: CodegenOptions { units: 1, jobs: 1 },
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "build failed for {label} ({backend:?}, {opt:?}): {:?}",
        report.diagnostics
    );

    Command::new(&exe)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()))
}

fn assert_traps(status: ExitStatus, label: &str) {
    assert!(
        !status.success(),
        "{label}: expected runtime abort/trap, but exited cleanly with {status:?}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert!(
            status.signal().is_some() || status.code() != Some(0),
            "{label}: expected signal or non-zero exit, got {status:?}"
        );
    }
}

#[test]
fn option_and_error_union_everyday_methods_and_try_dual_backend() {
    let dir = temp_dir("opt-err-methods");
    let src = r#"
enum MathErr {
    DivZero,
}

fn checked_div_opt(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        return Option.None()
    }
    return Option.Some(a / b)
}

fn chain_opt(a: i64, b: i64, c: i64) -> Option<i64> {
    let x = checked_div_opt(a, b)?
    let y = checked_div_opt(x, c)?
    return Option.Some(y + 1)
}

fn checked_div_err(a: i64, b: i64) -> MathErr!i64 {
    if b == 0 {
        return MathErr.DivZero()
    }
    return a / b
}

fn main() -> i32 {
    let some_v = chain_opt(100, 5, 2)
    let none_v = chain_opt(100, 0, 2)
    if !some_v.is_some() { return 1 }
    if some_v.is_none() { return 2 }
    if none_v.is_some() { return 3 }
    if !none_v.is_none() { return 4 }

    let a = some_v.unwrap()
    if a != 11 { return 5 }
    if some_v.unwrap_or(99) != 11 { return 6 }
    if none_v.unwrap_or(42) != 42 { return 7 }

    let ok_r = checked_div_err(84, 2)
    let err_r = checked_div_err(84, 0)
    if !ok_r.is_ok() { return 8 }
    if ok_r.is_err() { return 9 }
    if err_r.is_ok() { return 10 }
    if !err_r.is_err() { return 11 }

    if ok_r.unwrap() != 42 { return 12 }
    if ok_r.unwrap_or(0) != 42 { return 13 }
    if err_r.unwrap_or(77) != 77 { return 14 }

    return 0
}
"#;

    for (label, backend, opt) in backend_configs() {
        let status = compile_and_run_status(&dir, label, src, backend, opt);
        assert_eq!(
            status.code(),
            Some(0),
            "{label}: unexpected status {status:?}"
        );
    }
}

#[test]
fn option_and_error_union_unwrap_traps_on_none_and_err_dual_backend() {
    let dir = temp_dir("unwrap-traps");
    let opt_none_src = r#"
fn mk_none() -> Option<i64> {
    return Option.None()
}

fn main() -> i32 {
    let x = mk_none().unwrap()
    return i32(x)
}
"#;
    let err_fail_src = r#"
enum E { Fail }

fn mk_err() -> E!i64 {
    return E.Fail()
}

fn main() -> i32 {
    let x = mk_err().unwrap()
    return i32(x)
}
"#;

    for (label, backend, opt) in backend_configs() {
        let s1 = compile_and_run_status(
            &dir,
            &format!("opt-none-{label}"),
            opt_none_src,
            backend,
            opt,
        );
        assert_traps(s1, &format!("opt-none-{label}"));

        let s2 = compile_and_run_status(
            &dir,
            &format!("err-fail-{label}"),
            err_fail_src,
            backend,
            opt,
        );
        assert_traps(s2, &format!("err-fail-{label}"));
    }
}

#[test]
fn match_guards_fallthrough_dual_backend_and_exhaustiveness_rejection() {
    let dir = temp_dir("match-guards");
    let ok_src = r#"
fn classify(opt: Option<i64>) -> i64 {
    match opt {
        Option.Some(v) if v < 0 => { return 1 }
        Option.Some(v) if v == 0 => { return 2 }
        Option.Some(v) if v > 10 => { return 3 }
        Option.Some(v) => { return v + 100 }
        Option.None() => { return 0 }
    }
}

fn classify_int(n: i64) -> i64 {
    match n {
        x if x % 2 == 0 && x > 0 => { return 10 }
        x if x > 0 => { return 20 }
        _ => { return 30 }
    }
}

fn main() -> i32 {
    if classify(Option.Some(-5)) != 1 { return 1 }
    if classify(Option.Some(0)) != 2 { return 2 }
    if classify(Option.Some(42)) != 3 { return 3 }
    if classify(Option.Some(7)) != 107 { return 4 }
    if classify(Option.None()) != 0 { return 5 }

    if classify_int(4) != 10 { return 6 }
    if classify_int(5) != 20 { return 7 }
    if classify_int(-2) != 30 { return 8 }
    return 0
}
"#;

    for (label, backend, opt) in backend_configs() {
        let status = compile_and_run_status(&dir, label, ok_src, backend, opt);
        assert_eq!(
            status.code(),
            Some(0),
            "{label}: unexpected status {status:?}"
        );
    }

    // Negative exhaustiveness test: guarded catch-all alone is NOT exhaustive.
    let non_exhaustive_src = r#"
fn bad(n: i64) -> i64 {
    match n {
        x if x > 0 => { return x }
    }
}

fn main() -> i32 {
    return i32(bad(1))
}
"#;
    let bad_entry = dir.join("non_exhaustive_guard.zil");
    std::fs::write(&bad_entry, non_exhaustive_src).expect("write bad source");
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Types,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&bad_entry, &opts);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "guarded catch-all alone must be rejected as non-exhaustive"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("cover every value") || d.code.contains("EXHAUST")),
        "expected non-exhaustive diagnostic, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn record_pattern_shorthand_and_rest_dual_backend() {
    let dir = temp_dir("record-pat-shorthand");
    let src = r#"
struct Point {
    x: i64,
    y: i64,
    z: i64,
}

fn sum_xy(p: Point) -> i64 {
    let Point { x, y, .. } = p
    return x + y
}

fn match_point(p: Point) -> i64 {
    match p {
        Point { x, y, z } if x == y => { return x + z }
        Point { x, .. } => { return x * 10 }
    }
}

fn main() -> i32 {
    let p1 = Point { x: 3, y: 4, z: 5 }
    let Point { x, y, z } = p1
    if x + y + z != 12 { return 1 }
    if sum_xy(p1) != 7 { return 2 }

    let p2 = Point { x: 6, y: 6, z: 9 }
    if match_point(p2) != 15 { return 3 }
    if match_point(p1) != 30 { return 4 }

    return 0
}
"#;

    for (label, backend, opt) in backend_configs() {
        let status = compile_and_run_status(&dir, label, src, backend, opt);
        assert_eq!(
            status.code(),
            Some(0),
            "{label}: unexpected status {status:?}"
        );
    }
}

#[test]
fn inclusive_range_for_loop_dual_backend() {
    let dir = temp_dir("inclusive-range");
    let src = r#"
fn main() -> i32 {
    var sum: i64 = 0
    for i in 1..=5 {
        sum += i
    }
    if sum != 15 { return 1 }

    var single: i64 = 0
    for i in 7..=7 {
        single += i
    }
    if single != 7 { return 2 }

    var empty: i64 = 0
    for i in 6..=5 {
        empty += i
    }
    if empty != 0 { return 3 }

    // Upper-bound inclusive loop reaching u8::MAX (255) without overflowing.
    var count: i64 = 0
    var total_u8: i64 = 0
    let lo: u8 = 253
    let hi: u8 = 255
    for b in lo..=hi {
        count += 1
        total_u8 += i64(b)
    }
    if count != 3 { return 4 }
    if total_u8 != 762 { return 5 }

    return 0
}
"#;

    for (label, backend, opt) in backend_configs() {
        let status = compile_and_run_status(&dir, label, src, backend, opt);
        assert_eq!(
            status.code(),
            Some(0),
            "{label}: unexpected status {status:?}"
        );
    }
}

#[test]
fn array_and_slice_bounds_checks_dual_backend() {
    let dir = temp_dir("bounds-checks");

    let in_bounds_src = r#"
fn sum_slice(s: &[i64]) -> i64 {
    var acc: i64 = 0
    for i in 0..s.len() {
        acc += s[i]
    }
    return acc
}

fn main() -> i32 {
    let arr: [i64; 4] = [10, 20, 30, 40]
    let i: i32 = 2
    if arr[0] != 10 { return 1 }
    if arr[i] != 30 { return 2 }

    let s = &arr[1..4]
    if s.len() != 3 { return 3 }
    if sum_slice(s) != 90 { return 4 }

    let sub = &s[1..3]
    if sub.len() != 2 { return 5 }
    if sub[0] != 30 || sub[1] != 40 { return 6 }

    let empty_end = &s[3..3]
    if empty_end.len() != 0 { return 7 }

    return 0
}
"#;

    let oob_array_src = r#"
fn read_at(idx: i64) -> i64 {
    let arr: [i64; 4] = [10, 20, 30, 40]
    return arr[idx]
}

fn main() -> i32 {
    let v = read_at(4)
    return i32(v)
}
"#;

    let oob_slice_index_src = r#"
fn read_slice(s: &[i64], idx: i64) -> i64 {
    return s[idx]
}

fn main() -> i32 {
    let arr: [i64; 4] = [10, 20, 30, 40]
    let s = &arr[1..4]
    let v = read_slice(s, 3)
    return i32(v)
}
"#;

    let oob_subslice_start_gt_end_src = r#"
fn bad_slice(a: i64, b: i64) -> i64 {
    let arr: [i64; 4] = [10, 20, 30, 40]
    let s = &arr[a..b]
    return s.len()
}

fn main() -> i32 {
    let n = bad_slice(3, 1)
    return i32(n)
}
"#;

    let oob_subslice_end_gt_len_src = r#"
fn bad_slice(a: i64, b: i64) -> i64 {
    let arr: [i64; 4] = [10, 20, 30, 40]
    let s = &arr[a..b]
    return s.len()
}

fn main() -> i32 {
    let n = bad_slice(0, 5)
    return i32(n)
}
"#;

    for (label, backend, opt) in backend_configs() {
        let ok = compile_and_run_status(
            &dir,
            &format!("in-bounds-{label}"),
            in_bounds_src,
            backend,
            opt,
        );
        assert_eq!(
            ok.code(),
            Some(0),
            "in-bounds-{label}: unexpected status {ok:?}"
        );

        let s_arr = compile_and_run_status(
            &dir,
            &format!("oob-arr-{label}"),
            oob_array_src,
            backend,
            opt,
        );
        assert_traps(s_arr, &format!("oob-arr-{label}"));

        let s_sl = compile_and_run_status(
            &dir,
            &format!("oob-slice-{label}"),
            oob_slice_index_src,
            backend,
            opt,
        );
        assert_traps(s_sl, &format!("oob-slice-{label}"));

        let s_rev = compile_and_run_status(
            &dir,
            &format!("oob-rev-{label}"),
            oob_subslice_start_gt_end_src,
            backend,
            opt,
        );
        assert_traps(s_rev, &format!("oob-rev-{label}"));

        let s_over = compile_and_run_status(
            &dir,
            &format!("oob-over-{label}"),
            oob_subslice_end_gt_len_src,
            backend,
            opt,
        );
        assert_traps(s_over, &format!("oob-over-{label}"));
    }
}
