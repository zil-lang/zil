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
    let complete = GeneratedParseContext {
        tokens: lexer::lex(&source)
            .tokens
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .map(|t| GeneratedTokenContext {
                span: t.span,
                origin: 7,
                name_context: None,
            })
            .collect(),
    };
    let result = parse_generated(&source, SyntaxCategory::Expr, &complete, &Limits::default());
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

#[test]
fn generated_parse_requires_all_real_tokens_including_identifiers() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "generated.zil", "value+1");
    for context in [
        GeneratedParseContext::default(),
        GeneratedParseContext {
            tokens: vec![
                GeneratedTokenContext {
                    span: Span::new(FileId(0), 5, 6),
                    origin: 2,
                    name_context: None,
                },
                GeneratedTokenContext {
                    span: Span::new(FileId(0), 6, 7),
                    origin: 2,
                    name_context: None,
                },
            ],
        },
    ] {
        let result = parse_generated(&source, SyntaxCategory::Expr, &context, &Limits::default());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "E-PARSE-GENERATED-CONTEXT")
        );
    }
}
#[test]
fn generated_context_has_exact_prepaid_node_receipt_and_rejects_forged_sites() {
    let source = SourceUnit::new(FileId(1), ModuleId(2), "generated.zil", "f(x+1)");
    let context = GeneratedParseContext {
        tokens: lexer::lex(&source)
            .tokens
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .map(|t| GeneratedTokenContext {
                span: t.span,
                origin: 8,
                name_context: (t.kind == TokenKind::Identifier).then_some(13),
            })
            .collect(),
    };
    let full = parse_generated(&source, SyntaxCategory::Expr, &context, &Limits::default());
    assert!(full.diagnostics.is_empty());
    let n = full.usage.nodes_required;
    let exact = Limits {
        max_nodes: n,
        ..Limits::default()
    };
    for _ in 0..2 {
        let fresh = parse_generated(&source, SyntaxCategory::Expr, &context, &exact);
        assert!(fresh.diagnostics.is_empty(), "{:?}", fresh.diagnostics);
        assert_eq!(fresh.syntax, full.syntax);
        assert_eq!(fresh.usage, full.usage);
    }
    let below = parse_generated(
        &source,
        SyntaxCategory::Expr,
        &context,
        &Limits {
            max_nodes: n - 1,
            ..Limits::default()
        },
    );
    assert!(
        below
            .diagnostics
            .iter()
            .any(|d| d.code == "E-RESOURCE-LIMIT")
    );
    for mutated in 0..3 {
        let mut forged = context.clone();
        match mutated {
            0 => forged.tokens.insert(1, forged.tokens[0].clone()),
            1 => forged.tokens[0].span.file = FileId(9),
            2 => forged.tokens[1].name_context = Some(13),
            _ => unreachable!(),
        }
        let result = parse_generated(&source, SyntaxCategory::Expr, &forged, &Limits::default());
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "E-PARSE-GENERATED-CONTEXT")
        );
        assert!(result.syntax.is_none());
    }
    println!("generated parser N={n}, N-1=ResourceLimit, fresh=N");
}
