//! A real multi-module application and its independently compiled Rust behavior oracle.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::ResultCategory;
use zil_driver::{CompileOptions, Stage, compile_path};

const EXPECTED: &str = "475000738663\n";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-mainline-app-{}-{}",
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
fn run(path: &Path, args: &[&str], expected: &str) {
    let mut child = Command::new(path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native application timed out: {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}: {output:?}", path.display());
    assert_eq!(output.stdout, expected.as_bytes(), "{}", path.display());
    assert!(output.stderr.is_empty(), "{}: {output:?}", path.display());
}

#[test]
fn mainline_application_matches_rust_and_self_checks_at_o0_and_o2() {
    let directory = Temp::new();
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/app");
    let input = app.join("main.zil");
    for (entry, mode, expected) in [(Some("result"), "result", EXPECTED), (None, "main", "")] {
        let report = compile_path(
            &input,
            &CompileOptions {
                stop_after: Stage::Build,
                entry: entry.map(str::to_owned),
                output: Some(directory.0.join(format!("zil-{mode}-O0"))),
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
        assert_eq!(report.source_files.len(), 3, "{}", report.to_json());
        assert!(
            report
                .source_files
                .values()
                .any(|p| p.ends_with("seed/data/mod.zil"))
        );
        run(&report.artifacts["executable"], &[], expected);
        let optimized = directory.0.join(format!("zil-{mode}-O2"));
        build(
            Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&optimized),
        );
        run(&optimized, &[], expected);
    }
    for level in ["0", "2"] {
        let executable = directory.0.join(format!("rust-O{level}"));
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
                .arg(&executable),
        );
        run(&executable, &[], EXPECTED);
        run(&executable, &["--check"], "");
    }
}
