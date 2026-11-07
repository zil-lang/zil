//! Independent Source, native, and Source-derived raw/Wire ordinary Store ownership consumers.
use super::*;

#[path = "allocation.rs"]
mod allocation;

#[test]
fn ordinary_store_ownership_reaches_complete_native_values() {
    for (label, source, expected, imported) in [
        ("ordinary-data-model", include_str!("ordinary-data-model.zil"), b"18446744073709551612\n".as_slice(), true),
        ("ordinary-sums", include_str!("ordinary-sums.zil"), b"18446744073709551613\n".as_slice(), false),
        ("ordinary-captures", include_str!("ordinary-captures.zil"), b"18446744073709551612\n".as_slice(), false),
        ("ordinary-direct-captures", include_str!("ordinary-direct-captures.zil"), b"18446744073709551612\n".as_slice(), false),
        ("ordinary-pair-baseline", include_str!("ordinary-pair-baseline.zil"), b"18446744073709551611\n".as_slice(), false),
        ("ordinary-prepare-failure", include_str!("ordinary-prepare-failure.zil"), b"18446744073709551612\n".as_slice(), false),
        ("ordinary-parent-two-views", include_str!("ordinary-parent-two-views.zil"), b"18446744073709551613\n".as_slice(), false),
        ("ordinary-error-side", include_str!("ordinary-error-side.zil"), b"18446744073709551610\n".as_slice(), false),
        ("ordinary-tuple-field", include_str!("ordinary-tuple-field.zil"), b"18446744073709551611\n".as_slice(), false),
        ("ordinary-enum-field", include_str!("ordinary-enum-field.zil"), b"18446744073709551612\n".as_slice(), false),
        ("ordinary-error-field", include_str!("ordinary-error-field.zil"), b"18446744073709551613\n".as_slice(), false),
        ("ordinary-sum-capture", include_str!("ordinary-sum-capture.zil"), b"18446744073709551614\n".as_slice(), false),
        ("ordinary-direct-shape-captures", include_str!("ordinary-direct-shape-captures.zil"), b"18446744073709551614\n".as_slice(), false),
    ] {
        super::g933_consumers::native_entry_all_backends(label, source, expected, imported, Some("result"));
    }
}

#[test]
fn ordinary_store_live_parent_view_and_duplicate_responsibility_reject() {
    for (label, text, expected_code) in [
        ("live-parent-view", include_str!("ordinary-live-view.zil"), "E-BORROW-CONFLICT"),
        ("duplicate-parent", include_str!("ordinary-duplicate.zil"), "E-UNINITIALIZED"),
        ("same-store-different-index", include_str!("ordinary-parent-same-store-index.zil"), "E-ANALYZE-IR"),
        ("live-parent-move", include_str!("ordinary-parent-live-move.zil"), "E-BORROW-CONFLICT"),
    ] {
        let report = check(label, text, Stage::Verify);
        assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
        assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
        assert!(report.diagnostics.iter().any(|d| d.code == expected_code), "{}", report.to_json());
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn ordinary_store_descriptor_alias_cannot_become_a_second_owner() {
    let report = check("ordinary-give-alias", include_str!("ordinary-give-alias.zil"), Stage::Verify);
    assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(report.diagnostics.iter().any(|d| d.code == "E-GIVE-DESCRIPTOR"), "{}", report.to_json());
}

#[test]
fn ordinary_ownership_does_not_admit_a_nested_store_element() {
    let report = check("ordinary-not-store-element", include_str!("ordinary-not-store-element.zil"), Stage::Verify);
    assert_eq!(report.status, ResultCategory::Unsupported, "{}", report.to_json());
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED" && d.message.contains("Store elements require")), "{}", report.to_json());
}

#[test]
fn ordinary_store_same_size_canonical_element_rejects_raw_and_wire() {
    let (mut module, mut plan) = candidate(include_str!("ordinary-sums.zil"));
    let store = module.resources.iter_mut().find(|r| matches!(r.payload, Type::OwnedStore { .. })).expect("Source Store definition");
    let Type::OwnedStore { element } = &mut store.payload else { unreachable!() };
    let Type::Record { name, .. } = element.as_mut() else { panic!("Source POD element must retain nominal Record") };
    name.push_str("_same_size_forgery");
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let out = directory("ordinary-same-size-canonical");
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    for (route, result) in [
        ("raw", zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)),
        ("wire", zil_verify::verify_module_bytes(&wire, &module.target, &limits)),
    ] {
        let errors = result.expect_err(route);
        assert!(errors.iter().any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject), "{route}: {errors:?}");
        fs::write(out.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
    }
}

#[test]
fn ordinary_store_actual_parent_field_receiver_rejects_stale_raw_and_wire_provenance() {
    let (mut module, mut plan) = candidate(include_str!("ordinary-parent-two-views.zil"));
    let function = module.functions.iter_mut().find(|f| f.name == "result").unwrap();
    let function_id = function.id;
    let mut first = None;
    let mut changed = false;
    let mut changed_results = None;
    for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::DescribePackageField { local, index } = &mut instruction.kind {
            if let Some((original_local, original_index, original_result)) = first {
                if *local != original_local || *index == original_index { continue; }
                *index = original_index;
                changed = true;
                changed_results = Some((original_result, instruction.result.unwrap()));
                break;
            } else { first = Some((*local, *index, instruction.result.unwrap())); }
        }
    }
    assert!(changed, "Source must describe two distinct Store fields of the same parent");
    let (first_result, second_result) = changed_results.unwrap();
    let holds = &plan.functions.iter().find(|row| row.function == function_id).unwrap().holds;
    assert_eq!(holds[first_result.0 as usize].roots, holds[second_result.0 as usize].roots,
        "public HoldRoot deliberately folds sibling fields under their actual parent Local");
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let out = directory("ordinary-sibling-receiver-alias");
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    for (route, result) in [
        ("raw", zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)),
        ("wire", zil_verify::verify_module_bytes(&wire, &module.target, &limits)),
    ] {
        let errors = result.expect_err(route);
        assert!(errors.iter().any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject), "{route}: {errors:?}");
        fs::write(out.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
    }
}

#[test]
fn ordinary_store_pair_cannot_duplicate_one_parent_raw_or_wire() {
    let text = include_str!("ordinary-pair-baseline.zil");
    let (mut module, mut plan) = candidate(text);
    let mut mutated = false;
    for instruction in module.functions.iter_mut().flat_map(|f| &mut f.blocks).flat_map(|b| &mut b.instructions) {
        if let I::MakePackage { fields } = &mut instruction.kind {
            if fields.len() == 2 { fields[1] = fields[0]; mutated = true; }
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
        ("raw", zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)),
        ("wire", zil_verify::verify_module_bytes(&wire, &module.target, &limits)),
    ] {
        let errors = result.expect_err(route);
        assert!(errors.iter().any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject), "{route}: {errors:?}");
        fs::write(out.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
    }
}
