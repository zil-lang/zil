#![cfg(feature = "timings")]
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-cli-timing-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("main.zil"), "fn main()->i32=>42").unwrap();
        Self(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn command(directory: &Dir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zil"));
    command.arg("check").arg(directory.0.join("main.zil"));
    command
}
#[test]
fn optional_timings_are_structured_and_do_not_replace_cli_output() {
    let directory = Dir::new();
    let plain = command(&directory).output().unwrap();
    let timed = command(&directory).arg("--timings").output().unwrap();
    assert!(plain.status.success() && timed.status.success());
    assert_eq!(plain.stdout, timed.stdout);
    assert!(plain.stderr.is_empty());
    assert!(
        String::from_utf8(timed.stderr)
            .unwrap()
            .contains("\"clock\":\"monotonic_wall\"")
    );
    let plain_json = command(&directory).arg("--json").output().unwrap();
    let timed_json = command(&directory)
        .args(["--json", "--timings"])
        .output()
        .unwrap();
    assert!(plain_json.status.success() && timed_json.status.success());
    assert!(
        String::from_utf8(plain_json.stdout)
            .unwrap()
            .contains("\"timings\":null")
    );
    let output = String::from_utf8(timed_json.stdout).unwrap();
    assert!(output.contains("\"timings\":{\"clock\":\"monotonic_wall\""));
    assert!(output.contains("\"name\":\"verify\""));
    assert!(timed_json.stderr.is_empty());
}
#[test]
fn failed_source_keeps_diagnostics_and_completed_timing_scopes() {
    let directory = Dir::new();
    fs::write(directory.0.join("main.zil"), "fn main( {").unwrap();
    let output = command(&directory)
        .args(["--json", "--timings"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("\"status\":\"SafeReject\""));
    assert!(text.contains("\"timings\":{\"clock\":\"monotonic_wall\""));
    assert!(!text.contains("\"outcome\":\"running\""));
    assert!(!text.contains("\"name\":\"verify\""));
}
