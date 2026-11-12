//! One ordinary manifest application composes owning hash storage, lazy owning
//! iteration, borrowed callback forwarding and the full integer domain.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_dir() {
            copy_tree(&item.path(), &target.join(item.file_name()));
        } else {
            fs::copy(item.path(), target.join(item.file_name())).unwrap();
        }
    }
}
fn command(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zil"))
        .args(args)
        .output()
        .unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn owning_inventory_project_builds_and_runs_from_a_fresh_manifest() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/ordinary-application");
    let scratch =
        std::env::temp_dir().join(format!("zil-ordinary-application-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    copy_tree(&root, &scratch);
    let app = scratch.join("app");
    let path = app.to_str().unwrap();
    success(&command(&["lock", path]));
    let check = command(&["check", path, "--json"]);
    success(&check);
    assert!(String::from_utf8_lossy(&check.stdout).contains("\"verification_runs\":1"));
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            let binary = scratch.join(format!("inventory-{backend}-{optimization}"));
            let result = command(&[
                "build",
                path,
                "--backend",
                backend,
                "--opt",
                optimization,
                "--output",
                binary.to_str().unwrap(),
                "--json",
            ]);
            success(&result);
            let report = String::from_utf8(result.stdout).unwrap();
            assert!(report.contains("\"verification_runs\":1"), "{report}");
            assert!(report.contains("\"fallback\":null"), "{report}");
            let ran = Command::new(&binary).output().unwrap();
            success(&ran);
            assert_eq!(ran.stdout, b"ABC");
            assert!(ran.stderr.is_empty());
        }
    }
    let tests = command(&["test", path, "--backend", "clang", "--json"]);
    success(&tests);
    let report = String::from_utf8(tests.stdout).unwrap();
    assert!(report.contains("\"passed\":2"), "{report}");
    let ran = command(&["run", path, "--backend", "clang", "--json"]);
    success(&ran);
    let report = String::from_utf8(ran.stdout).unwrap();
    assert!(report.contains("\"stdout\":\"ABC\""), "{report}");
    fs::write(
        scratch.join("model/records.zil"),
        fs::read_to_string(root.join("model/records.zil")).unwrap()
            + "\n// changed locked dependency\n",
    )
    .unwrap();
    let changed = command(&["check", path, "--json"]);
    assert!(!changed.status.success());
    assert!(String::from_utf8_lossy(&changed.stdout).contains("E-PROJECT-LOCK-MISMATCH"));
    fs::remove_dir_all(scratch).unwrap();
}
