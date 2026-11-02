#![cfg(all(feature = "llvm-api", target_os = "linux"))]
use std::{fs, process::Command};

#[test]
fn cli_policy_reaches_cache_keys_exports_and_failure_cleanup() {
    let dir = std::env::temp_dir().join(format!("zil-function-merging-cli-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let source = dir.join("main.zil");
    let output = dir.join("program");
    fs::write(&source,"fn alpha(x:i32)->i32{return ((x/3)*5)%97+11;}\nfn beta(x:i32)->i32{return ((x/3)*5)%97+11;}\nfn main()->i32{let a:fn(i32)->i32=alpha;let b:fn(i32)->i32=beta;return a(7)+b(7);}").unwrap();
    let command = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_zil"));
        c.arg("build")
            .arg(&source)
            .arg("-o")
            .arg(&output)
            .args([
                "--opt",
                "O2",
                "--backend",
                "llvm-api",
                "--json",
                "--object-cache",
            ])
            .arg(dir.join("objects"));
        c
    };
    let mut keys = Vec::<String>::new();
    for round in 0..2 {
        for (index, policy) in ["off", "after", "before-and-after"].into_iter().enumerate() {
            let result = command()
                .args(["--function-merging", policy])
                .output()
                .unwrap();
            let report = String::from_utf8(result.stdout).unwrap();
            assert!(result.status.success(), "{report}");
            assert!(report.contains("\"verification_runs\":1"));
            assert!(
                report.contains(if round == 0 {
                    "\"hits\":0,\"misses\":1"
                } else {
                    "\"hits\":1,\"misses\":0"
                }),
                "{report}"
            );
            let key = report
                .split("\"action_key\":\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned();
            if round == 0 {
                assert!(!keys.contains(&key));
                keys.push(key);
            } else {
                assert_eq!(key, keys[index]);
            }
            let pipeline = report
                .split("\"pipeline\":\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            assert_eq!(
                pipeline,
                match policy {
                    "off" => "default<O2>",
                    "after" => "default<O2>|merge-functions=after|aliases=false",
                    _ => "mergefunc,default<O2>|merge-functions=after|aliases=false",
                }
            );
            assert_eq!(Command::new(&output).status().unwrap().code(), Some(42));
            let nm = Command::new("nm")
                .args(["-g", "--defined-only"])
                .arg(&output)
                .output()
                .unwrap();
            assert!(nm.status.success());
            let symbols = String::from_utf8(nm.stdout).unwrap();
            let zil = symbols
                .lines()
                .filter(|line| line.contains(" T zil.fn."))
                .collect::<Vec<_>>();
            assert_eq!(zil.len(), 3);
            let address = |name: &str| {
                zil.iter()
                    .find(|line| line.ends_with(name))
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
            };
            assert_ne!(address("zil.fn.0"), address("zil.fn.1"));
        }
    }
    let before = fs::read(&output).unwrap();
    let unsupported = command()
        .args([
            "--backend",
            "clang",
            "--function-merging",
            "before-and-after",
        ])
        .output()
        .unwrap();
    assert!(!unsupported.status.success());
    let report = String::from_utf8(unsupported.stdout).unwrap();
    assert!(
        report.contains("\"status\":\"Unsupported\"")
            && report.contains("E-CODEGEN-MERGING-BACKEND"),
        "{report}"
    );
    assert_eq!(fs::read(&output).unwrap(), before);
    for args in [vec!["check", "--opt", "O2"], vec!["build", "--opt", "O0"]] {
        let result = Command::new(env!("CARGO_BIN_EXE_zil"))
            .args(args)
            .arg(&source)
            .args(["--function-merging", "after", "--json"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(64));
        assert!(String::from_utf8_lossy(&result.stdout).contains("E-CLI-ARGUMENT"));
    }
    fs::write(
        &source,
        "fn alpha(x:i32)->i32{return x;}fn main()->i32{return alpha(1,2);}",
    )
    .unwrap();
    let rejected = command()
        .args(["--function-merging", "before-and-after"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stdout).contains("E-CALL-ARITY"));
    assert_eq!(fs::read(&output).unwrap(), before);
}
