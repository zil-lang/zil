//! Actual allocator faults must preserve existing POD contents and release once.
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

fn allocator_wrapper(dir: &Path) -> std::path::PathBuf {
    let clang = std::env::var_os("ZIL_CLANG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    let observer = dir.join("allocator.o");
    let result = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/language/173/allocator_probe.c"),
        )
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let wrapper = dir.join("link.sh");
    fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n", quote(&clang), quote(&observer), quote(&clang))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    wrapper
}

#[test]
fn pod_reserve_allocator_failures_preserve_full_values_and_release_storage() {
    let dir = super::super::directory("pod-allocation");
    let wrapper = allocator_wrapper(&dir);
    let text = include_str!("allocation.zil");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "pod-allocation.zil", text);
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
                    2 => 3580246791358123,
                    _ => 3580246791358223,
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
