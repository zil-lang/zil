//! Actual Source-created field view; no field or lifetime is inferred from a
//! matching machine layout, a candidate root cache, or the Analyze decision.
use zil_base::Limits;
use zil_ir::{ContractProjection, PlaceProjection, StorageBase, StorageSource, StoredBorrowMode};

#[test]
fn actual_field_address_source_preserves_its_exact_actual_window_raw_and_wire() {
    let limits = Limits::default();
    let bytes = include_bytes!("fixtures/dyn-context/context-place-address.candidate.wire");
    let (module, plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let atom = &module.functions[create]
        .retained_return_sources
        .as_ref()
        .unwrap()[0]
        .sources[0];
    let StorageSource::PlaceAddress { place, mode } = atom else {
        panic!("{atom:?}")
    };
    assert_eq!(place.base, StorageBase::Parameter(0));
    assert_eq!(
        place.path,
        [ContractProjection::Place(PlaceProjection::Field(0))]
    );
    assert_eq!(*mode, StoredBorrowMode::SharedAddress);
    for attack in 0..6 {
        let mut wrong = module.clone();
        let sources = &mut wrong.functions[create]
            .retained_return_sources
            .as_mut()
            .unwrap()[0]
            .sources;
        match attack {
            0 => {
                let StorageSource::PlaceAddress { place, .. } = &mut sources[0] else {
                    unreachable!()
                };
                place.path = vec![ContractProjection::Place(PlaceProjection::Field(1))];
            }
            1 => {
                sources[0] = StorageSource::Exact(place.clone());
            }
            2 => {
                let StorageSource::PlaceAddress { place, .. } = &mut sources[0] else {
                    unreachable!()
                };
                place.base = StorageBase::Parameter(1);
            }
            3 => {
                let StorageSource::PlaceAddress { mode, .. } = &mut sources[0] else {
                    unreachable!()
                };
                *mode = StoredBorrowMode::ExclusiveAddress;
            }
            4 => sources.clear(),
            _ => sources.push(sources[0].clone()),
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = wrong.fingerprint();
        for wire in [false, true] {
            let error = if wire {
                let encoded =
                    zil_ir::wire::encode_module_candidate(&wrong, &candidate, &limits).unwrap();
                zil_verify::verify_module_bytes(&encoded, &target, &limits).unwrap_err()
            } else {
                zil_verify::verify_module(wrong.clone(), candidate.clone(), &target, &limits)
                    .unwrap_err()
            };
            assert!(
                error.iter().any(|d| d.code == "E-VERIFY-DYN-SOURCE"),
                "attack={attack}, Wire={wire}: {error:?}"
            );
        }
    }
}

#[test]
fn actual_source_field_view_accepts_a_proved_conservative_owned_region_upper() {
    let limits = Limits::default();
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/context-place-address.candidate.wire"),
        &limits,
    )
    .unwrap();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let zil_ir::Type::ErrorUnion { result, .. } = &module.functions[create].return_type else {
        unreachable!()
    };
    let zil_ir::Type::OwnedDyn { interface } = result.as_ref() else {
        unreachable!()
    };
    let leaf = module
        .dyn_interfaces
        .iter()
        .find(|item| item.id == *interface)
        .unwrap()
        .retained_life
        .ports[0]
        .leaf
        .clone();
    // Actual executable Source/SSA and every header stay fixed. This legal
    // broader upper includes both real Pair fields, without claiming either
    // is an actual returned value or changing the actual body summary.
    module.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources = vec![StorageSource::OwnedView {
        head: zil_ir::StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        },
        leaf,
        mode: StoredBorrowMode::SharedAddress,
    }];
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let encoded = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&encoded, &target, &limits).unwrap();
    // Keep the same real Field0 producer and claim only the distinct Field1
    // owned region. Same Type and same class do not identify the instance.
    let StorageSource::OwnedView { head, .. } = &mut module.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources[0]
    else {
        unreachable!()
    };
    head.path = vec![ContractProjection::Place(PlaceProjection::Field(1))];
    plan.module_fingerprint = module.fingerprint();
    for wire in [false, true] {
        let error = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap_err()
        };
        assert!(
            error.iter().any(|d| d.code == "E-VERIFY-DYN-SOURCE"),
            "{wire}: {error:?}"
        );
    }
}

#[test]
fn shared_field_view_from_mutable_head_retains_its_actual_window_not_its_sibling() {
    use zil_ir::{Constant, Instruction, InstructionKind, Type, ValueHold, ValueId};
    let limits = Limits::default();
    let bytes = include_bytes!("fixtures/dyn-context/context-shared-from-mutable.candidate.wire");
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap();
    assert!(
        module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                InstructionKind::ReborrowAddress { mutable: false, .. }
            ))
    );
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let creator = module.functions[create].id;
    // A verified narrower upper is allowed because this actual Source body
    // always returns &value.left. The public base roots remain identical.
    module.functions[create].retained_return_sources.as_mut().unwrap()[0].sources.retain(|atom|
        matches!(atom, StorageSource::PlaceAddress { place, mode: StoredBorrowMode::SharedAddress }
            if place.base == StorageBase::Parameter(0) && place.path == [ContractProjection::Place(PlaceProjection::Field(0))]));
    assert_eq!(
        module.functions[create]
            .retained_return_sources
            .as_ref()
            .unwrap()[0]
            .sources
            .len(),
        1
    );
    let mut site = None;
    for (function, f) in module.functions.iter().enumerate() {
        for (block, body) in f.blocks.iter().enumerate() {
            for (offset, i) in body.instructions.iter().enumerate() {
                if let InstructionKind::Call {
                    function: callee,
                    arguments,
                } = &i.kind
                    && *callee == creator
                {
                    site = Some((function, block, offset, arguments[0], i.span));
                }
            }
        }
    }
    let (function, block, offset, argument, span) = site.unwrap();
    let value = ValueId(module.functions[function].values.len() as u32);
    let address = ValueId(value.0 + 1);
    let f = &mut module.functions[function];
    f.values.extend([
        Type::I32,
        Type::Address {
            mutable: true,
            inner: Box::new(Type::I32),
        },
    ]);
    f.blocks[block].instructions.splice(
        offset + 1..offset + 1,
        [
            Instruction {
                result: Some(value),
                kind: InstructionKind::Constant(Constant::I32(7)),
                span,
            },
            Instruction {
                result: Some(address),
                kind: InstructionKind::AddressField {
                    address: argument,
                    index: 0,
                },
                span,
            },
            Instruction {
                result: None,
                kind: InstructionKind::WriteAddress { address, value },
                span,
            },
        ],
    );
    let holds = &mut plan
        .functions
        .iter_mut()
        .find(|p| p.function == f.id)
        .unwrap()
        .holds;
    holds.extend([
        ValueHold {
            value,
            roots: vec![],
        },
        ValueHold {
            value: address,
            roots: vec![zil_ir::HoldRoot::Parameter(0)],
        },
    ]);
    plan.module_fingerprint = module.fingerprint();
    for wire in [false, true] {
        let error = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap_err()
        };
        assert!(
            error.iter().any(
                |d| d.code == "E-VERIFY-STORED" && d.message.contains("live incompatible loan")
            ),
            "{wire}: {error:?}"
        );
    }
    // Change only the actual physical field selected by the original argument
    // SSA. No class, source declaration, parent or cleanup is weakened.
    let InstructionKind::AddressField { index, .. } =
        &mut module.functions[function].blocks[block].instructions[offset + 2].kind
    else {
        unreachable!()
    };
    *index = 1;
    plan.module_fingerprint = module.fingerprint();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}
