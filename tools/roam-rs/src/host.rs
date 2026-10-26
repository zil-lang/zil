//! Checkpointed worker supervision; Linux workers own an isolated process group.
use crate::{domain, native, profile, protocol};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use zil_base::Fingerprint;

const MAX_BINARY_BYTES: usize = 128 * 1024 * 1024;

pub fn durable(path: &Path, value: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
    file.write_all(b"\n")
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}
/// Read at most limit + 1 bytes, including for files that grow after metadata.
/// The extra byte establishes overflow without allocating for the entire input.
pub fn bounded_bytes(reader: &mut impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let cap = limit.checked_add(1).ok_or("invalid byte quota")?;
    let mut bytes = Vec::new();
    reader
        .take(cap as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("input byte quota exceeded".into());
    }
    Ok(bytes)
}
fn file_bytes(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("input must be a regular file".into());
    }
    if metadata.len() > limit as u64 {
        return Err("input byte quota exceeded".into());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    bounded_bytes(&mut file, limit)
}
pub fn read_json_file(path: &Path, limit: usize) -> Result<Value, String> {
    protocol::parse(&file_bytes(path, limit)?, limit)
}
pub fn read(path: &Path) -> Result<Value, String> {
    read_json_file(path, protocol::MAX_FRAME)
}
pub fn digest(path: &Path) -> Result<String, String> {
    // Fingerprint currently exposes a one-shot digest; bound that separate input
    // class explicitly rather than pretending the JSON quota covers executables.
    Ok(Fingerprint::of(&file_bytes(path, MAX_BINARY_BYTES)?).to_hex())
}
pub fn freeze(exe: &Path, root: &Path, commit: &str) -> Result<PathBuf, String> {
    let source = fs::File::open(exe).map_err(|e| e.to_string())?;
    freeze_open_file(source, root, commit)
}
fn freeze_open_file(mut source: fs::File, root: &Path, commit: &str) -> Result<PathBuf, String> {
    // Bind metadata, permissions and bytes to the opened object, even if a
    // concurrent build replaces the original pathname after File::open.
    let metadata = source.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("input must be a regular file".into());
    }
    if metadata.len() > MAX_BINARY_BYTES as u64 {
        return Err("input byte quota exceeded".into());
    }
    let bytes = bounded_bytes(&mut source, MAX_BINARY_BYTES)?;
    let hash = Fingerprint::of(&bytes).to_hex();
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let target = root.join(&hash);
    if target.exists() {
        drop(bytes);
        verify_snapshot(&target)?;
        return Ok(target);
    }
    let pending = root.join(format!(".pending-{}", std::process::id()));
    fs::create_dir(&pending).map_err(|e| e.to_string())?;
    let mut output = fs::File::create_new(pending.join("roam")).map_err(|e| e.to_string())?;
    output
        .write_all(&bytes)
        .and_then(|()| output.set_permissions(metadata.permissions()))
        .and_then(|()| output.sync_all())
        .map_err(|e| e.to_string())?;
    drop(output);
    drop(bytes);
    durable(
        &pending.join("manifest.json"),
        &json!({"format":"zil-roam-native-snapshot/1","sha256":hash,"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"sourceCommit":commit,"boundary":"Exact executable bytes only; source identified by Git commit. OS, CPU and dynamic libraries are not frozen."}),
    )?;
    fs::rename(&pending, &target).map_err(|e| e.to_string())?;
    verify_snapshot(&target)?;
    Ok(target)
}
pub fn verify_snapshot(directory: &Path) -> Result<Value, String> {
    let manifest = read(&directory.join("manifest.json"))?;
    if manifest["format"] != "zil-roam-native-snapshot/1"
        || manifest["platform"] != std::env::consts::OS
        || manifest["arch"] != std::env::consts::ARCH
    {
        return Err("snapshot format or platform mismatch".into());
    }
    if fs::symlink_metadata(directory.join("roam"))
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("snapshot executable must not be a symlink".into());
    }
    if manifest["sha256"] != digest(&directory.join("roam"))? {
        return Err("snapshot executable hash mismatch".into());
    }
    let mut paths = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .map(|e| {
            e.map(|e| e.file_name().to_string_lossy().into_owned())
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    if paths != ["manifest.json", "roam"] {
        return Err("snapshot inventory mismatch".into());
    }
    Ok(manifest)
}

fn line(reader: &mut impl BufRead, limit: usize) -> Result<Option<Vec<u8>>, String> {
    let mut value = Vec::new();
    let n = Read::take(reader, (limit + 1) as u64)
        .read_until(b'\n', &mut value)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(None);
    }
    if n > limit {
        return Err("worker frame quota exceeded".into());
    }
    if !value.ends_with(b"\n") {
        return Err("unterminated protocol frame".into());
    }
    Ok(Some(value))
}
fn send(writer: &mut impl Write, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|e| e.to_string())?;
    writer
        .write_all(b"\n")
        .and_then(|()| writer.flush())
        .map_err(|e| e.to_string())
}
pub fn worker(options: domain::Options, fault_name: &str) -> Result<(), String> {
    let mut input = BufReader::new(io::stdin().lock());
    let mut output = io::stdout().lock();
    send(
        &mut output,
        &json!({"type":"hello","format":"roam-worker/1","capabilities":[]}),
    )?;
    let frame = line(&mut input, protocol::MAX_CASE + 64)?.ok_or("controller closed before run")?;
    let message = protocol::parse(&frame, protocol::MAX_CASE + 64)?;
    if message["type"] != "run" {
        return Err("invalid controller message".into());
    }
    // RefCell is only to share the synchronous writer between the two callbacks.
    let output = std::cell::RefCell::new(output);
    let result = profile::execute(
        &message["case"],
        &options,
        fault_name,
        |index, action| {
            send(
                &mut *output.borrow_mut(),
                &json!({"type":"before","step":index,"action":action}),
            )?;
            let frame =
                line(&mut input, protocol::MAX_CASE)?.ok_or("controller closed before ACK")?;
            let ack = protocol::parse(&frame, protocol::MAX_CASE)?;
            if ack["type"] != "ack" || protocol::integer(&ack["step"]) != Some(index as i64) {
                return Err("checkpoint acknowledgement mismatch".into());
            }
            Ok(())
        },
        |index| {
            send(
                &mut *output.borrow_mut(),
                &json!({"type":"after","step":index}),
            )
        },
    );
    send(
        &mut *output.borrow_mut(),
        &json!({"type":"result","result":result}),
    )
}

pub struct HostOptions<'a> {
    pub command: &'a Path,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub max_output: usize,
    pub attempt: &'a Path,
}
pub struct Supervised {
    pub result: Value,
    pub stderr: String,
    pub elapsed_ms: u128,
    pub journal_records: usize,
}
enum Event {
    Line(Vec<u8>),
    StdoutDone,
    Stderr(Vec<u8>),
    StderrDone,
    ReadError(String),
    WriteError(String),
}

pub fn supervise(trace: &Value, options: HostOptions<'_>) -> Result<Supervised, String> {
    protocol::validate_case(trace)?;
    if options.timeout_ms == 0 {
        return Err("finite positive timeout required".into());
    }
    fs::create_dir(options.attempt).map_err(|e| e.to_string())?;
    durable(&options.attempt.join("case.json"), trace)?;
    let mut journal = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(options.attempt.join("history.jsonl"))
        .map_err(|e| e.to_string())?;
    let mut journal_records = 0;
    let mut record = |v: &Value| -> Result<(), String> {
        serde_json::to_writer(&mut journal, v).map_err(|e| e.to_string())?;
        journal
            .write_all(b"\n")
            .and_then(|()| journal.sync_all())
            .map_err(|e| e.to_string())?;
        journal_records += 1;
        Ok(())
    };
    let start = Instant::now();
    let mut command = Command::new(options.command);
    command
        .args(&options.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let native_root = fs::canonicalize(options.attempt)
        .map_err(|e| e.to_string())?
        .join("native");
    let native_profile = profile::Profile::for_case(trace).is_ok_and(|p| p.is_native());
    if native_profile {
        command.env("ZIL_QC_NATIVE_ROOT", &native_root);
        #[cfg(target_os = "linux")]
        command.env("ZIL_QC_SUPERVISED", "owned-process-group/1");
    } else {
        command
            .env_remove("ZIL_QC_SUPERVISED")
            .env_remove("ZIL_QC_NATIVE_ROOT");
    }
    let mut child = match command.spawn() {
        Ok(v) => v,
        Err(e) => {
            let r = protocol::host_error(0, "spawn", &e.to_string());
            durable(&options.attempt.join("result.json"), &r)?;
            return Ok(Supervised {
                result: r,
                stderr: String::new(),
                elapsed_ms: start.elapsed().as_millis(),
                journal_records: 0,
            });
        }
    };
    let mut input = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::sync_channel(16);
    // Pipe writes may block forever if a worker stops reading. Only this thread
    // writes stdin; the supervisor queues bounded messages without ever waiting
    // on that pipe, owns the deadline, and kills/reaps the worker before joins.
    let (write_tx, write_rx) = mpsc::sync_channel::<Value>(1);
    let writer_events = tx.clone();
    let input_writer = thread::spawn(move || {
        while let Ok(message) = write_rx.recv() {
            if let Err(error) = send(&mut input, &message) {
                let _ = writer_events.send(Event::WriteError(error));
                break;
            }
        }
    });
    let mut write_tx = Some(write_tx);
    let queue = |sender: &Option<mpsc::SyncSender<Value>>, message: Value| -> Result<(), String> {
        sender
            .as_ref()
            .ok_or("worker stdin closed")?
            .try_send(message)
            .map_err(|e| format!("worker write queue: {e}"))
    };
    let reader_tx = tx.clone();
    let stdout_reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match line(&mut reader, protocol::MAX_FRAME) {
                Ok(Some(v)) => {
                    if reader_tx.send(Event::Line(v)).is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    let _ = reader_tx.send(Event::StdoutDone);
                    break;
                }
                Err(e) => {
                    let _ = reader_tx.send(Event::ReadError(e));
                    break;
                }
            }
        }
    });
    let stderr_reader = thread::spawn(move || {
        let mut reader = stderr;
        let mut buf = [0; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(Event::StderrDone);
                    break;
                }
                Ok(n) => {
                    if tx.send(Event::Stderr(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Event::ReadError(e.to_string()));
                    break;
                }
            }
        }
    });
    let actions = trace["steps"].as_array().unwrap();
    let mut completed = 0;
    let mut dispatched = 0;
    let mut active = None;
    let mut hello = false;
    let mut terminal = None;
    let mut error = None;
    let mut bytes = 0;
    let mut stderr_bytes = Vec::new();
    let mut out_done = false;
    let mut err_done = false;
    let mut exit_status = None;
    while error.is_none() {
        if start.elapsed() >= Duration::from_millis(options.timeout_ms) {
            error = Some(("timeout".to_string(), "worker timeout".to_string()));
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => exit_status = Some(status),
            Ok(None) => {}
            Err(e) => {
                error = Some(("wait".into(), e.to_string()));
                break;
            }
        }
        if out_done && err_done && exit_status.is_some() {
            break;
        }
        let event = match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(v) => v,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => {
                if exit_status.is_some() {
                    break;
                }
                continue;
            }
        };
        match event {
            Event::StdoutDone => out_done = true,
            Event::StderrDone => err_done = true,
            Event::ReadError(e) => error = Some(("protocol:frame".into(), e)),
            Event::WriteError(e) => error = Some(("write".into(), e)),
            Event::Stderr(v) => {
                bytes += v.len();
                let keep = v
                    .len()
                    .min(options.max_output.saturating_sub(stderr_bytes.len()));
                stderr_bytes.extend_from_slice(&v[..keep]);
            }
            Event::Line(raw) => {
                bytes += raw.len();
                let process = (|| -> Result<(), String> {
                    if bytes > options.max_output {
                        return Err("worker output budget exceeded".into());
                    }
                    let m = protocol::parse(&raw, protocol::MAX_FRAME)?;
                    if !m.is_object() {
                        return Err("worker message must be an object".into());
                    }
                    if !hello {
                        if m["type"] != "hello"
                            || m["format"] != "roam-worker/1"
                            || m["capabilities"]
                                .as_array()
                                .is_none_or(|v| v.iter().any(|c| !c.is_string()))
                        {
                            return Err("invalid worker handshake".into());
                        }
                        hello = true;
                        queue(&write_tx, json!({"type":"run","case":trace}))?;
                        return Ok(());
                    }
                    if terminal.is_some() {
                        return Err("message after terminal result".into());
                    }
                    match m["type"].as_str() {
                        Some("before") => {
                            if dispatched >= actions.len()
                                || active.is_some()
                                || protocol::integer(&m["step"]) != Some(dispatched as i64)
                                || !protocol::equal(&m["action"], &actions[dispatched])
                            {
                                return Err("unexpected action checkpoint".into());
                            }
                            record(&m)?;
                            active = Some(dispatched);
                            queue(&write_tx, json!({"type":"ack","step":dispatched}))?;
                            dispatched += 1;
                        }
                        Some("after") => {
                            if active.is_none()
                                || protocol::integer(&m["step"]) != active.map(|n| n as i64)
                            {
                                return Err("unexpected action completion".into());
                            }
                            record(&m)?;
                            completed += 1;
                            active = None;
                        }
                        Some("result") => {
                            let r = &m["result"];
                            protocol::validate_result(r, completed)?;
                            if protocol::integer(&r["steps"]) != Some(completed as i64) {
                                return Err(
                                    "result step count differs from completed actions".into()
                                );
                            }
                            if r["status"] == "pass" && completed != actions.len() {
                                return Err("pass without complete action checkpoints".into());
                            }
                            record(&m)?;
                            terminal = Some(r.clone());
                            write_tx = None;
                        }
                        _ => return Err("unexpected worker message".into()),
                    }
                    Ok(())
                })();
                if let Err(e) = process {
                    error = Some((
                        if e == "worker output budget exceeded" {
                            "output-budget".into()
                        } else {
                            format!("protocol:{e}")
                        },
                        e,
                    ));
                }
            }
        }
        if bytes > options.max_output {
            error.get_or_insert((
                "output-budget".into(),
                "worker output budget exceeded".into(),
            ));
        }
    }
    drop(write_tx);
    // Only the group created above is addressed. Killing descendants also closes
    // inherited pipe ends before joins; a dead worker cannot leave a linker alive.
    drop(rx);
    let mut cleanup_error = None;
    #[cfg(target_os = "linux")]
    if (native_profile
        || error.is_some()
        || exit_status.is_none()
        || exit_status.is_some_and(|s| !s.success()))
        && let Err(e) = native::terminate_owned_group(child.id())
    {
        cleanup_error = Some(e);
    }
    if exit_status.is_none()
        && let Err(e) = child.kill()
    {
        cleanup_error = Some(e.to_string());
    }
    match child.wait() {
        Ok(status) => exit_status = Some(status),
        Err(e) => cleanup_error = Some(e.to_string()),
    }
    let writer_join = input_writer.join();
    let stdout_join = stdout_reader.join();
    let stderr_join = stderr_reader.join();
    if writer_join.is_err() || stdout_join.is_err() || stderr_join.is_err() {
        cleanup_error = Some("worker I/O thread panicked".into());
    }
    let mut result = if let Some((reason, message)) = error {
        protocol::host_error(completed, &reason, &message)
    } else if exit_status.is_some_and(|s| !s.success()) {
        protocol::host_error(completed, "exit", "worker exited unsuccessfully")
    } else {
        terminal.unwrap_or_else(|| {
            protocol::host_error(completed, "missing-result", "worker exited without result")
        })
    };
    if native_root.exists() {
        let pending = native::pending(&native_root);
        if !pending.is_empty() {
            result["nativePending"] = json!(pending);
        }
        if let Err(e) = fs::remove_dir_all(&native_root) {
            cleanup_error = Some(format!("native temporary cleanup: {e}"));
        }
    }
    if let Some(e) = cleanup_error {
        result["issues"]
            .as_array_mut()
            .unwrap()
            .push(json!({"phase":"host-cleanup","name":"WorkerCleanupError","message":e}));
        if result["status"] == "pass" {
            result["status"] = json!("error");
        }
    }
    durable(&options.attempt.join("result.json"), &result)?;
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
    durable(&options.attempt.join("stderr.json"), &json!(stderr))?;
    Ok(Supervised {
        result,
        stderr,
        elapsed_ms: start.elapsed().as_millis(),
        journal_records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn snapshot_uses_open_handle_when_source_path_is_replaced() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = std::env::temp_dir().join(format!(
            "roam-open-snapshot-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("current");
        let replacement = root.join("replacement");
        let a = b"opened executable A\n";
        let b = b"replacement executable B has different bytes\n";
        fs::write(&path, a).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
        let opened = fs::File::open(&path).unwrap();
        let original_inode = opened.metadata().unwrap().ino();
        fs::write(&replacement, b).unwrap();
        fs::set_permissions(&replacement, fs::Permissions::from_mode(0o640)).unwrap();
        fs::rename(&replacement, &path).unwrap();
        assert_ne!(fs::metadata(&path).unwrap().ino(), original_inode);
        assert_eq!(opened.metadata().unwrap().ino(), original_inode);

        let snapshot = freeze_open_file(opened, &root.join("snapshots"), "controlled").unwrap();
        let binary = snapshot.join("roam");
        assert_eq!(fs::read(&binary).unwrap(), a);
        assert_eq!(fs::read(&path).unwrap(), b);
        assert_eq!(
            fs::metadata(&binary).unwrap().permissions().mode() & 0o777,
            0o750
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let manifest = verify_snapshot(&snapshot).unwrap();
        assert_eq!(manifest["sha256"], Fingerprint::of(a).to_hex());
        assert_eq!(
            snapshot.file_name().unwrap().to_str().unwrap(),
            Fingerprint::of(a).to_hex()
        );
        assert_ne!(manifest["sha256"], Fingerprint::of(b).to_hex());

        fs::write(&binary, b"tampered").unwrap();
        assert_eq!(
            verify_snapshot(&snapshot).unwrap_err(),
            "snapshot executable hash mismatch"
        );
        assert_eq!(fs::read(&path).unwrap(), b);
        fs::remove_dir_all(&root).unwrap();
    }
}
