//! Full ordinary-input and callback-capture context, reconstructed by both cores.
use super::*;
use zil_base::{Limits, TargetProfile};
use zil_ir::InstructionKind;

const CONTEXT: &str = include_str!("growing-hash-collections/callable-store-context.zil");

fn raw_source(program: &str) -> zil_ir::TypedModule {
    let input = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "callable-store-context.zil",
        program,
    );
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits(&input, &ast, &Limits::default())
        .unwrap()
        .core;
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

#[test]
fn ordinary_input_and_public_capture_effects_keep_the_same_complete_call_context() {
    let raw = raw_source(CONTEXT);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for (label, program) in [
        (
            "ordinary input retires caller descriptor",
            CONTEXT.replace(
                "return i32(*store_at(holder.items,0))-22",
                "return i32(*store_at(old,0))-11",
            ),
        ),
        (
            "callback descriptor aliases ordinary input",
            CONTEXT.replace("let observed=other.marker", "let observed=holder.marker"),
        ),
    ] {
        let dir = directory(&format!("callable-context-{label}"));
        fs::write(dir.join("main.zil"), &program).unwrap();
        let report = CompilationSession::default().compile_path(
            &dir.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        println!("{label}: {}", report.to_json());
    }
}

#[test]
fn raw_and_wire_reject_stale_descriptor_and_actual_callback_alias_with_valid_ssa() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for attack in ["stale descriptor", "callback alias"] {
        let mut raw = raw_source(CONTEXT);
        let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let main = raw
            .functions
            .iter_mut()
            .find(|f| f.locals.iter().any(|l| l.name == "observed"))
            .unwrap();
        let named = |name: &str| main.locals.iter().find(|l| l.name == name).unwrap().id;
        let old = named("old");
        let holder = named("holder");
        let other = named("other");
        let descriptor = main
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::Store { local, value } if local == old => Some(value),
                _ => None,
            })
            .unwrap();
        let mut edits = 0;
        let mut changed_roots = std::collections::BTreeSet::new();
        for block in &mut main.blocks {
            for instruction in &mut block.instructions {
                if attack == "stale descriptor" {
                    if let InstructionKind::OwnedStoreBorrow { store, .. } = &mut instruction.kind {
                        assert_eq!(
                            main.values[store.0 as usize],
                            main.values[descriptor.0 as usize]
                        );
                        *store = descriptor;
                        edits += 1;
                    }
                } else if let InstructionKind::BorrowLocalMut { local } = &mut instruction.kind
                    && *local == holder
                {
                    assert_eq!(
                        main.locals[holder.0 as usize].ty,
                        main.locals[other.0 as usize].ty
                    );
                    *local = other;
                    // Origin candidates must describe the attacked program:
                    // force Verify past candidate-equality into actual loans.
                    let result = instruction.result.unwrap();
                    changed_roots.insert(result);
                    let hold = &mut plan.functions[main.id.0 as usize].holds[result.0 as usize];
                    assert_eq!(hold.roots, vec![zil_ir::HoldRoot::Local(holder)]);
                    hold.roots = vec![zil_ir::HoldRoot::Local(other)];
                    edits += 1;
                }
            }
        }
        assert_eq!(
            edits, 1,
            "{attack}: change exactly one actual root/use and keep SSA definitions"
        );
        // Track actual address producers, including source argument adaptation.
        // Unrelated descriptors and same-typed owners keep their actual roots.
        loop {
            let mut grew = false;
            for instruction in main.blocks.iter().flat_map(|b| &b.instructions) {
                let parent = match instruction.kind {
                    InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. }
                    | InstructionKind::DescribeAddress { address } => Some(address),
                    _ => None,
                };
                if let (Some(parent), Some(result)) = (parent, instruction.result)
                    && changed_roots.contains(&parent)
                    && changed_roots.insert(result)
                {
                    let hold = &mut plan.functions[main.id.0 as usize].holds[result.0 as usize];
                    for root in &mut hold.roots {
                        if *root == zil_ir::HoldRoot::Local(holder) {
                            *root = zil_ir::HoldRoot::Local(other);
                        }
                    }
                    hold.roots.sort_unstable();
                    hold.roots.dedup();
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        plan.module_fingerprint = raw.fingerprint();
        let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
        let verified =
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        for (entry, errors) in [
            ("Analyze", analyzed),
            ("Verify", verified),
            ("Wire", decoded),
        ] {
            println!("{attack}/{entry}: {errors:?}");
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::SafeReject),
                "{attack}/{entry}: {errors:?}"
            );
            assert!(
                !errors.iter().any(|d| {
                    let text = d.message.to_lowercase();
                    ["fingerprint", "ssa", "dominat", "out of bounds"]
                        .iter()
                        .any(|word| text.contains(word))
                }),
                "{attack}/{entry}: structural rejection does not prove resource safety: {errors:?}"
            );
            assert!(
                errors.iter().any(|d| {
                    let text = d.message.to_lowercase();
                    d.code == "E-BORROW-CONFLICT"
                        || text.contains("retir")
                        || text.contains("conflict")
                        || text.contains("overlap")
                }),
                "{attack}/{entry}: expected actual resource/borrow rejection: {errors:?}"
            );
        }
    }
}

const DELEGATION: &str = include_str!("growing-hash-collections/store-parent-delegation.zil");
const OWNED_FIELDS: &str = include_str!("growing-hash-collections/store-owned-field-windows.zil");
const BORROWED_SIBLINGS: &str =
    include_str!("growing-hash-collections/borrowed-store-sibling-window.zil");
const CALLBACK_REPLACEMENT: &str =
    include_str!("growing-hash-collections/callback-helper-replacement.zil");

#[test]
fn actual_store_parent_delegation_and_callback_through_helper_reach_verify() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for text in [DELEGATION, OWNED_FIELDS, CALLBACK_REPLACEMENT] {
        let raw = raw_source(text);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
    for (label, text) in [
        ("same actual field", OWNED_FIELDS.replace("store_at_mut(pair.right,0)", "store_at_mut(pair.left,0)")),
        ("parent reborrow while child is live", DELEGATION.replace(
            "let value=*left", "let again=&mut *pair\n    let value=*left+store_len(again.left)-1")),
        ("same owner new parent generation", DELEGATION.replace(
            "return i32(inspect(&mut pair))-11", "var pointer=&mut pair\n    let child=store_at_mut(pointer.left,0)\n    pointer=&mut pair\n    return i32(*child)-11")),
        ("callback replacement retires caller descriptor", CALLBACK_REPLACEMENT.replace(
            "return i32(current.n)-22", "return i32(old.n)-11")),
    ] {
        // All attacks must first form fully typed Source. A syntax/profile
        // rejection is not evidence for the requested loan/retirement property.
        let raw = raw_source(&text);
        let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
        println!("{label}: {errors:?}");
        assert!(errors.iter().all(|d| d.category == ResultCategory::SafeReject), "{label}: {errors:?}");
        assert!(errors.iter().any(|d| d.code == "E-BORROW-CONFLICT" || d.message.contains("retir")
            || d.message.contains("live element")), "{label}: {errors:?}");
    }
}

#[test]
fn raw_and_wire_distinguish_same_typed_store_fields_in_the_parent_loan_chain() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = raw_source(OWNED_FIELDS);
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let helper = raw
        .functions
        .iter_mut()
        .find(|f| f.locals.iter().any(|l| l.name == "previous"))
        .unwrap();
    let mut edits = 0;
    for instruction in helper.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::DescribePackageField { index, .. } = &mut instruction.kind
            && *index == 1
        {
            *index = 0;
            edits += 1;
        }
    }
    assert_eq!(edits, 1);
    // Exported Hold roots are deliberately coarse (the same local Pair);
    // the verifier must reconstruct the changed actual field and loan itself.
    plan.module_fingerprint = raw.fingerprint();
    let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
    let verified =
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap_err();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    for (entry, errors) in [
        ("Analyze", analyzed),
        ("Verify", verified),
        ("Wire", decoded),
    ] {
        println!("actual Store field alias/{entry}: {errors:?}");
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::SafeReject),
            "{entry}: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|d| ["fingerprint", "SSA", "dominat", "candidate differs"]
                    .iter()
                    .any(|part| d.message.contains(part))),
            "{entry}: {errors:?}"
        );
        assert!(
            errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"
                || d.message.contains("loan")
                || d.message.contains("window")
                || d.message.contains("exclusive")),
            "{entry}: {errors:?}"
        );
    }
}

#[test]
fn borrowed_sibling_store_windows_keep_the_existing_explicit_precision_boundary() {
    let raw = raw_source(BORROWED_SIBLINGS);
    let errors = zil_analyze::analyze(&raw, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .all(|d| d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|d| d
            .message
            .contains("actual borrowed resource access conflicts")),
        "{errors:?}"
    );
    println!("borrowed sibling Store fields remain outside this increment: {errors:?}");
}

const GENERATION: &str = include_str!("growing-hash-collections/store-descriptor-generation.zil");

#[test]
fn a_saved_store_descriptor_cannot_gain_a_rebound_pointer_generations_authority() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let raw = raw_source(GENERATION);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let attacked = GENERATION.replace(
        "    let child=store_at(old,0)\n    let value=*child\n    pointer=&mut holder",
        "    pointer=&mut holder\n    let child=store_at(old,0)\n    let value=*child",
    );
    assert_ne!(attacked, GENERATION);
    let rejected = raw_source(&attacked);
    let errors = zil_analyze::analyze(&rejected, &limits).unwrap_err();
    println!("Desc-first rebound-pointer Source: {errors:?}");
    assert!(
        errors
            .iter()
            .all(|d| d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|d| d
            .message
            .contains("actual borrowed resource access conflicts")
            || d.code == "E-BORROW-CONFLICT"),
        "{errors:?}"
    );
    // Frozen e3 already rejects this complete Source in the outer head-loan
    // proof. The additional defensive reaching-definition fence is tested
    // independently below the public entry, without weakening that guard.

    let mut changed = raw;
    let main = changed
        .functions
        .iter_mut()
        .find(|f| f.locals.iter().any(|l| l.name == "pointer"))
        .unwrap();
    let pointer = main.locals.iter().find(|l| l.name == "pointer").unwrap().id;
    let child_local = main.locals.iter().find(|l| l.name == "child").unwrap().id;
    let mut edits = 0;
    for block in &mut main.blocks {
        let Some(begin) = block.instructions.iter().position(|i| {
            matches!(i.kind,
            InstructionKind::StorageLive(local) if local == child_local)
        }) else {
            continue;
        };
        let rebound = block
            .instructions
            .iter()
            .enumerate()
            .skip(begin)
            .find_map(|(index, i)| {
                matches!(i.kind, InstructionKind::BorrowLocalMut { .. }).then_some(index)
            })
            .unwrap();
        let finish = block
            .instructions
            .iter()
            .enumerate()
            .skip(rebound)
            .find_map(|(index, i)| {
                matches!(i.kind,InstructionKind::Store { local, .. } if local == pointer)
                    .then_some(index + 1)
            })
            .unwrap();
        // Move the second complete parent definition/store before child creation.
        // All SSA definitions and their dominated uses move together unchanged.
        block.instructions[begin..finish].rotate_right(finish - rebound);
        edits += 1;
    }
    assert_eq!(edits, 1);
    let mut candidate = plan;
    candidate.module_fingerprint = changed.fingerprint();
    let analyzed = zil_analyze::analyze(&changed, &limits).unwrap_err();
    let verified = zil_verify::verify_module(changed.clone(), candidate.clone(), &target, &limits)
        .unwrap_err();
    let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    for (entry, errors) in [
        ("Analyze", analyzed),
        ("Verify", verified),
        ("Wire", decoded),
    ] {
        println!("Desc-first rebound-pointer/{entry}: {errors:?}");
        assert!(
            errors
                .iter()
                .all(|d| d.category == ResultCategory::SafeReject),
            "{entry}: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|d| ["fingerprint", "SSA", "dominat", "candidate differs"]
                    .iter()
                    .any(|part| d.message.contains(part))),
            "{entry}: {errors:?}"
        );
        assert!(
            errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"
                || d.message.contains("loan")
                || d.message.contains("window")
                || d.message.contains("exclusive")),
            "{entry}: {errors:?}"
        );
    }
}

#[test]
fn callback_borrow_of_a_store_package_keeps_its_existing_type_boundary() {
    let text = include_str!("growing-hash-collections/callback-store-address-boundary.zil");
    let dir = directory("callback-store-address-boundary");
    fs::write(dir.join("main.zil"), text).unwrap();
    let report = CompilationSession::default().compile_path(
        &dir.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Types,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("ordinary Package Product requires")),
        "{}",
        report.to_json()
    );
}
