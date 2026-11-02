use crate::{ast::*, parse};
use zil_base::{FileId, ModuleId, SourceUnit};
fn expression(text: &str) -> Expr {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "quote.zil",
        format!("fn build(){{return {text}}}"),
    );
    let module = parse(&source).into_result().unwrap();
    let ItemKind::Function(f) = module.items.into_iter().next().unwrap().kind else {
        panic!()
    };
    let FunctionBody::Block(b) = f.body else {
        panic!()
    };
    let StmtKind::Return(Some(e)) = b.statements.into_iter().next().unwrap().kind else {
        panic!()
    };
    e
}
#[test]
fn quote_four_categories_have_typed_name_type_and_list_holes() {
    for (text, cat) in [
        ("quote<Expr>{$(x)+1}", SyntaxCategory::Expr),
        ("quote<Type>{[$(t);4]}", SyntaxCategory::Type),
        (
            "quote<StmtList>{$(prefix)\nwork();}",
            SyntaxCategory::StmtList,
        ),
        (
            "quote<ItemList>{pub fn $(name)(p:$(ty))->$(ret){$(body)\n}\n$(items)}",
            SyntaxCategory::ItemList,
        ),
    ] {
        let ExprKind::Quote { category, template } = expression(text).kind else {
            panic!()
        };
        assert_eq!(category, cat);
        let kinds = template.holes.iter().map(|h| h.kind).collect::<Vec<_>>();
        match cat {
            SyntaxCategory::Expr => assert_eq!(kinds, vec![QuoteHoleKind::Expr]),
            SyntaxCategory::Type => assert_eq!(kinds, vec![QuoteHoleKind::Type]),
            SyntaxCategory::StmtList => assert_eq!(kinds, vec![QuoteHoleKind::StmtList]),
            SyntaxCategory::ItemList => assert_eq!(
                kinds,
                vec![
                    QuoteHoleKind::Ident {
                        namespace: QuoteNameNamespace::Value,
                        role: QuoteNameRole::Binding
                    },
                    QuoteHoleKind::Type,
                    QuoteHoleKind::Type,
                    QuoteHoleKind::StmtList,
                    QuoteHoleKind::ItemList
                ]
            ),
        }
    }
}
#[test]
fn nested_quote_holes_stay_in_their_actual_layer() {
    let ExprKind::Quote { template, .. } =
        expression("quote<Expr>{$(wrap(quote<Expr>{$(x)}))}").kind
    else {
        panic!()
    };
    assert_eq!(template.holes.len(), 1);
    let ExprKind::Call { arguments, .. } = &template.holes[0].value.kind else {
        panic!()
    };
    let ExprKind::Quote {
        template: inner, ..
    } = &arguments[0].value.kind
    else {
        panic!()
    };
    assert_eq!(inner.holes.len(), 1);
    let ExprKind::Quote { template, .. } =
        expression("quote<ItemList>{fn f(x:i32)->Syntax<Expr>{return quote<Expr>{$(x)}}}").kind
    else {
        panic!()
    };
    assert!(template.holes.is_empty());
}
#[test]
fn strings_are_not_holes_and_quote_is_contextual() {
    let ExprKind::Quote { template, .. } = expression("quote<Expr>{\"$(not_a_hole)\"}").kind else {
        panic!()
    };
    assert!(template.holes.is_empty());
    assert!(matches!(
        expression("quote+1").kind,
        ExprKind::Binary { .. }
    ));
    assert!(matches!(
        expression("quote<2").kind,
        ExprKind::Binary { .. }
    ));
}
#[test]
fn splice_has_distinct_expression_and_type_ast_nodes() {
    assert!(matches!(
        expression("#splice(make())").kind,
        ExprKind::MetaSplice(_)
    ));
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "splice.zil",
        "type Chosen=#splice(make())",
    );
    let module = parse(&source).into_result().unwrap();
    let ItemKind::TypeAlias(a) = &module.items[0].kind else {
        panic!()
    };
    assert!(matches!(
        a.value.as_ref().unwrap().kind,
        TypeKind::MetaSplice(_)
    ));
}
#[test]
fn malformed_or_outside_holes_are_rejected_without_recovery_success() {
    for text in [
        "fn f(){let x=$(v);}",
        "fn f(){let x=quote<Unknown>{1};}",
        "fn f(){let x=quote<Expr>{};}",
        "fn f(){let x=quote<Type>{i32+1};}",
        "fn f(){let x=quote<Expr>{$()};}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "badquote.zil", text);
        assert!(!parse(&source).is_ok(), "{text}");
    }
}

#[test]
fn syntax_is_contextual_in_standard_paths_aliases_and_ordinary_names() {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "stdsyntax.zil",
        "import std.syntax as syntax\nsyntax use providers.example as ui\nfn f(){let value=syntax.export(\"answer\");let syntax=1;let next=syntax+1;}",
    );
    let module = parse(&source).into_result().unwrap();
    let ItemKind::Import(import) = &module.items[0].kind else {
        panic!()
    };
    assert_eq!(import.path, vec!["std", "syntax"]);
    assert_eq!(import.alias.as_deref(), Some("syntax"));
    let ItemKind::Import(import) = &module.items[1].kind else {
        panic!()
    };
    assert!(import.syntax);
}
#[test]
fn name_holes_have_exact_binding_namespaces() {
    let ExprKind::Quote{template,..}=expression("quote<ItemList>{import $(module) as $(alias)\nfn f<$(ty),life $(lft),#$(constant):i32>(){let c=($(param):i32)=>$(body);with $(guard)=a{work()}}}").kind else {panic!()};
    let expected = [
        (QuoteNameNamespace::Module, QuoteNameRole::Reference),
        (QuoteNameNamespace::Module, QuoteNameRole::Binding),
        (QuoteNameNamespace::Type, QuoteNameRole::Binding),
        (QuoteNameNamespace::Label, QuoteNameRole::Binding),
        (QuoteNameNamespace::Value, QuoteNameRole::Binding),
        (QuoteNameNamespace::Value, QuoteNameRole::Binding),
    ];
    for (hole, (namespace, role)) in template.holes.iter().zip(expected) {
        assert_eq!(hole.kind, QuoteHoleKind::Ident { namespace, role });
    }
    assert_eq!(template.holes.len(), 8);
    assert_eq!(template.holes[6].kind, QuoteHoleKind::Expr);
    assert_eq!(
        template.holes[7].kind,
        QuoteHoleKind::Ident {
            namespace: QuoteNameNamespace::Value,
            role: QuoteNameRole::Binding
        }
    );
}
#[test]
fn quote_name_sites_are_original_tokens_and_exclude_hole_expressions() {
    let text = "fn build(){return quote<Expr>{free + $(inside.member)}}";
    let source = SourceUnit::new(FileId(3), ModuleId(7), "sites.zil", text);
    let parsed = parse(&source);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    let ItemKind::Function(f) = &parsed.module.items[0].kind else {
        panic!()
    };
    let FunctionBody::Block(b) = &f.body else {
        panic!()
    };
    let StmtKind::Return(Some(e)) = &b.statements[0].kind else {
        panic!()
    };
    let ExprKind::Quote { template, .. } = &e.kind else {
        panic!()
    };
    assert_eq!(template.name_sites.len(), 1);
    let site = template.name_sites[0];
    let token = &parsed.tokens[site.token_index as usize];
    assert_eq!(token.span, site.span);
    assert_eq!(token.text(text), "free");
}
#[test]
fn quote_receipt_exact_n_minus_one_and_fresh_limits() {
    use zil_base::Limits;
    for text in [
        "fn f(){return quote<Expr>{x+$(value)}}",
        "fn f(){return quote<ItemList>{pub fn $(name)(x:$(ty)){ $(body)\n }}}",
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "budget.zil", text);
        let full = parse(&source);
        assert!(full.is_ok(), "{:?}", full.diagnostics);
        let n = full.usage.nodes_required;
        let exact = Limits {
            max_nodes: n,
            ..Limits::default()
        };
        let pass = crate::parse_with_limits(&source, &exact);
        assert!(pass.is_ok(), "N{n}: {:?}", pass.diagnostics);
        assert_eq!(pass.module, full.module);
        let fail = crate::parse_with_limits(
            &source,
            &Limits {
                max_nodes: n - 1,
                ..Limits::default()
            },
        );
        assert!(
            fail.diagnostics
                .iter()
                .any(|d| d.code == "E-RESOURCE-LIMIT")
        );
        let fresh = crate::parse_with_limits(&source, &exact);
        assert!(fresh.is_ok());
        assert_eq!(fresh.usage, pass.usage);
        println!("quote parser N={n}, N-1=ResourceLimit, fresh=N");
    }
}
#[test]
fn unknown_island_keeps_raw_dollars_comments_delimiters_and_utf8() {
    let raw = "{  $(unknown)\t/* 保留\r\n /* nested */ */ [\"$()\",{x}] }";
    let text = format!("fn f(){{let value=missing.tokens!{raw}}}");
    let source = SourceUnit::new(FileId(0), ModuleId(0), "island.zil", &text);
    let module = parse(&source).into_result().unwrap();
    let ItemKind::Function(f) = &module.items[0].kind else {
        panic!()
    };
    let FunctionBody::Block(b) = &f.body else {
        panic!()
    };
    let StmtKind::Binding { value, .. } = &b.statements[0].kind else {
        panic!()
    };
    let ExprKind::SyntaxIsland { tree, .. } = &value.kind else {
        panic!()
    };
    assert_eq!(&text[tree.span.start..tree.span.end], raw);
    let formatted = crate::format::format(&source).unwrap();
    assert!(formatted.contains(raw));
    assert!(
        parse(&SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "formatted.zil",
            formatted
        ))
        .is_ok()
    );
}
#[test]
fn standard_syntax_alias_can_be_shadowed_by_parameter_or_local_before_ct_checks() {
    for (text, parameter) in [
        (
            "import std.syntax as syntax; fn bad(syntax:i32)->Syntax<Expr>{return syntax.literal(1)}",
            true,
        ),
        (
            "import std.syntax as syntax; fn bad()->Syntax<Expr>{let syntax=0;return syntax.literal(1)}",
            false,
        ),
    ] {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "shadow.zil", text);
        let parsed = parse(&source).into_result().unwrap();
        let ItemKind::Function(function) = &parsed.items[1].kind else {
            panic!()
        };
        if parameter {
            assert_eq!(function.params[0].name, "syntax");
        }
        let FunctionBody::Block(body) = &function.body else {
            panic!()
        };
        if !parameter {
            let StmtKind::Binding { pattern, .. } = &body.statements[0].kind else {
                panic!()
            };
            assert_eq!(pattern.kind, PatternKind::Name("syntax".into()));
        }
        let StmtKind::Return(Some(Expr {
            kind: ExprKind::Call { callee, .. },
            ..
        })) = &body.statements.last().unwrap().kind
        else {
            panic!()
        };
        let ExprKind::Member {
            object,
            member: MemberName::Name(member),
        } = &callee.kind
        else {
            panic!()
        };
        assert_eq!(object.kind, ExprKind::Name("syntax".into()));
        assert_eq!(member, "literal");
    }
}
