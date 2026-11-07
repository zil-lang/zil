//! Actual LLVM API allocation failures before registration and during cleanup.
#![cfg(all(unix, feature = "llvm-api"))]
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const SOURCE: &str = r#"
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
fn main()->void!i32{
 let first=bytes_new()
 if !bytes_reserve(first,7){return ()}
 defer{let consumed=give first;emit(65)}
 let second=bytes_new()
 if !bytes_reserve(second,11){return ()}
 defer{let inner=bytes_new();let prepared=bytes_reserve(inner,13);emit(66)}
 return 0
}"#;
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}
#[test]
fn registration_and_cleanup_preserve_actual_allocation_debt() {
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/173/allocator_probe.c");
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = root.join(format!(
            "machine227-{optimization:?}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let wrapper = dir.join("capture-object.sh");
        fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\ncase \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\nexec {} \"$@\"\n",quote(&dir.join("api.o")),quote(&clang))).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(dir.join("source.zil"), SOURCE).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "machine227.zil", SOURCE),
            &CompileOptions {
                backend: NativeBackend::LlvmApi,
                clang: Some(wrapper),
                stop_after: Stage::Build,
                optimization,
                emit_dir: Some(dir.join("phases")),
                output: Some(dir.join("program")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, NativeBackend::LlvmApi);
        assert!(summary.fallback.is_none());
        assert!(!summary.object_reports.is_empty());
        assert!(
            summary
                .object_reports
                .iter()
                .all(|r| r.contains("\"input_kind\":\"builder\""))
        );
        let probe = dir.join("probe");
        let linked = Command::new(&clang)
            .arg(dir.join("api.o"))
            .arg(&shim)
            .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
            .arg(&probe)
            .output()
            .unwrap();
        fs::write(dir.join("link.stdout"), &linked.stdout).unwrap();
        fs::write(dir.join("link.stderr"), &linked.stderr).unwrap();
        assert!(linked.status.success(), "{linked:?}");
        for fail in 0..=4 {
            let output = Command::new(&probe)
                .env("ZIL_TEST_FAIL_AT", fail.to_string())
                .output()
                .unwrap();
            fs::write(dir.join(format!("fail-{fail}.stdout")), &output.stdout).unwrap();
            fs::write(dir.join(format!("fail-{fail}.stderr")), &output.stderr).unwrap();
            fs::write(
                dir.join(format!("fail-{fail}.exit")),
                format!("{:?}", output.status.code()),
            )
            .unwrap();
            let returned_error = fail == 1 || fail == 2;
            assert_eq!(
                output.status.code(),
                Some(i32::from(returned_error)),
                "{optimization:?}/{fail}: {output:?}"
            );
            let expected = match fail {
                1 => "",
                2 => "A",
                _ => "BA",
            };
            assert_eq!(
                output.stdout,
                expected.as_bytes(),
                "{optimization:?}/{fail}: {output:?}"
            );
            let stats = String::from_utf8(output.stderr).unwrap();
            let calls = if returned_error { fail } else { 3 };
            let allocated = calls - usize::from((1..=3).contains(&fail));
            for fragment in [
                format!("\"alloc_calls\":{calls},"),
                format!("\"allocated\":{allocated},"),
                format!("\"frees\":{allocated},"),
                format!("\"failures\":{},", usize::from((1..=3).contains(&fail))),
                "\"live\":0,".into(),
            ] {
                assert!(
                    stats.contains(&fragment),
                    "{optimization:?}/{fail}: expected {fragment}, got {stats}"
                );
            }
        }
    }
}
