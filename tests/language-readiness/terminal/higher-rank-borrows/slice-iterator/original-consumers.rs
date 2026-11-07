//! The original readiness source is the acceptance target, unchanged.
use super::*;

const ORIGINAL: &str = include_str!("../slice_iter.zil");

fn typed_original() -> TypedModule {
    let unit = source(ORIGINAL);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let aliases = std::collections::BTreeMap::new();
    let expanded = zil_expand::expand_with_map_limits_target_context(
        &unit,
        &syntax,
        &Limits::default(),
        &TargetProfile::default(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &aliases,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap();
    let core = zil_types::infer_source_closures(&expanded.core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

#[test]
fn slice_iter_original_source_two_kernels_raw_and_wire() {
    let module = typed_original();
    // The source's scalar read through &mut self must retain the actual base
    // mode. Derive the nearest counterfeit from that real Typed instruction.
    let mut selected = None;
    'functions: for (function_index, function) in module.functions.iter().enumerate() {
        for block in &function.blocks {
            for instruction in &block.instructions {
                let zil_ir::InstructionKind::AddressField { address, .. } = instruction.kind else {
                    continue;
                };
                let Some(result) = instruction.result else {
                    continue;
                };
                if ORIGINAL.get(instruction.span.start..instruction.span.end) == Some("self.index")
                    && matches!(&function.values[address.0 as usize], zil_ir::Type::Address { mutable: true, inner }
                        if matches!(inner.as_ref(), zil_ir::Type::BorrowedStorage { .. }))
                    && matches!(&function.values[result.0 as usize], zil_ir::Type::Address { mutable: true, inner }
                        if inner.as_ref() == &zil_ir::Type::I64)
                {
                    selected = Some((function_index, result.0 as usize));
                    break 'functions;
                }
            }
        }
    }
    let (function_index, value) =
        selected.expect("actual self.index scalar read through &mut self");
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let (decoded, claim) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    assert_eq!(decoded, module);
    assert_eq!(zil_analyze::analyze(&decoded, &limits).unwrap(), plan);
    zil_verify::verify_module(decoded, claim, &module.target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();

    let mut forged = module.clone();
    let zil_ir::Type::Address { mutable, .. } = &mut forged.functions[function_index].values[value]
    else {
        unreachable!()
    };
    *mutable = false;
    assert!(zil_analyze::analyze(&forged, &limits).is_err());
    let mut forged_plan = plan.clone();
    forged_plan.module_fingerprint = forged.fingerprint();
    let forged_wire =
        zil_ir::wire::encode_module_candidate(&forged, &forged_plan, &limits).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&forged_wire, &limits).unwrap();
    assert!(zil_analyze::analyze(&decoded, &limits).is_err());
    assert!(zil_verify::verify_module(forged, forged_plan, &module.target, &limits).is_err());
    assert!(zil_verify::verify_module_bytes(&forged_wire, &module.target, &limits).is_err());
}

#[test]
fn slice_iter_original_source_native_result_42() {
    let temp = Temp::new();
    let mut backends = vec![zil_driver::NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(zil_driver::NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
            let report = compile_source(
                &source(ORIGINAL),
                &CompileOptions {
                    stop_after: Stage::Build,
                    entry: Some("result".into()),
                    backend,
                    optimization,
                    output: Some(temp.0.join("slice-iterator")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            execute(&report.artifacts["executable"], 42);
        }
    }
}

#[test]
fn slice_iter_actual_target_invalidation_is_rejected() {
    let baseline = compile_source(&source(ORIGINAL), &CompileOptions::default());
    assert_eq!(
        baseline.status,
        ResultCategory::Success,
        "original source must be accepted before testing target invalidation: {}",
        baseline.to_json()
    );
    let invalid = ORIGINAL
        .replace("let values:[i64;3]", "var values:[i64;3]")
        .replace("    match first{", "    values[0]=99;\n    match first{");
    assert_ne!(invalid, ORIGINAL);
    let report = compile_source(&source(&invalid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(report.phase, Phase::Analyze | Phase::Verify),
        "{}",
        report.to_json()
    );
}
