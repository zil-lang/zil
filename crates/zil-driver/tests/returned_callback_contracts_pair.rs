//! Frozen in-scope FnMut declaration pair. The historical out-of-scope source
//! remains untouched and is not used to claim a semantic rejection here.
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

#[test]
fn fnmut_returned_parameter_requires_writable_position_and_keeps_poststate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/callable-instance-forwarding/paired");
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("consumer256-pair-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    let mut failed = Vec::new();
    for (name, positive) in [
        ("fnmut_return_current_let", false),
        ("fnmut_return_current_var", true),
    ] {
        let path = root.join(format!("{name}.zil"));
        let source = fs::read_to_string(&path).unwrap();
        for optimization in if positive {
            vec![Optimization::O0, Optimization::O2]
        } else {
            vec![Optimization::O0]
        } {
            let dir = out.join(format!("{name}-{optimization:?}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("source.zil"), &source).unwrap();
            let report = compile_source(
                &SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    path.to_string_lossy().into_owned(),
                    source.as_str(),
                ),
                &CompileOptions {
                    stop_after: if positive {
                        Stage::Build
                    } else {
                        Stage::Verify
                    },
                    backend: if cfg!(feature = "llvm-api") {
                        NativeBackend::LlvmApi
                    } else {
                        NativeBackend::Clang
                    },
                    optimization,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            if !positive {
                assert!(!report.artifacts.contains_key("executable"));
                if report.status != ResultCategory::SafeReject
                    || !report.diagnostics.iter().any(|diagnostic| {
                        diagnostic.category == ResultCategory::SafeReject
                            && diagnostic.phase == Phase::Expand
                            && diagnostic.code == "E-GENERIC-ARGUMENT"
                            && diagnostic
                                .message
                                .contains("FnMut forwarding call requires a mutable binding")
                    })
                {
                    failed.push(report.to_json());
                }
                continue;
            }
            if report.status != ResultCategory::Success {
                failed.push(report.to_json());
                continue;
            }
            assert_eq!(report.verification_runs, 1);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let mut child = Command::new(&report.artifacts["executable"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait().unwrap().is_none() {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("pair native timed out");
                }
                thread::sleep(Duration::from_millis(2));
            }
            let actual = child.wait_with_output().unwrap();
            fs::write(dir.join("stdout"), &actual.stdout).unwrap();
            fs::write(dir.join("stderr"), &actual.stderr).unwrap();
            fs::write(dir.join("exit"), format!("{:?}\n", actual.status.code())).unwrap();
            assert_eq!(actual.status.code(), Some(0));
            assert_eq!(actual.stdout, b"123M");
            assert!(actual.stderr.is_empty());
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
