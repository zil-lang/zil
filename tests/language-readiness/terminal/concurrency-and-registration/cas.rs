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
                ("evaluation-order", include_str!("cas-evaluation-order.zil")),
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
        OBSERVED.replace("atomic_compare_exchange(cell,0,9)", "atomic_compare_exchange(cell,true,9)"),
        OBSERVED.replace("atomic_compare_exchange(cell,0,9)", "atomic_compare_exchange(cell,0,false)"),
        OBSERVED.replace("atomic_compare_exchange(cell,0,9)", "atomic_compare_exchange(cell,0)"),
        "fn unused(cell:&MutexI32)->(i32,bool){return atomic_compare_exchange(cell,0,1)} fn main()->i32{return 0}".to_owned(),
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

fn typed_cas() -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let limits = Limits::default();
    let core = expand(OBSERVED, &limits).unwrap().core;
    let typed = zil_types::check(&core, &zil_base::TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    (typed, plan)
}

#[test]
fn cas343_source_derived_raw_and_wire_order_matrix() {
    use zil_ir::{AtomicOrdering as O, InstructionKind as I};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let (typed, plan) = typed_cas();
    let orders = [O::Relaxed, O::Acquire, O::Release, O::AcqRel, O::SeqCst];
    // Explicit truth table, independent of each kernel's match implementation.
    let admitted = [
        [true, false, false, false, false],
        [true, true, false, false, false],
        [true, false, false, false, false],
        [true, true, false, false, false],
        [true, true, false, false, true],
    ];
    for (s, success) in orders.into_iter().enumerate() {
        for (f, failure) in orders.into_iter().enumerate() {
            let mut raw = typed.clone();
            let mut count = 0;
            for i in raw
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.instructions)
            {
                if let I::SyncI32CompareExchange {
                    success: a,
                    failure: b,
                    ..
                } = &mut i.kind
                {
                    assert_eq!((*a, *b), (O::SeqCst, O::SeqCst));
                    *a = success;
                    *b = failure;
                    count += 1;
                }
            }
            assert_eq!(count, 2);
            let mut claim = plan.clone();
            claim.module_fingerprint = raw.fingerprint();
            let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
            let (decoded, decoded_claim) =
                zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert_eq!(decoded.fingerprint(), raw.fingerprint());
            for input in [raw, decoded] {
                let analysis = zil_analyze::analyze(&input, &limits);
                let verification =
                    zil_verify::verify_module(input, decoded_claim.clone(), &target, &limits);
                if admitted[s][f] {
                    analysis.unwrap();
                    verification.unwrap();
                } else {
                    let errors = analysis.unwrap_err();
                    assert!(
                        errors.iter().any(|d| d.message.contains("order pair")),
                        "{errors:?}"
                    );
                    let errors = verification.unwrap_err();
                    assert!(
                        errors.iter().any(|d| d.message.contains("order pair")),
                        "{errors:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn cas343_each_operand_result_and_sealed_cell_are_independently_checked() {
    use zil_ir::{InstructionKind as I, Type, ValueId};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let (typed, plan) = typed_cas();
    for case in 0..8 {
        let mut raw = typed.clone();
        let f = raw
            .functions
            .iter_mut()
            .find(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, I::SyncI32CompareExchange { .. }))
            })
            .unwrap();
        let i = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, I::SyncI32CompareExchange { .. }))
            .unwrap();
        let I::SyncI32CompareExchange {
            address,
            expected,
            desired,
            ..
        } = &mut i.kind
        else {
            unreachable!()
        };
        match case {
            0 => *address = ValueId(u32::MAX),
            1 => *expected = ValueId(u32::MAX),
            2 => *desired = ValueId(u32::MAX),
            3 => f.values[expected.0 as usize] = Type::Bool,
            4 => f.values[desired.0 as usize] = Type::Bool,
            5 => f.values[i.result.unwrap().0 as usize] = Type::I32,
            6 | 7 => {
                f.values[address.0 as usize] = Type::Address {
                    mutable: false,
                    inner: Box::new(if case == 6 {
                        Type::I32
                    } else {
                        Type::SyncCell(zil_ir::SyncCellKind::MutexI32)
                    }),
                }
            }
            _ => unreachable!(),
        }
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
            .unwrap()
            .0;
        for input in [raw, decoded] {
            assert!(
                zil_analyze::analyze(&input, &limits).is_err(),
                "Analyze case={case}"
            );
            let errors =
                zil_verify::verify_module(input, claim.clone(), &target, &limits).unwrap_err();
            assert!(
                errors.iter().all(|d| d.code != "E-VERIFY-PLAN-IDENTITY"),
                "case={case}: {errors:?}"
            );
        }
    }
}

#[test]
fn cas343_decoded_wire_reaches_strong_machine_and_partition_cache() {
    use zil_backend_llvm::{BuildOptions, CodegenOptions, NativeBackend, Optimization};
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let core = expand(include_str!("cas-contention.zil"), &limits)
        .unwrap()
        .core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let claim = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &claim, &limits).unwrap();
    let verified = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    let text = zil_backend_llvm::emit(&verified, &lir).unwrap();
    assert!(text.contains("cmpxchg ptr"), "{text}");
    assert!(!text.contains("cmpxchg weak"));
    assert!(text.contains("seq_cst seq_cst"));
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-G8.3-wire-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let mut options = BuildOptions::new(dir.join("program"));
            options.backend = backend;
            options.optimization = optimization;
            options.codegen = CodegenOptions { units: 2, jobs: 2 };
            if backend == NativeBackend::LlvmApi {
                options.object_cache = Some(dir.join("cache"));
            }
            for run in 0..2 {
                let artifact = zil_backend_llvm::build(&verified, &lir, &options).unwrap();
                assert_eq!(artifact.codegen.backend, backend);
                assert!(artifact.codegen.fallback.is_none());
                if backend == NativeBackend::LlvmApi {
                    assert!(artifact.codegen.object_cache.enabled);
                    if run == 1 {
                        assert!(artifact.codegen.object_cache.hits > 0);
                    }
                }
                assert_eq!(
                    std::process::Command::new(&artifact.executable)
                        .status()
                        .unwrap()
                        .code(),
                    Some(0)
                );
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn cas343_independent_kernel_fees_have_exact_exhaustion_boundaries() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let (typed, plan) = typed_cas();
    for verify in [false, true] {
        let accepts = |steps| {
            let bounds = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            if verify {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &bounds).map(|_| ())
            } else {
                zil_analyze::analyze(&typed, &bounds).map(|_| ())
            }
        };
        let (mut lo, mut hi) = (1, limits.max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if accepts(mid).is_ok() {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        accepts(lo).unwrap();
        assert!(
            accepts(lo - 1)
                .unwrap_err()
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit)
        );
        accepts(lo).unwrap();
        eprintln!("G8.3 CAS kernel verify={verify} N={lo}; N-1 ResourceLimit/fresh N accepted");
    }
}
