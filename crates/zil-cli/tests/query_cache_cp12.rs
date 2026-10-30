#![cfg(target_os = "linux")]
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cp12-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("main.zil"), "fn main()->i32=>42").unwrap();
        Self(path)
    }
    fn command(&self, binary: impl AsRef<std::ffi::OsStr>, backend: &str) -> Command {
        let mut command = Command::new(binary);
        command
            .arg("build")
            .arg(self.0.join("main.zil"))
            .arg("-o")
            .arg(self.0.join("program"))
            .args(["--backend", backend, "--json", "--query-cache"])
            .arg(self.0.join("queries"));
        command
    }
    fn check(&self, mut command: Command, state: &str) -> String {
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let json = String::from_utf8(output.stdout).unwrap();
        assert!(
            json.contains(&format!(
                "\"query_cache\":{{\"requested\":true,\"state\":\"{state}\""
            )),
            "{json}"
        );
        assert!(json.contains("\"verification_runs\":1"), "{json}");
        assert_eq!(
            Command::new(self.0.join("program"))
                .status()
                .unwrap()
                .code(),
            Some(42)
        );
        json
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("CP12 CLI failure files retained at {}", self.0.display());
        } else {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn separate_cli_processes_hit_but_reinstalled_producer_misses() {
    let case = Case::new();
    let binary = env!("CARGO_BIN_EXE_zil");
    case.check(case.command(binary, "clang"), "miss");
    case.check(case.command(binary, "clang"), "hit");
    // Same ELF bytes/build ID in a distinct installed file still change the
    // installed-tool stat identity; this is not an adversarial-write claim.
    let installed = case.0.join("reinstalled-zil");
    fs::copy(binary, &installed).unwrap();
    case.check(case.command(&installed, "clang"), "miss");
    case.check(case.command(&installed, "clang"), "hit");
    let entries = fs::read_dir(case.0.join("queries/frontend-cp12-v1"))
        .unwrap()
        .count();
    assert_eq!(entries, 2);
}

#[test]
fn help_default_disabled_missing_value_and_interposition_are_explicit() {
    let binary = env!("CARGO_BIN_EXE_zil");
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("--query-cache DIR")
    );
    let case = Case::new();
    let missing = Command::new(binary)
        .arg("build")
        .arg(case.0.join("main.zil"))
        .args(["--json", "--query-cache"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(
        String::from_utf8(missing.stdout)
            .unwrap()
            .contains("option requires a value")
    );
    assert!(!case.0.join("queries").exists());
    let original = Command::new(binary)
        .arg("build")
        .arg(case.0.join("main.zil"))
        .arg("-o")
        .arg(case.0.join("program"))
        .args(["--backend", "clang", "--json"])
        .output()
        .unwrap();
    assert!(original.status.success(), "{original:?}");
    assert!(
        String::from_utf8(original.stdout)
            .unwrap()
            .contains("\"query_cache\":{\"requested\":false,\"state\":\"disabled\"")
    );
    assert!(!case.0.join("queries").exists());
    let mut interposed = case.command(binary, "clang");
    interposed.env("LD_PRELOAD", "/nonexistent/zil-cp12-test.so");
    let report = case.check(interposed, "bypass");
    assert!(report.contains("dynamic-loader interposition"));
    assert!(!case.0.join("queries").exists());
    let run = Command::new(binary)
        .arg("run")
        .arg(case.0.join("main.zil"))
        .arg("-o")
        .arg(case.0.join("program"))
        .args(["--backend", "clang", "--json", "--query-cache"])
        .arg(case.0.join("queries"))
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(42));
    let run_json = String::from_utf8(run.stdout).unwrap();
    assert!(
        run_json.contains("\"query_cache\":{\"requested\":true,\"state\":\"bypass\""),
        "{run_json}"
    );
    assert!(run_json.contains("run uses the original pipeline"));
    assert!(run_json.contains("\"verification_runs\":1"));
    assert!(!case.0.join("queries").exists());
}

#[test]
#[cfg(feature = "llvm-api")]
fn llvm_api_objects_and_frontend_checkpoint_have_separate_hits_and_one_verify() {
    let case = Case::new();
    let binary = env!("CARGO_BIN_EXE_zil");
    for (state, object_hits) in [("miss", 0), ("hit", 1)] {
        let mut command = case.command(binary, "llvm-api");
        command.arg("--object-cache").arg(case.0.join("objects"));
        let report = case.check(command, state);
        assert!(
            report.contains(&format!("\"hits\":{object_hits}")),
            "{report}"
        );
        assert!(report.contains("\"disk_cache\":\"disabled\""), "{report}");
    }
}
