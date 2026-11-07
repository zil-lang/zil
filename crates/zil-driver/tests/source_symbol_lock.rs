use std::collections::BTreeMap;
use zil_base::{FileId, Fingerprint, ModuleId, Phase, RULE_VERSION, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, ProjectSources, QueryDisposition, Stage, SyntaxLock,
    compile_project, compile_source,
};

fn lock_for(source: &SourceUnit) -> String {
    let hash = source.fingerprint();
    let mut grammar = b"zil/provider-grammar/v1\0".to_vec();
    grammar.extend_from_slice(RULE_VERSION.as_bytes());
    grammar.push(0);
    grammar.extend_from_slice(&hash.0);
    format!(
        "zil-syntax-lock 1\nrule {RULE_VERSION}\nprovider lib.syntax default expand Expr brace {} {}\nprovider lib.syntax items expand_items ItemList brace {} {}\n",
        hash,
        Fingerprint::of(&grammar),
        hash,
        Fingerprint::of(&grammar)
    )
}

#[test]
fn lock_resolves_two_exports_against_actual_source_and_symbol() {
    let source = SourceUnit::new(
        FileId(1),
        ModuleId(1),
        "lib/syntax.zil",
        "pub fn expand(input:TokenTree)->Syntax<Expr>{return input}\npub fn expand_items(input:TokenTree)->Syntax<ItemList>{return input}",
    );
    let parsed = zil_syntax::parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut paths = BTreeMap::new();
    paths.insert(vec!["lib".into(), "syntax".into()], source.module_id);
    let lock = SyntaxLock::parse(&lock_for(&source)).unwrap();
    let registry = lock
        .resolve(std::slice::from_ref(&source), &[&parsed.module], &paths)
        .unwrap();
    assert_eq!(registry.entries.len(), 2);
    assert_eq!(registry.entries[0].symbol.module, source.module_id);
    assert_ne!(
        registry.entries[0].symbol.declaration,
        registry.entries[1].symbol.declaration
    );
    let mut edited = source.clone();
    edited.text.push('\n');
    assert!(lock.resolve(&[edited], &[&parsed.module], &paths).is_err());
}

#[test]
fn lock_rejects_duplicate_stale_rule_and_grammar() {
    let source = SourceUnit::new(
        FileId(1),
        ModuleId(1),
        "lib/syntax.zil",
        "pub fn expand(input:TokenTree)->Syntax<Expr>{return input}",
    );
    let valid = lock_for(&source);
    assert!(SyntaxLock::parse(&valid).is_ok());
    assert!(SyntaxLock::parse(&valid.replace(RULE_VERSION, "old-rules")).is_err());
    let duplicate = format!("{valid}{}", valid.lines().nth(2).unwrap());
    assert!(SyntaxLock::parse(&duplicate).is_err());
    let stale = valid.replacen(
        &source.fingerprint().to_hex(),
        &Fingerprint::ZERO.to_hex(),
        1,
    );
    assert!(SyntaxLock::parse(&stale).is_err());
}

#[test]
fn mixed_type_materialization_keeps_ordinary_record_ct_in_its_original_domain() {
    let modules = vec![
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import std.meta as meta\nimport model\nfn type_value()->Type=>i32\nfn main()->i32{let p=#(model.make());let x:#(type_value())=1;return p.n+x-2}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "model.zil",
            "pub struct Point{pub n:i32}pub fn make()->Point=>Point{n:1}",
        ),
    ];
    let project = ProjectSources {
        root: ModuleId(0),
        modules,
        paths: BTreeMap::from([(vec!["model".into()], ModuleId(1))]),
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

#[test]
fn single_source_type_and_syntax_use_the_project_pipeline() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "main.zil",
        "fn build()->Syntax<Expr>{return quote<Expr>{42};}fn main()->i32{return #splice(build());}",
    );
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let single = compile_source(&source, &options);
    let project = compile_project(
        &ProjectSources {
            root: source.module_id,
            modules: vec![source.clone()],
            paths: BTreeMap::new(),
        },
        &options,
    );
    assert_eq!(
        single.status,
        zil_base::ResultCategory::Success,
        "{}",
        single.to_json()
    );
    assert_eq!(project.status, single.status, "{}", project.to_json());
    assert_eq!(single.phase, project.phase);
    assert_eq!(single.input_fingerprint, project.input_fingerprint);
    assert_eq!(single.scalar_interfaces, project.scalar_interfaces);
    assert_eq!(single.verification_runs, 1);
}

#[test]
fn type_alias_meta_candidate_does_not_select_plain_value_ct() {
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    for type_value in ["#(i64)", "  #(i64)", "/*comment*/#(i64)"] {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            format!(
                "type Word={type_value};fn main()->i32{{let n:Word=5000000000;if n==5000000000{{return 0}}return 1}}"
            ),
        );
        for report in [
            compile_source(&source, &options),
            compile_project(
                &ProjectSources {
                    root: source.module_id,
                    modules: vec![source.clone()],
                    paths: BTreeMap::new(),
                },
                &options,
            ),
        ] {
            assert_eq!(
                report.status,
                zil_base::ResultCategory::Success,
                "{}",
                report.to_json()
            );
        }
    }
    let ordinary = SourceUnit::new(
        FileId(1),
        ModuleId(1),
        "old.zil",
        "type Word=i64;fn main()->i32{let n=#(41);if n==41{return 0}return 1}",
    );
    let parsed = zil_syntax::parse(&ordinary);
    assert!(!parsed.type_meta_candidate);
    let report = compile_source(&ordinary, &options);
    assert_eq!(
        report.status,
        zil_base::ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report
            .query_events
            .iter()
            .filter(|event| event.phase == Phase::Parse)
            .count(),
        1
    );
}

#[test]
fn locked_missing_syntax_import_matches_closed_memory_graph_error() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "main.zil",
        "syntax use absent as macro\nfn main()->i32=>0",
    );
    let absent = SourceUnit::new(
        FileId(1),
        ModuleId(1),
        "absent.zil",
        "pub fn expand(input:TokenTree)->Syntax<Expr>{return input}",
    );
    let hash = absent.fingerprint();
    let mut grammar = b"zil/provider-grammar/v1\0".to_vec();
    grammar.extend_from_slice(RULE_VERSION.as_bytes());
    grammar.push(0);
    grammar.extend_from_slice(&hash.0);
    let lock = SyntaxLock::parse(&format!(
        "zil-syntax-lock 1\nrule {RULE_VERSION}\nprovider absent default expand Expr brace {} {}\n",
        hash,
        Fingerprint::of(&grammar)
    ))
    .unwrap();
    let report = compile_source(
        &source,
        &CompileOptions {
            stop_after: Stage::Core,
            syntax_lock: Some(lock),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        zil_base::ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E-MODULE-NOT-FOUND"),
        "{}",
        report.to_json()
    );
}

#[test]
fn warm_single_source_uses_published_parse_and_reports_reuse() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "main.zil",
        "fn build()->Syntax<Expr>{return quote<Expr>{42};}fn main()->i32{return #splice(build());}",
    );
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::default();
    let cold = session.compile_source(&source, &options);
    assert_eq!(
        cold.status,
        zil_base::ResultCategory::Success,
        "{}",
        cold.to_json()
    );
    assert_eq!(
        cold.query_events
            .iter()
            .filter(|event| event.phase == Phase::Parse
                && event.disposition == QueryDisposition::Executed)
            .count(),
        1
    );
    let mut observed = Vec::new();
    let warm = session
        .compile_source_with_observer(&source, &options, &mut |event| observed.push(event.clone()));
    assert_eq!(warm.status, cold.status, "{}", warm.to_json());
    assert_eq!(warm.query_events, observed);
    let parses = warm
        .query_events
        .iter()
        .filter(|event| event.phase == Phase::Parse)
        .collect::<Vec<_>>();
    assert_eq!(parses.len(), 1);
    assert_eq!(parses[0].disposition, QueryDisposition::Reused);
}
