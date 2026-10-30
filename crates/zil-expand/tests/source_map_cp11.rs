//! Source-derived integration controls for per-request source-map validation.
use std::collections::BTreeSet;
use zil_base::{FileId, Fingerprint, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_ir::CoreImport;

#[test]
fn project_maps_keep_local_targets_utf8_and_exact_fresh_work() {
    let texts = [
        (
            "main",
            "import model\n// é\nfn main()->i64{let a=model.Cell{};return a.x}",
        ),
        (
            "model",
            "fn seed()->i64=>42\npub struct Cell{pub x:i64=seed()}",
        ),
        ("empty", "// 空模块\n"),
    ];
    let sources = texts
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
        .map(|source| zil_syntax::parse(source).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = syntax
        .iter()
        .map(|module| {
            module
                .items
                .iter()
                .filter_map(|item| {
                    if let zil_syntax::ast::ItemKind::Import(import) = &item.kind {
                        let position = texts
                            .iter()
                            .position(|(name, _)| import.path == [*name])
                            .unwrap();
                        Some(CoreImport {
                            alias: import.path[0].clone(),
                            module: ModuleId(position as u32),
                            span: item.span,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let refs = syntax.iter().collect::<Vec<_>>();
    let run =
        |limits: &Limits| zil_expand::expand_program_with_limits(&sources, &refs, &imports, limits);
    let expected = run(&Limits::default()).unwrap();
    for (_, map) in &expected {
        map.validate(&sources, &Limits::default()).unwrap();
    }
    let first = expected[0]
        .1
        .entries
        .iter()
        .map(|entry| &entry.target)
        .collect::<BTreeSet<_>>();
    let second = expected[1]
        .1
        .entries
        .iter()
        .map(|entry| &entry.target)
        .collect::<BTreeSet<_>>();
    assert!(first.intersection(&second).next().is_some());
    assert!(expected[2].1.entries.is_empty());
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        match run(&Limits {
            max_analysis_steps: mid,
            ..Limits::default()
        }) {
            Ok(value) => {
                assert_eq!(value, expected);
                high = mid;
            }
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|error| error.category == ResultCategory::ResourceLimit)
                );
                low = mid;
            }
        }
    }
    // Frozen against the unchanged project pipeline before index reuse.
    assert_eq!(high, 352);
    let limits = Limits {
        max_analysis_steps: high,
        ..Limits::default()
    };
    assert_eq!(run(&limits).unwrap(), expected);
    assert_eq!(
        Fingerprint::of(format!("{expected:#?}").as_bytes()).to_hex(),
        "365776dbbbcb572d38267cbbf6435a73d826aee62cc9bb9a66300b4771aae55d"
    );
    let below = run(&Limits {
        max_analysis_steps: high - 1,
        ..limits.clone()
    })
    .unwrap_err();
    assert!(
        below
            .iter()
            .all(|error| error.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(run(&limits).unwrap(), expected);
    println!(
        "CP11_PROJECT_MAP_BOUNDARY:{high}:{}:{below:?}",
        Fingerprint::of(format!("{expected:#?}").as_bytes()).to_hex()
    );
}
