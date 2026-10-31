//! Exact reviewer inputs for ordinary and generated private reserved functions.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};

#[test]
fn generated_private_functions_preserve_the_ordinary_reserved_name_boundary() {
    let mut reports = Vec::new();
    for (label, text) in [
        (
            "plain",
            include_str!(
                "../../../examples/mainline/generated_events228/review/reserved_function_plain.zil"
            ),
        ),
        (
            "pure",
            include_str!(
                "../../../examples/mainline/generated_events228/review/reserved_function_pure.zil"
            ),
        ),
        (
            "mixed",
            include_str!(
                "../../../examples/mainline/generated_events228/review/reserved_function_mixed.zil"
            ),
        ),
    ] {
        let report_dir = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let out = report_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("reserved228-{label}-{}", std::process::id()));
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("source.zil"), text).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "reserved_function.zil", text),
            &CompileOptions {
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("compile.json"), report.to_json()).unwrap();
        reports.push((label, report));
        if report_dir.is_none() {
            fs::remove_dir_all(out).unwrap();
        }
    }
    let accepted: Vec<_> = reports
        .iter()
        .filter(|(_, report)| report.status == ResultCategory::Success)
        .map(|(label, _)| *label)
        .collect();
    assert!(
        accepted.is_empty(),
        "reserved functions accepted: {accepted:?}"
    );
    for (label, report) in reports {
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert_eq!(report.diagnostics[0].code, "E-BYTE-BUFFER-NAME");
        assert!(!report.artifacts.contains_key("executable"));
    }
}
