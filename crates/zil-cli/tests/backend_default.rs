//! Exercise the real CLI selection, not the library's compatibility default.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-209-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn build_and_run(backend: Option<&str>) {
    let dir = Scratch::new();
    let source = dir.0.join("main.zil");
    fs::write(&source, "fn main() -> i32 { return 17; }\n").unwrap();
    let expected = backend.unwrap_or(if cfg!(feature = "llvm-api") {
        "llvm-api"
    } else {
        "clang"
    });
    for optimization in ["O0", "O2"] {
        let output = dir.0.join(optimization);
        let mut command = Command::new(env!("CARGO_BIN_EXE_zil"));
        command
            .arg("build")
            .arg(&source)
            .arg("-o")
            .arg(&output)
            .args(["--opt", optimization, "--json"]);
        if let Some(backend) = backend {
            command.args(["--backend", backend]);
        }
        let result = command.output().unwrap();
        assert!(result.status.success(), "{result:?}");
        let report = String::from_utf8(result.stdout).unwrap();
        assert!(
            report.contains(&format!("\"backend\":\"{expected}\"")),
            "{report}"
        );
        assert!(report.contains("\"verification_runs\":1"), "{report}");
        let actual = Command::new(output).output().unwrap();
        assert_eq!(actual.status.code(), Some(17), "{actual:?}");
        assert!(actual.stdout.is_empty() && actual.stderr.is_empty());
    }
}
#[test]
fn ordinary_cli_build_uses_the_compiled_default() {
    build_and_run(None);
}
#[test]
fn explicit_clang_remains_a_real_reference_path() {
    build_and_run(Some("clang"));
}
#[test]
fn version_names_the_actual_default() {
    let output = Command::new(env!("CARGO_BIN_EXE_zil"))
        .arg("version")
        .output()
        .unwrap();
    assert!(output.status.success());
    let name = if cfg!(feature = "llvm-api") {
        "llvm-api"
    } else {
        "clang"
    };
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains(&format!("default backend: {name}"))
    );
}
#[cfg(not(feature = "llvm-api"))]
#[test]
fn reference_only_build_rejects_api_without_overwriting_an_old_output() {
    let dir = Scratch::new();
    let source = dir.0.join("main.zil");
    let output = dir.0.join("program");
    fs::write(&source, "fn main() -> i32 { return 17; }\n").unwrap();
    fs::write(&output, b"existing-output").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_zil"))
        .arg("build")
        .arg(source)
        .arg("-o")
        .arg(&output)
        .args(["--backend", "llvm-api", "--json"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8(result.stdout)
            .unwrap()
            .contains("E-LLVM-API-UNAVAILABLE")
    );
    assert_eq!(fs::read(output).unwrap(), b"existing-output");
}

#[test]
fn default_backend_runs_the_current_state_consumer_with_ascii_input() {
    use std::{io::Write, process::Stdio};
    let dir = Scratch::new();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/mainline/zui_language177/main.zil");
    let executable = dir.0.join("state");
    let build = Command::new(env!("CARGO_BIN_EXE_zil"))
        .arg("build")
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .arg("--json")
        .output()
        .unwrap();
    assert!(build.status.success(), "{build:?}");
    for (input, expected, code) in [
        (b"3".as_slice(), b"DEFHZMNOPK".as_slice(), 0),
        (b"\x03".as_slice(), b"".as_slice(), 9),
    ] {
        let mut child = Command::new(&executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let run = child.wait_with_output().unwrap();
        assert_eq!(run.status.code(), Some(code), "{run:?}");
        assert_eq!(run.stdout, expected);
        assert!(run.stderr.is_empty());
    }
}
