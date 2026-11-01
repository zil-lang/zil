#[test]
fn generate_schema30_envelope() {
    let limits=zil_base::Limits::default();
    let original=include_bytes!("../../zil-driver/tests/review223_fixtures/mutable-product-original.wire");
    let mut header=original.to_vec();
    header[8..12].copy_from_slice(&30u32.to_le_bytes());
    let (mut module,mut plan)=zil_ir::wire::decode_module_candidate(&header,&limits).unwrap();
    assert_eq!(module.schema_version,27);assert_eq!(plan.schema_version,27);
    assert_eq!(plan.target_fingerprint,module.target.fingerprint());
    module.schema_version=30;plan.schema_version=30;plan.module_fingerprint=module.fingerprint();
    let migrated=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
    let mut expected=original.to_vec();
    for offset in [8,12,1538] {expected[offset..offset+4].copy_from_slice(&30u32.to_le_bytes());}
    expected[1542..1574].copy_from_slice(&plan.module_fingerprint.0);
    assert_eq!(migrated,expected);
    std::fs::write("../zil-driver/tests/review223_fixtures/schema30-mutable-product.wire",migrated).unwrap();
}
