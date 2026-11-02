#![cfg(target_os = "linux")]
use std::{fs, process::Command};

#[test]
fn incremental_o2_keeps_exports_current_results_and_rejects_wrong_modes() {
    let root = std::env::temp_dir().join(format!("zil-incremental-o2-cli-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("main.zil");
    let output = root.join("program");
    let mut code = String::new();
    for i in 0..600 {
        code += &format!("fn h{i}(x:i32)->i32{{return x+{};}}\n", i % 7);
    }
    code += "fn main()->i32{var sum:i32=0;";
    for i in 0..600 {
        code += &format!("sum=sum+h{i}(1);");
    }
    code += "return sum%251;}";
    fs::write(&source, &code).unwrap();
    let backends = if cfg!(feature = "llvm-api") {
        vec!["llvm-api", "clang"]
    } else {
        vec!["clang"]
    };
    for backend in backends {
        for incremental in [false, true] {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_zil"));
            cmd.arg("build").arg(&source).arg("-o").arg(&output).args([
                "--backend",
                backend,
                "--opt",
                "O2",
                "--codegen-units",
                "8",
                "--codegen-jobs",
                "2",
                "--json",
            ]);
            if incremental {
                cmd.arg("--incremental-o2");
            }
            let result = cmd.output().unwrap();
            assert!(result.status.success(), "{result:?}");
            assert!(String::from_utf8_lossy(&result.stdout).contains("\"verification_runs\":1"));
            let report = String::from_utf8_lossy(&result.stdout);
            assert!(
                report.contains(if incremental {
                    "\"actual_units\":3"
                } else {
                    "\"actual_units\":1"
                }),
                "{report}"
            );
            let expected = (0..600).map(|i| 1 + i % 7).sum::<i32>() % 251;
            assert_eq!(
                Command::new(&output).status().unwrap().code(),
                Some(expected)
            );
            let nm = Command::new("nm")
                .args(["-g", "--defined-only"])
                .arg(&output)
                .output()
                .unwrap();
            assert!(nm.status.success());
            assert_eq!(
                String::from_utf8_lossy(&nm.stdout)
                    .lines()
                    .filter(|line| line.contains("zil.fn."))
                    .count(),
                601
            );
        }
    }
    for args in [
        vec!["check", "--opt", "O2"],
        vec!["run", "--opt", "O2"],
        vec!["build", "--opt", "O0"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_zil"))
            .args(args)
            .arg(&source)
            .args(["--incremental-o2", "--json"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(64));
        assert!(String::from_utf8_lossy(&result.stdout).contains("E-CLI-ARGUMENT"));
    }
    fs::remove_dir_all(root).unwrap();
}
