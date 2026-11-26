//! Dual-backend (`clang-o0`, `clang-o2`, and `llvmapi-o0`/`llvmapi-o2` when
//! `llvm-api` is enabled) end-to-end native integration tests for the eighth
//! everyday multi-module ZIL application:
//!   `examples/everyday-apps/async-task-scheduler`
//!
//! Exercises bounded-concurrency cooperative round-robin `Task` scheduling,
//! per-job and global tick-budget cancellation (`task_request_cancel` ->
//! `task_poll_close` -> `task_finish_destroy`), protocol-verified `JobHandle`
//! state transitions, heterogeneous `Store<OwnedDyn<dyn EventSink>>` telemetry,
//! CLI argument & manifest parsing, cancellation stress rounds, and paired
//! nearest async/task safety-negative controls.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization,
    ProjectManifest, Stage,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zil-everyday-app-async-scheduler-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn dual_configs() -> Vec<(&'static str, NativeBackend, Optimization)> {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }
    configs
}

fn build_project_dual(app_dir: &Path, out_dir: &Path, tag: &str) -> Vec<(&'static str, PathBuf)> {
    let manifest = ProjectManifest::open(app_dir, &CompileOptions::default().limits)
        .unwrap_or_else(|e| panic!("ProjectManifest::open({}) failed: {e:?}", app_dir.display()));
    let manifest = Arc::new(manifest);
    let mut built = Vec::new();
    for (label, backend, opt) in dual_configs() {
        let exe = out_dir.join(format!("bin-{tag}-{label}{}", std::env::consts::EXE_SUFFIX));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            project: Some(Arc::clone(&manifest)),
            build_inputs: manifest.fingerprint,
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(&manifest.entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            app_dir.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);
        built.push((label, exe));
    }
    built
}

fn run_exe(exe: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(exe)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn async_task_scheduler_demo_batch_and_cancel_stress_dual_backend_native() {
    let dir = temp_dir("native");
    let app_dir = repo_root().join("examples/everyday-apps/async-task-scheduler");
    let built = build_project_dual(&app_dir, &dir, "async-task-scheduler");

    let manifest_path = dir.join("jobs.txt");
    std::fs::write(
        &manifest_path,
        "# Async job manifest: <id> <steps> <weight> or <steps> <weight>\n\
         \n\
         10 1 4\n\
         20 5 2\n\
         30 2 5\n\
         0 9\n",
    )
    .expect("write manifest");

    let bad_manifest_path = dir.join("bad.txt");
    std::fs::write(&bad_manifest_path, "10 not-a-number 4\n").expect("write bad manifest");

    let missing_path = dir.join("does-not-exist.txt");

    let mut baseline_demo: Option<String> = None;
    let mut baseline_batch_budget: Option<String> = None;
    let mut baseline_batch_full: Option<String> = None;

    for (label, exe) in &built {
        // 1. Default demo mode: 4 completed, 1 cancelled mid-flight on budget 4.
        let (code, stdout, stderr) = run_exe(exe, &[]);
        assert_eq!(
            code, 0,
            "{label} demo exit failed: stdout={stdout} stderr={stderr}"
        );
        assert!(stderr.is_empty(), "{label} demo stderr: {stderr}");
        assert!(
            stdout.contains("JOB 1 COMPLETED value=10 ticks=3\n"),
            "{label} missing job 1 line:\n{stdout}"
        );
        assert!(
            stdout.contains("JOB 2 CANCELLED ticks=4\n"),
            "{label} missing job 2 cancel line:\n{stdout}"
        );
        assert!(
            stdout.contains("JOB 3 COMPLETED value=8 ticks=2\n"),
            "{label} missing job 3 line:\n{stdout}"
        );
        assert!(
            stdout.contains("JOB 4 COMPLETED value=28 ticks=4\n"),
            "{label} missing job 4 line:\n{stdout}"
        );
        assert!(
            stdout.contains("JOB 5 COMPLETED value=5 ticks=1\n"),
            "{label} missing job 5 line:\n{stdout}"
        );
        assert!(
            stdout.contains("SUMMARY completed=4 cancelled=1 "),
            "{label} unexpected demo summary:\n{stdout}"
        );
        if let Some(prev) = &baseline_demo {
            assert_eq!(
                &stdout, prev,
                "{label} demo output diverged across backends"
            );
        } else {
            baseline_demo = Some(stdout);
        }

        // 2. Batch manifest with per-job budget=3: job 20 (5 steps -> 6 ticks) is cancelled,
        //    jobs 10, 30, and 4 (default id 4, 0 steps) complete cleanly.
        let m_str = manifest_path.to_str().expect("utf8 path");
        let (b_code, b_out, b_err) = run_exe(exe, &["batch", m_str, "3"]);
        assert_eq!(b_code, 0, "{label} batch budget=3 failed: {b_out} {b_err}");
        assert!(b_err.is_empty());
        assert!(
            b_out.contains("JOB 10 COMPLETED value=14 ticks=2\n"),
            "{label} batch job 10:\n{b_out}"
        );
        assert!(
            b_out.contains("JOB 20 CANCELLED ticks=3\n"),
            "{label} batch job 20 cancel:\n{b_out}"
        );
        assert!(
            b_out.contains("JOB 30 COMPLETED value=45 ticks=3\n"),
            "{label} batch job 30:\n{b_out}"
        );
        assert!(
            b_out.contains("JOB 4 COMPLETED value=4 ticks=1\n"),
            "{label} batch job 4:\n{b_out}"
        );
        assert!(
            b_out.contains("SUMMARY completed=3 cancelled=1 "),
            "{label} batch budget summary:\n{b_out}"
        );
        if let Some(prev) = &baseline_batch_budget {
            assert_eq!(&b_out, prev, "{label} batch budget output diverged");
        } else {
            baseline_batch_budget = Some(b_out);
        }

        // 3. Batch manifest with unlimited per-job budget=0: all 4 jobs complete.
        let (f_code, f_out, f_err) = run_exe(exe, &["batch", m_str, "0"]);
        assert_eq!(f_code, 0, "{label} batch unlimited failed: {f_out} {f_err}");
        assert!(f_err.is_empty());
        assert!(
            f_out.contains("JOB 20 COMPLETED value=50 ticks=6\n"),
            "{label} batch job 20 completed:\n{f_out}"
        );
        assert!(
            f_out.contains("SUMMARY completed=4 cancelled=0 "),
            "{label} batch unlimited summary:\n{f_out}"
        );
        if let Some(prev) = &baseline_batch_full {
            assert_eq!(&f_out, prev, "{label} batch unlimited output diverged");
        } else {
            baseline_batch_full = Some(f_out);
        }

        // 4. Cancel-stress mode: 40 spawn -> partial poll -> cancel -> close -> destroy rounds.
        let (s_code, s_out, s_err) = run_exe(exe, &["cancel-stress", "40"]);
        assert_eq!(s_code, 0, "{label} cancel-stress failed: {s_out} {s_err}");
        assert!(s_err.is_empty());
        assert_eq!(
            s_out, "STRESS cancelled=40\n",
            "{label} cancel-stress output"
        );

        // 5. CLI error paths return clean non-zero exits without crashing.
        let (e_miss, _, _) = run_exe(exe, &["batch", missing_path.to_str().unwrap()]);
        assert_eq!(e_miss, 2, "{label} missing manifest should exit 2");
        let (e_bad, _, _) = run_exe(exe, &["batch", bad_manifest_path.to_str().unwrap()]);
        assert_eq!(e_bad, 3, "{label} malformed manifest should exit 3");
        let (e_bgt, _, _) = run_exe(exe, &["batch", m_str, "-5"]);
        assert_eq!(e_bgt, 4, "{label} negative budget should exit 4");
        let (e_cmd, _, _) = run_exe(exe, &["unknown-subcommand"]);
        assert_eq!(e_cmd, 2, "{label} unknown command should exit 2");
    }
}

#[test]
fn async_task_scheduler_paired_nearest_safety_negative_controls() {
    let dir = temp_dir("negatives");

    let expect_safe_reject = |name: &str, src: &str| {
        let path = dir.join(format!("{name}.zil"));
        std::fs::write(&path, src).expect("write negative source");
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        };
        let report = session.compile_path(&path, &opts);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "expected SafeReject for `{name}`, got {:?}: {:?}",
            report.status,
            report.diagnostics
        );
    };

    // 1. Destroying a Task handle without closing it first (protocol violation).
    expect_safe_reject(
        "destroy_unclosed_task",
        r#"
fn worker(n: i32) -> i32 {
    task_yield()
    return n + 1
}

fn bad_retire(t: Task<i32>) contract { requires(state(t, Open)); } {
    task_finish_destroy(give t)
}

fn main() -> i32 {
    match task_create(worker, 1) {
        .err(_) => { return 1 }
        .ok(t) => { bad_retire(give t) }
    }
    return 0
}
"#,
    );

    // 2. Claiming JobHandle transitions directly from Queued -> Completed without Running.
    expect_safe_reject(
        "skip_running_protocol_state",
        r#"
enum JobPhase { Queued, Running, Completed }
struct JobHandle { state: JobPhase }

fn complete_job(j: &mut JobHandle)
    contract {
        requires(state(j, JobPhase.Running));
        provides(state(j, JobPhase.Completed));
        effects(invalidates(j));
    }
{
    j.state = JobPhase.Completed
}

fn main() -> i32 {
    var h = JobHandle { state: JobPhase.Queued }
    complete_job(&mut h)
    return 0
}
"#,
    );

    // 3. Calling a suspending helper inside a `defer` block.
    expect_safe_reject(
        "suspend_inside_defer",
        r#"
fn yield_once() {
    task_yield()
}

fn bad_worker() -> i32 {
    defer { yield_once() }
    task_yield()
    return 1
}

fn main() -> i32 {
    match task_create(bad_worker) {
        .err(_) => { return 1 }
        .ok(t) => {
            var owned = give t
            task_request_cancel(&mut owned)
            loop {
                match task_poll_close(&mut owned) {
                    std.task.Poll.Pending() => { continue }
                    std.task.Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give owned)
        }
    }
    return 0
}
"#,
    );

    // 4. `resume(transfer)` worker capturing a stack reference across `task_yield()`.
    expect_safe_reject(
        "transfer_worker_captures_borrowed_ref",
        r#"
fn bad_transfer(r: &i32) -> i32 contract { resume(transfer); } {
    task_yield()
    return *r
}

fn main() -> i32 {
    let x = 7
    match task_create(bad_transfer, &x) {
        .err(_) => { return 1 }
        .ok(t) => {
            var owned = give t
            task_request_cancel(&mut owned)
            loop {
                match task_poll_close(&mut owned) {
                    std.task.Poll.Pending() => { continue }
                    std.task.Poll.Ready(_) => { break }
                }
            }
            task_finish_destroy(give owned)
        }
    }
    return 0
}
"#,
    );
}
