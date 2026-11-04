//! Test-only loading of structurally transcoded historical candidate records.
//! Reproduction: tools/migrate-impl253-wire37.py. Old v32/v34 bytes stay immutable.
//! Neither the old candidate plan nor this adapter grants proof authority.
pub fn migrate(bytes: &[u8], limits: &zil_base::Limits) -> Vec<u8> {
    assert_eq!(&bytes[..8], b"ZILMODP\0");
    assert_eq!(&bytes[8..12], &37u32.to_le_bytes());
    assert_eq!(zil_ir::wire::WIRE_VERSION, 37);
    assert_eq!(zil_base::SCHEMA_VERSION, 37);
    let (module, mut plan) = zil_ir::wire::decode_module_candidate(bytes, limits).unwrap();
    assert_eq!(module.schema_version, 37);
    assert_eq!(plan.schema_version, 37);
    assert_eq!(plan.target_fingerprint, module.target.fingerprint());
    for function in &module.functions {
        assert_eq!(function.call_kind, zil_ir::CallKind::Direct);
        if let Some(contract) = &function.callable_contract {
            assert!(contract.roles.iter().all(|role| role.life.is_none()));
        }
    }
    // Identity alone grants no authority. Callers independently Analyze and Verify
    // the actual module; historical cached claims are deliberately not reused.
    plan.module_fingerprint = module.fingerprint();
    zil_ir::wire::encode_module_candidate(&module, &plan, limits).unwrap()
}
