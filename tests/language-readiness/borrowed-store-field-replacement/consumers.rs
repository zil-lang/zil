//! Replace a real managed Store field through its exclusive Package address.
use super::*;

const REFILL: &str = include_str!("refill.zil");

#[test]
fn borrowed_generic_store_field_refill_reaches_independent_verify() {
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "borrowed-store-refill.zil", REFILL),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    println!("borrowed Store refill: {}", report.to_json());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
