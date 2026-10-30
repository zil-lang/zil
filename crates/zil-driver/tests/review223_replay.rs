//! Exact bytes of the independently discovered 223 mutable authority leak.
use std::{fs, path::PathBuf};
use zil_base::{Limits, Phase, ResultCategory, TargetProfile};

#[test]
fn frozen_mutable_product_raw_and_wire_must_be_rejected() {
    let wire = include_bytes!("review223_fixtures/mutable-product-original.wire");
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let (module, plan) = zil_ir::wire::decode_module_candidate(wire, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap(),
        wire.as_slice()
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
            zil_verify::verify_module_bytes(wire, &target, &limits),
        ),
    ];
    for (route, result) in &results {
        fs::write(root.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
    }
    for (route, result) in results {
        let errors = result.expect_err(route);
        assert!(
            errors.iter().all(|d| d.phase == Phase::Verify
                && matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                )),
            "{route}: {errors:#?}"
        );
        assert!(
            errors.iter().all(|d| !d.message.contains("fingerprint")),
            "{route}: {errors:#?}"
        );
    }
}
