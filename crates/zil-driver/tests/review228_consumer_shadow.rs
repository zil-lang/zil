//! Independent replay of an unused generic body shadowing a nominal type.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};

fn rejects_nominal_field_capture(label: &str, text: &str) {
    let report_dir = std::env::var_os("ZIL_TEST_REPORT_DIR");
    let out = report_dir
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("shadow229-{label}-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("source.zil"), text).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "nominal_shadow.zil", text),
        &CompileOptions {
            emit_dir: Some(out.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("executable"));
    if report_dir.is_none() {
        fs::remove_dir_all(out).unwrap();
    }
}

#[test]
fn trailing_copy_bound_does_not_capture_a_same_named_struct() {
    rejects_nominal_field_capture(
        "where",
        include_str!("../../../examples/mainline/generated_events228/review/shadow_where.zil"),
    );
}

#[test]
fn inline_copy_bound_does_not_capture_a_same_named_struct() {
    rejects_nominal_field_capture(
        "inline",
        include_str!("../../../examples/mainline/generated_events228/review/shadow_inline.zil"),
    );
}

#[test]
fn unbounded_generic_does_not_capture_a_same_named_struct() {
    rejects_nominal_field_capture(
        "unbounded",
        include_str!("../../../examples/mainline/generated_events228/review/shadow_unbounded.zil"),
    );
}
