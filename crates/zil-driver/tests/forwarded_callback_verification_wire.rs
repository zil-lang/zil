//! Rebuildable Wire34 loan-revival mutation, preserving the historical Wire30/31/32 bytes.
#[path = "../../zil-ir/test-support/schema40_fixture.rs"]
mod schema40_fixture;

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
    let core = zil_expand::expand(&source, &ast).unwrap();
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    let mut candidate = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits).unwrap();
    let main = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let owner = main.locals.iter().find(|l| l.name == "e").unwrap().id;
    let sibling = main.locals.iter().find(|l| l.name == "other").unwrap().id;
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
    candidate.module_fingerprint = typed.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &candidate, &limits).unwrap();
    let raw = zil_verify::verify_module(typed, candidate, &target, &limits).unwrap_err();
    let wire = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
    for errors in [&raw, &wire] {
        assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
    }
    let frozen = include_bytes!("../../../tests/borrowed-callable-arguments/v40/loan_revival.wire");
    let migrated = schema40_fixture::migrate(frozen, &limits);
    let errors = zil_verify::verify_module_bytes(&migrated, &target, &limits).unwrap_err();
    assert_eq!(errors[0].code, "E-BORROW-CONFLICT", "{errors:?}");
    // Only the test transport migration changes; historical bytes stay immutable.
    assert_eq!(migrated, bytes);
}
