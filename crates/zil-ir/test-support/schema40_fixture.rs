//! Test-only loading of structurally transcoded historical candidate records.
//! Input40 reproduction: tools/migrate-impl253-wire40.py. Old v32/v34/v37/v40
//! bytes stay immutable. The40->41 change only adds opcode133; existing record
//! layouts are identical. This adapter decodes that restricted old structure,
//! updates both schema fields and module identity, then fully re-encodes it.
//! Neither the old candidate plan nor this adapter grants proof authority.
pub fn migrate(bytes: &[u8], limits: &zil_base::Limits) -> Vec<u8> {
    assert_eq!(&bytes[..8], b"ZILMODP\0");
    assert_eq!(&bytes[8..12], &40u32.to_le_bytes());
    assert_eq!(zil_ir::wire::WIRE_VERSION, 41);
    assert_eq!(zil_base::SCHEMA_VERSION, 41);
    // The production decoder still rejects every old transport envelope. Only
    // this temporary test-owned copy permits reading the unchanged40 layout.
    let errors = zil_ir::wire::decode_module_candidate(bytes, limits).unwrap_err();
    assert_eq!(errors[0].code, "E-WIRE-VERSION");
    let mut transport = bytes.to_vec();
    transport[8..12].copy_from_slice(&41u32.to_le_bytes());
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(&transport, limits).unwrap();
    assert_eq!(module.schema_version, 40);
    assert_eq!(plan.schema_version, 40);
    // Round-trip the entire old module and plan before transforming them;
    // neither plan offset nor its version is guessed from a byte search.
    assert_eq!(
        zil_ir::wire::encode_module_candidate(&module, &plan, limits).unwrap(),
        transport
    );
    assert_eq!(plan.target_fingerprint, module.target.fingerprint());
    assert!(module.life_declarations.is_empty());
    assert!(module.life_instances.is_empty());
    assert!(module.dyn_declarations.is_empty());
    assert!(module.dyn_interfaces.is_empty());
    assert!(module.dyn_vtables.is_empty());
    assert!(
        module
            .closures
            .iter()
            .all(|closure| closure.storage_life.is_none())
    );
    for function in &module.functions {
        assert!(function.storage_life.is_none());
        assert_eq!(function.call_kind, zil_ir::CallKind::Direct);
        if let Some(contract) = &function.callable_contract {
            assert!(contract.roles.iter().all(|role| role.life.is_none()));
        }
        // Opcode133 never existed in40. A relabelled current CAS candidate is
        // outside this frozen profile even though the current reader knows it.
        assert!(
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .all(|instruction| !matches!(
                    instruction.kind,
                    zil_ir::InstructionKind::SyncI32CompareExchange { .. }
                ))
        );
    }
    // Identity alone grants no authority. Callers independently Analyze and Verify
    // the actual module; historical cached claims are deliberately not reused.
    module.schema_version = 41;
    plan.schema_version = 41;
    plan.module_fingerprint = module.fingerprint();
    zil_ir::wire::encode_module_candidate(&module, &plan, limits).unwrap()
}
