//! Independent Source, native, and Source-derived raw/Wire ordinary Store ownership consumers.
use super::*;

#[path = "allocation.rs"]
mod allocation;

#[test]
fn ordinary_store_ownership_reaches_complete_native_values() {
    for (label, source, expected, imported) in [
        (
            "ordinary-tuple-copy-companions",
            include_str!("ordinary-tuple-copy-companions.zil"),
            b"19\n".as_slice(),
            false,
        ),
        (
            "ordinary-data-model",
            include_str!("ordinary-data-model.zil"),
            b"18446744073709551612\n".as_slice(),
            true,
        ),
        (
            "ordinary-sums",
            include_str!("ordinary-sums.zil"),
            b"18446744073709551613\n".as_slice(),
            false,
        ),
        (
            "ordinary-captures",
            include_str!("ordinary-captures.zil"),
            b"18446744073709551612\n".as_slice(),
            false,
        ),
        (
            "ordinary-direct-captures",
            include_str!("ordinary-direct-captures.zil"),
            b"18446744073709551612\n".as_slice(),
            false,
        ),
        (
            "ordinary-pair-baseline",
            include_str!("ordinary-pair-baseline.zil"),
            b"18446744073709551611\n".as_slice(),
            false,
        ),
        (
            "ordinary-prepare-failure",
            include_str!("ordinary-prepare-failure.zil"),
            b"18446744073709551612\n".as_slice(),
            false,
        ),
        (
            "ordinary-parent-two-views",
            include_str!("ordinary-parent-two-views.zil"),
            b"18446744073709551613\n".as_slice(),
            false,
        ),
        (
            "ordinary-error-side",
            include_str!("ordinary-error-side.zil"),
            b"18446744073709551610\n".as_slice(),
            false,
        ),
        (
            "ordinary-tuple-field",
            include_str!("ordinary-tuple-field.zil"),
            b"18446744073709551611\n".as_slice(),
            false,
        ),
        (
            "ordinary-enum-field",
            include_str!("ordinary-enum-field.zil"),
            b"18446744073709551612\n".as_slice(),
            false,
        ),
        (
            "ordinary-error-field",
            include_str!("ordinary-error-field.zil"),
            b"18446744073709551613\n".as_slice(),
            false,
        ),
        (
            "ordinary-sum-capture",
            include_str!("ordinary-sum-capture.zil"),
            b"18446744073709551614\n".as_slice(),
            false,
        ),
        (
            "ordinary-direct-shape-captures",
            include_str!("ordinary-direct-shape-captures.zil"),
            b"18446744073709551614\n".as_slice(),
            false,
        ),
        (
            "ordinary-two-sum-captures",
            include_str!("ordinary-two-sum-captures.zil"),
            b"28\n".as_slice(),
            false,
        ),
        (
            "ordinary-fnmut-error-capture",
            include_str!("ordinary-fnmut-error-capture.zil"),
            b"17\n".as_slice(),
            false,
        ),
        (
            "ordinary-both-owned-error",
            include_str!("ordinary-both-owned-error.zil"),
            b"18446744073709551613\n".as_slice(),
            false,
        ),
    ] {
        super::g933_consumers::native_entry_all_backends(
            label,
            source,
            expected,
            imported,
            Some("result"),
        );
    }
}

#[test]
fn ordinary_store_live_parent_view_and_duplicate_responsibility_reject() {
    for (label, text, expected_code) in [
        (
            "live-parent-view",
            include_str!("ordinary-live-view.zil"),
            "E-BORROW-CONFLICT",
        ),
        (
            "duplicate-parent",
            include_str!("ordinary-duplicate.zil"),
            "E-UNINITIALIZED",
        ),
        (
            "same-store-different-index",
            include_str!("ordinary-parent-same-store-index.zil"),
            "E-ANALYZE-IR",
        ),
        (
            "live-parent-move",
            include_str!("ordinary-parent-live-move.zil"),
            "E-BORROW-CONFLICT",
        ),
    ] {
        let report = check(label, text, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == expected_code),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn ordinary_store_descriptor_alias_cannot_become_a_second_owner() {
    let report = check(
        "ordinary-give-alias",
        include_str!("ordinary-give-alias.zil"),
        Stage::Verify,
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-GIVE-DESCRIPTOR"),
        "{}",
        report.to_json()
    );
}

#[test]
fn ordinary_ownership_does_not_admit_a_nested_store_element() {
    let report = check(
        "ordinary-not-store-element",
        include_str!("ordinary-not-store-element.zil"),
        Stage::Verify,
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-UNSUPPORTED" && d.message.contains("Store elements require")),
        "{}",
        report.to_json()
    );
}

#[test]
fn ordinary_direct_sum_capture_source_rejects_lost_ownership() {
    let source = include_str!("ordinary-direct-shape-captures.zil");
    for (label, from, to, phase, code) in [
        (
            "shared-takes-owner",
            "let (ref values,tag)=pair",
            "let previous=pair\n        let (values,tag)=previous",
            Phase::Analyze,
            "E-ANALYZE-IR",
        ),
        (
            "mutable-return-unfilled",
            "choice=Choice.Value(make(18446744073709551612u64))",
            "if false { choice=Choice.Value(make(18446744073709551612u64)) }",
            Phase::Types,
            "E-UNINITIALIZED",
        ),
    ] {
        assert_eq!(
            source.matches(from).count(),
            1,
            "{label} must change its real capture body"
        );
        let changed = source.replacen(from, to, 1);
        let report = check(label, &changed, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{label}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{label}: {}",
            report.to_json()
        );
    }
    let error_source = include_str!("ordinary-fnmut-error-capture.zil");
    let refill_then_error = "        choice=Choice.Value(make(12u64))\n        if store_len(make(1u64))==1 { return fail() }";
    let error_then_refill = "        if store_len(make(1u64))==1 { return fail() }\n        choice=Choice.Value(make(12u64))";
    assert_eq!(error_source.matches(refill_then_error).count(), 1);
    let report = check(
        "mutable-error-exit-unfilled",
        &error_source.replacen(refill_then_error, error_then_refill, 1),
        Stage::Verify,
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-UNINITIALIZED"),
        "{}",
        report.to_json()
    );
}

#[test]
fn ordinary_tuple_destructure_consumes_its_actual_owned_capture() {
    let (module, _) = candidate(include_str!("ordinary-tuple-copy-companions.zil"));
    let definition = module
        .closures
        .iter()
        .find(|definition| {
            matches!(&definition.captures[..], [Type::Tuple(fields)]
                if fields.len() == 3
                    && matches!(&fields[0], Type::Managed(payload)
                        if matches!(payload.as_ref(), Type::OwnedStore { element }
                            if matches!(element.as_ref(), Type::U64)))
                    && matches!(&fields[1], Type::Record { .. })
                    && matches!(&fields[2], Type::Array { .. }))
        })
        .expect("Source captures its complete Store/Record/Array Tuple");
    assert_eq!(definition.kind, zil_ir::CallableKind::FnOnce);
    let entry = module
        .functions
        .iter()
        .find(|function| {
            function.id == definition.function && function.environment == Some(definition.id)
        })
        .expect("actual Source closure entry");
    assert!(
        entry
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction.kind, I::TakeCapture { index: 0 }))
    );
    assert!(
        entry
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction.kind, I::UnpackTuple { .. }))
    );
}

#[test]
fn ordinary_tuple_capture_mode_follows_owning_child_binding() {
    let source = include_str!("ordinary-tuple-copy-companions.zil");
    let original = "let (values,marker,bytes)=packet";
    assert_eq!(source.matches(original).count(), 1);
    let shared = source
        .replacen("impl FnOnce()->u64", "impl Fn()->u64", 1)
        .replacen(original, "let (ref values,marker,bytes)=packet", 1);
    let (module, _) = candidate(&shared);
    assert!(module.closures.iter().any(|definition| {
        definition.kind == zil_ir::CallableKind::Fn
            && matches!(&definition.captures[..], [Type::Tuple(fields)] if fields.len() == 3)
    }));

    let mixed = source.replacen(original, "let (values,ref marker,bytes)=packet", 1);
    let (module, _) = candidate(&mixed);
    assert!(module.closures.iter().any(|definition| {
        definition.kind == zil_ir::CallableKind::FnOnce
            && matches!(&definition.captures[..], [Type::Tuple(fields)] if fields.len() == 3)
    }));

    let discarded = source
        .replacen(original, "let (_,marker,bytes)=packet", 1)
        .replacen(
            "return *store_at(values,0)+marker.n+bytes[0]",
            "return marker.n+bytes[0]",
            1,
        );
    let (module, _) = candidate(&discarded);
    assert!(module.closures.iter().any(|definition| {
        definition.kind == zil_ir::CallableKind::FnOnce
            && matches!(&definition.captures[..], [Type::Tuple(fields)] if fields.len() == 3)
    }));
}

#[test]
fn ordinary_store_same_size_canonical_element_rejects_raw_and_wire() {
    let (mut module, mut plan) = candidate(include_str!("ordinary-sums.zil"));
    let store = module
        .resources
        .iter_mut()
        .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
        .expect("Source Store definition");
    let Type::OwnedStore { element } = &mut store.payload else {
        unreachable!()
    };
    let Type::Record { name, .. } = element.as_mut() else {
        panic!("Source POD element must retain nominal Record")
    };
    name.push_str("_same_size_forgery");
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let out = directory("ordinary-same-size-canonical");
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ] {
        let errors = result.expect_err(route);
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject),
            "{route}: {errors:?}"
        );
        fs::write(out.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
    }
}

#[test]
fn ordinary_store_actual_parent_field_receiver_rejects_stale_raw_and_wire_provenance() {
    let (mut module, mut plan) = candidate(include_str!("ordinary-parent-two-views.zil"));
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.name == "result")
        .unwrap();
    let function_id = function.id;
    let mut first = None;
    let mut changed = false;
    let mut changed_results = None;
    for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::DescribePackageField { local, index } = &mut instruction.kind {
            if let Some((original_local, original_index, original_result)) = first {
                if *local != original_local || *index == original_index {
                    continue;
                }
                *index = original_index;
                changed = true;
                changed_results = Some((original_result, instruction.result.unwrap()));
                break;
            } else {
                first = Some((*local, *index, instruction.result.unwrap()));
            }
        }
    }
    assert!(
        changed,
        "Source must describe two distinct Store fields of the same parent"
    );
    let (first_result, second_result) = changed_results.unwrap();
    let holds = &plan
        .functions
        .iter()
        .find(|row| row.function == function_id)
        .unwrap()
        .holds;
    assert_eq!(
        holds[first_result.0 as usize].roots, holds[second_result.0 as usize].roots,
        "public HoldRoot deliberately folds sibling fields under their actual parent Local"
    );
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let out = directory("ordinary-sibling-receiver-alias");
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ] {
        let errors = result.expect_err(route);
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject),
            "{route}: {errors:?}"
        );
        fs::write(out.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
    }
}

#[test]
fn ordinary_store_pair_cannot_duplicate_one_parent_raw_or_wire() {
    let text = include_str!("ordinary-pair-baseline.zil");
    let (mut module, mut plan) = candidate(text);
    let mut mutated = false;
    for instruction in module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let I::MakePackage { fields } = &mut instruction.kind {
            if fields.len() == 2 {
                fields[1] = fields[0];
                mutated = true;
            }
        }
    }
    assert!(mutated, "Source must construct the two-parent Pair");
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let out = directory("ordinary-double-parent");
    fs::write(out.join("source.zil"), text).unwrap();
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ] {
        let errors = result.expect_err(route);
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject),
            "{route}: {errors:?}"
        );
        fs::write(out.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
    }
}

fn two_owned_sum_capture_entry(module: &zil_ir::TypedModule) -> (usize, usize) {
    let (definition_index, definition) = module
        .closures
        .iter()
        .enumerate()
        .find(|(_, definition)| {
            definition.kind == zil_ir::CallableKind::FnMut
                && definition.captures.len() == 2
                && definition.captures.iter().all(|ty| {
                    matches!(ty, Type::Enum { variants, .. }
                        if variants.len() == 1
                            && variants[0].1.len() == 1
                            && matches!(&variants[0].1[0], Type::Managed(payload)
                                if matches!(payload.as_ref(), Type::OwnedStore { element }
                                    if matches!(element.as_ref(), Type::U64))))
                })
        })
        .expect("Source FnMut has two actual owned Enum<Store<u64>> captures");
    let function_index = module
        .functions
        .iter()
        .position(|function| {
            function.id == definition.function && function.environment == Some(definition.id)
        })
        .expect("actual Source closure entry relation");
    let entry = &module.functions[function_index];
    for index in 0..2 {
        assert!(
            entry
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|i| { matches!(i.kind, I::TakeCapture { index: actual } if actual == index) })
        );
        assert!(
            entry
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|i| {
                    matches!(i.kind, I::WriteCapture { index: actual, .. } if actual == index)
                })
        );
    }
    (definition_index, function_index)
}

fn reject_direct_sum_capture_candidate(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    code: &str,
    message: &str,
) {
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let out = directory(label);
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ] {
        let errors = result.expect_err(route);
        assert!(
            errors.iter().any(|d| {
                d.phase == Phase::Verify
                    && d.category == ResultCategory::SafeReject
                    && d.code == code
                    && d.message == message
            }),
            "{label}/{route}: {errors:?}"
        );
        fs::write(out.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
    }
}

#[test]
fn ordinary_source_derived_two_sum_capture_debts_reject_raw_and_wire() {
    let (module, plan) = candidate(include_str!("ordinary-two-sum-captures.zil"));
    let (definition_index, function_index) = two_owned_sum_capture_entry(&module);

    let mut shared_take = module.clone();
    shared_take.closures[definition_index].kind = zil_ir::CallableKind::Fn;
    let closure = shared_take.closures[definition_index].id;
    let mut claims = 0;
    let mut calls = 0;
    for function in &mut shared_take.functions {
        if let Some(contract) = &mut function.callable_return_contract
            && contract.closure == closure
        {
            assert_eq!(contract.kind, zil_ir::CallableKind::FnMut);
            contract.kind = zil_ir::CallableKind::Fn;
            claims += 1;
        }
        for instruction in function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
        {
            if let I::CallClosureMut { callee, arguments } = &instruction.kind
                && function.locals[callee.0 as usize].ty == Type::Closure(closure)
            {
                instruction.kind = I::CallClosureShared {
                    callee: *callee,
                    arguments: arguments.clone(),
                };
                calls += 1;
            }
        }
    }
    assert_eq!(
        claims, 1,
        "actual Source factory has one returned environment claim"
    );
    assert_eq!(calls, 2, "actual Source invokes its environment twice");
    reject_direct_sum_capture_candidate(
        "ordinary-shared-takes-owned-sum",
        shared_take,
        plan.clone(),
        "E-VERIFY-CLOSURE-CALL",
        "moving an owned capture requires a Consume entry",
    );

    let mut unfilled = module.clone();
    let entry = &mut unfilled.functions[function_index];
    let before = entry
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|i| matches!(i.kind, I::WriteCapture { index: 0, .. }))
        .count();
    assert_eq!(before, 1, "Source has one actual refill for capture 0");
    for block in &mut entry.blocks {
        block
            .instructions
            .retain(|i| !matches!(i.kind, I::WriteCapture { index: 0, .. }));
    }
    reject_direct_sum_capture_candidate(
        "ordinary-normal-exit-unfilled-capture",
        unfilled,
        plan.clone(),
        "E-VERIFY-SUM",
        "Fn/FnMut return leaves an unfilled direct Sum capture",
    );

    let mut duplicate_take = module.clone();
    let entry = &mut duplicate_take.functions[function_index];
    let second_take = entry
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|i| matches!(i.kind, I::TakeLocal { local } if local.0 == 0))
        .expect("Source FnMut takes its first actual activation owner");
    assert!(matches!(
        entry.values[second_take.result.unwrap().0 as usize],
        Type::Enum { .. }
    ));
    second_take.kind = I::TakeCapture { index: 0 };
    reject_direct_sum_capture_candidate(
        "ordinary-takes-same-capture-twice",
        duplicate_take,
        plan.clone(),
        "E-VERIFY-SUM",
        "direct Sum capture was already transferred",
    );

    let mut live_write = module.clone();
    let entry = &mut live_write.functions[function_index];
    let (block, position) = entry
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block, body)| {
            body.instructions
                .iter()
                .position(|i| matches!(i.kind, I::WriteCapture { index: 1, .. }))
                .map(|position| (block, position))
        })
        .expect("Source FnMut refills its second actual capture");
    let duplicate = entry.blocks[block].instructions[position].clone();
    entry.blocks[block]
        .instructions
        .insert(position + 1, duplicate);
    reject_direct_sum_capture_candidate(
        "ordinary-overwrites-live-capture",
        live_write,
        plan.clone(),
        "E-VERIFY-SUM",
        "direct Sum capture refill would overwrite an active owner",
    );

    let mut wrong_index = module.clone();
    let entry = &mut wrong_index.functions[function_index];
    let write = entry
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|i| matches!(i.kind, I::WriteCapture { index: 1, .. }))
        .expect("Source FnMut writes an in-range capture field");
    if let I::WriteCapture { index, .. } = &mut write.kind {
        *index = 0;
    }
    reject_direct_sum_capture_candidate(
        "ordinary-wrong-valid-capture-index",
        wrong_index,
        plan,
        "E-VERIFY-SUM",
        "direct Sum capture refill would overwrite an active owner",
    );
}

#[test]
fn ordinary_source_derived_error_exit_must_refill_sum_capture_raw_and_wire() {
    let (mut module, plan) = candidate(include_str!("ordinary-fnmut-error-capture.zil"));
    let definition = module
        .closures
        .iter()
        .find(|definition| {
            definition.kind == zil_ir::CallableKind::FnMut
                && definition.captures.len() == 1
                && matches!(definition.captures[0], Type::Enum { .. })
                && matches!(&definition.result, Type::ErrorUnion { error, result }
                    if matches!(error.as_ref(), Type::I32)
                        && matches!(result.as_ref(), Type::U64))
        })
        .expect("Source FnMut error entry with one actual owning Sum capture");
    let entry_id = definition.function;
    let environment = definition.id;
    let failure = module
        .functions
        .iter()
        .find(|function| {
            function.environment.is_none()
                && function.parameters.is_empty()
                && matches!(&function.return_type, Type::ErrorUnion { error, result }
                    if matches!(error.as_ref(), Type::I32)
                        && matches!(result.as_ref(), Type::U64))
        })
        .expect("Source calls a real ErrorUnion producer")
        .id;
    let entry = module
        .functions
        .iter_mut()
        .find(|function| function.id == entry_id && function.environment == Some(environment))
        .expect("Source error closure entry relation");
    let error_block = entry
        .blocks
        .iter_mut()
        .find(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(instruction.kind, I::Call { function, .. } if function == failure)
            }) && block
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.kind, I::WriteCapture { index: 0, .. }))
        })
        .expect("actual error return path refills the owning capture");
    error_block
        .instructions
        .retain(|instruction| !matches!(instruction.kind, I::WriteCapture { index: 0, .. }));
    reject_direct_sum_capture_candidate(
        "ordinary-error-exit-unfilled-capture",
        module,
        plan,
        "E-VERIFY-SUM",
        "Fn/FnMut return leaves an unfilled direct Sum capture",
    );
}

#[test]
fn ordinary_owning_store_request_budgets_are_exact_and_fresh() {
    for (label, text) in [
        ("siblings", include_str!("ordinary-parent-two-views.zil")),
        ("sum", include_str!("ordinary-sums.zil")),
        ("capture", include_str!("ordinary-direct-captures.zil")),
    ] {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "ordinary-budget.zil", text);
        for work in [true, false] {
            let options = |n| {
                let mut o = CompileOptions::default();
                if work {
                    o.limits.max_analysis_steps = n;
                } else {
                    o.limits.max_nodes = n;
                }
                o
            };
            let mut low = 1;
            let mut high = if work {
                Limits::default().max_analysis_steps
            } else {
                Limits::default().max_nodes
            };
            while low < high {
                let n = low + (high - low) / 2;
                let report = compile_source(&unit, &options(n));
                match report.status {
                    ResultCategory::Success => high = n,
                    ResultCategory::ResourceLimit => low = n + 1,
                    _ => panic!("{label}: {}", report.to_json()),
                }
            }
            let mut session = zil_driver::CompilationSession::default();
            for (n, expected) in [
                (low, ResultCategory::Success),
                (low - 1, ResultCategory::ResourceLimit),
                (low, ResultCategory::Success),
            ] {
                for report in [
                    session.compile_source(&unit, &options(n)),
                    zil_driver::CompilationSession::default().compile_source(&unit, &options(n)),
                ] {
                    assert_eq!(report.status, expected, "{label}: {}", report.to_json());
                }
                assert_eq!(session.active_query_count(), 0);
            }
            eprintln!("G9.3.5 {label} work={work} N={low}; N-1/cold/warm/fresh agree");
        }
    }
}
