//! Test-only loading of structurally transcoded historical candidate records.
//! Frozen Input40 source lineage: 28b3e37bb4d8a1fe8b146c72cf891fc99de32440:tools/migrate-impl253-wire40.py. Old v32/v34/v37/v40
//! bytes stay immutable. This frozen v40 subset lacks the V44
//! function.protocol_contract field; the restricted walker inserts None at parsed
//! function boundaries. New TaskCreate port bindings never occur in this subset.
//! This adapter decodes the restricted frozen old structure,
//! updates both schema fields and module identity, then fully re-encodes it.
//! Neither the old candidate plan nor this adapter grants proof authority.
#[path = "historical_function_layout.rs"]
mod historical;
pub fn migrate(bytes: &[u8], limits: &zil_base::Limits) -> Vec<u8> {
    // This is a migration of these17 immutable fixtures, not a general old
    // grammar decoder. In particular, a relabelled new Type37 cannot enter.
    const FROZEN_SHA256: &[&str] = &[
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/control_concrete_copy_byvalue_call.wire
        "4c4b440bfb693fbe1a9b544302c7fea6f2bc12ae5043a470de4312379d6f366e",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/control_shared_callback_i32_argument.wire
        "e544bdc6b175229704572fe18b0b318b57cb9359e854cf67f12c1413fdb5b516",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_byvalue_symbolic_fn_call.wire
        "31c462fbfdc3ec3f9efbb2da9c4b31fdcbcbd92675915404548432a5f121754d",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_callback_argument.wire
        "e3d61e6f1e8e2a2324383753077d9e0449600dfed08f417753122fe72f981d3b",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_shared_callback_f_argument.wire
        "612afe15ff2d13b52f76d4a0afbe6ec2c45cd16c43222c4f74f7d80e1d389e97",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_thin_store.wire
        "81a97f6a646aa09f8c1590794a7697a731f48018c1cc98af7fd376872022543c",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/escape_two_element_array.wire
        "1df37d660f6175d8da818a303b0828cb5d1ce29bdce2f71d717576d00db844ab",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/parent_stale_no_call_edge.wire
        "f56c04244a28ac12d9469b9b0fc2103cb6aac50effd88f512e4459dc3481c094",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/parent_stale_with_call_edge.wire
        "e41340a11fcf1b119a5af0d587b5b16b26895705e530a9e5f985947822e576aa",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/parent_valid_no_call_edge.wire
        "f193b557e3d4c2660f9a79e0a97671319ca30d0b694a28f0c3cab8dd8d6f745e",
        // crates/zil-analyze/tests/forward_call_wire_fixtures/v40/parent_valid_with_call_edge.wire
        "a20e2cb8c8750ee488b6d30a8350a4ce13ee2d629236abf6813283073bc46045",
        // crates/zil-driver/tests/product_store_wire_replay_fixtures/v40/mutable-product.wire
        "d3c496c8fc68d4135731791e7fbb052eb977475433c4646e3692f2deb1c0e14f",
        // tests/borrowed-callable-arguments/v40/control_callback_i32_argument.wire
        "c4f59db2ce30691888fa548b6100b608d82904c3809bc7469c5d7bb0ade9443d",
        // tests/borrowed-callable-arguments/v40/escape_callback_argument.wire
        "9504658bd740ec175e6e53320298d114c4cc5bde4ae493a1efb8afeefc148c19",
        // tests/borrowed-callable-arguments/v40/loan_revival.wire
        "00c022e566a4b855f5a3c97ae4eea0a216ee0f8dd05c01730a52f8b2d5561c7f",
        // tests/borrowed-callable-arguments/v40/parent_valid_no_call_edge.wire
        "ceaee43c3f99be1a17967fc54093d1a10bf48590adc7af18b09f52a1b5c217ff",
        // tests/borrowed-callable-arguments/v40/parent_valid_with_call_edge.wire
        "c35f9e2a36451b7ad32eb4873236fc0ad7f05a7501daf00566049e14c90354c5",
    ];
    assert!(FROZEN_SHA256.contains(&zil_base::Fingerprint::of(bytes).to_hex().as_str()));
    assert_eq!(&bytes[..8], b"ZILMODP\0");
    assert_eq!(&bytes[8..12], &40u32.to_le_bytes());
    const CURRENT_LAYOUT: u32 = 46;
    assert_eq!(zil_ir::wire::WIRE_VERSION, CURRENT_LAYOUT);
    assert_eq!(zil_base::SCHEMA_VERSION, CURRENT_LAYOUT);
    // The production decoder still rejects every old transport envelope. Only
    // this temporary test-owned copy structurally migrates the frozen40 layout.
    let errors = zil_ir::wire::decode_module_candidate(bytes, limits).unwrap_err();
    assert_eq!(errors[0].code, "E-WIRE-VERSION");
    let transport = historical::transport(
        bytes,
        limits.max_source_bytes,
        limits.max_nodes,
        limits.max_call_depth,
        limits.max_analysis_steps,
    )
    .expect("restricted frozen40 structural migration");
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
        assert!(!function.foreign.as_ref().is_some_and(|foreign| matches!(
            foreign.scoped_callback,
            Some(
                zil_ir::ScopedCallbackRole::AsyncRegister
                    | zil_ir::ScopedCallbackRole::AsyncRequestClose
                    | zil_ir::ScopedCallbackRole::AsyncPollDrain
            )
        )));
        if let Some(contract) = &function.callable_contract {
            assert!(contract.roles.iter().all(|role| role.life.is_none()));
        }
        // New operations never existed in40. Relabelling a current CAS/async
        // candidate does not place it in this frozen historical profile.
        assert!(
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .all(|instruction| !matches!(
                    instruction.kind,
                    zil_ir::InstructionKind::SyncI32CompareExchange { .. }
                        | zil_ir::InstructionKind::AsyncRegistrationStart { .. }
                        | zil_ir::InstructionKind::AsyncRegistrationValid { .. }
                        | zil_ir::InstructionKind::AsyncRegistrationRequestClose { .. }
                        | zil_ir::InstructionKind::AsyncRegistrationPollClose { .. }
                        | zil_ir::InstructionKind::AsyncRegistrationFinishDestroy { .. }
                ))
        );
    }
    // Identity alone grants no authority. Callers independently Analyze and Verify
    // the actual module; historical cached claims are deliberately not reused.
    module.schema_version = CURRENT_LAYOUT;
    plan.schema_version = CURRENT_LAYOUT;
    plan.module_fingerprint = module.fingerprint();
    zil_ir::wire::encode_module_candidate(&module, &plan, limits).unwrap()
}
