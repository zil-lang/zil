//! Source-derived owning callback ABI, borrowed inputs and actual cleanup.
use std::{fs, path::Path, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

const EVENT_REPLY: &str = include_str!("../../../tests/fixtures/callbacks/event-reply.zil");
const EVENT_COPY_RESULT: &str =
    include_str!("../../../tests/fixtures/callbacks/event-copy-result.zil");
const OWNED_RESULT: &str = include_str!("../../../tests/fixtures/callbacks/owned-result.zil");

#[test]
fn owning_callback_machine_concrete_owned_event_and_result_execute_o0_o2() {
    let report_root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("owning_callback_machine-{}", std::process::id()));
    for (name, source) in [
        ("event-reply", EVENT_REPLY),
        ("event-copy-result", EVENT_COPY_RESULT),
        ("owned-result", OWNED_RESULT),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = report_root.join(format!("{name}-{optimization:?}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("main.zil"), source).unwrap();
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source),
                &CompileOptions {
                    backend: NativeBackend::Clang,
                    optimization,
                    stop_after: Stage::Build,
                    emit_dir: Some(dir.join("phases")),
                    output: Some(dir.join("program")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let program = &report.artifacts["executable"];
            let actual = Command::new(Path::new(program)).output().unwrap();
            fs::write(dir.join("stdout"), &actual.stdout).unwrap();
            fs::write(dir.join("stderr"), &actual.stderr).unwrap();
            fs::write(dir.join("exit"), format!("{:?}\n", actual.status.code())).unwrap();
            assert_eq!(
                actual.status.code(),
                Some(0),
                "{name}/{optimization:?}: {actual:?}"
            );
            assert!(actual.stdout.is_empty());
            assert!(actual.stderr.is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn borrowed_input_owned_reply_cross_module_cleanup_and_allocation_failures() {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("borrowed-input-owned-reply-{}", std::process::id()));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = repo.join("tests/fixtures/callbacks/borrowed-input-owned-result/main.zil");
    let shim = repo.join("tools/language/composed-store-closures/allocator_probe.c");
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    #[cfg(not(feature = "llvm-api"))]
    let backends = [NativeBackend::Clang];
    #[cfg(feature = "llvm-api")]
    let backends = [NativeBackend::Clang, NativeBackend::LlvmApi];
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = root.join(format!("{backend:?}-{optimization:?}"));
            fs::create_dir_all(&dir).unwrap();
            let wrapper = dir.join("capture-object.sh");
            fs::write(&wrapper, format!("#!/bin/sh\nset -eu\n{} \"$@\"\nfor arg in \"$@\"; do\ncase \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\n", quote(&clang), quote(&dir.join("program.o")))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = zil_driver::compile_path(
                &source,
                &CompileOptions {
                    backend,
                    clang: Some(wrapper),
                    optimization,
                    stop_after: Stage::Build,
                    emit_dir: Some(dir.join("phases")),
                    output: Some(dir.join("program")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            if backend == NativeBackend::LlvmApi {
                let native = report.native_codegen.as_ref().unwrap();
                assert!(native.fallback.is_none());
                assert!(!native.object_reports.is_empty());
                assert!(
                    native
                        .object_reports
                        .iter()
                        .all(|object| object.contains("\"input_kind\":\"builder\""))
                );
            }
            if backend == NativeBackend::Clang {
                // The single-unit reference backend compiles IR directly to
                // its executable. Produce the shim input from that same IR
                // with the same optimization; the API case uses its actual
                // Builder-produced object captured at the link boundary.
                let compiled = Command::new(&clang)
                    .args(["-x", "ir", &format!("-{optimization:?}"), "-c"])
                    .arg(&report.artifacts["llvm"])
                    .arg("-o")
                    .arg(dir.join("program.o"))
                    .output()
                    .unwrap();
                assert!(compiled.status.success(), "{compiled:?}");
            }
            let probe = dir.join("probe");
            let linked = Command::new(&clang)
                .arg(dir.join("program.o"))
                .arg(&shim)
                .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
                .arg(&probe)
                .output()
                .unwrap();
            assert!(linked.status.success(), "{linked:?}");
            for fail in 0..=4 {
                let actual = Command::new(&probe)
                    .env("ZIL_TEST_FAIL_AT", fail.to_string())
                    .output()
                    .unwrap();
                fs::write(dir.join(format!("fail-{fail}.stdout")), &actual.stdout).unwrap();
                fs::write(dir.join(format!("fail-{fail}.stderr")), &actual.stderr).unwrap();
                fs::write(
                    dir.join(format!("fail-{fail}.exit")),
                    format!("{:?}", actual.status.code()),
                )
                .unwrap();
                let failure = (1..=3).contains(&fail);
                assert_eq!(
                    actual.status.code(),
                    Some(i32::from(failure)),
                    "{backend:?}/{optimization:?}/{fail}: {actual:?}"
                );
                let expected = match fail {
                    1 => "",
                    2 => "I",
                    3 => "BI",
                    _ => "XCBI",
                };
                assert_eq!(
                    actual.stdout,
                    expected.as_bytes(),
                    "{backend:?}/{optimization:?}/{fail}: {actual:?}"
                );
                let calls = if failure { fail } else { 3 };
                let allocated = calls - usize::from(failure);
                let stats = String::from_utf8(actual.stderr).unwrap();
                for fragment in [
                    format!("\"alloc_calls\":{calls},"),
                    format!("\"allocated\":{allocated},"),
                    format!("\"frees\":{allocated},"),
                    format!("\"failures\":{},", usize::from(failure)),
                    "\"live\":0,".into(),
                ] {
                    assert!(
                        stats.contains(&fragment),
                        "{backend:?}/{optimization:?}/{fail}: {fragment}: {stats}"
                    );
                }
            }
        }
    }
}
