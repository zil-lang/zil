//! Independent aliases in constructor patterns after generated type hygiene.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, compile_source};

#[test]
fn generated_alias_constructor_patterns_match_the_ordinary_source() {
    for (label, text) in [
        (
            "plain",
            include_str!(
                "../../../examples/mainline/generated_events228/review/enum_alias_plain.zil"
            ),
        ),
        (
            "generated",
            include_str!(
                "../../../examples/mainline/generated_events228/review/enum_alias_generated.zil"
            ),
        ),
    ] {
        let report_dir = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let out = report_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("pattern228-{label}-{}", std::process::id()));
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("source.zil"), text).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "alias_pattern.zil", text),
            &CompileOptions {
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        if report_dir.is_none() {
            fs::remove_dir_all(out).unwrap();
        }
    }
}
