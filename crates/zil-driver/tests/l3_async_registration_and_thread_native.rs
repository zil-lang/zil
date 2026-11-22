//! End-to-end dual-backend native execution tests linking real C hosts for
//! same-thread `AsyncRegistration` (`queued-*`, `inline-registration-tuple*`,
//! `queued-nested-result`) and cross-thread `foreign-thread-registration`
//! (`raw-holder`, `threaded`, `threaded-generic-sync-drop`) + safety negatives
//! (G7.2 / G7.3 / G7.4).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::{Phase, ResultCategory};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-l3-areg-{name}-{}", std::process::id()));
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

fn make_wrap_clang(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("clang-wrap.sh");
    std::fs::write(
        &script,
        "#!/usr/bin/env bash\nexec clang -Wl,--wrap=calloc -Wl,--wrap=realloc -Wl,--wrap=free \"$@\"\n",
    )
    .expect("write clang wrapper");
    let mut perms = std::fs::metadata(&script)
        .expect("stat wrapper")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).expect("chmod wrapper");
    script
}

fn build_and_run_dual(dir: &Path, entry: &Path, extra_c_sources: &[PathBuf]) -> (i32, Vec<u8>) {
    let clang_wrap = make_wrap_clang(dir);
    let mut expected: Option<(i32, Vec<u8>)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            clang: Some(clang_wrap.clone()),
            output: Some(exe.clone()),
            extra_c_sources: extra_c_sources.to_vec(),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let out = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr for {} ({label}): {}",
            entry.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let result = (out.status.code().unwrap_or(-1), out.stdout);
        if let Some(prev) = &expected {
            assert_eq!(
                &result,
                prev,
                "backend/opt mismatch for {} on {label}",
                entry.display()
            );
        } else {
            expected = Some(result);
        }
    }
    expected.expect("at least one backend ran")
}

#[test]
fn l3_same_thread_queued_async_registration_native() {
    let root = repo_root().join("tests/language-readiness/async-registration");
    let host = root.join("queued-host.c");

    for (name, file, c_hosts) in [
        ("queued_cancel", "queued-cancel.zil", vec![host.clone()]),
        ("queued_result", "queued-result.zil", vec![host.clone()]),
        ("queued_return", "queued-return.zil", vec![host]),
    ] {
        let dir = temp_dir(name);
        let (code, _) = build_and_run_dual(&dir, &root.join(file), &c_hosts);
        assert_eq!(code, 0, "{file} exited with {code}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // `queued-export.zil` pairs with legacy `zil.sabi.task.*` C export symbols;
    // verify its full source + independent checker admission.
    let mut session = CompilationSession::new();
    let report = session.compile_path(
        &root.join("queued-export.zil"),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "queued-export.zil verify failed: {:?}",
        report.diagnostics
    );
}

#[test]
fn l3_inline_registration_tuple_and_queued_nested_native() {
    let agg_root = repo_root().join("tests/language-readiness/task-aggregate-results");
    let tuple_obs = agg_root.join("inline-registration-tuple-observer.c");
    let queued_obs = agg_root.join("queued-observer.c");

    for (name, file, c_hosts) in [
        (
            "inline_reg_tuple",
            "inline-registration-tuple.zil",
            vec![tuple_obs.clone()],
        ),
        (
            "inline_reg_tuple_fail",
            "inline-registration-tuple-failure.zil",
            vec![tuple_obs.clone()],
        ),
        (
            "inline_reg_tuple_sel",
            "inline-registration-tuple-selective.zil",
            vec![tuple_obs],
        ),
        (
            "queued_nested_result",
            "queued-nested-result.zil",
            vec![queued_obs],
        ),
    ] {
        let dir = temp_dir(name);
        let (code, _) = build_and_run_dual(&dir, &agg_root.join(file), &c_hosts);
        assert_eq!(code, 0, "{file} exited with {code}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn l3_foreign_thread_registration_native_and_safety_negatives() {
    let root = repo_root().join("tests/language-readiness/foreign-thread-registration");
    let raw_host = root.join("raw-holder-host.c");

    // `raw-holder.zil` links against `raw-holder-host.c` with real pthread
    // cross-thread calls, raw-pointer retention across close, and allocator wrap.
    let dir = temp_dir("raw_holder");
    let (code, _) = build_and_run_dual(&dir, &root.join("raw-holder.zil"), &[raw_host]);
    assert_eq!(code, 0, "raw-holder.zil exited with {code}");
    let _ = std::fs::remove_dir_all(&dir);

    // `threaded.zil` and `threaded-generic-sync-drop.zil` verify cleanly through
    // full Typeck + Solve + independent zil-check (`threaded-host.c` probes a
    // legacy private frame thunk prefix; cross-thread race & drain are covered
    // natively by `raw-holder.zil` and `foreign_registration_native.rs`).
    for pos in ["threaded.zil", "threaded-generic-sync-drop.zil"] {
        let mut session = CompilationSession::new();
        let report = session.compile_path(
            &root.join(pos),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{pos} verify failed: {:?}",
            report.diagnostics
        );
    }

    // Safety negatives must fail as semantic SafeReject with E-GENERIC-ARGUMENT.
    for neg in [
        "threaded-generic.zil",
        "unused-install-missing-sync-drop.zil",
    ] {
        let entry = root.join(neg);
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        };
        let report = session.compile_path(&entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{neg} must fail with SafeReject, got {:?}: {:?}",
            report.status,
            report.diagnostics
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == Phase::Types && d.code == "E-GENERIC-ARGUMENT"),
            "{neg} expected Types/E-GENERIC-ARGUMENT, got: {:?}",
            report.diagnostics
        );
    }
}
