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
                    text.contains("retir") || text.contains("conflict") || text.contains("overlap")
                }),
                "{attack}/{entry}: expected actual resource/borrow rejection: {errors:?}"
            );
        }
    }
}

const DELEGATION: &str = include_str!("growing-hash-collections/store-parent-delegation.zil");
const CALLBACK_REPLACEMENT: &str =
    include_str!("growing-hash-collections/callback-helper-replacement.zil");

#[test]
fn actual_store_parent_delegation_and_callback_through_helper_reach_verify() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for text in [DELEGATION, CALLBACK_REPLACEMENT] {
        let raw = raw_source(text);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
    for (label, text) in [
        ("same actual field", DELEGATION.replace("store_at_mut(pair.right,0)", "store_at_mut(pair.left,0)")),
        ("parent reborrow while child is live", DELEGATION.replace(
            "let right=store_at_mut(pair.right,0)\n    *right=22", "let again=&mut *pair\n    let right=store_at_mut(again.right,0)\n    *right=22")),
        ("same owner new parent generation", DELEGATION.replace(
            "return i32(inspect(&mut pair))-11", "var pointer=&mut pair\n    let child=store_at_mut(pointer.left,0)\n    pointer=&mut pair\n    return i32(*child)-11")),
        ("callback replacement retires caller descriptor", CALLBACK_REPLACEMENT.replace(
            "return i32(*store_at(holder.items,0))-22", "return i32(*store_at(old,0))-11")),
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
    let mut raw = raw_source(DELEGATION);
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let helper = raw
        .functions
        .iter_mut()
        .find(|f| f.locals.iter().any(|l| l.name == "value"))
        .unwrap();
    let mut edits = 0;
    for instruction in helper.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::AddressField { index, .. } = &mut instruction.kind
            && *index == 1
        {
            *index = 0;
            edits += 1;
        }
    }
    assert_eq!(edits, 1);
    // Exported Hold roots are deliberately coarse (the same Parameter(0));
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
