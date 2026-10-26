//! Returned owned environments across module boundaries, native cleanup and an independent Rust oracle.
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
struct Temp(PathBuf, bool);
impl Temp {
    fn new() -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR").is_some();
        let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&base).unwrap();
        let path = base.join(format!(
            "zil-retained-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, keep)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn build(command: &mut Command, record: &Path) {
    let output = command.output().unwrap();
    fs::write(record.with_extension("command"), format!("{command:?}\n")).unwrap();
    fs::write(record.with_extension("stdout"), &output.stdout).unwrap();
    fs::write(record.with_extension("stderr"), &output.stderr).unwrap();
    fs::write(
        record.with_extension("exit"),
        format!("{:?}\n", output.status.code()),
    )
    .unwrap();
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
    if std::env::var_os("ZIL_TEST_REPORT_DIR").is_some() {
        let record = path.with_file_name(format!(
            "run-{}-{}",
            path.file_name().unwrap().to_string_lossy(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(record.with_extension("stdin"), input).unwrap();
        fs::write(record.with_extension("stdout"), &output.stdout).unwrap();
        fs::write(record.with_extension("stderr"), &output.stderr).unwrap();
        fs::write(
            record.with_extension("exit"),
            format!("{:?}\n", output.status.code()),
        )
        .unwrap();
    }
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
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/retained_ui");
    let report = compile_path(
        &app.join(format!("{stem}.zil")),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.join(format!("{stem}-zil-O0"))),
            emit_dir: Some(temp.join(format!("{stem}-ir"))),
            ..CompileOptions::default()
        },
    );
    fs::write(
        temp.join(format!("{stem}-compile-report.json")),
        report.to_json(),
    )
    .unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{stem}: {}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
    assert_eq!(report.source_files.len(), source_files);
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    let contracts = report
        .scalar_interfaces
        .iter()
        .flat_map(|interface| interface.callable_returns.as_ref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        contracts.len(),
        5,
        "all public factory identities must be checked"
    );
    assert!(contracts.iter().all(|entry| entry.contract.hold.is_empty()
        && entry.contract.call.is_empty()
        && entry.contract.writes.is_empty()));
    assert!(
        contracts
            .iter()
            .any(|entry| entry.contract.cleanup.len() == 2)
    );
    assert!(
        contracts
            .iter()
            .any(|entry| entry.contract.cleanup.is_empty())
    );
    assert!(llvm.contains("ptr %v."));
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
        &temp.join(format!("build-{stem}-zil-O2")),
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
            &temp.join(format!("build-{stem}-rust-O{level}")),
        );
        binaries.push(binary);
    }
    binaries
}

#[test]
fn retained070_native_factories_survive_return_move_events_and_cleanup() {
    let temp = Temp::new();
    let mut cases: Vec<(Vec<u8>, i32, String)> = [
        ("s", 0, "2121B"),
        ("z", 0, "ZB"),
        ("m", 0, "F23A"),
        ("0", 0, "CD0"),
        ("1", 0, "CD1"),
        ("p", 1, "BCDZ"),
        ("P", 0, "HCDB3Z"),
        ("e", 1, "1E"),
        ("f", 1, "12E"),
        ("E", 0, "12:3:33E"),
        ("c", 0, "42"),
        ("g", 0, "77G"),
        ("x", 1, ""),
        ("l", 0, "F0\nA"),
        ("lq", 0, "F0\nA"),
        ("l++-q", 0, "F0\n1\n2\n1\nA"),
        ("l--=q", 0, "F0\n-1\n-2\n-2\nA"),
        ("l+ x", 1, "F0\n1\nA"),
        ("l \n+q", 0, "F0\n1\nA"),
    ]
    .into_iter()
    .map(|(input, code, output)| (input.as_bytes().to_vec(), code, output.into()))
    .collect();
    let mut input = vec![b'l'];
    input.extend(vec![b'='; 32]);
    cases.push((input, 1, "F".to_owned() + &"0\n".repeat(33) + "A"));
    for binary in binaries(&temp.0, "main", 5) {
        for (input, code, output) in &cases {
            run(&binary, input, *code, output);
        }
    }
}
