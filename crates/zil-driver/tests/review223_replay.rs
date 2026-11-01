//! Exact original223 attack bytes, plus an explicitly migrated schema29 envelope.
use std::{fs, path::PathBuf};
use zil_base::{Fingerprint, Limits, Phase, ResultCategory, TargetProfile};

const ORIGINAL: &[u8] = include_bytes!("review223_fixtures/mutable-product-original.wire");
const SCHEMA29: &[u8] = include_bytes!("review223_fixtures/mutable-product-schema29.wire");

#[test]
fn frozen_schema27_wire_is_rejected_as_an_old_transport() {
    assert_eq!(
        Fingerprint::of(ORIGINAL).to_hex(),
        "5ba4b35d45093a324e31ab8971d92788a1e9c18b57cbdc17fdd317c7d2f59de9"
    );
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for errors in [
        zil_ir::wire::decode_module_candidate(ORIGINAL, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(ORIGINAL, &target, &limits).unwrap_err(),
    ] {
        assert_eq!(errors.len(), 1, "{errors:#?}");
        let error = &errors[0];
        assert_eq!(error.phase, Phase::Verify);
        assert_eq!(error.category, ResultCategory::SafeReject);
        assert_eq!(error.code, "E-WIRE-VERSION");
    }
}

// This is a fixture migration, not a production backward-compatible decoder.
// The original TypedModule payload uses the same field layout in27 and29.
fn migrate_original_envelope(limits: &Limits) -> Vec<u8> {
    assert_eq!(&ORIGINAL[..8], b"ZILMODP\0");
    assert_eq!(&ORIGINAL[8..12], &27u32.to_le_bytes());
    assert_eq!(zil_ir::wire::WIRE_VERSION, 29);
    assert_eq!(zil_base::SCHEMA_VERSION, 29);
    let mut transport = ORIGINAL.to_vec();
    transport[8..12].copy_from_slice(&29u32.to_le_bytes());
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(&transport, limits).unwrap();
    assert_eq!(module.schema_version, 27);
    assert_eq!(plan.schema_version, 27);
    assert_eq!(plan.target_fingerprint, module.target.fingerprint());
    module.schema_version = 29;
    plan.schema_version = 29;
    plan.module_fingerprint = module.fingerprint();
    let migrated = zil_ir::wire::encode_module_candidate(&module, &plan, limits).unwrap();
    // Fixed offsets are audited against the original1744-byte envelope.
    // Every attack byte outside these version/derived-fingerprint fields stays exact.
    let mut expected = ORIGINAL.to_vec();
    for offset in [8, 12, 1538] {
        assert_eq!(&ORIGINAL[offset..offset + 4], &27u32.to_le_bytes());
        expected[offset..offset + 4].copy_from_slice(&29u32.to_le_bytes());
    }
    expected[1542..1574].copy_from_slice(&plan.module_fingerprint.0);
    assert_eq!(migrated, expected);
    migrated
}

#[test]
fn frozen_mutable_product_raw_and_wire_must_be_rejected() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    assert_eq!(
        Fingerprint::of(SCHEMA29).to_hex(),
        "3d5bf5e48a6a0856d3fad6c8b7997e8ba5ccb7adfbb0327a2e7e0852d4da4198"
    );
    assert_eq!(migrate_original_envelope(&limits), SCHEMA29);
    let (module, plan) = zil_ir::wire::decode_module_candidate(SCHEMA29, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap(),
        SCHEMA29
    );
    assert_eq!(module.fingerprint(), plan.module_fingerprint);
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("frozen-mutable-replay");
    fs::create_dir_all(&root).unwrap();
    let results = [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(SCHEMA29, &target, &limits),
        ),
    ];
    for (route, result) in &results {
        fs::write(root.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
    }
    for (route, result) in results {
        let errors = result.expect_err(route);
        assert_eq!(errors.len(), 1, "{route}: {errors:#?}");
        assert_eq!(errors[0].category, ResultCategory::Unsupported, "{route}");
        assert_eq!(errors[0].code, "E-UNSUPPORTED", "{route}");
        assert_eq!(
            errors[0].message, "ordinary Product addresses are shared synchronous inputs",
            "{route}"
        );
        assert!(
            errors.iter().all(|d| d.phase == Phase::Verify
                && matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                )),
            "{route}: {errors:#?}"
        );
        assert!(
            errors
                .iter()
                .all(|d| !d.code.starts_with("E-WIRE-") && !d.message.contains("fingerprint")),
            "{route}: {errors:#?}"
        );
    }
}
