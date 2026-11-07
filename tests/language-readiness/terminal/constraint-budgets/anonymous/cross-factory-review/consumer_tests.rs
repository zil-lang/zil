use super::*;

const WIDE: &str = include_str!("multi-scalar-wide-values.zil");
const TWO_LIVES: &str = include_str!("../lifetime-identity-review/two-source-lives.zil");

#[test]
fn cross_factory_independent_sources_keep_full_scalar_and_nominal_contracts() {
    for (name, text) in [("wide-scalars", WIDE), ("two-source-lives", TWO_LIVES)] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    for (name, text, phase, code) in [
        (
            "bad-life",
            include_str!("../lifetime-identity-review/bad-swapped-source-life.zil"),
            Phase::Ct,
            "E-CT-SOURCE",
        ),
        (
            "bad-factory-nominal",
            include_str!("bad-cross-factory-nominal.zil"),
            Phase::Ct,
            "E-CT-SOURCE",
        ),
        (
            "old-type-nominal",
            include_str!("../lifetime-identity-review/bad-type-result-nominal.zil"),
            Phase::Types,
            "E-TYPE-MISMATCH",
        ),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{name}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{name}: {}",
            report.to_json()
        );
    }
    // This closed T -> Type path was supported before CONTEXTUAL_RECORD_RELOCATION. It is a regression
    // control and is not evidence for life-bearing Type value construction.
    let report = compile_source(
        &source(include_str!("../lifetime-identity-review/type-result-nominal.zil")),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn cross_factory_independent_native_values_preserve_width_sign_and_fraction() {
    use zil_driver::{NativeBackend, Optimization};
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    for (name, text, expected) in [
        ("wide-scalars", WIDE, "18446744073709551589\n"),
        ("two-source-lives", TWO_LIVES, "49\n"),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let temp = Temp::new();
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    entry: Some("main".into()),
                    output: Some(temp.0.join("program")),
                    emit_dir: Some(temp.0.join("ir")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{name}: {}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let run = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(run.status.success(), "{name}/{optimization:?}: {run:?}");
            assert_eq!(run.stdout, expected.as_bytes());
            assert!(run.stderr.is_empty());
            eprintln!(
                "P0 CONTEXTUAL_RECORD_RELOCATION {name}/{backend:?}/{optimization:?} full stdout={expected:?}; input {}",
                report.input_fingerprint
            );
        }
    }
}
