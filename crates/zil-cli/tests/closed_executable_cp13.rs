#![cfg(all(target_os = "linux", feature = "llvm-api"))]
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
            "cp13-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(
            path.join("main.zil"),
            "fn helper()->i32=>21\nfn main()->i32=>helper()+helper()\n",
        )
        .unwrap();
        Self(path)
    }
    fn command(&self, action: &str, backend: &str, closed: bool) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_zil"));
        command
            .arg(action)
            .arg(self.0.join("main.zil"))
            .arg("-o")
            .arg(self.0.join("program"))
            .args(["--backend", backend, "--json"]);
        if closed {
            command.arg("--closed-executable");
        }
        command
    }
    fn build(&self, mut command: Command, closed: bool) -> String {
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let report = String::from_utf8(output.stdout).unwrap();
        assert!(report.contains("\"verification_runs\":1"), "{report}");
        assert!(
            report.contains(&format!("\"closed_executable\":{closed}")),
            "{report}"
        );
        let count: usize = report
            .split("\"internalized_functions\":")
            .nth(1)
            .expect("actual count field")
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap();
        if closed {
            assert!(count > 0, "{report}");
        } else {
            assert_eq!(count, 0);
        }
        assert_eq!(
            Command::new(self.0.join("program"))
                .status()
                .unwrap()
                .code(),
            Some(42)
        );
        report
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("CP13 CLI failure retained at {}", self.0.display());
        } else {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn default_false_and_explicit_flag_report_actual_linkage_for_both_backends() {
    let case = Case::new();
    for backend in ["llvm-api", "clang"] {
        case.build(case.command("build", backend, false), false);
        case.build(case.command("build", backend, true), true);
    }
    let output = Command::new(env!("CARGO_BIN_EXE_zil"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--closed-executable"));
    assert!(help.contains("generated ZIL symbols have no export contract"));
}

#[test]
fn check_run_and_win64_reject_the_explicit_build_only_flag() {
    let case = Case::new();
    for action in ["check", "run", "build"] {
        let mut command = case.command(action, "llvm-api", true);
        if action == "build" {
            command.args(["--target", "win64"]);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let report = String::from_utf8(output.stdout).unwrap();
        assert!(report.contains("E-CLI-ARGUMENT"), "{report}");
        assert!(report.contains("complete Linux x86_64 build"), "{report}");
        assert!(!case.0.join("program").exists());
    }
}

#[test]
fn post_verify_mode_toggle_reuses_raw_checkpoint_but_invalidates_machine_objects() {
    let case = Case::new();
    for (index, closed, state, hits) in [
        (0, false, "miss", 0),
        (1, true, "hit", 0),
        (2, true, "hit", 1),
    ] {
        let mut command = case.command("build", "llvm-api", closed);
        command
            .arg("--query-cache")
            .arg(case.0.join("queries"))
            .arg("--object-cache")
            .arg(case.0.join("objects"));
        let report = case.build(command, closed);
        assert!(
            report.contains(&format!(
                "\"query_cache\":{{\"requested\":true,\"state\":\"{state}\""
            )),
            "step {index}: {report}"
        );
        assert!(
            report.contains(&format!("\"hits\":{hits}")),
            "step {index}: {report}"
        );
    }
    assert_eq!(
        fs::read_dir(case.0.join("queries/frontend-cp12-v1"))
            .unwrap()
            .count(),
        1
    );
}
