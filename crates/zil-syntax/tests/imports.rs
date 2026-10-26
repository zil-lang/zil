use zil_base::{FileId, ModuleId, SourceUnit};
use zil_syntax::ast::ItemKind;

fn parse(text: &str) -> zil_syntax::ParseResult {
    zil_syntax::parse(&SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "imports.zil",
        text,
    ))
}

#[test]
fn dotted_imports_and_syntax_imports_preserve_logical_segments() {
    let result = parse("import math.arithmetic as numbers\nsyntax use build.tokens as builder\n");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    for (index, expected, alias, syntax) in [
        (0, ["math", "arithmetic"], "numbers", false),
        (1, ["build", "tokens"], "builder", true),
    ] {
        let ItemKind::Import(import) = &result.module.items[index].kind else {
            panic!("expected an import");
        };
        assert_eq!(import.path, expected);
        assert_eq!(import.alias.as_deref(), Some(alias));
        assert_eq!(import.syntax, syntax);
    }
}

#[test]
fn old_and_mixed_separators_have_a_specific_migration_error() {
    for source in [
        "import a/b",
        "import a.b/c",
        "import a/b.c",
        "import a / b as c",
        "syntax use a/b as x",
    ] {
        let result = parse(source);
        assert_eq!(
            result.diagnostics[0].code, "E-PARSE-IMPORT-SEPARATOR",
            "{source}"
        );
        assert!(result.diagnostics[0].message.contains("import a.b"));
    }
}

#[test]
fn import_continuation_requires_a_trailing_dot() {
    let result = parse("import a.\n b as name\n");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let ItemKind::Import(import) = &result.module.items[0].kind else {
        panic!("expected an import");
    };
    assert_eq!(import.path, ["a", "b"]);
    for source in [
        "import a\n.b",
        "import a..b",
        "import a.*",
        "import \"a.b\"",
    ] {
        assert!(!parse(source).diagnostics.is_empty(), "{source}");
    }
}

#[test]
fn zil_is_an_ordinary_logical_segment() {
    let result = parse("import a.zil");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let ItemKind::Import(import) = &result.module.items[0].kind else {
        panic!("expected an import");
    };
    assert_eq!(import.path, ["a", "zil"]);
}
