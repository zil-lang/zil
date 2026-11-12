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
fn exclusive_capture_shared_view_uses_the_actual_receiver_head() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/captured-exclusive-short-view.zil"
    );
    let module = typed(text);
    let definition = &module.closures[0];
    let life = definition.storage_life.as_ref().unwrap();
    let receiver = life
        .bindings
        .iter()
        .find(|r| r.position.base == StorageBase::Receiver)
        .unwrap();
    assert!(receiver.position.path.is_empty());
    assert!(matches!(receiver.binder, StorageBinder::Invocation(_)));
    assert_eq!(
        life.bindings
            .iter()
            .find(|r| r.position.base == StorageBase::Result)
            .unwrap()
            .binder,
        receiver.binder
    );
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for change in 0..2 {
        let mut bad = module.clone();
        let life = bad.closures[0].storage_life.as_mut().unwrap();
        if change == 0 {
            life.bindings
                .iter_mut()
                .find(|r| r.position.base == StorageBase::Result)
                .unwrap()
                .binder = StorageBinder::Fixed(0);
        } else {
            life.bindings
                .retain(|r| r.position.base != StorageBase::Receiver);
        }
        bad.functions[definition.function.0 as usize].storage_life = Some(life.clone());
        assert!(
            zil_analyze::analyze(&bad, &Limits::default()).is_err(),
            "a short returned head cannot become Fixed or lose its receiver input"
        );
    }
    let bad = text.replace(
        "let second=*view",
        "let another=f();let second=*view+*another",
    );
    assert!(
        zil_analyze::analyze(&typed(&bad), &Limits::default()).is_err(),
        "a live receiver view prevents another exclusive call"
    );
}

#[test]
fn consuming_exclusive_capture_uses_a_real_take() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/fnonce-exclusive-capture-take.zil"
    );
    let module = typed(text);
    let definition = &module.closures[0];
    let entry = &module.functions[definition.function.0 as usize];
    assert!(
        entry
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, zil_ir::InstructionKind::TakeCapture { .. }))
    );
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut bad = module.clone();
    for instruction in bad.functions[definition.function.0 as usize]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if let zil_ir::InstructionKind::TakeCapture { index } = instruction.kind {
            instruction.kind = zil_ir::InstructionKind::ReadCapture { index };
        }
    }
    assert!(
        zil_analyze::analyze(&bad, &Limits::default()).is_err(),
        "ReadCapture cannot transfer the original Fixed exclusive capability"
    );
}

#[test]
fn consuming_capture_infers_the_actual_fixed_result_without_an_explicit_promise() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/fnonce-exclusive-capture-take-inferred-result.zil"
    );
    let module = typed(text);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
}

#[test]
fn shared_capture_copies_a_fixed_data_pointer_without_retaining_the_temporary_head() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/shared-capture-fixed-result.zil"
    );
    let module = typed(text);
    assert!(
        !module.closures[0]
            .storage_life
            .as_ref()
            .unwrap()
            .bindings
            .iter()
            .any(|r| r.position.base == StorageBase::Receiver)
    );
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
}

#[test]
fn a_stored_shared_field_keeps_its_fixed_class_without_the_borrowed_holder_head() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/captured-holder-fixed-data-result.zil"
    );
    let module = typed(text);
    assert!(
        !module.closures[0]
            .storage_life
            .as_ref()
            .unwrap()
            .bindings
            .iter()
            .any(|r| r.position.base == StorageBase::Receiver)
    );
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
}

#[test]
fn generic_mutable_helpers_transport_each_actual_returned_receiver_head() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/captured-exclusive-short-view-helper.zil"
    );
    zil_analyze::analyze(&typed(text), &Limits::default()).unwrap();
    let bad = text.replace(
        "let second=*pointer",
        "let other=relay(&mut f);let second=*pointer+*other",
    );
    assert!(zil_analyze::analyze(&typed(&bad), &Limits::default()).is_err());
}

#[test]
fn consuming_capture_transfers_the_taken_capability_into_the_actual_external_slot() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/fnonce-captured-holder-publishes-taken-capability.zil"
    );
    zil_analyze::analyze(&typed(text), &Limits::default()).unwrap();
    let helper = "fn consume<F:FnOnce()->i32>(input:F)->i32{return input()}";
    let forwarded = text
        .replace("fn main", &format!("{helper} fn main"))
        .replace(" f()", " consume(f)");
    zil_analyze::analyze(&typed(&forwarded), &Limits::default()).unwrap();
    let bad = text.replace(" let result=", " b=23;let result=");
    assert!(zil_analyze::analyze(&typed(&bad), &Limits::default()).is_err());
}

#[test]
fn exclusive_stored_field_reborrow_keeps_the_actual_holder_receiver_region() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/captured-exclusive-holder-short-view.zil"
    );
    let module = typed(text);
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let bad = text.replace(
        "let second=*pointer",
        "let other=f();let second=*pointer+*other",
    );
    assert!(zil_analyze::analyze(&typed(&bad), &Limits::default()).is_err());
    let mut bad = module.clone();
    let definition = &mut bad.closures[0];
    let life = definition.storage_life.as_mut().unwrap();
    life.bindings
        .iter_mut()
        .find(|row| row.position.base == StorageBase::Result)
        .unwrap()
        .binder = StorageBinder::Fixed(1);
    bad.functions[definition.function.0 as usize].storage_life = Some(life.clone());
    assert!(
        zil_analyze::analyze(&bad, &Limits::default()).is_err(),
        "an exclusive stored-slot reborrow cannot recover its full Fixed class"
    );
}

#[test]
fn public_return_life_keeps_extra_capture_classes_and_rejects_raw_relabeling() {
    let limits = Limits::default();
    for text in [
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/captured-holder-fixed-data-result.zil"
        ),
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/captured-exclusive-short-view.zil"
        ),
    ] {
        let mut module = typed(text);
        let factory = module
            .functions
            .iter()
            .position(|f| f.name == "make")
            .unwrap();
        let id = module.functions[factory]
            .callable_return_contract
            .as_ref()
            .unwrap()
            .closure;
        let mut expected = module.closures[id.0 as usize].storage_life.clone().unwrap();
        expected
            .bindings
            .retain(|row| !matches!(row.position.base, StorageBase::Capture(_)));
        expected
            .classes
            .retain(|row| !matches!(row.position.base, StorageBase::Capture(_)));
        module.functions[factory]
            .callable_return_contract
            .as_mut()
            .unwrap()
            .life = Some(expected);
        let claim = zil_analyze::analyze(&module, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&module, &claim, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
        zil_analyze::analyze(&decoded, &limits).unwrap();
        for attack in 0..3 {
            let mut bad = module.clone();
            let expected = bad.functions[factory]
                .callable_return_contract
                .as_mut()
                .unwrap()
                .life
                .as_mut()
                .unwrap();
            match attack {
                0 => expected.fixed_binder_count -= 1,
                1 => {
                    expected
                        .bindings
                        .iter_mut()
                        .find(|row| row.position.base == StorageBase::Result)
                        .unwrap()
                        .binder = StorageBinder::Fixed(0)
                }
                _ => expected
                    .bindings
                    .retain(|row| row.position.base != StorageBase::Result),
            }
            assert!(zil_analyze::analyze(&bad, &limits).is_err());
            let wire = zil_ir::wire::encode_module_candidate(&bad, &claim, &limits).unwrap();
            let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
            assert!(zil_analyze::analyze(&decoded, &limits).is_err());
        }
    }
}

#[test]
fn receiver_and_capture_epoch_analysis_has_exact_fresh_warm_and_wire_budgets() {
    for text in [
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/captured-exclusive-holder-short-view.zil"
        ),
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/captured-exclusive-short-view-helper.zil"
        ),
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/fnonce-qualified-holder-head-transfer.zil"
        ),
    ] {
        let module = typed(text);
        let defaults = Limits::default();
        let expected = zil_analyze::analyze(&module, &defaults).unwrap();
        let (mut low, mut high) = (1usize, defaults.max_analysis_steps);
        while low < high {
            let middle = low + (high - low) / 2;
            let limits = Limits {
                max_analysis_steps: middle,
                ..defaults.clone()
            };
            match zil_analyze::analyze(&module, &limits) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = middle;
                }
                Err(errors) => {
                    assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
                    low = middle + 1;
                }
            }
        }
        eprintln!(
            "actual receiver/capture budget {}: {low}",
            module.functions[0].name
        );
        let exact = Limits {
            max_analysis_steps: low,
            ..defaults.clone()
        };
        let short = Limits {
            max_analysis_steps: low - 1,
            ..defaults.clone()
        };
        let wire = zil_ir::wire::encode_module_candidate(&module, &expected, &defaults).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &defaults).unwrap();
        for actual in [&module, &decoded] {
            assert_eq!(zil_analyze::analyze(actual, &exact).unwrap(), expected);
            assert_eq!(
                zil_analyze::analyze(actual, &short).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            let mut session = zil_analyze::AnalysisSession::default();
            assert_eq!(session.analyze(actual, &exact).unwrap(), expected);
            assert_eq!(session.analyze(actual, &exact).unwrap(), expected);
            assert_eq!(
                session.analyze(actual, &short).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            assert_eq!(session.analyze(actual, &exact).unwrap(), expected);
        }
    }
}

#[test]
fn original_named_result_scope_selects_the_real_same_typed_input() {
    for (life, actual, wrong) in [("a", "left", "right"), ("b", "right", "left")] {
        let text = format!(
            "fn make<life a,life b>(left:&'a mut i32,right:&'b mut i32)->impl FnOnce()->&'{life} mut i32{{return move()=>give {actual}}} fn main()->i32{{var left=20;var right=20;let f=make(&mut left,&mut right);let pointer=f();*pointer+=2;return *pointer-22}}"
        );
        let module = typed(&text);
        zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let bad = text.replace(&format!("give {actual}"), &format!("give {wrong}"));
        let errors = zil_types::check(&core(&bad), &TargetProfile::host_llvm(), &Limits::default())
            .unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-CALLABLE-RETURN-LIFE"),
            "{errors:?}"
        );
    }
}

#[test]
fn consuming_qualified_holder_preserves_the_complete_pointee_class_and_real_slots() {
    let text=include_str!("../../../tests/fixtures/returned-owning-holders/fnonce-qualified-holder-head-transfer.zil");
    let module=typed(text);
    let limits=Limits::default();
    let candidate=zil_analyze::analyze(&module,&limits).unwrap();
    let wire=zil_ir::wire::encode_module_candidate(&module,&candidate,&limits).unwrap();
    let (decoded,_)=zil_ir::wire::decode_module_candidate(&wire,&limits).unwrap();
    assert_eq!(zil_analyze::analyze(&decoded,&limits).unwrap(),candidate);
    let definition=&module.closures[0];
    let life=definition.storage_life.as_ref().unwrap();
    let outer=life.bindings.iter().find(|row|row.position.base==StorageBase::Result).unwrap().binder;
    let inner=life.classes.iter().find(|row|row.position.base==StorageBase::Result).unwrap().binder;
    assert_ne!(outer,inner,"the head and nominal pointee preserve distinct original scopes");
    for attack in 0..3 {
        let mut bad=module.clone();
        if attack==2 {
            for instruction in bad.functions[definition.function.0 as usize].blocks.iter_mut().flat_map(|block|&mut block.instructions) {
                if let zil_ir::InstructionKind::TakeCapture{index}=instruction.kind {
                    instruction.kind=zil_ir::InstructionKind::ReadCapture{index};
                }
            }
        } else {
            let life=bad.closures[0].storage_life.as_mut().unwrap();
            if attack==0 {
                life.classes.iter_mut().find(|row|row.position.base==StorageBase::Result).unwrap().binder=outer;
            } else {life.classes.retain(|row|row.position.base!=StorageBase::Result);}
            bad.functions[definition.function.0 as usize].storage_life=Some(life.clone());
        }
        assert!(zil_analyze::analyze(&bad,&limits).is_err(),"qualified capability attack {attack}");
        let wire=zil_ir::wire::encode_module_candidate(&bad,&candidate,&limits).unwrap();
        let (decoded,_)=zil_ir::wire::decode_module_candidate(&wire,&limits).unwrap();
        assert!(zil_analyze::analyze(&decoded,&limits).is_err(),"Wire qualified capability attack {attack}");
    }
    let bad=text.replace("let old=*data\n value=21","value=21\n let old=*data");
    let errors=zil_analyze::analyze(&typed(&bad),&limits).unwrap_err();
    assert!(errors.iter().any(|error|error.category==zil_base::ResultCategory::SafeReject),"old data SSA must retain its original root until last use: {errors:?}");
}
