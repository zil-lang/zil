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
            "@_wsopen_s",
            "@_read",
            "@_write",
            "@_setmode",
            "@GetFileAttributesW",
            "@GetFileAttributesExW",
            "@CreateDirectoryW",
            "@DeleteFileW",
            "@RemoveDirectoryW",
            "@MoveFileExW",
            "@FindFirstFileW",
            "@FindNextFileW",
            "@QueryPerformanceCounter",
            "@Sleep",
            "@WSAStartup",
            "@closesocket",
            "@_create_locale",
            "@_strtod_l",
            "@_snprintf_l",
            "@GetEnvironmentVariableW",
            "@GetCurrentDirectoryW",
            "@GetCommandLineW",
        ] {
            assert!(
                ir.contains(expected),
                "Win64 std IR must reference Windows Unicode/UCRT/Winsock2 symbol {expected}"
            );
        }
        for forbidden in [
            "@_sopen_s",
            "@GetFileAttributesExA",
            "@MoveFileExA",
            "@FindFirstFileA",
            "@FindNextFileA",
            "@_access",
            "@_unlink",
            "@_mkdir",
            "@_rmdir",
            "@_getcwd",
            "@getenv",
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

    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }

    for (label, backend, opt) in configs {
        let exe = dir.join(format!("std_surface_{label}.exe"));
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &entry,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
                backend,
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

const WIN64_UNICODE_BOUNDARY_PROGRAM: &str = r#"
import std.env
import std.fs
import std.io
import std.path

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

fn ends_with(hay: &[u8], needle: &[u8]) -> bool {
    if needle.len() > hay.len() { return false }
    let off = hay.len() - needle.len()
    var i: i64 = 0
    while i < needle.len() {
        if hay[off + i] != needle[i] { return false }
        i += 1
    }
    return true
}

fn run_unicode_checks() -> io.Error!i32 {
    // 1. Lossless UTF-8 CLI args from Win32 GetCommandLineW() (incl. CJK, emoji, quotes, backslashes, empty arg).
    if env.arg_count() != 7 { return 10 }
    let all_args = env.args()?
    if store_len(all_args) != 7 { return 11 }
    let root = bytes_view(*store_at(all_args, 1))
    if !bytes_eq(bytes_view(*store_at(all_args, 2)), "中文参数") { return 12 }
    if !bytes_eq(bytes_view(*store_at(all_args, 3)), "emoji 🚀 test") { return 13 }
    if !bytes_eq(bytes_view(*store_at(all_args, 4)), "a\"b\\c") { return 14 }
    if !bytes_eq(bytes_view(*store_at(all_args, 5)), "") { return 15 }
    if !bytes_eq(bytes_view(*store_at(all_args, 6)), "café") { return 16 }

    match env.arg(2) {
        Option.None() => { return 17 },
        Option.Some(v) => {
            if !bytes_eq(bytes_view(v), "中文参数") { return 18 }
        }
    }
    match env.arg(99) {
        Option.Some(_) => { return 19 },
        Option.None() => {}
    }

    // 2. Unicode environment variables via Win32 GetEnvironmentVariableW().
    match env.get("ZIL_UNICODE_VAR") {
        Option.None() => { return 20 },
        Option.Some(v) => {
            if !bytes_eq(bytes_view(v), "你好世界🚀") { return 21 }
        }
    }
    match env.get("ZIL_UNICODE_UNSET_KEY") {
        Option.Some(_) => { return 22 },
        Option.None() => {}
    }
    let bad_utf8: [u8; 2] = [255u8, 254u8]
    match env.get(&bad_utf8[0..2]) {
        Option.Some(_) => { return 23 },
        Option.None() => {}
    }

    // 3. Unicode current working directory via Win32 GetCurrentDirectoryW().
    let cwd = env.current_dir()?
    if !ends_with(bytes_view(cwd), "中文工作_cwd_🚀") { return 24 }

    // 4. Unicode filesystem operations via Win32 *W + UCRT _wsopen_s.
    let sub = path.join(root, "中文目录/子目录_αβγ")?
    fs.mkdir_all(bytes_view(sub))?
    if !fs.exists(bytes_view(sub)) || !fs.is_dir(bytes_view(sub)) { return 30 }

    let file1 = path.join(bytes_view(sub), "日本語_🎉.txt")?
    let payload = "你好，世界！🚀\nsecond-line\0end"
    fs.write_bytes(bytes_view(file1), payload)?
    if !fs.exists(bytes_view(file1)) || !fs.is_file(bytes_view(file1)) { return 31 }
    let meta1 = fs.metadata(bytes_view(file1))?
    if !meta1.is_file || meta1.is_dir || meta1.size != payload.len() { return 32 }
    if fs.file_size(bytes_view(file1))? != payload.len() { return 33 }
    let read1 = fs.read_to_bytes(bytes_view(file1))?
    if !bytes_eq(bytes_view(read1), payload) { return 34 }

    let file2 = path.join(bytes_view(sub), "café.dat")?
    fs.write_bytes(bytes_view(file2), "coffee")?

    // Directory traversal must return exact valid UTF-8 filenames via FindFirstFileW/FindNextFileW.
    let entries = fs.read_dir(bytes_view(sub))?
    if store_len(entries) != 2 { return 35 }
    var seen_jp: i32 = 0
    var seen_cafe: i32 = 0
    while store_len(entries) > 0 {
        match store_pop(entries) {
            .err(_) => { return 36 },
            .ok(item) => {
                let name = bytes_view(item)
                if bytes_eq(name, "日本語_🎉.txt") { seen_jp += 1 }
                else if bytes_eq(name, "café.dat") { seen_cafe += 1 }
                else { return 37 }
            }
        }
    }
    if seen_jp != 1 || seen_cafe != 1 { return 38 }

    // Unicode rename via MoveFileExW.
    let renamed = path.join(bytes_view(sub), "改名_αβγ.txt")?
    fs.rename(bytes_view(file1), bytes_view(renamed))?
    if fs.exists(bytes_view(file1)) || !fs.is_file(bytes_view(renamed)) { return 39 }
    let read_ren = fs.read_to_bytes(bytes_view(renamed))?
    if !bytes_eq(bytes_view(read_ren), payload) { return 40 }

    // 5. Invalid UTF-8 paths must be rejected cleanly without UB.
    if fs.exists(&bad_utf8[0..2]) { return 41 }
    match io.open_read(&bad_utf8[0..2]) {
        .ok(_) => { return 42 },
        .err(e) => {
            match e {
                io.Error.InvalidPath() => {},
                _ => { return 43 }
            }
        }
    }
    let overlong: [u8; 2] = [192u8, 175u8]
    match fs.metadata(&overlong[0..2]) {
        .ok(_) => { return 44 },
        .err(e) => {
            match e {
                io.Error.InvalidPath() => {},
                _ => { return 45 }
            }
        }
    }

    // Cleanup via DeleteFileW + RemoveDirectoryW.
    match fs.remove_file(bytes_view(renamed)) {
        .ok(_) => {},
        .err(_) => { return 46 }
    }
    match fs.remove_file(bytes_view(file2)) {
        .ok(_) => {},
        .err(_) => { return 47 }
    }
    fs.remove_dir(bytes_view(sub))?
    let parent_sub = path.parent(bytes_view(sub))
    fs.remove_dir(parent_sub)?
    if fs.exists(parent_sub) { return 48 }

    io.println("win64-unicode-ok")?
    return 0
}

fn main() -> i32 {
    match run_unicode_checks() {
        .ok(code) => { return code },
        .err(_) => { return 99 }
    }
}
"#;

#[test]
fn win64_unicode_boundary_program_verifies_for_windows_target() {
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "win64_unicode.zil",
            WIN64_UNICODE_BOUNDARY_PROGRAM,
        ),
        &CompileOptions {
            stop_after: Stage::Verify,
            backend: NativeBackend::Clang,
            target: TargetProfile::windows_x64_llvm(),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "Win64 Unicode boundary program must verify for Windows target: {:?}",
        report.diagnostics
    );
    assert_eq!(report.verification_runs, 1);
}

#[cfg(target_os = "windows")]
#[test]
fn win64_native_unicode_paths_env_and_args_roundtrip() {
    let dir = std::env::temp_dir().join(format!("zil-win64-unicode-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let unicode_cwd = dir.join("中文工作_cwd_🚀");
    std::fs::create_dir_all(&unicode_cwd).expect("create unicode cwd");

    let entry = dir.join("unicode_boundary.zil");
    std::fs::write(&entry, WIN64_UNICODE_BOUNDARY_PROGRAM).expect("write unicode_boundary.zil");

    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }

    for (label, backend, opt) in configs {
        let exe = dir.join(format!("unicode_boundary_{label}.exe"));
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &entry,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
                backend,
                output: Some(exe.clone()),
                codegen: CodegenOptions { units: 1, jobs: 1 },
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "Win64 Unicode boundary build failed ({label}): {:?}",
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let root_dir = dir.join(format!("fs_root_{label}"));
        std::fs::create_dir_all(&root_dir).expect("create root_dir");

        let out = Command::new(&exe)
            .current_dir(&unicode_cwd)
            .arg(&root_dir)
            .arg("中文参数")
            .arg("emoji 🚀 test")
            .arg("a\"b\\c")
            .arg("")
            .arg("café")
            .env("ZIL_UNICODE_VAR", "你好世界🚀")
            .env_remove("ZIL_UNICODE_UNSET_KEY")
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
            "Win64 Unicode boundary exited with {:?} ({label}), stdout={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout)
        );
        assert_eq!(
            out.stdout.as_slice(),
            b"win64-unicode-ok\n",
            "expected verbatim LF stdout ({label})"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

const WIN64_ARGV_DUMPER_PROGRAM: &str = r#"
import std.env
import std.io

fn dump_args() -> io.Error!i32 {
    let n = env.arg_count()
    var i: i64 = 1
    while i < n {
        match env.arg(i) {
            Option.None() => { return 21 },
            Option.Some(arg) => {
                let v = bytes_view(arg)
                io.print_i64(v.len())?
                io.print(":")?
                io.print(v)?
                io.print("\n")?
            }
        }
        i += 1
    }
    return 0
}

fn main() -> i32 {
    match dump_args() {
        .ok(code) => { return code },
        .err(_) => { return 98 }
    }
}
"#;

#[test]
fn win64_argv_differential_dumper_verifies_for_windows_target() {
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "win64_argv_dumper.zil",
            WIN64_ARGV_DUMPER_PROGRAM,
        ),
        &CompileOptions {
            stop_after: Stage::Verify,
            backend: NativeBackend::Clang,
            target: TargetProfile::windows_x64_llvm(),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "Win64 argv dumper must verify for Windows target: {:?}",
        report.diagnostics
    );
    assert_eq!(report.verification_runs, 1);
}

#[cfg(target_os = "windows")]
#[test]
fn win64_native_argv_matches_msvc_crt_wmain_differential() {
    use std::os::windows::process::CommandExt;

    let dir = std::env::temp_dir().join(format!("zil-win64-argv-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");

    // 1. Build ZIL argv dumper.
    let zil_src = dir.join("argv_dumper.zil");
    std::fs::write(&zil_src, WIN64_ARGV_DUMPER_PROGRAM).expect("write argv_dumper.zil");
    let zil_exe = dir.join("zil_argv_dumper.exe");
    let mut session = CompilationSession::new();
    let report = session.compile_path(
        &zil_src,
        &CompileOptions {
            stop_after: Stage::Build,
            optimization: Optimization::O2,
            backend: NativeBackend::Clang,
            output: Some(zil_exe.clone()),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "Win64 argv dumper build failed: {:?}",
        report.diagnostics
    );

    // 2. Build reference MSVC CRT wmain dumper using the exact same Windows clang toolchain.
    let c_src = dir.join("crt_wmain_dumper.c");
    std::fs::write(
        &c_src,
        r#"
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <io.h>
#include <fcntl.h>

int wmain(int argc, wchar_t **argv) {
    _setmode(_fileno(stdout), _O_BINARY);
    for (int i = 1; i < argc; i++) {
        int needed = WideCharToMultiByte(CP_UTF8, 0, argv[i], -1, NULL, 0, NULL, NULL);
        if (needed <= 0) return 10;
        char *buf = (char *)malloc((size_t)needed);
        if (!buf) return 11;
        int written = WideCharToMultiByte(CP_UTF8, 0, argv[i], -1, buf, needed, NULL, NULL);
        if (written <= 0) { free(buf); return 12; }
        int utf8_len = written - 1;
        printf("%d:", utf8_len);
        if (utf8_len > 0) fwrite(buf, 1, (size_t)utf8_len, stdout);
        fputc('\n', stdout);
        free(buf);
    }
    fflush(stdout);
    return 0;
}
"#,
    )
    .expect("write crt_wmain_dumper.c");
    let crt_exe = dir.join("crt_wmain_dumper.exe");
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    let cc = Command::new(&clang)
        .arg(&c_src)
        .arg("-o")
        .arg(&crt_exe)
        .output()
        .expect("compile crt_wmain_dumper.c");
    assert!(
        cc.status.success(),
        "failed to compile CRT reference dumper: stdout={} stderr={}",
        String::from_utf8_lossy(&cc.stdout),
        String::from_utf8_lossy(&cc.stderr)
    );

    // 3. Differential matrix of raw Win32 command lines (quotes, backslashes, empty args,
    //    escaped quotes, doubled quotes inside quotes, tabs, CJK, and emoji surrogate pairs).
    let raw_cases = [
        r#"plain "hello world" "" tail"#,
        r#"a\"b a\\"b a\\\"b a\\\\"b"#,
        r#""" "" "a""b" """a b""" """" """"" end"#,
        r#""trailing\\" "trailing\\\"quote" \"\" \"a b\""#,
        "\"a b\"c\t\"d\te\"  \"\"x\"\"   \\\\\\notquote",
        "中文 \"带 引 号\" \"emoji 🚀 \"\"quoted\"\"\" café",
    ];

    for raw in raw_cases {
        let zil_out = Command::new(&zil_exe)
            .raw_arg(raw)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("run zil_argv_dumper failed: {e}"));
        let crt_out = Command::new(&crt_exe)
            .raw_arg(raw)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("run crt_wmain_dumper failed: {e}"));
        assert_eq!(
            zil_out.status.code(),
            Some(0),
            "zil dumper failed on raw={raw:?}: stderr={}",
            String::from_utf8_lossy(&zil_out.stderr)
        );
        assert_eq!(
            crt_out.status.code(),
            Some(0),
            "crt dumper failed on raw={raw:?}: stderr={}",
            String::from_utf8_lossy(&crt_out.stderr)
        );
        assert_eq!(
            zil_out.stdout,
            crt_out.stdout,
            "ZIL std.env.args() diverged from MSVC CRT wmain on raw={raw:?}\nZIL:\n{}\nCRT:\n{}",
            String::from_utf8_lossy(&zil_out.stdout),
            String::from_utf8_lossy(&crt_out.stdout)
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}
