use super::*;
use crate::{ast::SyntaxCategory, token::TokenKind};
use zil_base::{FileId, ModuleId};
#[test]
fn generated_categories_preserve_exact_token_contexts() {
    for (category, text) in [
        (SyntaxCategory::Expr, "x+1"),
        (SyntaxCategory::Type, "[i32;4]"),
        (SyntaxCategory::StmtList, "let x=1;work(x);"),
        (SyntaxCategory::ItemList, "pub fn f()->i32{return 1}"),
    ] {
        let source = SourceUnit::new(FileId(4), ModuleId(2), "generated.zil", text);
        let tokens = lexer::lex(&source).tokens;
        let context = GeneratedParseContext {
            tokens: tokens
                .iter()
                .filter(|t| t.kind != TokenKind::Eof)
                .map(|t| GeneratedTokenContext {
                    span: t.span,
                    origin: 17,
                    name_context: (t.kind == TokenKind::Identifier).then_some(23),
                })
                .collect(),
        };
        let result = parse_generated(&source, category, &context, &Limits::default())
            .into_result()
            .unwrap();
        assert_eq!(result.context, context);
        assert_eq!(result.tokens, tokens);
        assert!(matches!(
            (category, result.syntax.unwrap()),
            (SyntaxCategory::Expr, GeneratedSyntax::Expr(_))
                | (SyntaxCategory::Type, GeneratedSyntax::Type(_))
                | (SyntaxCategory::StmtList, GeneratedSyntax::StmtList(_))
                | (SyntaxCategory::ItemList, GeneratedSyntax::ItemList(_))
        ));
    }
}
#[test]
fn generated_parse_rejects_extra_input_and_bad_context_without_dropping_source() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "generated.zil", "1;2");
    let result = parse_generated(
        &source,
        SyntaxCategory::Expr,
        &GeneratedParseContext::default(),
        &Limits::default(),
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "E-PARSE-GENERATED-TRAILING")
    );
    assert!(!result.tokens.is_empty());
    let context = GeneratedParseContext {
        tokens: vec![GeneratedTokenContext {
            span: Span::new(FileId(0), 0, 999),
            origin: 0,
            name_context: None,
        }],
    };
    let result = parse_generated(&source, SyntaxCategory::Expr, &context, &Limits::default());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "E-PARSE-GENERATED-CONTEXT")
    );
}
