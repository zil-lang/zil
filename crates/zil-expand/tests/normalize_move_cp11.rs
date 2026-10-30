//! Source-derived controls for moving Normalize's owned specialization items.
use zil_base::{
    CompileResult, FileId, Fingerprint, Limits, ModuleId, ResultCategory, SourceMap, SourceUnit,
};
use zil_ir::{CoreImport, ProgramModule};

struct Project {
    sources: Vec<SourceUnit>,
    syntax: Vec<zil_syntax::ast::Module>,
    imports: Vec<Vec<CoreImport>>,
}

impl Project {
    fn new(texts: &[(&str, &str)]) -> Self {
        let file_ids = [11, 5, 29];
        let module_ids = [40, 7, 19];
        let sources = texts
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(file_ids[i]),
                    ModuleId(module_ids[i]),
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
                        let zil_syntax::ast::ItemKind::Import(import) = &item.kind else {
                            return None;
                        };
                        let index = texts
                            .iter()
                            .position(|(name, _)| import.path == [*name])
                            .unwrap();
                        Some(CoreImport {
                            alias: import
                                .alias
                                .clone()
                                .unwrap_or_else(|| import.path[0].clone()),
                            module: sources[index].module_id,
                            span: item.span,
                        })
                    })
                    .collect()
            })
            .collect();
        Self {
            sources,
            syntax,
            imports,
        }
    }

    fn expand(&self, limits: &Limits) -> CompileResult<Vec<(ProgramModule, SourceMap)>> {
        let refs = self.syntax.iter().collect::<Vec<_>>();
        let before = format!("{:?}", self.syntax);
        let expanded =
            zil_expand::expand_program_with_limits(&self.sources, &refs, &self.imports, limits);
        assert_eq!(format!("{:?}", self.syntax), before);
        expanded
    }

    fn observe(&self, name: &str) -> Vec<(ProgramModule, SourceMap)> {
        let result = self.expand(&Limits::default());
        let digest = Fingerprint::of(format!("{result:#?}").as_bytes()).to_hex();
        let expected = match name {
            "borrowed" => "421a64372cb38589d96a9ac39543ee5063df15ace48aebf8bd74b86f847b2dca",
            "borrowed-rotated" => {
                "2073275a1c915d0a99ef7327d0f725a3cf53434c5d6b22a3eca9763953be3a57"
            }
            "owned-generic" => "a608cd698353458f2b3c7c6d6113bf17a145c16ff95462ef196cca8f41b36813",
            "owned-alias" => "8a49b2e458c06bcd8be11e5941bc2ec00a37bc63ddd4c635ee9decfa367d6195",
            _ => panic!("unfrozen source case"),
        };
        assert_eq!(digest, expected);
        println!("CP11_NORMALIZE_SNAPSHOT:{name}:{}", digest);
        let result = result.unwrap();
        for ((module, map), source) in result.iter().zip(&self.sources) {
            assert_eq!(module.core.module_id, source.module_id);
            assert!(module.core.unsupported.is_empty());
            map.validate(&self.sources, &Limits::default()).unwrap();
        }
        result
    }

    fn boundaries(&self, name: &str) {
        let expected = self.expand(&Limits::default()).unwrap();
        for nodes in [false, true] {
            let run = |value| {
                let mut limits = Limits::default();
                if nodes {
                    limits.max_nodes = value;
                } else {
                    limits.max_analysis_steps = value;
                }
                self.expand(&limits)
            };
            let (mut low, mut high) = (
                0,
                if nodes {
                    Limits::default().max_nodes
                } else {
                    Limits::default().max_analysis_steps
                },
            );
            while low + 1 < high {
                let mid = low + (high - low) / 2;
                match run(mid) {
                    Ok(actual) => {
                        assert_eq!(actual, expected);
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
            let expected_boundary = match (name, nodes) {
                ("borrowed", false) => 606,
                ("borrowed", true) => 103,
                ("owned-generic", false) => 1238,
                ("owned-generic", true) => 285,
                ("owned-alias", false) => 275,
                ("owned-alias", true) => 41,
                _ => panic!("unfrozen source boundary"),
            };
            assert_eq!(high, expected_boundary);
            assert_eq!(run(high).unwrap(), expected);
            let below = run(high - 1).unwrap_err();
            assert!(
                below
                    .iter()
                    .all(|error| error.category == ResultCategory::ResourceLimit)
            );
            assert_eq!(run(high).unwrap(), expected);
            println!(
                "CP11_NORMALIZE_BOUNDARY:{name}:{}:{high}:{below:?}",
                if nodes { "nodes" } else { "work" }
            );
        }
    }
}

#[test]
fn borrowed_items_keep_defining_files_restore_order_and_imports() {
    let mut project = Project::new(&[
        (
            "main",
            "import left\nimport right\nfn main()->i64=>left.value()+right.value()",
        ),
        (
            "left",
            "fn hidden()->i64=>20\npub fn value()->i64=>hidden()",
        ),
        (
            "right",
            "fn hidden()->i64=>22\npub fn value()->i64=>hidden()",
        ),
    ]);
    let output = project.observe("borrowed");
    assert_eq!(output[0].0.imports, project.imports[0]);
    for (module, _) in &output[1..] {
        assert_eq!(
            module
                .core
                .functions
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            vec!["hidden", "value"]
        );
        assert_eq!(module.exports, vec![module.core.functions[1].id]);
    }
    project.boundaries("borrowed");
    project.sources.rotate_left(1);
    project.syntax.rotate_left(1);
    project.imports.rotate_left(1);
    project.observe("borrowed-rotated");
}

#[test]
fn owned_generic_and_alias_items_keep_template_sources_and_exports() {
    let generic = Project::new(&[
        (
            "main",
            "import library\nfn main()->i64{let a=library.make::<i64>(42);return library.read::<i64>(a)}",
        ),
        (
            "library",
            "pub struct Cell<T>{pub value:T}\npub fn make<T>(x:T)->Cell<T> => Cell::<T>{value:x}\npub fn read<T>(x:Cell<T>)->T=>x.value",
        ),
    ]);
    let output = generic.observe("owned-generic");
    assert_eq!(output[0].0.imports[0], generic.imports[0][0]);
    assert!(!output[1].0.core.aggregates.is_empty());
    for function in &output[1].0.core.functions {
        assert_eq!(function.span.file, FileId(5));
        assert!(output[1].0.exports.contains(&function.id));
    }
    generic.boundaries("owned-generic");
    let alias = Project::new(&[
        ("main", "import library\nfn main()->i64=>library.value()"),
        ("library", "type Number=i64\npub fn value()->Number=>42"),
    ]);
    let output = alias.observe("owned-alias");
    assert_eq!(output[1].0.core.functions[0].return_type, zil_ir::Type::I64);
    alias.boundaries("owned-alias");
}

#[test]
fn declaration_and_private_template_errors_keep_the_original_precedence() {
    for (name, texts, code) in [
        (
            "duplicate-before-alias",
            [
                ("main", "fn repeated()->i64=>1\nfn repeated()->i64=>2"),
                ("library", "type Cycle=Cycle"),
            ],
            "E-NAME-DUPLICATE",
        ),
        (
            "private-template",
            [
                (
                    "main",
                    "import library\nfn main()->i64=>library.id::<i64>(42)",
                ),
                ("library", "fn id<T>(x:T)->T=>x"),
            ],
            "E-MODULE-PRIVATE",
        ),
    ] {
        let project = Project::new(&texts);
        let errors = project.expand(&Limits::default()).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, code);
        println!("CP11_NORMALIZE_ERROR:{name}:{errors:?}");
    }
}

#[test]
fn raw_item_with_lost_definition_file_retains_work_and_source_error_order() {
    let mut project = Project::new(&[("main", "fn main()->i64=>42")]);
    // Raw untrusted AST control; the source-derived positive cases are above.
    project.syntax[0].items[0].span.file = FileId(999);
    let errors = project.expand(&Limits::default()).unwrap_err();
    assert_eq!(errors[0].code, "E-MODULE-ID");
    assert_eq!(
        errors[0].message,
        "instantiated declaration lost its definition source"
    );
    for work in [
        0,
        1,
        16,
        64,
        256,
        512,
        1024,
        Limits::default().max_analysis_steps,
    ] {
        let result = project.expand(&Limits {
            max_analysis_steps: work,
            ..Limits::default()
        });
        assert!(result.is_err());
        println!("CP11_NORMALIZE_RAW_FILE:{work}:{result:?}");
    }
}
