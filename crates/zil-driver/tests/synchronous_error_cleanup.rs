//! Source synchronous errors compared with an independent Rust lifecycle oracle.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_path, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-error-cleanup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn build(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn run(path: &Path, input: &[u8], expected_code: i32, expected: &str) {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("error application timed out: {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "{}: {output:?}",
        path.display()
    );
    assert_eq!(output.stdout, expected.as_bytes(), "{}", path.display());
    assert!(output.stderr.is_empty(), "{}: {output:?}", path.display());
}

#[test]
fn synchronous_errors_and_cleanup_match_rust_at_o0_and_o2() {
    let temp = Temp::new();
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/error_cleanup");
    let report = compile_path(
        &app.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("zil-O0")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
    assert_eq!(report.source_files.len(), 4);
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("@zil.closure.drop."));
    assert!(llvm.contains("%closure.once."));
    assert!(!llvm.contains("@malloc("));
    assert!(!llvm.contains(" noalias "));
    let mut binaries = vec![report.artifacts["executable"].clone()];
    let optimized = temp.0.join("zil-O2");
    build(
        Command::new("clang")
            .args(["-x", "ir", "-O2"])
            .arg(&report.artifacts["llvm"])
            .arg("-o")
            .arg(&optimized),
    );
    binaries.push(optimized);
    for level in ["0", "2"] {
        let binary = temp.0.join(format!("rust-O{level}"));
        build(
            Command::new("rustc")
                .arg(app.join("main.rs"))
                .args([
                    "--edition=2024",
                    "-C",
                    &format!("opt-level={level}"),
                    "-C",
                    "overflow-checks=yes",
                    "-D",
                    "warnings",
                    "-o",
                ])
                .arg(&binary),
        );
        binaries.push(binary);
    }
    for binary in &binaries {
        for (input, code, output) in [
            (b"0", 1, "A"),
            (b"1", 1, "BA"),
            (b"2", 1, "CBA"),
            (b"3", 7, "CBA"),
            (b"A", 1, "vBA"),
            (b"a", 0, "vXBA"),
            (b"F", 1, "vBA"),
            (b"f", 0, "vXBA"),
            (b"P", 1, "vA"),
            (b"p", 0, "vBA"),
            (b"C", 1, "BA"),
            (b"c", 0, "XBA"),
            (b"L", 1, "BCA"),
            (b"l", 0, "BCDA"),
            (b"?", 1, ""),
        ] {
            run(binary, input, code, output);
        }
    }
}

#[test]
fn source_error_boundaries_are_rejected_without_fabricated_success() {
    for (name, source, code) in [
        (
            "plain-question",
            "fn bad()->bool!i32{return 1?}",
            "E-TRY-NOT-ERROR",
        ),
        (
            "outside-error",
            "fn f()->bool!i32{return true} fn bad()->i32{return f()?}",
            "E-TRY-RETURN-TYPE",
        ),
        (
            "different-error",
            "fn f()->bool!i32{return true} fn bad()->i64!i32{return f()?}",
            "E-TRY-ERROR-TYPE",
        ),
        (
            "ambiguous-injection",
            "fn bad()->i32!i32{return 1}",
            "E-ERROR-INJECTION-AMBIGUOUS",
        ),
        (
            "unrelated-payload",
            "fn bad()->bool!i32{let value:i64=3;return value}",
            "E-ERROR-INJECTION",
        ),
        (
            "error-is-not-success",
            "fn f()->bool!i32{return true} fn bad()->i32{return f()}",
            "E-TYPE-MISMATCH",
        ),
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), name, source),
            &CompileOptions::default(),
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.diagnostics[0].phase, Phase::Types, "{name}");
        assert_eq!(report.diagnostics[0].code, code, "{name}");
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn source_nested_copy_errors_and_void_entry_execute_without_unwind() {
    for (name, source, code) in [
        ("unit-ok", "fn main()->bool!void{return ()}", 0),
        ("unit-error", "fn main()->bool!void{return true}", 1),
        (
            "aggregate-result",
            "struct Pair{x:i32;y:bool;} fn f()->bool!Pair{return Pair{x:42,y:true}} fn main()->bool!i32{let p=f()?;return p.x}",
            42,
        ),
        (
            "aggregate-error",
            "struct Fault{x:i32;} fn f()->Fault!i32{return Fault{x:9}} fn main()->Fault!i32{return f()?}",
            1,
        ),
        (
            "function-pointer",
            "fn f()->bool!i32{return 42} fn main()->bool!i32{let g:fn()->bool!i32=f;return g()?}",
            42,
        ),
    ] {
        let temp = Temp::new();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), name, source),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(temp.0.join(name)),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        run(&report.artifacts["executable"], b"", code, "");
    }
}

#[test]
fn runtime_try_does_not_hide_comptime_work_or_execute_runtime_errors_at_ct() {
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "ct-under-try",
            "fn f(v:i32)->bool!i32{return v} fn main()->bool!i32{return f(#(21+21))?}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "try-at-ct",
            "fn f()->bool!i32{return 42} fn main()->i32{return #(f()?)}",
        ),
        &CompileOptions::default(),
    );
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
