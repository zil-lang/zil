//! Raw syntax callers cannot bypass the new bridge's borrowed size/depth gate.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_syntax::ast::{Expr, ExprKind, FunctionBody, ItemKind, Literal};

fn expression(text: &str) -> (SourceUnit, Expr) {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "raw219.zil",
        format!("fn query()->bool=>{text}"),
    );
    let parsed = zil_syntax::parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let ItemKind::Function(function) = &parsed.module.items[0].kind else {
        panic!()
    };
    let FunctionBody::Expression(expression) = &function.body else {
        panic!()
    };
    (source, expression.clone())
}

#[test]
fn exact_borrowed_size_and_text_limits_are_enforced_before_lowering() {
    let (source, expression) = expression("(3+4)*2==14");
    let cost = zil_expand::ct_selection::expression_cost(&expression, &Limits::default()).unwrap();
    for (nodes, work, success) in [
        (cost.nodes, cost.work, true),
        (cost.nodes - 1, cost.work, false),
        (cost.nodes, cost.work - 1, false),
    ] {
        let result = zil_expand::ct_selection::expression(
            source.module_id,
            &expression,
            &Limits {
                max_nodes: nodes,
                max_analysis_steps: work,
                ..Limits::default()
            },
        );
        if success {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit)
            );
        }
    }
    let mut large = expression.clone();
    large.kind = ExprKind::Literal(Literal::Integer("1".repeat(4096)));
    let result = zil_expand::ct_selection::expression(
        source.module_id,
        &large,
        &Limits {
            max_analysis_steps: 16,
            ..Limits::default()
        },
    );
    assert!(
        result
            .unwrap_err()
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
}

#[test]
fn raw_deep_trees_fail_as_resource_limit_without_returning_a_partial_core() {
    let (source, mut value) = expression("true");
    for _ in 0..40 {
        value = Expr {
            span: value.span,
            kind: ExprKind::Group(Box::new(value)),
        };
    }
    let low = Limits {
        max_call_depth: 20,
        ..Limits::default()
    };
    let error = zil_expand::ct_selection::expression(source.module_id, &value, &low).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    assert!(
        zil_expand::ct_selection::expression(source.module_id, &value, &Limits::default()).is_ok()
    );
}
