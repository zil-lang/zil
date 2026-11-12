//! Real Source candidate, followed by metadata-only raw/Wire near attacks.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{ContractProjection, StorageBase, StorageBinder};

#[test]
fn actual_zero_data_task_head_preserves_complete_relation_and_rejects_relabeling() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "ordinary-task-head.zil",
        include_str!("../../../tests/language-readiness/task-aggregate-results/enum-result.zil"),
    );
    let limits = Limits::default();
    let target = TargetProfile::default();
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let driver = module
        .functions
        .iter()
        .position(|f| f.name == "drive")
        .unwrap();
    let header = module.functions[driver].storage_life.as_ref().unwrap();
    assert_eq!(header.fixed_binder_count, 0);
    assert_eq!(header.invocation_binder_count, 1);
    assert!(header.classes.is_empty());
    assert_eq!(header.bindings.len(), 1);
    assert_eq!(header.bindings[0].position.base, StorageBase::Parameter(0));
    assert!(header.bindings[0].position.path.is_empty());
    for wire in [false, true] {
        let (module, plan) = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
        } else {
            (module.clone(), plan.clone())
        };
        zil_verify::verify_module(module, plan, &target, &limits).unwrap();
    }
    for attack in 0..8 {
        let mut wrong = module.clone();
        let c = wrong.functions[driver].storage_life.as_mut().unwrap();
        match attack {
            0 => c.bindings[0].position.base = StorageBase::Parameter(1),
            1 => c.bindings[0].position.base = StorageBase::Result,
            2 => c.bindings[0].position.base = StorageBase::Capture(0),
            3 => c.bindings[0]
                .position
                .path
                .push(ContractProjection::RetainedPort(0)),
            4 => c.bindings[0].binder = StorageBinder::Fixed(0),
            5 => c.bindings[0].binder = StorageBinder::Invocation(1),
            6 => c.bindings.push(c.bindings[0].clone()),
            _ => c.bindings.clear(),
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = wrong.fingerprint();
        for wire in [false, true] {
            let (wrong, candidate) = if wire {
                let bytes =
                    zil_ir::wire::encode_module_candidate(&wrong, &candidate, &limits).unwrap();
                zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
            } else {
                (wrong.clone(), candidate.clone())
            };
            let failure =
                zil_verify::verify_module(wrong, candidate, &target, &limits).unwrap_err();
            assert!(
                failure.iter().any(|d| d.code == "E-VERIFY-SIGNATURE-CLASS"),
                "attack={attack}, Wire={wire}: {failure:?}"
            );
        }
    }
}
