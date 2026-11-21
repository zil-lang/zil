use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "zil-cli-single-pipeline-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_check_admits_valid_and_rejects_safety_and_unused_ct_errors() {
    let dir = Scratch::new();
    let ok_src = dir.0.join("ok.zil");
    fs::write(&ok_src, "fn main() -> i32 { return 7; }\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zil"))
        .args(["check", ok_src.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("\"status\":\"Success\""), "{text}");
    assert!(text.contains("\"verification_runs\":1"), "{text}");

    let bad_borrow = dir.0.join("bad_borrow.zil");
    fs::write(
        &bad_borrow,
        "fn leak() -> &i32 { let x = 1; return &x; }\nfn main() -> i32 { return 0; }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zil"))
        .args(["check", bad_borrow.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("\"status\":\"SafeReject\""), "{text}");
    assert!(text.contains("\"verification_runs\":0"), "{text}");

    let bad_ct = dir.0.join("bad_ct.zil");
    fs::write(
        &bad_ct,
        "fn unused(t: Type) -> i32 { return missing_sym; }\nfn main() -> i32 { return 0; }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zil"))
        .args(["check", bad_ct.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("\"phase\":\"ct\""), "{text}");
    assert!(text.contains("\"status\":\"SafeReject\""), "{text}");
}

#[test]
fn cli_build_and_run_execute_store_and_bytes_at_o0_and_o2() {
    let dir = Scratch::new();
    let src = dir.0.join("app.zil");
    fs::write(
        &src,
        r#"
fn main() -> i32 {
    let b = bytes_new();
    bytes_push(b, u8(12));
    bytes_push(b, u8(30));
    let v = bytes_view(b);
    return i32(v[0]) + i32(v[1]);
}
"#,
    )
    .unwrap();
    for opt in ["O0", "O2"] {
        let exe = dir.0.join(format!("app_{opt}"));
        let build = Command::new(env!("CARGO_BIN_EXE_zil"))
            .args([
                "build",
                src.to_str().unwrap(),
                "-o",
                exe.to_str().unwrap(),
                "--opt",
                opt,
                "--json",
            ])
            .output()
            .unwrap();
        assert!(build.status.success(), "build failed at {opt}: {build:?}");
        let run = Command::new(&exe).status().unwrap();
        assert_eq!(run.code(), Some(42), "expected 42 at {opt}");
    }
}

#[test]
fn cli_resident_reuses_parse_and_always_runs_fresh_verify() {
    let dir = Scratch::new();
    let src = dir.0.join("res.zil");
    fs::write(&src, "fn main() -> i32 { return 0; }\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_zil"))
        .args(["check", src.to_str().unwrap(), "--resident", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let req = "compile\ncompile\nquit\n";
    child
        .stdin
        .take()
        .unwrap()
        .write_all(req.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("\"disposition\":\"reused\""), "{text}");
    assert!(text.contains("\"verification_runs\":1"), "{text}");
}
