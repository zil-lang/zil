//! Independent complete StoreElement binding review. Raw Core is untrusted;
//! qualification failures are recorded separately from independent Verify.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
use zil_ir::*;

const ORIGINAL_PUSH: &str = include_str!("impl202_fixtures/01_original_push.zil");
const ORIGINAL_FACTORY: &str = include_str!("impl202_fixtures/02_original_factory.zil");
const SHADOW: &str = include_str!("impl202_fixtures/03_shadow_nested.zil");
const PAIR: &str = include_str!("impl201_fixtures/01_pair_store_mut.zil");
const HELPERS: &str = r#"
fn empty<T:StoreElement>()->Store<T> contract{result(create)}{return store_new()}
fn reserve<T:StoreElement>(items:Store<T>,count:i64)->bool{return store_reserve(items,count)}
fn push<T:StoreElement>(items:Store<T>,value:T)->T!void{return store_push(items,value)}
fn pop<T:StoreElement>(items:Store<T>)->void!T{return store_pop(items)}
"#;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "independent202-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn compile(case: &Case, source: &str, native: Option<Optimization>) -> CompileReport {
    case.write("source.zil", source);
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "independent202.zil", source),
        &CompileOptions {
            stop_after: if native.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: native.unwrap_or(Optimization::O0),
            output: native.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn native(label: &str, source: &str, expected: &str, exit: i32) {
    for opt in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Some(opt));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        case.write("stdout.log", &output.stdout);
        case.write("stderr.log", &output.stderr);
        case.write("native.txt", format!("optimization={opt:?}\nexit={:?}\nexpected_exit={exit}\nexpected_stdout={expected:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty());
    }
}
fn reject_source(label: &str, source: &str, phases: &[Phase], code: Option<&str>) {
    let case = Case::new(label);
    let r = compile(&case, source, None);
    assert!(
        matches!(
            r.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        r.to_json()
    );
    assert!(phases.contains(&r.phase), "{}", r.to_json());
    if let Some(code) = code {
        assert!(
            r.diagnostics.iter().any(|d| d.code == code),
            "{}",
            r.to_json()
        );
    }
    assert!(!r.artifacts.contains_key("executable"));
}
fn expanded(label: &str, source: &str) -> CoreModule {
    let case = Case::new(label);
    case.write("source.zil", source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent202.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&unit, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    core.unwrap()
}
fn program(core: CoreModule) -> CoreProgram {
    CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            imports: vec![],
            exports: core
                .functions
                .iter()
                .filter(|f| f.name == "main")
                .map(|f| f.id)
                .collect(),
            core,
        }],
    }
}
fn raw_types(label: &str, core: &CoreModule, limits: &Limits, reject: bool) {
    let case = Case::new(label);
    case.write("core.txt", format!("{core:#?}"));
    case.write("limits.txt", format!("{limits:#?}"));
    let single = zil_types::check(core, &TargetProfile::host_llvm(), limits);
    case.write("single.txt", format!("{single:#?}"));
    let multiple =
        zil_types::check_program(&program(core.clone()), &TargetProfile::host_llvm(), limits);
    case.write("program.txt", format!("{multiple:#?}"));
    if reject {
        for ds in [single.unwrap_err(), multiple.unwrap_err()] {
            assert!(
                ds.iter().any(|d| d.phase == Phase::Types
                    && matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )),
                "{label}: {ds:#?}"
            );
        }
    } else {
        single.unwrap();
        multiple.unwrap();
    }
}
fn typed_control(label: &str, source: &str) -> (CoreModule, TypedModule, CandidatePlan) {
    let core = expanded(label, source);
    let case = Case::new(&format!("{label}-typed"));
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default());
    case.write("typed.txt", format!("{typed:#?}"));
    let typed = typed.unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    let plan = plan.unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    case.write("candidate.wire", &wire);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (typed.clone(), plan.clone())
    );
    let raw = zil_verify::verify_module(
        typed.clone(),
        plan.clone(),
        &typed.target,
        &Limits::default(),
    );
    let decoded = zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={decoded:#?}"));
    raw.unwrap();
    decoded.unwrap();
    (core, typed, plan)
}

#[test]
fn both_historical_helper_failures_execute_with_unchanged_source_bytes() {
    assert_eq!(ORIGINAL_PUSH.len(), 1655);
    assert_eq!(ORIGINAL_FACTORY.len(), 1657);
    // Exact SHA256 identities are recorded with the committed fixture files;
    // these are the original helpers01 bytes, not simplified replacements.
    native("original-push", ORIGINAL_PUSH, "1A2BZ", 0);
    native("original-factory", ORIGINAL_FACTORY, "1A2BZ", 0);
}

#[test]
fn caller_binders_nominal_names_and_child_binders_have_lexical_identity() {
    native("shadow-nested", SHADOW, "1A2BZ", 0);
    let inferred = SHADOW.replace("nested_empty::<Item<i32,T>>()", "nested_empty()");
    native("shadow-inferred-result", &inferred, "1A2BZ", 0);
    let no_context = SHADOW.replace(
        "let items: Store<Item<i32,T>> = nested_empty::<Item<i32,T>>()",
        "let items = empty()",
    );
    assert_ne!(no_context, SHADOW);
    reject_source(
        "empty-without-unique-type",
        &no_context,
        &[Phase::Expand, Phase::Types],
        None,
    );
}

fn nominal_pair_source(phantom: bool, reversed: bool) -> String {
    let mut prefix = PAIR[..PAIR.find("fn run<").unwrap()].to_owned();
    if phantom {
        prefix = prefix.replace("; value: T", "");
    }
    let mut body = r#"
struct Left{value:i32}struct Right{value:i32}
fn run<F:FnMut()->i32>(pair:Pair<F>)->void!i32{
let a:Store<Item<Left,F>>=empty()
let b:Store<Item<Right,F>>=empty()
if !reserve(a,1)||!reserve(b,1){return ()}
push(a,Item::<Left,F>{root:Ticket{code:49},callback:pair.first,value:Left{value:10}})
push(b,Item::<Right,F>{root:Ticket{code:50},callback:pair.second,value:Right{value:20}})
var left=pop(a)?
var right=pop(b)?
return left.callback()+right.callback()-32
}
fn main()->void!i32{return run(pair(make(65,10),make(66,20)))}
"#
    .to_owned();
    if phantom {
        body = body
            .replace(",value:Left{value:10}", "")
            .replace(",value:Right{value:20}", "");
    }
    if reversed {
        body = body.replace(
            "let a:Store<Item<Left,F>>=empty()\nlet b:Store<Item<Right,F>>=empty()",
            "let b:Store<Item<Right,F>>=empty()\nlet a:Store<Item<Left,F>>=empty()",
        );
    }
    format!("{HELPERS}{prefix}{body}")
}

#[test]
fn full_nominal_and_phantom_data_arguments_are_not_a_closure_only_instance_key() {
    for phantom in [false, true] {
        for reversed in [false, true] {
            let source = nominal_pair_source(phantom, reversed);
            native(
                &format!("nominal-phantom-{phantom}-reverse-{reversed}"),
                &source,
                "2B1AZ",
                0,
            );
        }
    }
    let wrong = nominal_pair_source(false, false)
        .replace("push(b,Item::<Right,F>", "push(a,Item::<Right,F>");
    reject_source(
        "same-closure-wrong-full-nominal",
        &wrong,
        &[Phase::Expand, Phase::Types],
        None,
    );
}

#[test]
fn actual_desc_structural_writes_survive_nested_helper_parameter_remapping() {
    native("desc-write-before-window", SHADOW, "1A2BZ", 0);
    for (label, changed) in [
        (
            "nested-reserve",
            "let first = store_at_mut(items, 0)\nshifted(0,items,4)",
        ),
        (
            "noop-reserve",
            "let first = store_at_mut(items, 0)\nshifted(0,items,2)",
        ),
        (
            "failed-reserve",
            "let first = store_at_mut(items, 0)\nshifted(0,items,-1)",
        ),
    ] {
        let source = SHADOW.replace("let first = store_at_mut(items, 0)", changed);
        reject_source(label, &source, &[Phase::Verify], Some("E-BORROW-CONFLICT"));
    }
}

#[test]
fn ordinary_templates_can_plan_independent_factory_and_callable_dependencies() {
    let prefix = r#"
fn signal()->impl Fn()->i32{return ()=>0}
fn observe<G:Fn()->i32>(callback:G)->i32{return callback()}
fn measured<U:StoreElement>(items:Store<U>)->i64{
let callback=signal()
let zero=observe(callback)
return store_len(items)+i64(zero)
}
"#;
    let source = format!(
        "{prefix}{}",
        ORIGINAL_PUSH.replace(
            "let first = store_at_mut(items, 0)",
            "if measured(items)!=2{return 9}\nlet first = store_at_mut(items, 0)"
        )
    );
    native("ordinary-factory-callable-plan", &source, "1A2BZ", 0);
}

#[test]
fn unused_declarations_do_not_gain_fields_copy_capture_or_give_from_the_bound() {
    for (label, declaration) in [
        (
            "unused-field",
            "fn invalid<T:StoreElement>(value:T)->i32{return value.value}",
        ),
        (
            "unused-callback",
            "fn invalid<T:StoreElement>(value:T)->i32{return value.callback()}",
        ),
        (
            "unused-give",
            "fn invalid<T:StoreElement>(items:Store<T>)->Store<T> contract{result(give)}{return give items}",
        ),
        (
            "unused-double-take",
            "fn invalid<T:StoreElement>(items:Store<T>,value:T){store_push(items,value);store_push(items,value)}",
        ),
    ] {
        reject_source(
            label,
            &format!("{declaration}\n{ORIGINAL_PUSH}"),
            &[Phase::Expand, Phase::Types],
            None,
        );
    }
}

fn visit_block(block: &mut CoreBlock, action: &mut impl FnMut(&mut CoreExpr) -> bool) {
    use CoreStatementKind as S;
    for statement in &mut block.statements {
        match &mut statement.kind {
            S::Binding {
                initializer: Some(v),
                ..
            }
            | S::Expression(v)
            | S::Return(Some(v)) => visit_expr(v, action),
            S::Assign { target, value } => {
                visit_expr(target, action);
                visit_expr(value, action);
            }
            S::If {
                condition,
                then_body,
                else_body,
            } => {
                visit_expr(condition, action);
                visit_block(then_body, action);
                if let Some(b) = else_body {
                    visit_block(b, action);
                }
            }
            S::IfLet {
                value,
                then_body,
                else_body,
                ..
            } => {
                visit_expr(value, action);
                visit_block(then_body, action);
                if let Some(b) = else_body {
                    visit_block(b, action);
                }
            }
            S::While { condition, body } => {
                visit_expr(condition, action);
                visit_block(body, action);
            }
            S::WhileLet { value, body, .. } => {
                visit_expr(value, action);
                visit_block(body, action);
            }
            S::ForRange {
                start, end, body, ..
            } => {
                visit_expr(start, action);
                visit_expr(end, action);
                visit_block(body, action);
            }
            S::Match { value, arms } => {
                visit_expr(value, action);
                for arm in arms {
                    visit_block(&mut arm.body, action);
                }
            }
            S::Block(b) | S::Unsafe(b) => visit_block(b, action),
            _ => {}
        }
    }
}
fn visit_expr(expr: &mut CoreExpr, action: &mut impl FnMut(&mut CoreExpr) -> bool) {
    if action(expr) {
        return;
    }
    use CoreExprKind as E;
    match &mut expr.kind {
        E::Ascribed { value, .. }
        | E::IntegerConvert { value, .. }
        | E::CompileTime(value)
        | E::Try(value)
        | E::Unary { operand: value, .. } => visit_expr(value, action),
        E::Binary { left, right, .. } => {
            visit_expr(left, action);
            visit_expr(right, action);
        }
        E::Call { callee, arguments } => {
            visit_expr(callee, action);
            for a in arguments {
                visit_expr(a, action);
            }
        }
        E::TypeApply { callee, .. } => visit_expr(callee, action),
        E::Tuple(xs) | E::Array(xs) => {
            for x in xs {
                visit_expr(x, action);
            }
        }
        E::Index { object, index } => {
            visit_expr(object, action);
            visit_expr(index, action);
        }
        E::Slice { object, start, end } => {
            visit_expr(object, action);
            for x in [start, end].into_iter().flatten() {
                visit_expr(x, action);
            }
        }
        E::Member { object, .. } => visit_expr(object, action),
        E::Record { fields, .. } => {
            for (_, v) in fields {
                visit_expr(v, action);
            }
        }
        E::RecordUpdate { base, fields, .. } => {
            visit_expr(base, action);
            for (_, v) in fields {
                visit_expr(v, action);
            }
        }
        E::Sequence { bindings, value } => {
            for (_, v) in bindings {
                visit_expr(v, action);
            }
            visit_expr(value, action);
        }
        E::Closure { body, .. } => visit_block(body, action),
        E::If {
            condition,
            then_value,
            else_value,
        } => {
            visit_expr(condition, action);
            visit_expr(then_value, action);
            visit_expr(else_value, action);
        }
        _ => {}
    }
}
fn first_application(core: &mut CoreModule, mutate: impl FnOnce(&mut CoreExpr)) {
    let mut mutate = Some(mutate);
    for f in &mut core.functions {
        visit_block(&mut f.body, &mut |expr| {
            if matches!(expr.kind, CoreExprKind::TypeApply { .. })
                && let Some(mutate) = mutate.take()
            {
                mutate(expr);
                return true;
            }
            false
        });
    }
    assert!(mutate.is_none(), "source must actually retain a TypeApply");
}

#[test]
fn raw_template_headers_and_lexical_free_binders_are_checked_in_both_entries() {
    let source = format!("fn unrelated(value:i32)->i32{{return 0}}\n{ORIGINAL_PUSH}");
    let core = expanded("metadata-control", &source);
    raw_types("metadata-valid", &core, &Limits::default(), false);
    let index = core
        .functions
        .iter()
        .position(|f| !f.type_parameters.is_empty())
        .expect("retained ordinary binder");
    let callable = core
        .functions
        .iter()
        .find(|f| !f.callable_parameters.is_empty())
        .unwrap()
        .callable_parameters
        .clone();
    let opaque = core
        .functions
        .iter()
        .find_map(|f| f.callable_return.clone())
        .unwrap();
    let foreign = core
        .functions
        .iter()
        .find_map(|f| f.foreign.clone())
        .unwrap();
    for attack in 0..8 {
        let mut changed = core.clone();
        match attack {
            0 => changed.functions[index].type_parameters[0].index = 1,
            1 => {
                let p = changed.functions[index].type_parameters[0].clone();
                changed.functions[index].type_parameters.push(p);
            }
            2 => changed.functions[index].type_parameters[0].name.clear(),
            3 => changed.functions[index].callable_parameters = callable.clone(),
            4 => changed.functions[index].callable_return = Some(opaque.clone()),
            5 => changed.functions[index].foreign = Some(foreign.clone()),
            6 => {
                let mut duplicate = changed.functions[index].clone();
                duplicate.name.push_str("_different_name_same_id");
                changed.functions.push(duplicate);
            }
            7 => {
                changed.functions[index].receiver = Some(ReceiverShape {
                    ty: Type::I32,
                    mode: PassingMode::Copy,
                    implicit: true,
                })
            }
            _ => unreachable!(),
        }
        raw_types(
            &format!("header-attack-{attack}"),
            &changed,
            &Limits::default(),
            true,
        );
    }
    for free in ["T", "F"] {
        let mut changed = core.clone();
        let unrelated = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "unrelated")
            .unwrap();
        unrelated.parameters[0].ty = Type::Named(free.into());
        raw_types(
            &format!("free-binder-{free}-outside-definition"),
            &changed,
            &Limits::default(),
            true,
        );
    }
}

#[test]
fn raw_type_application_constraints_cannot_be_dropped_or_become_runtime_values() {
    let core = expanded("application-control", ORIGINAL_PUSH);
    raw_types("application-valid", &core, &Limits::default(), false);
    for attack in 0..9 {
        let mut changed = core.clone();
        first_application(&mut changed, |expr| {
            let CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } = &mut expr.kind
            else {
                unreachable!()
            };
            match attack {
                0 => type_arguments.clear(),
                1 => type_arguments.push(type_arguments[0].clone()),
                2 => type_arguments[0] = Type::Named("UnboundOutsideEveryScope".into()),
                3 => type_arguments[0] = Type::Desc(Box::new(type_arguments[0].clone())),
                4 => callee.kind = CoreExprKind::Name("putchar".into()),
                5 => callee.kind = CoreExprKind::Name("make".into()),
                6 => {
                    let inner = callee.clone();
                    callee.kind = CoreExprKind::TypeApply {
                        callee: inner,
                        type_arguments: type_arguments.clone(),
                    };
                }
                7 => expr.kind = callee.kind.clone(),
                8 => callee.kind = CoreExprKind::Name("run".into()),
                _ => unreachable!(),
            }
        });
        raw_types(
            &format!("application-attack-{attack}"),
            &changed,
            &Limits::default(),
            true,
        );
    }
    let mut changed = core.clone();
    let mut done = false;
    for f in &mut changed.functions {
        visit_block(&mut f.body, &mut |expr| {
            if !done
                && let CoreExprKind::Call { callee, .. } = &expr.kind
                && matches!(callee.kind, CoreExprKind::TypeApply { .. })
            {
                expr.kind = callee.kind.clone();
                done = true;
                return true;
            }
            false
        });
    }
    assert!(done);
    raw_types(
        "type-application-as-bare-value",
        &changed,
        &Limits::default(),
        true,
    );
}

#[test]
fn a_raw_recursive_template_cannot_reuse_an_unfinished_function_identity() {
    let core = expanded("unfinished-instance-control", ORIGINAL_PUSH);
    raw_types(
        "unfinished-instance-valid",
        &core,
        &Limits::default(),
        false,
    );
    let mut changed = core.clone();
    let mut changed_calls = 0;
    for f in &mut changed.functions {
        if f.type_parameters.len() != 1 {
            continue;
        }
        let target = f.name.clone();
        let argument = Type::Named(f.type_parameters[0].name.clone());
        visit_block(&mut f.body, &mut |expr| {
            let CoreExprKind::Call { callee, .. } = &mut expr.kind else {
                return false;
            };
            if !matches!(&callee.kind, CoreExprKind::Name(name) if name == "store_push") {
                return false;
            }
            // Existing value arguments, result and parameter modes still
            // match. Only replace the real builtin edge with this exact
            // template's own type-constrained edge, preserving caller T.
            let original = callee.clone();
            callee.kind = CoreExprKind::TypeApply {
                callee: Box::new(CoreExpr {
                    id: NodeId(910_001),
                    span: original.span,
                    kind: CoreExprKind::Name(target.clone()),
                }),
                type_arguments: vec![argument.clone()],
            };
            changed_calls += 1;
            true
        });
    }
    assert_eq!(changed_calls, 1);
    raw_types(
        "unfinished-self-instance",
        &changed,
        &Limits::default(),
        true,
    );
}

#[test]
fn raw_actual_type_constraints_cannot_select_another_same_environment_data_family() {
    let source = nominal_pair_source(true, false);
    let core = expanded("phantom-type-constraint-control", &source);
    raw_types(
        "phantom-type-constraint-valid",
        &core,
        &Limits::default(),
        false,
    );
    let constraints = applications(&core);
    let first = constraints.first().unwrap().1[0].clone();
    let other = constraints
        .iter()
        .find_map(|(_, ts)| (ts[0] != first).then(|| ts[0].clone()))
        .expect("two full phantom data families, not just one ClosureId");
    let mut changed = core.clone();
    first_application(&mut changed, |expr| {
        let CoreExprKind::TypeApply { type_arguments, .. } = &mut expr.kind else {
            unreachable!()
        };
        assert_eq!(type_arguments[0], first);
        type_arguments[0] = other;
    });
    raw_types(
        "phantom-factory-result-selects-other-family",
        &changed,
        &Limits::default(),
        true,
    );
}

#[test]
fn raw_new_type_trees_are_bounded_before_type_copy_in_single_program_and_ct() {
    let core = expanded("raw-depth-control", ORIGINAL_PUSH);
    let limits = Limits {
        max_call_depth: 32,
        ..Limits::default()
    };
    raw_types("raw-depth-valid", &core, &limits, false);
    let mut changed = core.clone();
    first_application(&mut changed, |expr| {
        let CoreExprKind::TypeApply { type_arguments, .. } = &mut expr.kind else {
            unreachable!()
        };
        let mut deep = Type::I32;
        for _ in 0..64 {
            deep = Type::Array {
                element: Box::new(deep),
                length: 1,
            };
        }
        type_arguments[0] = deep;
    });
    let case = Case::new("raw-type-tree-depth");
    case.write("core.txt", format!("{changed:#?}"));
    let single = zil_types::check(&changed, &TargetProfile::host_llvm(), &limits);
    case.write("single.txt", format!("{single:#?}"));
    let multi = zil_types::check_program(
        &program(changed.clone()),
        &TargetProfile::host_llvm(),
        &limits,
    );
    case.write("program.txt", format!("{multi:#?}"));
    let ct = zil_ct::evaluate_source_constants_with_usage(
        &changed,
        &TargetProfile::host_llvm(),
        &limits,
    );
    case.write("ct.txt", format!("{ct:#?}"));
    for ds in [single.unwrap_err(), multi.unwrap_err(), ct.unwrap_err()] {
        assert!(
            ds.iter()
                .any(|d| d.category == ResultCategory::ResourceLimit),
            "{ds:#?}"
        );
    }
}

fn applications(core: &CoreModule) -> Vec<(NodeId, Vec<Type>)> {
    let mut copy = core.clone();
    let mut found = vec![];
    for f in &mut copy.functions {
        visit_block(&mut f.body, &mut |e| {
            if let CoreExprKind::TypeApply { type_arguments, .. } = &e.kind {
                found.push((e.id, type_arguments.clone()));
            }
            false
        });
    }
    found
}

#[test]
fn runtime_ct_normalization_retains_full_type_constraints_and_stale_wire_is_rejected() {
    let (core, typed, _) = typed_control("ct-normalization-control", ORIGINAL_PUSH);
    let case = Case::new("runtime-ct-normalization");
    let evaluated = zil_ct::evaluate_source_constants_with_usage(
        &core,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    case.write("ct.txt", format!("{evaluated:#?}"));
    let (normalized, _) = evaluated.unwrap();
    assert_eq!(applications(&core), applications(&normalized));
    assert!(!applications(&normalized).is_empty());
    let before = core
        .functions
        .iter()
        .map(|f| (f.id, f.type_parameters.clone()))
        .collect::<Vec<_>>();
    let after = normalized
        .functions
        .iter()
        .map(|f| (f.id, f.type_parameters.clone()))
        .collect::<Vec<_>>();
    assert_eq!(before, after);
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    let mut bytes =
        zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 26);
    bytes[8..12].copy_from_slice(&25u32.to_le_bytes());
    case.write("stale25.wire", &bytes);
    let errors =
        zil_verify::verify_module_bytes(&bytes, &typed.target, &Limits::default()).unwrap_err();
    case.write("stale25.txt", format!("{errors:#?}"));
    assert!(errors.iter().any(|d| d.code == "E-WIRE-VERSION"));
}

fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>) -> usize {
    let (mut lo, mut hi) = (0, Limits::default().max_analysis_steps);
    run(hi).unwrap();
    while lo + 1 < hi {
        let n = lo + (hi - lo) / 2;
        match run(n) {
            Ok(()) => hi = n,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:#?}"
                );
                lo = n;
            }
        }
    }
    run(hi).unwrap();
    assert!(
        run(hi - 1)
            .unwrap_err()
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    run(hi).unwrap();
    hi
}

#[test]
fn complete_type_instantiation_and_independent_checks_have_exact_fresh_work_bounds() {
    let source = nominal_pair_source(true, true);
    let (core, typed, plan) = typed_control("full-type-budget-control", &source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent202.zil", &source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let expand = boundary(|n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let types = boundary(|n| {
        zil_types::check(
            &core,
            &typed.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let analyze = boundary(|n| {
        zil_analyze::analyze(
            &typed,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let verify = boundary(|n| {
        zil_verify::verify_module(
            typed.clone(),
            plan.clone(),
            &typed.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    Case::new("exact-full-type-budgets").write("boundaries.json",format!("{{\"expand\":{expand},\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"defaults_unchanged\":true}}\n"));
}
