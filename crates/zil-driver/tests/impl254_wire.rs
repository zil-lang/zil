//! Rebuildable Wire31 loan-revival mutation, preserving the original Wire30 bytes.
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
        include_str!("../../../tests/impl254/loan_control.zil"),
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
            if let InstructionKind::TakeLocal { local } = &mut instruction.kind {
                if *local == sibling {
                    *local = owner;
                    changed += 1;
                }
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
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/impl254/loan_revival_v31.wire"
    );
    if std::env::var_os("ZIL_GENERATE_IMPL254_WIRE").is_some() {
        std::fs::write(path, &bytes).unwrap();
    }
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}
