//! Real CLI/worker tests for the persistent producer/consumer workflow.
use roam::{host, search};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "roam-pipeline-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn input(&self, name: &str, edit: &str) -> PathBuf {
        let path = self.path(name);
        host::durable(
            &path,
            &search::trace(vec![
                search::check(),
                search::action("edit", edit),
                search::check(),
            ]),
        )
        .unwrap();
        path
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        if thread::panicking() {
            eprintln!("preserved failed pipeline fixture: {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_roam"));
    command.current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    command
}
fn arg(name: &str, value: impl AsRef<Path>) -> String {
    format!("--{name}={}", value.as_ref().display())
}
fn invoke(args: &[String]) -> Output {
    command().args(args).output().unwrap()
}
fn result(output: Output, expected: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.stdout.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
fn hunt_args(root: &TestDir, input: &Path, run: &str, count: u64) -> Vec<String> {
    vec![
        "hunt".into(),
        arg("queue", root.path("queue")),
        arg("out", root.path(run)),
        arg("case", input),
        format!("--cases={count}"),
        "--fault=stale-after-edit".into(),
        "--allow-dirty=true".into(),
    ]
}
fn enqueue(root: &TestDir, input: &Path, run: &str) -> String {
    let report = result(invoke(&hunt_args(root, input, run, 1)), 1);
    assert_eq!(report["completedCases"], 1);
    assert_eq!(report["counts"]["fail"], 1);
    report["findings"][0].as_str().unwrap().to_owned()
}
fn issue(root: &TestDir, action: &str, id: &str) -> Vec<String> {
    let mut args = vec![
        "issue".into(),
        action.into(),
        arg("queue", root.path("queue")),
    ];
    if !id.is_empty() {
        args.push(format!("--id={id}"));
    }
    args
}
fn claim(root: &TestDir, id: &str) -> String {
    let mut args = issue(root, "claim", id);
    args.push("--owner=test-fixer".into());
    result(invoke(&args), 0)["token"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn verify_args(root: &TestDir, id: &str, token: &str) -> Vec<String> {
    let mut args = issue(root, "verify", id);
    args.extend([format!("--token={token}"), "--allow-dirty=true".into()]);
    args
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn scalar_native_queue_replays_reduces_and_closes_with_mixed_profile_regressions() {
    let root = TestDir::new();
    // A previously closed session example still executes when a scalar
    // candidate consumes this same queue's closed regression corpus.
    let session = root.input("session.json", "one");
    let session_id = enqueue(&root, &session, "session-hunt");
    let session_token = claim(&root, &session_id);
    assert_eq!(
        result(invoke(&verify_args(&root, &session_id, &session_token)), 0)["closed"],
        true
    );
    let input = root.path("scalar.json");
    host::durable(&input, &roam::scalar::mutation_case()).unwrap();
    let mut hunt = hunt_args(&root, &input, "scalar-hunt", 2);
    hunt.retain(|a| !a.starts_with("--fault="));
    hunt.push("--fault=scalar-sub-to-add".into());
    let found = result(invoke(&hunt), 1);
    assert_eq!(found["scope"], roam::scalar::CONTRACT);
    assert_eq!(found["queued"], 1);
    assert_eq!(found["duplicates"], 1);
    let id = found["findings"][0].as_str().unwrap();
    let token = claim(&root, id);
    let original = root.path(&format!("queue/entries/{id}/case.json"));
    let bytes = fs::read(&original).unwrap();
    assert!(read(&original)["meta"]["nativeToolchain"]["sha256"].is_string());
    let mut replay = issue(&root, "replay", id);
    replay.push(arg("out", root.path("scalar-replay")));
    assert_eq!(result(invoke(&replay), 0)["reproduced"], true);
    let mut reduce = issue(&root, "reduce", id);
    reduce.extend([
        format!("--token={token}"),
        arg("out", root.path("zero-reduction")),
        "--max-runs=0".into(),
    ]);
    let zero = result(invoke(&reduce), 1);
    assert_eq!(zero["status"], "incomplete");
    assert_eq!(zero["workerExecutions"], 0);
    reduce.retain(|a| !a.starts_with("--out=") && !a.starts_with("--max-runs="));
    reduce.extend([
        arg("out", root.path("reduction")),
        "--max-runs=64".into(),
        "--budget-ms=30000".into(),
    ]);
    let reduced = result(invoke(&reduce), 0);
    assert_eq!(reduced["sameFailure"], true);
    assert_eq!(reduced["originalSteps"], 3);
    assert_eq!(reduced["reducedSteps"], 1);
    assert!(reduced["astAccepted"].as_u64().unwrap() > 0);
    assert_eq!(
        reduced["result"]["failure"]["propertyId"],
        roam::scalar::PROPERTY
    );
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert!(root.path(&format!("queue/claims/{id}.json")).exists());
    assert!(!root.path(&format!("queue/closed/{id}.json")).exists());
    let mut bad = verify_args(&root, id, &token);
    bad.push("--fault=scalar-sub-to-add".into());
    assert_eq!(result(invoke(&bad), 1)["closed"], false);
    let good = result(invoke(&verify_args(&root, id, &token)), 0);
    assert_eq!(good["closed"], true);
    assert_eq!(good["regressions"], roam::scalar::fixed().len() + 1);
    assert!(
        good["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == format!("closed-{session_id}"))
    );
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert!(!root.path(&format!("queue/claims/{id}.json")).exists());
}

#[test]
fn producer_continues_and_repair_requires_reproduction_candidate_and_regressions() {
    let root = TestDir::new();
    let input = root.input("one.json", "one");
    let report = result(invoke(&hunt_args(&root, &input, "hunt", 3)), 1);
    assert_eq!(report["completedCases"], 3);
    assert_eq!(report["actions"], 9);
    assert_eq!(report["counts"]["fail"], 3);
    assert_eq!(report["queued"], 1);
    assert_eq!(report["duplicates"], 2);
    let id = report["findings"][0].as_str().unwrap();
    let original = root.path(&format!("queue/entries/{id}/case.json"));
    let bytes = fs::read(&original).unwrap();
    let listing = result(invoke(&issue(&root, "list", "")), 0);
    assert_eq!(listing["issues"].as_array().unwrap().len(), 1);
    assert_eq!(listing["issues"][0]["state"], "open");
    assert_eq!(listing["issues"][0]["controlledMutation"], true);
    let token = claim(&root, id);
    let claim_path = root.path(&format!("queue/claims/{id}.json"));
    let claim_bytes = fs::read(&claim_path).unwrap();
    let mut replay = issue(&root, "replay", id);
    replay.push(arg("out", root.path("replay")));
    assert_eq!(result(invoke(&replay), 0)["reproduced"], true);
    let mut bad = verify_args(&root, id, &token);
    bad.push("--fault=stale-after-edit".into());
    let bad = result(invoke(&bad), 1);
    assert_eq!(bad["closed"], false);
    assert_eq!(bad["regressions"], 0);
    assert!(!root.path(&format!("queue/closed/{id}.json")).exists());
    let good = result(invoke(&verify_args(&root, id, &token)), 0);
    assert_eq!(good["closed"], true);
    assert_eq!(good["regressions"], 11);
    assert_eq!(good["checks"][0]["result"]["status"], "fail");
    assert_eq!(good["checks"][1]["result"]["status"], "pass");
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert!(!claim_path.exists());
    let listing = result(invoke(&issue(&root, "list", "")), 0);
    assert_eq!(listing["issues"][0]["state"], "closed");
    assert_eq!(listing["issues"][0]["owner"], Value::Null);
    assert_eq!(listing["issues"][0]["cleanupPending"], false);
    // A subsequent repair consumes previously closed examples as regression inputs.
    let second = root.input("second.json", "type-error");
    let second_id = enqueue(&root, &second, "second-hunt");
    let second_token = claim(&root, &second_id);
    let second_report = result(invoke(&verify_args(&root, &second_id, &second_token)), 0);
    assert_eq!(second_report["regressions"], 12);
    assert!(
        second_report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["name"] == format!("closed-{id}"))
    );
    assert!(!claim_path.exists());
    assert!(
        !root
            .path(&format!("queue/claims/{second_id}.json"))
            .exists()
    );

    // Controlled interrupted-close state: closed was committed, claim cleanup
    // did not finish. This is separate from the natural absence checks above.
    let closed_path = root.path(&format!("queue/closed/{id}.json"));
    let closed_bytes = fs::read(&closed_path).unwrap();
    fs::write(&claim_path, &claim_bytes).unwrap();
    let listing = result(invoke(&issue(&root, "list", "")), 0);
    let terminal = listing["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap();
    assert_eq!(terminal["state"], "closed");
    assert_eq!(terminal["owner"], Value::Null);
    assert_eq!(terminal["cleanupPending"], true);
    assert_eq!(fs::read(&claim_path).unwrap(), claim_bytes);
    let mut retry = issue(&root, "claim", id);
    retry.push("--owner=must-not-reopen".into());
    result(invoke(&retry), 2);
    result(invoke(&verify_args(&root, id, &token)), 2);
    let mut release = issue(&root, "release", id);
    release.push(format!("--token={token}"));
    result(invoke(&release), 2);
    assert_eq!(fs::read(&claim_path).unwrap(), claim_bytes);
    assert_eq!(fs::read(&closed_path).unwrap(), closed_bytes);
    // The stale claim is not read; it cannot override a valid terminal state.
    fs::write(&claim_path, b"not valid json").unwrap();
    assert!(result(invoke(&issue(&root, "list", "")), 0)["issues"].is_array());
    fs::write(&closed_path, b"{}").unwrap();
    result(invoke(&issue(&root, "list", "")), 2);
    fs::write(&closed_path, &closed_bytes).unwrap();
    assert_eq!(fs::read(&original).unwrap(), bytes);
}

#[test]
fn concurrent_consumers_cannot_share_a_claim_and_wrong_tokens_cannot_mutate_it() {
    let root = TestDir::new();
    let input = root.input("input.json", "one");
    let id = enqueue(&root, &input, "hunt");
    let mut args = issue(&root, "claim", &id);
    args.push("--owner=consumer-a".into());
    let a = command()
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    *args.last_mut().unwrap() = "--owner=consumer-b".into();
    let b = command()
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let a = a.wait_with_output().unwrap();
    let b = b.wait_with_output().unwrap();
    let (winner, loser) = if a.status.success() { (a, b) } else { (b, a) };
    let won = result(winner, 0);
    result(loser, 2);
    let token = won["token"].as_str().unwrap();
    let mut release = issue(&root, "release", &id);
    release.push(format!("--token={}", "0".repeat(64)));
    result(invoke(&release), 2);
    result(invoke(&verify_args(&root, &id, &"0".repeat(64))), 2);
    assert!(root.path(&format!("queue/claims/{id}.json")).exists());
    *release.last_mut().unwrap() = format!("--token={token}");
    assert_eq!(result(invoke(&release), 0)["action"], "release");
    assert!(!root.path(&format!("queue/claims/{id}.json")).exists());
    assert_ne!(claim(&root, &id), token);
}

#[test]
fn kernel_operation_locks_release_on_process_death_without_replacing_inode() {
    let root = TestDir::new();
    let input = root.input("input.json", "one");
    let id = enqueue(&root, &input, "found");
    let mut args = issue(&root, "claim", &id);
    args.push("--owner=crash-control".into());
    let claim = result(invoke(&args), 0);
    let token = claim["token"].as_str().unwrap();
    let claim_path = root.path(&format!("queue/claims/{id}.json"));
    let claim_bytes = fs::read(&claim_path).unwrap();
    let lock_path = root.path(&format!("queue/locks/{id}"));
    let open = |path| {
        fs::File::options()
            .read(true)
            .write(true)
            .open(path)
            .unwrap()
    };
    let snapshot = open(root.path("queue/locks/snapshot"));
    snapshot.try_lock().unwrap();
    let other = open(root.path("queue/locks/snapshot"));
    assert!(matches!(
        other.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
    drop(other);
    let observer = open(lock_path.clone());
    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        observer.metadata().unwrap().ino()
    };

    // The real verifier holds the issue lock while it waits for our snapshot
    // lock. Kill that owned process, before it can mutate the business claim.
    let mut verify = issue(&root, "verify", &id);
    verify.push(format!("--token={token}"));
    verify.push("--allow-dirty=true".into());
    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut holder = OwnedChild(
        command()
            .args(&verify)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    loop {
        match observer.try_lock() {
            Err(fs::TryLockError::WouldBlock) => break,
            Ok(()) => observer.unlock().unwrap(),
            Err(e) => panic!("cannot observe issue lock: {e}"),
        }
        assert!(holder.0.try_wait().unwrap().is_none());
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(5));
    }
    let mut release = issue(&root, "release", &id);
    release.push(format!("--token={token}"));
    let busy = invoke(&release);
    assert_eq!(busy.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&busy.stderr).contains("operation busy"));
    holder.0.kill().unwrap();
    holder.0.wait().unwrap();
    observer.try_lock().unwrap();
    assert_eq!(fs::read(&claim_path).unwrap(), claim_bytes);
    assert_eq!(fs::metadata(&lock_path).unwrap().len(), 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&lock_path).unwrap().ino(), inode);
    }
    drop(observer);
    drop(snapshot);
    result(invoke(&release), 0);
    assert!(!claim_path.exists());
    assert!(lock_path.is_file());
}

#[test]
fn tampered_original_metadata_case_or_snapshot_cannot_replay_or_close() {
    let root = TestDir::new();
    let input = root.input("input.json", "one");
    let id = enqueue(&root, &input, "hunt");
    let token = claim(&root, &id);
    let original = root.path(&format!("queue/entries/{id}/case.json"));
    let original_bytes = fs::read(&original).unwrap();
    fs::write(&original, b"{}").unwrap();
    result(invoke(&verify_args(&root, &id, &token)), 2);
    fs::write(&original, &original_bytes).unwrap();
    let metadata = root.path(&format!("queue/entries/{id}/finding.json"));
    let metadata_bytes = fs::read(&metadata).unwrap();
    let mut finding = read(&metadata);
    finding["source"]["commit"] = json!("0".repeat(40));
    fs::write(&metadata, finding.to_string()).unwrap();
    result(invoke(&verify_args(&root, &id, &token)), 2);
    fs::write(&metadata, &metadata_bytes).unwrap();
    let snapshot = read(&metadata)["snapshot"].as_str().unwrap().to_owned();
    fs::write(
        root.path(&format!("queue/snapshots/{snapshot}/roam")),
        b"corrupt",
    )
    .unwrap();
    let mut replay = issue(&root, "replay", &id);
    replay.push(arg("out", root.path("must-not-run")));
    result(invoke(&replay), 2);
    assert!(!root.path("must-not-run").exists());
    assert!(!root.path(&format!("queue/closed/{id}.json")).exists());
    assert!(root.path(&format!("queue/claims/{id}.json")).exists());
}

#[test]
fn queue_capacity_retains_unpublished_failure_and_original_issue() {
    let root = TestDir::new();
    let input = root.input("input.json", "one");
    let first = enqueue(&root, &input, "first");
    let other = root.input("other.json", "type-error");
    let mut args = hunt_args(&root, &other, "full", 2);
    args.push("--max-issues=1".into());
    let report = result(invoke(&args), 2);
    assert_eq!(report["completedCases"], 1);
    assert_eq!(report["queued"], 0);
    assert_eq!(report["status"], "error");
    assert!(report["error"].as_str().unwrap().contains("capacity"));
    assert_eq!(
        read(&root.path("full/attempt-00000000/result.json"))["status"],
        "fail"
    );
    assert!(
        root.path(&format!("queue/entries/{first}/case.json"))
            .exists()
    );
    assert_eq!(fs::read_dir(root.path("queue/entries")).unwrap().count(), 1);
}

#[test]
fn continuous_hunt_observes_stop_file_and_zero_execution_is_never_pass() {
    let root = TestDir::new();
    let input = root.input("input.json", "one");
    let mut args = hunt_args(&root, &input, "continuous", 0);
    args.retain(|s| !s.starts_with("--fault="));
    args.extend([arg("stop-file", root.path("stop")), "--pause-ms=10".into()]);
    let mut child = command()
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        let progress = root.path("continuous/progress.json");
        if progress.exists() && read(&progress)["completedCases"].as_u64().unwrap_or(0) >= 2 {
            break;
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            panic!("continuous hunt did not execute two cases");
        }
        thread::sleep(Duration::from_millis(10));
    }
    fs::write(root.path("stop"), b"requested").unwrap();
    let report = result(child.wait_with_output().unwrap(), 0);
    assert_eq!(report["status"], "stopped");
    assert_eq!(report["stopReason"], "stop-file");
    assert!(report["completedCases"].as_u64().unwrap() >= 2);
    assert!(report["actions"].as_u64().unwrap() >= 6);
    assert!(fs::read_dir(root.path("continuous")).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("attempt-")
    }));
    let pos = args.iter().position(|s| s.starts_with("--out=")).unwrap();
    args[pos] = arg("out", root.path("already-stopped"));
    let zero = result(invoke(&args), 0);
    assert_eq!(zero["completedCases"], 0);
    assert_eq!(zero["status"], "stopped");
    args.retain(|s| !s.starts_with("--stop-file="));
    result(invoke(&args), 2);

    // A long inter-case pause cannot postpone a completed campaign or its budget.
    let mut args = hunt_args(&root, &input, "time-budget", 0);
    args.retain(|s| !s.starts_with("--fault="));
    args.extend(["--max-ms=20".into(), "--pause-ms=5000".into()]);
    let timed = result(invoke(&args), 0);
    assert_eq!(timed["status"], "stopped");
    assert_eq!(timed["stopReason"], "time-budget");
    assert!(timed["elapsedMs"].as_u64().unwrap() < 3000);
    args.retain(|s| !s.starts_with("--max-ms="));
    let out = args.iter().position(|s| s.starts_with("--out=")).unwrap();
    args[out] = arg("out", root.path("last-case"));
    let cases = args.iter().position(|s| s.starts_with("--cases=")).unwrap();
    args[cases] = "--cases=1".into();
    let final_case = result(invoke(&args), 0);
    assert_eq!(final_case["status"], "pass");
    assert_eq!(final_case["completedCases"], 1);
    assert!(final_case["elapsedMs"].as_u64().unwrap() < 3000);
}

#[test]
fn unsupported_is_queued_as_capability_and_cannot_be_closed_as_a_fix() {
    let root = TestDir::new();
    let input = root.path("unsupported.json");
    let mut trace = search::trace(vec![search::check()]);
    trace["schedule"] = json!([{"deliver":"not-supported"}]);
    host::durable(&input, &trace).unwrap();
    let mut args = hunt_args(&root, &input, "hunt", 2);
    args.retain(|s| !s.starts_with("--fault="));
    let report = result(invoke(&args), 1);
    assert_eq!(report["completedCases"], 2);
    assert_eq!(report["counts"]["unsupported"], 2);
    let id = report["findings"][0].as_str().unwrap();
    let listing = result(invoke(&issue(&root, "list", "")), 0);
    assert_eq!(listing["issues"][0]["class"], "capability");
    assert_eq!(listing["issues"][0]["controlledMutation"], false);
    let token = claim(&root, id);
    let verified = result(invoke(&verify_args(&root, id, &token)), 1);
    assert_eq!(verified["closed"], false);
}

#[test]
fn concurrent_producers_deduplicate_complete_records_and_binary_snapshots() {
    let root = TestDir::new();
    let input = root.input("input.json", "one");
    let a = command()
        .args(hunt_args(&root, &input, "a", 2))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let b = command()
        .args(hunt_args(&root, &input, "b", 2))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let a = result(a.wait_with_output().unwrap(), 1);
    let b = result(b.wait_with_output().unwrap(), 1);
    assert_eq!(a["completedCases"], 2);
    assert_eq!(b["completedCases"], 2);
    assert_eq!(
        a["queued"].as_u64().unwrap() + b["queued"].as_u64().unwrap(),
        1
    );
    assert_eq!(
        a["duplicates"].as_u64().unwrap() + b["duplicates"].as_u64().unwrap(),
        3
    );
    assert_eq!(fs::read_dir(root.path("queue/entries")).unwrap().count(), 1);
    assert_eq!(
        fs::read_dir(root.path("queue/snapshots")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(root.path("queue/incoming")).unwrap().count(),
        0
    );
    assert_eq!(
        result(invoke(&issue(&root, "list", "")), 0)["issues"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn invalid_identifiers_and_existing_output_are_rejected_without_overwrite() {
    let root = TestDir::new();
    let input = root.input("input.json", "one");
    enqueue(&root, &input, "hunt");
    let mut bad = issue(&root, "claim", "../outside");
    bad.push("--owner=consumer".into());
    result(invoke(&bad), 2);
    assert!(!root.path("outside").exists());
    let protected = root.path("hunt/report.json");
    let bytes = fs::read(&protected).unwrap();
    result(invoke(&hunt_args(&root, &input, "hunt", 1)), 2);
    assert_eq!(fs::read(&protected).unwrap(), bytes);
    let mut unknown = issue(&root, "list", "");
    unknown.push("--shell=must-not-execute".into());
    result(invoke(&unknown), 2);
    #[cfg(unix)]
    {
        let id = result(invoke(&issue(&root, "list", "")), 0)["issues"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let case = root.path(&format!("queue/entries/{id}/case.json"));
        fs::remove_file(&case).unwrap();
        std::os::unix::fs::symlink(&input, &case).unwrap();
        result(invoke(&issue(&root, "list", "")), 2);
    }
}
