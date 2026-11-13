//! Actual Source returns a view of first.left. Region uppers keep complete
//! actual physical windows; equal Pair Types do not select the same instance.
use zil_base::Limits;
use zil_ir::{ContractProjection, PlaceProjection, StorageBase, StorageSource};
#[test]
fn actual_nested_owned_region_source_checks_each_complete_head_and_field_window() {
    let limits = Limits::default();
    let bytes = include_bytes!("fixtures/dyn-context/context-owned-region.candidate.wire");
    let (module, plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let sources = &module.functions[create]
        .retained_return_sources
        .as_ref()
        .unwrap()[0]
        .sources;
    assert_eq!(sources.len(), 2);
    for (ordinal, atom) in sources.iter().enumerate() {
        let StorageSource::OwnedView { head, .. } = atom else {
            panic!("{atom:?}")
        };
        assert_eq!(head.base, StorageBase::Parameter(0));
        assert_eq!(
            head.path,
            [ContractProjection::Place(PlaceProjection::Field(
                ordinal as u32
            ))]
        );
    }
    // Both heads are real Pair instances. Keeping only first remains a valid
    // conservative upper containing its two i32 windows.
    let mut first = module.clone();
    first.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources
        .truncate(1);
    let mut first_plan = plan.clone();
    first_plan.module_fingerprint = first.fingerprint();
    zil_verify::verify_module(first.clone(), first_plan.clone(), &target, &limits).unwrap();
    let encoded = zil_ir::wire::encode_module_candidate(&first, &first_plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&encoded, &target, &limits).unwrap();
    for case in 0..5 {
        let mut attack = first.clone();
        let sources = &mut attack.functions[create]
            .retained_return_sources
            .as_mut()
            .unwrap()[0]
            .sources;
        match case {
            0 => {
                let StorageSource::OwnedView { head, .. } = &mut sources[0] else {
                    unreachable!()
                };
                head.path = vec![ContractProjection::Place(PlaceProjection::Field(1))]
            }
            1 => {
                let StorageSource::OwnedView { leaf, .. } = &mut sources[0] else {
                    unreachable!()
                };
                *leaf = zil_ir::Type::Address {
                    mutable: false,
                    inner: Box::new(zil_ir::Type::I64),
                }
            }
            2 => {
                let StorageSource::OwnedView { head, .. } = &sources[0] else {
                    unreachable!()
                };
                sources[0] = StorageSource::Exact(head.clone())
            }
            3 => sources.clear(),
            _ => sources.push(sources[0].clone()),
        }
        let mut candidate = first_plan.clone();
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
            assert!(
                errors.iter().any(|d| d.code == "E-VERIFY-DYN-SOURCE"),
                "{case}, {wire}: {errors:?}"
            );
        }
    }
}
