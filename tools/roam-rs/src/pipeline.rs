//! Persistent local exploration/repair workflow. Queue data never supplies shell commands.
use crate::{
    byte_buffer, host, native, profile::Profile, protocol, resource_closure, scalar, search,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, TryLockError},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zil_base::Fingerprint;

type Result<T> = std::result::Result<T, String>;
const MAX_ENTRIES: usize = 10_000;
const ARTIFACTS: [(&str, usize); 4] = [
    ("case.json", protocol::MAX_CASE + 1),
    ("result.json", protocol::MAX_FRAME),
    ("history.jsonl", protocol::MAX_FRAME),
    ("stderr.json", protocol::MAX_FRAME),
];
static SERIAL: AtomicU64 = AtomicU64::new(0);

fn unique() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    )
}
fn digest_value(value: &Value) -> String {
    Fingerprint::of(value.to_string().as_bytes()).to_hex()
}
fn hex_id(value: &str) -> Result<&str> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(value)
    } else {
        Err("expected a 64-character lowercase hexadecimal identity".into())
    }
}
fn directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!("expected a real directory: {}", path.display()));
    }
    Ok(())
}
fn regular(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(format!(
            "expected a regular non-symlink file: {}",
            path.display()
        ));
    }
    Ok(())
}
fn hash_file(path: &Path, limit: usize) -> Result<String> {
    regular(path)?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    Ok(Fingerprint::of(&host::bounded_bytes(&mut file, limit)?).to_hex())
}
fn read(path: &Path) -> Result<Value> {
    regular(path)?;
    host::read(path)
}
fn output_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    directory(parent)
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    #[cfg(not(unix))]
    let _ = path; // Files are synced; directory power-loss persistence is platform dependent.
    Ok(())
}
/// Publish complete bytes. Hard-link creation is atomic and cannot replace an existing record.
fn atomic_json(path: &Path, value: &Value, replace: bool) -> Result<()> {
    let parent = path.parent().ok_or("output requires parent directory")?;
    directory(parent)?;
    let temp = parent.join(format!(".pending-{}", unique()));
    host::durable(&temp, value)?;
    let result = if replace {
        fs::rename(&temp, path)
    } else {
        fs::hard_link(&temp, path)
    };
    if !replace || result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| e.to_string())?;
    sync_directory(parent)
}
// Keep the inode permanently. Unlinking a lock file would let a second process
// lock a different inode while this handle still owns the original lock.
struct Lock {
    _file: File,
}
impl Lock {
    fn acquire(path: PathBuf, wait_ms: u64) -> Result<Self> {
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            _ => regular(&path)?,
        }
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("cannot open operation lock {}: {e}", path.display()))?;
        regular(&path)?;
        let start = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) if start.elapsed().as_millis() < wait_ms as u128 => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    return Err(format!(
                        "operation busy or unavailable at {}: {e}",
                        path.display()
                    ));
                }
            }
        }
    }
}

struct Queue(PathBuf);
impl Queue {
    fn open(path: &Path, create: bool) -> Result<Self> {
        if create {
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        directory(path)?;
        for name in [
            "entries",
            "incoming",
            "claims",
            "closed",
            "locks",
            "snapshots",
            "verifications",
            "events",
        ] {
            let child = path.join(name);
            if create {
                fs::create_dir_all(&child).map_err(|e| e.to_string())?;
            }
            directory(&child)?;
        }
        let format = json!({"format":"zil-qc-queue/1","caseProtocol":"roam/1","workerProtocol":"roam-worker/1"});
        let metadata = path.join("queue.json");
        if create
            && !metadata.exists()
            && atomic_json(&metadata, &format, false).is_err()
            && !metadata.exists()
        {
            return Err("cannot publish queue identity".into());
        }
        if read(&metadata)? != format {
            return Err("queue format mismatch".into());
        }
        Ok(Self(fs::canonicalize(path).map_err(|e| e.to_string())?))
    }
    fn ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(self.0.join("entries")).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "non-Unicode queue entry")?;
            hex_id(&name)?;
            directory(&entry.path())?;
            ids.push(name);
            if ids.len() > MAX_ENTRIES {
                return Err("queue entry quota exceeded".into());
            }
        }
        ids.sort();
        Ok(ids)
    }
    fn state_path(&self, state: &str, id: &str) -> Result<PathBuf> {
        hex_id(id)?;
        Ok(self.0.join(state).join(format!("{id}.json")))
    }
    fn lock(&self, id: &str) -> Result<Lock> {
        hex_id(id)?;
        Lock::acquire(self.0.join("locks").join(id), 0)
    }
    fn freeze(&self, source: &Value) -> Result<PathBuf> {
        let _guard = Lock::acquire(self.0.join("locks/snapshot"), 10_000)?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let snapshot = host::freeze(
            &exe,
            &self.0.join("snapshots"),
            source["commit"].as_str().ok_or("missing commit")?,
        )?;
        File::open(snapshot.join("roam"))
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        sync_directory(&self.0.join("snapshots"))?;
        Ok(snapshot)
    }
    fn load(&self, id: &str) -> Result<(Value, Value)> {
        hex_id(id)?;
        let root = self.0.join("entries").join(id);
        directory(&root)?;
        let finding = read(&root.join("finding.json"))?;
        let inventory = finding["artifacts"]
            .as_object()
            .ok_or("missing artifact inventory")?;
        if finding["format"] != "zil-qc-finding/1" || finding["id"] != id {
            return Err("finding identity mismatch".into());
        }
        for (name, limit) in ARTIFACTS {
            if inventory.get(name).is_none()
                || inventory[name] != hash_file(&root.join(name), limit)?
            {
                return Err(format!("original artifact changed: {name}"));
            }
        }
        if inventory.len() != ARTIFACTS.len()
            || fs::read_dir(&root).map_err(|e| e.to_string())?.count() != ARTIFACTS.len() + 1
        {
            return Err("original artifact inventory mismatch".into());
        }
        let trace = host::read_json_file(&root.join("case.json"), protocol::MAX_CASE + 1)?;
        protocol::validate_case(&trace)?;
        let result = read(&root.join("result.json"))?;
        protocol::validate_result(
            &result,
            trace["steps"].as_array().ok_or("missing steps")?.len(),
        )?;
        let key = protocol::failure_key(&result).ok_or("pass is not a finding")?;
        let snapshot = finding["snapshot"]
            .as_str()
            .ok_or("missing snapshot identity")?;
        hex_id(snapshot)?;
        if finding["failureKey"] != key || finding["controlledMutation"].as_bool().is_none() {
            return Err("failure metadata mismatch".into());
        }
        if finding["status"] != result["status"]
            || finding["class"] != classify(&result)
            || finding["propertyId"] != result["failure"]["propertyId"]
            || finding["contract"] != trace["contract"]
            || finding["seed"] != *trace.get("seed").unwrap_or(&Value::Null)
            || finding["source"]["commit"].as_str().is_none()
            || finding["source"]["dirty"].as_bool().is_none()
            || finding["source"]["development"].as_bool().is_none()
        {
            return Err("finding description differs from original evidence".into());
        }
        let identity = json!([
            inventory["case.json"],
            snapshot,
            key,
            finding["controlledMutation"],
            finding["source"]
        ]);
        if digest_value(&identity) != id {
            return Err("finding content identity mismatch".into());
        }
        Ok((finding, trace))
    }
    fn snapshot(&self, finding: &Value) -> Result<PathBuf> {
        let hash = hex_id(finding["snapshot"].as_str().ok_or("missing snapshot")?)?;
        let snapshot = self.0.join("snapshots").join(hash);
        directory(&snapshot)?;
        if host::verify_snapshot(&snapshot)?["sha256"] != hash {
            return Err("snapshot directory identity mismatch".into());
        }
        Ok(snapshot)
    }
    fn publish(
        &self,
        attempt: &Path,
        snapshot: &Path,
        source: &Value,
        fault: bool,
        max_issues: usize,
    ) -> Result<(String, bool)> {
        let result = read(&attempt.join("result.json"))?;
        let failure_key = protocol::failure_key(&result).ok_or("cannot enqueue pass")?;
        if !attempt.join("stderr.json").exists() {
            host::durable(&attempt.join("stderr.json"), &json!(""))?;
        }
        let mut artifacts = BTreeMap::new();
        for (name, limit) in ARTIFACTS {
            artifacts.insert(name, hash_file(&attempt.join(name), limit)?);
        }
        let snapshot_hash = snapshot
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid snapshot path")?;
        let id = digest_value(&json!([
            artifacts["case.json"],
            snapshot_hash,
            failure_key,
            fault,
            source
        ]));
        let _guard = Lock::acquire(self.0.join("locks/publish"), 10_000)?;
        let target = self.0.join("entries").join(&id);
        if target.exists() {
            self.load(&id)?;
            return Ok((id, false));
        }
        if self.ids()?.len() >= max_issues {
            return Err(
                "queue capacity reached; unqueued attempt retained in run directory".into(),
            );
        }
        let pending = self.0.join("incoming").join(unique());
        fs::create_dir(&pending).map_err(|e| e.to_string())?;
        for (name, _) in ARTIFACTS {
            fs::copy(attempt.join(name), pending.join(name)).map_err(|e| e.to_string())?;
            File::open(pending.join(name))
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        let trace = host::read_json_file(&attempt.join("case.json"), protocol::MAX_CASE + 1)?;
        host::durable(
            &pending.join("finding.json"),
            &json!({
                "format":"zil-qc-finding/1","id":id,"source":source,"snapshot":snapshot_hash,
                "seed":trace.get("seed"),"contract":trace["contract"],"failureKey":failure_key,
                "propertyId":result["failure"]["propertyId"],"status":result["status"],
                "class":classify(&result),"controlledMutation":fault,"artifacts":artifacts,
                "boundary":"Exact case and executable bytes; source commit recorded, not build attestation. Trusted local queue, not an execution sandbox."
            }),
        )?;
        sync_directory(&pending)?;
        fs::rename(&pending, &target).map_err(|e| e.to_string())?;
        sync_directory(&self.0.join("entries"))?;
        self.load(&id)?;
        Ok((id, true))
    }
    fn claim(&self, requested: Option<&str>, owner: &str) -> Result<Value> {
        if owner.is_empty() || owner.len() > 128 || owner.chars().any(char::is_control) {
            return Err("owner must contain 1..128 printable bytes".into());
        }
        let ids = if let Some(id) = requested {
            vec![hex_id(id)?.to_string()]
        } else {
            self.ids()?
        };
        for id in ids {
            let guard = match self.lock(&id) {
                Ok(v) => v,
                Err(e) if requested.is_some() => return Err(e),
                Err(_) => continue,
            };
            self.load(&id)?;
            let claim_path = self.state_path("claims", &id)?;
            if claim_path.exists() || self.state_path("closed", &id)?.exists() {
                drop(guard);
                if requested.is_some() {
                    return Err("finding already claimed or closed".into());
                }
                continue;
            }
            let claim = json!({"format":"zil-qc-claim/1","id":id,"owner":owner,"token":digest_value(&json!(unique()))});
            atomic_json(&claim_path, &claim, false)?;
            return Ok(claim);
        }
        Err("no unclaimed findings".into())
    }
    fn claimed(&self, id: &str, token: &str) -> Result<Value> {
        hex_id(token)?;
        if self.state_path("closed", id)?.exists() {
            return Err("finding is closed".into());
        }
        let claim = read(&self.state_path("claims", id)?)?;
        if claim["format"] != "zil-qc-claim/1" || claim["id"] != id || claim["token"] != token {
            return Err("claim token mismatch".into());
        }
        Ok(claim)
    }
    fn release(&self, id: &str, token: &str, note: &str) -> Result<Value> {
        if note.len() > 4096 {
            return Err("note quota exceeded".into());
        }
        let _guard = self.lock(id)?;
        let claim = self.claimed(id, token)?;
        let event = json!({"format":"zil-qc-event/1","action":"release","id":id,"owner":claim["owner"],"note":note});
        atomic_json(
            &self.0.join("events").join(format!("{}.json", unique())),
            &event,
            false,
        )?;
        fs::remove_file(self.state_path("claims", id)?).map_err(|e| e.to_string())?;
        sync_directory(&self.0.join("claims"))?;
        Ok(event)
    }
    fn closed(&self, id: &str) -> Result<bool> {
        let path = self.state_path("closed", id)?;
        if !path.exists() {
            return Ok(false);
        }
        let receipt = read(&path)?;
        let run = receipt["verification"]
            .as_str()
            .ok_or("missing verification identity")?;
        if run.is_empty()
            || run.len() > 128
            || !run.bytes().all(|b| b.is_ascii_digit() || b == b'-')
        {
            return Err("invalid verification identity".into());
        }
        let report_path = self
            .0
            .join("verifications")
            .join(id)
            .join(run)
            .join("report.json");
        directory(report_path.parent().ok_or("invalid verification path")?)?;
        let report = read(&report_path)?;
        if receipt["format"] != "zil-qc-closed/1"
            || receipt["id"] != id
            || receipt["reportSha256"] != hash_file(&report_path, protocol::MAX_FRAME)?
            || report["status"] != "pass"
            || report["id"] != id
            || report["checksPassed"] != true
        {
            return Err("closed receipt does not identify a passing verification".into());
        }
        Ok(true)
    }
    fn list(&self) -> Result<Value> {
        let mut issues = Vec::new();
        for id in self.ids()? {
            let (finding, _) = self.load(&id)?;
            let closed = self.closed(&id)?;
            let claim = self.state_path("claims", &id)?;
            let has_claim = claim.exists();
            let state = if closed {
                "closed"
            } else if has_claim {
                "claimed"
            } else {
                "open"
            };
            // Closing commits before claim cleanup. A valid terminal receipt
            // wins even if the process stopped between those two operations.
            let owner = if !closed && has_claim {
                read(&claim)?["owner"].clone()
            } else {
                Value::Null
            };
            issues.push(json!({"id":id,"state":state,"class":finding["class"],"failureKey":finding["failureKey"],"controlledMutation":finding["controlledMutation"],"owner":owner,"cleanupPending":closed && has_claim}));
        }
        Ok(json!({"format":"zil-qc-list/1","issues":issues}))
    }
}

fn summary(result: &Value) -> Value {
    json!({"status":result["status"],"steps":result["steps"],"failureKey":protocol::failure_key(result),"issueCount":result["issues"].as_array().map(Vec::len),"reason":result.get("reason")})
}
fn classify(result: &Value) -> &'static str {
    match result["status"].as_str() {
        Some("fail") => "property",
        Some("invalid") => "input",
        Some("unsupported") => "capability",
        Some("incomplete") => "budget",
        _ => "infrastructure",
    }
}
fn source_identity(allow_dirty: bool) -> Result<Value> {
    let git = |args: &[&str]| -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        String::from_utf8(output.stdout)
            .map(|s| s.trim().to_string())
            .map_err(|e| e.to_string())
    };
    let commit = git(&["rev-parse", "HEAD"])?;
    let tracked = git(&["status", "--porcelain", "--untracked-files=no"])?;
    let sources = git(&[
        "status",
        "--porcelain",
        "--untracked-files=all",
        "--",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "crates",
        "tools/roam-rs",
        "tests/roam-rust",
    ])?;
    let dirty = !tracked.is_empty() || !sources.is_empty();
    if dirty && !allow_dirty {
        return Err(
            "commit source before a formal run; --allow-dirty=true is development only".into(),
        );
    }
    Ok(json!({"commit":commit,"dirty":dirty,"development":allow_dirty}))
}
fn worker(
    trace: &Value,
    snapshot: &Path,
    fault: bool,
    attempt: &Path,
    timeout: u64,
) -> Result<Value> {
    let run = host::supervise(
        trace,
        host::HostOptions {
            command: &snapshot.join("roam"),
            args: vec![
                "worker".into(),
                format!(
                    "--fault={}",
                    if fault {
                        Profile::for_case(trace)?.fault_name()
                    } else {
                        "none"
                    }
                ),
                format!("--budget-ms={timeout}"),
            ],
            timeout_ms: timeout,
            max_output: protocol::MAX_FRAME,
            attempt,
        },
    )?;
    Ok(run.result)
}
fn stopped(path: Option<&Path>) -> Result<bool> {
    let Some(path) = path else {
        return Ok(false);
    };
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(true),
        Ok(_) => Err("stop-file must be a regular non-symlink file".into()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

fn hunt(options: &Options) -> Result<(Value, u8)> {
    let requested_profile = options.get("profile").map(Profile::named).transpose()?;
    let cases = options.number("cases", 1000, u64::MAX)?;
    let max_ms = options.number("max-ms", 0, u64::MAX)?;
    let stop = options.get("stop-file").map(PathBuf::from);
    if cases == 0 && max_ms == 0 && stop.is_none() {
        return Err("continuous hunt requires --stop-file or a positive --max-ms".into());
    }
    let seed = options.number("seed", 1, u32::MAX as u64)? as u32;
    let default_profile = requested_profile.unwrap_or(Profile::Session);
    let steps = options.number(
        "steps",
        default_profile.default_steps() as u64,
        default_profile.max_steps() as u64,
    )? as usize;
    if default_profile.is_native() && steps == 0 {
        return Err("native generation requires positive steps".into());
    }
    let limit = options.number("max-issues", 1000, MAX_ENTRIES as u64)? as usize;
    if limit == 0 {
        return Err("max-issues must be positive".into());
    }
    let timeout = options.timeout()?;
    let pause = options.number("pause-ms", 0, 60_000)?;
    let fixed = if let Some(path) = options.get("case") {
        if options.get("seed").is_some() || options.get("steps").is_some() {
            return Err("--case cannot be combined with --seed or --steps".into());
        }
        let trace = host::read_json_file(Path::new(path), protocol::MAX_CASE)?;
        protocol::validate_case(&trace)?;
        if trace["steps"].as_array().is_none_or(Vec::is_empty) {
            return Err("hunt requires at least one action".into());
        }
        Some(trace)
    } else {
        None
    };
    let profile = fixed
        .as_ref()
        .map(Profile::for_case)
        .transpose()?
        .unwrap_or(default_profile);
    if requested_profile.is_some_and(|p| p != profile) {
        return Err("profile differs from case contract".into());
    }
    let toolchain = if profile.is_native() && cfg!(all(target_os = "linux", target_arch = "x86_64"))
    {
        Some(native::identity()?)
    } else {
        None
    };
    let source = source_identity(options.boolean("allow-dirty", false)?)?;
    let queue = Queue::open(&options.queue(), true)?;
    let root = PathBuf::from(options.required("out")?);
    output_parent(&root)?;
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let snapshot = queue.freeze(&source)?;
    let fault = options.fault(profile)?;
    let started = Instant::now();
    let mut counts = BTreeMap::<String, u64>::new();
    let mut findings = BTreeSet::new();
    let mut completed = 0u64;
    let mut actions = 0u64;
    let mut published = 0u64;
    let mut duplicates = 0u64;
    let scope = fixed
        .as_ref()
        .and_then(|case| case["contract"].as_str())
        .unwrap_or(profile.contract());
    let mut report = json!({"format":"zil-qc-hunt/1","status":"running","source":source,"snapshot":snapshot.file_name().unwrap().to_string_lossy(),"controlledMutation":fault,"requestedCases":cases,"completedCases":0,"actions":0,"queued":0,"duplicates":0,"counts":{},"scope":scope});
    atomic_json(&root.join("progress.json"), &report, false)?;
    let work = (|| -> Result<&str> {
        loop {
            if stopped(stop.as_deref())? {
                return Ok("stop-file");
            }
            if max_ms > 0 && started.elapsed().as_millis() >= max_ms as u128 {
                return Ok("time-budget");
            }
            if cases > 0 && completed >= cases {
                return Ok("cases");
            }
            let mut trace = fixed.clone().unwrap_or_else(|| {
                profile.generate(seed.wrapping_add(completed as u32) as i64, steps)
            });
            if let Some(toolchain) = &toolchain {
                native::bind_identity(&mut trace, toolchain)?;
            }
            let attempt = root.join(format!("attempt-{completed:08}"));
            let result = worker(&trace, &snapshot, fault, &attempt, timeout)?;
            completed += 1;
            actions = actions
                .checked_add(
                    result["steps"]
                        .as_u64()
                        .ok_or("missing completed action count")?,
                )
                .ok_or("action count overflow")?;
            *counts
                .entry(result["status"].as_str().ok_or("missing status")?.into())
                .or_default() += 1;
            if result["status"] != "pass" {
                let (id, new) = queue.publish(&attempt, &snapshot, &source, fault, limit)?;
                findings.insert(id);
                if new {
                    published += 1;
                } else {
                    duplicates += 1;
                }
            }
            fs::remove_dir_all(&attempt).map_err(|e| e.to_string())?;
            report["completedCases"] = json!(completed);
            report["actions"] = json!(actions);
            report["queued"] = json!(published);
            report["duplicates"] = json!(duplicates);
            report["counts"] = json!(counts);
            report["elapsedMs"] = json!(started.elapsed().as_millis());
            atomic_json(&root.join("progress.json"), &report, true)?;
            if cases > 0 && completed >= cases {
                return Ok("cases");
            }
            let pause_start = Instant::now();
            while pause_start.elapsed().as_millis() < pause as u128 {
                if stopped(stop.as_deref())? {
                    return Ok("stop-file");
                }
                if max_ms > 0 && started.elapsed().as_millis() >= max_ms as u128 {
                    return Ok("time-budget");
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    })();
    report["completedCases"] = json!(completed);
    report["actions"] = json!(actions);
    report["queued"] = json!(published);
    report["duplicates"] = json!(duplicates);
    report["counts"] = json!(counts);
    report["findings"] = json!(findings);
    report["elapsedMs"] = json!(started.elapsed().as_millis());
    let non_pass = counts
        .iter()
        .filter(|(s, _)| s.as_str() != "pass")
        .map(|(_, n)| n)
        .sum::<u64>();
    let checked = work.and_then(|reason| {
        host::verify_snapshot(&snapshot)?;
        let end = source_identity(true)?;
        if end["commit"] != source["commit"] || end["dirty"] != source["dirty"] {
            return Err("source identity changed during run".into());
        }
        Ok(reason)
    });
    let exit = match checked {
        Ok(reason) => {
            report["stopReason"] = json!(reason);
            report["status"] = json!(if non_pass > 0 {
                "findings"
            } else if reason == "cases" && completed > 0 && actions > 0 {
                "pass"
            } else {
                "stopped"
            });
            if non_pass > 0 { 1 } else { 0 }
        }
        Err(error) => {
            report["status"] = json!("error");
            report["error"] = json!(error);
            2
        }
    };
    atomic_json(&root.join("report.json"), &report, false)?;
    atomic_json(&root.join("progress.json"), &report, true)?;
    Ok((report, exit))
}

fn replay(queue: &Queue, id: &str, out: &Path, timeout: u64) -> Result<(Value, u8)> {
    let (finding, trace) = queue.load(id)?;
    let snapshot = queue.snapshot(&finding)?;
    output_parent(out)?;
    let result = worker(
        &trace,
        &snapshot,
        finding["controlledMutation"] == true,
        out,
        timeout,
    )?;
    let same = protocol::failure_key(&result).as_ref() == Some(&finding["failureKey"]);
    let report = json!({"format":"zil-qc-replay/1","id":id,"reproduced":same,"expectedFailureKey":finding["failureKey"],"actual":summary(&result),"controlledMutation":finding["controlledMutation"]});
    atomic_json(&out.join("replay.json"), &report, false)?;
    Ok((report, if same { 0 } else { 1 }))
}

fn verify(queue: &Queue, id: &str, token: &str, options: &Options) -> Result<(Value, u8)> {
    let _guard = queue.lock(id)?;
    let claim = queue.claimed(id, token)?;
    let (finding, trace) = queue.load(id)?;
    let original = queue.snapshot(&finding)?;
    let source = source_identity(options.boolean("allow-dirty", false)?)?;
    let candidate = queue.freeze(&source)?;
    let timeout = options.timeout()?;
    let profile = Profile::for_case(&trace)?;
    let fault = options.fault(profile)?;
    let run_id = unique();
    let parent = queue.0.join("verifications").join(id);
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    directory(&parent)?;
    let root = parent.join(&run_id);
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let mut report = json!({"format":"zil-qc-verification/1","id":id,"status":"error","checksPassed":false,"owner":claim["owner"],"source":source,"originalSnapshot":finding["snapshot"],"candidateSnapshot":candidate.file_name().unwrap().to_string_lossy(),"controlledMutation":finding["controlledMutation"],"candidateControlledMutation":fault,"checks":[],"regressions":0});
    let mut checks = Vec::new();
    let work = (|| -> Result<()> {
        let original_result = worker(
            &trace,
            &original,
            finding["controlledMutation"] == true,
            &root.join("original"),
            timeout,
        )?;
        checks.push(json!({"name":"original","result":summary(&original_result)}));
        if protocol::failure_key(&original_result).as_ref() != Some(&finding["failureKey"]) {
            return Err("original snapshot did not reproduce the same failure identity".into());
        }
        let candidate_result = worker(&trace, &candidate, fault, &root.join("candidate"), timeout)?;
        checks.push(json!({"name":"candidate","result":summary(&candidate_result)}));
        if candidate_result["status"] != "pass" {
            return Err("candidate does not pass the original case".into());
        }
        let fixed = profile.fixed();
        let closed = queue
            .ids()?
            .into_iter()
            .filter_map(|other| {
                if other == id {
                    return None;
                }
                match queue.closed(&other) {
                    Ok(true) => Some(Ok(other)),
                    Ok(false) => None,
                    Err(e) => Some(Err(e)),
                }
            })
            .collect::<Result<Vec<_>>>()?;
        // Freeze identifiers, not all case bodies. A large queue costs at most one case in memory.
        let names = fixed
            .iter()
            .map(|(name, _)| format!("fixed-{name}"))
            .chain(closed.iter().map(|id| format!("closed-{id}")))
            .collect::<Vec<_>>();
        let mut selection = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let regression = if index < fixed.len() {
                fixed[index].1.clone()
            } else {
                queue.load(&closed[index - fixed.len()])?.1
            };
            selection.push(json!({"name":name,"caseSha256":digest_value(&regression)}));
            let attempt = root.join(format!("regression-{index:05}"));
            let regression_fault = fault && Profile::for_case(&regression)? == profile;
            let result = worker(&regression, &candidate, regression_fault, &attempt, timeout)?;
            checks.push(json!({"name":name,"result":summary(&result)}));
            report["regressions"] = json!(index + 1);
            report["regressionSelection"] = json!(selection);
            if result["status"] != "pass" {
                return Err(format!("candidate regression failed: {name}"));
            }
            fs::remove_dir_all(&attempt).map_err(|e| e.to_string())?;
        }
        queue.load(id)?;
        queue.claimed(id, token)?;
        host::verify_snapshot(&original)?;
        host::verify_snapshot(&candidate)?;
        let end = source_identity(true)?;
        if end["commit"] != source["commit"] || end["dirty"] != source["dirty"] {
            return Err("source identity changed during verification".into());
        }
        Ok(())
    })();
    report["checks"] = json!(checks);
    let pass = match work {
        Ok(()) => {
            report["status"] = json!("pass");
            report["checksPassed"] = json!(true);
            true
        }
        Err(e) => {
            report["error"] = json!(e);
            false
        }
    };
    atomic_json(&root.join("report.json"), &report, false)?;
    if pass {
        atomic_json(
            &queue.state_path("closed", id)?,
            &json!({
                "format":"zil-qc-closed/1","id":id,"verification":run_id,
                "reportSha256":hash_file(&root.join("report.json"), protocol::MAX_FRAME)?,
                "controlledMutation":finding["controlledMutation"],"development":source["development"]
            }),
            false,
        )?;
        fs::remove_file(queue.state_path("claims", id)?).map_err(|e| e.to_string())?;
        sync_directory(&queue.0.join("claims"))?;
    }
    report["report"] = json!(root.join("report.json"));
    report["closed"] = json!(pass);
    Ok((report, if pass { 0 } else { 1 }))
}

fn reduce(queue: &Queue, id: &str, token: &str, options: &Options) -> Result<(Value, u8)> {
    let _guard = queue.lock(id)?;
    let claim = queue.claimed(id, token)?;
    let (finding, original) = queue.load(id)?;
    if finding["status"] != "fail" {
        return Err("reduction requires a reproducible property failure".into());
    }
    let profile = Profile::for_case(&original)?;
    let snapshot = queue.snapshot(&finding)?;
    let max_runs = options.number("max-runs", 64, 256)? as usize;
    let budget_ms = options.number("budget-ms", 30_000, 60_000)?;
    let timeout = options.timeout()?;
    let root = PathBuf::from(options.required("out")?);
    output_parent(&root)?;
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let mut calls = 0usize;
    let mut trials = Vec::new();
    let mut replay = |trace: &Value| -> Result<Value> {
        let remaining = budget_ms.saturating_sub(start.elapsed().as_millis() as u64);
        if remaining == 0 {
            return Ok(
                json!({"status":"incomplete","steps":0,"issues":[],"reason":"reduction-budget"}),
            );
        }
        let index = calls;
        calls += 1;
        let result = worker(
            trace,
            &snapshot,
            finding["controlledMutation"] == true,
            &root.join(format!("attempt-{index:04}")),
            timeout.min(remaining),
        )?;
        trials.push(
            json!({"attempt":index,"caseSha256":digest_value(trace),"result":summary(&result)}),
        );
        Ok(result)
    };
    let reduced = search::shrink(&original, max_runs, budget_ms, &mut replay)?;
    let mut trace = reduced["reduced"].clone();
    let mut result = reduced["result"].clone();
    let mut runs = reduced["runs"].as_u64().unwrap_or(0) as usize;
    let mut stopped = reduced["stopped"].as_str().unwrap_or("error").to_string();
    let mut ast_accepted = 0;
    if profile.is_native()
        && stopped == "complete"
        && protocol::failure_key(&result) == Some(finding["failureKey"].clone())
    {
        let mut seen = BTreeSet::from([digest_value(&trace)]);
        'ast: loop {
            let mut changed = false;
            for candidate in if profile == Profile::ResourceClosure {
                resource_closure::candidates(&trace)
            } else if profile == Profile::ByteBuffer {
                byte_buffer::candidates(&trace)
            } else {
                scalar::candidates(&trace)
            } {
                if runs >= max_runs || start.elapsed().as_millis() >= budget_ms as u128 {
                    stopped = "budget".into();
                    break 'ast;
                }
                if !seen.insert(digest_value(&candidate)) {
                    continue;
                }
                let next = replay(&candidate)?;
                runs += 1;
                if next["status"] == "fail"
                    && protocol::failure_key(&next) == Some(finding["failureKey"].clone())
                {
                    trace = candidate;
                    result = next;
                    ast_accepted += 1;
                    changed = true;
                    break;
                }
            }
            if !changed {
                break;
            }
        }
    }
    queue.load(id)?;
    queue.claimed(id, token)?;
    host::verify_snapshot(&snapshot)?;
    let reproduced = result["status"] == "fail"
        && protocol::failure_key(&result) == Some(finding["failureKey"].clone());
    let status = if stopped == "budget" {
        "incomplete"
    } else if reproduced {
        "reduced"
    } else {
        "not-reproduced"
    };
    let report = json!({"format":"zil-qc-reduction/1","id":id,"owner":claim["owner"],"status":status,"stopped":stopped,"sameFailure":reproduced,"originalSnapshot":finding["snapshot"],"originalCaseSha256":digest_value(&original),"reducerSha256":host::digest(&std::env::current_exe().map_err(|e|e.to_string())?)?,"controlledMutation":finding["controlledMutation"],"runs":runs,"workerExecutions":calls,"astAccepted":ast_accepted,"originalSteps":original["steps"].as_array().unwrap().len(),"reducedSteps":trace["steps"].as_array().unwrap().len(),"reduced":trace,"result":result,"trials":trials,"boundary":"Same property maintained; finite typed candidates, not a proof of a globally minimal program; original issue and claim unchanged"});
    if report.to_string().len() > protocol::MAX_FRAME {
        return Err("reduction report quota exceeded".into());
    }
    atomic_json(&root.join("report.json"), &report, false)?;
    atomic_json(&root.join("reduced.case.json"), &trace, false)?;
    atomic_json(
        &queue.0.join("events").join(format!("{}.json", unique())),
        &json!({"event":"reduction","id":id,"owner":claim["owner"],"report":fs::canonicalize(root.join("report.json")).map_err(|e|e.to_string())?,"reportSha256":hash_file(&root.join("report.json"),protocol::MAX_FRAME)?,"status":status}),
        false,
    )?;
    Ok((report, if status == "reduced" { 0 } else { 1 }))
}

struct Options(BTreeMap<String, String>);
impl Options {
    fn parse(args: &[String], allowed: &[&str]) -> Result<Self> {
        let mut values = BTreeMap::new();
        for arg in args {
            let (name, value) = arg
                .strip_prefix("--")
                .and_then(|s| s.split_once('='))
                .ok_or("expected --key=value")?;
            if !allowed.contains(&name)
                || values.insert(name.to_string(), value.to_string()).is_some()
            {
                return Err(format!("unknown or duplicate option: {name}"));
            }
        }
        Ok(Self(values))
    }
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
    fn required(&self, key: &str) -> Result<&str> {
        self.get(key)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("--{key} is required"))
    }
    fn number(&self, key: &str, default: u64, max: u64) -> Result<u64> {
        let value = self.get(key).map_or(Ok(default), |s| {
            s.parse::<u64>().map_err(|_| format!("invalid {key}"))
        })?;
        if value > max {
            Err(format!("{key} exceeds {max}"))
        } else {
            Ok(value)
        }
    }
    fn boolean(&self, key: &str, default: bool) -> Result<bool> {
        match self.get(key) {
            None => Ok(default),
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            _ => Err(format!("{key} must be true or false")),
        }
    }
    fn queue(&self) -> PathBuf {
        PathBuf::from(self.get("queue").unwrap_or("out/qc/issues"))
    }
    fn fault(&self, profile: Profile) -> Result<bool> {
        profile.fault(self.get("fault").unwrap_or("none"))
    }
    fn timeout(&self) -> Result<u64> {
        let timeout = self.number("timeout-ms", 10_000, 60_000)?;
        if timeout == 0 {
            Err("timeout-ms must be positive".into())
        } else {
            Ok(timeout)
        }
    }
}
pub fn cli(args: &[String]) -> Result<u8> {
    let (report, code) = if args.first().is_some_and(|s| s == "hunt") {
        let options = Options::parse(
            &args[1..],
            &[
                "queue",
                "out",
                "cases",
                "case",
                "seed",
                "steps",
                "profile",
                "max-ms",
                "stop-file",
                "pause-ms",
                "max-issues",
                "timeout-ms",
                "fault",
                "allow-dirty",
            ],
        )?;
        hunt(&options)?
    } else {
        let action = args.get(1).ok_or(
            "usage: issue list|claim|replay|verify|reduce|release --queue=PATH [--key=value]",
        )?;
        let allowed = match action.as_str() {
            "list" => &["queue"][..],
            "claim" => &["queue", "id", "owner"][..],
            "release" => &["queue", "id", "token", "note"][..],
            "replay" => &["queue", "id", "out", "timeout-ms"][..],
            "verify" => &["queue", "id", "token", "timeout-ms", "fault", "allow-dirty"][..],
            "reduce" => &[
                "queue",
                "id",
                "token",
                "out",
                "timeout-ms",
                "max-runs",
                "budget-ms",
            ][..],
            _ => return Err("unknown issue action".into()),
        };
        let options = Options::parse(&args[2..], allowed)?;
        let queue = Queue::open(&options.queue(), false)?;
        match action.as_str() {
            "list" => (queue.list()?, 0),
            "claim" => (
                queue.claim(options.get("id"), options.required("owner")?)?,
                0,
            ),
            "release" => (
                queue.release(
                    options.required("id")?,
                    options.required("token")?,
                    options.get("note").unwrap_or(""),
                )?,
                0,
            ),
            "replay" => replay(
                &queue,
                options.required("id")?,
                Path::new(options.required("out")?),
                options.timeout()?,
            )?,
            "verify" => verify(
                &queue,
                options.required("id")?,
                options.required("token")?,
                &options,
            )?,
            "reduce" => reduce(
                &queue,
                options.required("id")?,
                options.required("token")?,
                &options,
            )?,
            _ => unreachable!(),
        }
    };
    println!("{report}");
    Ok(code)
}
