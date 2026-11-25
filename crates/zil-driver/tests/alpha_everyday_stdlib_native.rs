//! Dual-backend (`clang-o0`, `clang-o2`, `llvmapi-o0`, `llvmapi-o2`) native
//! execution tests for everyday standard library modules:
//! - `std.io` (`print`, `println`, `eprint`, `eprintln`, `print_i64`, `println_i64`, `stderr`)
//! - `std.fs` (`write_bytes`, `exists`, `read_to_bytes`, `remove_file`)
//! - `std.env` (`get`, `arg_count`, `arg`, `args`, `current_dir`, `exit`)
//! - `std.time` (`monotonic_ns`, `sleep_ms`)
//! - `std.string` (`starts_with`, `ends_with`, `contains`, `trim*`, `split`, `lines`, `replace_all`)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zil-alpha-everyday-stdlib-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
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

fn build_and_run_dual_with_args_and_units(
    dir: &Path,
    tag: &str,
    entry: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    units: usize,
) -> (i32, Vec<u8>, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>, Vec<u8>)> = None;
    let jobs = if units > 1 { 2 } else { 1 };
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{tag}-u{units}-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            codegen: CodegenOptions { units, jobs },
            ..CompileOptions::default()
        };
        let report = session.compile_path(entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}, units={units}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let mut cmd = Command::new(&exe);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));

        let code = out.status.code().unwrap_or(-1);
        if let Some((prev_code, ref prev_stdout, ref prev_stderr)) = expected {
            assert_eq!(
                code,
                prev_code,
                "exit code mismatch for {} on {label} (units={units})",
                entry.display()
            );
            assert_eq!(
                out.stdout,
                *prev_stdout,
                "stdout mismatch for {} on {label} (units={units})",
                entry.display()
            );
            assert_eq!(
                out.stderr,
                *prev_stderr,
                "stderr mismatch for {} on {label} (units={units})",
                entry.display()
            );
        } else {
            expected = Some((code, out.stdout, out.stderr));
        }
    }
    expected.expect("at least one backend ran")
}

fn build_and_run_dual(
    dir: &Path,
    tag: &str,
    entry: &Path,
    envs: &[(&str, &str)],
) -> (i32, Vec<u8>, Vec<u8>) {
    build_and_run_dual_with_args_and_units(dir, tag, entry, &[], envs, 1)
}

#[test]
fn std_io_print_and_stderr_execute_natively_on_dual_backends() {
    let dir = temp_dir("io-print");
    let entry = write_source(
        &dir,
        "main.zil",
        r#"
import std.io

fn main() -> io.Error!i32 {
    io.print("hello ")?
    io.println("world")?
    io.print_i64(-42)?
    io.print(",")?
    io.println_i64(9001)?
    io.eprint("err:")?
    io.eprintln("line")?
    let err_file = io.stderr()?
    io.write_all(&err_file, "via-stderr\n")?
    return 0
}
"#,
    );

    let (code, stdout, stderr) = build_and_run_dual(&dir, "io-print", &entry, &[]);
    assert_eq!(code, 0);
    assert_eq!(stdout, b"hello world\n-42,9001\n");
    assert_eq!(stderr, b"err:line\nvia-stderr\n");
}

#[test]
fn std_fs_roundtrip_and_missing_file_execute_natively_on_dual_backends() {
    let dir = temp_dir("fs-roundtrip");
    let target_file = dir.join("sample.bin");
    let missing_file = dir.join("does-not-exist.bin");
    let target_str = target_file.to_str().expect("utf8 path");
    let missing_str = missing_file.to_str().expect("utf8 path");

    let src = format!(
        r#"
import std.fs
import std.io

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {{
    if a.len() != b.len() {{ return false }}
    var i: i64 = 0
    while i < a.len() {{
        if a[i] != b[i] {{ return false }}
        i += 1
    }}
    return true
}}

fn main() -> io.Error!i32 {{
    let path = "{target_str}"
    let missing = "{missing_str}"
    if fs.exists(path) {{ return 1 }}
    if fs.exists("bad\0path") {{ return 2 }}

    let payload = "alpha-stdlib-payload\n\0binary-tail-1234567890"
    fs.write_bytes(path, payload)?
    if !fs.exists(path) {{ return 3 }}

    let read_back = fs.read_to_bytes(path)?
    if !bytes_eq(bytes_view(read_back), payload) {{ return 4 }}

    match fs.remove_file(path) {{
        .ok(_) => {{}},
        .err(_) => {{ return 5 }}
    }}
    if fs.exists(path) {{ return 6 }}

    match fs.remove_file(missing) {{
        .ok(_) => {{ return 7 }},
        .err(e) => {{
            match e {{
                fs.RemoveError.Remove() => {{}},
                _ => {{ return 8 }}
            }}
        }}
    }}

    match fs.remove_file("bad\0path") {{
        .ok(_) => {{ return 9 }},
        .err(e) => {{
            match e {{
                fs.RemoveError.InvalidPath() => {{}},
                _ => {{ return 10 }}
            }}
        }}
    }}

    match fs.remove_file("") {{
        .ok(_) => {{ return 11 }},
        .err(e) => {{
            match e {{
                fs.RemoveError.InvalidPath() => {{}},
                _ => {{ return 12 }}
            }}
        }}
    }}

    match fs.read_to_bytes(missing) {{
        .ok(_) => {{ return 13 }},
        .err(_) => {{}}
    }}
    return 0
}}
"#
    );
    let entry = write_source(&dir, "main.zil", &src);

    let (code, stdout, stderr) = build_and_run_dual(&dir, "fs-roundtrip", &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
    assert!(!target_file.exists());
}

#[test]
fn std_env_get_current_dir_and_exit_execute_natively_on_dual_backends() {
    let dir = temp_dir("env-ops");
    let entry = write_source(
        &dir,
        "main.zil",
        r#"
import std.env
import std.io

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

fn main() -> io.Error!i32 {
    match env.get("ZIL_TEST_ENV_KEY") {
        Option.None() => { return 1 },
        Option.Some(val) => {
            if !bytes_eq(bytes_view(val), "hello_zil") { return 2 }
        }
    }
    match env.get("ZIL_TEST_ENV_DEFINITELY_UNSET_99999") {
        Option.None() => {},
        Option.Some(_) => { return 3 }
    }
    match env.get("BAD\0NAME") {
        Option.None() => {},
        Option.Some(_) => { return 4 }
    }
    match env.get("") {
        Option.None() => {},
        Option.Some(_) => { return 6 }
    }
    let cwd = env.current_dir()?
    if bytes_len(cwd) <= 0 { return 5 }
    io.print(bytes_view(cwd))?
    return 0
}
"#,
    );

    let (code, stdout, stderr) = build_and_run_dual(
        &dir,
        "env-ops",
        &entry,
        &[("ZIL_TEST_ENV_KEY", "hello_zil")],
    );
    assert_eq!(code, 0);
    assert!(!stdout.is_empty(), "current_dir() must be non-empty");
    assert!(stderr.is_empty());

    let exit_entry = write_source(
        &dir,
        "exit_main.zil",
        r#"
import std.env

fn main() -> i32 {
    env.exit(42)
    return 0
}
"#,
    );
    let (exit_code, exit_out, exit_err) = build_and_run_dual(&dir, "env-exit", &exit_entry, &[]);
    assert_eq!(exit_code, 42);
    assert!(exit_out.is_empty());
    assert!(exit_err.is_empty());
}

#[test]
fn std_time_monotonic_ns_and_sleep_ms_execute_natively_on_dual_backends() {
    let dir = temp_dir("time-ops");
    let entry = write_source(
        &dir,
        "main.zil",
        r#"
import std.time

fn main() -> i32 {
    let t0 = time.monotonic_ns()
    time.sleep_ms(5u64)
    let t1 = time.monotonic_ns()
    if t1 < t0 { return 1 }
    if t1 == 0u64 { return 2 }
    return 0
}
"#,
    );

    let (code, stdout, stderr) = build_and_run_dual(&dir, "time-ops", &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn std_io_eintr_retry_executes_natively_on_dual_backends() {
    let dir = temp_dir("io-eintr");
    let data_file = dir.join("eintr.bin");
    let data_str = data_file.to_str().expect("utf8 path");

    let probe_c = write_source(
        &dir,
        "eintr_probe.c",
        r#"
#define _GNU_SOURCE
#include <errno.h>
#include <stddef.h>
#include <sys/syscall.h>
#include <unistd.h>

static int g_write_eintr_left = 0;
static int g_write_short_once = 0;
static int g_write_eintr_after_short = 0;
static int g_write_eio_once = 0;
static int g_write_eintr_count = 0;

static int g_read_eintr_left = 0;
static int g_read_eio_once = 0;
static int g_read_eintr_count = 0;

void eintr_arm_write(int eintr_before, int short_once, int eintr_after) {
    g_write_eintr_left = eintr_before;
    g_write_short_once = short_once;
    g_write_eintr_after_short = eintr_after;
    g_write_eio_once = 0;
}

void eintr_arm_write_eio(void) {
    g_write_eintr_left = 0;
    g_write_short_once = 0;
    g_write_eintr_after_short = 0;
    g_write_eio_once = 1;
}

void eintr_arm_read(int eintr_before) {
    g_read_eintr_left = eintr_before;
    g_read_eio_once = 0;
}

void eintr_arm_read_eio(void) {
    g_read_eintr_left = 0;
    g_read_eio_once = 1;
}

int eintr_write_hits(void) { return g_write_eintr_count; }
int eintr_read_hits(void) { return g_read_eintr_count; }

ssize_t write(int fd, const void *buf, size_t count) {
    if (fd >= 3) {
        if (g_write_eio_once) {
            g_write_eio_once = 0;
            errno = EIO;
            return -1;
        }
        if (g_write_eintr_left > 0) {
            g_write_eintr_left--;
            g_write_eintr_count++;
            errno = EINTR;
            return -1;
        }
        if (g_write_short_once && count > 3) {
            g_write_short_once = 0;
            if (g_write_eintr_after_short > 0) {
                g_write_eintr_left = g_write_eintr_after_short;
                g_write_eintr_after_short = 0;
            }
            return (ssize_t)syscall(SYS_write, fd, buf, (size_t)3);
        }
    }
    return (ssize_t)syscall(SYS_write, fd, buf, count);
}

ssize_t read(int fd, void *buf, size_t count) {
    if (fd >= 3) {
        if (g_read_eio_once) {
            g_read_eio_once = 0;
            errno = EIO;
            return -1;
        }
        if (g_read_eintr_left > 0) {
            g_read_eintr_left--;
            g_read_eintr_count++;
            errno = EINTR;
            return -1;
        }
    }
    return (ssize_t)syscall(SYS_read, fd, buf, count);
}
"#,
    );

    let src = format!(
        r#"
import std.io

extern "C" fn eintr_arm_write(eintr_before: i32, short_once: i32, eintr_after: i32);
extern "C" fn eintr_arm_write_eio();
extern "C" fn eintr_arm_read(eintr_before: i32);
extern "C" fn eintr_arm_read_eio();
extern "C" fn eintr_write_hits() -> i32;
extern "C" fn eintr_read_hits() -> i32;

fn write_phase(path: &[u8]) -> io.Error!i32 {{
    let payload = "hello-eintr-retry"
    let wf = io.create_write(path, 420)?
    // 2 EINTR before short write of 3 bytes, then 2 EINTR before finishing.
    unsafe {{ eintr_arm_write(2, 1, 2) }}
    io.write_all(&wf, payload)?

    // 2 EINTR before write_chunk succeeds.
    var tail = io.empty_buffer()
    tail[0] = 33u8 // '!'
    tail[1] = 84u8 // 'T'
    tail[2] = 65u8 // 'A'
    tail[3] = 73u8 // 'I'
    tail[4] = 76u8 // 'L'
    unsafe {{ eintr_arm_write(2, 0, 0) }}
    let wrote = io.write_chunk(&wf, &tail, 5)?
    if wrote != 5 {{ return 1 }}

    // Non-EINTR error (EIO) must return Error.Write instead of retrying.
    unsafe {{ eintr_arm_write_eio() }}
    match io.write_chunk(&wf, &tail, 1) {{
        .ok(_) => {{ return 2 }},
        .err(e) => {{
            match e {{
                io.Error.Write() => {{}},
                _ => {{ return 3 }}
            }}
        }}
    }}
    return 0
}}

fn read_phase(path: &[u8]) -> io.Error!i32 {{
    let expected = "hello-eintr-retry!TAIL"
    let rf = io.open_read(path)?
    var rbuf = io.empty_buffer()
    unsafe {{ eintr_arm_read(2) }}
    let res = io.read_chunk(&rf, &mut rbuf)?
    if res.eof {{ return 4 }}
    if res.count != expected.len() {{ return 5 }}
    var i: i64 = 0
    while i < expected.len() {{
        if rbuf[i] != expected[i] {{ return 6 }}
        i += 1
    }}

    // Non-EINTR error (EIO) must return Error.Read instead of retrying.
    unsafe {{ eintr_arm_read_eio() }}
    match io.read_chunk(&rf, &mut rbuf) {{
        .ok(_) => {{ return 7 }},
        .err(e) => {{
            match e {{
                io.Error.Read() => {{}},
                _ => {{ return 8 }}
            }}
        }}
    }}
    return 0
}}

fn main() -> io.Error!i32 {{
    let path = "{data_str}"
    let w_status = write_phase(path)?
    if w_status != 0 {{ return w_status }}
    let r_status = read_phase(path)?
    if r_status != 0 {{ return r_status }}

    var w_hits: i32 = 0
    var r_hits: i32 = 0
    unsafe {{
        w_hits = eintr_write_hits()
        r_hits = eintr_read_hits()
    }}
    if w_hits != 6 {{ return 9 }}
    if r_hits != 2 {{ return 10 }}
    return 0
}}
"#
    );
    let entry = write_source(&dir, "main.zil", &src);

    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-eintr-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            extra_c_sources: vec![probe_c.clone()],
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(&entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let out = Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert_eq!(
            out.status.code(),
            Some(0),
            "EINTR retry test failed on {label}: status={:?}, stderr={}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert!(out.stderr.is_empty());
    }
}

#[test]
fn std_env_cli_args_execute_natively_on_dual_backends_and_multi_units() {
    let dir = temp_dir("env-cli-args");
    let entry = write_source(
        &dir,
        "main.zil",
        r#"
import std.env
import std.io

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

fn check_arg(index: i64, expected: &[u8]) -> bool {
    match env.arg(index) {
        Option.None() => { return false },
        Option.Some(val) => { return bytes_eq(bytes_view(val), expected) }
    }
}

fn main() -> io.Error!i32 {
    let count = env.arg_count()
    if count < 1 { return 1 }

    // argv[0] is always present and non-empty.
    match env.arg(0) {
        Option.None() => { return 2 },
        Option.Some(prog) => {
            if bytes_len(prog) <= 0 { return 3 }
        }
    }

    // Out-of-bounds indices must return Option.None().
    match env.arg(-1) {
        Option.None() => {},
        Option.Some(_) => { return 4 }
    }
    match env.arg(count) {
        Option.None() => {},
        Option.Some(_) => { return 5 }
    }
    match env.arg(99) {
        Option.None() => {},
        Option.Some(_) => { return 6 }
    }
    match env.arg(-9223372036854775807) {
        Option.None() => {},
        Option.Some(_) => { return 7 }
    }

    let all = env.args()?
    if store_len(all) != count { return 8 }

    if count == 1 {
        io.print("NO_EXTRA_ARGS")?
        return 0
    }

    if count != 6 { return 9 }
    if !check_arg(1, "--flag") { return 10 }
    if !check_arg(2, "hello") { return 11 }
    if !check_arg(3, "") { return 12 }
    if !check_arg(4, "world") { return 13 }
    if !check_arg(5, "café") { return 14 }

    // Verify Store<Managed<ByteBuffer>> elements match arg(i) and print them.
    var i: i64 = 1
    while i < store_len(all) {
        if i > 1 { io.print("|")? }
        let item = store_at(all, i)
        io.print(bytes_view(item))?
        i += 1
    }
    return 0
}
"#,
    );

    let cli_args = ["--flag", "hello", "", "world", "café"];
    for units in [1usize, 4, 8] {
        let (code, stdout, stderr) =
            build_and_run_dual_with_args_and_units(&dir, "env-args", &entry, &cli_args, &[], units);
        assert_eq!(code, 0, "env args failed with units={units}");
        assert_eq!(stdout, "--flag|hello||world|café".as_bytes());
        assert!(stderr.is_empty());
    }

    let (code_no_args, stdout_no_args, stderr_no_args) =
        build_and_run_dual_with_args_and_units(&dir, "env-no-args", &entry, &[], &[], 4);
    assert_eq!(code_no_args, 0);
    assert_eq!(stdout_no_args, b"NO_EXTRA_ARGS");
    assert!(stderr_no_args.is_empty());
}

#[test]
fn std_string_everyday_helpers_execute_natively_on_dual_backends() {
    let dir = temp_dir("string-helpers");
    let entry = write_source(
        &dir,
        "main.zil",
        r#"
import std.string

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

fn str_eq(s: &string.String, expected: &[u8]) -> bool {
    return bytes_eq(string.as_bytes(s), expected)
}

fn is_invalid_utf8(e: string.Error) -> bool {
    match e {
        string.Error.InvalidUtf8() => { return true },
        _ => { return false }
    }
}

fn is_capacity(e: string.Error) -> bool {
    match e {
        string.Error.Capacity() => { return true },
        _ => { return false }
    }
}

fn test_predicates() -> string.Error!i32 {
    let s = string.from_utf8("hello café world", 64)?
    let empty_s = string.empty()
    let bad = [255u8, 128u8]

    if !string.starts_with(&s, "hello")? { return 1 }
    if !string.starts_with(&s, "hello café world")? { return 2 }
    if !string.starts_with(&s, "")? { return 3 }
    if string.starts_with(&s, "world")? { return 4 }
    if string.starts_with(&s, "hello café world!")? { return 5 }
    if !string.starts_with(&empty_s, "")? { return 6 }
    if string.starts_with(&empty_s, "a")? { return 7 }

    if !string.ends_with(&s, "world")? { return 8 }
    if !string.ends_with(&s, "café world")? { return 9 }
    if !string.ends_with(&s, "")? { return 10 }
    if string.ends_with(&s, "cafe world")? { return 11 }
    if string.ends_with(&empty_s, "a")? { return 12 }
    if !string.ends_with(&empty_s, "")? { return 13 }

    if !string.contains(&s, "café")? { return 14 }
    if !string.contains(&s, "")? { return 15 }
    if string.contains(&s, "xyz")? { return 16 }
    if !string.contains(&empty_s, "")? { return 17 }
    if string.contains(&empty_s, "a")? { return 18 }

    // Invalid UTF-8 patterns must be rejected even on empty haystacks.
    match string.starts_with(&empty_s, &bad[..]) {
        .ok(_) => { return 19 },
        .err(e) => { if !is_invalid_utf8(e) { return 20 } }
    }
    match string.ends_with(&empty_s, &bad[..]) {
        .ok(_) => { return 21 },
        .err(e) => { if !is_invalid_utf8(e) { return 22 } }
    }
    match string.contains(&empty_s, &bad[..]) {
        .ok(_) => { return 23 },
        .err(e) => { if !is_invalid_utf8(e) { return 24 } }
    }
    return 0
}

fn test_trim() -> string.Error!i32 {
    let padded = string.from_utf8(" \t\r\nhello café\n\r\t ", 64)?
    let ts = string.trim_start(&padded, 64)?
    if !str_eq(&ts, "hello café\n\r\t ") { return 30 }
    let te = string.trim_end(&padded, 64)?
    if !str_eq(&te, " \t\r\nhello café") { return 31 }
    let t = string.trim(&padded, 64)?
    if !str_eq(&t, "hello café") { return 32 }

    let ws_only = string.from_utf8(" \t\r\n ", 64)?
    let ws1 = string.trim_start(&ws_only, 64)?
    let ws2 = string.trim_end(&ws_only, 64)?
    let ws3 = string.trim(&ws_only, 64)?
    if !string.is_empty(&ws1) || !string.is_empty(&ws2) || !string.is_empty(&ws3) { return 33 }

    let empty_s = string.empty()
    let et = string.trim(&empty_s, 0)?
    if !string.is_empty(&et) { return 34 }

    let tight = string.from_utf8("  hello  ", 64)?
    let ok_tight = string.trim(&tight, 5)?
    if !str_eq(&ok_tight, "hello") { return 35 }
    match string.trim(&tight, 4) {
        .ok(_) => { return 36 },
        .err(e) => { if !is_capacity(e) { return 37 } }
    }
    match string.trim_start(&tight, -1) {
        .ok(_) => { return 38 },
        .err(e) => { if !is_capacity(e) { return 39 } }
    }
    return 0
}

fn test_split() -> string.Error!i32 {
    let csv = string.from_utf8("a,b,c", 64)?
    let p1 = string.split(&csv, ",", 64)?
    if store_len(p1) != 3 { return 40 }
    if !str_eq(store_at(p1, 0), "a") { return 41 }
    if !str_eq(store_at(p1, 1), "b") { return 42 }
    if !str_eq(store_at(p1, 2), "c") { return 43 }

    let edges = string.from_utf8(",a,,b,", 64)?
    let p2 = string.split(&edges, ",", 64)?
    if store_len(p2) != 5 { return 44 }
    if !str_eq(store_at(p2, 0), "") { return 45 }
    if !str_eq(store_at(p2, 1), "a") { return 46 }
    if !str_eq(store_at(p2, 2), "") { return 47 }
    if !str_eq(store_at(p2, 3), "b") { return 48 }
    if !str_eq(store_at(p2, 4), "") { return 49 }

    let overlap = string.from_utf8("aaa", 64)?
    let p3 = string.split(&overlap, "aa", 64)?
    if store_len(p3) != 2 { return 50 }
    if !str_eq(store_at(p3, 0), "") || !str_eq(store_at(p3, 1), "a") { return 51 }

    let empty_s = string.empty()
    let p4 = string.split(&empty_s, ",", 0)?
    if store_len(p4) != 1 || !str_eq(store_at(p4, 0), "") { return 52 }

    let utf8_s = string.from_utf8("café::你好::🦀", 64)?
    let p5 = string.split(&utf8_s, "::", 64)?
    if store_len(p5) != 3 { return 53 }
    if !str_eq(store_at(p5, 0), "café") { return 54 }
    if !str_eq(store_at(p5, 1), "你好") { return 55 }
    if !str_eq(store_at(p5, 2), "🦀") { return 56 }

    // Empty separator splits on Unicode scalar boundaries (matching Rust str::split("")).
    let ae = string.from_utf8("aé", 64)?
    let p6 = string.split(&ae, "", 64)?
    if store_len(p6) != 4 { return 57 }
    if !str_eq(store_at(p6, 0), "") || !str_eq(store_at(p6, 1), "a") { return 58 }
    if !str_eq(store_at(p6, 2), "é") || !str_eq(store_at(p6, 3), "") { return 58 }

    let p7 = string.split(&empty_s, "", 0)?
    if store_len(p7) != 2 || !str_eq(store_at(p7, 0), "") || !str_eq(store_at(p7, 1), "") {
        return 59
    }

    let bad = [192u8, 175u8]
    match string.split(&csv, &bad[..], 64) {
        .ok(_) => { return 60 },
        .err(e) => { if !is_invalid_utf8(e) { return 61 } }
    }
    let two = string.from_utf8("abc,def", 64)?
    match string.split(&two, ",", 5) {
        .ok(_) => { return 62 },
        .err(e) => { if !is_capacity(e) { return 63 } }
    }
    return 0
}

fn test_lines() -> string.Error!i32 {
    let empty_s = string.empty()
    let l0 = string.lines(&empty_s, 64)?
    if store_len(l0) != 0 { return 70 }

    let one_lf = string.from_utf8("\n", 64)?
    let l1 = string.lines(&one_lf, 64)?
    if store_len(l1) != 1 || !str_eq(store_at(l1, 0), "") { return 71 }

    let one_crlf = string.from_utf8("\r\n", 64)?
    let l2 = string.lines(&one_crlf, 64)?
    if store_len(l2) != 1 || !str_eq(store_at(l2, 0), "") { return 72 }

    let mixed = string.from_utf8("alpha\nbeta\r\ncafé\n", 64)?
    let l3 = string.lines(&mixed, 64)?
    if store_len(l3) != 3 { return 73 }
    if !str_eq(store_at(l3, 0), "alpha") { return 74 }
    if !str_eq(store_at(l3, 1), "beta") { return 75 }
    if !str_eq(store_at(l3, 2), "café") { return 76 }

    let blank = string.from_utf8("a\n\nb", 64)?
    let l4 = string.lines(&blank, 64)?
    if store_len(l4) != 3 { return 77 }
    if !str_eq(store_at(l4, 0), "a") || !str_eq(store_at(l4, 1), "") { return 78 }
    if !str_eq(store_at(l4, 2), "b") { return 78 }

    let trailing_blank = string.from_utf8("a\n\n", 64)?
    let l5 = string.lines(&trailing_blank, 64)?
    if store_len(l5) != 2 || !str_eq(store_at(l5, 0), "a") || !str_eq(store_at(l5, 1), "") {
        return 79
    }

    let bare_cr = string.from_utf8("a\rb\r\r\nc", 64)?
    let l6 = string.lines(&bare_cr, 64)?
    if store_len(l6) != 2 { return 80 }
    if !str_eq(store_at(l6, 0), "a\rb\r") || !str_eq(store_at(l6, 1), "c") { return 81 }

    match string.lines(&mixed, 8) {
        .ok(_) => { return 82 },
        .err(e) => { if !is_capacity(e) { return 83 } }
    }
    return 0
}

fn test_replace_all() -> string.Error!i32 {
    let s = string.from_utf8("foo bar foo baz foo", 64)?
    let r1 = string.replace_all(&s, "foo", "qux", 64)?
    if !str_eq(&r1, "qux bar qux baz qux") { return 90 }

    let overlap = string.from_utf8("aaa", 64)?
    let r2 = string.replace_all(&overlap, "aa", "b", 64)?
    if !str_eq(&r2, "ba") { return 91 }

    let utf8_s = string.from_utf8("café-café", 64)?
    let r3 = string.replace_all(&utf8_s, "é", "e!", 64)?
    if !str_eq(&r3, "cafe!-cafe!") { return 92 }

    let del = string.from_utf8("hello", 64)?
    let r4 = string.replace_all(&del, "l", "", 64)?
    if !str_eq(&r4, "heo") { return 93 }

    let ae = string.from_utf8("aé", 64)?
    let r5 = string.replace_all(&ae, "", "-", 64)?
    if !str_eq(&r5, "-a-é-") { return 94 }

    let empty_s = string.empty()
    let r6 = string.replace_all(&empty_s, "", "-", 64)?
    if !str_eq(&r6, "-") { return 95 }

    let bad = [255u8]
    match string.replace_all(&s, &bad[..], "x", 64) {
        .ok(_) => { return 96 },
        .err(e) => { if !is_invalid_utf8(e) { return 97 } }
    }
    match string.replace_all(&s, "foo", &bad[..], 64) {
        .ok(_) => { return 98 },
        .err(e) => { if !is_invalid_utf8(e) { return 99 } }
    }
    match string.replace_all(&s, "foo", "longreplacement", 10) {
        .ok(_) => { return 100 },
        .err(e) => { if !is_capacity(e) { return 101 } }
    }
    return 0
}

fn main() -> string.Error!i32 {
    let c1 = test_predicates()?
    if c1 != 0 { return c1 }
    let c2 = test_trim()?
    if c2 != 0 { return c2 }
    let c3 = test_split()?
    if c3 != 0 { return c3 }
    let c4 = test_lines()?
    if c4 != 0 { return c4 }
    let c5 = test_replace_all()?
    if c5 != 0 { return c5 }
    return 0
}
"#,
    );

    let (code, stdout, stderr) = build_and_run_dual(&dir, "string-helpers", &entry, &[]);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}
