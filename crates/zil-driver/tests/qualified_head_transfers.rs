use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{StorageBase, StorageBinder};

fn core(text: &str) -> zil_ir::CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "receiver-borrow.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    core
}
fn typed(text: &str) -> zil_ir::TypedModule {
    zil_types::check(&core(text), &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

#[test]
fn consuming_qualified_holder_preserves_the_complete_pointee_class_and_real_slots() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/fnonce-qualified-holder-head-transfer.zil"
    );
    let module = typed(text);
    let limits = Limits::default();
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    assert_eq!(zil_analyze::analyze(&decoded, &limits).unwrap(), candidate);
    let definition = &module.closures[0];
    let life = definition.storage_life.as_ref().unwrap();
    let outer = life
        .bindings
        .iter()
        .find(|row| row.position.base == StorageBase::Result)
        .unwrap()
        .binder;
    let inner = life
        .classes
        .iter()
        .find(|row| row.position.base == StorageBase::Result)
        .unwrap()
        .binder;
    assert_ne!(
        outer, inner,
        "the head and nominal pointee preserve distinct original scopes"
    );
    for attack in 0..3 {
        let mut bad = module.clone();
        if attack == 2 {
            for instruction in bad.functions[definition.function.0 as usize]
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.instructions)
            {
                if let zil_ir::InstructionKind::TakeCapture { index } = instruction.kind {
                    instruction.kind = zil_ir::InstructionKind::ReadCapture { index };
                }
            }
        } else {
            let life = bad.closures[0].storage_life.as_mut().unwrap();
            if attack == 0 {
                life.classes
                    .iter_mut()
                    .find(|row| row.position.base == StorageBase::Result)
                    .unwrap()
                    .binder = outer;
            } else {
                life.classes
                    .retain(|row| row.position.base != StorageBase::Result);
            }
            bad.functions[definition.function.0 as usize].storage_life = Some(life.clone());
            // Change the public expectation too: rejection must still follow
            // the real captured pointee class, not just two unequal headers.
            for function in &mut bad.functions {
                if let Some(contract) = &mut function.callable_return_contract
                    && contract.closure == definition.id
                    && let Some(expected) = &mut contract.life
                {
                    if attack == 0 {
                        for row in &mut expected.classes {
                            if row.position.base == StorageBase::Result {
                                row.binder = outer;
                            }
                        }
                    } else {
                        expected
                            .classes
                            .retain(|row| row.position.base != StorageBase::Result);
                    }
                }
            }
        }
        assert!(
            zil_analyze::analyze(&bad, &limits).is_err(),
            "qualified capability attack {attack}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&bad, &candidate, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
        assert!(
            zil_analyze::analyze(&decoded, &limits).is_err(),
            "Wire qualified capability attack {attack}"
        );
    }
    let bad = text.replace("let old=*data\n value=21", "value=21\n let old=*data");
    let errors = zil_analyze::analyze(&typed(&bad), &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.category == zil_base::ResultCategory::SafeReject),
        "old data SSA must retain its original root until last use: {errors:?}"
    );
}
