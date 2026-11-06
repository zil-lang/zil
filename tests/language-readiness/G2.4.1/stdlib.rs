//! Exercise the actual std.iter file through ordinary module resolution.
use super::*;
#[test]
fn g241_std_iter_resolves_real_trait_and_generic_consumer() {
    let mut backends = vec![zil_driver::NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(zil_driver::NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(format!(
                    "g241-stdlib-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&dir).unwrap();
            let main = include_str!("project/main.zil")
                .replace("import api", "import std.iter")
                .replace("import consumer", "")
                .replace("consumer.next_copy", "iter.next_copy");
            for (name, text) in [
                ("main", main),
                (
                    "cursor",
                    include_str!("project/cursor.zil")
                        .replace("import api", "import std.iter")
                        .replace("api.LendingIterator", "iter.LendingIterator"),
                ),
                (
                    "other",
                    include_str!("project/other.zil")
                        .replace("import api", "import std.iter")
                        .replace("api.LendingIterator", "iter.LendingIterator"),
                ),
            ] {
                fs::write(dir.join(format!("{name}.zil")), text).unwrap();
            }
            let executable = dir.join("program");
            let report = zil_driver::compile_path(
                &dir.join("main.zil"),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("main".into()),
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
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
            assert!(
                report
                    .source_files
                    .values()
                    .any(|p| p == "<std.iter>"),
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let cg = report.native_codegen.as_ref().unwrap();
            assert_eq!(cg.backend, backend);
            assert!(cg.fallback.is_none());
            let output = Command::new(executable).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(output.stdout, b"42\n", "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
