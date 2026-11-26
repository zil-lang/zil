//! Dual-backend (`clang-o0`, `clang-o2`, `llvmapi-o0`, `llvmapi-o2`) native
//! execution tests for `std.net` POSIX TCP sockets (`Ipv4Addr`, `TcpListener`, `TcpStream`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("zil-stdlib-net-tcp-{name}-{}", std::process::id()));
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

fn build_and_run_dual_units(
    dir: &Path,
    tag: &str,
    entry: &Path,
    units: usize,
) -> (i32, Vec<u8>, Vec<u8>) {
    let mut expected: Option<(i32, Vec<u8>, Vec<u8>)> = None;
    let jobs = if units > 1 { 2 } else { 1 };
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!(
            "bin-{tag}-u{units}-{label}{}",
            std::env::consts::EXE_SUFFIX
        ));
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

        let out = Command::new(&exe)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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

#[test]
fn std_net_loopback_echo_and_http_roundtrip_dual_backend_and_multi_cu() {
    let dir = temp_dir("echo-http");
    let entry = write_source(
        &dir,
        "main.zil",
        r#"
import std.io
import std.net
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

fn test_http_conn(listener: &net.TcpListener, addr: net.Ipv4Addr, port: i32) -> io.Error!i32 {
    var client = net.TcpStream.connect(addr, port)?
    var server = listener.accept()?

    let req_bytes = "GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n"
    client.write_all(req_bytes)?
    client.shutdown_write()?

    let req_buf = server.read_to_end(4096)?
    if !bytes_eq(bytes_view(req_buf), req_bytes) {
        return 3
    }
    match string.from_utf8(bytes_view(req_buf), 4096) {
        .err(_) => { return 4 },
        .ok(req_str) => {
            match string.starts_with(&req_str, "GET /health HTTP/1.0") {
                .ok(ok) => { if !ok { return 5 } },
                .err(_) => { return 5 }
            }
            match string.contains(&req_str, "Host: localhost") {
                .ok(ok) => { if !ok { return 6 } },
                .err(_) => { return 6 }
            }
        }
    }

    let resp_bytes = "HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok"
    server.write_all(resp_bytes)?
    server.shutdown_write()?

    let resp_buf = client.read_to_end(4096)?
    if !bytes_eq(bytes_view(resp_buf), resp_bytes) {
        return 7
    }
    client.close()?
    server.close()?
    return 0
}

fn test_echo_conn(listener: &net.TcpListener, port: i32) -> io.Error!i32 {
    var client = net.TcpStream.connect(net.localhost(), port)?
    var server = listener.accept()?

    var empty_buf = [0u8]
    let zero_w = client.write(&empty_buf[0..0])?
    if zero_w != 0 { return 8 }
    let zero_r = server.read(&mut empty_buf[0..0])?
    if zero_r != 0 { return 9 }

    let msg = "echo-payload-0123456789"
    let first_written = client.write(msg)?
    if first_written <= 0 || first_written > msg.len() { return 10 }
    if first_written < msg.len() {
        client.write_all(&msg[first_written..msg.len()])?
    }
    client.shutdown_write()?

    var scratch = io.empty_buffer()
    var total: i64 = 0
    loop {
        let n = server.read(&mut scratch[0..8])?
        if n == 0 { break }
        server.write_all(&scratch[0..n])?
        total += n
    }
    if total != msg.len() { return 11 }
    server.shutdown_write()?

    let echoed = client.read_to_end(1024)?
    if !bytes_eq(bytes_view(echoed), msg) { return 12 }
    return 0
}

fn main() -> io.Error!i32 {
    let lo = net.localhost()
    let explicit_lo = net.ipv4(127u8, 0u8, 0u8, 1u8)
    if lo.a != explicit_lo.a || lo.b != explicit_lo.b || lo.c != explicit_lo.c || lo.d != explicit_lo.d {
        return 1
    }

    var listener = net.TcpListener.bind(lo, 0)?
    let port = listener.local_port()?
    if port <= 0 || port > 65535 {
        return 2
    }

    let r1 = test_http_conn(&listener, explicit_lo, port)?
    if r1 != 0 { return r1 }

    let r2 = test_echo_conn(&listener, port)?
    if r2 != 0 { return r2 }

    listener.close()?
    io.println("net-ok")?
    return 0
}
"#,
    );

    let (code1, stdout1, stderr1) = build_and_run_dual_units(&dir, "echo-http", &entry, 1);
    assert_eq!(code1, 0);
    assert_eq!(stdout1, b"net-ok\n");
    assert!(stderr1.is_empty());

    let (code2, stdout2, stderr2) = build_and_run_dual_units(&dir, "echo-http", &entry, 2);
    assert_eq!(code2, 0);
    assert_eq!(stdout2, b"net-ok\n");
    assert!(stderr2.is_empty());
}

#[test]
fn std_net_multithreaded_client_server_with_thread_scope_dual_backend() {
    let dir = temp_dir("threaded-tcp");
    let entry = write_source(
        &dir,
        "main.zil",
        r#"
import std.io
import std.net

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

fn send_via_mut_ref(stream: &mut net.TcpStream, payload: &[u8]) -> io.Error!void {
    stream.write_all(payload)?
    return stream.shutdown_write()
}

fn client_worker(port: i32) -> i32 {
    match net.TcpStream.connect(net.localhost(), port) {
        .err(_) => { return 10 },
        .ok(s) => {
            var stream = give s
            // Build a 4096-byte payload (> 256-byte read chunk) to exercise
            // multi-chunk streaming and ByteBuffer geometric growth.
            let payload = bytes_new()
            var i: i64 = 0
            while i < 4096 {
                if !bytes_push(payload, u8(65 + (i % 26))) { return 15 }
                i += 1
            }
            match send_via_mut_ref(&mut stream, bytes_view(payload)) {
                .err(_) => { return 11 },
                .ok(_) => {}
            }
            match stream.read_to_end(8192) {
                .err(_) => { return 13 },
                .ok(reply) => {
                    if !bytes_eq(bytes_view(reply), bytes_view(payload)) {
                        return 14
                    }
                }
            }
            return 0
        }
    }
}

fn main() -> io.Error!i32 {
    let listener = net.TcpListener.bind(net.localhost(), 0)?
    let port = listener.local_port()?

    let (created, child_rc, parent_rc) = thread_scope(client_worker, port, () -> i32 => {
        match listener.accept() {
            .err(_) => { return 20 },
            .ok(s) => {
                var srv = give s
                match srv.read_to_end(8192) {
                    .err(_) => { return 21 },
                    .ok(req) => {
                        if bytes_len(req) != 4096 {
                            return 22
                        }
                        match srv.write_all(bytes_view(req)) {
                            .err(_) => { return 23 },
                            .ok(_) => {}
                        }
                    }
                }
                match srv.shutdown_write() {
                    .err(_) => { return 24 },
                    .ok(_) => {}
                }
                return 0
            }
        }
    })
    if !created { return 1 }
    if child_rc != 0 { return child_rc }
    if parent_rc != 0 { return parent_rc }
    return 0
}
"#,
    );

    let (code, stdout, stderr) = build_and_run_dual_units(&dir, "threaded-tcp", &entry, 1);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}

#[test]
fn std_net_errors_drop_cleanup_limits_and_sigpipe_safety_dual_backend() {
    let dir = temp_dir("errors-cleanup");
    let entry = write_source(
        &dir,
        "main.zil",
        r#"
import std.io
import std.net

fn check_read_limit(listener: &net.TcpListener, lo: net.Ipv4Addr, port: i32) -> io.Error!i32 {
    var c = net.TcpStream.connect(lo, port)?
    var s = listener.accept()?
    c.write_all("0123456789")?
    c.shutdown_write()?
    match s.read_to_end(-1) {
        .ok(_) => { return 8 },
        .err(e) => {
            match e {
                io.Error.Capacity() => {},
                _ => { return 9 }
            }
        }
    }
    match s.read_to_end(5) {
        .ok(_) => { return 10 },
        .err(e) => {
            match e {
                io.Error.Capacity() => {},
                _ => { return 11 }
            }
        }
    }
    return 0
}

fn check_exact_limit_and_post_close(listener: &net.TcpListener, lo: net.Ipv4Addr, port: i32) -> io.Error!i32 {
    var c = net.TcpStream.connect(lo, port)?
    var s = listener.accept()?
    c.write_all("exact")?
    c.shutdown_write()?
    let exact_buf = s.read_to_end(5)?
    if bytes_len(exact_buf) != 5 { return 12 }
    // Post-close operations on TcpStream must return .err, and second close is idempotent.
    c.close()?
    c.close()?
    var tmp = [0u8, 0u8]
    match c.read(&mut tmp[0..2]) {
        .ok(_) => { return 13 },
        .err(_) => {}
    }
    match c.write("x") {
        .ok(_) => { return 14 },
        .err(_) => {}
    }
    match c.shutdown_write() {
        .ok(_) => { return 15 },
        .err(_) => {}
    }
    return 0
}

fn check_sigpipe_safety(listener: &net.TcpListener, lo: net.Ipv4Addr, port: i32) -> io.Error!i32 {
    var c = net.TcpStream.connect(lo, port)?
    var s = listener.accept()?
    // Send data into s's receive buffer, then close s without reading so Linux
    // immediately sends TCP RST to c.
    c.write_all("trigger-rst")?
    s.close()?
    var saw_err = false
    var attempts: i32 = 0
    while attempts < 64 {
        match c.write_all("after-peer-close-payload-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx") {
            .ok(_) => { attempts += 1 },
            .err(_) => { saw_err = true; break }
        }
    }
    if !saw_err { return 16 }
    return 0
}

fn main() -> io.Error!i32 {
    let lo = net.localhost()

    // 1. Invalid port validation on bind and connect.
    match net.TcpListener.bind(lo, -1) {
        .ok(_) => { return 1 },
        .err(_) => {}
    }
    match net.TcpListener.bind(lo, 65536) {
        .ok(_) => { return 2 },
        .err(_) => {}
    }
    match net.TcpStream.connect(lo, -5) {
        .ok(_) => { return 3 },
        .err(_) => {}
    }
    match net.TcpStream.connect(lo, 70000) {
        .ok(_) => { return 4 },
        .err(_) => {}
    }

    // 2. Explicit close (including idempotent second close), post-close error,
    //    and connect to closed/unused port returns .err.
    var temp_listener = net.TcpListener.bind(lo, 0)?
    let closed_port = temp_listener.local_port()?
    temp_listener.close()?
    temp_listener.close()?
    match temp_listener.local_port() {
        .ok(_) => { return 5 },
        .err(_) => {}
    }
    match temp_listener.accept() {
        .ok(_) => { return 6 },
        .err(_) => {}
    }
    match net.TcpStream.connect(lo, closed_port) {
        .ok(_) => { return 7 },
        .err(_) => {}
    }

    // 3. read_to_end byte limit enforcement and post-close stream checks.
    let listener = net.TcpListener.bind(lo, 0)?
    let port = listener.local_port()?
    let r1 = check_read_limit(&listener, lo, port)?
    if r1 != 0 { return r1 }
    let r2 = check_exact_limit_and_post_close(&listener, lo, port)?
    if r2 != 0 { return r2 }

    // 4. MSG_NOSIGNAL broken-peer write must return .err (EPIPE/ECONNRESET), not SIGPIPE.
    let r3 = check_sigpipe_safety(&listener, lo, port)?
    if r3 != 0 { return r3 }

    // 5. Automatic Drop cleanup loop (> 1024 default ulimit -n, proves zero fd leaks).
    var round: i32 = 0
    while round < 1100 {
        let ephemeral = net.TcpListener.bind(lo, 0)?
        let ep_port = ephemeral.local_port()?
        if ep_port <= 0 { return 17 }
        round += 1
    }

    return 0
}
"#,
    );

    let (code, stdout, stderr) = build_and_run_dual_units(&dir, "errors-cleanup", &entry, 1);
    assert_eq!(code, 0);
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}

#[test]
fn std_net_rejects_mutable_methods_on_immutable_resource_bindings() {
    let dir = temp_dir("mutability-reject");

    let immutable_close = write_source(
        &dir,
        "immutable_close.zil",
        r#"
import std.io
import std.net

fn main() -> io.Error!i32 {
    let listener = net.TcpListener.bind(net.localhost(), 0)?
    listener.close()?
    return 0
}
"#,
    );
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&immutable_close, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "calling close() on immutable `let listener` must be rejected: {:?}",
        report.diagnostics
    );

    let shared_to_mut = write_source(
        &dir,
        "shared_to_mut.zil",
        r#"
import std.io
import std.net

fn touch(s: &mut net.TcpStream) -> io.Error!void {
    return s.shutdown_write()
}

fn main() -> io.Error!i32 {
    let listener = net.TcpListener.bind(net.localhost(), 0)?
    let port = listener.local_port()?
    let stream = net.TcpStream.connect(net.localhost(), port)?
    touch(&stream)?
    return 0
}
"#,
    );
    let mut session2 = CompilationSession::new();
    let report2 = session2.compile_path(&shared_to_mut, &opts);
    assert_eq!(
        report2.status,
        ResultCategory::SafeReject,
        "passing shared `&stream` to `&mut net.TcpStream` must be rejected: {:?}",
        report2.diagnostics
    );
}
