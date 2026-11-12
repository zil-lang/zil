//! A real mutable Holder's shared leaf can be copied independently. An explicit
//! bounded reborrow additionally retains its exact head, without retagging data.
use zil_base::Limits;
use zil_ir::{ContractProjection, PlaceProjection, StorageBase, StorageSource};
#[test]
fn actual_holder_leaf_factory_preserves_data_class_and_optional_bounded_head() {
    let limits = Limits::default();
    let bytes = include_bytes!("fixtures/dyn-context/context-holder-leaf.candidate.wire");
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let StorageSource::Exact(source) = module.functions[create]
        .retained_return_sources
        .as_ref()
        .unwrap()[0]
        .sources[0]
        .clone()
    else {
        panic!("actual Holder source must select its stored leaf")
    };
    assert_eq!(source.base, StorageBase::Parameter(0));
    assert_eq!(
        source.path,
        [ContractProjection::Place(PlaceProjection::Field(0))]
    );
    let original_life = module.functions[create].storage_life.clone();
    module.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources[0] = StorageSource::ReborrowedLeaf {
        source,
        head: zil_ir::StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        },
    };
    plan.module_fingerprint = module.fingerprint();
    // This is an explicit conservative raw operation over the same actual
    // Source body, data permission and signature, not a producer success claim.
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    assert_eq!(module.functions[create].storage_life, original_life);
}
