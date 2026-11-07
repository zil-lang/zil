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

#[test]
fn ordinary_sum_replacement_and_owned_captures_release_every_actual_store_once() {
    // Each row reuses the exact native Source consumer. A missing Drop on the
    // replaced Enum branch leaves live=1 and fails the process at exit.
    for (label, text, expected, allocations) in [
        ("error-side", include_str!("ordinary-error-side.zil"), b"18446744073709551610\n".as_slice(), 1),
        ("tuple-field", include_str!("ordinary-tuple-field.zil"), b"18446744073709551611\n".as_slice(), 1),
        ("enum-field", include_str!("ordinary-enum-field.zil"), b"18446744073709551612\n".as_slice(), 2),
        ("error-field", include_str!("ordinary-error-field.zil"), b"18446744073709551613\n".as_slice(), 1),
        ("sum-capture", include_str!("ordinary-sum-capture.zil"), b"18446744073709551614\n".as_slice(), 1),
        ("direct-shape-captures", include_str!("ordinary-direct-shape-captures.zil"), b"18446744073709551614\n".as_slice(), 5),
        ("parent-captures", include_str!("ordinary-captures.zil"), b"18446744073709551612\n".as_slice(), 3),
        ("direct-captures", include_str!("ordinary-direct-captures.zil"), b"18446744073709551612\n".as_slice(), 3),
        ("both-owned-error", include_str!("ordinary-both-owned-error.zil"), b"18446744073709551613\n".as_slice(), 2),
    ] {
        let dir = directory(&format!("ordinary-sum-allocation-{label}"));
        let wrapper = super::super::pod_collections::allocation::allocator_wrapper(&dir);
        let source = SourceUnit::new(FileId(0), ModuleId(0), "ordinary-sum-allocation.zil", text);
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") { continue; }
            for optimization in [Optimization::O0, Optimization::O2] {
                let exe = dir.join(format!("program-{backend:?}-{optimization:?}"));
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
                assert_eq!(report.status, ResultCategory::Success, "{label}: {}", report.to_json());
                assert!(report.verification_runs > 0);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let run = Command::new(&exe).output().unwrap();
                assert_eq!(run.status.code(), Some(0), "{label}/{backend:?}/{optimization:?}: {run:?}");
                assert_eq!(run.stdout, expected, "{label}/{backend:?}/{optimization:?}: {run:?}");
                let audit = String::from_utf8(run.stderr).unwrap();
                for field in [
                    format!("\"alloc_calls\":{allocations}"),
                    format!("\"allocated\":{allocations}"),
                    format!("\"frees\":{allocations}"),
                    "\"failures\":0".into(),
                    "\"live\":0".into(),
                ] {
                    assert!(audit.contains(&field), "{label}/{backend:?}/{optimization:?}: {audit}");
                }
                fs::write(dir.join(format!("audit-{backend:?}-{optimization:?}.txt")), &audit).unwrap();
            }
        }
    }
}
