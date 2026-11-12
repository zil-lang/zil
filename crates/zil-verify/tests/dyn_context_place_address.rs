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
