//! Real resource lifetimes across source modules and native optimization levels.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::ResultCategory;
use zil_driver::{CompileOptions, Stage, compile_path};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-resource-mainline-{}-{}",
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
            panic!("resource application timed out: {}", path.display());
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
fn resources_match_independent_rust_at_o0_and_o2() {
    let temp = Temp::new();
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/resources");
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
    assert_eq!(report.source_files.len(), 3);
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("resource.live."));
    assert!(llvm.contains("resource.drop."));
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
        for input in [b"".as_slice(), b"x", b"\0", b"\xff"] {
            run(binary, input, 0, "EICFG|HA");
        }
        run(binary, b"!", 2, "EICA");
    }
}

#[test]
fn real_linux_descriptor_is_closed_after_transfer_and_scope_exit() {
    let temp = Temp::new();
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/resources");
    let report = compile_path(
        &app.join("fd.zil"),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("zil-fd-O0")),
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
    let mut binaries = vec![report.artifacts["executable"].clone()];
    let optimized = temp.0.join("zil-fd-O2");
    build(
        Command::new("clang")
            .args(["-x", "ir", "-O2"])
            .arg(&report.artifacts["llvm"])
            .arg("-o")
            .arg(&optimized),
    );
    binaries.push(optimized);
    for level in ["0", "2"] {
        let binary = temp.0.join(format!("rust-fd-O{level}"));
        build(
            Command::new("rustc")
                .arg(app.join("fd.rs"))
                .args([
                    "--edition=2024",
                    "-C",
                    &format!("opt-level={level}"),
                    "-D",
                    "warnings",
                    "-o",
                ])
                .arg(&binary),
        );
        binaries.push(binary);
    }
    for binary in &binaries {
        run(binary, b"", 0, "F");
    }
}
