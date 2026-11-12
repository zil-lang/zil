//! Frozen untrusted Types/Analyze outputs, checked here without either producer.
use zil_base::Limits;
use zil_ir::wire;
const SIMPLE: &[u8] =
    include_bytes!("../../../tests/returned-environment-sources/two-addresses.wire");
const OWNED: &[u8] =
    include_bytes!("../../../tests/returned-environment-sources/owned-projected-multiple.wire");
#[test]
fn actual_returned_environments_survive_independent_raw_and_wire_check() {
    for bytes in [SIMPLE, OWNED] {
        let (module, plan) = wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        let target = module.target.clone();
        zil_verify::verify_module(module.clone(), plan, &target, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(bytes, &target, &Limits::default()).unwrap();
    }
}
#[test]
fn same_type_forged_sources_empty_and_unknown_never_supply_authority() {
    let (original, plan) = wire::decode_module_candidate(SIMPLE, &Limits::default()).unwrap();
    let factory = original
        .functions
        .iter()
        .position(|f| f.name == "make")
        .unwrap();
    for mode in 0..5 {
        let mut module = original.clone();
        let mut plan = plan.clone();
        let sources = &mut module.functions[factory]
            .callable_return_contract
            .as_mut()
            .unwrap()
            .capture_sources;
        match mode {
            0 => *sources = None,
            1 => *sources = Some(vec![]),
            2 => sources.as_mut().unwrap()[0].sources[0].base = zil_ir::StorageBase::Parameter(1),
            3 => sources.as_mut().unwrap()[0].position.base = zil_ir::StorageBase::Capture(99),
            4 => {
                let instruction = module.functions[factory]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, zil_ir::InstructionKind::MakeClosure { .. }))
                    .unwrap();
                if let zil_ir::InstructionKind::MakeClosure { captures, .. } = &mut instruction.kind
                {
                    captures[0] = captures[1];
                }
            }
            _ => unreachable!(),
        }
        plan.module_fingerprint = module.fingerprint();
        let target = module.target.clone();
        assert!(
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default())
                .is_err(),
            "mutation {mode}"
        );
        let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()).is_err(),
            "wire mutation {mode}"
        );
    }
}

#[test]
fn factory_result_keeps_the_real_parent_loan_until_its_last_use() {
    use zil_ir::{Constant, Instruction, InstructionKind, Type, ValueHold, ValueId};
    for before_last_use in [true, false] {
        let (mut module, mut plan) =
            wire::decode_module_candidate(SIMPLE, &Limits::default()).unwrap();
        let main = module
            .functions
            .iter()
            .position(|f| f.name == "main")
            .unwrap();
        let f = &mut module.functions[main];
        let local = f.locals.iter().find(|l| l.name == "a").unwrap().id;
        f.locals[local.0 as usize].mutable = true;
        let value = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions.iter().any(|i| {
                    matches!(
                        i.kind,
                        InstructionKind::CallClosureShared { .. }
                            | InstructionKind::CallClosure { .. }
                    )
                })
            })
            .unwrap();
        let call = block
            .instructions
            .iter()
            .position(|i| {
                matches!(
                    i.kind,
                    InstructionKind::CallClosureShared { .. } | InstructionKind::CallClosure { .. }
                )
            })
            .unwrap();
        let at = call + usize::from(!before_last_use);
        let span = f.span;
        block.instructions.splice(
            at..at,
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
        plan.functions[main].holds.push(ValueHold {
            value,
            roots: vec![],
        });
        plan.module_fingerprint = module.fingerprint();
        let target = module.target.clone();
        let raw =
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default());
        let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        let encoded = zil_verify::verify_module_bytes(&bytes, &target, &Limits::default());
        if before_last_use {
            assert!(raw.is_err(), "factory detached the input loan");
            assert!(encoded.is_err());
        } else {
            raw.unwrap();
            encoded.unwrap();
        }
    }
}

#[test]
fn stored_pointer_factory_retains_the_target_after_the_holder_changes() {
    for bytes in [
        include_bytes!("../../../tests/returned-environment-sources/holder-pointer.wire")
            .as_slice(),
        include_bytes!("../../../tests/returned-environment-sources/holder-pointer-replaced.wire")
            .as_slice(),
    ] {
        let (module, plan) = wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        let target = module.target.clone();
        zil_verify::verify_module(module.clone(), plan, &target, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(bytes, &target, &Limits::default()).unwrap();
    }
}

#[test]
fn replacing_the_holder_preserves_the_old_target_loan_and_last_use() {
    use zil_ir::{Constant, Instruction, InstructionKind, Type, ValueHold, ValueId};
    let bytes =
        include_bytes!("../../../tests/returned-environment-sources/holder-pointer-replaced.wire");
    for before_last_use in [true, false] {
        let (mut module, mut plan) =
            wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        let main = module
            .functions
            .iter()
            .position(|f| f.name == "main")
            .unwrap();
        let f = &mut module.functions[main];
        let local = f.locals.iter().find(|l| l.name == "n").unwrap().id;
        let value = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions.iter().any(|i| {
                    matches!(
                        i.kind,
                        InstructionKind::CallClosure { .. }
                            | InstructionKind::CallClosureShared { .. }
                    )
                })
            })
            .unwrap();
        let call = block
            .instructions
            .iter()
            .position(|i| {
                matches!(
                    i.kind,
                    InstructionKind::CallClosure { .. } | InstructionKind::CallClosureShared { .. }
                )
            })
            .unwrap();
        let at = call + usize::from(!before_last_use);
        let span = f.span;
        block.instructions.splice(
            at..at,
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
        plan.functions[main].holds.push(ValueHold {
            value,
            roots: vec![],
        });
        plan.module_fingerprint = module.fingerprint();
        let target = module.target.clone();
        let raw =
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default());
        let wire = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        let encoded = zil_verify::verify_module_bytes(&wire, &target, &Limits::default());
        if before_last_use {
            assert!(raw.is_err(), "replacing holder detached old target n");
            assert!(encoded.is_err());
        } else {
            raw.unwrap();
            encoded.unwrap();
        }
    }
}

#[test]
fn an_actual_stack_capture_cannot_advertise_an_input_source() {
    use zil_ir::{
        Constant, HoldRoot, Instruction, InstructionKind, LocalDecl, LocalId, Type, ValueHold,
        ValueId,
    };
    let (mut module, mut plan) = wire::decode_module_candidate(SIMPLE, &Limits::default()).unwrap();
    let factory = module
        .functions
        .iter()
        .position(|f| f.name == "make")
        .unwrap();
    let f = &mut module.functions[factory];
    let local = LocalId(f.locals.len() as u32);
    let scalar = ValueId(f.values.len() as u32);
    let address = ValueId(scalar.0 + 1);
    let span = f.span;
    f.locals.push(LocalDecl {
        id: local,
        name: "stack_capture".into(),
        ty: Type::I32,
        mutable: false,
        span,
    });
    f.values.extend([
        Type::I32,
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
    ]);
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        })
        .unwrap();
    let at = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    block.instructions.splice(
        at..at,
        [
            Instruction {
                result: None,
                kind: InstructionKind::StorageLive(local),
                span,
            },
            Instruction {
                result: Some(scalar),
                kind: InstructionKind::Constant(Constant::I32(99)),
                span,
            },
            Instruction {
                result: None,
                kind: InstructionKind::Store {
                    local,
                    value: scalar,
                },
                span,
            },
            Instruction {
                result: Some(address),
                kind: InstructionKind::BorrowLocal { local },
                span,
            },
        ],
    );
    if let InstructionKind::MakeClosure { captures, .. } = &mut block.instructions[at + 4].kind {
        captures[0] = address;
    }
    let exit = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
        .unwrap();
    block.instructions.insert(
        exit,
        Instruction {
            result: None,
            kind: InstructionKind::StorageDead(local),
            span,
        },
    );
    plan.functions[factory].holds.extend([
        ValueHold {
            value: scalar,
            roots: vec![],
        },
        ValueHold {
            value: address,
            roots: vec![HoldRoot::Local(local)],
        },
    ]);
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    let errors =
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default())
            .unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-VERIFY-CALLABLE-SOURCE"),
        "{errors:?}"
    );
    let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()).is_err());
}

#[test]
fn fresh_shared_address_environment_cannot_acquire_owning_state_capability() {
    use zil_ir::{LocalDecl, LocalId, Type};
    let (mut module, mut plan) = wire::decode_module_candidate(OWNED, &Limits::default()).unwrap();
    let closure = module
        .functions
        .iter()
        .find_map(|f| f.callable_return_contract.as_ref().map(|c| c.closure))
        .unwrap();
    let owner = module.closures[closure.0 as usize]
        .captures
        .iter()
        .find(|ty| matches!(ty, Type::Package { .. }))
        .unwrap()
        .clone();
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let f = &mut module.functions[main];
    f.locals.push(LocalDecl {
        id: LocalId(f.locals.len() as u32),
        name: "forged_owning_state".into(),
        mutable: false,
        span: f.span,
        ty: Type::Package {
            name: "ForgedOwningState".into(),
            fields: vec![
                ("owner".into(), owner),
                ("callback".into(), Type::Closure(closure)),
            ],
        },
    });
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    let errors =
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default())
            .unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-UNSUPPORTED"),
        "{errors:?}"
    );
    let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()).is_err());
}

#[test]
fn actual_nested_and_wrapped_borrowed_environments_preserve_every_leaf() {
    for bytes in [
        include_bytes!("../../../tests/returned-environment-sources/nested-borrowed.wire")
            .as_slice(),
        include_bytes!("../../../tests/returned-environment-sources/wrapped-borrowed.wire")
            .as_slice(),
    ] {
        let (module, plan) = wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        let target = module.target.clone();
        zil_verify::verify_module(module.clone(), plan, &target, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(bytes, &target, &Limits::default()).unwrap();
    }
}

#[test]
fn cfg_loop_sources_require_both_the_entry_and_reachable_backedge_seed() {
    let bytes = include_bytes!("../../../tests/returned-environment-sources/loop-sources.wire");
    let (original, plan) = wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
    let target = original.target.clone();
    zil_verify::verify_module(original.clone(), plan.clone(), &target, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(bytes, &target, &Limits::default()).unwrap();
    let factory = original
        .functions
        .iter()
        .position(|f| f.name == "make")
        .unwrap();
    for omitted in 0..2 {
        let mut module = original.clone();
        let bindings = module.functions[factory]
            .callable_return_contract
            .as_mut()
            .unwrap()
            .capture_sources
            .as_mut()
            .unwrap();
        let binding = bindings
            .iter_mut()
            .find(|binding| binding.sources.len() == 2)
            .unwrap();
        binding.sources.remove(omitted);
        let mut plan = plan.clone();
        plan.module_fingerprint = module.fingerprint();
        let errors =
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default())
                .unwrap_err();
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-CALLABLE-SOURCE"),
            "{errors:?}"
        );
        let wire = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        assert!(zil_verify::verify_module_bytes(&wire, &target, &Limits::default()).is_err());
    }
}

#[test]
fn nested_environment_keeps_its_actual_original_parent_until_last_use() {
    use zil_ir::{Constant, Instruction, InstructionKind, Type, ValueHold, ValueId};
    for bytes in [
        include_bytes!("../../../tests/returned-environment-sources/nested-borrowed.wire")
            .as_slice(),
        include_bytes!("../../../tests/returned-environment-sources/wrapped-borrowed.wire")
            .as_slice(),
    ] {
        for before_last_use in [true, false] {
            let (mut module, mut plan) =
                wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
            let main = module
                .functions
                .iter()
                .position(|f| f.name == "main")
                .unwrap();
            let f = &mut module.functions[main];
            let local = f.locals.iter().find(|l| l.name == "value").unwrap().id;
            f.locals[local.0 as usize].mutable = true;
            let value = ValueId(f.values.len() as u32);
            f.values.push(Type::I32);
            let block = f
                .blocks
                .iter_mut()
                .find(|b| {
                    b.instructions.iter().any(|i| {
                        matches!(
                            i.kind,
                            InstructionKind::CallClosure { .. }
                                | InstructionKind::CallClosureShared { .. }
                        )
                    })
                })
                .unwrap();
            let at = block
                .instructions
                .iter()
                .position(|i| {
                    matches!(
                        i.kind,
                        InstructionKind::CallClosure { .. }
                            | InstructionKind::CallClosureShared { .. }
                    )
                })
                .unwrap()
                + usize::from(!before_last_use);
            let span = f.span;
            block.instructions.splice(
                at..at,
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
            plan.functions[main].holds.push(ValueHold {
                value,
                roots: vec![],
            });
            plan.module_fingerprint = module.fingerprint();
            let target = module.target.clone();
            let raw = zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &target,
                &Limits::default(),
            );
            let wire = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
            let encoded = zil_verify::verify_module_bytes(&wire, &target, &Limits::default());
            if before_last_use {
                assert!(raw.is_err());
                assert!(encoded.is_err());
            } else {
                raw.unwrap();
                encoded.unwrap();
            }
        }
    }
}

#[test]
fn fresh_request_budget_boundaries_cover_direct_stored_nested_and_cyclic_sources() {
    use zil_base::ResultCategory;
    for bytes in [
        SIMPLE,
        include_bytes!("../../../tests/returned-environment-sources/holder-pointer-replaced.wire")
            .as_slice(),
        include_bytes!("../../../tests/returned-environment-sources/wrapped-borrowed.wire")
            .as_slice(),
        include_bytes!("../../../tests/returned-environment-sources/loop-sources.wire").as_slice(),
    ] {
        let (module, plan) = wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        for encoded in [false, true] {
            let run = |steps| {
                let limits = Limits {
                    max_analysis_steps: steps,
                    ..Limits::default()
                };
                if encoded {
                    zil_verify::verify_module_bytes(bytes, &module.target, &limits)
                } else {
                    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                }
            };
            let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
            run(high).unwrap();
            while low + 1 < high {
                let at = low + (high - low) / 2;
                match run(at) {
                    Ok(_) => high = at,
                    Err(errors) => {
                        assert!(
                            errors
                                .iter()
                                .all(|d| d.category == ResultCategory::ResourceLimit),
                            "{errors:?}"
                        );
                        low = at;
                    }
                }
            }
            run(high).unwrap();
            assert!(
                run(high - 1)
                    .unwrap_err()
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit)
            );
            run(high).unwrap();
        }
    }
}
