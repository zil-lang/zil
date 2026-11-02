//! Source negatives never reach native. A missing positive F-return gate is not
//! evidence that a particular invalid source was semantically rejected.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

#[test]
fn nearest_source_rejections_require_the_actual_mode_origin_or_window_reason() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/impl256");
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("consumer256-negatives-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    let mut failed = Vec::new();
    for directory in [root.join("frozen-prep/negative"), root.join("negative")] {
        let mut paths = fs::read_dir(directory)
            .unwrap()
            .map(|p| p.unwrap().path())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            let name = path.file_stem().unwrap().to_str().unwrap();
            let source = fs::read_to_string(&path).unwrap();
            let dir = out.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("source.zil"), &source).unwrap();
            let report = compile_source(
                &SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    path.to_string_lossy().into_owned(),
                    source,
                ),
                &CompileOptions {
                    stop_after: Stage::Verify,
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert!(!report.artifacts.contains_key("executable"));
            let environment = name.starts_with("external_address");
            let accepted_rejection = if environment {
                report.status == ResultCategory::Unsupported
                    && report
                        .diagnostics
                        .iter()
                        .any(|d| d.message.to_lowercase().contains("self-contained"))
            } else {
                report.status == ResultCategory::SafeReject
            };
            if !accepted_rejection {
                failed.push(format!("{name}: {}", report.to_json()));
            }
        }
    }
    assert!(
        failed.is_empty(),
        "{} source negatives have not reached their required semantic check:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
