use zil_base::{FileId, ModuleId, SourceUnit};
const SOURCE: &str =
    include_str!("../../../tests/language-readiness/task-owner-transport/returned-task-fnonce.zil");

fn expand_original(source: &SourceUnit) -> zil_base::CompileResult<zil_ir::CoreModule> {
    let ast = zil_syntax::parse(source).into_result()?;
    zil_expand::expand_with_map_limits_target_context(
        source,
        &ast,
        &zil_base::Limits::default(),
        &zil_base::TargetProfile::default(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .map(|expanded| expanded.core)
}

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

#[test]
fn two_equal_task_types_keep_distinct_actual_capture_classes() {
    let text=SOURCE.replace("fn keep<life a>(give original:Task<i32,&'a mut i32>)", "fn keep<life a,life b>(give original:Task<i32,&'a mut i32>,give second:Task<i32,&'b mut i32>)")
        .replace("return move()=>drive(give original);", "return move()=>drive(give original)+drive(give second);")
        .replace("var number=37;", "var number=37;var other=41;")
        .replace("let callback=keep(give original);", "let otherTask=task_create(worker,&mut other)?;let callback=keep(give original,give otherTask);")
        .replace("return answer-42;", "if other!=46{return 92;}return answer-88;");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "two-live-task-captures.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &zil_base::Limits::default(),
    )
    .unwrap();
    let closure = &typed.closures[0];
    assert_eq!(
        closure.captures[0], closure.captures[1],
        "equal complete machine types do not prove a shared lifetime source"
    );
    let relation = closure.storage_life.as_ref().unwrap();
    assert_eq!(relation.fixed_binder_count, 2);
    assert_eq!(relation.bindings.len(), 2);
    for (index, binding) in relation.bindings.iter().enumerate() {
        assert_eq!(
            binding.position,
            zil_ir::StoragePosition {
                base: zil_ir::StorageBase::Capture(index as u32),
                path: vec![zil_ir::ContractProjection::RetainedPort(0)]
            }
        );
        assert_eq!(binding.binder, zil_ir::StorageBinder::Fixed(index as u32));
    }
    let creator = typed
        .functions
        .iter()
        .find(|f| {
            f.callable_return_contract
                .as_ref()
                .is_some_and(|c| c.closure == closure.id)
        })
        .unwrap();
    let contract = creator.callable_return_contract.as_ref().unwrap();
    assert_eq!(contract.writes, vec![0, 1]);
    let sources = contract.capture_sources.as_ref().unwrap();
    assert_eq!(sources.len(), 2);
    for (index, row) in sources.iter().enumerate() {
        assert_eq!(
            row.position.base,
            zil_ir::StorageBase::Capture(index as u32)
        );
        assert_eq!(
            row.sources,
            vec![zil_ir::StorageSource::Exact(zil_ir::StoragePosition {
                base: zil_ir::StorageBase::Parameter(index as u32),
                path: vec![zil_ir::ContractProjection::RetainedPort(0)]
            })]
        );
    }
}

#[test]
fn returned_task_class_zero_maps_to_the_actual_second_capture_domain() {
    use zil_ir::{ContractProjection, LifeTypeProjection, StorageBase, StorageBinder};
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/returned-second-task-fnonce.zil"
    );
    let input = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "returned-second-task-fnonce.zil",
        text,
    );
    let core = expand_original(&input).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &zil_base::Limits::default(),
    )
    .unwrap();
    let closure = &typed.closures[0];
    assert_eq!(closure.captures[0], closure.captures[1]);
    let scope = closure.storage_life.as_ref().unwrap();
    assert_eq!(scope.fixed_binder_count, 2);
    let port = scope
        .bindings
        .iter()
        .find(|row| {
            row.position.base == StorageBase::Result
                && row.position.path == [ContractProjection::RetainedPort(0)]
        })
        .unwrap();
    assert_eq!(port.binder, StorageBinder::Fixed(1));
    let class = scope
        .classes
        .iter()
        .find(|row| {
            row.position.base == StorageBase::Result
                && row.position.path == [LifeTypeProjection::Pointee]
                && row.position.class == 0
        })
        .unwrap();
    assert_eq!(class.binder, StorageBinder::Fixed(1));
    let entry = &typed.functions[closure.function.0 as usize];
    assert_eq!(entry.storage_life.as_ref(), Some(scope));
    let bad = text.replace(
        "drive(give original);return give second;",
        "drive(give second);return give original;",
    );
    let input = SourceUnit::new(FileId(0), ModuleId(0), "wrong-task-return-scope.zil", bad);
    assert!(expand_original(&input).is_err());
}
