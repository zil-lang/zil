use super::*;

#[test]
fn direct_workers_keep_both_the_ordinary_and_task_adapter_cleanup_executable_native() {
    let original = include_str!("../../../../tests/language-readiness/task-owner-transport/direct-worker-owner-cleanup.zil");
    let direct_child = original
        .replace("fn parent<F:FnOnce()->i32 + ClosedEnvironment>(give callback:F)", "fn parent(give callback:Task<i32>)")
        .replace("let callback=keep(give original,give quiet);", "let callback=give original;")
        .replace("close_callback(give input);return reason;", "close(give input);return reason;");
    for (name, text) in [("callable", original), ("child", direct_child.as_str())] {
        let typed = typed_original(text);
        assert!(typed.functions.iter().any(|function| function.name.starts_with("parent")
            && function.call_kind == zil_ir::CallKind::Direct));
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") { continue; }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../../out/runs/task-direct-owner-native/{name}-{backend:?}-{optimization:?}"));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("source.zil"), text).unwrap();
                let clang = std::env::var_os("ZIL_CLANG").map(PathBuf::from).unwrap_or_else(|| "clang".into());
                let object = dir.join("observer.o");
                let observer = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    "../../tests/language-readiness/task-owner-transport/direct-frame-observer.c");
                let built = Command::new(&clang).args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .args(["-DZIL_TASK_EXPECTED_FRAMES=2", "-DZIL_TASK_EXPECTED_QUIET=1"])
                    .arg(format!("-DZIL_TASK_DIRECT_CHILD={}", usize::from(name == "child")))
                    .arg(observer).arg("-o").arg(&object).output().unwrap();
                assert!(built.status.success(), "{built:?}");
                let report = compile_source(&source(text), &CompileOptions {
                    stop_after: Stage::Build, backend, optimization,
                    emit_dir: Some(dir.clone()), clang: Some(observer_linker(&dir, &clang, &object)),
                    output: Some(dir.join("program")), ..CompileOptions::default()
                });
                fs::write(dir.join("report.json"), report.to_json()).unwrap();
                assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
                let generated = report.native_codegen.as_ref().unwrap();
                assert_eq!(generated.backend, backend);
                assert!(generated.fallback.is_none());
                let llvm = fs::read_to_string(dir.join("phases/llvm.ll")).unwrap();
                assert!(!llvm.contains(".marker"), "private continuation marker escaped into the executable: {llvm}");
                for failure in 0..=2 {
                    let output = Command::new(&report.artifacts["executable"])
                        .env("ZIL_TASK_AGGREGATE_FAIL_AT", failure.to_string()).output().unwrap();
                    fs::write(dir.join(format!("failure-{failure}.txt")), format!("{output:?}")).unwrap();
                    assert_eq!(output.status.code(), Some(i32::from(failure != 0)), "failure {failure}: {output:?}");
                    assert!(output.stdout.is_empty(), "{output:?}");
                }
            }
        }
    }
}
