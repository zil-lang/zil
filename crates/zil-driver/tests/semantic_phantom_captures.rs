use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
#[test]
fn a_semantic_only_nominal_capture_keeps_its_original_creator_class() {
    let text = include_str!(
        "../../../tests/fixtures/returned-owning-holders/semantic-only-phantom-capture.zil"
    );
    let source = SourceUnit::new(FileId(0), ModuleId(0), "phantom-capture.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    if let Ok(directory) = std::env::var("ZIL_PHANTOM_CAPTURE_ARTIFACT_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join("core.txt"),
            format!("{core:#?}"),
        )
        .unwrap();
    }
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let d = &module.closures[0];
    assert_eq!(d.storage_life.as_ref().unwrap().fixed_binder_count, 1);
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    if let Ok(directory) = std::env::var("ZIL_PHANTOM_CAPTURE_ARTIFACT_DIR") {
        std::fs::write(
            std::path::Path::new(&directory).join("typed.txt"),
            format!("{module:#?}"),
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join("candidate.wire"),
            zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap(),
        )
        .unwrap();
    }
}
