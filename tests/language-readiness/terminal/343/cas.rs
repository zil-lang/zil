use super::*;
const OBSERVED: &str = include_str!("sync-probes/cas-observed-value.zil");
#[test]
fn cas343_source_observed_result_and_real_thread_contention() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (case, text) in [
                ("observed", OBSERVED),
                ("negative", include_str!("cas-negative-values.zil")),
                ("contention", include_str!("cas-contention.zil")),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "zil343-cas-source-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(dir.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{case}: {}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let actual = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(actual.status.code(), Some(0), "{case}: {actual:?}");
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}
#[test]
fn cas343_plain_address_mutex_and_name_spoofs_reject() {
    for text in [
        include_str!("sync-probes/cas-ordinary-address-reject.zil").to_owned(),
        OBSERVED
            .replace("atomic_scope", "mutex_scope")
            .replace("AtomicI32", "MutexI32"),
        "fn atomic_compare_exchange(x:i32)->i32{return x} fn main()->i32{return 0}".to_owned(),
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
}
