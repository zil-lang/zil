//! Actual allocator faults must preserve existing POD contents and release once.
use std::{fs, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

#[test]
fn mutated_values_survive_reserve_fault_and_release_once() {
    let dir = super::super::directory("pod-allocation");
    let wrapper = super::super::pod_collections::allocation::allocator_wrapper(&dir);
    let text = include_str!("../store-allocation/allocation.zil").replace("    let grown=store_reserve(items,4)", "    let changed=store_at_mut(items,0);changed.value+=1;\n    let grown=store_reserve(items,4)");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "mutable-pod-allocation.zil", text);
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = dir.join(format!("program-{backend:?}-{optimization:?}"));
            let report = compile_source(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    entry: Some("result".into()),
                    clang: Some(wrapper.clone()),
                    output: Some(output.clone()),
                    ..CompileOptions::default()
                },
            );
            fs::write(output.with_extension("json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            for failure in 0..=2 {
                let result = Command::new(&output)
                    .env("ZIL_TEST_FAIL_AT", failure.to_string())
                    .output()
                    .unwrap();
                let expected = match failure {
                    1 => -101i64,
                    2 => 3580246791358124,
                    _ => 3580246791358224,
                };
                assert_eq!(
                    result.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}/{failure}: {result:?}"
                );
                assert_eq!(result.stdout, format!("{expected}\n").as_bytes());
                let audit = String::from_utf8(result.stderr).unwrap();
                assert!(audit.contains("\"live\":0"), "{audit}");
                assert!(
                    audit.contains(if failure == 0 {
                        "\"failures\":0"
                    } else {
                        "\"failures\":1"
                    }),
                    "{audit}"
                );
                assert!(
                    audit.contains(if failure == 1 {
                        "\"alloc_calls\":1"
                    } else {
                        "\"alloc_calls\":2"
                    }),
                    "{audit}"
                );
                assert!(
                    audit.contains(if failure == 1 {
                        "\"frees\":0"
                    } else {
                        "\"frees\":1"
                    }),
                    "{audit}"
                );
                fs::write(
                    dir.join(format!("audit-{backend:?}-{optimization:?}-{failure}.txt")),
                    format!("stdout={expected}\n{audit}"),
                )
                .unwrap();
            }
        }
    }
}
