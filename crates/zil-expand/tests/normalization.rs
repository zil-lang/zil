//! Source-to-Core conformance checks. The small event observer below tests only
//! the Core evaluation order; it is not a runtime, ownership checker, or proof.
use std::collections::{BTreeMap, BTreeSet};
use zil_base::{ExpansionRole, FileId, Limits, ModuleId, NodeId, SourceTarget, SourceUnit};
use zil_expand::{ExpandedModule, expand_with_map};
use zil_ir::*;

fn parse_expand(text: &str) -> (SourceUnit, ExpandedModule) {
    let source = SourceUnit::new(FileId(7), ModuleId(2), "normalization.zil", text);
    let parsed = zil_syntax::parse(&source);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    let expanded = expand_with_map(&source, &parsed.module).unwrap();
    expanded
        .source_map
        .validate(std::slice::from_ref(&source), &Limits::default())
        .unwrap();
    (source, expanded)
}
fn function<'a>(module: &'a CoreModule, name: &str) -> &'a CoreFunction {
    module
        .functions
        .iter()
        .find(|f| f.name == name)
        .expect("named function")
}
fn returned(function: &CoreFunction) -> &CoreExpr {
    function
        .body
        .statements
        .iter()
        .find_map(|s| match &s.kind {
            CoreStatementKind::Return(Some(v)) => Some(v),
            _ => None,
        })
        .expect("return expression")
}
fn initializer<'a>(function: &'a CoreFunction, name: &str) -> &'a CoreExpr {
    function
        .body
        .statements
        .iter()
        .find_map(|s| match &s.kind {
            CoreStatementKind::Binding {
                name: n,
                initializer: Some(v),
                ..
            } if n == name => Some(v),
            _ => None,
        })
        .expect("binding initializer")
}
fn self_field(value: &CoreExpr, field: &str) {
    let CoreExprKind::Member { object, member } = &value.kind else {
        panic!("member: {value:?}");
    };
    assert_eq!(member, field);
    assert!(matches!(&object.kind, CoreExprKind::Name(name) if name == "self"));
}

#[test]
fn range_name_and_ordered_endpoints_survive_source_expansion() {
    let (_, expanded) = parse_expand(
        "fn lower() -> i32 => 1\nfn upper() -> i32 => 4\nfn value() -> i32 { for n in (lower()..upper()) { let copy = n }\nreturn 0 }",
    );
    let statement = &function(&expanded.core, "value").body.statements[0];
    let CoreStatementKind::ForRange {
        name,
        start,
        end,
        body,
    } = &statement.kind
    else {
        panic!("expected source-derived range node: {statement:?}");
    };
    assert_eq!(name, "n");
    for (endpoint, expected) in [(start, "lower"), (end, "upper")] {
        let CoreExprKind::Call { callee, arguments } = &endpoint.kind else {
            panic!("endpoint call: {endpoint:?}");
        };
        assert!(arguments.is_empty());
        assert!(matches!(&callee.kind, CoreExprKind::Name(name) if name == expected));
    }
    assert!(start.span.start < end.span.start);
    assert_eq!(body.statements.len(), 1);
    assert!(
        matches!(&body.statements[0].kind, CoreStatementKind::Binding {
        name, initializer: Some(CoreExpr { kind: CoreExprKind::Name(value), .. }), ..
    } if name == "copy" && value == "n")
    );
}

#[test]
fn plan_t01_002_explicit_group_default_and_static_are_fixed_before_body() {
    let (_, expanded) = parse_expand(
        "struct Point { x: i32 }\nimpl Point(self: &Self) { fn read() -> i32 => .x\nfn take(self: Self) -> Self => self\nfn unused() -> i32 => 7 }\nimpl Point { fn static_value() -> i32 => 3\nfn explicit(self: &mut Self) -> i32 => .x }",
    );
    let read = function(&expanded.core, "Point.read");
    assert_eq!(read.parameters.len(), 1);
    assert_eq!(
        read.parameters[0].ty,
        Type::Address {
            mutable: false,
            inner: Box::new(Type::Named("Point".into()))
        }
    );
    assert!(read.receiver.as_ref().unwrap().implicit);
    self_field(returned(read), "x");
    let take = function(&expanded.core, "Point.take");
    assert_eq!(take.parameters[0].ty, Type::Named("Point".into()));
    assert_eq!(take.return_type, Type::Named("Point".into()));
    assert!(!take.receiver.as_ref().unwrap().implicit);
    assert!(function(&expanded.core, "Point.unused").receiver.is_some());
    assert!(
        function(&expanded.core, "Point.static_value")
            .receiver
            .is_none()
    );
    assert_eq!(
        function(&expanded.core, "Point.explicit")
            .receiver
            .as_ref()
            .unwrap()
            .mode,
        PassingMode::BorrowMut
    );
}

#[test]
fn plan_t01_002_nested_closures_inherit_lexical_receiver_only() {
    let (_, expanded) = parse_expand(
        "struct Point { x: i32, Center: i32 }\nimpl Point(self: &Self) { fn nested() { let x = 3; let cb = move () => () => .Center; let same = () => self.x; let ordinary = () => x; let pipe = 4 |> .callback } }\nfn bad() => .x",
    );
    let method = function(&expanded.core, "Point.nested");
    let CoreExprKind::Closure { body, .. } = &initializer(method, "cb").kind else {
        panic!();
    };
    let CoreStatementKind::Return(Some(inner)) = &body.statements[0].kind else {
        panic!();
    };
    let CoreExprKind::Closure { body, .. } = &inner.kind else {
        panic!();
    };
    let CoreStatementKind::Return(Some(field)) = &body.statements[0].kind else {
        panic!();
    };
    self_field(field, "Center");
    let CoreExprKind::Closure { body, .. } = &initializer(method, "ordinary").kind else {
        panic!();
    };
    assert!(
        matches!(&body.statements[0].kind, CoreStatementKind::Return(Some(CoreExpr { kind: CoreExprKind::Name(n), .. })) if n == "x")
    );
    let CoreExprKind::Sequence { value, .. } = &initializer(method, "pipe").kind else {
        panic!();
    };
    let CoreExprKind::Call { callee, .. } = &value.kind else {
        panic!();
    };
    self_field(callee, "callback");
    assert!(matches!(
        returned(function(&expanded.core, "bad")).kind,
        CoreExprKind::ReceiverMember(_)
    ));
}

#[test]
fn plan_t01_002_trait_receiver_comes_from_signature_not_body_or_order() {
    let (_, expanded) = parse_expand(
        "struct Point { x: i32 }\nimpl Read for Point { fn value() -> i32 => .x\nfn zero() -> i32 => .x }\ntrait Read { fn value(self: &Self) -> i32; fn zero() -> i32; }",
    );
    let value = function(&expanded.core, "Point.value");
    assert_eq!(value.parameters[0].name, "self");
    assert!(value.receiver.as_ref().unwrap().implicit);
    self_field(returned(value), "x");
    let zero = function(&expanded.core, "Point.zero");
    assert!(zero.receiver.is_none());
    assert!(matches!(
        returned(zero).kind,
        CoreExprKind::ReceiverMember(_)
    ));
    assert!(
        expanded
            .core
            .unsupported
            .iter()
            .any(|u| u.feature.contains("trait")),
        "shape normalization must not claim trait contract checking"
    );
}

#[test]
fn plan_t01_002_implicit_parameter_and_shorthand_map_to_real_sources() {
    let (source, expanded) = parse_expand(
        "struct Point { x: i32 }\nimpl Point(self: &Self) { fn value() -> i32 => .x }",
    );
    let method = function(&expanded.core, "Point.value");
    let entry = expanded
        .source_map
        .trace(
            &SourceTarget::Parameter {
                function: method.id,
                index: 0,
            },
            &Limits::default(),
        )
        .unwrap();
    assert_eq!(entry[0].role, ExpansionRole::ReceiverDefault);
    assert_eq!(
        &source.text[entry[0].origin.start..entry[0].origin.end],
        "self: &Self"
    );
    assert_eq!(entry[0].related, vec![method.span]);
    let member = returned(method);
    let trace = expanded
        .source_map
        .trace(&SourceTarget::Node(member.id), &Limits::default())
        .unwrap();
    assert_eq!(trace[0].role, ExpansionRole::ReceiverShorthand);
    assert_eq!(
        &source.text[trace[0].origin.start..trace[0].origin.end],
        ".x"
    );
    assert_eq!(trace[0].related[0], method.parameters[0].span);
}

#[derive(Clone, Debug)]
enum ObservedValue {
    Opaque,
    Callable(String),
}
fn observe(
    value: &CoreExpr,
    env: &mut BTreeMap<String, ObservedValue>,
    events: &mut Vec<String>,
) -> Result<ObservedValue, ()> {
    match &value.kind {
        CoreExprKind::Name(n) => Ok(env
            .get(n)
            .cloned()
            .unwrap_or_else(|| ObservedValue::Callable(n.clone()))),
        CoreExprKind::Integer(_) | CoreExprKind::Bool(_) | CoreExprKind::Unit => {
            Ok(ObservedValue::Opaque)
        }
        CoreExprKind::Sequence { bindings, value } => {
            let mut local = env.clone();
            for (name, expression) in bindings {
                let value = observe(expression, &mut local, events)?;
                local.insert(name.clone(), value);
            }
            observe(value, &mut local, events)
        }
        CoreExprKind::Call { callee, arguments } => {
            let callable = observe(callee, env, events)?;
            for argument in arguments {
                observe(argument, env, events)?;
            }
            let ObservedValue::Callable(name) = callable else {
                panic!("observer fixture requires callable result");
            };
            events.push(name.clone());
            if name == "fail" {
                return Err(());
            }
            if name == "factory" {
                Ok(ObservedValue::Callable("produced".into()))
            } else {
                Ok(ObservedValue::Opaque)
            }
        }
        CoreExprKind::RecordUpdate { base, fields, .. } => {
            observe(base, env, events)?;
            for (_, value) in fields {
                observe(value, env, events)?;
            }
            events.push("commit_update".into());
            Ok(ObservedValue::Opaque)
        }
        CoreExprKind::Record { fields, .. } => {
            for (_, value) in fields {
                observe(value, env, events)?;
            }
            events.push("construct".into());
            Ok(ObservedValue::Opaque)
        }
        other => panic!("observer deliberately does not implement {other:?}"),
    }
}

#[test]
fn plan_t01_004_pipeline_order_single_input_and_callable_factory() {
    let (_, expanded) =
        parse_expand("fn demo() { return input() |> factory(seed()) |> final_target }");
    let mut events = Vec::new();
    observe(
        returned(function(&expanded.core, "demo")),
        &mut BTreeMap::new(),
        &mut events,
    )
    .unwrap();
    assert_eq!(
        events,
        ["input", "seed", "factory", "produced", "final_target"]
    );
    let (_, failed) = parse_expand("fn demo() { return fail() |> factory(seed()) }");
    let mut events = Vec::new();
    assert!(
        observe(
            returned(function(&failed.core, "demo")),
            &mut BTreeMap::new(),
            &mut events
        )
        .is_err()
    );
    assert_eq!(events, ["fail"]);
}

#[test]
fn plan_t01_004_record_update_prepares_base_then_source_order_overrides() {
    let (_, expanded) = parse_expand(
        "struct Point { x: i32 = default_x(), y: i32 = default_y() }\nfn demo() -> Point => Point { ...base(), y: override_y(), x: override_x() }",
    );
    let mut events = Vec::new();
    observe(
        returned(function(&expanded.core, "demo")),
        &mut BTreeMap::new(),
        &mut events,
    )
    .unwrap();
    assert_eq!(
        events,
        ["base", "override_y", "override_x", "commit_update"]
    );
    assert!(
        !expanded
            .core
            .functions
            .iter()
            .any(|f| f.name.starts_with("$field_default")),
        "updates must not evaluate or materialize defaults"
    );
    let CoreExprKind::Sequence { bindings, value } =
        &returned(function(&expanded.core, "demo")).kind
    else {
        panic!();
    };
    assert_eq!(bindings.len(), 3);
    let CoreExprKind::RecordUpdate { base, fields, .. } = &value.kind else {
        panic!();
    };
    assert!(matches!(base.kind, CoreExprKind::Name(_)));
    assert!(
        fields
            .iter()
            .all(|(_, field)| matches!(field.kind, CoreExprKind::Name(_)))
    );
    assert_eq!(
        fields
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["y", "x"]
    );
}

#[test]
fn plan_t01_004_failed_override_does_not_commit_or_run_later_fields() {
    let (_, expanded) = parse_expand(
        "struct Point { x: i32, y: i32 }\nfn demo() -> Point => Point { ...base(), x: fail(), y: later() }",
    );
    let mut events = Vec::new();
    assert!(
        observe(
            returned(function(&expanded.core, "demo")),
            &mut BTreeMap::new(),
            &mut events
        )
        .is_err()
    );
    assert_eq!(events, ["base", "fail"]);
    // Cleanup of the prepared base/value is a later CFG obligation, not inferred
    // by this event observer or granted by normalization.
}

#[test]
fn record_defaults_keep_declaration_scope_and_declared_order() {
    let (_, expanded) = parse_expand(
        "struct Point { x: i32 = from_module(), y: i32, z: i32 = last_default() }\nfn demo() -> Point { let from_module = 99; return { y: explicit() } }",
    );
    let expression = returned(function(&expanded.core, "demo"));
    let CoreExprKind::Sequence { bindings, value } = &expression.kind else {
        panic!();
    };
    assert_eq!(bindings.len(), 3);
    let CoreExprKind::Call { callee, .. } = &bindings[0].1.kind else {
        panic!();
    };
    assert!(matches!(&callee.kind, CoreExprKind::Name(n) if n == "explicit"));
    let defaults: Vec<_> = bindings[1..]
        .iter()
        .map(|(_, value)| {
            let CoreExprKind::Call { callee, arguments } = &value.kind else {
                panic!();
            };
            assert!(arguments.is_empty());
            let CoreExprKind::Name(name) = &callee.kind else {
                panic!();
            };
            let helper = function(&expanded.core, name);
            assert!(helper.parameters.is_empty());
            assert!(helper.receiver.is_none());
            let CoreExprKind::Call { callee, .. } = &returned(helper).kind else {
                panic!();
            };
            let CoreExprKind::Name(name) = &callee.kind else {
                panic!();
            };
            name.as_str()
        })
        .collect();
    assert_eq!(defaults, ["from_module", "last_default"]);
    let CoreExprKind::Record { ty, fields } = &value.kind else {
        panic!();
    };
    assert_eq!(ty, &Type::Named("Point".into()));
    assert_eq!(
        fields
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["y", "x", "z"]
    );
}

#[test]
fn context_records_do_not_infer_nominal_types_from_fields_or_outer_closure_return() {
    let (_, expanded) = parse_expand(
        "struct Point { x: i32 }\nfn demo() -> Point { let cb = () => ({x: 1}); let inferred = { x: 2 }; let explicit: Point = {x: 3}; return explicit }",
    );
    let method = function(&expanded.core, "demo");
    let CoreExprKind::Closure { body, .. } = &initializer(method, "cb").kind else {
        panic!();
    };
    let CoreStatementKind::Return(Some(value)) = &body.statements[0].kind else {
        panic!();
    };
    for value in [value, initializer(method, "inferred")] {
        let CoreExprKind::Sequence { value, .. } = &value.kind else {
            panic!();
        };
        assert!(matches!(
            value.kind,
            CoreExprKind::Record {
                ty: Type::Unsupported(_),
                ..
            }
        ));
    }
    let CoreExprKind::Sequence { value, .. } = &initializer(method, "explicit").kind else {
        panic!();
    };
    assert!(
        matches!(&value.kind, CoreExprKind::Record { ty: Type::Named(name), .. } if name == "Point")
    );
}

#[test]
fn source_map_covers_unique_core_node_ids_and_is_deterministic() {
    let text = "struct Point { x: i32 = 1 }\nfn demo() -> Point { var n = 1; n += 2; let v = n |> identity; return Point { x: v } }";
    let (source, first) = parse_expand(text);
    let (_, second) = parse_expand(text);
    assert_eq!(first.core, second.core);
    assert_eq!(first.source_map, second.source_map);
    let mut ids = BTreeSet::new();
    for f in &first.core.functions {
        assert!(ids.insert(f.id));
        visit_block(&f.body, &mut |id| {
            assert!(ids.insert(id), "duplicate Core node {id:?}")
        });
    }
    for a in &first.core.aggregates {
        assert!(ids.insert(a.id));
    }
    for id in ids {
        let trace = first
            .source_map
            .trace(&SourceTarget::Node(id), &Limits::default())
            .unwrap();
        assert!(!trace.is_empty());
        assert!(trace[0].origin.is_valid_for(&source));
    }
}

fn visit_block(block: &CoreBlock, visit: &mut impl FnMut(NodeId)) {
    for statement in &block.statements {
        visit(statement.id);
        match &statement.kind {
            CoreStatementKind::Binding {
                initializer: Some(v),
                ..
            }
            | CoreStatementKind::Expression(v)
            | CoreStatementKind::Return(Some(v)) => visit_expr(v, visit),
            CoreStatementKind::Assign { target, value } => {
                visit_expr(target, visit);
                visit_expr(value, visit);
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                visit_expr(condition, visit);
                visit_block(then_body, visit);
                if let Some(body) = else_body {
                    visit_block(body, visit);
                }
            }
            CoreStatementKind::While { condition, body } => {
                visit_expr(condition, visit);
                visit_block(body, visit);
            }
            CoreStatementKind::Block(body) => visit_block(body, visit),
            _ => {}
        }
    }
}
fn visit_expr(value: &CoreExpr, visit: &mut impl FnMut(NodeId)) {
    visit(value.id);
    match &value.kind {
        CoreExprKind::Unary { operand, .. }
        | CoreExprKind::Member {
            object: operand, ..
        }
        | CoreExprKind::CompileTime(operand) => visit_expr(operand, visit),
        CoreExprKind::Binary { left, right, .. } => {
            visit_expr(left, visit);
            visit_expr(right, visit);
        }
        CoreExprKind::Call { callee, arguments } => {
            visit_expr(callee, visit);
            for a in arguments {
                visit_expr(a, visit);
            }
        }
        CoreExprKind::Tuple(values) => {
            for v in values {
                visit_expr(v, visit);
            }
        }
        CoreExprKind::Record { fields, .. } => {
            for (_, v) in fields {
                visit_expr(v, visit);
            }
        }
        CoreExprKind::RecordUpdate { base, fields, .. } => {
            visit_expr(base, visit);
            for (_, v) in fields {
                visit_expr(v, visit);
            }
        }
        CoreExprKind::Sequence { bindings, value } => {
            for (_, v) in bindings {
                visit_expr(v, visit);
            }
            visit_expr(value, visit);
        }
        CoreExprKind::Closure { body, .. } => visit_block(body, visit),
        CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            visit_expr(condition, visit);
            visit_expr(then_value, visit);
            visit_expr(else_value, visit);
        }
        _ => {}
    }
}

#[test]
fn explicit_compile_time_marker_survives_until_the_ct_stage() {
    let (_, expanded) = parse_expand("fn value() -> i32 => #(2 + 3)");
    assert!(matches!(
        returned(function(&expanded.core, "value")).kind,
        CoreExprKind::CompileTime(_)
    ));
}
