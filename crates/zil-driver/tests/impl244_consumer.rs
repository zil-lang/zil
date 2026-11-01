//! Predeclared 244 consumer. This authors source tests, not production safety.
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../")
}
fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = base.join(format!(
        "consumer244-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}
fn backend() -> NativeBackend {
    if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    }
}
fn json(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn project() -> ProjectSources {
    let names = ["main", "portable", "model", "storage", "state"];
    ProjectSources {
        root: ModuleId(0),
        modules: names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let path = root().join(format!("tests/impl244/project/{name}.zil"));
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy().into_owned(),
                    fs::read_to_string(&path).unwrap(),
                )
            })
            .collect(),
        paths: names
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, n)| (vec![(*n).into()], ModuleId(i as u32)))
            .collect(),
    }
}
fn assert_builder(report: &zil_driver::CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let native = report.native_codegen.as_ref().unwrap();
    assert_eq!(native.backend, backend());
    assert!(native.fallback.is_none());
    if cfg!(feature = "llvm-api") {
        assert!(
            native
                .object_reports
                .iter()
                .all(|r| r.contains("\"input_kind\":\"builder\"")
                    && r.contains("\"verify_before_after\":true"))
        );
    }
}
fn execute(binary: &Path, input: &[u8], out: &Path, expected: &Value, allocator: bool) {
    fs::create_dir_all(out).unwrap();
    fs::write(out.join("stdin"), input).unwrap();
    let mut command = Command::new(binary);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if allocator {
        command.env(
            "ZIL_TEST_FAIL_AT",
            expected["fail_at"].as_u64().unwrap().to_string(),
        );
    }
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{}: consumer timed out", out.display());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let result = child.wait_with_output().unwrap();
    fs::write(out.join("stdout"), &result.stdout).unwrap();
    fs::write(out.join("stderr"), &result.stderr).unwrap();
    fs::write(
        out.join("exit.txt"),
        format!("{:?}\n", result.status.code()),
    )
    .unwrap();
    fs::write(
        out.join("expected.json"),
        serde_json::to_vec_pretty(expected).unwrap(),
    )
    .unwrap();
    assert_eq!(
        result.status.code().map(i64::from),
        expected["exit"].as_i64(),
        "{}",
        out.display()
    );
    assert_eq!(
        result.stdout,
        expected["stdout"].as_str().unwrap().as_bytes(),
        "{}",
        out.display()
    );
    if allocator {
        let actual: Value = serde_json::from_slice(&result.stderr).unwrap();
        for field in [
            "alloc_calls",
            "allocated",
            "frees",
            "failures",
            "live",
            "peak_live",
        ] {
            assert_eq!(
                actual[field],
                expected[field],
                "{} {field}: {actual}",
                out.display()
            );
        }
    } else {
        assert!(result.stderr.is_empty(), "{}: {result:?}", out.display());
    }
}
fn frozen_case(name: &str) {
    let plan = json(root().join("tests/impl244/frozen-Q/PLAN.json"));
    let expected = plan["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap();
    let path = root().join(format!("tests/impl244/frozen-Q/{name}/main.zil"));
    let text = fs::read_to_string(&path).unwrap();
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = directory(name);
        fs::write(out.join("source.zil"), &text).unwrap();
        let negative = expected["negative"].as_bool().unwrap();
        let report = compile_source(
            &SourceUnit::new(
                FileId(0),
                ModuleId(0),
                path.to_string_lossy().into_owned(),
                text.as_str(),
            ),
            &CompileOptions {
                stop_after: if negative {
                    Stage::Verify
                } else {
                    Stage::Build
                },
                backend: backend(),
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        if negative {
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.phase == Phase::Analyze && d.code == "E-UNINITIALIZED"),
                "{}",
                report.to_json()
            );
            assert!(!report.artifacts.contains_key("executable"));
        } else {
            assert_builder(&report);
            for (i, case) in expected["native_if_source_verified"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                execute(
                    &report.artifacts["executable"],
                    case["input"].as_str().unwrap().as_bytes(),
                    &out.join(format!("case-{i}")),
                    case,
                    false,
                );
            }
        }
    }
}
#[test]
fn original_q01_flat_control() {
    frozen_case("01-unit-flat")
}
#[test]
fn original_q02_nested_control() {
    frozen_case("02-unit-nested-and-owned-error-unit")
}
#[test]
fn original_q03_store_session_success() {
    frozen_case("03-unit-store-session")
}
#[test]
fn original_q04_both_owned_control() {
    frozen_case("04-both-owned-flat")
}
#[test]
fn original_q05_owned_option_success() {
    frozen_case("05-owned-option-session")
}
#[test]
fn original_q06_duplicate_consumption_rejects() {
    frozen_case("06-negative-double-consume")
}

#[test]
fn five_module_predeclared_native_matrix() {
    let p = project();
    let oracle = json(root().join("tests/impl244/ORACLE.json"));
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = directory("matrix");
        for module in &p.modules {
            fs::write(
                out.join(format!("module{}.zil", module.module_id.0)),
                &module.text,
            )
            .unwrap();
        }
        let report = compile_project(
            &p,
            &CompileOptions {
                backend: backend(),
                optimization,
                stop_after: Stage::Build,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_builder(&report);
        for case in oracle["cases"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            execute(
                &report.artifacts["executable"],
                input.as_bytes(),
                &out.join(format!("normal-{input}")),
                &case["normal"],
                false,
            );
        }
        for (index, case) in oracle["invalid"].as_array().unwrap().iter().enumerate() {
            let hex = case["input_hex"].as_str().unwrap();
            let input = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>();
            execute(
                &report.artifacts["executable"],
                &input,
                &out.join(format!("invalid-{index}")),
                case,
                false,
            );
        }
    }
}

#[cfg(all(unix, feature = "llvm-api"))]
#[test]
fn five_module_actual_allocator_failures_follow_the_frozen_source_oracle() {
    use std::os::unix::fs::PermissionsExt;
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let p = project();
    let oracle = json(root().join("tests/impl244/ORACLE.json"));
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = directory("allocator");
        for module in &p.modules {
            fs::write(
                out.join(format!("module{}.zil", module.module_id.0)),
                &module.text,
            )
            .unwrap();
        }
        let wrapper = out.join("capture-object.sh");
        fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\ncase \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\nexec {} \"$@\"\n",quote(&out.join("api.o")),quote(&clang))).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let report = compile_project(
            &p,
            &CompileOptions {
                backend: NativeBackend::LlvmApi,
                optimization,
                clang: Some(wrapper),
                stop_after: Stage::Build,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_builder(&report);
        assert_eq!(report.native_codegen.as_ref().unwrap().actual_units, 1);
        let probe = out.join("probe");
        let linked = Command::new(&clang)
            .arg(out.join("api.o"))
            .arg(root().join("tools/language/173/allocator_probe.c"))
            .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
            .arg(&probe)
            .output()
            .unwrap();
        fs::write(out.join("link.stdout"), &linked.stdout).unwrap();
        fs::write(out.join("link.stderr"), &linked.stderr).unwrap();
        assert!(linked.status.success(), "{linked:?}");
        for case in oracle["cases"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            for expected in case["allocator"].as_array().unwrap() {
                let fail = expected["fail_at"].as_u64().unwrap();
                execute(
                    &probe,
                    input.as_bytes(),
                    &out.join(format!("input-{input}-fail-{fail}")),
                    expected,
                    true,
                );
            }
        }
    }
}
