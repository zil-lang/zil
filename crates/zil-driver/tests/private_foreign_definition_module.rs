//! A private foreign dependency stays in its declaration's lexical module,
//! including when a generic export is the only requested dependency.
use zil_base::{FileId, Fingerprint, Limits, ModuleId, SourceUnit, TargetProfile};
fn expand(main: &str) -> Result<Vec<zil_ir::ProgramModule>, Vec<zil_base::Diagnostic>> {
    let texts = [
        ("checks", main),
        (
            "records",
            include_str!(
                "../../../tests/language-readiness/ordinary-application/model/records.zil"
            ),
        ),
        (
            "owning_map",
            include_str!("../../../lib/std/owning_map.zil"),
        ),
        (
            "owning_hash_map",
            include_str!("../../../lib/std/owning_hash_map.zil"),
        ),
        (
            "owning_entries",
            include_str!("../../../lib/std/owning_entries.zil"),
        ),
        ("store", include_str!("../../../lib/std/store.zil")),
    ];
    let sources: Vec<_> = texts
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
        .collect();
    let parsed: Vec<_> = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect();
    let syntax: Vec<_> = parsed.iter().collect();
    let imports: Vec<Vec<_>> = syntax
        .iter()
        .map(|m| {
            m.items
                    .iter()
                    .filter_map(|i| {
                        let zil_syntax::ast::ItemKind::Import(import) = &i.kind else {
                            return None;
                        };
                        let name = import.path.last().unwrap();
                        Some(zil_ir::CoreImport {
                            alias: import.alias.as_ref().unwrap_or(name).clone(),
                            module: ModuleId(
                                texts.iter().position(|(n, _)| *n == name).unwrap() as u32
                            ),
                            span: i.span,
                        })
                    })
                    .collect()
        })
        .collect();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    zil_expand::expand_program_with_limits_target(&sources, &syntax, &imports, &limits, &target)
        .map(|modules| modules.into_iter().map(|(module, _)| module).collect())
}
#[test]
fn imported_generic_keeps_private_drop_foreign_dependency_in_its_actual_module() {
    let modules = expand("import model.records\nfn main()->i32{return records.echo(42)}").unwrap();
    let root = &modules[0];
    let records = &modules[1];
    assert_eq!(root.core.module_id, ModuleId(0));
    assert_eq!(records.core.module_id, ModuleId(1));
    assert!(!root.core.functions.iter().any(|f| f.name == "putchar"));
    let foreign = records
        .core
        .functions
        .iter()
        .find(|f| f.name == "putchar")
        .expect("private extern in its actual definition module");
    assert!(foreign.foreign.is_some());
    assert!(
        !records.exports.contains(&foreign.id),
        "private extern became public"
    );
    assert!(
        records
            .core
            .functions
            .iter()
            .any(|f| f.name.contains("drop")),
        "Drop dependency left its actual module"
    );
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    zil_types::check_program(
        &zil_ir::CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::of(b"private imported drop"),
            modules,
        },
        &target,
        &limits,
    )
    .unwrap();
}
#[test]
fn imported_private_foreign_dependency_cannot_be_called_by_consumer() {
    let modules =
        expand("import model.records\nfn main()->i32{unsafe{return records.putchar(65)}}").unwrap();
    let errors = zil_types::check_program(
        &zil_ir::CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::of(b"private extern access"),
            modules,
        },
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == "E-MODULE-PRIVATE"
            && e.message == "function records.putchar is private"),
        "{errors:?}"
    );
}

#[test]
fn warm_project_identity_includes_actual_alias_targets_and_retries() {
    use zil_base::{Phase, ResultCategory};
    use zil_driver::{CompilationSession, CompileOptions, ProjectSources, QueryDisposition, Stage};
    let main = "import model.records\nfn main()->i32{return records.echo(42)}";
    let texts = [
        main,
        include_str!("../../../tests/language-readiness/ordinary-application/model/records.zil"),
        include_str!("../../../lib/std/owning_map.zil"),
        include_str!("../../../lib/std/owning_hash_map.zil"),
        include_str!("../../../lib/std/owning_entries.zil"),
        include_str!("../../../lib/std/store.zil"),
    ];
    let mut project = ProjectSources {
        root: ModuleId(0),
        modules: texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("actual-{i}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: Default::default(),
    };
    for (path, id) in [
        ("model.records", 1),
        ("std.owning_map", 2),
        ("std.owning_hash_map", 3),
        ("std.owning_entries", 4),
        ("std.store", 5),
    ] {
        project
            .paths
            .insert(path.split('.').map(str::to_owned).collect(), ModuleId(id));
    }
    let options = CompileOptions {
        stop_after: Stage::Types,
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::default();
    let cold = session.compile_project(&project, &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    let warm = session.compile_project(&project, &options);
    assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
    assert!(
        warm.query_events
            .iter()
            .any(|e| e.phase == Phase::Expand && e.disposition == QueryDisposition::Reused)
    );
    let mut wrong = project.clone();
    wrong
        .paths
        .insert(vec!["model".into(), "records".into()], ModuleId(2));
    let rejected = session.compile_project(&wrong, &options);
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "{}",
        rejected.to_json()
    );
    assert!(
        !rejected
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Expand && e.disposition == QueryDisposition::Reused)
    );
    let retry = session.compile_project(&project, &options);
    assert_eq!(retry.status, ResultCategory::Success, "{}", retry.to_json());
    let fresh = CompilationSession::default().compile_project(&project, &options);
    assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
}
