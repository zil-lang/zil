use zil_base::{FileId, Limits, ModuleId, SourceUnit};
use zil_ir::CoreImport;

#[test]
fn five_modules_preserve_two_declared_roots_through_ordinary_helpers() {
    let entries = [
        (
            "main",
            include_str!("../../../examples/mainline/state_bindings203/main.zil"),
        ),
        (
            "factory",
            include_str!("../../../examples/mainline/state_bindings203/factory.zil"),
        ),
        (
            "state",
            include_str!("../../../examples/mainline/state_bindings203/state.zil"),
        ),
        (
            "portable",
            include_str!("../../../examples/mainline/state_bindings203/portable.zil"),
        ),
        (
            "store_helpers",
            include_str!("../../../examples/mainline/state_bindings203/store_helpers.zil"),
        ),
    ];
    let sources = entries
        .iter()
        .enumerate()
        .map(|(i, (name, text))| {
            SourceUnit::new(
                FileId(i as u32),
                ModuleId(i as u32),
                format!("{name}.zil"),
                *text,
            )
        })
        .collect::<Vec<_>>();
    let syntax = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let edges: &[&[usize]] = &[&[1, 2, 3, 4], &[3], &[3], &[], &[]];
    let imports = edges
        .iter()
        .enumerate()
        .map(|(owner, targets)| {
            targets
                .iter()
                .map(|&target| CoreImport {
                    alias: entries[target].0.into(),
                    module: ModuleId(target as u32),
                    span: sources[owner].span(),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let modules = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &imports,
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(modules.len(), 5);
    for (i, (module, map)) in modules.iter().enumerate() {
        map.validate(&sources, &Limits::default()).unwrap();
        assert!(module.core.unsupported.is_empty());
        if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
            let directory = std::path::PathBuf::from(root).join("consumer203");
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join(format!("{}.zil", entries[i].0)),
                &sources[i].text,
            )
            .unwrap();
            std::fs::write(
                directory.join(format!("{}.core.txt", entries[i].0)),
                format!("{:#?}", module.core),
            )
            .unwrap();
        }
    }
}
