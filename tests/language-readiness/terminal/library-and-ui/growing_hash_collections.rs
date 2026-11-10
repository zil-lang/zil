//! Ordinary POD hash growth keeps the old owner until rehash completes.
use super::*;

const MAP: &str = include_str!("growing-hash-collections/map.zil");
const SET: &str = include_str!("growing-hash-collections/set.zil");

#[test]
fn growing_generic_copy_key_set_source_completes_independent_verification() {
    let dir = directory("growing-generic-copy-key-set-source");
    fs::write(dir.join("main.zil"), SET).unwrap();
    let report = CompilationSession::default().compile_path(
        &dir.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Verify,
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
}
const CALLABLE_REFILL: &str =
    include_str!("growing-hash-collections/callable-store-field-refill.zil");

#[test]
fn callable_store_field_refill_source_preserves_the_mutable_input_effect() {
    let dir = directory("callable-store-field-refill-source");
    fs::write(dir.join("main.zil"), CALLABLE_REFILL).unwrap();
    let report = CompilationSession::default().compile_path(
        &dir.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Verify,
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
}

#[test]
fn growing_pod_hash_map_source_completes_independent_verification() {
    let dir = directory("growing-pod-hash-map-source");
    fs::write(dir.join("main.zil"), MAP).unwrap();
    let report = CompilationSession::default().compile_path(
        &dir.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Verify,
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
}

#[path = "growing_callable_effects.rs"]
mod callable_effects;

#[cfg(unix)]
#[test]
fn growing_map_set_native_rehash_and_every_allocation_failure_preserve_owners() {
    use std::{os::unix::fs::PermissionsExt, path::Path};
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let root = directory("growing-map-set-native");
    let observer = root.join("allocator.o");
    let out = Command::new(&clang)
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/language/composed-store-closures/allocator_probe.c"),
        )
        .arg("-o")
        .arg(&observer)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    let wrapper = root.join("link.sh");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nexec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free\n",
            quote(&clang),
            quote(&observer)
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    let field = |audit: &str, key: &str| -> usize {
        let needle = format!("\"{key}\":");
        audit
            .split_once(&needle)
            .unwrap()
            .1
            .split(|ch: char| !ch.is_ascii_digit())
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let mut executions = 0;
    let mut compile_failures = Vec::new();
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, source, failpoints) in [
                ("map", MAP, 0),
                ("set", SET, 0),
                (
                    "map-allocation",
                    include_str!("growing-hash-collections/map-allocation.zil"),
                    6,
                ),
                (
                    "set-allocation",
                    include_str!("growing-hash-collections/set-allocation.zil"),
                    6,
                ),
            ] {
                let dir = root.join(format!("{backend:?}-{optimization:?}-{label}"));
                fs::create_dir_all(&dir).unwrap();
                fs::write(dir.join("main.zil"), source).unwrap();
                let report = CompilationSession::default().compile_path(
                    &dir.join("main.zil"),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        clang: Some(wrapper.clone()),
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                fs::write(dir.join("compile.json"), report.to_json()).unwrap();
                if report.status != ResultCategory::Success {
                    let failure =
                        format!("{label}/{backend:?}/{optimization:?}: {}", report.to_json());
                    println!("{failure}");
                    compile_failures.push(failure);
                    continue;
                }
                assert_eq!(report.verification_runs, 1);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                for failure in 0..=failpoints {
                    let out = Command::new(&report.artifacts["executable"])
                        .env("ZIL_TEST_FAIL_AT", failure.to_string())
                        .output()
                        .unwrap();
                    fs::write(
                        dir.join(format!("failure-{failure}.txt")),
                        format!("{out:?}"),
                    )
                    .unwrap();
                    fs::write(
                        dir.join(format!("failure-{failure}.json")),
                        format!(
                            "{{\"schema\":1,\"failure\":{failure},\"exit_code\":{},\"success\":{},\"stdout_bytes\":{:?},\"stderr_bytes\":{:?}}}",
                            out.status.code().map_or_else(|| "null".into(), |value| value.to_string()),
                            out.status.success(), out.stdout, out.stderr,
                        ),
                    )
                    .unwrap();
                    assert_eq!(
                        out.status.code(),
                        Some(0),
                        "{label}/{backend:?}/{optimization:?}/{failure}: {out:?}"
                    );
                    assert!(out.stdout.is_empty(), "{out:?}");
                    let audit = String::from_utf8(out.stderr).unwrap();
                    assert_eq!(field(&audit, "live"), 0, "{audit}");
                    assert_eq!(
                        field(&audit, "allocated"),
                        field(&audit, "frees"),
                        "{audit}"
                    );
                    assert_eq!(
                        field(&audit, "failures"),
                        usize::from(failure != 0),
                        "{audit}"
                    );
                    if failure != 0 {
                        assert!(field(&audit, "alloc_calls") >= failure, "{audit}");
                    }
                    if failure == 0 {
                        assert!(field(&audit, "allocated") >= 2, "{audit}");
                        if failpoints != 0 {
                            assert_eq!(field(&audit, "alloc_calls"), failpoints, "{audit}");
                        }
                    }
                    println!(
                        "{label}/{backend:?}/{optimization:?}/failure-{failure}: exit0 {audit}"
                    );
                    executions += 1;
                }
            }
        }
    }
    assert!(
        compile_failures.is_empty(),
        "native inputs failed independently: {compile_failures:#?}"
    );
    assert_eq!(
        executions,
        32 + 32 * usize::from(cfg!(feature = "llvm-api"))
    );
}
