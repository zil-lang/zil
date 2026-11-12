use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::StorageBase;

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

#[test]
fn consuming_slice_keeps_the_real_fixed_capability_and_complete_fat_pointer() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/fnonce-exclusive-slice-take.zil"
    );
    let module = typed(text);
    let limits = Limits::default();
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    for attack in 0..5 {
        let mut bad = module.clone();
        let definition = &module.closures[0];
        match attack {
            0 | 1 => {
                let entry = &mut bad.functions[definition.function.0 as usize];
                let block = entry
                    .blocks
                    .iter_mut()
                    .find(|block| {
                        block.instructions.iter().any(|instruction| {
                            matches!(
                                instruction.kind,
                                zil_ir::InstructionKind::TakeCapture { .. }
                            )
                        })
                    })
                    .unwrap();
                let position = block
                    .instructions
                    .iter()
                    .position(|instruction| {
                        matches!(
                            instruction.kind,
                            zil_ir::InstructionKind::TakeCapture { .. }
                        )
                    })
                    .unwrap();
                if attack == 0 {
                    let zil_ir::InstructionKind::TakeCapture { index } =
                        block.instructions[position].kind
                    else {
                        unreachable!()
                    };
                    block.instructions[position].kind =
                        zil_ir::InstructionKind::ReadCapture { index };
                } else {
                    let mut duplicate = block.instructions[position].clone();
                    let ty = entry.values[duplicate.result.unwrap().0 as usize].clone();
                    duplicate.result = Some(zil_ir::ValueId(entry.values.len() as u32));
                    entry.values.push(ty);
                    block.instructions.insert(position + 1, duplicate);
                }
            }
            2 => {
                let factory = bad
                    .functions
                    .iter_mut()
                    .find(|function| function.name == "make")
                    .unwrap();
                factory
                    .callable_return_contract
                    .as_mut()
                    .unwrap()
                    .capture_sources = Some(vec![]);
            }
            3 => {
                let factory = bad
                    .functions
                    .iter_mut()
                    .find(|function| function.name == "make")
                    .unwrap();
                factory.callable_return_contract.as_mut().unwrap().cleanup = vec![0];
            }
            _ => {
                let main = bad
                    .functions
                    .iter_mut()
                    .find(|function| function.name == "main")
                    .unwrap();
                let instruction=main.blocks.iter_mut().flat_map(|block|&mut block.instructions).find(|instruction|matches!(instruction.kind,zil_ir::InstructionKind::TakeLocal{local}if matches!(main.locals[local.0 as usize].ty,zil_ir::Type::Closure(_)))).unwrap();
                let zil_ir::InstructionKind::TakeLocal { local } = instruction.kind else {
                    unreachable!()
                };
                instruction.kind = zil_ir::InstructionKind::Load(local);
            }
        }
        assert!(
            zil_analyze::analyze(&bad, &limits).is_err(),
            "Slice capability attack {attack}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&bad, &candidate, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
        assert!(
            zil_analyze::analyze(&decoded, &limits).is_err(),
            "Wire Slice capability attack {attack}"
        );
    }
    let bad = text.replace("let result=", "bytes[1]=0u8;let result=");
    let errors = zil_analyze::analyze(&typed(&bad), &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.category == zil_base::ResultCategory::SafeReject),
        "returned slice preserves the original array parent until last use: {errors:?}"
    );
}

#[test]
fn qualified_heads_and_slices_have_exact_fresh_warm_wire_analysis_budgets() {
    for text in [
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/fnonce-exclusive-slice-take.zil"
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
fn actual_static_byte_capture_proposes_a_complete_static_source_without_inventing_a_free_domain() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/static-byte-slice-capture.zil"
    );
    let module = typed(text);
    let factory = module
        .functions
        .iter()
        .find(|function| function.name == "make")
        .unwrap();
    let definition = &module.closures[0];
    assert!(
        definition.storage_life.is_none(),
        "a missing domain remains unknown; no input-grounded Fixed class is invented"
    );
    let sources = factory
        .callable_return_contract
        .as_ref()
        .unwrap()
        .capture_sources
        .as_ref()
        .unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources[0].position,
        zil_ir::StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![]
        }
    );
    assert_eq!(
        sources[0].sources,
        vec![zil_ir::StorageSource::Static {
            leaf: zil_ir::Type::Address {
                mutable: false,
                inner: Box::new(zil_ir::Type::Slice {
                    element: Box::new(zil_ir::Type::U8)
                })
            },
            mode: zil_ir::StoredBorrowMode::SharedAddress,
        }]
    );
}
