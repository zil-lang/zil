//! Independent Verify control for a returned parent environment. The checked
//! candidate is frozen; this test never asks Analyze for proof.
#[path = "../../zil-ir/test-support/schema40_fixture.rs"]
mod schema40_fixture;

use zil_base::{Limits, TargetProfile};

#[test]
fn returned_parent_capture_old_position_window_is_checked_with_or_without_a_consumer() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for (name, frozen) in [
        (
            "no_call_edge",
            include_bytes!("../../../tests/borrowed-callable-arguments/v40/parent_valid_no_call_edge.wire").as_slice(),
        ),
        (
            "with_call_edge",
            include_bytes!("../../../tests/borrowed-callable-arguments/v40/parent_valid_with_call_edge.wire")
                .as_slice(),
        ),
    ] {
        let migrated = schema40_fixture::migrate(frozen, &limits);
        let (valid, plan) = zil_ir::wire::decode_module_candidate(&migrated, &limits).unwrap();
        for stale in [false, true] {
            let mut typed = valid.clone();
            let mut candidate = plan.clone();
            if stale {
                // BorrowCapture -> TakeCapture -> use of the old address.
                // The parent body and contract stay identical; only the
                // actual instruction order changes.
                typed.functions[2].blocks[0].instructions.swap(3, 4);
                candidate.module_fingerprint = typed.fingerprint();
            }
            let raw = zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits);
            let wire = zil_ir::wire::encode_module_candidate(&typed, &candidate, &limits).unwrap();
            let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits);
            for (label, result) in [("raw", raw), ("wire", decoded)] {
                if stale {
                    let errors = result.unwrap_err();
                    assert_eq!(
                        errors[0].code, "E-BORROW-CONFLICT",
                        "{name} {label}: {errors:?}"
                    );
                } else {
                    result.unwrap_or_else(|errors| panic!("{name} {label}: {errors:?}"));
                }
            }
        }
    }
}
