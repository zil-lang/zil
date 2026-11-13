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

// This only exercises Verify's independent lexical/SSA sub-proof. The real
// producer still reports its own proof gap, and the empty plan is not Analyze.
const COMPLETE_IDENTITY:&[u8]=include_bytes!("fixtures/ordinary-identity-complete-call-sites.wire");
#[test]
fn source_identity_actual_generic_call_reaches_the_complete_semantic_subproof() {
    let limits=Limits::default();
    let (module,empty_plan)=zil_ir::wire::decode_module_candidate(COMPLETE_IDENTITY,&limits).unwrap();
    check_program(&module,&mut Budget::new(&limits)).unwrap();
    let bytes=zil_ir::wire::encode_module_candidate(&module,&empty_plan,&limits).unwrap();
    let (decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
    check_program(&decoded,&mut Budget::new(&limits)).unwrap();
}
#[test]
fn source_identity_keeps_the_original_full_actual_type_and_site_on_both_routes() {
    let limits=Limits::default();
    let (original,empty_plan)=zil_ir::wire::decode_module_candidate(COMPLETE_IDENTITY,&limits).unwrap();
    for attack in 0..4 {
        let mut module=original.clone();
        let program=module.semantic.as_mut().unwrap();
        match attack {
            0=>{
                let identity=program.instances.iter_mut().find(|instance|!instance.actuals.is_empty()).unwrap();
                identity.actuals[0]=SemanticActual::Type(SemanticTypeValue::Resolved{ty:Type::U32,life:empty_life(),classes:vec![]});
            }
            1=>{
                let SemanticSiteAction::Call{actuals,..}=&mut program.sites[0].action else{panic!("real source Call")};
                actuals[0]=SemanticActual::Type(SemanticTypeValue::Resolved{ty:Type::U32,life:empty_life(),classes:vec![]});
            }
            2=>program.sites.clear(),
            3=>{
                let identity=program.instances.iter_mut().find(|instance|!instance.actuals.is_empty()).unwrap();
                identity.actuals[0]=SemanticActual::CoreRecipe{scope:identity.id,recipe:Box::new(SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(Type::I32)))};
            }
            _=>unreachable!(),
        }
        let error=check_program(&module,&mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error.code,"E-VERIFY-SEMANTIC","attack {attack}: {error:?}");
        let bytes=zil_ir::wire::encode_module_candidate(&module,&empty_plan,&limits).unwrap();
        let (decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        let error=check_program(&decoded,&mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error.code,"E-VERIFY-SEMANTIC","wire attack {attack}: {error:?}");
    }
}
