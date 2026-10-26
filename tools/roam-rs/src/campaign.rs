use crate::{host, protocol, search};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

pub fn brief(r: &Value) -> Value {
    json!({"status":r["status"],"steps":r["steps"],"failure":r.get("failure"),"issues":r["issues"],"reason":r.get("reason")})
}
pub fn observations(r: &Value) -> Value {
    let mut checks = 0;
    let mut reused = 0;
    let mut verify = 0;
    for h in r["history"].as_array().unwrap_or(&Vec::new()) {
        if h["phase"] != "record"
            || h["step"]
                .as_i64()
                .is_none_or(|i| i < 0 || i >= r["steps"].as_i64().unwrap_or(0))
            || h["value"]["kind"] != "check"
        {
            continue;
        }
        checks += 1;
        for q in h["value"]["warm"]["query_events"]
            .as_array()
            .unwrap_or(&Vec::new())
        {
            if q["phase"] != "verify" && q["disposition"] == "reused" {
                reused += 1;
            }
            if q["phase"] == "verify" && q["disposition"] == "executed" {
                verify += 1;
            }
        }
    }
    json!({"checks":checks,"warm_raw_reused":reused,"warm_verify_executed":verify})
}
pub struct Runner {
    pub root: PathBuf,
    pub snapshot: PathBuf,
    pub checks: Vec<Value>,
    pub serial: usize,
}
impl Runner {
    pub fn run(&mut self, trace: &Value, label: &str, fault: bool) -> Result<Value, String> {
        let name = format!("{:04}-{label}", self.serial);
        self.serial += 1;
        let attempt = self.root.join("attempts").join(&name);
        let run = host::supervise(
            trace,
            host::HostOptions {
                command: &self.snapshot.join("roam"),
                args: vec![
                    "worker".into(),
                    format!(
                        "--fault={}",
                        if fault { "stale-after-edit" } else { "none" }
                    ),
                ],
                timeout_ms: 10_000,
                max_output: protocol::MAX_FRAME,
                attempt: &attempt,
            },
        )?;
        let mut record = brief(&run.result);
        record["name"] = json!(name);
        record["fault"] = json!(fault);
        record["elapsed_ms"] = json!(run.elapsed_ms);
        record["journal_records"] = json!(run.journal_records);
        record["observations"] = observations(&run.result);
        if run.result["status"] == "pass" {
            fs::remove_dir_all(&attempt).map_err(|e| e.to_string())?;
        } else {
            let target = self.root.join("failures").join(&name);
            host::durable(
                &attempt.join("environment.json"),
                &json!({"format":"zil-roam-native-environment/1","snapshot":format!("../../snapshots/{}",self.snapshot.file_name().unwrap().to_string_lossy()),"fault":fault,"source":"Git commit in snapshot manifest"}),
            )?;
            fs::rename(&attempt, &target).map_err(|e| e.to_string())?;
            record["evidence"] = json!(format!("failures/{name}"));
        }
        self.checks.push(record);
        Ok(run.result)
    }
}
fn require(value: bool, message: &str) -> Result<(), String> {
    if value { Ok(()) } else { Err(message.into()) }
}
pub fn run(root: &Path, commit: &str, source_dirty: bool) -> Result<Value, String> {
    fs::create_dir(root).map_err(|e| e.to_string())?;
    fs::create_dir(root.join("attempts")).map_err(|e| e.to_string())?;
    fs::create_dir(root.join("failures")).map_err(|e| e.to_string())?;
    let snapshot = host::freeze(
        &std::env::current_exe().map_err(|e| e.to_string())?,
        &root.join("snapshots"),
        commit,
    )?;
    let snapshot_name = snapshot.file_name().unwrap().to_string_lossy().into_owned();
    let mut runner = Runner {
        root: root.into(),
        snapshot,
        checks: Vec::new(),
        serial: 0,
    };
    let started = Instant::now();
    let mut report = json!({"format":"zil-native-session-gate/1","status":"error","scope":"zil-compilation-session/1; cache transparency, not language semantic correctness","sourceCommit":commit,"sourceDirty":source_dirty,"snapshot":format!("snapshots/{snapshot_name}"),"required_normal_paths":84,"required_normal_actions":1311});
    let work = (|| -> Result<(), String> {
        for (label, trace) in search::fixed() {
            let result = runner.run(&trace, &format!("fixed-{label}"), false)?;
            require(
                result["status"] == "pass",
                &format!("fixed path {label} failed"),
            )?;
            if label == "reuse" {
                let obs = observations(&result);
                require(
                    obs["warm_raw_reused"].as_i64().unwrap() >= 5
                        && obs["warm_verify_executed"] == 2,
                    "raw reuse or uncached Verify was not exercised",
                )?;
            }
        }
        let bounded = search::bfs(&search::alphabet(), 2, 1000, |c| {
            runner.run(c, "bounded-bfs", false)
        })?;
        require(
            bounded["edges"] == 56
                && bounded["states"] == 57
                && bounded["fail"] == 0
                && bounded["complete"] == false
                && bounded["stopReason"] == "depth",
            "bounded BFS coverage differs",
        )?;
        report["bounded_bfs"] = bounded;
        for seed in search::SEEDS {
            let result = runner.run(&search::generate(seed, 64), &format!("seed-{seed}"), false)?;
            require(result["status"] == "pass", &format!("seed {seed} failed"))?;
        }
        let normal_actions = runner
            .checks
            .iter()
            .filter_map(|r| r["steps"].as_u64())
            .sum::<u64>();
        require(
            runner.checks.len() == 84 && normal_actions == 1311,
            "normal campaign coverage/count differs",
        )?;
        report["normal_paths"] = json!(runner.checks.len());
        report["normal_actions"] = json!(normal_actions);
        report["normal_elapsed_ms"] = json!(started.elapsed().as_millis());
        let discovered = search::bfs(
            &[
                search::check(),
                search::action("edit", "type-error"),
                search::action("edit", "zero"),
            ],
            3,
            1000,
            |c| runner.run(c, "injected-bfs", true),
        )?;
        require(
            discovered["fail"] == 1
                && discovered["bad"][0]["path"]
                    .as_array()
                    .is_some_and(|xs| xs.len() == 3)
                && discovered["bad"][0]["why"]["failure"]["propertyId"] == "session.warm-cold",
            "injected BFS did not discover three-action witness",
        )?;
        let c = search::check;
        let a = search::action;
        let original = search::trace(vec![
            a("cache", "on"),
            c(),
            a("target", "alternate-llvm"),
            c(),
            a("cancel", "none"),
            a("edit", "type-error"),
            a("cache", "corrupt"),
            c(),
        ]);
        host::durable(&root.join("original.case.json"), &original)?;
        let original_hash = host::digest(&root.join("original.case.json"))?;
        for _ in 0..3 {
            let r = runner.run(&original, "historical-replay", true)?;
            require(
                r["status"] == "fail" && r["failure"]["propertyId"] == "session.warm-cold",
                "original frozen replay failed",
            )?;
        }
        let reduced = search::shrink(&original, 64, 30_000, |c| {
            runner.run(c, "shrink-attempt", true)
        })?;
        require(
            reduced["stopped"] == "complete"
                && reduced["reduced"]["steps"]
                    .as_array()
                    .is_some_and(|xs| xs.len() == 3)
                && reduced["result"]["failure"]["propertyId"] == "session.warm-cold",
            "shrink did not retain property and reduce to three actions",
        )?;
        host::durable(&root.join("shrink.json"), &reduced)?;
        host::durable(&root.join("reduced.case.json"), &reduced["reduced"])?;
        let replay = runner.run(&reduced["reduced"], "reduced-replay", true)?;
        require(
            replay["status"] == "fail" && replay["failure"]["propertyId"] == "session.warm-cold",
            "reduced replay failed",
        )?;
        let clean = runner.run(&reduced["reduced"], "clean-control", false)?;
        require(clean["status"] == "pass", "clean control failed")?;
        require(
            host::digest(&root.join("original.case.json"))? == original_hash,
            "original evidence changed",
        )?;
        report["injection"] = json!({"production_defect":false,"property":"session.warm-cold","discovery":discovered,"original_steps":8,"original_sha256":original_hash,"replays":3,"reduced_steps":3,"shrink_runs":reduced["runs"],"shrink_stopped":reduced["stopped"],"clean_control":brief(&clean),"original_unchanged":true});
        host::verify_snapshot(&runner.snapshot)?;
        Ok(())
    })();
    report["checks"] = json!(runner.checks);
    report["elapsed_ms"] = json!(started.elapsed().as_millis());
    match work {
        Ok(()) => report["status"] = json!("pass"),
        Err(e) => report["error"] = json!(e),
    }
    report["boundaries"] = json!([
        "Native session profile; not a full port of all original Roam APIs",
        "No FileStore multi-worker leasing, Bun bundle freeze, browser plugin, or explicit schedule support",
        "BFS retains complete history keys and is depth-bounded, not exhaustive",
        "Shrink preserves property identity and valid paths, not root cause or global minimum",
        "Cold/warm share compiler implementation; independent source oracle remains required",
        "Native worker creates no descendants; external worker process trees are outside this host profile",
        "Binary snapshot is not an OS/dynamic-library container"
    ]);
    host::durable(&root.join("report.json"), &report)?;
    Ok(report)
}
