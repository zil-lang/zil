use std::collections::BTreeMap;
use zil_base::{FileId, Fingerprint, ModuleId, RULE_VERSION, SourceUnit};
use zil_driver::{CompileOptions, ProjectSources, Stage, SyntaxLock, compile_project};

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
        SourceUnit::new(FileId(0), ModuleId(0), "main.zil",
            "import std.meta as meta\nimport model\nfn type_value()->Type=>i32\nfn main()->i32{let p=#(model.make());let x:#(type_value())=1;return p.n+x-2}"),
        SourceUnit::new(FileId(1), ModuleId(1), "model.zil",
            "pub struct Point{pub n:i32}pub fn make()->Point=>Point{n:1}"),
    ];
    let project = ProjectSources { root: ModuleId(0), modules,
        paths: BTreeMap::from([(vec!["model".into()], ModuleId(1))]) };
    let report = compile_project(&project, &CompileOptions { stop_after: Stage::Verify,
        ..CompileOptions::default() });
    assert_eq!(report.status, zil_base::ResultCategory::Success, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1);
}
