//! Dual-backend (`clang-o0`, `clang-o2`, `llvmapi-o0`, `llvmapi-o2`) native
//! execution tests for everyday standard library modules:
//! - `std.io` (`print`, `println`, `eprint`, `eprintln`, `print_i64`, `println_i64`, `stderr`)
//! - `std.fs` (`write_bytes`, `exists`, `read_to_bytes`, `remove_file`)
//! - `std.env` (`var`, `current_dir`, `exit`)
//! - `std.time` (`monotonic_ns`, `sleep_ms`)

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

fn build_and_run_dual(
    dir: &Path,
    tag: &str,
    entry: &Path,
    envs: &[(&str, &str)],
) -> (i32, Vec<u8>, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>, Vec<u8>)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{tag}-{label}"));
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

        let mut cmd = Command::new(&exe);
        cmd.stdin(Stdio::null())
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
                "exit code mismatch for {} on {label}",
                entry.display()
            );
            assert_eq!(
                out.stdout,
                *prev_stdout,
                "stdout mismatch for {} on {label}",
                entry.display()
            );
            assert_eq!(
                out.stderr,
                *prev_stderr,
                "stderr mismatch for {} on {label}",
                entry.display()
            );
        } else {
            expected = Some((code, out.stdout, out.stderr));
        }
    }
    expected.expect("at least one backend ran")
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
