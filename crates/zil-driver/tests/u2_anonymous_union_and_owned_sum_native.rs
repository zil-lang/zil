//! Dual-backend native execution & safety negative tests for Wave 1 Batch U2:
//! - Anonymous union types `T | U` (canonical flattening/sorting, `A | B == B | A`,
//!   implicit unique-member injection, exhaustive `match` with `ref`/`ref mut`/nested/
//!   module-qualified patterns, owning `ByteBuffer | Store<i64>` & `ResourceA | ResourceB`
//!   selective `Drop` trace, and `SafeReject` safety negatives).
//! - `main()` ErrorUnion error-payload drop fix (`_zil_design.md:573`).
//! - `tests/owned-sum-consumer/project/main.zil` all 8 modes (0..7) x counts (0..3)
//!   verified against `ORACLE.json` + all 6 `frozen-probes/`.

use std::io::Write;
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
    let dir = std::env::temp_dir().join(format!("zil-u2-{name}-{}", std::process::id()));
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

fn build_exe(entry: &Path, exe: &Path, backend: NativeBackend, opt: Optimization, label: &str) {
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
        "build failed for {} ({label}): {:?}",
        entry.display(),
        report.diagnostics
    );
    assert_eq!(report.verification_runs, 1);
}

fn run_exe(exe: &Path, stdin_data: &str) -> (i32, String, String) {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", exe.display()));
    if !stdin_data.is_empty() {
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(stdin_data.as_bytes())
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait output");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn build_and_run_dual(dir: &Path, entry: &Path, stdin_data: &str) -> (i32, String) {
    let mut expected: Option<(i32, String)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        build_exe(entry, &exe, backend, opt, label);
        let (code, stdout, stderr) = run_exe(&exe, stdin_data);
        assert!(
            stderr.is_empty(),
            "unexpected stderr for {} ({label}): {stderr}",
            entry.display()
        );
        if let Some((prev_code, ref prev_out)) = expected {
            assert_eq!(code, prev_code, "exit mismatch on {label}");
            assert_eq!(&stdout, prev_out, "stdout mismatch on {label}");
        } else {
            expected = Some((code, stdout));
        }
    }
    expected.expect("at least one backend ran")
}

fn assert_safe_reject(name: &str, src: &str) {
    let dir = temp_dir(name);
    let entry = dir.join("main.zil");
    std::fs::write(&entry, src).expect("write negative source");
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected SafeReject for {name}, got {:?}: {:?}",
        report.status,
        report.diagnostics
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn u2_anonymous_union_copy_canonical_and_match_native() {
    let dir = temp_dir("union_copy");
    std::fs::write(
        dir.join("geom.zil"),
        r#"
pub struct Point {
    pub x: i32,
    pub y: i32,
}
"#,
    )
    .expect("write geom.zil");

    let entry = dir.join("main.zil");
    std::fs::write(
        &entry,
        r#"
import geom

fn make_ib(flag: bool) -> i32 | bool {
    if flag {
        return 42
    }
    return true
}

fn consume_bi(v: bool | i32) -> i32 {
    match v {
        i32(n) => { return n }
        bool(b) => {
            if b { return 7 }
            return 0
        }
    }
}

fn flatten_test(v: (i32 | bool) | i64) -> i64 {
    let w: i64 | (bool | i32) = v
    match w {
        i32(n) => { return i64(n) + 100 }
        bool(b) => {
            if b { return 200 }
            return 201
        }
        i64(k) => { return k + 300 }
    }
}

fn match_nested_and_ref(v: geom.Point | i32) -> i32 {
    var local: i32 | geom.Point = v
    match local {
        geom.Point(ref mut p) => {
            p.x = p.x + 1
        }
        _ => {}
    }
    match local {
        geom.Point(geom.Point { x: px, y: py }) => { return px * 10 + py }
        i32(n) => { return n }
    }
}

fn main() -> i32 {
    let a: i32 | bool = make_ib(true)
    let b: bool | i32 = make_ib(false)
    if consume_bi(a) != 42 { return 1 }
    if consume_bi(b) != 7 { return 2 }

    let f1: (i32 | bool) | i64 = 5i32
    let f2: i64 | (bool | i32) = false
    let f3: i32 | (i64 | bool) = 12i64
    if flatten_test(f1) != 105 { return 3 }
    if flatten_test(f2) != 201 { return 4 }
    if flatten_test(f3) != 312 { return 5 }

    let p = geom.Point { x: 3, y: 4 }
    if match_nested_and_ref(p) != 44 { return 6 }
    if match_nested_and_ref(99) != 99 { return 7 }

    return 0
}
"#,
    )
    .expect("write main.zil");

    let (code, stdout) = build_and_run_dual(&dir, &entry, "");
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn u2_anonymous_union_owning_drop_trace_native() {
    let dir = temp_dir("union_owning");
    let entry = dir.join("main.zil");
    std::fs::write(
        &entry,
        r#"
extern "C" fn putchar(code: i32) -> i32;

fn emit(code: i32) {
    unsafe { putchar(code) }
}

resource ResA {
    code: i32,
}
impl Drop for ResA {
    fn drop(self: Self) {
        emit(.code)
    }
}

resource ResB {
    code: i32,
}
impl Drop for ResB {
    fn drop(self: Self) {
        emit(.code)
    }
}

fn make_res(pick_a: bool, c: i32) -> Managed<ResA> | Managed<ResB> {
    if pick_a {
        return ResA { code: c }
    }
    return ResB { code: c }
}

fn make_buf_or_store(pick_buf: bool) -> ByteBuffer | Store<i64>
    contract { result(give) }
{
    if pick_buf {
        let b = bytes_new()
        let _ = bytes_reserve(b, 2)
        bytes_push(b, 65u8)
        bytes_push(b, 66u8)
        return give b
    }
    let s: Store<i64> = store_new()
    let _ = store_reserve(s, 2)
    store_push(s, 10i64)
    store_push(s, 32i64)
    return give s
}

fn inspect_buf_or_store(u: Managed<ByteBuffer> | Managed<Store<i64>>) -> i64 {
    match u {
        ByteBuffer(b) => { return bytes_len(b) }
        Store(s) => { return store_len(s) * 10 }
    }
}

fn scope_exit_a() {
    let u: Managed<ResA> | Managed<ResB> = make_res(true, 65)
}

fn scope_exit_b() {
    let u: Managed<ResB> | Managed<ResA> = make_res(false, 66)
}

fn scope_reassign() {
    var u: Managed<ResA> | Managed<ResB> = make_res(true, 67)
    u = make_res(false, 68)
}

fn scope_match_move() {
    let u: Managed<ResA> | Managed<ResB> = make_res(true, 69)
    match u {
        ResA(a) => {
            emit(109)
        }
        ResB(b) => {
            emit(120)
        }
    }
}

fn main() -> i32 {
    scope_exit_a()
    scope_exit_b()
    scope_reassign()
    scope_match_move()
    if inspect_buf_or_store(make_buf_or_store(true)) != 2 { return 1 }
    if inspect_buf_or_store(make_buf_or_store(false)) != 20 { return 2 }
    return 0
}
"#,
    )
    .expect("write main.zil");

    let (code, stdout) = build_and_run_dual(&dir, &entry, "");
    assert_eq!(code, 0);
    assert_eq!(stdout, "ABCDmE");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn u2_anonymous_union_safety_negatives_safe_reject() {
    // 1. Direct duplicate member `i32 | i32`.
    assert_safe_reject(
        "neg_dup_direct",
        r#"
fn f(x: i32 | i32) -> i32 { return 0 }
fn main() -> i32 { return 0 }
"#,
    );

    // 2. Duplicate member via nested flattening `(i32 | bool) | i32`.
    assert_safe_reject(
        "neg_dup_nested",
        r#"
fn f(x: (i32 | bool) | i32) -> i32 { return 0 }
fn main() -> i32 { return 0 }
"#,
    );

    // 3. Non-exhaustive match on `i32 | bool`.
    assert_safe_reject(
        "neg_nonexhaustive",
        r#"
fn f(x: i32 | bool) -> i32 {
    match x {
        i32(n) => { return n }
    }
}
fn main() -> i32 { return f(1) }
"#,
    );

    // 4. Double-consume / use-after-move of owning `Managed<ResA> | Managed<ResB>`.
    assert_safe_reject(
        "neg_double_consume",
        r#"
resource ResA { code: i32 }
impl Drop for ResA { fn drop(self: Self) {} }
resource ResB { code: i32 }
impl Drop for ResB { fn drop(self: Self) {} }

fn eat(u: Managed<ResA> | Managed<ResB>) {
    match u {
        ResA(a) => {}
        ResB(b) => {}
    }
}

fn main() -> i32 {
    let u: Managed<ResA> | Managed<ResB> = ResA { code: 1 }
    eat(u)
    eat(u)
    return 0
}
"#,
    );

    // 5. Invalid coercion (`f64` into `i32 | bool`).
    assert_safe_reject(
        "neg_invalid_coercion",
        r#"
fn main() -> i32 {
    let x: i32 | bool = 3.14f64
    return 0
}
"#,
    );

    // 6. Ambiguous unsuffixed integer literal coercion into `i32 | i64`.
    assert_safe_reject(
        "neg_ambiguous_int",
        r#"
fn main() -> i32 {
    let x: i32 | i64 = 42
    return 0
}
"#,
    );
}

/// Parses `(input, normal.exit, normal.stdout)` entries from `ORACLE.json`
/// without external JSON dependencies.
fn parse_oracle_normal_cases(oracle_text: &str) -> Vec<(String, i32, String)> {
    let mut out = Vec::new();
    let mut rest = oracle_text;
    while let Some(pos) = rest.find("\"input\": \"") {
        rest = &rest[pos + "\"input\": \"".len()..];
        let end_in = rest.find('"').expect("closing quote for input");
        let input = rest[..end_in].to_string();
        rest = &rest[end_in + 1..];

        let norm_pos = rest.find("\"normal\":").expect("normal block");
        rest = &rest[norm_pos..];

        let exit_pos = rest.find("\"exit\": ").expect("exit field");
        rest = &rest[exit_pos + "\"exit\": ".len()..];
        let exit_end = rest.find(',').expect("comma after exit");
        let exit_code: i32 = rest[..exit_end].trim().parse().expect("parse exit code");
        rest = &rest[exit_end + 1..];

        let out_pos = rest.find("\"stdout\": \"").expect("stdout field");
        rest = &rest[out_pos + "\"stdout\": \"".len()..];
        let out_end = rest.find('"').expect("closing quote for stdout");
        let stdout = rest[..out_end].to_string();
        rest = &rest[out_end + 1..];

        out.push((input, exit_code, stdout));
    }
    out
}

#[test]
fn u2_owned_sum_consumer_all_8_modes_oracle_native() {
    let root = repo_root();
    let project_main = root.join("tests/owned-sum-consumer/project/main.zil");
    let oracle_path = root.join("tests/owned-sum-consumer/ORACLE.json");
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("read ORACLE.json");
    let cases = parse_oracle_normal_cases(&oracle_text);
    assert_eq!(
        cases.len(),
        32,
        "expected 32 (count 0..3 x mode 0..7) cases in ORACLE.json"
    );

    let dir = temp_dir("owned_sum_oracle");
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("owned-sum-{label}"));
        build_exe(&project_main, &exe, backend, opt, label);
        for (input, exp_exit, exp_stdout) in &cases {
            let (code, stdout, stderr) = run_exe(&exe, input);
            assert!(
                stderr.is_empty(),
                "owned-sum-consumer ({label}) input `{input}` unexpected stderr: {stderr}"
            );
            assert_eq!(
                code, *exp_exit,
                "owned-sum-consumer ({label}) input `{input}` exit mismatch (stdout=`{stdout}`)"
            );
            assert_eq!(
                stdout, *exp_stdout,
                "owned-sum-consumer ({label}) input `{input}` stdout mismatch"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn u2_owned_sum_consumer_frozen_probes_native() {
    let root = repo_root();
    let probes_dir = root.join("tests/owned-sum-consumer/frozen-probes");

    type ProbeRun<'a> = (&'a str, i32, &'a str);
    let positive_probes: &[(&str, &[ProbeRun<'_>])] = &[
        ("unit-flat", &[("0", 0, "SRA"), ("1", 1, "SA")]),
        (
            "unit-nested-and-owned-error-unit",
            &[("0", 0, "SRA"), ("1", 1, "SA"), ("2", 0, "SER")],
        ),
        ("unit-store-session", &[("0", 0, "SRA"), ("1", 1, "SA")]),
        ("both-owned-flat", &[("0", 0, "SAR"), ("1", 0, "SER")]),
        ("owned-option-session", &[("0", 0, "SRA"), ("1", 0, "SR")]),
    ];

    let dir = temp_dir("owned_sum_probes");
    for (probe_name, runs) in positive_probes {
        let entry = probes_dir.join(probe_name).join("main.zil");
        for (label, backend, opt) in dual_configs() {
            let exe = dir.join(format!("{probe_name}-{label}"));
            build_exe(&entry, &exe, backend, opt, label);
            for (input, exp_exit, exp_stdout) in *runs {
                let (code, stdout, stderr) = run_exe(&exe, input);
                assert!(
                    stderr.is_empty(),
                    "probe {probe_name} ({label}) input `{input}` unexpected stderr: {stderr}"
                );
                assert_eq!(
                    code, *exp_exit,
                    "probe {probe_name} ({label}) input `{input}` exit mismatch"
                );
                assert_eq!(
                    stdout, *exp_stdout,
                    "probe {probe_name} ({label}) input `{input}` stdout mismatch"
                );
            }
        }
    }

    // 6th frozen probe: `negative-double-consume` must be rejected with SafeReject.
    let neg_entry = probes_dir.join("negative-double-consume/main.zil");
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&neg_entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected SafeReject for negative-double-consume, got {:?}: {:?}",
        report.status,
        report.diagnostics
    );

    let _ = std::fs::remove_dir_all(&dir);
}
