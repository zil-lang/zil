//! Independent replay of an unused generic body shadowing a nominal type.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};

fn rejects_nominal_field_capture(label: &str, text: &str, control: &str) {
    assert_eq!(text.replacen("struct T", "struct Nominal", 1), control);
    let mut reports = Vec::new();
    for (kind, source) in [("no-collision-control", control), ("shadow", text)] {
        let report_dir = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let out = report_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("shadow229-{label}-{kind}-{}", std::process::id()));
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("source.zil"), source).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "nominal_shadow.zil", source),
            &CompileOptions {
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("compile.json"), report.to_json()).unwrap();
        // Neither Copy nor an unbounded type parameter exposes nominal fields.
        // Compare the exact established unsupported-operation category with an
        // ordinary no-collision definition, not merely any non-success result.
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{label}/{kind}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert_eq!(report.diagnostics[0].code, "E-UNSUPPORTED");
        assert_eq!(
            report.diagnostics[0].message,
            "operation on an unconstrained type parameter requires a declared bound"
        );
        assert!(!report.artifacts.contains_key("executable"));
        reports.push(report);
        if report_dir.is_none() {
            fs::remove_dir_all(out).unwrap();
        }
    }
    assert_eq!(
        reports[0].diagnostics[0].code,
        reports[1].diagnostics[0].code
    );
    assert_eq!(
        reports[0].diagnostics[0].message,
        reports[1].diagnostics[0].message
    );
}

#[test]
fn trailing_copy_bound_does_not_capture_a_same_named_struct() {
    rejects_nominal_field_capture(
        "where",
        include_str!("../../../examples/mainline/generated_events228/review/shadow_where.zil"),
        include_str!(
            "../../../examples/mainline/generated_events228/review/shadow_where_control.zil"
        ),
    );
}

#[test]
fn inline_copy_bound_does_not_capture_a_same_named_struct() {
    rejects_nominal_field_capture(
        "inline",
        include_str!("../../../examples/mainline/generated_events228/review/shadow_inline.zil"),
        include_str!(
            "../../../examples/mainline/generated_events228/review/shadow_inline_control.zil"
        ),
    );
}

#[test]
fn unbounded_generic_does_not_capture_a_same_named_struct() {
    rejects_nominal_field_capture(
        "unbounded",
        include_str!("../../../examples/mainline/generated_events228/review/shadow_unbounded.zil"),
        include_str!(
            "../../../examples/mainline/generated_events228/review/shadow_unbounded_control.zil"
        ),
    );
}
