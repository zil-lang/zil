//! Native no-future regression: a real thread holds userdata before trampoline
//! admission. The control invokes F only on the owner thread.
use super::*;

#[test]
#[cfg(target_os = "linux")]
fn foreign_raw_holder_is_pending_until_thread_and_queue_return() {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
    use zil_driver::{NativeBackend, Optimization, Stage};
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-raw-holder-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let observer = dir.join("host.o");
            let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread", "-c"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../../tests/language-readiness/foreign-thread-registration/raw-holder-host.c",
                ))
                .arg("-o")
                .arg(&observer)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper, format!(
                "#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -pthread -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n",
                quote(Path::new(&clang)), quote(&observer), quote(Path::new(&clang))
            )).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = compile_source(
                &source(include_str!("raw-holder.zil")),
                &CompileOptions {
                    backend,
                    optimization,
                    clang: Some(wrapper),
                    stop_after: Stage::Build,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("artifacts")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            for failure in [
                None,
                Some("ZIL_RAW_ALLOC_FAIL"),
                Some("ZIL_RAW_INSTALL_FAIL"),
            ] {
                let mut run = Command::new(&report.artifacts["executable"]);
                if let Some(failure) = failure {
                    run.env(failure, "1");
                }
                let result = run.output().unwrap();
                assert_eq!(
                    result.status.code(),
                    Some(0),
                    "{backend:?} {optimization:?} {failure:?}: {result:?}"
                );
                let expected = if failure.is_some() {
                    "failed construction drained raw holders; environment dropped once\n"
                } else {
                    "pre-trampoline holder kept frame live; no-future before once cleanup\n"
                };
                assert_eq!(result.stdout, expected.as_bytes(), "{result:?}");
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
