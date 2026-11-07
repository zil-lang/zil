//! Real package source paths, native cleanup, and an independent Rust oracle.
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
            "zil-package-ui-{}-{}",
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
            panic!("package application timed out: {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "{} input {input:?}: {output:?}",
        path.display()
    );
    assert_eq!(
        output.stdout,
        expected.as_bytes(),
        "{} input {input:?}",
        path.display()
    );
    assert!(output.stderr.is_empty(), "{}: {output:?}", path.display());
}
fn binaries(temp: &Path, stem: &str, source_files: usize) -> Vec<PathBuf> {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/package_ui");
    let report = compile_path(
        &app.join(format!("{stem}.zil")),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.join(format!("{stem}-zil-O0"))),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{stem}: {}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
    assert_eq!(report.source_files.len(), source_files);
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("package.drop."));
    assert!(!llvm.contains("@malloc("));
    assert!(!llvm.contains(" noalias "));
    let mut binaries = vec![report.artifacts["executable"].clone()];
    let optimized = temp.join(format!("{stem}-zil-O2"));
    build(
        Command::new("clang")
            .args(["-x", "ir", "-O2"])
            .arg(&report.artifacts["llvm"])
            .arg("-o")
            .arg(&optimized),
    );
    binaries.push(optimized);
    for level in ["0", "2"] {
        let binary = temp.join(format!("{stem}-rust-O{level}"));
        build(
            Command::new("rustc")
                .arg(app.join(format!("{stem}.rs")))
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
    binaries
}

#[test]
fn bounded_ui_events_move_complete_packages_without_losing_field_descriptions() {
    let temp = Temp::new();
    let mut cases: Vec<(Vec<u8>, i32, String)> = [
        ("", 0, "0\n#1\nMSU"),
        ("q", 0, "0\n#1\nMSU"),
        ("++q", 0, "0\n1\n2\n#3\nMSU"),
        ("--+r=q", 0, "0\n-1\n-2\n-1\n-1\n#5\nMSU"),
        ("= =q", 0, "0\n#1\nMSU"),
        ("x", 1, "0\nMSU"),
        ("+x", 1, "0\n1\nMSU"),
        (" \n\r+q", 0, "0\n1\n#2\nMSU"),
    ]
    .into_iter()
    .map(|(input, code, output)| (input.as_bytes().to_vec(), code, output.into()))
    .collect();
    cases.push((vec![b'='; 128], 1, "0\nMSU".into()));
    let mut saturated = vec![b'+'; 52];
    saturated.push(b'q');
    let expected = (0..=50)
        .map(|value| format!("{value}\n"))
        .collect::<String>()
        + "#51\nMSU";
    cases.push((saturated, 0, expected));
    for binary in binaries(&temp.0, "main", 4) {
        for (input, code, output) in &cases {
            run(&binary, input, *code, output);
        }
    }
}

#[test]
fn partial_preparation_and_complete_package_cleanup_have_distinct_native_orders() {
    let temp = Temp::new();
    for binary in binaries(&temp.0, "lifecycle", 3) {
        for (input, code, output) in [
            (b"0", 1, "Z"),
            (b"1", 1, "CZ"),
            (b"2", 1, "ACZ"),
            (b"3", 1, "BACZ"),
            (b"4", 1, "ABCZ"),
            (b"5", 0, "XABCZ"),
            (b"l", 0, "ABCABC"),
        ] {
            run(&binary, input, code, output);
        }
    }
}

#[test]
fn whole_package_call_return_closes_both_real_native_descriptors() {
    let temp = Temp::new();
    for binary in binaries(&temp.0, "fd", 1) {
        run(&binary, b"", 0, "F");
    }
}
