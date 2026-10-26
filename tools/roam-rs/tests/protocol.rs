use roam::{domain, host, protocol, search};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
static SEQ: AtomicU64 = AtomicU64::new(0);
fn temp() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "roam-test-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&p).unwrap();
    p
}
fn execute(case: &Value) -> Value {
    domain::execute(case, &domain::Options::default(), |_, _| Ok(()), |_| Ok(()))
}
#[test]
fn case_validation_rejects_loss_and_malformed_shape() {
    for text in [
        "-0",
        "-0.0",
        "1e999",
        "{\"format\":\"roam/1\",\"contract\":\"x\",\"steps\":[],\"seed\":-0}",
    ] {
        assert!(protocol::parse(text.as_bytes(), 4096).is_err(), "{text}");
    }
    let base = search::trace(vec![search::check()]);
    for (field, value) in [
        ("format", json!("roam/2")),
        ("contract", json!("")),
        ("steps", json!([{}])),
        ("requires", json!([3])),
        ("schedule", json!({})),
        ("seed", json!(9007199254740992u64)),
        ("meta", json!([])),
    ] {
        let mut t = base.clone();
        t[field] = value;
        assert_eq!(execute(&t)["status"], "invalid");
    }
    assert!(protocol::equal(&json!({"n":1}), &json!({"n":1.0})));
    assert!(!protocol::equal(&json!({"n":1}), &json!({"n":2})));
}
#[test]
fn six_statuses_remain_distinct() {
    let case = search::trace(vec![
        search::check(),
        search::action("edit", "one"),
        search::check(),
    ]);
    assert_eq!(execute(&case)["status"], "pass");
    let fault = domain::Options {
        fault: true,
        ..domain::Options::default()
    };
    let failed = domain::execute(&case, &fault, |_, _| Ok(()), |_| Ok(()));
    assert_eq!(failed["status"], "fail");
    assert_eq!(failed["failure"]["propertyId"], "session.warm-cold");
    assert_eq!(failed["steps"], 3);
    let invalid = search::trace(vec![search::action("unknown", "x")]);
    assert_eq!(execute(&invalid)["status"], "invalid");
    let mut unsupported = case.clone();
    unsupported["schedule"] = json!([{"deliver":"x"}]);
    assert_eq!(execute(&unsupported)["status"], "unsupported");
    unsupported["schedule"] = json!([]);
    unsupported["requires"] = json!(["network"]);
    assert_eq!(execute(&unsupported)["status"], "unsupported");
    let mut error = case.clone();
    error["initial"] = json!({"source":"one"});
    assert_eq!(execute(&error)["status"], "error");
    let bounded = domain::Options {
        max_steps: 1,
        ..domain::Options::default()
    };
    let r = domain::execute(&case, &bounded, |_, _| Ok(()), |_| Ok(()));
    assert_eq!(r["status"], "incomplete");
    assert_eq!(r["steps"], 1);
    assert_eq!(r["reason"], "step-budget");
}
#[test]
fn acknowledgement_failure_dispatches_no_action_and_time_budget_is_not_pass() {
    let mut calls = 0;
    let trace = search::trace(vec![search::check()]);
    let r = domain::execute(
        &trace,
        &domain::Options::default(),
        |_, _| Err("wrong ack".into()),
        |_| {
            calls += 1;
            Ok(())
        },
    );
    assert_eq!(r["status"], "error");
    assert_eq!(r["steps"], 0);
    assert_eq!(calls, 0);
    let options = domain::Options {
        budget_ms: 0,
        ..domain::Options::default()
    };
    let r = domain::execute(
        &search::trace(vec![]),
        &options,
        |_, _| panic!(),
        |_| panic!(),
    );
    assert_eq!(r["status"], "incomplete");
    assert_eq!(r["reason"], "time-budget");
}
#[test]
fn invalid_or_different_failures_do_not_replace_original() {
    let original = search::trace(vec![search::check(), search::check()]);
    let original_bytes = original.to_string();
    for status in ["invalid", "error", "fail"] {
        let report = search::shrink(&original, 64, 1000, |c| {
            if c["steps"].as_array().unwrap().len() == 2 {
                Ok(protocol::failure("target", 2, 1, json!(0), json!(1)))
            } else if status == "fail" {
                Ok(protocol::failure("different", 0, -1, json!(0), json!(1)))
            } else {
                Ok(protocol::issue(
                    status, 0, "fixture", -1, "Error", "rejected",
                ))
            }
        })
        .unwrap();
        assert_eq!(report["reduced"], original);
        assert_eq!(report["result"]["failure"]["propertyId"], "target");
        assert_eq!(original.to_string(), original_bytes);
    }
    let report = search::shrink(&original, 0, 1000, |_| panic!()).unwrap();
    assert_eq!(report["stopped"], "budget");
    assert_eq!(report["runs"], 0);
    let report = search::shrink(&original, 1, 1000, |_| {
        Ok(protocol::failure("target", 0, -1, json!(0), json!(1)))
    })
    .unwrap();
    assert_eq!(report["stopped"], "budget");
    assert_eq!(report["runs"], 1);
}
#[test]
fn complete_history_bfs_checks_all_56_edges_and_root() {
    let mut calls = 0;
    let mut actions = 0;
    let report = search::bfs(&search::alphabet(), 2, 1000, |c| {
        calls += 1;
        actions += c["steps"].as_array().unwrap().len();
        Ok(protocol::result(
            "pass",
            c["steps"].as_array().unwrap().len(),
        ))
    })
    .unwrap();
    assert_eq!(report["edges"], 56);
    assert_eq!(report["states"], 57);
    assert_eq!(calls, 57);
    assert_eq!(actions, 105);
    assert_eq!(report["complete"], false);
    assert_eq!(report["stopReason"], "depth");
    let capped = search::bfs(&search::alphabet(), 2, 1, |c| {
        Ok(protocol::result(
            "pass",
            c["steps"].as_array().unwrap().len(),
        ))
    })
    .unwrap();
    assert_eq!(capped["capped"], true);
    assert_eq!(capped["stopReason"], "cap");
}
#[test]
fn result_validation_rejects_forged_pass_and_missing_failure_identity() {
    let mut r = protocol::result("pass", 1);
    r["steps"] = json!(true);
    assert!(protocol::validate_result(&r, 1).is_err());
    r = protocol::result("fail", 1);
    assert!(protocol::validate_result(&r, 1).is_err());
    r = protocol::failure("x", 1, 0, json!(0), json!(1));
    r["status"] = json!("pass");
    assert!(protocol::validate_result(&r, 1).is_err());
    r = protocol::result("pass", 2);
    assert!(protocol::validate_result(&r, 1).is_err());
    let a = protocol::issue("error", 0, "host", -1, "Timeout", "a");
    let b = protocol::issue("error", 0, "host", -1, "Timeout", "different address");
    assert_eq!(protocol::failure_key(&a), protocol::failure_key(&b));
}
#[test]
fn snapshot_tampering_and_extra_files_fail_closed() {
    let root = temp();
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_roam"));
    let snapshot = host::freeze(&binary, &root, "test-commit").unwrap();
    assert!(host::verify_snapshot(&snapshot).is_ok());
    fs::write(snapshot.join("extra"), b"x").unwrap();
    assert!(host::verify_snapshot(&snapshot).is_err());
    fs::remove_file(snapshot.join("extra")).unwrap();
    fs::write(snapshot.join("roam"), b"bad").unwrap();
    assert!(host::verify_snapshot(&snapshot).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn real_worker_rejects_wrong_ack_before_action() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_roam"))
        .arg("worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut text = String::new();
    output.read_line(&mut text).unwrap();
    let hello: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(hello["type"], "hello");
    writeln!(
        input,
        "{}",
        json!({"type":"run","case":search::trace(vec![search::check()])})
    )
    .unwrap();
    input.flush().unwrap();
    text.clear();
    output.read_line(&mut text).unwrap();
    let before: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(before["type"], "before");
    writeln!(input, "{}", json!({"type":"ack","step":99})).unwrap();
    input.flush().unwrap();
    text.clear();
    output.read_line(&mut text).unwrap();
    let result: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(result["type"], "result");
    assert_eq!(result["result"]["status"], "error");
    assert_eq!(result["result"]["steps"], 0);
    drop(input);
    assert!(child.wait().unwrap().success());
}
#[test]
fn native_supervisor_checks_durable_order_and_action_count() {
    let root = temp();
    let trace = search::trace(vec![search::check(), search::check()]);
    let r = host::supervise(
        &trace,
        host::HostOptions {
            command: &PathBuf::from(env!("CARGO_BIN_EXE_roam")),
            args: vec!["worker".into()],
            timeout_ms: 10_000,
            max_output: protocol::MAX_FRAME,
            attempt: &root.join("attempt"),
        },
    )
    .unwrap();
    assert_eq!(r.result["status"], "pass");
    assert_eq!(r.result["steps"], 2);
    assert_eq!(r.journal_records, 5);
    let frames = fs::read_to_string(root.join("attempt/history.jsonl"))
        .unwrap()
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(frames, ["before", "after", "before", "after", "result"]);
    assert!(
        host::supervise(
            &trace,
            host::HostOptions {
                command: &PathBuf::from("/definitely-missing-roam-worker"),
                args: vec![],
                timeout_ms: 100,
                max_output: 1000,
                attempt: &root.join("spawn")
            }
        )
        .unwrap()
        .result["status"]
            == "error"
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn malicious_worker_protocol_and_timeout_never_pass() {
    // This fixture is test-only, never spawns descendants, and exercises actual pipes.
    let root = temp();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/roam-rust/fault-worker.py");
    let trace = search::trace(vec![search::check()]);
    for mode in [
        "missing",
        "malformed",
        "after-before",
        "wrong-action",
        "duplicate-result",
        "forged-pass",
        "wrong-count",
        "timeout",
        "output-budget",
        "crash",
    ] {
        let r = host::supervise(
            &trace,
            host::HostOptions {
                command: &PathBuf::from("python3"),
                args: vec![fixture.to_string_lossy().into_owned(), mode.into()],
                timeout_ms: if mode == "timeout" { 100 } else { 3000 },
                max_output: 4096,
                attempt: &root.join(mode),
            },
        )
        .unwrap();
        assert_eq!(r.result["status"], "error", "{mode}: {}", r.result);
        assert!(root.join(mode).join("case.json").exists());
        assert!(root.join(mode).join("result.json").exists());
        if mode == "timeout" {
            assert_eq!(r.result["reason"], "timeout");
        }
        if mode == "missing" {
            assert_eq!(r.result["reason"], "missing-result");
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn worker_not_reading_large_run_frame_cannot_block_host_deadline() {
    let root = temp();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/roam-rust/fault-worker.py");
    let mut trace = search::trace(vec![search::check()]);
    trace["meta"] = json!({"padding":"x".repeat(512*1024)});
    let started = std::time::Instant::now();
    let result = host::supervise(
        &trace,
        host::HostOptions {
            command: &PathBuf::from("python3"),
            args: vec![fixture.to_string_lossy().into_owned(), "not-reading".into()],
            timeout_ms: 100,
            max_output: protocol::MAX_FRAME,
            attempt: &root.join("attempt"),
        },
    )
    .unwrap();
    assert_eq!(result.result["status"], "error");
    assert_eq!(result.result["reason"], "timeout");
    assert_eq!(result.result["steps"], 0);
    // Loose scheduling allowance, well below the fixture's natural 1.2s exit.
    assert!(
        started.elapsed().as_millis() < 900,
        "blocked host for {}ms",
        started.elapsed().as_millis()
    );
    assert_eq!(fs::read_dir(root.join("attempt")).unwrap().count(), 4);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn byte_limited_read_consumes_at_most_limit_plus_one() {
    struct Endless {
        read: usize,
    }
    impl std::io::Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            buffer.fill(b'x');
            self.read += buffer.len();
            Ok(buffer.len())
        }
    }
    let mut reader = Endless { read: 0 };
    let result = host::bounded_bytes(&mut reader, 8192);
    assert!(result.unwrap_err().contains("byte quota"));
    assert_eq!(reader.read, 8193);
    let mut exact = &b"{}"[..];
    assert_eq!(host::bounded_bytes(&mut exact, 2).unwrap(), b"{}");
    let mut zero = &b"x"[..];
    assert!(host::bounded_bytes(&mut zero, 0).is_err());
}

#[test]
fn oversized_case_manifest_and_binary_files_fail_before_full_read() {
    let root = temp();
    let oversized = root.join("sparse.json");
    fs::File::create(&oversized)
        .unwrap()
        .set_len(256 * 1024 * 1024)
        .unwrap();
    assert!(
        host::read_json_file(&oversized, protocol::MAX_CASE)
            .unwrap_err()
            .contains("byte quota")
    );
    assert!(host::read(&oversized).unwrap_err().contains("byte quota"));
    assert!(host::digest(&oversized).unwrap_err().contains("byte quota"));
    let output = Command::new(env!("CARGO_BIN_EXE_roam"))
        .args([
            "replay",
            &format!("--case={}", oversized.display()),
            &format!("--out={}", root.join("must-not-exist").display()),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("byte quota"));
    assert!(!root.join("must-not-exist").exists());
    let mut case = search::trace(vec![]);
    case["meta"] = json!({"padding":"x".repeat(protocol::MAX_CASE)});
    assert_eq!(execute(&case)["status"], "invalid");
    fs::remove_dir_all(root).unwrap();
}
