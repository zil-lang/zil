//! Primitive declaration erasure and ordinary lexical bindings use their real stages.
use std::collections::BTreeMap;
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, ProjectSources, compile_project};

#[test]
fn store_at_local_and_parameter_bindings_are_rejected_in_types() {
    for source in [
        "fn main(){let store_at=1;}",
        "fn main(){var store_at=1;}",
        "fn receive(store_at:i32)->i32{return store_at} fn main(){}",
    ] {
        let report = compile_project(
            &ProjectSources {
                root: ModuleId(0),
                modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
                paths: BTreeMap::new(),
            },
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == Phase::Types && d.code == "E-OWNED-STORE-NAME"),
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}
