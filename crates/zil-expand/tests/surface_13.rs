use zil_base::{FileId, Limits, ModuleId, SourceUnit};
use zil_expand::{ExpandedModule, expand_with_map};
use zil_ir::*;

fn expanded(text: &str) -> ExpandedModule {
    let source = SourceUnit::new(FileId(13), ModuleId(13), "surface13.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let expanded = expand_with_map(&source, &syntax).unwrap();
    expanded
        .source_map
        .validate(&[source], &Limits::default())
        .unwrap();
    expanded
}
fn initializer(function: &CoreFunction, index: usize) -> &CoreExpr {
    let CoreStatementKind::Binding {
        initializer: Some(value),
        ..
    } = &function.body.statements[index].kind
    else {
        panic!("binding")
    };
    value
}

#[test]
fn short_closure_tail_becomes_core_return_with_source_mapping() {
    let result =
        expanded("fn test() { let f = move x => { let next = x + 1; next }; let g = () => {} }");
    let function = &result.core.functions[0];
    let CoreExprKind::Closure {
        move_capture,
        parameters,
        body,
        ..
    } = &initializer(function, 0).kind
    else {
        panic!("closure")
    };
    assert!(*move_capture);
    assert_eq!(parameters[0].name, "x");
    assert!(
        parameters[0].ty.is_none(),
        "inference is represented by absence, never an Unsupported placeholder"
    );
    assert!(
        matches!(&body.statements[1].kind, CoreStatementKind::Return(Some(CoreExpr { kind: CoreExprKind::Name(name), .. })) if name == "next")
    );
    let CoreExprKind::Closure { body, .. } = &initializer(function, 1).kind else {
        panic!("empty closure")
    };
    assert!(matches!(
        body.statements[0].kind,
        CoreStatementKind::Return(None)
    ));
}

#[test]
fn closure_tail_branches_preserve_local_scope_and_unit_fallthrough() {
    let result = expanded(
        "fn test(flag: bool) { let f = () => { if flag { let x = 1; x } else { 2 } }; let g = () => { if flag { return 3 } }; let h = () => { if flag { 4 } else { 5 }; } }",
    );
    let function = &result.core.functions[0];
    let CoreExprKind::Closure { body, .. } = &initializer(function, 0).kind else {
        panic!("closure")
    };
    let CoreStatementKind::If {
        then_body,
        else_body: Some(other),
        ..
    } = &body.statements[0].kind
    else {
        panic!("if")
    };
    assert_eq!(then_body.statements.len(), 2);
    assert!(matches!(
        then_body.statements[0].kind,
        CoreStatementKind::Binding { .. }
    ));
    assert!(matches!(
        then_body.statements[1].kind,
        CoreStatementKind::Return(Some(_))
    ));
    assert!(matches!(
        other.statements[0].kind,
        CoreStatementKind::Return(Some(_))
    ));
    let CoreExprKind::Closure { body, .. } = &initializer(function, 1).kind else {
        panic!("closure")
    };
    let CoreStatementKind::If {
        else_body: Some(other),
        ..
    } = &body.statements[0].kind
    else {
        panic!("unit else")
    };
    assert!(matches!(
        other.statements[0].kind,
        CoreStatementKind::Return(None)
    ));
    let CoreExprKind::Closure { body, .. } = &initializer(function, 2).kind else {
        panic!("closure")
    };
    assert!(matches!(
        body.statements[1].kind,
        CoreStatementKind::Return(None)
    ));
}

#[test]
fn closure_return_annotation_is_preserved_in_core_for_type_checking() {
    let result =
        expanded("fn test() { let f = (x: i32) -> bool => x; let g = move () -> i64 => 7 }");
    for (index, expected) in [(0, Type::Bool), (1, Type::I64)] {
        let CoreExprKind::Closure { return_type, .. } =
            &initializer(&result.core.functions[0], index).kind
        else {
            panic!("closure annotation must reach typed checking")
        };
        assert_eq!(return_type.as_ref(), Some(&expected));
    }
}

#[test]
fn point_record_uses_the_existing_nominal_construction_path() {
    let result = expanded(
        "struct Point { x: i32; y: i32 }\nfn origin() -> Point => .{ x: 1, y: 2 }\nfn untyped() { let bad = .{ x: 1 } }",
    );
    let origin = result
        .core
        .functions
        .iter()
        .find(|f| f.name == "origin")
        .unwrap();
    let CoreStatementKind::Return(Some(value)) = &origin.body.statements[0].kind else {
        panic!("return")
    };
    let CoreExprKind::Sequence { bindings, value } = &value.kind else {
        panic!("ordered fields")
    };
    assert_eq!(bindings.len(), 2);
    assert!(
        matches!(&value.kind, CoreExprKind::Record { ty: Type::Named(name), .. } if name == "Point")
    );
    let untyped = result
        .core
        .functions
        .iter()
        .find(|f| f.name == "untyped")
        .unwrap();
    let CoreExprKind::Sequence { value, .. } = &initializer(untyped, 0).kind else {
        panic!("unresolved record")
    };
    assert!(matches!(
        value.kind,
        CoreExprKind::Record {
            ty: Type::Unsupported(_),
            ..
        }
    ));
}
