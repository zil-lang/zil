use super::*;
use zil_ir::*;
#[allow(dead_code)]
mod neighbors {
    include!("cfg_neighbors.rs");
}
const CASES: [(&str, &str, &[u8]); 5] = [
    ("before", include_str!("branch-before-call.zil"), b"108\n"),
    ("drop", include_str!("drop-other-branch.zil"), b"51\n"),
    ("diamond", include_str!("diamond-before-call.zil"), b"101\n"),
    ("nested", include_str!("nested-before-call.zil"), b"106\n"),
    (
        "transfer",
        include_str!("forward-before-diamond.zil"),
        b"101\n",
    ),
];
#[test]
fn actual_source_prebranch_diamonds_and_honest_raw_wire() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (case, text, _) in CASES {
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
            "{case}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = zil_types::check(&shared_helper_life_core(text), &target, &limits).unwrap();
        assert_eq!(raw.closures[0].kind, CallableKind::FnOnce);
        assert!(matches!(raw.closures[0].captures[0], Type::Managed(_)));
        let f = raw
            .functions
            .iter()
            .find(|f| f.callable_contract.is_some())
            .unwrap();
        assert_eq!(f.callable_contract.as_ref().unwrap().roles[0].parameter, 1);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        eprintln!(
            "HELPER_CFG_JOIN actual {case}: {} real blocks, honest Analyze/raw/Wire",
            f.blocks.len()
        );
        if case != "transfer" {
            continue;
        }
        assert_eq!(f.blocks.len(), 4);
        for (name, changed) in neighbors::hostile(f) {
            let mut candidate = raw.clone();
            candidate.functions[f.id.0 as usize] = changed;
            let errors = zil_analyze::analyze(&candidate, &limits).unwrap_err();
            assert!(
                errors.iter().all(|d| matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                )),
                "A {name}: {errors:?}"
            );
            let mut claim = plan.clone();
            claim.module_fingerprint = candidate.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&candidate, &claim, &limits).unwrap();
            for wire in [false, true] {
                let result = if wire {
                    zil_verify::verify_module_bytes(&bytes, &target, &limits)
                } else {
                    zil_verify::verify_module(candidate.clone(), claim.clone(), &target, &limits)
                };
                let errors = result.unwrap_err();
                assert!(
                    errors.iter().all(|d| matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )),
                    "V {name}/{wire}: {errors:?}"
                );
                eprintln!(
                    "HELPER_CFG_JOIN full neighbor {name} wire={wire}: {}",
                    errors[0].message
                );
            }
        }
    }
    let report = compile_source(
        &source(include_str!("../fnonce-helper-next/helper-branch.zil")),
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
}
#[test]
fn actual_dag_budget_boundaries_and_old_tail_fees() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (label, text, known) in [
        ("new-transfer", CASES[4].1, None),
        (
            "old-tail",
            include_str!("../fnonce-helper-next/helper-tail-true.zil"),
            Some([53167, 51600, 51600]),
        ),
    ] {
        let raw = zil_types::check(&shared_helper_life_core(text), &target, &limits).unwrap();
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
                    Err(ds)
                        if ds
                            .iter()
                            .any(|d| d.category == ResultCategory::ResourceLimit) =>
                    {
                        low = n + 1
                    }
                    Err(ds) => panic!("{label}/{phase}: {ds:?}"),
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
            if let Some(known) = known {
                // closure environment.10 replaces u8 marks by a pointer-sized actual-role key
                // plus flags. This exact old source has one 4-local/12-SSA
                // table in each independent helper consumer: 16 * 15 = 240
                // added physical units on this 64-bit host. Keep old N red.
                let slots: usize = raw
                    .functions
                    .iter()
                    .filter(|f| f.callable_contract.is_some())
                    .map(|f| f.locals.len() + f.values.len())
                    .sum();
                assert_eq!(slots, 16);
                let physical_delta = slots * (std::mem::size_of::<(usize, u8)>() - 1);
                assert!(
                    check(known[phase])
                        .unwrap_err()
                        .iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit)
                );
                // G4's complete nominal graph checks add 98/178 work in
                // these independent kernels. Actual retained-boundary scans
                // add 25*3 / 41*3 respectively; neither is staged CFG work.
                let nominal_delta = if phase == 0 { 98 } else { 178 };
                let retained_delta = if phase == 0 { 25 * 3 } else { 41 * 3 };
                assert_eq!(
                    low,
                    known[phase] + physical_delta + nominal_delta + retained_delta,
                    "{label}/{phase}"
                );
                assert!(
                    check(known[phase] + physical_delta)
                        .unwrap_err()
                        .iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit)
                );
                eprintln!(
                    "HELPER_CFG_JOIN old-tail phase{phase} prior N={} now ResourceLimit; Origin +{physical_delta}, nominal graph +{nominal_delta}, retained scans +{retained_delta}",
                    known[phase],
                );
            }
            eprintln!("HELPER_CFG_JOIN {label} phase{phase} N={low}/N-1/fresh/retry");
        }
    }
}
#[test]
#[cfg(target_os = "linux")]
fn actual_prebranch_dag_native_clang_api_o0_o2() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for (case, text, expected) in CASES {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "fnonce-control-flow-{}-{case}-{backend:?}-{optimization:?}",
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
                    "{case}: {}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let run = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(run.status.success(), "{run:?}");
                assert_eq!(run.stdout, expected, "{case}/{backend:?}/{optimization:?}");
                assert!(run.stderr.is_empty());
                eprintln!(
                    "HELPER_CFG_JOIN native {case}/{backend:?}/{optimization:?}: {:?} {}",
                    String::from_utf8_lossy(&run.stdout),
                    dir.display()
                );
            }
        }
    }
}
