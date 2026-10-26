use roam::{campaign, domain, host, profile::Profile, protocol};
use serde_json::json;
use std::{path::PathBuf, process::ExitCode};

fn argument(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .find_map(|a| a.strip_prefix(&format!("--{name}=")).map(str::to_owned))
}
fn number(args: &[String], name: &str, default: u64) -> Result<u64, String> {
    argument(args, name).map_or(Ok(default), |v| {
        v.parse().map_err(|_| format!("invalid {name}"))
    })
}
fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
fn run() -> Result<u8, String> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|s| s == "--target=x86_64-unknown-linux-gnu")
    {
        return roam::native::clang_proxy(&args);
    }
    if args.first().is_some_and(|s| s == "hunt" || s == "issue") {
        return roam::pipeline::cli(&args);
    }
    if args.is_empty() {
        return Err(
            "usage: roam worker|campaign|replay|generate|verify-snapshot|hunt|issue [--key=value]"
                .into(),
        );
    }
    let command = args.remove(0);
    let allowed = match command.as_str() {
        "worker" => &["fault", "max-steps", "budget-ms"][..],
        "campaign" => &["out", "commit", "allow-dirty"][..],
        "replay" => &[
            "case",
            "out",
            "snapshot",
            "fault",
            "timeout-ms",
            "max-steps",
            "budget-ms",
        ][..],
        "generate" => &["seed", "steps", "profile"][..],
        "verify-snapshot" => &["snapshot"][..],
        _ => return Err("unknown command".into()),
    };
    let mut seen = std::collections::HashSet::new();
    for arg in &args {
        let (key, _) = arg
            .strip_prefix("--")
            .and_then(|s| s.split_once('='))
            .ok_or("expected --key=value")?;
        if !allowed.contains(&key) || !seen.insert(key) {
            return Err("unknown or duplicate option".into());
        }
    }
    let fault_name = argument(&args, "fault").unwrap_or_else(|| "none".into());
    let fault = match fault_name.as_str() {
        "none" => false,
        "stale-after-edit" | "scalar-sub-to-add" | "closure-omit-call" | "bytes-omit-push" => true,
        _ => return Err("unknown fault".into()),
    };
    match command.as_str() {
        "worker" => {
            host::worker(
                domain::Options {
                    max_steps: number(&args, "max-steps", 256)? as usize,
                    budget_ms: number(&args, "budget-ms", 10_000)?,
                    history: true,
                    fault,
                },
                &fault_name,
            )?;
            Ok(0)
        }
        "generate" => {
            let seed: i64 = argument(&args, "seed")
                .ok_or("seed required")?
                .parse()
                .map_err(|_| "invalid seed")?;
            if seed.unsigned_abs() > 9_007_199_254_740_991 {
                return Err("seed exceeds JSON safe integer range".into());
            }
            let profile =
                Profile::named(argument(&args, "profile").as_deref().unwrap_or("session"))?;
            let steps = number(&args, "steps", profile.default_steps() as u64)?;
            if steps > profile.max_steps() as u64 || (profile.is_native() && steps == 0) {
                return Err("generated actions exceed profile budget or are empty".into());
            }
            println!("{}", profile.generate(seed, steps as usize));
            Ok(0)
        }
        "campaign" => {
            let out = PathBuf::from(argument(&args, "out").ok_or("new --out directory required")?);
            let git = |arguments: &[&str]| -> Result<String, String> {
                let output = std::process::Command::new("git")
                    .args(arguments)
                    .output()
                    .map_err(|e| e.to_string())?;
                if !output.status.success() {
                    return Err(String::from_utf8_lossy(&output.stderr).into_owned());
                }
                String::from_utf8(output.stdout)
                    .map(|v| v.trim().to_string())
                    .map_err(|e| e.to_string())
            };
            let commit = git(&["rev-parse", "HEAD"])?;
            if argument(&args, "commit").is_some_and(|v| v != commit) {
                return Err("declared source commit differs from Git HEAD".into());
            }
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
            let allow_dirty = match argument(&args, "allow-dirty").as_deref() {
                None | Some("false") => false,
                Some("true") => true,
                _ => return Err("allow-dirty must be true or false".into()),
            };
            if dirty && !allow_dirty {
                return Err("commit implementation before formal campaign; --allow-dirty=true is development only".into());
            }
            let report = campaign::run(&out, &commit, dirty)?;
            println!(
                "{}",
                json!({"status":report["status"],"report":out.join("report.json"),"normal_paths":report["normal_paths"],"normal_actions":report["normal_actions"],"elapsed_ms":report["elapsed_ms"],"error":report.get("error")})
            );
            Ok(if report["status"] == "pass" { 0 } else { 1 })
        }
        "verify-snapshot" => {
            let dir = PathBuf::from(argument(&args, "snapshot").ok_or("snapshot required")?);
            println!("{}", host::verify_snapshot(&dir)?);
            Ok(0)
        }
        "replay" => {
            let case = PathBuf::from(argument(&args, "case").ok_or("case required")?);
            let trace = host::read_json_file(&case, protocol::MAX_CASE)?;
            let out = PathBuf::from(argument(&args, "out").ok_or("new out directory required")?);
            let exe = if let Some(s) = argument(&args, "snapshot") {
                let dir = PathBuf::from(s);
                host::verify_snapshot(&dir)?;
                dir.join("roam")
            } else {
                std::env::current_exe().map_err(|e| e.to_string())?
            };
            let run = host::supervise(
                &trace,
                host::HostOptions {
                    command: &exe,
                    args: vec![
                        "worker".into(),
                        format!("--fault={}", fault_name),
                        format!("--max-steps={}", number(&args, "max-steps", 256)?),
                        format!("--budget-ms={}", number(&args, "budget-ms", 10_000)?),
                    ],
                    timeout_ms: number(&args, "timeout-ms", 10_000)?,
                    max_output: protocol::MAX_FRAME,
                    attempt: &out,
                },
            )?;
            println!("{}", campaign::brief(&run.result));
            Ok(match run.result["status"].as_str() {
                Some("pass") => 0,
                Some("fail") => 1,
                _ => 2,
            })
        }
        _ => unreachable!(),
    }
}
