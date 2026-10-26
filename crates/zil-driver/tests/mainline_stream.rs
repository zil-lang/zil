//! Dynamic input across generic modules, mutable captures and an explicit C ABI.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_path, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-mainline-stream-{}-{}",
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
    // Inputs fit in a pipe; outputs are deliberately tiny. A bad generated
    // program that loops or fills stdout is killed by the same deadline.
    assert!(input.len() < 4096);
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
            panic!("stream application timed out: {}", path.display());
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
fn dynamic_stream_matches_independent_rust_at_o0_and_o2() {
    let temp = Temp::new();
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/stream");
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
    assert!(report.verification_runs > 0, "{}", report.to_json());
    assert_eq!(report.source_files.len(), 5, "{}", report.to_json());
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("@getchar("));
    assert!(llvm.contains("@putchar("));
    assert!(!llvm.contains("@malloc("));

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
    let mut cases: Vec<(Vec<u8>, i32, &str)> = vec![
        (b"".to_vec(), 0, "0 0 0\n"),
        (b" \t\n\r\x0b\x0c".to_vec(), 0, "0 0 0\n"),
        (b"10 20 -3\n".to_vec(), 0, "3 0 45\n"),
        (b"1 broken 2\t-4 +5\n".to_vec(), 0, "4 1 23\n"),
        (
            b"1000000 -1000000 1000001 -1000001 2147483647 -2147483648 000001 +0 -0 + - 9x"
                .to_vec(),
            0,
            "5 7 1000024\n",
        ),
        (b"\0 3 \xff 4".to_vec(), 0, "2 2 21\n"),
        (b"\t\r\n\x0b\x0c 7\t8\r\n".to_vec(), 0, "2 0 33\n"),
        (
            b"99999999999999999999999999 00000000000000000000000001".to_vec(),
            0,
            "1 1 2\n",
        ),
    ];
    cases.push((b"1 ".repeat(1000), 0, "1000 0 7000\n"));
    cases.push((b"1 ".repeat(1001), 2, ""));
    for binary in &binaries {
        for (input, code, expected) in &cases {
            run(binary, input, *code, expected);
        }
    }
}

#[test]
fn generic_payload_inference_projects_one_address_layer_without_erasing_types() {
    for source in [
        "enum Value<T>{Empty,Item(T)} struct Row{value:i64;} fn result()->i64{let row=Row{value:5000000042};let p=&row;let item=Value.Item(p.value);if let Value.Item(n)=item{return n}return 0}",
        "enum Value<T>{Empty,Item(T)} fn result()->i64{var row:(i64,i64)=(1,42);let p=&mut row;let item=Value.Item(p.1);if let Value.Item(n)=item{return n}return 0}",
        "enum Value<T>{Empty,Item(T)} fn result()->i64{var row:[i64;2]=[1,42];let p=&mut row;let item=Value.Item(p[1]);if let Value.Item(n)=item{return n}return 0}",
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "projection.zil", source),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.verification_runs > 0);
    }
    let report = compile_source(
        &SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "projection-mismatch.zil",
            "enum Value<T>{Empty,Item(T)} struct Row{value:bool;} fn result()->i64{let row=Row{value:true};let p=&row;let item:Value<i64>=Value.Item(p.value);return 0}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.diagnostics[0].code,
        "E-GENERIC-ARGUMENT",
        "{}",
        report.to_json()
    );
}

#[test]
fn mixed_external_capture_uses_each_roots_parent_and_still_rejects_conflicts() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "mixed-external.zil",
        "fn answer()->i32{var x=1;let y=2;var f=()=>{x=x+y;x};let a=f();let b=f();return a+b+x+y}",
    );
    let temp = Temp::new();
    let report = compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("answer".into()),
            output: Some(temp.0.join("mixed-O0")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    run(&report.artifacts["executable"], b"", 0, "15\n");
    let optimized = temp.0.join("mixed-O2");
    build(
        Command::new("clang")
            .args(["-x", "ir", "-O2"])
            .arg(&report.artifacts["llvm"])
            .arg("-o")
            .arg(&optimized),
    );
    run(&optimized, b"", 0, "15\n");
    for source in [
        "fn answer()->i32{var x=1;let y=2;var f=()=>{x=x+y;x};x=10;return f()}",
        "fn answer()->i32{var x=1;var y=2;var f=()=>{x=x+y;x};y=10;return f()}",
        "fn answer()->i32{var x=1;let y=2;let p=&x;var f=()=>{x=x+y;x};let a=f();return a+*p}",
    ] {
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "mixed-conflict.zil", source),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.diagnostics[0].code,
            "E-BORROW-CONFLICT",
            "{}",
            report.to_json()
        );
    }
}
