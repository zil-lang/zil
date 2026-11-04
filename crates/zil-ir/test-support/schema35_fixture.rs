//! Test-only transport migration for unchanged pre-35 records. Never a production reader.
//! All old bytes remain immutable and must fail the current transport version check.
pub fn migrate(bytes: &[u8], limits: &zil_base::Limits) -> Vec<u8> {
    assert_eq!(&bytes[..8], b"ZILMODP\0");
    assert_eq!(&bytes[8..12], &34u32.to_le_bytes());
    assert_eq!(zil_ir::wire::WIRE_VERSION, 35);
    assert_eq!(zil_base::SCHEMA_VERSION, 35);
    let errors = zil_ir::wire::decode_module_candidate(bytes, limits).unwrap_err();
    assert_eq!(errors[0].code, "E-WIRE-VERSION");
    // Version 35 appends variants; these fixed fixtures contain only existing tags.
    // Only the test decoder envelope changes before raw structural decoding.
    let mut envelope = bytes.to_vec();
    envelope[8..12].copy_from_slice(&35u32.to_le_bytes());
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(&envelope, limits).unwrap();
    assert_eq!(module.schema_version, 34);
    assert_eq!(plan.schema_version, 34);
    assert_eq!(plan.target_fingerprint, module.target.fingerprint());
    module.schema_version = 35;
    plan.schema_version = 35;
    plan.module_fingerprint = module.fingerprint();
    // This remains an untrusted candidate: callers rerun independent safety kernels.
    zil_ir::wire::encode_module_candidate(&module, &plan, limits).unwrap()
}
