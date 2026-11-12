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
