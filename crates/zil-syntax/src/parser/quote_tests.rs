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
