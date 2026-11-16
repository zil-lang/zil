//! Real CLI policy, native consumers, current verification and cache isolation.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-work-profile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn command(action: &str, input: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zil"))
        .arg(action)
        .arg(input)
        .args(args)
        .output()
        .unwrap()
}
fn text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn successful(output: &Output) -> String {
    assert!(output.status.success(), "{output:?}");
    let report = text(output);
    assert!(report.contains("\"verification_runs\":1"), "{report}");
    report
}
fn policy(report: &str, profile: &str) {
    let (revision, steps) = match profile {
        "alpha" => ("alpha-64m-v2", 64_000_000),
        "standard" => ("standard-10m-v1", 10_000_000),
        _ => unreachable!(),
    };
    assert!(
        report.contains(&format!("\"work_profile\":\"{profile}\"")),
        "{report}"
    );
    assert!(
        report.contains(&format!("\"work_profile_revision\":\"{revision}\"")),
        "{report}"
    );
    let expected = format!(
        "\"effective_limits\":{{\"max_source_bytes\":16777216,\"max_tokens\":1000000,\"max_nodes\":1000000,\"max_functions\":10000,\"max_blocks\":100000,\"max_instructions\":1000000,\"max_locals\":100000,\"max_values\":1000000,\"max_analysis_steps\":{steps},\"max_ct_steps\":1000000,\"max_call_depth\":256}}"
    );
    assert!(report.contains(&expected), "{report}");
}
fn resident(input: &Path, profile: Option<&str>, cache: Option<&Path>) -> Vec<String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zil"));
    command
        .arg("check")
        .arg(input)
        .args(["--json", "--resident"]);
    if let Some(profile) = profile {
        command.args(["--work-profile", profile]);
    }
    if let Some(cache) = cache {
        command.arg("--semantic-cache").arg(cache);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"compile\ncompile\nquit\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let all = text(&output);
    let lines = all.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3, "{all}");
    assert!(lines[0].contains("\"ready\":true"), "{all}");
    lines[1..].iter().map(|line| (*line).to_owned()).collect()
}

#[test]
fn invalid_or_repeated_profiles_are_usage_errors_before_compilation() {
    for action in ["check", "build", "run", "test"] {
        for args in [
            vec!["--json", "--work-profile", "unbounded"],
            vec![
                "--json",
                "--work-profile",
                "alpha",
                "--work-profile",
                "standard",
            ],
            vec![
                "--json",
                "--work-profile",
                "alpha",
                "--work-profile",
                "alpha",
            ],
            vec!["--json", "--work-profile"],
        ] {
            let output = command(action, Path::new("not-read.zil"), &args);
            assert_eq!(output.status.code(), Some(64), "{output:?}");
            assert!(text(&output).contains("E-CLI-ARGUMENT"), "{output:?}");
            assert!(!text(&output).contains("E-SOURCE-READ"), "{output:?}");
        }
    }
}

#[test]
fn profiles_reach_check_build_run_tests_and_resident_without_skipping_verify() {
    let dir = Scratch::new();
    fs::write(dir.0.join("Zil.toml"), "[package]\nroot = \"main.zil\"\n").unwrap();
    let source = dir.0.join("main.zil");
    fs::write(
        &source,
        "fn main()->i32{return 0;}\n#[test]\nfn zero()->bool{return true;}\n",
    )
    .unwrap();
    assert!(command("lock", &dir.0, &[]).status.success());
    let default = successful(&command("check", &source, &["--json"]));
    policy(&default, "alpha");
    let standard = successful(&command(
        "check",
        &source,
        &["--json", "--work-profile", "standard"],
    ));
    policy(&standard, "standard");
    let help = Command::new(env!("CARGO_BIN_EXE_zil"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success(), "{help:?}");
    assert!(
        text(&help).contains("work quota (default: alpha)"),
        "{help:?}"
    );
    for (action, explicit) in ["check", "build", "run", "test"]
        .into_iter()
        .flat_map(|action| [(action, false), (action, true)])
    {
        let output = dir.0.join("program");
        let mut args = vec!["--json"];
        if explicit {
            args.extend(["--work-profile", "alpha"]);
        }
        if action == "build" {
            args.extend(["--output", output.to_str().unwrap()]);
        }
        let report = successful(&command(action, &dir.0, &args));
        policy(&report, "alpha");
        if action == "run" {
            assert!(report.starts_with("{\"compile\":"), "{report}");
            assert!(report.contains("\"exit_code\":0"), "{report}");
        }
        if action == "test" {
            assert!(report.contains("\"passed\":1,\"total\":1"), "{report}");
            assert_eq!(
                report.matches("\"effective_limits\":").count(),
                2,
                "{report}"
            );
        }
    }
    for selected in [None, Some("standard"), Some("alpha")] {
        let profile = selected.unwrap_or("alpha");
        let reports = resident(&source, selected, None);
        for report in &reports {
            policy(report, profile);
            assert!(report.contains("\"status\":\"Success\""), "{report}");
            assert!(report.contains("\"verification_runs\":1"), "{report}");
        }
        assert!(
            reports[1].contains("\"disposition\":\"reused\""),
            "{:?}",
            reports
        );
    }
    fs::write(&source, "fn main()->i32{return absent;}\n").unwrap();
    for profile in ["standard", "alpha"] {
        let bad = command("check", &source, &["--json", "--work-profile", profile]);
        assert_eq!(bad.status.code(), Some(1), "{bad:?}");
        let report = text(&bad);
        policy(&report, profile);
        assert!(report.contains("\"status\":\"SafeReject\""), "{report}");
        assert!(!report.contains("\"phase\":\"driver\""), "{report}");
    }
}

#[test]
fn lower_quota_cache_and_resident_match_fresh_complete_source_compilation() {
    // This complete five-module input crosses the 10M boundary in the frozen
    // Alpha candidate. Future real optimizations may make standard succeed;
    // compare with cold standard rather than requiring rejection forever.
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/mainline/state-replacement/main.zil");
    let dir = Scratch::new();
    let fresh = command("check", &source, &["--json", "--work-profile", "standard"]);
    let fresh_text = text(&fresh);
    policy(&fresh_text, "standard");
    assert!(
        fresh.status.success() || fresh.status.code() == Some(4),
        "{fresh:?}"
    );
    let cache = dir.0.join("semantic");
    let cache_arg = cache.to_str().unwrap();
    let alpha = command(
        "check",
        &source,
        &[
            "--json",
            "--work-profile",
            "alpha",
            "--semantic-cache",
            cache_arg,
        ],
    );
    policy(&successful(&alpha), "alpha");
    let lower = command(
        "check",
        &source,
        &[
            "--json",
            "--work-profile",
            "standard",
            "--semantic-cache",
            cache_arg,
        ],
    );
    assert_eq!(
        lower.status.code(),
        fresh.status.code(),
        "fresh={fresh:?}, cached={lower:?}"
    );
    let expected = if fresh.status.success() {
        "Success"
    } else {
        "ResourceLimit"
    };
    for report in
        std::iter::once(text(&lower)).chain(resident(&source, Some("standard"), Some(&cache)))
    {
        policy(&report, "standard");
        assert!(
            report.contains(&format!("\"status\":\"{expected}\"")),
            "{report}"
        );
        if expected == "Success" {
            assert!(report.contains("\"verification_runs\":1"), "{report}");
        }
    }
}
