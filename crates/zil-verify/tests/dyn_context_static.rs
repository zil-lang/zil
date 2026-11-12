//! Actual static bytes remain targets, with checked indexing and conversion
//! effects. No empty source list or same-class input is static evidence.
use zil_base::Limits;
use zil_ir::{DynEffect, StorageSource};

#[test]
fn actual_static_saved_slice_is_read_and_its_complete_source_and_effects_are_checked() {
    let limits = Limits::default();
    let bytes = include_bytes!("fixtures/dyn-context/context-static-input.candidate.wire");
    let (module, plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    assert!(
        module.functions[create]
            .retained_return_sources
            .as_ref()
            .unwrap()[0]
            .sources
            .iter()
            .any(|source| matches!(source, StorageSource::Static { .. }))
    );
    for case in 0..4 {
        let mut attack = module.clone();
        match case {
            0 => attack.functions[create]
                .retained_return_sources
                .as_mut()
                .unwrap()[0]
                .sources
                .clear(),
            1 => attack.functions[create].retained_return_sources = Some(vec![]),
            2 => {
                attack.functions[create]
                    .retained_return_sources
                    .as_mut()
                    .unwrap()[0]
                    .sources = vec![StorageSource::Exact(zil_ir::StoragePosition {
                    base: zil_ir::StorageBase::Parameter(0),
                    path: vec![],
                })]
            }
            _ => {
                for interface in &mut attack.dyn_interfaces {
                    if interface.context.is_none() {
                        continue;
                    }
                    for slot in &mut interface.slots {
                        slot.contract
                            .effects
                            .as_mut()
                            .unwrap()
                            .retain(|effect| *effect != DynEffect::Panic);
                    }
                }
            }
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = attack.fingerprint();
        for wire in [false, true] {
            let errors = if wire {
                let encoded =
                    zil_ir::wire::encode_module_candidate(&attack, &candidate, &limits).unwrap();
                zil_verify::verify_module_bytes(&encoded, &target, &limits).unwrap_err()
            } else {
                zil_verify::verify_module(attack.clone(), candidate.clone(), &target, &limits)
                    .unwrap_err()
            };
            let code = if case < 3 {
                "E-VERIFY-DYN-SOURCE"
            } else {
                "E-VERIFY-DYN-BODY"
            };
            assert!(
                errors.iter().any(|d| d.code == code),
                "{case}, {wire}: {errors:?}"
            );
        }
    }
}

#[test]
fn same_leaf_static_body_and_parameter_body_have_different_actual_source_proofs() {
    let limits = Limits::default();
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/context-static-input.candidate.wire"),
        &limits,
    )
    .unwrap();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let parameter = module.functions[create].parameters[0];
    let ty = module.functions[create].locals[parameter.0 as usize]
        .ty
        .clone();
    let source = module.functions[create]
        .retained_return_sources
        .as_ref()
        .unwrap()[0]
        .sources
        .iter()
        .find(|atom| matches!(atom, StorageSource::Static { .. }))
        .unwrap()
        .clone();
    let StorageSource::Static { leaf, .. } = &source else {
        unreachable!()
    };
    assert_eq!(
        *leaf, ty,
        "the nearest counterexample keeps complete leaf and mode identical"
    );
    let values = module.functions[create].values.clone();
    for i in module.functions[create]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if matches!(i.kind, zil_ir::InstructionKind::StaticUtf8 { .. }) {
            assert_eq!(values[i.result.unwrap().0 as usize], ty);
            i.kind = zil_ir::InstructionKind::Load(parameter);
        }
    }
    let id = module.functions[create].id;
    // The only changed data origin in this raw program is this factory's
    // literal→parameter load. These remain untrusted candidate claims.
    for hold in &mut plan
        .functions
        .iter_mut()
        .find(|p| p.function == id)
        .unwrap()
        .holds
    {
        for root in &mut hold.roots {
            if *root == zil_ir::HoldRoot::Static {
                *root = zil_ir::HoldRoot::Parameter(0)
            }
        }
        hold.roots.sort_unstable();
        hold.roots.dedup();
    }
    module.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources = vec![source];
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    for wire in [false, true] {
        let errors = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap_err()
        };
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-DYN-SOURCE"),
            "{wire}: {errors:?}"
        );
    }
    // Exactly the same raw body, Type and header now declare its real input.
    module.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources = vec![StorageSource::Exact(zil_ir::StoragePosition {
        base: zil_ir::StorageBase::Parameter(0),
        path: vec![],
    })];
    plan.module_fingerprint = module.fingerprint();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}

#[test]
fn actual_no_input_static_factory_keeps_its_complete_original_invocation_class() {
    let limits = Limits::default();
    let bytes = include_bytes!("fixtures/dyn-context/context-static-no-input.candidate.wire");
    let (module, plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    assert!(module.functions[create].parameters.is_empty());
    let c = module.functions[create].storage_life.as_ref().unwrap();
    assert_eq!((c.fixed_binder_count, c.invocation_binder_count), (0, 1));
    assert_eq!(c.classes[0].binder, zil_ir::StorageBinder::Invocation(0));
    assert_eq!(
        c.classes[0].position.path,
        [zil_ir::LifeTypeProjection::Place(
            zil_ir::PlaceProjection::ErrorArm(false)
        )]
    );
    assert!(module.dyn_interfaces.iter().any(|i| i.context.is_some()
        && i.retained_life.binder_count == 1
        && !i.retained_life.nominal_arguments.is_empty()));
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap();
    for case in 0..5 {
        let mut attack = module.clone();
        match case {
            0 => attack.functions[create].retained_return_sources = None,
            1 => attack.functions[create].retained_return_sources = Some(vec![]),
            2 => attack.functions[create]
                .retained_return_sources
                .as_mut()
                .unwrap()[0]
                .sources
                .clear(),
            3 => {
                attack.functions[create]
                    .storage_life
                    .as_mut()
                    .unwrap()
                    .classes[0]
                    .position
                    .class = 1
            }
            _ => {
                let row = attack.functions[create]
                    .retained_return_sources
                    .as_mut()
                    .unwrap();
                let duplicate = row[0].sources[0].clone();
                row[0].sources.push(duplicate)
            }
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = attack.fingerprint();
        for wire in [false, true] {
            let errors = if wire {
                let bytes =
                    zil_ir::wire::encode_module_candidate(&attack, &candidate, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
            } else {
                zil_verify::verify_module(attack.clone(), candidate.clone(), &target, &limits)
                    .unwrap_err()
            };
            let code = if case == 3 {
                "E-VERIFY-SIGNATURE-CLASS"
            } else {
                "E-VERIFY-DYN-SOURCE"
            };
            assert!(
                errors.iter().any(|d| d.code == code),
                "{case}, {wire}: {errors:?}"
            );
        }
    }
}
