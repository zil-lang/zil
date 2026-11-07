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
    let default_backend = dir.run(&["test", path, "--codegen-units", "2", "--json"]);
    assert!(default_backend.status.success(), "{default_backend:?}");
    fs::write(
        app.join("tests.zil"),
        "#[test]\nfn boolean_fails() -> bool => false\n#[test]\nfn integer_fails() -> i32 => 7\n",
    )
    .unwrap();
    let failed = dir.run(&["test", path, "--backend", "clang", "--json"]);
    assert!(!failed.status.success(), "{failed:?}");
    let text = String::from_utf8(failed.stdout).unwrap();
    assert!(text.contains("\"outcome\":\"exit:1\""), "{text}");
    assert!(text.contains("\"outcome\":\"exit:7\""), "{text}");
    fs::write(
        math.join("arithmetic.zil"),
        "pub fn answer() -> i32 => 42\n// changed dependency\n",
    )
    .unwrap();
    let stale = dir.run(&["test", path, "--backend", "clang", "--json"]);
    assert!(!stale.status.success());
    assert!(
        String::from_utf8(stale.stdout)
            .unwrap()
            .contains("E-PROJECT-LOCK-MISMATCH")
    );
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

#[test]
fn two_dependencies_can_use_distinct_helpers_with_the_same_import_path() {
    let dir = Scratch::new();
    let app = dir.0.join("app");
    fs::create_dir(&app).unwrap();
    fs::write(
        app.join("Zil.toml"),
        "[package]\nroot = \"main.zil\"\n[dependencies]\na = \"../a\"\nb = \"../b\"\n",
    )
    .unwrap();
    fs::write(
        app.join("main.zil"),
        "import a.lib as a\nimport b.lib as b\nfn main() -> i32 => a.answer() + b.answer()\n",
    )
    .unwrap();
    for (name, value) in [("a", 20), ("b", 22)] {
        let package = dir.0.join(name);
        fs::create_dir(&package).unwrap();
        fs::write(package.join("Zil.toml"), "[package]\nroot = \"lib.zil\"\n").unwrap();
        fs::write(
            package.join("lib.zil"),
            "import helper\npub fn answer() -> i32 => helper.value()\n",
        )
        .unwrap();
        fs::write(
            package.join("helper.zil"),
            format!("pub fn value() -> i32 => #({value})\n"),
        )
        .unwrap();
    }
    let path = app.to_str().unwrap();
    assert!(dir.run(&["lock", path]).status.success());
    let output = app.join("program");
    let cache = dir.0.join("semantic-cache");
    let build = |dir: &Scratch| {
        dir.run(&[
            "build",
            path,
            "--backend",
            "clang",
            "-o",
            output.to_str().unwrap(),
            "--semantic-cache",
            cache.to_str().unwrap(),
            "--json",
        ])
    };
    let build = build(&dir);
    assert!(build.status.success(), "{build:?}");
    let native = Command::new(&output).output().unwrap();
    assert_eq!(native.status.code(), Some(42), "{native:?}");
    fs::write(
        dir.0.join("b/helper.zil"),
        "pub fn value() -> i32 => #(23)\n",
    )
    .unwrap();
    let stale = dir.run(&[
        "build",
        path,
        "--backend",
        "clang",
        "-o",
        output.to_str().unwrap(),
        "--semantic-cache",
        cache.to_str().unwrap(),
        "--json",
    ]);
    assert!(!stale.status.success());
    assert!(
        String::from_utf8(stale.stdout)
            .unwrap()
            .contains("E-PROJECT-LOCK-MISMATCH")
    );
    assert_eq!(Command::new(&output).status().unwrap().code(), Some(42));
    assert!(dir.run(&["lock", path]).status.success());
    let rebuilt = dir.run(&[
        "build",
        path,
        "--backend",
        "clang",
        "-o",
        output.to_str().unwrap(),
        "--semantic-cache",
        cache.to_str().unwrap(),
        "--json",
    ]);
    assert!(rebuilt.status.success(), "{rebuilt:?}");
    assert!(
        String::from_utf8(rebuilt.stdout)
            .unwrap()
            .contains("\"verification_runs\":1")
    );
    assert_eq!(Command::new(&output).status().unwrap().code(), Some(43));
    fs::write(
        dir.0.join("a/lib.zil"),
        "import b.helper as stolen\npub fn answer() -> i32 => stolen.value()\n",
    )
    .unwrap();
    assert!(dir.run(&["lock", path]).status.success());
    let wrong_package = dir.run(&[
        "build",
        path,
        "--backend",
        "clang",
        "-o",
        output.to_str().unwrap(),
        "--semantic-cache",
        cache.to_str().unwrap(),
        "--json",
    ]);
    assert!(!wrong_package.status.success(), "{wrong_package:?}");
    assert!(
        String::from_utf8(wrong_package.stdout)
            .unwrap()
            .contains("E-MODULE-NOT-FOUND")
    );
    assert_eq!(Command::new(&output).status().unwrap().code(), Some(43));
    fs::write(
        dir.0.join("a/Zil.toml"),
        "[package]\nroot = \"lib.zil\"\n[dependencies]\nb = \"../b\"\n",
    )
    .unwrap();
    assert!(dir.run(&["lock", path]).status.success());
    let transitive = dir.run(&[
        "build",
        path,
        "--backend",
        "clang",
        "-o",
        output.to_str().unwrap(),
        "--semantic-cache",
        cache.to_str().unwrap(),
        "--json",
    ]);
    assert!(transitive.status.success(), "{transitive:?}");
    assert_eq!(Command::new(&output).status().unwrap().code(), Some(46));
}

#[test]
fn manifest_byte_and_file_budgets_are_checked_before_read_allocation() {
    let dir = Scratch::new();
    let app = dir.0.join("app");
    let dep = dir.0.join("dep");
    fs::create_dir(&app).unwrap();
    fs::create_dir(&dep).unwrap();
    let root_manifest = "[package]\nroot = \"main.zil\"\n[dependencies]\ndep = \"../dep\"\n";
    let dep_manifest = "[package]\nroot = \"lib.zil\"\n";
    let dep_source = "pub fn answer() -> i32 => 1\n";
    fs::write(app.join("Zil.toml"), root_manifest).unwrap();
    fs::write(app.join("main.zil"), "fn main() -> i32 => 0\n").unwrap();
    fs::write(dep.join("Zil.toml"), dep_manifest).unwrap();
    fs::write(dep.join("lib.zil"), dep_source).unwrap();
    let mut limits = zil_driver::CompileOptions::default().limits;
    limits.max_source_bytes = root_manifest.len() + dep_manifest.len() + dep_source.len();
    assert!(zil_driver::ProjectManifest::open(&app, &limits).is_ok());
    limits.max_source_bytes -= 1;
    assert!(zil_driver::ProjectManifest::open(&app, &limits).is_err());
    limits.max_source_bytes += 1;
    limits.max_nodes = 2;
    assert!(zil_driver::ProjectManifest::open(&app, &limits).is_ok());
    limits.max_nodes = 1;
    assert!(zil_driver::ProjectManifest::open(&app, &limits).is_err());
    limits.max_nodes = 2;
    limits.max_analysis_steps = "Zil.toml".len() + "lib.zil".len();
    assert!(zil_driver::ProjectManifest::open(&app, &limits).is_ok());
    limits.max_analysis_steps -= 1;
    assert!(zil_driver::ProjectManifest::open(&app, &limits).is_err());
}
