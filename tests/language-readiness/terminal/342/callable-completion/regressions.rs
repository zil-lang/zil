use super::*;
use zil_ir::*;
const CASES: [(&str, &str); 3] = [
    ("before", include_str!("loop-before.zil")),
    ("after", include_str!("loop-after.zil")),
    ("nested-transfer", include_str!("loop-transfer.zil")),
];
#[test]
fn g339_source_loop_origins_raw_wire() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (name, text) in CASES {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        eprintln!("G339 {name}: actual Source/raw/Wire success");
    }
    let report = compile_source(
        &source(include_str!("loop-double.zil")),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("generic loop must preserve"))
    );
}
