use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_ir::{
    CoreBlock, CoreExpr, CoreExprKind, CoreFunction, CoreModule, CoreStatementKind, CoreTypeBound,
    PassingMode, Type,
};

const PAIR: &str = include_str!("../../zil-driver/tests/impl201_fixtures/01_pair_store_mut.zil");
const HELPERS: &str = r#"
fn empty<T: StoreElement>() -> Store<T> contract { result(create) } { return store_new() }
fn reserve<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }
fn push<T: StoreElement>(items: Store<T>, value: T) -> T!void { return store_push(items, value) }
fn pop<T: StoreElement>(items: Store<T>) -> void!T { return store_pop(items) }
"#;

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "store-template202.zil", text)
}
fn expand(text: &str) -> CoreModule {
    let unit = source(text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let expanded = zil_expand::expand_with_map(&unit, &syntax).unwrap();
    assert!(
        expanded.core.unsupported.is_empty(),
        "{:?}",
        expanded.core.unsupported
    );
    expanded
        .source_map
        .validate(&[unit], &Limits::default())
        .unwrap();
    expanded.core
}
fn function<'a>(core: &'a CoreModule, name: &str) -> &'a CoreFunction {
    core.functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("missing {name}: {core:#?}"))
}
fn application<'a>(expr: &'a CoreExpr, target: &str) -> Option<&'a [Type]> {
    match &expr.kind {
        CoreExprKind::Call { callee, .. } => match &callee.kind {
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } if matches!(&callee.kind, CoreExprKind::Name(n) if n == target) => {
                Some(type_arguments)
            }
            _ => None,
        },
        CoreExprKind::Unary { operand, .. } | CoreExprKind::Try(operand) => {
            application(operand, target)
        }
        _ => None,
    }
}
fn applications<'a>(body: &'a CoreBlock, target: &str) -> Vec<&'a [Type]> {
    body.statements
        .iter()
        .filter_map(|s| match &s.kind {
            CoreStatementKind::Binding {
                initializer: Some(e),
                ..
            }
            | CoreStatementKind::Expression(e)
            | CoreStatementKind::Return(Some(e))
            | CoreStatementKind::If { condition: e, .. } => application(e, target),
            _ => None,
        })
        .collect()
}
fn element(ty: &Type) -> &Type {
    let Type::Desc(inner) = ty else {
        panic!("expected Store description: {ty:?}")
    };
    let Type::OwnedStore { element } = inner.as_ref() else {
        panic!("expected Store payload: {ty:?}")
    };
    element
}
fn caller_element(core: &CoreModule) -> &Type {
    function(core, "run")
        .body
        .statements
        .iter()
        .find_map(|s| match &s.kind {
            CoreStatementKind::Binding {
                name, ty: Some(ty), ..
            } if name == "items" => Some(element(ty)),
            _ => None,
        })
        .unwrap()
}
fn assert_header(f: &CoreFunction) {
    assert_eq!(f.type_parameters.len(), 1);
    assert_eq!(f.type_parameters[0].index, 0);
    assert_eq!(f.type_parameters[0].name, "T");
    assert_eq!(f.type_parameters[0].bound, CoreTypeBound::StoreElement);
    assert!(f.callable_parameters.is_empty());
    assert!(f.callable_return.is_none());
}

#[test]
fn original_push_source_retains_whole_element_and_one_original_template() {
    // Exact helpers01 source: SHA256 101580f8d78e661011d3b73f731c4f2f2ae376a408c1cc02c8aa072d4f627bf9.
    let text = format!(
        "{HELPERS}\n{}",
        PAIR.replace("store_push(items,", "push(items,")
    );
    let core = expand(&text);
    let push = function(&core, "push");
    assert_header(push);
    assert_eq!(element(&push.parameters[0].ty), &Type::Named("T".into()));
    assert_eq!(push.parameters[1].ty, Type::Named("T".into()));
    let calls = applications(&function(&core, "run").body, "push");
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|args| *args == [caller_element(&core).clone()])
    );
    assert_eq!(
        core.functions
            .iter()
            .filter(|f| !f.type_parameters.is_empty())
            .count(),
        1
    );
}

#[test]
fn original_empty_and_reserve_source_preserves_caller_nominal_family() {
    // Exact helpers01 source: SHA256 d8fd10675872542e99533ee23957db2c3150760a2f71ea8ff28150ad6c3a7a55.
    let text = format!(
        "{HELPERS}\n{}",
        PAIR.replace("= store_new()", "= empty()")
            .replace("store_reserve(items, 2)", "reserve(items, 2)")
    );
    let core = expand(&text);
    for name in ["empty", "reserve"] {
        let helper = function(&core, name);
        assert_header(helper);
        let calls = applications(&function(&core, "run").body, name);
        assert_eq!(calls, vec![std::slice::from_ref(caller_element(&core))]);
    }
    assert_eq!(function(&core, "empty").result_mode, PassingMode::Create);
    assert_eq!(
        element(&function(&core, "empty").return_type),
        &Type::Named("T".into())
    );
}

#[test]
fn explicit_element_constraint_and_inferred_constraint_share_the_nominal() {
    let text = format!(
        "{HELPERS}\n{}",
        PAIR.replace("= store_new()", "= empty::<Item<i32,F>>()")
            .replace(
                "store_reserve(items, 2)",
                "reserve::<Item<i32,F>>(items, 2)"
            )
    );
    let core = expand(&text);
    for name in ["empty", "reserve"] {
        assert_eq!(
            applications(&function(&core, "run").body, name),
            vec![std::slice::from_ref(caller_element(&core))]
        );
    }
}

#[test]
fn child_ordinary_t_is_forwarded_in_its_callers_scope() {
    let outer = "fn outer<T: StoreElement>(items: Store<T>, count: i64) -> bool { return reserve::<T>(items, count) }\n";
    let text = format!(
        "{HELPERS}{outer}{}",
        PAIR.replace("store_reserve(items, 2)", "outer(items, 2)")
    );
    let core = expand(&text);
    assert_header(function(&core, "outer"));
    assert_header(function(&core, "reserve"));
    assert_eq!(
        applications(&function(&core, "outer").body, "reserve"),
        vec![&[Type::Named("T".into())][..]]
    );
    assert_eq!(
        applications(&function(&core, "run").body, "outer"),
        vec![std::slice::from_ref(caller_element(&core))]
    );
}

#[test]
fn contextless_factory_and_unused_bad_definition_stay_rejected() {
    for (text, category) in [
        (
            format!("{HELPERS}fn main() {{ let items = empty() }}"),
            ResultCategory::SafeReject,
        ),
        (
            format!(
                "{HELPERS}fn bad<T: StoreElement>(x: T) -> i32 {{ return x.value }} fn main() {{}}"
            ),
            ResultCategory::Unsupported,
        ),
        (
            format!(
                "{HELPERS}fn bad<T>(items: Store<T>) -> i64 {{ return store_len(items) }} fn main() {{}}"
            ),
            ResultCategory::SafeReject,
        ),
    ] {
        let unit = source(&text);
        let parsed = zil_syntax::parse(&unit).into_result().unwrap();
        let errors = zil_expand::expand(&unit, &parsed).unwrap_err();
        assert!(
            errors.iter().all(|d| d.phase == Phase::Expand),
            "{errors:?}"
        );
        assert!(errors.iter().any(|d| d.category == category), "{errors:?}");
    }
}

#[test]
fn legacy_single_field_state_preserves_exact_expand_budget() {
    let text =
        include_str!("../../zil-driver/tests/impl197_cases/01-existing-state-f-sibling/main.zil");
    let unit = source(text);
    let parsed = zil_syntax::parse(&unit).into_result().unwrap();
    let check = |n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &parsed,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    check(1731).unwrap();
    let errors = check(1730).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    check(1731).unwrap();
}
