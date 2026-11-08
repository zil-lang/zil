//! Every fixed address-array element retains its own actual external target.
use super::*;
const BATCH: &str = include_str!("fixed-shared-address-array.zil");
const HELPER: &str = include_str!("fixed-shared-address-array-helper.zil");

#[test]
fn source_raw_and_wire_preserve_each_array_element() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (name, program) in [("array-batch", BATCH), ("array-helper", HELPER)] {
        let report = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = typed_source(name, program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        println!("{name}: Source/raw/Wire actual element proof passed");
    }
}
