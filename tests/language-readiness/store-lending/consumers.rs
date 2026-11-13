//! A returned Store-element field keeps its actual manager loan alive.
use super::{NEXT, Temp, execute, reject, source, typed};
use std::sync::atomic::Ordering;
use zil_base::{Limits, Phase, ResultCategory};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{HoldRoot, InstructionKind, Terminator};

const SHARED: &str = include_str!("shared-element-return.zil");
const OWNED: &str = include_str!("shared-owned-element-return.zil");
const GENERIC_OWNED: &str = include_str!("generic-owned-entry-return.zil");

#[test]
fn shared_store_element_field_return_executes_and_releases_manager_after_last_use() {
    let verified = compile_source(
        &source(SHARED),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        verified.status,
        ResultCategory::Success,
        "{}",
        verified.to_json()
    );
    assert_eq!(verified.verification_runs, 1);
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let temp = Temp::new();
            let output = temp.0.join(format!(
                "shared-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let report = compile_source(
                &source(SHARED),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    entry: Some("result".into()),
                    output: Some(output),
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
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            execute(&report.artifacts["executable"], 42);
        }
    }
}

#[test]
fn returned_store_window_blocks_live_structure_and_owner_move() {
    let push = SHARED.replace(
        "let observed=*value",
        "store_push(map.items,Item{data:bytes_new(),value:8})\n    let observed=*value",
    );
    reject(
        &push,
        ResultCategory::SafeReject,
        Phase::Analyze,
        "E-BORROW-CONFLICT",
    );
    let move_owner = SHARED
        .replace(
            "let observed=*value",
            "let moved=map\n    let observed=*value",
        )
        .replace(
            "store_push(map.items,Item{data:bytes_new(),value:8})\n    return observed",
            "return observed",
        );
    reject(
        &move_owner,
        ResultCategory::SafeReject,
        Phase::Analyze,
        "E-BORROW-CONFLICT",
    );
    let local = SHARED.replace(
        "fn result()->i64 {",
        "fn local_escape(map:&Map)->&i64 {\n    let items:Store<Item>=store_new()\n    let entry=store_at(items,0)\n    return &entry.value\n}\nfn result()->i64 {",
    );
    reject(
        &local,
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-ESCAPE",
    );
}

#[test]
fn returned_store_window_raw_and_wire_recompute_actual_origin() {
    let module = typed(SHARED);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();

    // Change the distinct-manager reserve into a reserve of the manager whose
    // returned element address is still live. Preserve the canonical weak
    // descriptor claim so actual loan reconstruction must reject the attack.
    let mut changed = module.clone();
    let caller = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "result")
        .unwrap();
    let caller_id = caller.id;
    let map = caller
        .locals
        .iter()
        .find(|local| local.name == "map")
        .unwrap()
        .id;
    let other = caller
        .locals
        .iter()
        .find(|local| local.name == "other")
        .unwrap()
        .id;
    let descriptor = caller
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find_map(|instruction| {
            let InstructionKind::DescribePackageField { local, index: 0 } = &mut instruction.kind
            else {
                return None;
            };
            if *local != other {
                return None;
            }
            *local = map;
            instruction.result
        })
        .unwrap();
    let mut changed_plan = plan.clone();
    changed_plan.module_fingerprint = changed.fingerprint();
    changed_plan.functions[caller_id.0 as usize]
        .holds
        .iter_mut()
        .find(|row| row.value == descriptor)
        .unwrap()
        .roots
        .clear();
    // Changing the real descriptor producer is the attack; a weak Desc
    // does not acquire public address debt from the substituted manager.
    assert_ne!(changed, module);
    let analyze = zil_analyze::analyze(&changed, &Limits::default()).unwrap_err();
    assert!(
        analyze
            .iter()
            .any(|d| d.phase == Phase::Analyze && d.code == "E-BORROW-CONFLICT"),
        "{analyze:?}"
    );
    let raw = zil_verify::verify_module(
        changed.clone(),
        changed_plan.clone(),
        &changed.target,
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        raw.iter()
            .any(|d| d.phase == Phase::Verify && d.code == "E-BORROW-CONFLICT"),
        "{raw:?}"
    );
    let changed_wire =
        zil_ir::wire::encode_module_candidate(&changed, &changed_plan, &Limits::default()).unwrap();
    let decoded =
        zil_verify::verify_module_bytes(&changed_wire, &changed.target, &Limits::default())
            .unwrap_err();
    assert!(
        decoded
            .iter()
            .any(|d| d.phase == Phase::Verify && d.code == "E-BORROW-CONFLICT"),
        "{decoded:?}"
    );

    let function = module.functions.iter().find(|f| f.name == "at").unwrap();
    let result = function
        .blocks
        .iter()
        .find_map(|block| match block.terminator {
            Terminator::Return(Some(value)) => Some(value),
            _ => None,
        })
        .unwrap();
    let mut forged = plan;
    let row = forged.functions[function.id.0 as usize]
        .holds
        .iter_mut()
        .find(|row| row.value == result)
        .unwrap();
    assert_eq!(row.roots, [HoldRoot::Parameter(0)]);
    row.roots = vec![HoldRoot::Static];
    let raw = zil_verify::verify_module(
        module.clone(),
        forged.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        raw.iter()
            .any(|d| d.phase == Phase::Verify && d.code == "E-VERIFY-HOLD"),
        "{raw:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&module, &forged, &Limits::default()).unwrap();
    let decoded =
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap_err();
    assert!(
        decoded
            .iter()
            .any(|d| d.phase == Phase::Verify && d.code == "E-VERIFY-HOLD"),
        "{decoded:?}"
    );
}

#[test]
fn shared_owned_store_element_result_keeps_complete_owner_and_executes() {
    for program in [OWNED, GENERIC_OWNED] {
        let verified = compile_source(
            &source(program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            verified.status,
            ResultCategory::Success,
            "{}",
            verified.to_json()
        );
        assert_eq!(verified.verification_runs, 1);
        let mut backends = vec![NativeBackend::Clang];
        if cfg!(feature = "llvm-api") {
            backends.push(NativeBackend::LlvmApi);
        }
        for backend in backends {
            for optimization in [Optimization::O0, Optimization::O2] {
                let temp = Temp::new();
                let output = temp.0.join(format!(
                    "owned-store-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                let report = compile_source(
                    &source(program),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("result".into()),
                        output: Some(output),
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
                assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                execute(&report.artifacts["executable"], 42);
            }
        }
    }
}

#[test]
fn shared_owned_store_result_rejects_live_mutation_move_and_wrong_source() {
    let push = OWNED.replace(
        "let observed=value.value",
        "store_push(map.items,Item{data:bytes_new(),value:9})\n    let observed=value.value",
    );
    reject(
        &push,
        ResultCategory::SafeReject,
        Phase::Analyze,
        "E-ANALYZE-IR",
    );
    let moved = OWNED
        .replace(
            "let observed=value.value",
            "let moved=map\n    let observed=value.value",
        )
        .replace(
            "store_push(map.items,Item{data:bytes_new(),value:8})\n    return observed",
            "return observed",
        );
    reject(
        &moved,
        ResultCategory::SafeReject,
        Phase::Analyze,
        "E-ANALYZE-IR",
    );
    let local = OWNED.replace(
        "fn result()->i64 {",
        "fn local_escape<life s>(map:&'s Map)->&'s Item {\n    let items:Store<Item>=store_new()\n    store_reserve(items,1)\n    store_push(items,Item{data:bytes_new(),value:5})\n    return store_at(items,0)\n}\nfn result()->i64 {",
    );
    reject(
        &local,
        ResultCategory::SafeReject,
        Phase::Analyze,
        "E-LIFE-RETURN",
    );
    let wrong = OWNED.replace(
        "fn result()->i64 {",
        "fn wrong<life s>(map:&'s Map,other:&Map)->&'s Item {\n    return store_at(other.items,0)\n}\nfn result()->i64 {",
    );
    reject(
        &wrong,
        ResultCategory::SafeReject,
        Phase::Analyze,
        "E-LIFE-RETURN",
    );
}

#[test]
fn shared_owned_store_result_raw_and_wire_require_actual_return_bound() {
    let module = typed(OWNED);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();

    let at = module.functions.iter().find(|f| f.name == "at").unwrap();
    let returned = at
        .blocks
        .iter()
        .find_map(|block| match block.terminator {
            Terminator::Return(Some(value)) => Some(value),
            _ => None,
        })
        .unwrap();
    let mut false_origin = plan.clone();
    let row = false_origin.functions[at.id.0 as usize]
        .holds
        .iter_mut()
        .find(|row| row.value == returned)
        .unwrap();
    assert_eq!(row.roots, [HoldRoot::Parameter(0)]);
    row.roots = vec![HoldRoot::Static];
    let raw = zil_verify::verify_module(
        module.clone(),
        false_origin.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        raw.iter()
            .any(|d| d.phase == Phase::Verify && d.code == "E-VERIFY-HOLD"),
        "{raw:?}"
    );
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &false_origin, &Limits::default()).unwrap();
    let decoded =
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap_err();
    assert!(
        decoded
            .iter()
            .any(|d| d.phase == Phase::Verify && d.code == "E-VERIFY-HOLD"),
        "{decoded:?}"
    );

    let mut forged = module;
    let callee = forged
        .functions
        .iter_mut()
        .find(|f| f.name == "at")
        .unwrap();
    assert_eq!(
        callee.reference_return_bound.as_deref(),
        Some([0].as_slice())
    );
    callee.reference_return_bound = None;
    let mut forged_plan = plan;
    forged_plan.module_fingerprint = forged.fingerprint();
    let raw = zil_verify::verify_module(
        forged.clone(),
        forged_plan.clone(),
        &forged.target,
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        raw.iter()
            .any(|d| d.phase == Phase::Verify && d.code == "E-VERIFY-RESOURCE"),
        "{raw:?}"
    );
    let wire =
        zil_ir::wire::encode_module_candidate(&forged, &forged_plan, &Limits::default()).unwrap();
    let decoded =
        zil_verify::verify_module_bytes(&wire, &forged.target, &Limits::default()).unwrap_err();
    assert!(
        decoded
            .iter()
            .any(|d| d.phase == Phase::Verify && d.code == "E-VERIFY-RESOURCE"),
        "{decoded:?}"
    );
}
