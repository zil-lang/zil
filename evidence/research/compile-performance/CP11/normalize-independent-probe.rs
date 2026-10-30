use zil_base::{FileId, Fingerprint, Limits, ModuleId, SourceUnit};
use zil_ir::CoreImport;

fn main() {
    let fixtures: &[(&str, &[(&str, &str)], bool)] = &[
        (
            "borrowed-empty",
            &[
                ("main", "import lib\nfn main()->i64=>lib.get()"),
                ("lib", "fn hidden()->i64=>7\npub fn get()->i64=>hidden()"),
                ("empty", "// 空\n"),
            ],
            true,
        ),
        (
            "owned-generic",
            &[
                ("main", "import lib\nfn main()->i64=>lib.keep::<i64>(19)"),
                ("lib", "pub fn keep<T>(value:T)->T=>value"),
            ],
            true,
        ),
        (
            "owned-alias",
            &[
                ("main", "import lib\nfn main()->i64=>lib.get()"),
                (
                    "lib",
                    "type Count=i64\nfn get_inner()->Count=>23\npub fn get()->Count=>get_inner()",
                ),
            ],
            true,
        ),
        (
            "private-generic",
            &[
                ("main", "import lib\nfn main()->i64=>lib.keep::<i64>(19)"),
                ("lib", "fn keep<T>(value:T)->T=>value"),
            ],
            false,
        ),
        (
            "duplicate-before-cycle",
            &[
                ("main", "fn same()->i64=>1\nfn same()->i64=>2"),
                ("lib", "type Ring=Ring"),
            ],
            false,
        ),
    ];
    for (name, texts, accepted) in fixtures {
        let mut sources = texts
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId([101, 8, 45][i]),
                    ModuleId([17, 3, 41][i]),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect::<Vec<_>>();
        let mut syntax = sources
            .iter()
            .map(|s| zil_syntax::parse(s).into_result().unwrap())
            .collect::<Vec<_>>();
        let mut imports = syntax
            .iter()
            .map(|m| {
                m.items
                    .iter()
                    .filter_map(|item| {
                        let zil_syntax::ast::ItemKind::Import(import) = &item.kind else {
                            return None;
                        };
                        let i = texts.iter().position(|(n, _)| import.path == [*n]).unwrap();
                        Some(CoreImport {
                            alias: import.path[0].clone(),
                            module: sources[i].module_id,
                            span: item.span,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for order in 0..2 {
            let refs = syntax.iter().collect::<Vec<_>>();
            let before = format!("{syntax:#?}");
            let run = |limits: &Limits| {
                let result =
                    zil_expand::expand_program_with_limits(&sources, &refs, &imports, limits);
                assert_eq!(format!("{syntax:#?}"), before);
                result
            };
            let result = run(&Limits::default());
            assert_eq!(result.is_ok(), *accepted, "{name}:{order}:{result:?}");
            println!(
                "{name}:{order}:default:{}",
                Fingerprint::of(format!("{result:#?}").as_bytes()).to_hex()
            );
            for nodes in [false, true] {
                for limit in [0, 1, 32, 64, 128, 256, 512, 1024] {
                    let mut limits = Limits::default();
                    if nodes {
                        limits.max_nodes = limit;
                    } else {
                        limits.max_analysis_steps = limit;
                    }
                    let result = run(&limits);
                    let detail = match &result {
                        Ok(_) => Fingerprint::of(format!("{result:#?}").as_bytes()).to_hex(),
                        Err(errors) => format!("{errors:?}"),
                    };
                    println!("{name}:{order}:{nodes}:{limit}:{detail}");
                }
                if let Ok(expected) = &result {
                    let (mut low, mut high) = (
                        0,
                        if nodes {
                            Limits::default().max_nodes
                        } else {
                            Limits::default().max_analysis_steps
                        },
                    );
                    let bounded = |limit| {
                        let mut limits = Limits::default();
                        if nodes {
                            limits.max_nodes = limit;
                        } else {
                            limits.max_analysis_steps = limit;
                        }
                        run(&limits)
                    };
                    while low + 1 < high {
                        let mid = low + (high - low) / 2;
                        if bounded(mid).is_ok() {
                            high = mid;
                        } else {
                            low = mid;
                        }
                    }
                    assert_eq!(&bounded(high).unwrap(), expected);
                    let below = bounded(high - 1).unwrap_err();
                    assert_eq!(&bounded(high).unwrap(), expected);
                    println!("{name}:{order}:boundary:{nodes}:{high}:{below:?}");
                }
            }
            sources.rotate_left(1);
            syntax.rotate_left(1);
            imports.rotate_left(1);
        }
    }
}
