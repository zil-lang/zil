//! Source-derived Expand controls for request-local declaration lookup reuse.
use zil_base::{
    CompileResult, ExpansionRole, FileId, Limits, ModuleId, Phase, ResultCategory, SourceMap,
    SourceUnit, Span,
};
use zil_ir::{CoreExprKind, CoreImport, CoreStatementKind, ProgramModule};

struct Project {
    sources: Vec<SourceUnit>,
    syntax: Vec<zil_syntax::ast::Module>,
    imports: Vec<Vec<CoreImport>>,
}

impl Project {
    fn new(texts: &[&str], edges: &[&[(&str, u32)]]) -> Self {
        let sources: Vec<_> = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("cp10-{i}.zil"),
                    *text,
                )
            })
            .collect();
        let syntax = sources
            .iter()
            .map(|source| zil_syntax::parse(source).into_result().unwrap())
            .collect();
        let imports = edges
            .iter()
            .enumerate()
            .map(|(i, entries)| {
                entries
                    .iter()
                    .map(|(alias, module)| CoreImport {
                        alias: (*alias).into(),
                        module: ModuleId(*module),
                        span: Span::new(FileId(i as u32), 0, 0),
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
        let syntax: Vec<_> = self.syntax.iter().collect();
        zil_expand::expand_program_with_limits(&self.sources, &syntax, &self.imports, limits)
    }
}

fn defaults_project() -> Project {
    Project::new(
        &[
            "import left\nimport right\nfn main()->i64 { let a=left.Cell{}; let b=right.Cell{}; return a.x+b.x }",
            "fn seed()->i64 => 20\npub struct Cell { pub x:i64 = seed() }",
            "pub struct Cell { pub x:i64 = 22 }",
            "import left\npub fn value()->i64 { let a:left.Cell=.{}; return a.x }",
        ],
        &[&[("left", 1), ("right", 2)], &[], &[], &[("left", 1)]],
    )
}

#[test]
fn shared_named_declarations_keep_owner_defaults_and_source_maps() {
    let mut project = defaults_project();
    for _ in 0..4 {
        let expanded = project.expand(&Limits::default()).unwrap();
        for (module, map) in &expanded {
            assert!(module.core.unsupported.is_empty());
            map.validate(&project.sources, &Limits::default()).unwrap();
        }
        for (consumer, origins) in [(0, vec![FileId(1), FileId(2)]), (3, vec![FileId(1)])] {
            let (module, map) = expanded
                .iter()
                .find(|(module, _)| module.core.module_id == ModuleId(consumer))
                .unwrap();
            let defaults: Vec<_> = map
                .entries
                .iter()
                .filter(|entry| entry.role == ExpansionRole::RecordDefault)
                .map(|entry| entry.origin.file)
                .collect();
            assert_eq!(defaults, origins);
            let generated: Vec<_> = module
                .imports
                .iter()
                .filter(|import| import.alias.starts_with("$record_defaults_module"))
                .map(|import| import.module)
                .collect();
            let expected: Vec<_> = origins.iter().map(|file| ModuleId(file.0)).collect();
            assert_eq!(generated, expected);
        }
        for owner in [1, 2] {
            let module = &expanded
                .iter()
                .find(|(module, _)| module.core.module_id == ModuleId(owner))
                .unwrap()
                .0;
            let helper = module
                .core
                .functions
                .iter()
                .find(|function| function.name.starts_with("$record_default_"))
                .unwrap();
            assert!(module.exports.contains(&helper.id));
            assert_eq!(helper.span.file, FileId(owner));
        }
        // Preserve module identities while changing the input traversal order.
        project.sources.rotate_left(1);
        project.syntax.rotate_left(1);
        project.imports.rotate_left(1);
    }
}

#[test]
fn private_field_errors_keep_the_first_consumer_source() {
    let project = Project::new(
        &[
            "import model\nfn first(){let a=model.Hidden{value:1}}\nfn second(){let b=model.Hidden{value:2}}",
            "pub struct Hidden { value:i64 }",
        ],
        &[&[("model", 1)], &[]],
    );
    let errors = project.expand(&Limits::default()).unwrap_err();
    assert_eq!(errors.len(), 2);
    for error in &errors {
        assert_eq!(error.phase, Phase::Expand);
        assert_eq!(error.code, "E-FIELD-PRIVATE");
        assert_eq!(error.category, ResultCategory::SafeReject);
        assert_eq!(error.span.unwrap().file, FileId(0));
    }
    let first = errors[0].span.unwrap();
    let second = errors[1].span.unwrap();
    assert!(first.start < second.start);
    assert_eq!(
        &project.sources[0].text[first.start..first.end],
        "model.Hidden{value:1}"
    );
}

#[test]
fn duplicate_declaration_rejection_still_precedes_consumer_lowering() {
    let project = Project::new(
        &[
            "import model\nfn main(){let a=model.Cell{}}",
            "pub struct Cell { hidden:i64 }\npub struct Cell { pub value:i64=42 }",
        ],
        &[&[("model", 1)], &[]],
    );
    let errors = project.expand(&Limits::default()).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].phase, Phase::Expand);
    assert_eq!(errors[0].code, "E-NAME-DUPLICATE");
    let span = errors[0].span.unwrap();
    assert_eq!(span.file, FileId(1));
    assert_eq!(
        span.start,
        project.sources[1]
            .text
            .find("pub struct Cell { pub")
            .unwrap()
    );
}

#[test]
fn single_module_uses_first_written_aggregate_until_duplicate_check() {
    let source = SourceUnit::new(
        FileId(4),
        ModuleId(7),
        "cp10-local.zil",
        "struct Cell{x:i64=11}\nstruct Cell{x:i64=22}\nfn value()->Cell=>Cell{}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = zil_expand::expand_with_map(&source, &syntax).unwrap();
    assert_eq!(expanded.core.aggregates.len(), 2);
    let default = expanded
        .core
        .functions
        .iter()
        .find(|function| function.name.starts_with("$field_default"))
        .unwrap();
    assert!(matches!(
        &default.body.statements[0].kind,
        CoreStatementKind::Return(Some(value)) if matches!(&value.kind, CoreExprKind::Integer(number) if number == "11")
    ));
}

#[test]
fn project_analysis_budget_has_an_exact_fresh_boundary() {
    let project = defaults_project();
    // Measured against the unchanged CP10 baseline, not adjusted by the reuse.
    let limits = Limits {
        max_analysis_steps: 677,
        ..Limits::default()
    };
    let accepted = project.expand(&limits).unwrap();
    let errors = project
        .expand(&Limits {
            max_analysis_steps: 676,
            ..limits.clone()
        })
        .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].phase, Phase::Expand);
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].message, "generic expansion budget exceeded");
    assert_eq!(errors[0].span, Some(Span::new(FileId(3), 62, 63)));
    assert_eq!(project.expand(&limits).unwrap(), accepted);
}
