use zil_base::{FileId, Limits, ModuleId, SourceUnit};
use zil_syntax::ast::*;

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(13), ModuleId(13), "surface13.zil", text)
}
fn parsed(text: &str) -> Module {
    zil_syntax::parse(&source(text)).into_result().unwrap()
}
fn function(module: &Module, index: usize) -> &Function {
    let ItemKind::Function(value) = &module.items[index].kind else {
        panic!("function")
    };
    value
}
fn body(function: &Function) -> &Block {
    let FunctionBody::Block(value) = &function.body else {
        panic!("block")
    };
    value
}
fn binding(block: &Block, index: usize) -> &Expr {
    let StmtKind::Binding { value, .. } = &block.statements[index].kind else {
        panic!("binding")
    };
    value
}
fn closure_body(value: &Expr) -> &Block {
    let ExprKind::Closure {
        body: ArrowBody::Block(body),
        ..
    } = &value.kind
    else {
        panic!("closure block")
    };
    body
}
fn alias(module: &Module, index: usize) -> &TypeRef {
    let ItemKind::TypeAlias(alias) = &module.items[index].kind else {
        panic!("alias")
    };
    alias.value.as_ref().unwrap()
}

#[test]
fn short_parameters_move_and_optional_return_annotation_preserve_syntax() {
    let module = parsed(
        "fn test() { let a = x => x + 1; let b = move y => y; let c = (z: i32) -> i64 => z; let d = q -> bool => q > 0 }",
    );
    let statements = body(function(&module, 0));
    for (index, name, moved, annotated) in [
        (0, "x", false, false),
        (1, "y", true, false),
        (2, "z", false, true),
        (3, "q", false, true),
    ] {
        let ExprKind::Closure {
            move_capture,
            params,
            return_type,
            ..
        } = &binding(statements, index).kind
        else {
            panic!("closure")
        };
        assert_eq!(*move_capture, moved);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, name);
        assert_eq!(params[0].ty.is_some(), index == 2);
        assert_eq!(return_type.is_some(), annotated);
    }
}

#[test]
fn short_closures_do_not_consume_group_tuple_or_pipeline_boundaries() {
    let module = parsed(
        "fn test() { let a = (1 + 2); let b = (1, 2); let c = 3 |> (x => x + 1); let d = (x, y) => x + y }",
    );
    let block = body(function(&module, 0));
    assert!(matches!(binding(block, 0).kind, ExprKind::Group(_)));
    assert!(matches!(binding(block, 1).kind, ExprKind::Tuple(_)));
    assert!(matches!(binding(block, 2).kind, ExprKind::Pipeline { .. }));
    for text in [
        "fn bad() { let a = x: i32 => x }",
        "fn bad() { let a = x\n=> x }",
        "fn bad() { let a = 3 |> x => x }",
    ] {
        assert!(!zil_syntax::parse(&source(text)).is_ok(), "{text}");
    }
    let result = zil_syntax::parse(&source("fn bad() { let a = 3 |> x => x }"));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "E-PARSE-PIPE-LAMBDA")
    );
}

#[test]
fn callable_bound_source_parameters_become_internal_args_tuple() {
    let module = parsed(
        "fn apply<F: Fn(i32) -> i64, G: FnMut() -> void, H: FnOnce((i32, i64)) -> bool>(f: F, g: G, h: H) {}\nfn make() -> impl Fn(i32, i64) -> bool => move (a, b) => a < b\ntype Dynamic = dyn FnMut(i32) -> bool",
    );
    let apply = function(&module, 0);
    for (index, name, arity) in [(0, "Fn", 1), (1, "FnMut", 0), (2, "FnOnce", 1)] {
        let GenericParamKind::Type(bounds) = &apply.generics[index].kind else {
            panic!("bounds")
        };
        let TypeKind::Named { path, args } = &bounds[0].kind else {
            panic!("nominal callable")
        };
        assert_eq!(path, &[name]);
        assert_eq!(args.len(), 2);
        let GenericArg::Type(TypeRef {
            kind: TypeKind::Tuple(params),
            ..
        }) = &args[0]
        else {
            panic!("Args tuple")
        };
        assert_eq!(params.len(), arity);
        if index == 2 {
            assert!(matches!(params[0].kind, TypeKind::Tuple(_)));
        }
    }
    assert!(matches!(
        function(&module, 1).return_type.as_ref().unwrap().kind,
        TypeKind::Impl(_)
    ));
    let TypeKind::Dyn { path, args } = &alias(&module, 2).kind else {
        panic!("dyn callable")
    };
    assert_eq!(path, &["FnMut"]);
    assert_eq!(args.len(), 2);
}

#[test]
fn legacy_callable_tuple_spelling_has_a_specific_migration_error() {
    for text in [
        "fn f<F: Fn<(i32,), i32>>(f: F) {}",
        "fn make() -> impl FnOnce<(), i64> {}",
        "type Old = dyn FnMut<(), void>",
    ] {
        let result = zil_syntax::parse(&source(text));
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "E-PARSE-CALLABLE-SYNTAX"),
            "{text}: {:?}",
            result.diagnostics
        );
    }
}

#[test]
fn dot_record_is_explicit_record_without_inventing_an_anonymous_type() {
    let module = parsed(
        "fn test() { let a: Point = .{ x: 1 }; let make = () -> Point => .{ x: 2 }; let b = .{ ...a, x: 3 } }",
    );
    let block = body(function(&module, 0));
    for index in [0, 2] {
        let ExprKind::Record { ty, fields, .. } = &binding(block, index).kind else {
            panic!("record")
        };
        assert!(ty.is_none());
        assert_eq!(fields.len(), 1);
    }
    let ExprKind::Closure {
        body: ArrowBody::Expression(value),
        ..
    } = &binding(block, 1).kind
    else {
        panic!("expression closure")
    };
    assert!(matches!(value.kind, ExprKind::Record { .. }));
    let bad = zil_syntax::parse(&source("fn bad() { let f = () => { x: 1 } }"));
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.code == "E-PARSE-RECORD-BLOCK")
    );
}

#[test]
fn closure_tail_uses_tokens_not_comment_or_string_punctuation() {
    let module = parsed(
        "fn test() { let a = () => { 7 /* ; */ }; let b = () => { 7; }; let c = () => { \";\" }; let d = () => { 7\n; }; let e = () => {}; let f = () => { return 8; }; let g = () => { let x = 4 } }",
    );
    let block = body(function(&module, 0));
    for index in [0, 2, 5] {
        assert!(
            matches!(
                closure_body(binding(block, index))
                    .statements
                    .last()
                    .unwrap()
                    .kind,
                StmtKind::Return(Some(_))
            ),
            "index {index}"
        );
    }
    for index in [1, 3, 4, 6] {
        assert!(
            matches!(
                closure_body(binding(block, index))
                    .statements
                    .last()
                    .unwrap()
                    .kind,
                StmtKind::Return(None)
            ),
            "index {index}"
        );
    }
    assert!(matches!(
        closure_body(binding(block, 1)).statements[0].kind,
        StmtKind::Expression(_)
    ));
}

#[test]
fn only_final_if_paths_gain_returns_and_missing_else_has_unit_continuation() {
    let module = parsed(
        "fn test(flag: bool) { let a = () => { if flag { let x = 1; x } else if flag { return 2 } else { 3 } }; let b = () => { if flag { 4 } }; let c = () => { if flag { 5 } else { 6 }; }; let d = () => { if flag { 7 } else { 8 }\n9 } }",
    );
    let block = body(function(&module, 0));
    let a = closure_body(binding(block, 0));
    let StmtKind::If {
        then_body,
        else_body: Some(ElseBranch::If(nested)),
        ..
    } = &a.statements[0].kind
    else {
        panic!("tail if")
    };
    assert!(matches!(
        then_body.statements.last().unwrap().kind,
        StmtKind::Return(Some(_))
    ));
    let StmtKind::If {
        then_body,
        else_body: Some(ElseBranch::Block(other)),
        ..
    } = &nested.kind
    else {
        panic!("else if")
    };
    assert!(matches!(
        then_body.statements[0].kind,
        StmtKind::Return(Some(_))
    ));
    assert!(matches!(
        other.statements[0].kind,
        StmtKind::Return(Some(_))
    ));
    let b = closure_body(binding(block, 1));
    let StmtKind::If {
        else_body: Some(ElseBranch::Block(other)),
        ..
    } = &b.statements[0].kind
    else {
        panic!("unit else")
    };
    assert!(matches!(other.statements[0].kind, StmtKind::Return(None)));
    for index in [2, 3] {
        let value = closure_body(binding(block, index));
        let StmtKind::If { then_body, .. } = &value.statements[0].kind else {
            panic!("ordinary if")
        };
        assert!(matches!(
            then_body.statements[0].kind,
            StmtKind::Expression(_)
        ));
    }
    assert!(matches!(
        closure_body(binding(block, 2))
            .statements
            .last()
            .unwrap()
            .kind,
        StmtKind::Return(None)
    ));
    assert!(matches!(
        closure_body(binding(block, 3))
            .statements
            .last()
            .unwrap()
            .kind,
        StmtKind::Return(Some(_))
    ));
}

#[test]
fn loops_defer_and_named_function_blocks_do_not_gain_tail_returns() {
    let module = parsed(
        "fn plain() -> i32 { 5 }\nfn arrow() -> i32 => { 6 }\nfn test(flag: bool) { let a = () => { while flag { 1 } }; let b = () => { defer { 2 } } }",
    );
    for index in [0, 1] {
        assert!(matches!(
            body(function(&module, index)).statements[0].kind,
            StmtKind::Expression(_)
        ));
    }
    let block = body(function(&module, 2));
    let first = closure_body(binding(block, 0));
    let StmtKind::While { body, .. } = &first.statements[0].kind else {
        panic!("while")
    };
    assert!(matches!(body.statements[0].kind, StmtKind::Expression(_)));
    assert!(matches!(first.statements[1].kind, StmtKind::Return(None)));
    let StmtKind::Defer(deferred) = &closure_body(binding(block, 1)).statements[0].kind else {
        panic!("defer")
    };
    assert!(matches!(
        deferred.statements[0].kind,
        StmtKind::Expression(_)
    ));
}

#[test]
fn explicit_reference_and_option_parentheses_remain_distinct_while_shorthand_is_pending() {
    let module = parsed(
        "type Current = &i32?\ntype OptionalAddress = (&i32)?\ntype AddressOfOption = &(i32?)\ntype MutableOption = &mut (i64?)",
    );
    let TypeKind::Optional(inner) = &alias(&module, 0).kind else {
        panic!("existing &T? meaning")
    };
    assert!(matches!(inner.kind, TypeKind::Reference { .. }));
    let TypeKind::Optional(inner) = &alias(&module, 1).kind else {
        panic!("optional address")
    };
    assert!(matches!(inner.kind, TypeKind::Group(_)));
    for index in [2, 3] {
        let TypeKind::Reference { inner, mutable, .. } = &alias(&module, index).kind else {
            panic!("reference to option")
        };
        assert_eq!(*mutable, index == 3);
        let TypeKind::Group(inner) = &inner.kind else {
            panic!("explicit group")
        };
        assert!(matches!(inner.kind, TypeKind::Optional(_)));
    }
}

#[test]
fn closure_tail_normalization_remains_under_the_parser_work_budget() {
    let text = source("fn test(flag: bool) { let f = () => { if flag { 1 } else { 2 } } }");
    assert!(zil_syntax::parse(&text).is_ok());
    let limited = zil_syntax::parse_with_limits(
        &text,
        &Limits {
            max_nodes: 8,
            ..Limits::default()
        },
    );
    assert!(
        limited
            .diagnostics
            .iter()
            .any(|d| d.code == "E-RESOURCE-LIMIT"),
        "{:?}",
        limited.diagnostics
    );
}

#[test]
fn authored_and_migrated_conformance_sources_match_the_surface_contract() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases");
    for path in [
        "closure/CL12-desc-write-shared-environment.zil",
        "closure/CL15-opaque-owned-return.zil",
        "closure/CL16-opaque-return-local-borrow.zil",
        "closure/CL17-move-address-not-lifetime-extension.zil",
        "closure/CL18-opaque-distinct-return-types.zil",
        "closure/CL22-move-desc-does-not-own.zil",
        "closure/CL23-body-give-infers-managed-capture.zil",
        "closure/CL25-owned-close-is-not-shared-call.zil",
        "closure/CL27-short-move-tail.zil",
        "closure/CL29-explicit-result.zil",
        "closure/CL30-tail-if-branches.zil",
        "closure/CL31-unconstrained-signature.zil",
        "record/RC15-point-record.zil",
    ] {
        let text = std::fs::read_to_string(corpus.join(path)).unwrap();
        let result = zil_syntax::parse(&source(&text));
        assert!(result.is_ok(), "{path}: {:?}", result.diagnostics);
    }
    let text =
        std::fs::read_to_string(corpus.join("closure/CL28-legacy-callable-tuple.zil")).unwrap();
    let result = zil_syntax::parse(&source(&text));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "E-PARSE-CALLABLE-SYNTAX")
    );
}
