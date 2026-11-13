//! Complete original Source proposals for independent downstream consumers.
use std::{collections::BTreeMap, fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};

#[test]
fn original_fixed_getter_and_context_store_publish_complete_untrusted_types() {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("source-formal-candidates-{}", std::process::id()));
    let mut failures = Vec::new();
    for (label, text) in [
        (
            "long-slice-getter",
            include_str!("../../../tests/fixtures/source-formal/long-slice-getter.zil"),
        ),
        (
            "store-owner",
            include_str!("../../../tests/fixtures/source-formal/store-owner.zil"),
        ),
    ] {
        let directory = root.join(label);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("main.zil"), text).unwrap();
        let source = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let expanded = zil_expand::expand_with_map_limits_target_context(
            &source,
            &parsed,
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap();
        fs::write(directory.join("core.txt"), format!("{:#?}", expanded.core)).unwrap();
        match zil_types::check(&expanded.core, &target, &limits) {
            Ok(module) => {
                fs::write(directory.join("typed.txt"), format!("{module:#?}")).unwrap();
                let empty = zil_ir::CandidatePlan::empty_for(&module);
                let wire = zil_ir::wire::encode_module_candidate(&module, &empty, &limits).unwrap();
                fs::write(directory.join("typed-unverified-empty-plan.wire"), &wire).unwrap();
                let (decoded, plan) =
                    zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
                assert_eq!(decoded, module);
                assert_eq!(plan, empty);
                eprintln!(
                    "{label}: Source/Types success; untrusted input {}",
                    directory.display()
                );
            }
            Err(errors) => {
                fs::write(directory.join("types-errors.txt"), format!("{errors:#?}")).unwrap();
                eprintln!("{label}: {errors:?}; actual Core {}", directory.display());
                failures.push(label);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "actual Types consumers remain: {failures:?}; {}",
        root.display()
    );
}
