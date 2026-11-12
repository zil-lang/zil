//! Whole Source, raw candidate, decoded candidate and real machine consumers.
use source_receiver_contracts::typed;
use std::{fs, process::Command};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{StorageBase, StorageBinder};
fn accepted(name: &str, text: &str) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
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
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let verified = zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits);
    if verified.is_err() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/runs/receiver");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("{name}-typed.txt")),
            format!("{module:#?}"),
        )
        .unwrap();
    }
    verified.unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-receiver-{}-{name}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let report = compile_source(
                &source,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(dir.join("program")),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}
macro_rules! consumer {
    ($test:ident, $source:literal) => {
        #[test]
        fn $test() {
            accepted(
                $source,
                include_str!(concat!(
                    "../../../tests/fixtures/returned-owning-holders/",
                    $source,
                    ".zil"
                )),
            );
        }
    };
}
consumer!(
    captured_mutable_holder_updates_original_storage,
    "captured-mutable-holder"
);
consumer!(
    captured_mutable_holder_survives_borrowed_helpers,
    "captured-mutable-holder-helper"
);
consumer!(
    same_entry_receivers_keep_distinct_holders,
    "captured-mutable-holder-two-receivers"
);
consumer!(
    readonly_exclusive_capture_reborrows_without_copying_environment,
    "exclusive-capture-readonly-fn"
);
consumer!(
    call_before_return_keeps_actual_holder_update,
    "captured-mutable-holder-call-before-return"
);

mod source_receiver_contracts {
    use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
    use zil_ir::{StorageBase, StorageBinder};

    pub(super) fn typed(text: &str) -> zil_ir::TypedModule {
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
        zil_types::check(&core, &target, &limits).unwrap()
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
}

consumer!(
    exclusive_capture_shared_view_keeps_the_live_receiver,
    "captured-exclusive-short-view"
);

consumer!(
    shared_external_capture_outlives_receiver_cleanup,
    "shared-capture-fixed-result"
);
consumer!(
    shared_pointee_slot_copy_outlives_receiver_cleanup,
    "captured-holder-fixed-data-result"
);
consumer!(
    short_receiver_view_crosses_two_helpers,
    "captured-exclusive-short-view-helper"
);
consumer!(
    inferred_consuming_capture_transfers_the_actual_exclusive_pointer,
    "fnonce-exclusive-capture-take-inferred-result"
);
consumer!(
    consuming_capture_publishes_actual_original_holder,
    "fnonce-captured-holder-publishes-taken-capability"
);

mod additional_source_cases {
    use super::source_receiver_contracts::typed;
    use zil_base::Limits;
    use zil_ir::StorageBase;
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

mod public_life_source_proofs {
    use super::source_receiver_contracts::typed;
    use zil_base::Limits;
    use zil_ir::{StorageBase, StorageBinder};
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
            let wire =
                zil_ir::wire::encode_module_candidate(&module, &expected, &defaults).unwrap();
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
}

mod original_named_source_scope {
    use super::source_receiver_contracts::typed;
    use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
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
    #[test]
    fn original_named_result_scope_selects_the_real_same_typed_input() {
        for (life, actual, wrong) in [("a", "left", "right"), ("b", "right", "left")] {
            let text = format!(
                "fn make<life a,life b>(left:&'a mut i32,right:&'b mut i32)->impl FnOnce()->&'{life} mut i32{{return move()=>give {actual}}} fn main()->i32{{var left=20;var right=20;let f=make(&mut left,&mut right);let pointer=f();*pointer+=2;return *pointer-22}}"
            );
            let module = typed(&text);
            zil_analyze::analyze(&module, &Limits::default()).unwrap();
            let bad = text.replace(&format!("give {actual}"), &format!("give {wrong}"));
            let errors =
                zil_types::check(&core(&bad), &TargetProfile::host_llvm(), &Limits::default())
                    .unwrap_err();
            assert!(
                errors.iter().any(|e| e.code == "E-CALLABLE-RETURN-LIFE"),
                "{errors:?}"
            );
        }
    }
}

consumer!(
    named_consuming_capture_transfers_the_actual_exclusive_pointer,
    "fnonce-exclusive-capture-take"
);
#[test]
fn same_typed_inputs_keep_the_original_named_result_at_runtime() {
    for (life, actual, expected) in [("a", "left", 22), ("b", "right", 42)] {
        let text = format!(
            "fn make<life a,life b>(left:&'a mut i32,right:&'b mut i32)->impl FnOnce()->&'{life} mut i32{{return move()=>give {actual}}} fn main()->i32{{var left=20;var right=40;let f=make(&mut left,&mut right);let pointer=f();*pointer+=2;let observed=*pointer;if observed!={expected}{{return 1}};return left+right-62}}"
        );
        accepted(&format!("named-return-{life}"), &text);
    }
}

#[test]
fn static_text_capture_has_an_explicit_complete_source() {
    accepted(
        "static-captured-text",
        r#"fn make()->impl Fn()->u8{let text="abc";return move()=>text[0]} fn main()->i32{let f=make();return i32(f())-97}"#,
    );
}

fn independently_reject_typed_attack(
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    label: &str,
) {
    let limits = Limits::default();
    let target = module.target.clone();
    // A stale cache identity must not substitute for a semantic rejection.
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for (transport, checked) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits).map(|_| ()),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).map(|_| ()),
        ),
    ] {
        let errors = checked.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.phase == zil_base::Phase::Verify
                    && error.category == ResultCategory::SafeReject
                    && error.code != "E-VERIFY-MODULE"),
            "{label} via {transport}: {errors:?}"
        );
    }
}

#[test]
fn verify_reconstructs_taken_capture_vacancy_and_receiver_result_permissions() {
    use zil_ir::{InstructionKind, ValueHold, ValueId};
    let original = typed(include_str!(
        "../../../tests/fixtures/returned-owning-holders/fnonce-exclusive-capture-take.zil"
    ));
    let plan = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    zil_verify::verify_module(
        original.clone(),
        plan.clone(),
        &original.target,
        &Limits::default(),
    )
    .unwrap();
    let entry = original.closures[0].function;
    for attack in 0..3 {
        let mut bad = original.clone();
        let mut candidate = plan.clone();
        let function = &mut bad.functions[entry.0 as usize];
        let (block, at) = function
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block, body)| {
                body.instructions
                    .iter()
                    .position(|instruction| {
                        matches!(instruction.kind, InstructionKind::TakeCapture { .. })
                    })
                    .map(|at| (block, at))
            })
            .unwrap();
        let mut instruction = function.blocks[block].instructions[at].clone();
        let InstructionKind::TakeCapture { index } = instruction.kind else {
            unreachable!()
        };
        if attack == 0 {
            function.blocks[block].instructions[at].kind = InstructionKind::ReadCapture { index };
        } else {
            let original_value = instruction.result.unwrap();
            let result = ValueId(function.values.len() as u32);
            function
                .values
                .push(function.values[original_value.0 as usize].clone());
            instruction.result = Some(result);
            if attack == 2 {
                instruction.kind = InstructionKind::ReadCapture { index };
            }
            function.blocks[block]
                .instructions
                .insert(at + 1, instruction);
            let function_plan = candidate
                .functions
                .iter_mut()
                .find(|p| p.function == entry)
                .unwrap();
            let roots = function_plan
                .holds
                .iter()
                .find(|hold| hold.value == original_value)
                .map(|hold| hold.roots.clone())
                .unwrap_or_default();
            function_plan.holds.push(ValueHold {
                value: result,
                roots,
            });
        }
        independently_reject_typed_attack(
            bad,
            candidate,
            &format!("capture take mutation {attack}"),
        );
    }
    let original = typed(include_str!(
        "../../../tests/fixtures/returned-owning-holders/captured-exclusive-short-view.zil"
    ));
    let plan = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    zil_verify::verify_module(
        original.clone(),
        plan.clone(),
        &original.target,
        &Limits::default(),
    )
    .unwrap();
    for attack in 0..2 {
        let mut bad = original.clone();
        let definition = &mut bad.closures[0];
        let life = definition.storage_life.as_mut().unwrap();
        if attack == 0 {
            life.bindings
                .iter_mut()
                .find(|row| row.position.base == StorageBase::Result)
                .unwrap()
                .binder = StorageBinder::Fixed(0);
        } else {
            life.bindings
                .retain(|row| row.position.base != StorageBase::Receiver);
        }
        bad.functions[definition.function.0 as usize].storage_life = Some(life.clone());
        independently_reject_typed_attack(
            bad,
            plan.clone(),
            &format!("receiver mutation {attack}"),
        );
    }
}

#[test]
fn verify_authenticates_the_complete_expected_public_return_domain() {
    for text in [
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/captured-holder-fixed-data-result.zil"
        ),
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/captured-exclusive-short-view.zil"
        ),
    ] {
        let mut original = typed(text);
        let factory = original
            .functions
            .iter()
            .position(|f| f.name == "make")
            .unwrap();
        let closure = original.functions[factory]
            .callable_return_contract
            .as_ref()
            .unwrap()
            .closure;
        let mut life = original.closures[closure.0 as usize]
            .storage_life
            .clone()
            .unwrap();
        life.bindings
            .retain(|row| !matches!(row.position.base, StorageBase::Capture(_)));
        life.classes
            .retain(|row| !matches!(row.position.base, StorageBase::Capture(_)));
        original.functions[factory]
            .callable_return_contract
            .as_mut()
            .unwrap()
            .life = Some(life);
        let plan = zil_analyze::analyze(&original, &Limits::default()).unwrap();
        zil_verify::verify_module(
            original.clone(),
            plan.clone(),
            &original.target,
            &Limits::default(),
        )
        .unwrap();
        for attack in 0..3 {
            let mut bad = original.clone();
            let expected = bad.functions[factory]
                .callable_return_contract
                .as_mut()
                .unwrap()
                .life
                .as_mut()
                .unwrap();
            match attack {
                0 => expected.fixed_binder_count += 1,
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
            independently_reject_typed_attack(
                bad,
                plan.clone(),
                &format!("expected return mutation {attack}"),
            );
        }
    }
}

#[test]
fn consuming_qualified_holder_preserves_the_complete_pointee_class_and_real_slots() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/fnonce-qualified-holder-head-transfer.zil"
    );
    let module = typed(text);
    let limits = Limits::default();
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits).unwrap();
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
        }
        assert!(
            zil_analyze::analyze(&bad, &limits).is_err(),
            "qualified capability attack {attack}"
        );
        independently_reject_typed_attack(
            bad.clone(),
            candidate.clone(),
            &format!("qualified return {attack}"),
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

consumer!(
    consuming_qualified_holder_head_preserves_actual_data_and_head_scopes_at_runtime,
    "fnonce-qualified-holder-head-transfer"
);

consumer!(
    consuming_qualified_head_reads_the_current_pointee_after_a_local_transfer,
    "fnonce-qualified-head-updates-pointee"
);

#[test]
fn consuming_capture_with_two_possible_original_holders_keeps_weak_updates() {
    use zil_ir::{Constant, Instruction, InstructionKind, Type, ValueHold, ValueId};
    let source = include_str!(
        "../../../tests/fixtures/returned-owning-holders/fnonce-multiple-possible-holders.zil"
    );
    let original = typed(source);
    let plan = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    zil_verify::verify_module(
        original.clone(),
        plan.clone(),
        &original.target,
        &Limits::default(),
    )
    .unwrap();
    accepted("multiple-possible-holders", source);
    let mut bad = original;
    let mut claim = plan;
    let function = bad.functions.iter_mut().find(|f| f.name == "run").unwrap();
    let local = function
        .locals
        .iter()
        .find(|local| local.name == "a")
        .unwrap()
        .id;
    let (block, at) = function
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block, body)| {
            body.instructions
                .iter()
                .position(|instruction| {
                    matches!(instruction.kind, InstructionKind::CallClosureOnce { .. })
                })
                .map(|at| (block, at))
        })
        .unwrap();
    let span = function.blocks[block].instructions[at].span;
    let value = ValueId(function.values.len() as u32);
    function.values.push(Type::I32);
    function.blocks[block].instructions.splice(
        at + 1..at + 1,
        [
            Instruction {
                result: Some(value),
                kind: InstructionKind::Constant(Constant::I32(99)),
                span,
            },
            Instruction {
                result: None,
                kind: InstructionKind::Store { local, value },
                span,
            },
        ],
    );
    claim
        .functions
        .iter_mut()
        .find(|p| p.function == function.id)
        .unwrap()
        .holds
        .push(ValueHold {
            value,
            roots: vec![],
        });
    independently_reject_typed_attack(
        bad,
        claim,
        "weak update cannot erase an untouched original holder",
    );
}
