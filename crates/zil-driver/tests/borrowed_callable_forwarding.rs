use std::{fs, process::Command};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CallableReturnSource, StorageBase};

const SOURCE: &str =
    include_str!("../../../tests/fixtures/returned-owning-holders/forward-borrowed.zil");

fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "borrowed-forwarding.zil", text)
}

#[test]
fn forwarded_borrowed_environments_keep_sources_and_instance_contracts_raw_wire() {
    let source = input(SOURCE);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let helpers: Vec<_> = module
        .functions
        .iter()
        .filter(|f| {
            matches!(
                f.callable_return_contract.as_ref().map(|c| c.source),
                Some(CallableReturnSource::Parameter(_))
            )
        })
        .map(|f| f.id)
        .collect();
    assert_eq!(helpers.len(), 2);
    for helper in helpers {
        for attack in 0..5 {
            let mut changed = module.clone();
            let claim = changed.functions[helper.0 as usize]
                .callable_return_contract
                .as_mut()
                .unwrap();
            assert!(!claim.capture_sources.as_ref().unwrap().is_empty());
            match attack {
                0 => claim.capture_sources = None,
                1 => claim.capture_sources = Some(Vec::new()),
                2 => {
                    claim.capture_sources.as_mut().unwrap()[0].sources[0].base =
                        StorageBase::Parameter(99)
                }
                3 => claim.source = CallableReturnSource::Fresh,
                4 => claim.hold.clear(),
                _ => unreachable!(),
            }
            let mut candidate = plan.clone();
            candidate.module_fingerprint = changed.fingerprint();
            let errors =
                zil_verify::verify_module(changed.clone(), candidate.clone(), &target, &limits)
                    .unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject),
                "raw {attack}: {errors:?}"
            );
            let bytes =
                zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
            let (decoded, candidate) =
                zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            let errors =
                zil_verify::verify_module(decoded, candidate, &target, &limits).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject),
                "Wire {attack}: {errors:?}"
            );
        }
    }
}

#[test]
fn ordinary_forwarding_runs_two_actual_environments_in_both_backends() {
    run_native(SOURCE, "external");
}

#[test]
fn lending_forwarding_keeps_the_actual_owner_and_lending_indices_raw_wire() {
    let source = input(include_str!(
        "../../../tests/fixtures/returned-owning-holders/forward-lending.zil"
    ));
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    for (index, function) in module.functions.iter().enumerate() {
        let Some(contract) = &function.callable_return_contract else {
            continue;
        };
        if !matches!(contract.source, CallableReturnSource::Parameter(_)) {
            continue;
        }
        assert_eq!(contract.lending_captures, [0]);
        for attack in 0..3 {
            let mut changed = module.clone();
            let c = changed.functions[index]
                .callable_return_contract
                .as_mut()
                .unwrap();
            match attack {
                0 => c.lending_captures.clear(),
                1 => c.lending_captures = vec![99],
                2 => c.cleanup.clear(),
                _ => unreachable!(),
            }
            let mut candidate = plan.clone();
            candidate.module_fingerprint = changed.fingerprint();
            let errors =
                zil_verify::verify_module(changed.clone(), candidate.clone(), &target, &limits)
                    .unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject),
                "{errors:?}"
            );
            let wire =
                zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
            let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn lending_forwarding_runs_distinct_buffer_owners_in_both_backends() {
    run_native(
        include_str!("../../../tests/fixtures/returned-owning-holders/forward-lending.zil"),
        "lending",
    );
}

fn run_native(source: &str, name: &str) {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-forwarded-borrows-{name}-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let report = compile_source(
                &input(source),
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
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            let result = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(result.status.success(), "{result:?}");
            assert!(result.stdout.is_empty());
        }
    }
}
