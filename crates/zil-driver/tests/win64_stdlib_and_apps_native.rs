//! Windows x64 W4 standard library platform abstraction and 8 everyday apps verification suite.
//!
//! Cross-platform tests (run on Linux and Windows):
//! - Verify all 7 Windows `std` modules (`io`, `fs`, `path`, `env`, `time`, `net`, `float`)
//!   and all 8 everyday application projects compile through independent `zil-check` and emit
//!   clean `x86_64-pc-windows-msvc` IR with Win32/UCRT/Winsock2 symbols and zero Linux POSIX calls.
//! - Verify `E-STD-RESERVED` rejects forged `std.*` modules on Windows targets.
//!
//! Native Windows x64 tests (`#[cfg(target_os = "windows")]`, O0 & O2):
//! - `std.time` (`QueryPerformanceCounter` + `Sleep`)
//! - `std.env` (`arg_count`/`arg`/`args`/`get`/`current_dir`)
//! - `std.float` (`_create_locale`/`_strtod_l`/`_strtof_l`/`_snprintf_l` parse & format round-trip)
//! - `std.path` (Windows drive prefixes `C:\`, mixed `/` & `\`, `..` clamping, UNC paths)
//! - `std.io` binary fidelity (`\n`, `\r\n`, `\0`, `0x1A` Ctrl-Z preserved verbatim without CRLF/EOF mangling)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zil_backend_llvm::NativeBackend;
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompilationSession, CompileOptions, ProjectManifest, ProjectSources, Stage, compile_project,
    compile_source,
};

#[cfg(target_os = "windows")]
use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use zil_driver::{CodegenOptions, Optimization};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

const WIN64_STD_SURFACE_PROGRAM: &str = r#"
import std.env
import std.float
import std.fs
import std.io
import std.net
import std.path
import std.time

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

fn check_windows_paths() -> io.Error!i32 {
    if !bytes_eq(path.file_name("C:\\Users\\dev\\app.zil"), "app.zil") { return 10 }
    if !bytes_eq(path.parent("C:\\Users\\dev\\app.zil"), "C:\\Users\\dev") { return 11 }
    if !bytes_eq(path.extension("C:\\Users\\dev\\app.zil"), "zil") { return 12 }
    if !bytes_eq(path.file_name("C:\\"), "") { return 13 }
    if !bytes_eq(path.parent("C:\\"), "") { return 14 }
    if !bytes_eq(path.parent("C:\\top"), "C:\\") { return 15 }

    let n1 = path.normalize("C:\\a\\b\\..\\c//d\\")?
    if !bytes_eq(bytes_view(n1), "C:/a/c/d") { return 16 }
    let n2 = path.normalize("C:\\..\\..\\root")?
    if !bytes_eq(bytes_view(n2), "C:/root") { return 17 }
    let n3 = path.normalize("\\\\srv\\share\\dir\\..\\file.txt")?
    if !bytes_eq(bytes_view(n3), "\\\\srv/share/file.txt") { return 18 }

    let j1 = path.join("C:\\base", "D:\\override\\x.txt")?
    if !bytes_eq(bytes_view(j1), "D:\\override\\x.txt") { return 19 }
    let j2 = path.join("C:\\base\\", "child.txt")?
    if !bytes_eq(bytes_view(j2), "C:\\base\\child.txt") { return 20 }
    let j3 = path.join("C:\\base", "child.txt")?
    if !bytes_eq(bytes_view(j3), "C:\\base/child.txt") { return 21 }
    return 0
}

fn check_float() -> i32 {
    match float.parse_f64("3.5") {
        .err(_) => { return 30 }
        .ok(v) => { if v != 3.5 { return 31 } }
    }
    match float.parse_f64("-12.25e1") {
        .err(_) => { return 32 }
        .ok(v) => { if v != -122.5 { return 33 } }
    }
    match float.parse_f64("bad") {
        .ok(_) => { return 34 }
        .err(_) => {}
    }
    match float.parse_f32("1.25") {
        .err(_) => { return 35 }
        .ok(v) => { if v != 1.25f32 { return 36 } }
    }
    match float.format_f64(12.5) {
        .err(_) => { return 37 }
        .ok(dec) => {
            if !bytes_eq(&dec.bytes[0..dec.len], "12.5") { return 38 }
        }
    }
    match float.format_f32(2.5f32) {
        .err(_) => { return 39 }
        .ok(dec) => {
            if !bytes_eq(&dec.bytes[0..dec.len], "2.5") { return 40 }
        }
    }
    return 0
}

fn main() -> io.Error!i32 {
    let p_rc = check_windows_paths()?
    if p_rc != 0 { return p_rc }
    let f_rc = check_float()
    if f_rc != 0 { return f_rc }

    // std.time: monotonic clock advances across sleep_ms(15).
    let t0 = time.monotonic_ns()
    time.sleep_ms(15u64)
    let t1 = time.monotonic_ns()
    if t0 == 0u64 || t1 <= t0 { return 50 }

    // std.env: args, env.get, current_dir.
    if env.arg_count() < 2 { return 51 }
    let cwd = env.current_dir()?
    if bytes_len(cwd) == 0 { return 52 }
    match env.get("ZIL_WIN64_ENV_CHECK") {
        Option.None() => { return 53 }
        Option.Some(val) => {
            if !bytes_eq(bytes_view(val), "ok_win64") { return 54 }
        }
    }

    // Binary file fidelity: LF, CRLF, NUL (0x00), Ctrl-Z (0x1A), high bytes (0xFF).
    match env.arg(1) {
        Option.None() => { return 55 }
        Option.Some(bin_path) => {
            let raw = [65u8, 10u8, 13u8, 10u8, 0u8, 26u8, 255u8, 90u8]
            fs.write_bytes(bytes_view(bin_path), &raw[0..8])?
            let back = fs.read_to_bytes(bytes_view(bin_path))?
            if !bytes_eq(bytes_view(back), &raw[0..8]) { return 56 }
            match fs.remove_file(bytes_view(bin_path)) {
                .ok(_) => {}
                .err(_) => { return 57 }
            }
        }
    }

    // Quick Winsock2 loopback bind/local_port/close check.
    var listener = net.TcpListener.bind(net.localhost(), 0)?
    let port = listener.local_port()?
    if port <= 0 || port > 65535 { return 58 }
    listener.close()?

    io.println("win64-std-ok")?
    return 0
}
"#;

const EVERYDAY_APPS: &[&str] = &[
    "examples/everyday-apps/text-stats-and-file-search",
    "examples/everyday-apps/csv-processor",
    "examples/everyday-apps/json-processor",
    "examples/everyday-apps/cli-todo-app",
    "examples/everyday-apps/http-mini-service-and-client",
    "examples/everyday-apps/file-batch-processor",
    "examples/everyday-apps/graph-dependency-analyzer",
    "examples/everyday-apps/async-task-scheduler",
];

#[test]
fn win64_std_modules_and_all_8_everyday_apps_verify_for_windows_target() {
    let target = TargetProfile::windows_x64_llvm();

    // 1. Verify combined 7-module Windows std surface program.
    let src_unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "win64_std_surface.zil",
        WIN64_STD_SURFACE_PROGRAM,
    );
    let report = compile_source(
        &src_unit,
        &CompileOptions {
            stop_after: Stage::Verify,
            target: target.clone(),
            backend: NativeBackend::Clang,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "Win64 std surface program failed Verify: {:?}",
        report.diagnostics
    );
    assert_eq!(report.verification_runs, 1);

    // 1b. On Unix hosts, also emit the real Win64 MSVC LLVM 23 IR file and verify
    //     Win32/UCRT/Winsock2 symbols, -lws2_32 link flag, and zero Linux POSIX symbols.
    #[cfg(unix)]
    {
        let probe_dir =
            std::env::temp_dir().join(format!("zil-win64-w4-ir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&probe_dir);
        std::fs::create_dir_all(&probe_dir).expect("create probe dir");
        let out_exe = probe_dir.join("std_surface.exe");
        let ir_report = compile_source(
            &src_unit,
            &CompileOptions {
                stop_after: Stage::Build,
                target: target.clone(),
                backend: NativeBackend::Clang,
                clang: Some(PathBuf::from("/bin/true")),
                output: Some(out_exe.clone()),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            ir_report.status,
            ResultCategory::Success,
            "Win64 std surface IR emit failed: {:?}",
            ir_report.diagnostics
        );
        assert!(
            ir_report
                .commands
                .iter()
                .flatten()
                .any(|arg| arg == "-lws2_32"),
            "Windows link command must include -lws2_32, got {:?}",
            ir_report.commands
        );
        let ir_path = PathBuf::from(format!("{}.ll", out_exe.display()));
        let ir = std::fs::read_to_string(&ir_path)
            .unwrap_or_else(|e| panic!("failed to read emitted {}: {e}", ir_path.display()));
        assert!(ir.contains("target triple = \"x86_64-pc-windows-msvc\""));
        for expected in [
            "@_sopen_s",
            "@_read",
            "@_write",
            "@_setmode",
            "@GetFileAttributesExA",
            "@MoveFileExA",
            "@FindFirstFileA",
            "@QueryPerformanceCounter",
            "@Sleep",
            "@WSAStartup",
            "@closesocket",
            "@_create_locale",
            "@_strtod_l",
            "@_snprintf_l",
            "@_getcwd",
        ] {
            assert!(
                ir.contains(expected),
                "Win64 std IR must reference Windows/UCRT/Winsock2 symbol {expected}"
            );
        }
        for forbidden in [
            "@mkdirat",
            "@renameat",
            "@opendir",
            "@readdir",
            "@closedir",
            "@clock_gettime",
            "@nanosleep",
            "@newlocale",
            "@freelocale",
            "@uselocale",
            "@strtod_l",
            "@strfromd",
            "@__errno_location",
        ] {
            assert!(
                !ir.contains(forbidden),
                "Win64 std IR must not reference Linux POSIX symbol {forbidden}"
            );
        }
        let _ = std::fs::remove_dir_all(&probe_dir);
    }

    // 2. Verify all 8 everyday application manifests for TargetProfile::windows_x64_llvm().
    let root = repo_root();
    let limits = CompileOptions::default().limits;
    for app_rel in EVERYDAY_APPS {
        let app_dir = root.join(app_rel);
        let manifest = ProjectManifest::open(&app_dir, &limits)
            .unwrap_or_else(|e| panic!("failed to open Zil.toml in {app_rel}: {e:?}"));
        let manifest = Arc::new(manifest);
        let mut session = CompilationSession::new();
        let rep = session.compile_path(
            &manifest.entry,
            &CompileOptions {
                stop_after: Stage::Verify,
                target: target.clone(),
                backend: NativeBackend::Clang,
                project: Some(Arc::clone(&manifest)),
                build_inputs: manifest.fingerprint,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            rep.status,
            ResultCategory::Success,
            "everyday app {app_rel} failed Windows-target Verify: {:?}",
            rep.diagnostics
        );
        assert_eq!(rep.verification_runs, 1);
    }
}

#[test]
fn win64_forged_std_module_rejected_with_e_std_reserved() {
    let mut paths = BTreeMap::new();
    paths.insert(vec!["main".into()], ModuleId(1));
    paths.insert(vec!["std".into(), "io".into()], ModuleId(2));
    let project = ProjectSources {
        entry: ModuleId(1),
        sources: vec![
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "main.zil",
                "import std.io\nfn main() -> i32 { return 0 }\n",
            ),
            SourceUnit::new(
                FileId(2),
                ModuleId(2),
                "lib/std/io.zil",
                "pub fn forged() -> i32 { return 1 }\n",
            ),
        ],
        paths,
    };

    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Verify,
            target: TargetProfile::windows_x64_llvm(),
            ..CompileOptions::default()
        },
    );
    assert_eq!(report.status, ResultCategory::SafeReject);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-STD-RESERVED"),
        "expected E-STD-RESERVED on forged Windows std.io, got {:?}",
        report.diagnostics
    );
}

#[cfg(target_os = "windows")]
#[test]
fn win64_native_std_time_env_float_binary_io_and_windows_paths() {
    let dir = std::env::temp_dir().join(format!("zil-win64-w4-std-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let entry = dir.join("std_surface.zil");
    std::fs::write(&entry, WIN64_STD_SURFACE_PROGRAM).expect("write std_surface.zil");

    for (label, opt) in [("O0", Optimization::O0), ("O2", Optimization::O2)] {
        let exe = dir.join(format!("std_surface_{label}.exe"));
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &entry,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
                backend: NativeBackend::Clang,
                output: Some(exe.clone()),
                codegen: CodegenOptions { units: 1, jobs: 1 },
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "Win64 std surface build failed ({label}): {:?}",
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let bin_file = dir.join(format!("binary_{label}.dat"));
        let out = Command::new(&exe)
            .arg(&bin_file)
            .env("ZIL_WIN64_ENV_CHECK", "ok_win64")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr ({label}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "Win64 std surface exited with {:?} ({label})",
            out.status.code()
        );
        // Verify exact LF byte output (no CRLF text-mode mangling on stdout).
        assert_eq!(
            out.stdout.as_slice(),
            b"win64-std-ok\n",
            "expected verbatim LF stdout ({label})"
        );
        assert!(!bin_file.exists(), "binary test file should be removed");
    }

    let _ = std::fs::remove_dir_all(&dir);
}
