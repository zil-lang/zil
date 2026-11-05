use super::*;
use zil_ir::*;
#[allow(dead_code)]
mod neighbors {
    include!("tail_neighbors.rs");
}
const TRUE: &str = include_str!("helper-tail-true.zil");
const FALSE: &str = include_str!("helper-tail-false.zil");

#[test]
fn g337_actual_source_tail_and_raw_wire_hostile_neighbors() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for text in [TRUE, FALSE] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
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
        let raw = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        let helper = raw
            .functions
            .iter()
            .find(|f| f.callable_contract.is_some())
            .unwrap();
        assert_eq!(helper.blocks.len(), 3);
        assert_eq!(
            helper.callable_contract.as_ref().unwrap().roles[0].cleanup,
            CallableCleanup::Owned
        );
        let closure = &raw.closures[0];
        assert_eq!(closure.kind, CallableKind::FnOnce);
        assert!(matches!(closure.captures[0], Type::Managed(_)));
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        if text != TRUE {
            continue;
        }
        for (name, changed) in neighbors::hostile(helper) {
            let mut candidate = raw.clone();
            candidate.functions[helper.id.0 as usize] = changed;
            let errors = zil_analyze::analyze(&candidate, &limits).unwrap_err();
            assert!(
                errors.iter().all(|d| matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                )),
                "Analyze {name}: {errors:?}"
            );
            let mut claim = plan.clone();
            claim.module_fingerprint = candidate.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&candidate, &claim, &limits).unwrap();
            for wire in [false, true] {
                let checked = if wire {
                    zil_verify::verify_module_bytes(&bytes, &target, &limits)
                } else {
                    zil_verify::verify_module(candidate.clone(), claim.clone(), &target, &limits)
                };
                let errors = checked.unwrap_err();
                assert!(
                    errors.iter().all(|d| matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )),
                    "Verify {name}/{wire}: {errors:?}"
                );
                eprintln!(
                    "G337 raw neighbor {name} wire={wire}: {}",
                    errors[0].message
                );
            }
        }
    }
    for (text, phase, fragment) in [
        (
            include_str!("helper-branch.zil"),
            zil_base::Phase::Analyze,
            "staged owning callback",
        ),
        (
            include_str!("user-drop.zil"),
            zil_base::Phase::Analyze,
            "user-defined Drop",
        ),
        (
            include_str!("give-f.zil"),
            zil_base::Phase::Expand,
            "callable parameter must",
        ),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, phase);
        assert_eq!(report.verification_runs, 0);
        assert!(
            report.diagnostics[0].message.contains(fragment),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn g337_actual_source_whole_analyze_verify_work_boundaries() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let raw = zil_types::check(&g334_core(TRUE), &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    for phase in [0, 1, 2] {
        let check = |n| {
            let tight = Limits {
                max_analysis_steps: n,
                ..limits.clone()
            };
            match phase {
                0 => zil_analyze::analyze(&raw, &tight).map(|p| assert_eq!(p, plan)),
                1 => zil_verify::verify_module(raw.clone(), plan.clone(), &target, &tight)
                    .map(|_| ()),
                _ => zil_verify::verify_module_bytes(&bytes, &target, &tight).map(|_| ()),
            }
        };
        let (mut low, mut high) = (1, limits.max_analysis_steps);
        while low < high {
            let n = low + (high - low) / 2;
            match check(n) {
                Ok(()) => high = n,
                Err(errors)
                    if errors
                        .iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit) =>
                {
                    low = n + 1
                }
                Err(errors) => panic!("phase{phase}: {errors:?}"),
            }
        }
        for n in [low, low - 1, low, low] {
            if n == low {
                check(n).unwrap();
            } else {
                assert!(
                    check(n)
                        .unwrap_err()
                        .iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit)
                );
            }
        }
        eprintln!(
            "G337 whole actual Source phase={phase} (Analyze/Verify/Wire) N={low}/N-1/fresh/retry"
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn g337_actual_source_native_tail_true_false_clang_api_o0_o2() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for (case, text, expected) in [("true", TRUE, b"42\n"), ("false", FALSE, b"43\n")] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-g337-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("main".into()),
                        output: Some(dir.join("program")),
                        emit_dir: Some(dir.join("ir")),
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
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let run = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(
                    run.status.success(),
                    "{case}/{backend:?}/{optimization:?}: {run:?}"
                );
                assert_eq!(run.stdout, expected);
                assert!(run.stderr.is_empty());
                eprintln!(
                    "G337 actual native {case}/{backend:?}/{optimization:?}: stdout {:?}; artifact {}",
                    String::from_utf8_lossy(&run.stdout),
                    dir.display()
                );
            }
        }
    }
}
