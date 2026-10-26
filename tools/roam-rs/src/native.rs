//! Real source-to-native observation with independent, tool-local models.
use crate::{byte_buffer, domain, host, protocol, resource_closure, scalar};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
use zil_base::{FileId, Fingerprint, ModuleId, SourceUnit};
use zil_driver::{CompileOptions, Optimization, Stage};

static SERIAL: AtomicU64 = AtomicU64::new(0);
pub const EVIDENCE_LIMIT: usize = 64 * 1024;
type ModelFunctions = (
    fn(&Value) -> Result<(), String>,
    fn(&Value) -> Result<Value, String>,
    fn(&Value, bool) -> Result<String, String>,
    &'static str,
);

/// Only call with the pid returned by a Command to which we applied process_group(0).
/// No case, queue record or user-supplied pid is accepted by this helper.
#[cfg(target_os = "linux")]
pub(crate) fn terminate_owned_group(pid: u32) -> Result<(), String> {
    if pid <= 1 {
        return Err("invalid owned process group".into());
    }
    let output = Command::new("/bin/kill")
        .env("LC_ALL", "C")
        .args(["-KILL", "--", &format!("-{pid}")])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success()
        || String::from_utf8_lossy(&output.stderr).contains("No such process")
    {
        Ok(())
    } else {
        Err(format!(
            "owned process-group cleanup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn resolve_clang() -> Result<PathBuf, String> {
    let name = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    let path = PathBuf::from(&name);
    let path = if path.components().count() > 1 || path.is_absolute() {
        path
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|dir| dir.join(&name))
            .find(|p| p.is_file())
            .ok_or("clang not found")?
    };
    fs::canonicalize(path).map_err(|e| format!("resolve clang: {e}"))
}

/// File capture bounds memory throughout execution, and a live size/deadline
/// check terminates the direct child. The owning host subsequently kills any
/// surviving member of its worker process group, including linker descendants.
fn capture(
    command: &mut Command,
    root: &Path,
    label: &str,
    timeout_ms: u64,
) -> Result<Output, String> {
    let stdout = root.join(format!("{label}.stdout"));
    let stderr = root.join(format!("{label}.stderr"));
    let mut child = command
        .stdin(Stdio::null())
        .stdout(File::create_new(&stdout).map_err(|e| e.to_string())?)
        .stderr(File::create_new(&stderr).map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let status = loop {
        let exceeded = [&stdout, &stderr]
            .iter()
            .any(|p| fs::metadata(p).is_ok_and(|m| m.len() > 16_384));
        if exceeded || start.elapsed() >= Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if exceeded {
                "native process output quota exceeded"
            } else {
                "native process time budget exceeded"
            }
            .into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    };
    Ok(Output {
        status,
        stdout: host::bounded_bytes(&mut File::open(stdout).map_err(|e| e.to_string())?, 16_384)?,
        stderr: host::bounded_bytes(&mut File::open(stderr).map_err(|e| e.to_string())?, 16_384)?,
    })
}

/// The compiler invokes this same snapshot as a strictly-shaped clang proxy.
/// Actual clang output is bounded before the existing backend's output() sees it.
pub fn clang_proxy(args: &[String]) -> Result<u8, String> {
    use std::io::Write;
    if std::env::var("ZIL_QC_SUPERVISED").as_deref() != Ok("owned-process-group/1")
        || args.len() != 7
        || args[0] != "--target=x86_64-unknown-linux-gnu"
        || args[1] != "-x"
        || args[2] != "ir"
        || !["-O0", "-O2"].contains(&args[3].as_str())
        || args[5] != "-o"
    {
        return Err("invalid supervised clang proxy invocation".into());
    }
    let root =
        fs::canonicalize(std::env::var_os("ZIL_QC_NATIVE_ROOT").ok_or("missing native root")?)
            .map_err(|e| e.to_string())?;
    let input = fs::canonicalize(&args[4]).map_err(|e| e.to_string())?;
    let relative = input
        .strip_prefix(&root)
        .map_err(|_| "clang input is outside owned root")?;
    let step = relative.components().next().ok_or("missing native step")?;
    let mut dir = root.join(step.as_os_str());
    // The byte profile builds each optimization in its own owned subdirectory.
    // Existing scalar/resource paths retain their original single-step layout.
    if let Some(variant) = relative.components().nth(1)
        && matches!(variant.as_os_str().to_str(), Some("zil-O0" | "zil-O2"))
    {
        dir.push(variant.as_os_str());
    }
    let output_parent = fs::canonicalize(
        Path::new(&args[6])
            .parent()
            .ok_or("missing output parent")?,
    )
    .map_err(|e| e.to_string())?;
    if !output_parent.starts_with(&dir) {
        return Err("clang output is outside owned step".into());
    }
    let pinned = host::read_json_file(&dir.join("clang-identity.json"), EVIDENCE_LIMIT)?;
    let clang = PathBuf::from(pinned["path"].as_str().ok_or("missing pinned clang path")?);
    if !clang.is_absolute()
        || resolve_clang()? != clang
        || host::digest(&clang)? != pinned["sha256"]
    {
        return Err("clang proxy identity drift before compilation".into());
    }
    let mut actual_args = args.to_vec();
    let observer = dir.join("byte-observer.json");
    let observer_pin = if observer.exists() {
        let config = host::read_json_file(&observer, EVIDENCE_LIMIT)?;
        let fail = protocol::integer(&config["failAt"])
            .filter(|n| (0..=64).contains(n))
            .ok_or("invalid observer failure position")? as u8;
        let source = byte_buffer::owned::observer_source(fail)?;
        let path = dir.join("byte-observer.c");
        if config["model"] != byte_buffer::owned::MODEL
            || fs::read(&path).map_err(|e| e.to_string())? != source.as_bytes()
            || config["sha256"] != host::digest(&path)?
        {
            return Err("byte observer differs from frozen worker implementation".into());
        }
        actual_args.extend([
            "-x".into(),
            "c".into(),
            path.to_string_lossy().into_owned(),
            "-Wl,--wrap=malloc".into(),
            "-Wl,--wrap=realloc".into(),
            "-Wl,--wrap=free".into(),
        ]);
        Some((path, config))
    } else {
        None
    };
    let mut command = Command::new(&clang);
    command.args(&actual_args).current_dir(&dir);
    let run = capture(&mut command, &dir, "clang", 5_000);
    let mut recorded = json!({"argv":std::iter::once(clang.to_string_lossy().into_owned()).chain(actual_args).collect::<Vec<_>>(),"clangSha256":pinned["sha256"]});
    if let Some((path, config)) = observer_pin {
        if host::digest(&path)? != config["sha256"] {
            return Err("byte observer changed during compilation".into());
        }
        recorded["byteObserver"] = config;
    }
    if host::digest(&clang)? != pinned["sha256"] {
        recorded["error"] = json!("clang proxy identity drift during compilation");
        host::durable(&dir.join("clang-process.json"), &recorded)?;
        return Err("clang proxy identity drift during compilation".into());
    }
    match run {
        Ok(output) => {
            recorded["exit"] = json!(output.status.code());
            recorded["stdout"] = json!(String::from_utf8_lossy(&output.stdout));
            recorded["stderr"] = json!(String::from_utf8_lossy(&output.stderr));
            host::durable(&dir.join("clang-process.json"), &recorded)?;
            std::io::stdout()
                .write_all(&output.stdout)
                .map_err(|e| e.to_string())?;
            std::io::stderr()
                .write_all(&output.stderr)
                .map_err(|e| e.to_string())?;
            Ok(if output.status.success() { 0 } else { 2 })
        }
        Err(e) => {
            recorded["error"] = json!(e);
            host::durable(&dir.join("clang-process.json"), &recorded)?;
            Err(e)
        }
    }
}

/// A version command is bounded too; its output never enters the wire unbounded.
pub fn identity() -> Result<Value, String> {
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    return Err("scalar-native requires Linux x86_64 process-group supervision".into());
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        use std::os::unix::process::CommandExt;
        let clang = resolve_clang()?;
        let hash = host::digest(&clang)?;
        let root = std::env::temp_dir().join(format!(
            "zil-native-identity-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let result = (|| {
            let stdout = root.join("stdout");
            let stderr = root.join("stderr");
            let private_group =
                std::env::var("ZIL_QC_SUPERVISED").as_deref() != Ok("owned-process-group/1");
            let mut command = Command::new(&clang);
            command
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(File::create(&stdout).map_err(|e| e.to_string())?)
                .stderr(File::create(&stderr).map_err(|e| e.to_string())?);
            if private_group {
                command.process_group(0);
            }
            let mut child = command.spawn().map_err(|e| e.to_string())?;
            let pid = child.id();
            let start = Instant::now();
            let status = loop {
                let exceeded = [&stdout, &stderr]
                    .iter()
                    .any(|p| fs::metadata(p).is_ok_and(|m| m.len() > 16_384));
                if start.elapsed() >= Duration::from_secs(2) || exceeded {
                    let cleanup = if private_group {
                        terminate_owned_group(pid)
                    } else {
                        Ok(())
                    };
                    let _ = child.kill();
                    let _ = child.wait();
                    cleanup?;
                    return Err("clang identity command exceeded time/output budget".into());
                }
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => thread::sleep(Duration::from_millis(5)),
                    Err(e) => {
                        if private_group {
                            let _ = terminate_owned_group(pid);
                        }
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(e.to_string());
                    }
                }
            };
            if private_group {
                terminate_owned_group(pid)?;
            }
            if !status.success() {
                return Err("clang --version failed".into());
            }
            let version =
                host::bounded_bytes(&mut File::open(&stdout).map_err(|e| e.to_string())?, 16_384)?;
            Ok(
                json!({"path":clang,"sha256":hash,"version":String::from_utf8_lossy(&version).trim(),"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"boundary":"Clang executable and version; OS, CPU, linker and dynamic libraries are not frozen"}),
            )
        })();
        let cleanup = fs::remove_dir_all(&root).map_err(|e| e.to_string());
        match (result, cleanup) {
            (Ok(v), Ok(())) => Ok(v),
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    }
}
pub fn bind_identity(trace: &mut Value, identity: &Value) -> Result<(), String> {
    if let Some(previous) = trace.get("meta").and_then(|m| m.get("nativeToolchain")) {
        if previous != identity {
            return Err("recorded native toolchain identity differs".into());
        }
    } else {
        if trace.get("meta").is_none() {
            trace["meta"] = json!({});
        }
        trace["meta"]["nativeToolchain"] = identity.clone();
    }
    Ok(())
}

fn observe(
    source: &str,
    root: &Path,
    toolchain: &Value,
    resource_model: bool,
    optimization: Optimization,
) -> Result<(Value, Value), String> {
    let input_path = root.join("input.zil");
    fs::write(&input_path, source).map_err(|e| e.to_string())?;
    let clang = PathBuf::from(toolchain["path"].as_str().ok_or("invalid clang identity")?);
    if host::digest(&clang)? != toolchain["sha256"] {
        return Err("clang changed before compilation".into());
    }
    host::durable(&root.join("clang-identity.json"), toolchain)?;
    let input = SourceUnit::new(FileId(0), ModuleId(0), input_path.to_string_lossy(), source);
    let options = CompileOptions {
        stop_after: Stage::Build,
        output: Some(root.join("program")),
        entry: if resource_model {
            None
        } else {
            Some("value".into())
        },
        clang: Some(std::env::current_exe().map_err(|e| e.to_string())?),
        optimization,
        ..CompileOptions::default()
    };
    let report = zil_driver::compile_source(&input, &options);
    let compile: Value = serde_json::from_str(&report.to_json()).map_err(|e| e.to_string())?;
    let mut detail = json!({"source":source,"sourceSha256":Fingerprint::of(source.as_bytes()).to_hex(),"compile":compile});
    if root.join("clang-process.json").exists() {
        detail["clangProcess"] =
            host::read_json_file(&root.join("clang-process.json"), EVIDENCE_LIMIT)?;
    }
    if report.status.as_str() != "Success" {
        let status = report.status.as_str();
        if status == "InfraError" {
            return Err(format!(
                "native compiler infrastructure: {}",
                report.to_json()
            ));
        }
        let observed = if status == "SafeReject"
            && compile["phase"] == "ct"
            && compile["diagnostics"].as_array().is_some_and(|ds| {
                ds.iter()
                    .any(|d| d["phase"] == "ct" && d["code"] == "E-CT-ARITHMETIC")
            }) {
            json!({"kind":"ct-reject"})
        } else {
            json!({"kind":"compile-result","category":status,"phase":compile["phase"],"diagnostics":compile["diagnostics"]})
        };
        return Ok((observed, detail));
    }
    let executable = report
        .artifacts
        .get("executable")
        .ok_or("successful compile without executable")?;
    let llvm = report
        .artifacts
        .get("llvm")
        .ok_or("successful compile without LLVM IR")?;
    detail["llvmSha256"] = json!(host::digest(llvm)?);
    detail["executableSha256"] = json!(host::digest(executable)?);
    // The worker's own process group owns this child and all compiler descendants.
    // Generated bounded programs have one integer-output wrapper; the outer
    // supervisor enforces the real deadline even if compiler/native code hangs.
    let mut command = Command::new(executable);
    command.current_dir(root);
    let run = capture(&mut command, root, "runtime", 3_000)?;
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        run.status.signal()
    };
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    detail["run"] = json!({"argv":[executable],"exit":run.status.code(),"signal":signal,"stdout":stdout,"stderr":stderr});
    if root.join("byte-observer.json").exists()
        && run.status.code() == Some(95)
        && stderr == "observer: unplanned allocation failure\n"
    {
        return Err(
            json!({"environmentError":"unplanned allocation failure","partialEvidence":detail})
                .to_string(),
        );
    }
    let observed = if resource_model && signal.is_none() {
        json!({"kind":"lifecycle","stdout":stdout,"stderr":stderr,"exit":run.status.code()})
    } else if signal == Some(4) {
        json!({"kind":"trap"})
    } else if run.status.success() {
        match stdout.trim().parse::<i128>() {
            Ok(n) if stdout.trim() == n.to_string() => {
                json!({"kind":"value","value":n.to_string()})
            }
            _ => json!({"kind":"unexpected-runtime","run":detail["run"]}),
        }
    } else {
        json!({"kind":"unexpected-runtime","run":detail["run"]})
    };
    if host::digest(&clang)? != toolchain["sha256"] {
        return Err("clang changed during compilation".into());
    }
    Ok((observed, detail))
}

fn observe_c(
    source: &str,
    root: &Path,
    toolchain: &Value,
    level: &str,
) -> Result<(Value, Value), String> {
    let clang = PathBuf::from(toolchain["path"].as_str().ok_or("invalid clang identity")?);
    if resolve_clang()? != clang || host::digest(&clang)? != toolchain["sha256"] {
        return Err("C reference toolchain identity drift".into());
    }
    let input = root.join("reference.c");
    let executable = root.join("reference");
    fs::write(&input, source).map_err(|e| e.to_string())?;
    let args = vec![
        "-std=c11".to_string(),
        "-Wall".into(),
        "-Wextra".into(),
        "-Werror".into(),
        format!("-{level}"),
        input.to_string_lossy().into_owned(),
        "-o".into(),
        executable.to_string_lossy().into_owned(),
    ];
    let built = capture(
        Command::new(&clang).args(&args).current_dir(root),
        root,
        "compile",
        5_000,
    )?;
    let compile = json!({"argv":std::iter::once(clang.to_string_lossy().into_owned()).chain(args).collect::<Vec<_>>(),"exit":built.status.code(),"stdout":String::from_utf8_lossy(&built.stdout),"stderr":String::from_utf8_lossy(&built.stderr),"clangSha256":toolchain["sha256"]});
    if !built.status.success() {
        return Err(format!("C reference failed: {compile}"));
    }
    let run = capture(
        Command::new(&executable).current_dir(root),
        root,
        "runtime",
        3_000,
    )?;
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        run.status.signal()
    };
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    let observation = json!({"kind":"lifecycle","stdout":String::from_utf8_lossy(&run.stdout),"stderr":String::from_utf8_lossy(&run.stderr),"exit":run.status.code()});
    if host::digest(&clang)? != toolchain["sha256"] {
        return Err("C reference toolchain changed during execution".into());
    }
    let detail = json!({"compile":compile,"sourceSha256":Fingerprint::of(source.as_bytes()).to_hex(),"executableSha256":host::digest(&executable)?,"run":{"argv":[executable],"signal":signal,"observation":observation}});
    Ok((observation, detail))
}

fn observe_bytes(
    source: &str,
    reference: &str,
    root: &Path,
    toolchain: &Value,
    owned_program: Option<&Value>,
) -> Result<(Value, Value), String> {
    let mut observations = json!({});
    let mut details = json!({"source":source,"cReference":reference,"variants":{}});
    let result = (|| {
        for (name, level) in [("O0", Optimization::O0), ("O2", Optimization::O2)] {
            for language in ["zil", "c"] {
                let key = format!("{language}-{name}");
                let dir = root.join(&key);
                fs::create_dir(&dir).map_err(|e| e.to_string())?;
                if language == "zil"
                    && let Some(program) = owned_program
                {
                    let fail = byte_buffer::owned::fail_at(program)?;
                    let source = byte_buffer::owned::observer_source(fail)?;
                    let path = dir.join("byte-observer.c");
                    fs::write(&path, &source).map_err(|e| e.to_string())?;
                    host::durable(
                        &dir.join("byte-observer.json"),
                        &json!({"model":byte_buffer::owned::MODEL,"failAt":fail,"controlledAllocationFailure":fail != 0,"sha256":host::digest(&path)?}),
                    )?;
                    details["byteObserverSource"] = json!(source);
                }
                let (observation, mut detail) = if language == "zil" {
                    observe(source, &dir, toolchain, true, level)?
                } else {
                    observe_c(reference, &dir, toolchain, name)?
                };
                // Preserve the source once, while retaining each real command,
                // compile receipt, output and executable/LLVM identity.
                detail.as_object_mut().unwrap().remove("source");
                observations[&key] = observation;
                details["variants"][&key] = detail;
            }
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        return Err(
            json!({"error":error,"partialEvidence":details,"partialObservations":observations})
                .to_string(),
        );
    }
    Ok((observations, details))
}

pub fn execute(
    trace: &Value,
    options: &domain::Options,
    mut before: impl FnMut(usize, &Value) -> Result<(), String>,
    mut after: impl FnMut(usize) -> Result<(), String>,
) -> Value {
    if let Err(e) = protocol::validate_case(trace) {
        return protocol::issue("invalid", 0, "case", -1, "InvalidCase", &e);
    }
    let incomplete =
        |n, reason| json!({"status":"incomplete","steps":n,"issues":[],"reason":reason});
    let start = Instant::now();
    let resource_model = trace["contract"] == resource_closure::CONTRACT;
    let owned_byte_model = trace["contract"] == byte_buffer::owned::CONTRACT;
    let byte_model = trace["contract"] == byte_buffer::CONTRACT || owned_byte_model;
    let (validate, expected_program, source_program, property): ModelFunctions = if owned_byte_model
    {
        (
            byte_buffer::owned::validate,
            byte_buffer::owned::expected,
            byte_buffer::owned::source,
            byte_buffer::owned::PROPERTY,
        )
    } else if byte_model {
        (
            byte_buffer::validate,
            byte_buffer::expected,
            byte_buffer::source,
            byte_buffer::PROPERTY,
        )
    } else if resource_model {
        (
            resource_closure::validate,
            resource_closure::expected,
            resource_closure::source,
            resource_closure::PROPERTY,
        )
    } else {
        (
            scalar::validate,
            scalar::expected,
            scalar::source,
            scalar::PROPERTY,
        )
    };
    if options.budget_ms == 0 {
        return incomplete(0, "time-budget");
    }
    if !cfg!(all(target_os = "linux", target_arch = "x86_64"))
        || std::env::var("ZIL_QC_SUPERVISED").as_deref() != Ok("owned-process-group/1")
    {
        return protocol::issue(
            "unsupported",
            0,
            "init",
            -1,
            "Unsupported",
            "native profile requires owned Linux x86_64 process-group supervision",
        );
    }
    if trace["requires"]
        .as_array()
        .is_some_and(|xs| !xs.is_empty())
        || trace["schedule"]
            .as_array()
            .is_some_and(|xs| !xs.is_empty())
    {
        return protocol::issue(
            "unsupported",
            0,
            "init",
            -1,
            "Unsupported",
            "explicit capabilities/schedule not implemented",
        );
    }
    if trace.get("initial").is_some_and(|v| !v.is_null()) {
        return protocol::issue(
            "invalid",
            0,
            "init",
            -1,
            "InvalidCase",
            "native profile has no initial state",
        );
    }
    let actions = trace["steps"].as_array().unwrap();
    if actions.is_empty() {
        return incomplete(0, "zero-execution");
    }
    let toolchain = match identity() {
        Ok(v) => v,
        Err(e) => return protocol::issue("error", 0, "native-toolchain", -1, "ToolchainError", &e),
    };
    if trace
        .get("meta")
        .and_then(|m| m.get("nativeToolchain"))
        .is_some_and(|v| v != &toolchain)
    {
        return protocol::issue(
            "error",
            0,
            "native-toolchain",
            -1,
            "IdentityMismatch",
            "recorded clang identity differs",
        );
    }
    let Some(root) = std::env::var_os("ZIL_QC_NATIVE_ROOT").map(PathBuf::from) else {
        return protocol::issue(
            "error",
            0,
            "native-init",
            -1,
            "Error",
            "missing supervisor-owned output",
        );
    };
    if let Err(e) = fs::create_dir(&root) {
        return protocol::issue("error", 0, "native-init", -1, "Error", &e.to_string());
    }
    let mut result = protocol::result("pass", 0);
    let mut evidence = Vec::new();
    let mut history = Vec::new();
    let mut steps = 0;
    for (i, action) in actions.iter().enumerate() {
        if start.elapsed() >= Duration::from_millis(options.budget_ms) {
            result = incomplete(steps, "time-budget");
            break;
        }
        if steps
            >= options.max_steps.min(if byte_model {
                byte_buffer::MAX_PROGRAMS
            } else {
                scalar::MAX_PROGRAMS
            })
        {
            result = incomplete(steps, "step-budget");
            break;
        }
        if action["op"] != "run"
            || validate(&action["program"]).is_err()
            || (byte_model && !owned_byte_model && action["program"].get("model").is_some())
        {
            result = protocol::issue(
                "invalid",
                steps,
                "validate",
                i as i64,
                "InvalidCase",
                "invalid bounded native program",
            );
            break;
        }
        let model = expected_program(&action["program"]).unwrap();
        let expected = if byte_model {
            json!({"zil-O0":model,"zil-O2":model,"c-O0":model,"c-O2":model})
        } else {
            model
        };
        let source = source_program(&action["program"], options.fault).unwrap();
        if let Err(e) = before(i, action) {
            result = protocol::issue("error", steps, "before-step", i as i64, "Error", &e);
            break;
        }
        let dir = root.join(format!("step-{i:04}"));
        let real = (|| {
            fs::create_dir(&dir).map_err(|e| e.to_string())?;
            host::durable(
                &dir.join("pending.json"),
                &json!({"step":i,"source":source,"expected":expected,"controlledMutation":options.fault}),
            )?;
            if byte_model {
                observe_bytes(
                    &source,
                    &if owned_byte_model {
                        byte_buffer::owned::c_source(&action["program"])?
                    } else {
                        byte_buffer::c_source(&action["program"])?
                    },
                    &dir,
                    &toolchain,
                    owned_byte_model.then_some(&action["program"]),
                )
            } else {
                observe(&source, &dir, &toolchain, resource_model, Optimization::O0)
            }
        })();
        let (observed, mut detail) = match real {
            Ok(v) => v,
            Err(e) => {
                result =
                    protocol::issue("error", steps, "native-step", i as i64, "NativeError", &e);
                break;
            }
        };
        steps += 1;
        detail["step"] = json!(i);
        detail["expected"] = expected.clone();
        detail["observed"] = observed.clone();
        detail["controlledMutation"] = json!(options.fault);
        if owned_byte_model {
            detail["modelContract"] = json!(byte_buffer::owned::CONTRACT);
            detail["model"] = json!(byte_buffer::owned::MODEL);
            detail["controlledAllocationFailure"] = json!(action["program"]["failAt"] != 0);
        }
        if detail.to_string().len() > EVIDENCE_LIMIT {
            result = protocol::issue(
                "error",
                steps,
                "evidence",
                i as i64,
                "Quota",
                "native evidence quota exceeded",
            );
            if let Err(e) = after(i) {
                result["reason"] = json!(e);
            }
            break;
        }
        evidence.push(detail);
        if let Err(e) = after(i) {
            result = protocol::issue("error", steps, "after-step", i as i64, "Error", &e);
            break;
        }
        if options.history {
            history
                .push(json!({"phase":"complete","step":i,"expected":expected,"observed":observed}));
        }
        if expected != observed {
            result = protocol::failure(property, steps, i as i64, expected, observed);
            break;
        }
        result["steps"] = json!(steps);
    }
    if result["status"] == "pass" && start.elapsed() >= Duration::from_millis(options.budget_ms) {
        result = incomplete(steps, "time-budget");
    }
    result["nativeToolchain"] = toolchain;
    result["nativeEvidence"] = json!(evidence);
    if options.history {
        result["history"] = json!(history);
    }
    // Host also cleans this exact owned directory after a killed worker.
    if let Err(e) = fs::remove_dir_all(&root) {
        result["issues"]
            .as_array_mut()
            .unwrap()
            .push(json!({"phase":"cleanup","name":"CleanupError","message":e.to_string()}));
        if result["status"] == "pass" {
            result["status"] = json!("error");
        }
    }
    result
}

/// Recover bounded source/expected records when the worker died mid-action.
pub(crate) fn pending(root: &Path) -> Vec<Value> {
    (0..scalar::MAX_PROGRAMS)
        .filter_map(|i| {
            host::read_json_file(
                &root.join(format!("step-{i:04}/pending.json")),
                EVIDENCE_LIMIT,
            )
            .ok()
        })
        .collect()
}
