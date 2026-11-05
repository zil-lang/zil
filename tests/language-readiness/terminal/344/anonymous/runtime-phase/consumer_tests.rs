use super::*;

fn runtime_source_project_native(text: &str, expected: &[u8]) {
    let input = source(text);
    let project = zil_driver::ProjectSources {
        root: input.module_id,
        paths: std::collections::BTreeMap::from([(vec!["main".into()], input.module_id)]),
        modules: vec![input.clone()],
    };
    let options = CompileOptions::default();
    let mut session = zil_driver::CompilationSession::default();
    for _ in 0..2 {
        let report = session.compile_source(&input, &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let report = zil_driver::compile_project(&project, &options);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    review329_native_entry_output(&[("main", text)], "main", expected);
}

#[test]
fn g447_runtime_ct_mixed_life_source_project_native() {
    runtime_source_project_native(include_str!("../ct5-mixed-runtime-instance.zil"), b"42\n");
}

#[test]
fn g447_runtime_ct_multiple_instances_source_project_native() {
    runtime_source_project_native(
        include_str!("../ct6-runtime-multiple-instances.zil"),
        b"16\n",
    );
}

#[test]
fn g447_runtime_ct_const_without_life_source_project_native() {
    runtime_source_project_native(
        include_str!("../ct6-runtime-const-without-life.zil"),
        b"8\n",
    );
}

#[test]
fn g447_runtime_ct_checks_unused_definitions() {
    let capture = include_str!("../ct6-runtime-unused-ct-capture-reject.zil");
    let report = compile_source(&source(capture), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-CT-SOURCE" || d.code == "E-CT-RUNTIME"),
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);

    // CT result typing does not excuse the surrounding parametric body.
    let ordinary = "fn unused<T:Copy,#N:i32>(marker:T)->T{let n=#(N+1);return marker+marker}fn main()->i32{return 0}";
    let report = compile_source(&source(ordinary), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert_eq!(report.verification_runs, 0);
}
