//! Project imports and tests use the real CLI, filesystem sources, verifier and native runner.
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
            "zil-project-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_zil"))
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn declared_path_dependency_and_test_results_are_real() {
    let dir = Scratch::new();
    let app = dir.0.join("app");
    let math = dir.0.join("math");
    fs::create_dir(&app).unwrap();
    fs::create_dir(&math).unwrap();
    fs::write(app.join("Zil.toml"), "[package]\nroot = \"main.zil\"\ntests = [\"tests.zil\"]\n[dependencies]\nmath = \"../math\"\n").unwrap();
    fs::write(
        math.join("Zil.toml"),
        "[package]\nroot = \"arithmetic.zil\"\n",
    )
    .unwrap();
    fs::write(
        math.join("arithmetic.zil"),
        "pub fn answer() -> i32 => 42\n",
    )
    .unwrap();
    fs::write(app.join("main.zil"), "import math.arithmetic as m\nfn main() -> i32 => m.answer()\n#[test]\nfn answer_is_42() -> bool => m.answer() == 42\n").unwrap();
    fs::write(
        app.join("tests.zil"),
        "#[test]\nfn unit_passes() {}\n#[test]\nfn integer_passes() -> i32 => 0\n",
    )
    .unwrap();
    let path = app.to_str().unwrap();
    let lock = dir.run(&["lock", path]);
    assert!(lock.status.success(), "{lock:?}");
    let check = dir.run(&["check", path, "--json"]);
    assert!(check.status.success(), "{check:?}");
    let text = String::from_utf8(check.stdout).unwrap();
    assert!(text.contains("\"verification_runs\":1"), "{text}");
    assert!(text.contains("arithmetic.zil"), "{text}");
    let test = dir.run(&["test", path, "--backend", "clang", "--json"]);
    assert!(test.status.success(), "{test:?}");
    let text = String::from_utf8(test.stdout).unwrap();
    assert!(text.contains("\"status\":\"Pass\""), "{text}");
    assert!(text.contains("\"passed\":3"), "{text}");
    fs::write(
        app.join("tests.zil"),
        "#[test]\nfn boolean_fails() -> bool => false\n#[test]\nfn integer_fails() -> i32 => 7\n",
    )
    .unwrap();
    let stale = dir.run(&["test", path, "--backend", "clang", "--json"]);
    assert!(!stale.status.success());
    assert!(
        String::from_utf8(stale.stdout)
            .unwrap()
            .contains("E-PROJECT-LOCK-MISMATCH")
    );
    assert!(dir.run(&["lock", path]).status.success());
    let failed = dir.run(&["test", path, "--backend", "clang", "--json"]);
    assert!(!failed.status.success(), "{failed:?}");
    let text = String::from_utf8(failed.stdout).unwrap();
    assert!(text.contains("\"outcome\":\"exit:1\""), "{text}");
    assert!(text.contains("\"outcome\":\"exit:7\""), "{text}");
}

#[test]
fn undeclared_dependency_and_cycle_are_rejected() {
    let dir = Scratch::new();
    let app = dir.0.join("app");
    let dep = dir.0.join("dep");
    fs::create_dir(&app).unwrap();
    fs::create_dir(&dep).unwrap();
    fs::write(app.join("Zil.toml"), "[package]\nroot = \"main.zil\"\n").unwrap();
    fs::write(
        app.join("main.zil"),
        "import dep.helper\nfn main() -> i32 => 0\n",
    )
    .unwrap();
    fs::write(
        dep.join("Zil.toml"),
        "[package]\nroot = \"helper.zil\"\n[dependencies]\napp = \"../app\"\n",
    )
    .unwrap();
    fs::write(dep.join("helper.zil"), "pub fn answer() -> i32 => 1\n").unwrap();
    let path = app.to_str().unwrap();
    assert!(dir.run(&["lock", path]).status.success());
    let blocked = dir.run(&["check", path, "--json"]);
    assert!(!blocked.status.success(), "{blocked:?}");
    assert!(
        String::from_utf8(blocked.stdout)
            .unwrap()
            .contains("E-MODULE-NOT-FOUND")
    );
    fs::write(
        app.join("Zil.toml"),
        "[package]\nroot = \"main.zil\"\n[dependencies]\ndep = \"../dep\"\n",
    )
    .unwrap();
    let cycle = dir.run(&["lock", path]);
    assert!(!cycle.status.success(), "{cycle:?}");
    assert!(
        String::from_utf8(cycle.stderr)
            .unwrap()
            .contains("E-PROJECT-CYCLE")
    );
}

#[test]
fn empty_invalid_and_timed_out_tests_never_pass() {
    let dir = Scratch::new();
    fs::write(dir.0.join("Zil.toml"), "[package]\nroot = \"main.zil\"\n").unwrap();
    let source = dir.0.join("main.zil");
    let path = dir.0.to_str().unwrap();
    fs::write(&source, "fn main() -> i32 => 0\n").unwrap();
    assert!(dir.run(&["lock", path]).status.success());
    let empty = dir.run(&["test", path, "--json"]);
    assert!(!empty.status.success());
    assert!(
        String::from_utf8(empty.stdout)
            .unwrap()
            .contains("E-TEST-EMPTY")
    );
    fs::write(&source, "#[test]\nfn invalid(x: i32) -> i32 => x\n").unwrap();
    assert!(dir.run(&["lock", path]).status.success());
    let invalid = dir.run(&["test", path, "--json"]);
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8(invalid.stdout)
            .unwrap()
            .contains("E-TEST-SIGNATURE")
    );
    fs::write(
        &source,
        "#[test]\nfn fails_to_compile() -> i32 => missing\n",
    )
    .unwrap();
    assert!(dir.run(&["lock", path]).status.success());
    let failed = dir.run(&["test", path, "--json"]);
    assert!(!failed.status.success());
    assert!(
        String::from_utf8(failed.stdout)
            .unwrap()
            .contains("compile-failed")
    );
    fs::write(&source, "#[test]\nfn never_returns() { loop {} }\n").unwrap();
    assert!(dir.run(&["lock", path]).status.success());
    let timed_out = dir.run(&[
        "test",
        path,
        "--backend",
        "clang",
        "--test-timeout-ms",
        "30",
        "--json",
    ]);
    assert!(!timed_out.status.success(), "{timed_out:?}");
    assert!(
        String::from_utf8(timed_out.stdout)
            .unwrap()
            .contains("\"outcome\":\"timeout\"")
    );
}
