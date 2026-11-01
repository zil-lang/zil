//! Author-only allocator observations on actual LLVM API objects. Not a
//! separate-review receipt. Native error/cleanup oracles are asserted, not inferred.
#![cfg(all(unix, feature = "llvm-api"))]
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const SOURCE: &str = include_str!("impl235_fixtures/faults.zil");
fn quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"))
}
#[test]
fn replacement_preparation_and_moved_child_failures_settle_actual_store_prefixes() {
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/173/allocator_probe.c");
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let source = SOURCE;
    let requests = 7;
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = root.join(format!(
            "machine235-{optimization:?}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let wrapper = dir.join("capture-object.sh");
        fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\ncase \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\nexec {} \"$@\"\n",quote(&dir.join("api.o")),quote(&clang))).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(dir.join("source.zil"), &source).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "fault235.zil", source),
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
        assert_eq!(summary.actual_units, 1);
        assert!(!summary.object_reports.is_empty());
        assert!(
            summary
                .object_reports
                .iter()
                .all(|r| r.contains("\"input_kind\":\"builder\"")
                    && r.contains("\"verify_before_after\":true"))
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
        for fail in 0..=requests + 1 {
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
            let failed = fail > 0 && fail <= requests;
            let expected = if failed {
                match fail {
                    1 => "",
                    2 => "A",
                    3 => "A",
                    4 => "BA",
                    5 => "AB",
                    6 => "CAB",
                    7 => "ABC",
                    _ => unreachable!(),
                }
            } else {
                "ABC"
            };
            assert_eq!(
                output.status.code(),
                Some(i32::from(failed)),
                "{optimization:?}/{fail}: {output:?}"
            );
            assert_eq!(output.stdout, expected.as_bytes());
            let stats = String::from_utf8(output.stderr).unwrap();
            let calls = if failed { fail } else { requests };
            let allocated = if failed { (fail - 1).min(6) } else { 6 };
            for fragment in [
                format!("\"alloc_calls\":{calls},"),
                format!("\"allocated\":{allocated},"),
                format!("\"frees\":{allocated},"),
                format!("\"failures\":{},", usize::from(failed)),
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
