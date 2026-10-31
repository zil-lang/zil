#![cfg(target_os = "linux")]
use std::{fs, process::Command};
#[test]
fn explicit_closed_build_preserves_entry_and_changes_generated_symbol_exports() {
    let root = std::env::temp_dir().join(format!("zil-closed-cli-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("main.zil");
    fs::write(
        &source,
        "fn helper()->i32{return 21;}fn main()->i32{return helper()+helper();}",
    )
    .unwrap();
    let backends = if cfg!(feature = "llvm-api") {
        vec!["llvm-api", "clang"]
    } else {
        vec!["clang"]
    };
    for backend in backends {
        for closed in [false, true] {
            let output = root.join("program");
            let mut command = Command::new(env!("CARGO_BIN_EXE_zil"));
            command
                .arg("build")
                .arg(&source)
                .arg("-o")
                .arg(&output)
                .args(["--backend", backend, "--opt", "O2", "--json"]);
            if closed {
                command.arg("--closed-executable");
            }
            let result = command.output().unwrap();
            assert!(result.status.success(), "{result:?}");
            assert!(String::from_utf8_lossy(&result.stdout).contains("\"verification_runs\":1"));
            assert_eq!(Command::new(&output).status().unwrap().code(), Some(42));
            let nm = Command::new("nm")
                .args(["-g", "--defined-only"])
                .arg(&output)
                .output()
                .unwrap();
            assert!(nm.status.success());
            assert_eq!(
                String::from_utf8_lossy(&nm.stdout).contains("zil.fn."),
                !closed
            );
        }
    }
    for action in ["check", "run", "build"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_zil"));
        command
            .arg(action)
            .arg(&source)
            .args(["--closed-executable", "--json"]);
        if action == "build" {
            command.args(["--target", "win64"]);
        }
        let result = command.output().unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains("E-CLI-ARGUMENT"));
    }
    fs::remove_dir_all(root).unwrap();
}
