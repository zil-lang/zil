//! Actual stdio sessions; malformed/stale inputs never become cached success.
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-resident-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Worker {
    fn new(path: &Path, args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zil"))
            .arg("check")
            .arg(path)
            .args(["--json", "--resident"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut worker = Self {
            child,
            input,
            output,
        };
        let ready = worker.read();
        assert!(ready.contains("\"ready\":true"), "{ready}");
        worker
    }
    fn read(&mut self) -> String {
        let mut line = String::new();
        assert!(
            self.output.read_line(&mut line).unwrap() > 0,
            "worker closed its output"
        );
        line
    }
    fn request(&mut self, command: &str) -> String {
        writeln!(self.input, "{command}").unwrap();
        self.input.flush().unwrap();
        self.read()
    }
    fn quit(&mut self) {
        writeln!(self.input, "quit").unwrap();
        self.input.flush().unwrap();
        assert!(self.child.wait().unwrap().success());
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn success(text: &str) {
    assert!(text.contains("\"status\":\"Success\""), "{text}");
    assert!(text.contains("\"verification_runs\":1"), "{text}");
    assert!(text.contains("\"active_queries\":0"), "{text}");
}

#[test]
fn resident_edits_errors_and_recovery_obtain_current_verification() {
    let dir = Scratch::new();
    dir.write("main.zil", "fn main()->i32{return 42;}\n");
    let mut worker = Worker::new(&dir.0.join("main.zil"), &[]);
    success(&worker.request("compile"));
    let warm = worker.request("compile");
    success(&warm);
    assert!(warm.contains("\"disposition\":\"reused\""), "{warm}");
    dir.write("main.zil", "fn main()->i32{return absent;}\n");
    let bad = worker.request("compile");
    assert!(!bad.contains("\"status\":\"Success\""), "{bad}");
    let fresh = Command::new(env!("CARGO_BIN_EXE_zil"))
        .arg("check")
        .arg(dir.0.join("main.zil"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(!fresh.status.success());
    let fresh = String::from_utf8(fresh.stdout).unwrap();
    let code = fresh
        .split("\"code\":\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    assert!(bad.contains(code), "cached {bad}, fresh {fresh}");
    dir.write("main.zil", "fn main()->i32{return 17;}\n");
    success(&worker.request("compile"));
    fs::remove_file(dir.0.join("main.zil")).unwrap();
    assert!(worker.request("compile").contains("E-SOURCE-READ"));
    dir.write("main.zil", "fn main()->i32{return 19;}\n");
    success(&worker.request("compile"));
    worker.quit();
}

#[test]
fn resident_clears_every_cache_epoch_without_skipping_verify() {
    let dir = Scratch::new();
    dir.write("main.zil", "fn main()->i32{return 7;}\n");
    let mut worker = Worker::new(
        &dir.0.join("main.zil"),
        &["--resident-capacity", "8", "--resident-reset-after", "2"],
    );
    success(&worker.request("compile"));
    let second = worker.request("compile");
    success(&second);
    assert!(
        second.contains("\"session_cleared\":true") && second.contains("\"cached_queries\":0"),
        "{second}"
    );
    let third = worker.request("compile");
    success(&third);
    assert!(!third.contains("\"disposition\":\"reused\""), "{third}");
    let clear = worker.request("clear");
    assert!(clear.contains("\"cleared\":true"));
    let stats = worker.request("stats");
    assert!(
        stats.contains("\"cached_queries\":0") && stats.contains("\"retained_requests\":0"),
        "{stats}"
    );
    success(&worker.request("compile"));
    worker.quit();
}

#[test]
fn resident_reopens_changed_manifests_and_rejects_stale_dependency_locks() {
    let dir = Scratch::new();
    fs::create_dir(dir.0.join("app")).unwrap();
    fs::create_dir(dir.0.join("lib")).unwrap();
    dir.write(
        "app/Zil.toml",
        "[package]\nroot = \"main.zil\"\n[dependencies]\nlib = \"../lib\"\n",
    );
    dir.write("lib/Zil.toml", "[package]\nroot = \"helper.zil\"\n");
    dir.write("lib/helper.zil", "pub fn answer()->i32{return 42;}\n");
    dir.write(
        "app/main.zil",
        "import lib.helper as h\nfn main()->i32{return h.answer();}\n",
    );
    let app = dir.0.join("app");
    let lock = || {
        let status = Command::new(env!("CARGO_BIN_EXE_zil"))
            .arg("lock")
            .arg(&app)
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
    };
    lock();
    let mut worker = Worker::new(&app, &[]);
    success(&worker.request("compile"));
    dir.write("lib/helper.zil", "pub fn answer()->i32{return 43;}\n");
    let stale = worker.request("compile");
    assert!(stale.contains("E-PROJECT-LOCK-MISMATCH"), "{stale}");
    assert!(stale.contains("\"verification_runs\":0"));
    lock();
    success(&worker.request("compile"));
    dir.write(
        "app/Zil.toml",
        "[package]\nroot = \"other.zil\"\n[dependencies]\nlib = \"../lib\"\n",
    );
    dir.write("app/other.zil", "fn main()->i32{return wrong;}\n");
    lock();
    let changed = worker.request("compile");
    assert!(!changed.contains("\"status\":\"Success\""), "{changed}");
    dir.write("app/other.zil", "fn main()->i32{return 0;}\n");
    success(&worker.request("compile"));
    worker.quit();
}

#[test]
fn resident_timeout_starts_at_each_request_not_process_start() {
    let dir = Scratch::new();
    dir.write("main.zil", "fn main()->i32{return 0;}\n");
    let mut worker = Worker::new(&dir.0.join("main.zil"), &["--compile-timeout-ms", "1000"]);
    std::thread::sleep(Duration::from_millis(1100));
    success(&worker.request("compile"));
    std::thread::sleep(Duration::from_millis(1100));
    success(&worker.request("compile"));
    worker.quit();
    let mut cancelled = Worker::new(&dir.0.join("main.zil"), &["--compile-timeout-ms", "0"]);
    for _ in 0..2 {
        let report = cancelled.request("compile");
        assert!(report.contains("\"status\":\"Cancelled\""), "{report}");
        assert!(report.contains("\"verification_runs\":0"));
    }
    cancelled.quit();
}

#[test]
fn resident_options_are_explicit_and_cannot_run_a_user_program() {
    for args in [
        vec!["run", "missing.zil", "--resident", "--json"],
        vec!["test", ".", "--resident", "--json"],
        vec!["check", "missing.zil", "--resident"],
        vec!["check", "missing.zil", "--json", "--resident-capacity", "8"],
        vec![
            "check",
            "missing.zil",
            "--json",
            "--resident",
            "--resident-capacity",
            "0",
        ],
        vec![
            "check",
            "missing.zil",
            "--json",
            "--resident",
            "--resident-reset-after",
            "0",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_zil"))
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(64), "{output:?}");
        assert!(
            !String::from_utf8(output.stdout)
                .unwrap()
                .contains("\"ready\":true")
        );
    }
}
