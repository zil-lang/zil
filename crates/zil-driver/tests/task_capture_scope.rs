use zil_base::{FileId, ModuleId, SourceUnit};
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/task-owner-transport/returned-task-fnonce.zil");

#[test]
fn task_capture_fixed_scope_comes_from_actual_input_classes() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "returned-task-fnonce.zil", SOURCE);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &zil_base::Limits::default(),
    )
    .unwrap();
    let closure = &typed.closures[0];
    let relation = closure
        .storage_life
        .as_ref()
        .expect("a captured Task's actual Fixed domain must remain explicit");
    assert_eq!(relation.fixed_binder_count, 1);
    assert_eq!(relation.invocation_binder_count, 0);
    assert_eq!(
        relation.bindings,
        vec![zil_ir::StorageLifeBinding {
            position: zil_ir::StoragePosition {
                base: zil_ir::StorageBase::Capture(0),
                path: vec![zil_ir::ContractProjection::RetainedPort(0)]
            },
            binder: zil_ir::StorageBinder::Fixed(0),
        }]
    );
    assert_eq!(
        relation.classes,
        vec![zil_ir::SignatureLifeClassBinding {
            position: zil_ir::SignatureLifeClass {
                base: zil_ir::StorageBase::Capture(0),
                path: vec![zil_ir::LifeTypeProjection::Pointee],
                class: 0
            },
            binder: zil_ir::StorageBinder::Fixed(0),
        }]
    );
    assert_eq!(
        typed.functions[closure.function.0 as usize]
            .storage_life
            .as_ref(),
        Some(relation)
    );
    let creator = typed
        .functions
        .iter()
        .find(|f| {
            f.callable_return_contract
                .as_ref()
                .is_some_and(|c| c.closure == closure.id)
        })
        .unwrap();
    assert!(
        creator
            .storage_life
            .as_ref()
            .unwrap()
            .classes
            .iter()
            .any(|row| row.position.base == zil_ir::StorageBase::Result
                && row.position.class == 0
                && row.binder == zil_ir::StorageBinder::Fixed(0))
    );
}
