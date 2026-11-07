//! Source negatives never reach native. A missing positive F-return gate is not
//! evidence that a particular invalid source was semantically rejected.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

fn expected(
    name: &str,
) -> (
    ResultCategory,
    Phase,
    &'static str,
    &'static [&'static str],
    &'static str,
) {
    match name {
        "external_address_environment" | "external_address_child_environment" => (
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
            &[
                "self-contained environments",
                "borrowed dependencies cannot escape",
            ],
            "Direct and child F environments must be recursively self-contained; this is a closed profile boundary, not proof of a loan conflict.",
        ),
        "once_called_then_returned" | "once_mixed_return_path" => (
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
            &[
                "used after move",
                "Copy cannot be inferred from a concrete instance",
            ],
            "A reachable FnOnce call consumes the declared input position, which cannot then be returned as live F.",
        ),
        "once_declared_actual_fn_copy" => (
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
            &[
                "used after move",
                "Copy cannot be inferred from a concrete instance",
            ],
            "An outer symbolic FnOnce position does not gain Copy from its concrete Fn instantiation after keep.",
        ),
        "once_declared_actual_fn_twice" | "once_mode_through_loop" => (
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
            &["callable used after its environment was consumed"],
            "Direct and loop forwarding preserve the caller's declared FnOnce mode, so a second call is forbidden.",
        ),
        "same_closure_id_different_instance" | "same_closure_id_wrong_branch" => (
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
            &[
                "return the actual input instance",
                "not a fresh factory environment",
            ],
            "Every normal return must originate in the actual F argument; another instance of the same factory/ClosureId is insufficient.",
        ),
        "use_after_transfer" => (
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-UNINITIALIZED",
            &["not initialized on every incoming path"],
            "The original non-Copy owning closure position is moved into keep and cannot be called again.",
        ),
        "address_active_window" => (
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
            &[
                "overlaps an active address window",
                "without its delegated authority",
            ],
            "Returning F does not erase the caller's live shared view when a later explicit mutable address call accesses the same scalar.",
        ),
        _ => panic!("unregistered current negative source {name}"),
    }
}

#[test]
fn nearest_source_rejections_require_the_actual_mode_origin_or_window_reason() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/callable-instance-forwarding");
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
            if name == "mut_declared_actual_fn_shared_call" {
                fs::write(dir.join("out-of-profile.json"), format!("{{\"classification\":\"historical B0 gap; not a current semantic rejection Pass\",\"original_expected\":\"SafeReject\",\"actual\":\"{:?}\",\"source_and_original_expectation_preserved\":true,\"root_registrations\":[\"5b6b609e5\",\"b113\"]}}\n",report.status)).unwrap();
                if report.status == ResultCategory::Success {
                    failed.push(format!(
                        "historical invalid source unexpectedly accepted: {}",
                        report.to_json()
                    ));
                }
                continue;
            }
            let (category, phase, code, fragments, obligation) = expected(name);
            fs::write(dir.join("expected-diagnostic.json"),format!("{{\"category\":\"{category:?}\",\"phase\":\"{}\",\"code\":\"{code}\",\"message_fragments\":{fragments:?},\"source_obligation\":\"{obligation}\"}}\n",format!("{phase:?}").to_lowercase())).unwrap();
            let accepted_rejection = report.status == category
                && report.diagnostics.iter().any(|diagnostic| {
                    diagnostic.category == category
                        && diagnostic.phase == phase
                        && diagnostic.code == code
                        && fragments
                            .iter()
                            .all(|fragment| diagnostic.message.contains(fragment))
                });
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
