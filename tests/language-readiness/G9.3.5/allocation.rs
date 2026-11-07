//! Real malloc/realloc failure and one-release observations for owning POD/scalar parents.
use super::*;

#[test]
fn ordinary_store_parent_failure_keeps_full_values_and_releases_each_allocation_once() {
    let dir = directory("ordinary-parent-allocation");
    let wrapper = super::super::pod_collections::allocation::allocator_wrapper(&dir);
    let text = include_str!("ordinary-allocation.zil");
    let input = dir.join("main.zil");
    fs::write(&input, text).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") { backends.push(NativeBackend::LlvmApi); }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("ordinary-{backend:?}-{optimization:?}"));
            let report = zil_driver::compile_path(&input, &CompileOptions {
                backend,
                optimization,
                entry: Some("result".into()),
                stop_after: Stage::Build,
                clang: Some(wrapper.clone()),
                output: Some(exe.clone()),
                emit_dir: Some(dir.join(format!("phases-{backend:?}-{optimization:?}"))),
                ..CompileOptions::default()
            });
            fs::write(exe.with_extension("json"), report.to_json()).unwrap();
            assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
            assert!(report.verification_runs > 0);
            assert!(report.source_files.len() >= 2, "real std.store import required: {}", report.to_json());
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            for (failure, expected, calls, frees) in [
                (0, "18446744073709551611\n", 3, 2),
                (1, "101\n", 1, 0),
                (2, "18446744073709551610\n", 2, 1),
                (3, "18446744073709551610\n", 3, 2),
            ] {
                let run = Command::new(&exe).env("ZIL_TEST_FAIL_AT", failure.to_string()).output().unwrap();
                assert_eq!(run.status.code(), Some(0), "{backend:?}/{optimization:?}/{failure}: {run:?}");
                assert_eq!(run.stdout, expected.as_bytes(), "{backend:?}/{optimization:?}/{failure}: {run:?}");
                let audit = String::from_utf8(run.stderr).unwrap();
                assert!(audit.contains("\"live\":0"), "{audit}");
                assert!(audit.contains(&format!("\"alloc_calls\":{calls}")), "{audit}");
                assert!(audit.contains(&format!("\"frees\":{frees}")), "{audit}");
                assert!(audit.contains(if failure == 0 { "\"failures\":0" } else { "\"failures\":1" }), "{audit}");
                fs::write(dir.join(format!("audit-{backend:?}-{optimization:?}-{failure}.txt")), format!("stdout={expected}{audit}")).unwrap();
            }
        }
    }
}

#[test]
fn ordinary_prepared_replacement_error_retires_only_prepared_owner_then_old_parent() {
    let dir = directory("ordinary-prepared-error-allocation");
    let wrapper = super::super::pod_collections::allocation::allocator_wrapper(&dir);
    let text = include_str!("ordinary-prepare-failure.zil");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "ordinary-prepare-failure.zil", text);
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") { backends.push(NativeBackend::LlvmApi); }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let exe = dir.join(format!("prepared-{backend:?}-{optimization:?}"));
            let report = compile_source(&source, &CompileOptions {
                backend,
                optimization,
                entry: Some("result".into()),
                stop_after: Stage::Build,
                clang: Some(wrapper.clone()),
                output: Some(exe.clone()),
                emit_dir: Some(dir.join(format!("phases-{backend:?}-{optimization:?}"))),
                ..CompileOptions::default()
            });
            fs::write(exe.with_extension("json"), report.to_json()).unwrap();
            assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
            assert!(report.verification_runs > 0);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let run = Command::new(&exe).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "{backend:?}/{optimization:?}: {run:?}");
            assert_eq!(run.stdout, b"18446744073709551612\n");
            let audit = String::from_utf8(run.stderr).unwrap();
            for expected in ["\"alloc_calls\":3", "\"allocated\":3", "\"frees\":3", "\"failures\":0", "\"live\":0"] {
                assert!(audit.contains(expected), "{audit}");
            }
            fs::write(dir.join(format!("audit-{backend:?}-{optimization:?}.txt")), &audit).unwrap();
        }
    }
}
