use super::*;
const CASES: [(&str, &str); 4] = [
    ("before", include_str!("loop-before.zil")),
    ("after", include_str!("loop-after.zil")),
    ("nested-transfer", include_str!("loop-transfer.zil")),
    ("consume-break", include_str!("loop-consume-break.zil")),
];
#[test]
fn g339_source_loop_origins_raw_wire() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (name, text) in CASES {
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
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        eprintln!("G339 {name}: actual Source/raw/Wire success");
    }
    let report = compile_source(
        &source(include_str!("loop-double.zil")),
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
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("generic loop must preserve"))
    );
}

#[test]
fn g339_multi_role_user_drop_and_managed_give_source_raw_wire() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (name, text) in [
        ("single-user-drop", include_str!("single-user-drop.zil")),
        ("mixed-roles-loop", include_str!("mixed-roles-loop.zil")),
        ("two-role-no-drop", include_str!("two-role-no-drop.zil")),
        ("two-role-user-drop", include_str!("two-role-user-drop.zil")),
        ("managed-companion", include_str!("managed-companion.zil")),
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
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        eprintln!("G339 {name}: actual Source/raw/Wire success");
    }
}

#[test]
fn g339_qualified_environment_lending_actual_source_raw_wire() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (name, text) in [
        ("lending", include_str!("qualified-lending.zil")),
        ("lending-mut", include_str!("qualified-lending-mut.zil")),
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
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let typed = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        assert!(
            typed
                .closures
                .iter()
                .any(|d| d.storage_life.is_some() && !d.lending_captures.is_empty())
        );
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        eprintln!("G339 {name}: actual Source/raw/Wire success");
    }
}

#[test]
#[cfg(target_os = "linux")]
fn g339_actual_completion_native_clang_api_o0_o2() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for (case, text, expected) in [
        ("before", include_str!("loop-before.zil"), "45\n"),
        (
            "nonconsuming-managed",
            include_str!("nonconsuming-managed.zil"),
            "ABC52\n0\n",
        ),
        (
            "five-role-composition",
            include_str!("five-role-drop-lending.zil"),
            "ABCCBA158\n0\n",
        ),
        ("after", include_str!("loop-after.zil"), "45\n"),
        ("nested", include_str!("loop-transfer.zil"), "45\n"),
        (
            "drop-order",
            include_str!("two-role-user-drop.zil"),
            "ABBA91\n0\n",
        ),
        (
            "mixed-roles-loop",
            include_str!("mixed-roles-loop.zil"),
            "48\n",
        ),
        (
            "give-companion",
            include_str!("managed-companion.zil"),
            "AS45\n0\n",
        ),
        (
            "consume-break",
            include_str!("loop-consume-break.zil"),
            "AB42\n0\n",
        ),
        ("lending", include_str!("qualified-lending.zil"), "0\n"),
        (
            "lending-mut",
            include_str!("qualified-lending-mut.zil"),
            "0\n",
        ),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-g339-{}-{case}-{backend:?}-{optimization:?}",
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
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let run = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(run.status.success(), "{run:?}");
                assert_eq!(
                    run.stdout,
                    expected.as_bytes(),
                    "{case}/{backend:?}/{optimization:?}"
                );
                assert!(run.stderr.is_empty());
                eprintln!(
                    "G339 native {case}/{backend:?}/{optimization:?}: {:?}",
                    String::from_utf8_lossy(&run.stdout)
                );
            }
        }
    }
}

#[test]
fn g339_environment_lending_nearest_live_receiver_rejections() {
    for (name, text) in [
        ("replace-live", include_str!("lending-move-live.zil")),
        ("exclusive-call-live", include_str!("lending-mut-live.zil")),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::ResourceLimit,
            "{name}: {}",
            report.to_json()
        );
        eprintln!("G339 {name} nearest rejection: {}", report.to_json());
    }
}

#[test]
fn g339_lending_field_claim_is_independent_of_copy_capture_type() {
    let text = include_str!("qualified-lending.zil").replace(
        "return &first_value",
        "let ignored=second_value;ignored;return &first_value",
    );
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let typed = zil_types::check(&g334_core(&text), &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let mut forged = typed.clone();
    let definition = forged
        .closures
        .iter_mut()
        .find(|d| {
            !d.lending_captures.is_empty()
                && d.captures
                    .iter()
                    .filter(|t| **t == zil_ir::Type::I32)
                    .count()
                    > 1
        })
        .unwrap();
    let actual = definition.lending_captures[0];
    let other = definition
        .captures
        .iter()
        .enumerate()
        .find(|(i, t)| *i != actual as usize && **t == zil_ir::Type::I32)
        .unwrap()
        .0 as u32;
    definition.lending_captures = vec![other];
    assert!(zil_analyze::analyze(&forged, &limits).is_err());
    let mut claim = plan;
    claim.module_fingerprint = forged.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
    for wire_mode in [false, true] {
        let result = if wire_mode {
            zil_verify::verify_module_bytes(&wire, &target, &limits)
        } else {
            zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits)
        };
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        eprintln!(
            "G339 forged same-type lending field wire={wire_mode}: {}",
            errors[0].message
        );
    }
}

#[test]
fn g339_completion_exact_two_kernel_budgets() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (case, text) in [
        ("cyclic", include_str!("loop-transfer.zil")),
        (
            "conditional-cleanup",
            include_str!("loop-consume-break.zil"),
        ),
        ("five-role", include_str!("five-role-drop-lending.zil")),
        ("retained-managed", include_str!("nonconsuming-managed.zil")),
        ("give-companion", include_str!("managed-companion.zil")),
        ("drop", include_str!("two-role-user-drop.zil")),
        ("lending", include_str!("qualified-lending-mut.zil")),
    ] {
        let typed = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        for phase in [0, 1, 2] {
            let check = |n| {
                let tight = Limits {
                    max_analysis_steps: n,
                    ..limits.clone()
                };
                match phase {
                    0 => zil_analyze::analyze(&typed, &tight).map(|p| assert_eq!(p, plan)),
                    1 => zil_verify::verify_module(typed.clone(), plan.clone(), &target, &tight)
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
                    Err(ds) => panic!("{case}/{phase}: {ds:?}"),
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
                "G339 {case}/phase{phase} exact N={low}; N-1 ResourceLimit, fresh retry success"
            );
        }
    }
}

#[test]
fn g339_merged_five_real_roles_mixed_cfg_drop_and_single_role_lending() {
    let text = include_str!("five-role-drop-lending.zil");
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
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let typed = zil_types::check(&g334_core(text), &target, &limits).unwrap();
    assert!(typed.functions.iter().any(|f| {
        f.callable_contract
            .as_ref()
            .is_some_and(|c| c.roles.len() == 5)
    }));
    assert!(
        typed
            .closures
            .iter()
            .any(|d| d.storage_life.is_some() && !d.lending_captures.is_empty())
    );
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    eprintln!(
        "G339 merged five qualified roles: mixed Fn/FnMut/FnOnce cyclic CFG + user Drop + separate single-role lending actual Source/raw/Wire success"
    );
}

#[test]
fn g339_nonconsuming_managed_capture_actual_repeat_and_lending() {
    let text = include_str!("nonconsuming-managed.zil");
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
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let typed = zil_types::check(&g334_core(text), &target, &limits).unwrap();
    for kind in [zil_ir::CallableKind::Fn, zil_ir::CallableKind::FnMut] {
        assert!(typed.closures.iter().any(|d| {
            d.kind == kind
                && d.storage_life.is_some()
                && d.captures
                    .iter()
                    .any(|t| matches!(t, zil_ir::Type::Managed(_)))
        }));
    }
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    eprintln!(
        "G339 nonconsuming Managed Fn/FnMut actual Source/raw/Wire repeat + separate single-role lending success"
    );
}

#[test]
fn g339_nonconsuming_managed_entry_cannot_forge_capture_transfer() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let typed = zil_types::check(
        &g334_core(include_str!("nonconsuming-managed.zil")),
        &target,
        &limits,
    )
    .unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for kind in [zil_ir::CallableKind::Fn, zil_ir::CallableKind::FnMut] {
        let mut forged = typed.clone();
        let id = forged
            .closures
            .iter()
            .find(|d| {
                d.kind == kind
                    && d.storage_life.is_some()
                    && d.captures
                        .iter()
                        .any(|t| matches!(t, zil_ir::Type::Managed(_)))
            })
            .unwrap()
            .id;
        let entry = forged
            .functions
            .iter_mut()
            .find(|f| f.environment == Some(id))
            .unwrap();
        let instruction = entry
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, zil_ir::InstructionKind::DescribeCapture { .. }))
            .unwrap();
        let zil_ir::InstructionKind::DescribeCapture { index } = instruction.kind else {
            unreachable!()
        };
        instruction.kind = zil_ir::InstructionKind::TakeCapture { index };
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        for wire_mode in [false, true] {
            let errors = if wire_mode {
                zil_verify::verify_module_bytes(&wire, &target, &limits)
            } else {
                zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits)
            }
            .unwrap_err();
            assert!(
                errors
                    .iter()
                    .all(|d| d.category != ResultCategory::ResourceLimit),
                "{errors:?}"
            );
            eprintln!(
                "G339 actual {kind:?} forged owner transfer wire={wire_mode}: {}",
                errors[0].message
            );
        }
    }
}

#[test]
fn g339_lending_live_exclusive_call_is_rejected_independently_from_source_plan() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let good = zil_types::check(
        &g334_core(include_str!("qualified-lending-mut.zil")),
        &target,
        &limits,
    )
    .unwrap();
    let mut claim = zil_analyze::analyze(&good, &limits).unwrap();
    // Use a real source-produced body with the first returned pointer live
    // across the second exclusive call. No Analyze result exists for this body.
    let bad = zil_types::check(
        &g334_core(include_str!("lending-mut-live.zil")),
        &target,
        &limits,
    )
    .unwrap();
    claim.module_fingerprint = bad.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&bad, &claim, &limits).unwrap();
    for wire_mode in [false, true] {
        let errors = if wire_mode {
            zil_verify::verify_module_bytes(&wire, &target, &limits)
        } else {
            zil_verify::verify_module(bad.clone(), claim.clone(), &target, &limits)
        }
        .unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{errors:?}"
        );
        eprintln!(
            "G339 independent actual live receiver wire={wire_mode}: {}",
            errors[0].message
        );
    }
}

#[test]
fn g339_actual_cycle_reentering_owner_consumption_is_rejected_by_both_kernels() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let typed = zil_types::check(
        &g334_core(include_str!("loop-before.zil")),
        &target,
        &limits,
    )
    .unwrap();
    let mut claim = zil_analyze::analyze(&typed, &limits).unwrap();
    let mut forged = typed.clone();
    let helper = forged
        .functions
        .iter_mut()
        .find(|f| {
            f.callable_contract.as_ref().is_some_and(|c| {
                c.roles
                    .iter()
                    .any(|r| r.kind == zil_ir::CallableKind::FnOnce)
            })
        })
        .unwrap();
    let block = helper
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, zil_ir::InstructionKind::CallClosureOnce { .. }))
        })
        .unwrap();
    block.terminator = zil_ir::Terminator::Jump(block.id);
    claim.module_fingerprint = forged.fingerprint();
    let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .all(|d| d.category != ResultCategory::ResourceLimit),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
    for wire_mode in [false, true] {
        let errors = if wire_mode {
            zil_verify::verify_module_bytes(&wire, &target, &limits)
        } else {
            zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits)
        }
        .unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        eprintln!(
            "G339 actual consuming backedge wire={wire_mode}: {}",
            errors[0].message
        );
    }
}

#[test]
fn g339_scalar_foreign_actual_declaration_and_region_are_independent() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let text = format!(
        "{}\nextern \"C\" fn other(code:i32)->i32;\n",
        include_str!("two-role-user-drop.zil")
    );
    let typed = zil_types::check(&g334_core(&text), &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for lane in [
        "abi",
        "noescape",
        "canonical",
        "duplicate-symbol",
        "region",
        "ordinary-call",
    ] {
        let mut forged = typed.clone();
        if lane == "region" || lane == "ordinary-call" {
            let call = forged
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, zil_ir::InstructionKind::ForeignCall { .. }))
                .unwrap();
            if lane == "region" {
                let zil_ir::InstructionKind::ForeignCall { region, .. } = &mut call.kind else {
                    unreachable!()
                };
                *region = region.saturating_add(100);
            } else {
                let zil_ir::InstructionKind::ForeignCall {
                    function,
                    arguments,
                    ..
                } = &call.kind
                else {
                    unreachable!()
                };
                call.kind = zil_ir::InstructionKind::Call {
                    function: *function,
                    arguments: arguments.clone(),
                };
            }
        } else {
            let foreign = forged
                .functions
                .iter_mut()
                .find(|f| f.foreign.as_ref().is_some_and(|d| d.symbol == "other"))
                .unwrap();
            match lane {
                "abi" => foreign.foreign.as_mut().unwrap().abi = "Rust".into(),
                "noescape" => foreign.foreign.as_mut().unwrap().noescape_parameters = vec![0],
                "canonical" => foreign.locals[0].mutable = true,
                "duplicate-symbol" => foreign.foreign.as_mut().unwrap().symbol = "putchar".into(),
                _ => unreachable!(),
            }
        }
        let errors = zil_analyze::analyze(&forged, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit),
            "{lane}: {errors:?}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        for wire_mode in [false, true] {
            let errors = if wire_mode {
                zil_verify::verify_module_bytes(&wire, &target, &limits)
            } else {
                zil_verify::verify_module(forged.clone(), claim.clone(), &target, &limits)
            }
            .unwrap_err();
            assert!(
                errors
                    .iter()
                    .all(|d| d.category != ResultCategory::ResourceLimit),
                "{lane}: {errors:?}"
            );
            eprintln!(
                "G339 scalar foreign {lane} independently rejected wire={wire_mode}: {}",
                errors[0].message
            );
        }
    }
}
