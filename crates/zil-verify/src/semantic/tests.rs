//! Frozen genuine Source→Core→Typed probe outputs. These are incomplete,
//! untrusted producer proposals, not successful Types/Analyze receipts.
use super::*;
const IDENTITY:&[u8]=include_bytes!("fixtures/ordinary-identity-missing-call-sites.wire");
const LOCAL:&[u8]=include_bytes!("fixtures/local-closure-missing-child-instance.wire");
#[test]
fn actual_source_identity_cannot_invent_its_missing_original_call_site() {
    let limits=Limits::default();
    let (module,_empty_plan)=zil_ir::wire::decode_module_candidate(IDENTITY,&limits).unwrap();
    assert!(module.semantic.is_some());
    let error=check_program(&module,&mut Budget::new(&limits)).unwrap_err();
    assert_eq!(error.code,"E-VERIFY-SEMANTIC");
    assert!(error.message.contains("actual Call has no original semantic specialization site"),"{error:?}");
}
#[test]
fn actual_default_borrowing_closure_requires_its_real_child_instance() {
    let limits=Limits::default();
    let (module,_empty_plan)=zil_ir::wire::decode_module_candidate(LOCAL,&limits).unwrap();
    assert_eq!(module.closures[0].storage_life.as_ref().unwrap().fixed_binder_count,1);
    assert!(matches!(module.closures[0].captures.as_slice(),[Type::Address{mutable:false,..}]));
    let error=check_program(&module,&mut Budget::new(&limits)).unwrap_err();
    assert_eq!(error.code,"E-VERIFY-SEMANTIC");
    assert!(error.message.contains("actual Closure entry has no complete semantic child instance"),"{error:?}");
}
