//! Current Source-derived loan-revival mutation. Historical Wire bytes remain
//! immutable version-rejection controls, never semantic producer witnesses.

use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::InstructionKind;

#[test]
fn control_and_minimal_transfer_mutation() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "loan_control.zil",
        include_str!("../../../tests/borrowed-callable-arguments/loan_control.zil"),
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let aliases = std::collections::BTreeMap::new();
    let expanded = zil_expand::expand_program_with_semantics(
        std::slice::from_ref(&source),
        &[&ast],
        &[Vec::new()],
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &aliases,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap();
    assert_eq!(expanded.len(), 1);
    let mut typed = zil_types::check(&expanded[0].0.core, &target, &limits).unwrap();
    let mut candidate = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::encode_module_candidate(&typed, &candidate, &limits).unwrap(),
        include_bytes!("../../../tests/borrowed-callable-arguments/loan_control.candidate.wire")
            .as_slice(),
        "current fixture must be the complete original Source producer output"
    );
    let original = typed.clone();
    let main = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let owner = main.locals.iter().find(|l| l.name == "e").unwrap().id;
    let sibling = main.locals.iter().find(|l| l.name == "other").unwrap().id;
    assert_ne!(owner, sibling);
    assert_eq!(
        main.locals[owner.0 as usize].ty,
        main.locals[sibling.0 as usize].ty
    );
    let mut changed = 0;
    for block in &mut main.blocks {
        for instruction in &mut block.instructions {
            if let InstructionKind::TakeLocal { local } = &mut instruction.kind
                && *local == sibling
            {
                *local = owner;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1, "single actual argument source mutation");
    assert_ne!(typed, original);
    candidate.module_fingerprint = typed.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &candidate, &limits).unwrap();
    let raw = zil_verify::verify_module(typed, candidate, &target, &limits).unwrap_err();
    let wire = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
    for errors in [&raw, &wire] {
        assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
    }
    let frozen = include_bytes!("../../../tests/borrowed-callable-arguments/v40/loan_revival.wire");
    assert_eq!(
        zil_ir::wire::decode_module_candidate(frozen, &limits).unwrap_err()[0].code,
        "E-WIRE-VERSION"
    );
    let current =
        include_bytes!("../../../tests/borrowed-callable-arguments/loan_revival.candidate.wire");
    let errors = zil_verify::verify_module_bytes(current, &target, &limits).unwrap_err();
    assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
    // The current Source output is mutated at precisely the original operand;
    // all original definitions, sites, source names and plan claims remain.
    assert_eq!(current.as_slice(), bytes);
}
