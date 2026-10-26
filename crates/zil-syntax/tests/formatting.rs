use zil_base::{FileId, ModuleId, SourceUnit};
use zil_syntax::ast::*;
use zil_syntax::token::TokenKind;

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(13), ModuleId(4), "format.zil", text)
}

#[test]
fn short_closure_annotations_and_tail_discard_survive_trivia_formatting() {
    let text = "  fn sample() {\r\n\tlet a = move x -> i32 => { x + 1 /* ; */ }\r\n\tlet b = () => { \";\"; }\r\n}\r\n";
    let before = zil_syntax::parse(&source(text)).into_result().unwrap();
    let formatted = zil_syntax::format::format(&source(text)).unwrap();
    assert_ne!(formatted, text);
    let after = zil_syntax::parse(&source(&formatted))
        .into_result()
        .unwrap();
    assert_eq!(module_shape(&before), module_shape(&after));
    assert_eq!(
        zil_syntax::format::format(&source(&formatted)).unwrap(),
        formatted
    );
}

#[test]
fn plan_t01_001_formatter_changes_trivia_preserving_bindings_and_ast_shape() {
    let text = "  struct   Point { x: i32 }  \r\n\timpl Point(self: &Self) {\r\n\tfn make(x: i32) {\r\n\t\tlet   cb = (x: i32) => x + .x;\r\n\t\tlet quoted = \"#[] \\n 字\"; // keep  spaces\r\n\t\treturn\r\n\t\tcb(x)\r\n\t}\r\n}\r\n";
    let before = zil_syntax::parse(&source(text)).into_result().unwrap();
    let output = zil_syntax::format::format(&source(text)).unwrap();
    assert_ne!(
        output, text,
        "this fixture must exercise a real formatting change"
    );
    assert!(!output.contains('\r'));
    assert!(output.contains("    fn make"));
    assert!(output.contains("\"#[] \\n 字\""));
    assert!(output.contains("// keep  spaces"));
    let after = zil_syntax::parse(&source(&output)).into_result().unwrap();
    assert_eq!(module_shape(&before), module_shape(&after));
    assert_eq!(
        zil_syntax::format::format(&source(&output)).unwrap(),
        output,
        "idempotence"
    );
    let ItemKind::Impl(implementation) = &after.items[1].kind else {
        panic!();
    };
    assert_eq!(
        implementation.receiver_default.as_ref().unwrap().name,
        "self"
    );
    let ItemKind::Function(function) = &implementation.members[0].kind else {
        panic!();
    };
    assert_eq!(function.params[0].name, "x");
    let FunctionBody::Block(block) = &function.body else {
        panic!();
    };
    let StmtKind::Binding { value, .. } = &block.statements[0].kind else {
        panic!();
    };
    let ExprKind::Closure { params, .. } = &value.kind else {
        panic!();
    };
    assert_eq!(
        params[0].name, "x",
        "the inner shadowing binding remains distinct syntax"
    );
    assert!(
        matches!(block.statements[2].kind, StmtKind::Return(None)),
        "return must not absorb the next line"
    );
}

#[test]
fn plan_t01_001_logical_newlines_and_syntax_island_bytes_are_preserved() {
    let island = "{  a\t/* 保留\r\n /* inner */ */ (\r\n  x  ) }";
    let text = format!(
        "fn f() {{\r\nlet x = 1 +\r\n 2\r\nx\r\n(4)\r\nlet z = b.tokens!{island}\r\n}}\r\n"
    );
    let before = zil_syntax::parse(&source(&text)).into_result().unwrap();
    let output = zil_syntax::format::format(&source(&text)).unwrap();
    assert!(output.contains(island));
    let after = zil_syntax::parse(&source(&output)).into_result().unwrap();
    assert_eq!(module_shape(&before), module_shape(&after));
    let ItemKind::Function(function) = &after.items[0].kind else {
        panic!();
    };
    let FunctionBody::Block(block) = &function.body else {
        panic!();
    };
    assert_eq!(block.statements.len(), 4);
    assert!(
        matches!(&block.statements[1].kind, StmtKind::Expression(Expr { kind: ExprKind::Name(n), .. }) if n == "x")
    );
    assert!(matches!(
        &block.statements[2].kind,
        StmtKind::Expression(Expr {
            kind: ExprKind::Group(_),
            ..
        })
    ));
}

#[test]
fn plan_t01_001_lossless_tokens_and_delimiter_recovery_keep_real_spans() {
    let input = source(
        "// 保留\r\nfn broken(a: i32) { let value = (1; }\r\n/* nested /* 内 */ */ fn good() -> i32 => 7\r\n",
    );
    let parsed = zil_syntax::parse(&input);
    assert!(!parsed.is_ok());
    let mut bytes = String::new();
    for token in &parsed.tokens {
        assert!(token.span.is_valid_for(&input));
        bytes.push_str(token.text(&input.text));
    }
    assert_eq!(bytes, input.text);
    assert!(
        parsed
            .tokens
            .iter()
            .any(|t| t.kind == TokenKind::LineComment)
    );
    assert!(
        parsed
            .tokens
            .iter()
            .any(|t| t.kind == TokenKind::BlockComment)
    );
    assert!(
        parsed
            .tokens
            .iter()
            .any(|t| t.kind == TokenKind::Newline && t.text(&input.text) == "\r\n")
    );
    for diagnostic in &parsed.diagnostics {
        assert!(diagnostic.span.unwrap().is_valid_for(&input));
    }
    let good = parsed
        .module
        .items
        .iter()
        .find_map(|i| match &i.kind {
            ItemKind::Function(f) if f.name == "good" => Some(f),
            _ => None,
        })
        .expect("a later valid declaration must survive recovery");
    assert_eq!(
        &input.text[good.span.start..good.span.end],
        "fn good() -> i32 => 7\r\n"
    );
    assert!(
        zil_syntax::format::format(&input).is_err(),
        "formatting must not silently repair malformed source"
    );
}

// An independent structural oracle for the fixture grammar. It records every
// declaration/binding/use and expression tag, excluding only byte positions.
// Unsupported oracle shapes panic instead of silently disappearing.
fn module_shape(module: &Module) -> String {
    module
        .items
        .iter()
        .map(item_shape)
        .collect::<Vec<_>>()
        .join("\n")
}
fn item_shape(item: &Item) -> String {
    assert!(item.attributes.is_empty());
    let shape = match &item.kind {
        ItemKind::Aggregate(a) => format!(
            "aggregate:{:?}:{}:{:?}:{}",
            a.kind,
            a.name,
            a.fields
                .iter()
                .map(|f| (&f.name, ty_shape(&f.ty), f.default.as_ref().map(expr_shape)))
                .collect::<Vec<_>>(),
            a.methods
                .iter()
                .map(item_shape)
                .collect::<Vec<_>>()
                .join(";")
        ),
        ItemKind::Impl(i) => format!(
            "impl:{}:{:?}:{}",
            ty_shape(&i.target),
            i.receiver_default
                .as_ref()
                .map(|p| (&p.name, p.give, ty_shape(&p.ty))),
            i.members
                .iter()
                .map(item_shape)
                .collect::<Vec<_>>()
                .join(";")
        ),
        ItemKind::Function(f) => format!(
            "fn:{}:{:?}:{:?}:{}",
            f.name,
            f.params
                .iter()
                .map(|p| (&p.name, p.give, ty_shape(&p.ty)))
                .collect::<Vec<_>>(),
            f.return_type.as_ref().map(ty_shape),
            match &f.body {
                FunctionBody::Block(block) => block_shape(block),
                FunctionBody::Expression(e) => format!("arrow:{}", expr_shape(e)),
                FunctionBody::Declaration => "declaration".into(),
            }
        ),
        other => panic!("oracle fixture item {other:?}"),
    };
    format!("public:{}:{shape}", item.public)
}
fn ty_shape(ty: &TypeRef) -> String {
    match &ty.kind {
        TypeKind::Named { path, args } if args.is_empty() => path.join("."),
        TypeKind::Reference {
            lifetime,
            mutable,
            inner,
        } => format!("ref:{lifetime:?}:{mutable}:{}", ty_shape(inner)),
        other => panic!("oracle fixture type {other:?}"),
    }
}
fn block_shape(block: &Block) -> String {
    block
        .statements
        .iter()
        .map(|statement| match &statement.kind {
            StmtKind::Binding {
                mutable,
                pattern,
                ty,
                value,
            } => {
                let PatternKind::Name(name) = &pattern.kind else {
                    panic!();
                };
                format!(
                    "bind:{mutable}:{name}:{:?}:{}",
                    ty.as_ref().map(ty_shape),
                    expr_shape(value)
                )
            }
            StmtKind::Return(value) => format!("return:{:?}", value.as_ref().map(expr_shape)),
            StmtKind::Expression(value) => format!("statement:{}", expr_shape(value)),
            other => panic!("oracle fixture statement {other:?}"),
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn expr_shape(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Literal(l) => format!("literal:{l:?}"),
        ExprKind::Name(n) => format!("name:{n}"),
        ExprKind::ReceiverMember(n) => format!("receiver:{n}"),
        ExprKind::Group(e) => format!("group:{}", expr_shape(e)),
        ExprKind::Binary { op, left, right } => {
            format!("binary:{op:?}:{}:{}", expr_shape(left), expr_shape(right))
        }
        ExprKind::Call { callee, arguments } => format!(
            "call:{}:{:?}",
            expr_shape(callee),
            arguments
                .iter()
                .map(|a| (a.spread, expr_shape(&a.value)))
                .collect::<Vec<_>>()
        ),
        ExprKind::Closure {
            move_capture,
            params,
            return_type,
            body,
        } => format!(
            "closure:{move_capture}:{:?}:{:?}:{}",
            params
                .iter()
                .map(|p| (&p.name, p.ty.as_ref().map(ty_shape)))
                .collect::<Vec<_>>(),
            return_type.as_ref().map(ty_shape),
            match body {
                ArrowBody::Expression(e) => expr_shape(e),
                ArrowBody::Block(b) => block_shape(b),
            }
        ),
        ExprKind::SyntaxIsland { path, tree } => format!("island:{path:?}:{}", tree_shape(tree)),
        other => panic!("oracle fixture expression {other:?}"),
    }
}
fn tree_shape(tree: &TokenTree) -> String {
    format!(
        "{:?}:{}",
        tree.delimiter,
        tree.children
            .iter()
            .map(|child| match child {
                TokenTreeEntry::Token { text, .. } => text.clone(),
                TokenTreeEntry::Tree(tree) => tree_shape(tree),
            })
            .collect::<Vec<_>>()
            .join("|")
    )
}
