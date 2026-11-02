//! Frozen and independent source inputs through the Verify boundary.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};

fn checked(name: &str, text: &str) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed, plan, &target, &limits)
        .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
}

#[test]
fn original_source_bytes_reach_verify() {
    checked("original", include_str!("impl252_fixtures/original.zil"));
}

#[test]
fn fnmut_and_loop_forwarding_reach_verify() {
    checked("fnmut", include_str!("../../../tests/impl254/fnmut.zil"));
    checked(
        "branch_loop",
        include_str!("../../../tests/impl254/branch_loop.zil"),
    );
}

#[test]
fn complete_error_union_store_companion_reaches_verify() {
    checked(
        "owned_companion",
        include_str!("../../../tests/impl254/owned_companion.zil"),
    );
}

#[test]
fn frozen_six_copy_and_owned_signatures_reach_verify() {
    for (name, source) in [
        (
            "fn-copy",
            include_str!("../../../tests/impl256/frozen-prep/probes/fn-copy.zil"),
        ),
        (
            "fnmut-copy",
            include_str!("../../../tests/impl256/frozen-prep/probes/fnmut-copy.zil"),
        ),
        (
            "fnonce-copy",
            include_str!("../../../tests/impl256/frozen-prep/probes/fnonce-copy.zil"),
        ),
        (
            "fn-owned",
            include_str!("../../../tests/impl256/frozen-prep/probes/fn-owned.zil"),
        ),
        (
            "fnmut-owned",
            include_str!("../../../tests/impl256/frozen-prep/probes/fnmut-owned.zil"),
        ),
        (
            "fnonce-owned",
            include_str!("../../../tests/impl256/frozen-prep/probes/fnonce-owned.zil"),
        ),
    ] {
        checked(name, source);
    }
}

#[test]
fn frozen_three_module_source_reaches_verify() {
    use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/impl256/frozen-prep/project");
    let names = ["main", "model", "relay"];
    let project = ProjectSources {
        root: ModuleId(0),
        modules: names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let path = root.join(format!("{name}.zil"));
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    path.to_string_lossy().into_owned(),
                    std::fs::read_to_string(path).unwrap(),
                )
            })
            .collect(),
        paths: [
            (vec!["model".into()], ModuleId(1)),
            (vec!["relay".into()], ModuleId(2)),
        ]
        .into_iter()
        .collect(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        zil_base::ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
