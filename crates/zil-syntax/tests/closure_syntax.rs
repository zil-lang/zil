use zil_base::{FileId, ModuleId, SourceUnit};
use zil_syntax::ast::*;

fn parsed(text: &str) -> Module {
    zil_syntax::parse(&SourceUnit::new(
        FileId(1),
        ModuleId(1),
        "closure.zil",
        text,
    ))
    .into_result()
    .unwrap()
}
fn function(module: &Module) -> &Function {
    let ItemKind::Function(function) = &module.items[0].kind else {
        panic!();
    };
    function
}

#[test]
fn plan_t01_003_move_arrow_and_block_closures_have_distinct_shapes() {
    let module = parsed(
        "fn f() { let a = move (x: i32) => x + 1; let b = () => { return 2 }; let c = (x) => x |> consume }",
    );
    let FunctionBody::Block(block) = &function(&module).body else {
        panic!();
    };
    let values: Vec<_> = block
        .statements
        .iter()
        .map(|statement| {
            let StmtKind::Binding { value, .. } = &statement.kind else {
                panic!();
            };
            value
        })
        .collect();
    let ExprKind::Closure {
        move_capture,
        params,
        body,
        ..
    } = &values[0].kind
    else {
        panic!();
    };
    assert!(*move_capture);
    assert_eq!(params[0].name, "x");
    assert!(params[0].ty.is_some());
    assert!(matches!(body, ArrowBody::Expression(_)));
    assert!(matches!(
        values[1].kind,
        ExprKind::Closure {
            move_capture: false,
            body: ArrowBody::Block(_),
            ..
        }
    ));
    let ExprKind::Closure {
        body: ArrowBody::Expression(value),
        ..
    } = &values[2].kind
    else {
        panic!();
    };
    assert!(matches!(value.kind, ExprKind::Pipeline { .. }));
}

#[test]
fn plan_t01_003_function_pointer_arrow_takes_complete_return_type() {
    let module = parsed(
        "type First = fn() -> i32?\ntype Second = (fn() -> i32)?\ntype Third = fn(i32) -> E!i32",
    );
    let aliases: Vec<_> = module
        .items
        .iter()
        .map(|item| {
            let ItemKind::TypeAlias(alias) = &item.kind else {
                panic!();
            };
            alias.value.as_ref().unwrap()
        })
        .collect();
    let TypeKind::Function { result, .. } = &aliases[0].kind else {
        panic!();
    };
    assert!(matches!(result.kind, TypeKind::Optional(_)));
    let TypeKind::Optional(inner) = &aliases[1].kind else {
        panic!();
    };
    let TypeKind::Group(inner) = &inner.kind else {
        panic!();
    };
    assert!(matches!(inner.kind, TypeKind::Function { .. }));
    let TypeKind::Function { result, params } = &aliases[2].kind else {
        panic!();
    };
    assert_eq!(params.len(), 1);
    assert!(matches!(result.kind, TypeKind::Error { .. }));
}

#[test]
fn plan_t01_003_impl_trait_is_only_a_function_return_production() {
    let module = parsed("fn make(n: i64) -> impl Fn() -> i64 => move () => n");
    let TypeKind::Impl(bounds) = &function(&module).return_type.as_ref().unwrap().kind else {
        panic!();
    };
    assert_eq!(bounds.len(), 1);
    assert!(matches!(
        function(&module).body,
        FunctionBody::Expression(Expr {
            kind: ExprKind::Closure {
                move_capture: true,
                ..
            },
            ..
        })
    ));
    for invalid in [
        "fn bad(f: impl Fn() -> i64) {}",
        "type Bad = impl Trait",
        "type Bad = fn() -> impl Trait",
    ] {
        let result = zil_syntax::parse(&SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "invalid.zil",
            invalid,
        ));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "E-PARSE-IDENTIFIER"),
            "{invalid}: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn explicit_match_return_paths_do_not_gain_a_dead_unit_return() {
    for (body, terminates) in [
        ("match flag {true=>{return 1},false=>{return 2}}", true),
        (
            "match flag {true=>{if flag {return 1} else if flag {return 2} else {return 3}},false=>{return 4}}",
            true,
        ),
        (
            "match flag {true=>{match flag {_=>{return 1}}},false=>{return 2}};",
            true,
        ),
        ("match flag {true=>{return 1}}", true),
        ("match flag {}", false),
        ("match flag {true=>{return 1},false=>{2}}", false),
        (
            "match flag {true=>{if flag {return 1}},false=>{return 2}}",
            false,
        ),
        ("match flag {true=>1,false=>2}", false),
    ] {
        let module = parsed(&format!("fn f() {{let f=(flag:bool)=>{{{body}}}}}"));
        let FunctionBody::Block(outer) = &function(&module).body else {
            panic!()
        };
        let StmtKind::Binding { value, .. } = &outer.statements[0].kind else {
            panic!()
        };
        let ExprKind::Closure {
            body: ArrowBody::Block(closure),
            ..
        } = &value.kind
        else {
            panic!()
        };
        assert_eq!(
            matches!(
                closure.statements.last().unwrap().kind,
                StmtKind::Return(None)
            ),
            !terminates,
            "{body}"
        );
    }
}
