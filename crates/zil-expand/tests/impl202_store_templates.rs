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
        .validate(std::slice::from_ref(&unit), &Limits::default())
        .unwrap();
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let path = std::path::PathBuf::from(root).join(format!(
            "expand202-{}-{}",
            std::process::id(),
            unit.fingerprint()
        ));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("main.zil"), &unit.text).unwrap();
        std::fs::write(path.join("core.txt"), format!("{:#?}\n", expanded.core)).unwrap();
        std::fs::write(
            path.join("source-map.txt"),
            format!("{:#?}\n", expanded.source_map),
        )
        .unwrap();
    }
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
    // 205 closes an observed unused-definition alias hole on this real
    // borrow/sibling input. Its pre-fix N1731 baseline is retained separately;
    // the new actual window walk is charged, with defaults unchanged.
    check(1938).unwrap();
    let errors = check(1937).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    check(1938).unwrap();
}

const ORDINARY: &str = r#"
resource Token { value: i32 }
impl Drop for Token { fn drop(self: Self) {} }
package Element { manages token: Token; value: i32 }
fn empty<T: StoreElement>() -> Store<T> contract { result(create) } { return store_new() }
fn reserve<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }
fn push<T: StoreElement>(items: Store<T>, value: T) -> T!void { return store_push(items, value) }
fn main() -> i32 {
    let items: Store<Element> = empty()
    if !reserve(items, 2) { return 1 }
    push(items, Element { token: Token { value: 1 }, value: 2 })
    return 0
}
"#;

fn boundary(text: &str) -> usize {
    let unit = source(text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let check = |n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let mut high = Limits::default().max_analysis_steps;
    check(high).unwrap();
    let mut low = 0;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match check(middle) {
            Ok(_) => high = middle,
            Err(diagnostics) => {
                assert!(
                    diagnostics
                        .iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{diagnostics:?}"
                );
                low = middle;
            }
        }
    }
    check(high).unwrap();
    assert!(
        check(high - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    check(high).unwrap();
    high
}

#[test]
fn ordinary_store_calls_stay_eager_with_a_measured_exact_budget() {
    let core = expand(ORDINARY);
    assert!(core.functions.iter().all(|f| f.type_parameters.is_empty()));
    assert!(
        !core
            .functions
            .iter()
            .any(|f| ["empty", "reserve", "push"].contains(&f.name.as_str()))
    );
    let n = boundary(ORDINARY);
    eprintln!("ordinary_store_expand_n={n}");
    assert_eq!(
        n, 2255,
        "same source measured on pre202 production at 9bc570bc"
    );
}

#[test]
fn empty_factory_forwards_the_outer_binder_without_rebinding_same_name() {
    for binder in ["T", "U"] {
        let outer = format!(
            "fn outer<{binder}: StoreElement>() -> Store<{binder}> contract {{ result(create) }} {{ return empty::<{binder}>() }}\n"
        );
        let text = format!(
            "{HELPERS}{outer}{}",
            PAIR.replace("= store_new()", "= outer()")
        );
        let core = expand(&text);
        let outer = function(&core, "outer");
        assert_eq!(outer.type_parameters[0].name, binder);
        assert_eq!(
            applications(&outer.body, "empty"),
            vec![&[Type::Named(binder.into())][..]]
        );
        assert_eq!(
            applications(&function(&core, "run").body, "outer"),
            vec![std::slice::from_ref(caller_element(&core))]
        );
    }
}

#[test]
fn phantom_ordinary_arguments_keep_distinct_complete_element_identities() {
    let text = format!(
        r#"{HELPERS}
resource Token {{ value: i32 }}
impl Drop for Token {{ fn drop(self: Self) {{}} }}
package State<F> {{ manages token: Token; callback: F }}
package Phantom<T,F> {{ manages token: Token; callback: F }}
fn run<F: Fn() -> i32>(state: State<F>) {{
    let a: Store<Phantom<i32,F>> = empty()
    let b: Store<Phantom<i64,F>> = empty()
}}
fn main() {{}}
"#
    );
    let core = expand(&text);
    let calls = applications(&function(&core, "run").body, "empty");
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0], calls[1]);
    let fields = |args: &[Type]| {
        let Type::Named(name) = &args[0] else {
            panic!("nominal element: {args:?}")
        };
        &core
            .aggregates
            .iter()
            .find(|a| a.name == *name)
            .unwrap()
            .fields
    };
    assert!(
        fields(calls[0])
            .iter()
            .zip(fields(calls[1]))
            .all(|(a, b)| a.name == b.name && a.ty == b.ty)
    );
}

#[test]
fn new_template_work_has_a_fresh_exact_boundary() {
    let text = format!(
        "{HELPERS}\n{}",
        PAIR.replace("= store_new()", "= empty()")
            .replace("store_reserve(items, 2)", "reserve(items, 2)")
    );
    eprintln!("deferred_store_expand_n={}", boundary(&text));
}

#[test]
fn imported_template_stays_in_its_definition_module_and_keeps_caller_type() {
    use zil_ir::CoreImport;
    let app = format!(
        "import ops\n{}",
        PAIR.replace("store_reserve(items, 2)", "ops.reserve(items, 2)")
    );
    let helper = "pub fn reserve<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }";
    let sources = [
        source(&app),
        SourceUnit::new(FileId(1), ModuleId(1), "ops.zil", helper),
    ];
    let syntax = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = [
        vec![CoreImport {
            alias: "ops".into(),
            module: ModuleId(1),
            span: syntax[0].span,
        }],
        vec![],
    ];
    let modules = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &imports,
        &Limits::default(),
    )
    .unwrap();
    let caller = &modules[0].0.core;
    let owner = &modules[1].0;
    let retained = function(&owner.core, "reserve");
    assert_header(retained);
    assert_eq!(retained.span.file, FileId(1));
    assert!(owner.exports.contains(&retained.id));
    assert!(!caller.functions.iter().any(|f| f.name == "reserve"));
    let condition = function(caller, "run")
        .body
        .statements
        .iter()
        .find_map(|s| match &s.kind {
            CoreStatementKind::If { condition, .. } => Some(condition),
            _ => None,
        })
        .unwrap();
    let CoreExprKind::Unary { operand, .. } = &condition.kind else {
        panic!("{condition:?}")
    };
    let CoreExprKind::Call { callee, .. } = &operand.kind else {
        panic!("{operand:?}")
    };
    let CoreExprKind::TypeApply {
        callee,
        type_arguments,
    } = &callee.kind
    else {
        panic!("{callee:?}")
    };
    assert_eq!(type_arguments, &vec![caller_element(caller).clone()]);
    assert!(
        matches!(&callee.kind, CoreExprKind::Member { object, member }
        if member == "reserve" && matches!(&object.kind, CoreExprKind::Name(name) if name == "ops"))
    );
    for (_, map) in &modules {
        map.validate(&sources, &Limits::default()).unwrap();
    }

    let private = SourceUnit::new(
        FileId(1),
        ModuleId(1),
        "ops.zil",
        helper.strip_prefix("pub ").unwrap(),
    );
    let private_syntax = zil_syntax::parse(&private).into_result().unwrap();
    let errors = zil_expand::expand_program_with_limits(
        &[sources[0].clone(), private],
        &[&syntax[0], &private_syntax],
        &imports,
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.code == "E-MODULE-PRIVATE" && d.category == ResultCategory::SafeReject),
        "{errors:?}"
    );
}

#[test]
fn explicit_constraints_cannot_disagree_with_real_arguments_or_result() {
    for body in [
        PAIR.replace("= store_new()", "= empty::<Item<i64,F>>()"),
        PAIR.replace(
            "store_reserve(items, 2)",
            "reserve::<Item<i64,F>>(items, 2)",
        ),
        PAIR.replace("= store_new()", "= empty::<Item<i32,F>,i32>()"),
    ] {
        let unit = source(&format!("{HELPERS}\n{body}"));
        let syntax = zil_syntax::parse(&unit).into_result().unwrap();
        let errors = zil_expand::expand(&unit, &syntax).unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Expand
                && d.category == ResultCategory::SafeReject
                && d.code == "E-GENERIC-ARGUMENT"),
            "{errors:?}"
        );
    }
}

#[test]
fn deferred_mixed_generic_head_does_not_become_a_supported_template() {
    let helpers = HELPERS.replace(
        "fn reserve<T: StoreElement>",
        "fn reserve<T: StoreElement, N: Copy>",
    );
    let text = format!(
        "{helpers}\n{}",
        PAIR.replace(
            "store_reserve(items, 2)",
            "reserve::<Item<i32,F>,i32>(items, 2)"
        )
    );
    let unit = source(&text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let errors = zil_expand::expand(&unit, &syntax).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::Unsupported
                && d.message.contains("exactly one StoreElement")),
        "{errors:?}"
    );
}

#[test]
fn five_module_consumer_preserves_all_six_ordinary_helpers() {
    use zil_ir::CoreImport;
    let entries = [
        (
            "main",
            include_str!("../../../examples/mainline/store_template202/main.zil"),
        ),
        (
            "factory",
            include_str!("../../../examples/mainline/store_template202/factory.zil"),
        ),
        (
            "state",
            include_str!("../../../examples/mainline/store_template202/state.zil"),
        ),
        (
            "portable",
            include_str!("../../../examples/mainline/store_template202/portable.zil"),
        ),
        (
            "store_helpers",
            include_str!("../../../examples/mainline/store_template202/store_helpers.zil"),
        ),
    ];
    let sources = entries
        .iter()
        .enumerate()
        .map(|(i, (name, text))| {
            SourceUnit::new(
                FileId(i as u32),
                ModuleId(i as u32),
                format!("{name}.zil"),
                *text,
            )
        })
        .collect::<Vec<_>>();
    let syntax = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let edges: &[&[usize]] = &[&[1, 2, 3], &[3], &[3, 4], &[], &[]];
    let imports = edges
        .iter()
        .enumerate()
        .map(|(owner, targets)| {
            targets
                .iter()
                .map(|&target| CoreImport {
                    alias: entries[target].0.into(),
                    module: ModuleId(target as u32),
                    span: sources[owner].span(),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let modules = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &imports,
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(modules.len(), 5);
    let helper_module = &modules[4].0;
    for name in ["empty", "reserve", "push", "pop", "len", "capacity"] {
        let helper = function(&helper_module.core, name);
        assert_header(helper);
        assert!(helper_module.exports.contains(&helper.id));
    }
    for (index, (module, map)) in modules.iter().enumerate() {
        assert!(
            module.core.unsupported.is_empty(),
            "{:?}",
            module.core.unsupported
        );
        map.validate(&sources, &Limits::default()).unwrap();
        if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
            let path = std::path::PathBuf::from(root)
                .join(format!("consumer202-expand-{}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(
                path.join(format!("{}.zil", entries[index].0)),
                &sources[index].text,
            )
            .unwrap();
            std::fs::write(
                path.join(format!("{}.core.txt", entries[index].0)),
                format!("{:#?}\n", module.core),
            )
            .unwrap();
        }
    }
}

#[test]
fn callable_binder_shadows_a_real_nominal_before_aggregate_instantiation() {
    let body = PAIR
        .replace("Item<T,F>", "Item<D,F>")
        .replace("value: T", "value: D")
        .replace("<F", "<T")
        .replace(",F", ",T")
        .replace("callback: F", "callback: T")
        .replace("first: F", "first: T")
        .replace("second: F", "second: T")
        .replace("= store_new()", "= empty::<Item<i32,T>>()")
        .replace("return run(", "return concrete(T { value: 0 }) + run(");
    let text = format!(
        "{HELPERS}\nstruct T {{ value: i32 }}\nfn concrete(value:T)->i32 {{ return value.value }}\n{body}"
    );
    let core = expand(&text);
    let run = function(&core, "run");
    assert_eq!(run.callable_parameters[0].generic_name, "T");
    let Type::Named(pair) = &run.parameters[0].ty else {
        panic!("{run:#?}")
    };
    let pair = core
        .aggregates
        .iter()
        .find(|aggregate| aggregate.name == *pair)
        .unwrap();
    assert_eq!(pair.fields[1].ty, Type::Named("T".into()));
    assert_eq!(pair.fields[2].ty, Type::Named("T".into()));
    let concrete = function(&core, "concrete");
    let Type::Named(nominal) = &concrete.parameters[0].ty else {
        panic!("{concrete:#?}")
    };
    assert_ne!(nominal, "T");
    assert!(
        core.aggregates
            .iter()
            .any(|aggregate| &aggregate.name == nominal
                && aggregate.fields.len() == 1
                && aggregate.fields[0].ty == Type::I32)
    );
    assert!(
        !core
            .aggregates
            .iter()
            .any(|aggregate| aggregate.name == "T")
    );
    assert_eq!(
        applications(&run.body, "empty"),
        vec![std::slice::from_ref(caller_element(&core))]
    );
    eprintln!("shadow_store_expand_n={}", boundary(&text));
}

const POPPED_HELPER_SOURCE: &str = r#"fn invoke<G:FnMut()->i32>(callback:&mut G)->i32{return callback()}

fn empty<T:StoreElement>()->Store<T> contract{result(create)}{return store_new()}
fn reserve<T:StoreElement>(items:Store<T>,count:i64)->bool{return store_reserve(items,count)}
fn push<T:StoreElement>(items:Store<T>,value:T)->T!void{return store_push(items,value)}
fn pop<T:StoreElement>(items:Store<T>)->void!T{return store_pop(items)}
extern "C" fn putchar(n: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package Pair<F> { manages root: Ticket; first: F; second: F }
package Item<T,F> { manages root: Ticket; callback: F; value: T }

fn make(label: i32, seed: i32) -> impl FnMut() -> i32 {
    let ticket = Ticket { code: label }
    var count = seed
    return move () => { ticket.code; count += 1; return count }
}
fn pair<F: FnMut() -> i32>(first: F, second: F) -> Pair<F> {
    return Pair::<F> { root: Ticket { code: 90 }, first: first, second: second }
}

struct Left{value:i32}struct Right{value:i32}
fn run<F:FnMut()->i32>(pair:Pair<F>)->void!i32{
let a:Store<Item<Left,F>>=empty()
let b:Store<Item<Right,F>>=empty()
if !reserve(a,1)||!reserve(b,1){return ()}
push(a,Item::<Left,F>{root:Ticket{code:49},callback:pair.first,value:Left{value:10}})
push(b,Item::<Right,F>{root:Ticket{code:50},callback:pair.second,value:Right{value:20}})
var left=pop(a)?
var right=pop(b)?
return invoke(&mut left.callback)+invoke(&mut right.callback)-32
}
fn main()->void!i32{return run(pair(make(65,10),make(66,20)))}
"#;

#[test]
fn popped_complete_element_keeps_its_callback_type_for_a_nested_helper() {
    let text = POPPED_HELPER_SOURCE;
    let core = expand(text);
    assert_eq!(
        function(&core, "run").callable_parameters[0].generic_name,
        "F"
    );
    assert_eq!(
        function(&core, "invoke").callable_parameters[0].generic_name,
        "G"
    );
    eprintln!("popped_helper_expand_n={}", boundary(text));
}

#[test]
fn payload_context_still_rejects_actual_result_and_error_side_mismatches() {
    let direct_return = POPPED_HELPER_SOURCE.replace(
        "return invoke(&mut left.callback)+invoke(&mut right.callback)-32",
        "return invoke(&mut left.callback)",
    );
    let matching_union =
        direct_return.replace("(callback:&mut G)->i32", "(callback:&mut G)->void!i32");
    expand(&matching_union);
    let bad_payload = POPPED_HELPER_SOURCE.replace(
        "fn run<F:FnMut()->i32>(pair:Pair<F>)->void!i32",
        "fn run<F:FnMut()->i32>(pair:Pair<F>)->void!bool",
    );
    let bad_error = direct_return.replace("(callback:&mut G)->i32", "(callback:&mut G)->bool!i32");
    for (label, text) in [("payload", bad_payload), ("error-side", bad_error)] {
        let unit = source(&text);
        let syntax = zil_syntax::parse(&unit).into_result().unwrap();
        let errors = zil_expand::expand(&unit, &syntax).unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Expand
                && d.category == ResultCategory::SafeReject
                && d.code == "E-GENERIC-ARGUMENT"),
            "{label}: {errors:?}"
        );
        eprintln!("popped_helper_reject_{label}={errors:?}");
    }
}
