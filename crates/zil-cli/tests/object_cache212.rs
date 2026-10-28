#![cfg(all(feature = "llvm-api", target_os = "linux"))]
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil-cli-cache212-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::write(p.join("main.zil"), "fn main()->i32 { return 42; }").unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn command(d: &Dir) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_zil"));
    c.arg("build")
        .arg(d.0.join("main.zil"))
        .arg("-o")
        .arg(d.0.join("program"))
        .arg("--json")
        .arg("--object-cache")
        .arg(d.0.join("objects"));
    c
}
#[test]
fn standalone_cli_warm_hit_is_not_a_noop_or_reused_verify() {
    let d = Dir::new();
    for expected in ["\"hits\":0", "\"hits\":1"] {
        let o = command(&d).output().unwrap();
        assert!(o.status.success(), "{o:?}");
        let s = String::from_utf8(o.stdout).unwrap();
        assert!(s.contains(expected), "{s}");
        assert!(s.contains("\"verification_runs\":1"));
        assert_eq!(
            Command::new(d.0.join("program")).status().unwrap().code(),
            Some(42)
        );
    }
}
#[test]
fn clang_does_not_silently_claim_native_api_cache_hits() {
    let d = Dir::new();
    let o = command(&d).args(["--backend", "clang"]).output().unwrap();
    assert!(!o.status.success());
    assert!(
        String::from_utf8(o.stdout)
            .unwrap()
            .contains("E-OBJECT-CACHE-BACKEND")
    );
    assert!(!d.0.join("program").exists());
}
#[test]
fn loader_interposition_environment_disables_object_reuse_visibly() {
    let d = Dir::new();
    let o = command(&d)
        .env("LD_PRELOAD", "/nonexistent/zil-test-interpose.so")
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(text.contains("dynamic-loader interposition"), "{text}");
    assert!(text.contains("\"enabled\":false"));
}
