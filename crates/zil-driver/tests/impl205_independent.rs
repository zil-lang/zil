//! Independent 205 review. Declared generic obligations, concrete raw safety
//! and actual native cleanup are separate checks. Unsafe negatives are never run.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    FileId, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit, Span, TargetProfile,
};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
use zil_ir::*;

const ORIGINAL: &str =
    include_str!("impl205_independent_fixtures/00_original_state_and_replacement.zil");
const MIXED_BAD_ALIAS: &str =
    include_str!("impl205_independent_fixtures/01_mixed_unused_copy_alias.zil");
const OLD_BAD_ALIAS: &str =
    include_str!("impl205_independent_fixtures/02_single_state_unused_copy_alias.zil");
const MIXED_LAST_USE: &str =
    include_str!("impl205_independent_fixtures/03_mixed_unused_copy_alias_last_use.zil");
const OLD_NON_T_BAD_ALIAS: &str =
    include_str!("impl205_independent_fixtures/04_single_state_non_t_alias.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let path = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "independent205-{label}-{}-{}",
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
fn compile(case: &Case, source: &str, optimization: Option<Optimization>) -> CompileReport {
    case.write("source.zil", source);
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "independent205.zil", source),
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn accepted(label: &str, source: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("executable"));
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Some(optimization));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        case.write("stdout.log", &output.stdout);
        case.write("stderr.log", &output.stderr);
        case.write("native.txt", format!("optimization={optimization:?}\nactual_exit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}
fn reject(
    label: &str,
    source: &str,
    phases: &[Phase],
    category: ResultCategory,
    code: Option<&str>,
) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert!(phases.contains(&report.phase), "{}", report.to_json());
    if let Some(code) = code {
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{}",
            report.to_json()
        );
    }
    assert!(!report.artifacts.contains_key("executable"));
}
fn expanded(label: &str, source: &str) -> CoreModule {
    let case = Case::new(label);
    case.write("source.zil", source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent205.zil", source);
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
fn types_pair(label: &str, core: &CoreModule, expected: Option<&str>) {
    let case = Case::new(label);
    case.write("core.txt", format!("{core:#?}"));
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let single = zil_types::check(core, &target, &limits);
    let whole =
        zil_types::check_program(&program(core.clone()), &target, &limits).map(|p| p.module);
    case.write("single.txt", format!("{single:#?}"));
    case.write("program.txt", format!("{whole:#?}"));
    for result in [single, whole] {
        if let Some(code) = expected {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|d| d.phase == Phase::Types
                    && d.category == ResultCategory::SafeReject
                    && d.code == code),
                "{errors:#?}"
            );
        } else {
            result.unwrap();
        }
    }
}

const RESOURCE: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn emit(n:i32){unsafe{putchar(n)}}
"#;
const STATE: &str = r#"
package State<T,F>{manages root:Ticket;tag:T;a:F;b:F}
fn pack<T:Copy,F:Fn()->i32>(a:F,b:F,tag:T)->State<T,F>{
    return State::<T,F>{root:Ticket{code:90},tag:tag,a:a,b:b}
}
fn make(code:i32)->impl Fn()->i32{let ticket=Ticket{code:code};return move()=>ticket.code}
"#;

#[test]
fn original_138_bytes_become_a_checked_unused_definition() {
    assert_eq!(ORIGINAL.len(), 138);
    accepted("original-138-byte-definition", ORIGINAL);
}

#[test]
fn same_type_has_three_real_values_and_each_selected_field_cleans_once() {
    let header = format!(
        r#"{RESOURCE}{STATE}
fn revise<T:Copy,F:Fn()->i32>(state:State<T,F>,replacement:F,left:bool)->State<T,F>{{
    var next=state;
    next.a();next.b();replacement();
    if left{{next.a=replacement}}else{{next.b=replacement}}
    return next
}}
"#
    );
    for (left, output, value) in [(true, "AMZCB", 154), (false, "BMZAC", 153)] {
        let source = format!(
            "{header}fn main()->i32{{let state=revise(pack(make(65),make(66),21),make(67),{left});emit(77);return state.a()+state.b()+state.tag-{value}}}"
        );
        native(&format!("three-distinct-{left}"), &source, output, 0);
    }
}

#[test]
fn reversed_nonadjacent_fnmut_values_preserve_three_independent_states() {
    let prefix = format!(
        r#"{RESOURCE}
package State<T,F>{{manages root:Ticket;tag:T;a:F;b:F}}
fn pack<F:FnMut(i32)->i32>(a:F,b:F)->State<i32,F>{{return State::<i32,F>{{root:Ticket{{code:90}},tag:0,a:a,b:b}}}}
fn make(code:i32)->impl FnMut(i32)->i32{{let ticket=Ticket{{code:code}};var count=0;return move(delta:i32)=>{{ticket.code;count+=delta;return count}}}}
fn revise<F:FnMut(i32)->i32>(replacement:F,mark:i32,state:State<i32,F>)->State<i32,F>{{
    var next=state;var incoming=replacement;
    next.tag=next.a(1)+next.b(10)+incoming(100)+mark;
    next.a=incoming;return next
}}
fn marker()->i32{{emit(80);return 0}}
fn main()->i32{{var state=revise(make(67),marker(),pack(make(65),make(66)));emit(77);return state.a(1)+state.b(1)+state.tag-223}}
"#
    );
    native("reversed-three-fnmut", &prefix, "PAMZCB", 0);
}

#[test]
fn declared_once_consumes_only_the_chosen_actual_position_before_next_statement() {
    let prefix = format!(
        r#"{RESOURCE}{STATE}
fn revise<F:FnOnce()->i32>(state:State<i32,F>,replacement:F,replace_old:bool)->State<i32,F>{{
    var next=state;
    if replace_old{{next.a();next.a=replacement}}else{{replacement()}}
    return next
}}
"#
    );
    for (replace_old, expected, value) in [(true, "AMZCB", 133), (false, "CMZAB", 131)] {
        let source = format!(
            "{prefix}fn main()->i32{{let state=revise(pack(make(65),make(66),0),make(67),{replace_old});emit(77);return state.a()+state.b()-{value}}}"
        );
        native(&format!("weaker-once-{replace_old}"), &source, expected, 0);
    }
}

#[test]
fn one_field_unit_callback_remains_an_actual_owning_replacement() {
    let source = format!(
        r#"{RESOURCE}
package State<F>{{manages root:Ticket;a:F}}
fn pack<F:Fn(i32)->void>(a:F)->State<F>{{return State::<F>{{root:Ticket{{code:90}},a:a}}}}
fn make(code:i32)->impl Fn(i32)->void{{let ticket=Ticket{{code:code}};return move(delta:i32)=>{{emit(ticket.code+delta)}}}}
fn revise<F:Fn(i32)->void>(state:State<F>,replacement:F)->State<F>{{var next=state;replacement(0);next.a=replacement;return next}}
fn main()->i32{{let state=revise(pack(make(65)),make(67));state.a(1);emit(77);return 0}}
"#
    );
    native("one-field-unit", &source, "CADMZC", 0);
}

#[test]
fn later_argument_failures_clean_prepared_state_or_replacement_without_a_field_commit() {
    let header = format!(
        r#"{RESOURCE}{STATE}
enum E{{Fail}}
fn fail()->E!i32{{emit(80);return E.Fail}}
fn revise<F:Fn()->i32>(state:State<i32,F>,replacement:F,mark:i32)->State<i32,F>{{var next=state;emit(88);next.a=replacement;return next}}
fn reverse<F:Fn()->i32>(replacement:F,mark:i32,state:State<i32,F>)->State<i32,F>{{var next=state;emit(88);next.a=replacement;return next}}
"#
    );
    for (label, expression, expected) in [
        (
            "replacement-not-acquired",
            "revise(pack(make(65),make(66),0),make(fail()?),0)",
            "PZAB",
        ),
        (
            "state-and-replacement-acquired",
            "revise(pack(make(65),make(66),0),make(67),fail()?)",
            "PCZAB",
        ),
        (
            "only-replacement-acquired",
            "reverse(make(67),fail()?,pack(make(65),make(66),0))",
            "PC",
        ),
    ] {
        native(
            label,
            &format!("{header}fn main()->E!i32{{let state={expression};return 0}}"),
            expected,
            1,
        );
    }
}

#[test]
fn different_factories_are_rejected_in_both_actual_parameter_orders() {
    let prefix = format!(
        r#"{RESOURCE}{STATE}
fn other(code:i32)->impl Fn()->i32{{let ticket=Ticket{{code:code}};return move()=>ticket.code}}
fn forward<F:Fn()->i32>(state:State<i32,F>,replacement:F)->State<i32,F>{{var next=state;next.b=replacement;return next}}
fn reverse<F:Fn()->i32>(replacement:F,flag:bool,state:State<i32,F>)->State<i32,F>{{var next=state;next.b=replacement;return next}}
"#
    );
    for (label, expression) in [
        ("forward", "forward(pack(make(65),make(66),0),other(67))"),
        (
            "reverse",
            "reverse(other(67),true,pack(make(65),make(66),0))",
        ),
    ] {
        let bad = format!("{prefix}fn main()->i32{{let state={expression};return 0}}");
        let good = bad.replace("other(67)", "make(67)");
        native(&format!("matching-factory-{label}"), &good, "BZAC", 0);
        reject(
            &format!("different-factory-{label}"),
            &bad,
            &[Phase::Types],
            ResultCategory::SafeReject,
            Some("E-TYPE-MISMATCH"),
        );
    }
}

#[test]
fn unused_copy_state_alias_originals_are_checked_before_any_instance_exists() {
    accepted("mixed-alias-original-last-use", MIXED_LAST_USE);
    reject(
        "mixed-alias-original-bad",
        MIXED_BAD_ALIAS,
        &[Phase::Expand],
        ResultCategory::SafeReject,
        None,
    );
    reject(
        "old-single-alias-original-bad",
        OLD_BAD_ALIAS,
        &[Phase::Expand],
        ResultCategory::SafeReject,
        None,
    );
    reject(
        "old-single-non-t-alias-original-bad",
        OLD_NON_T_BAD_ALIAS,
        &[Phase::Expand],
        ResultCategory::SafeReject,
        None,
    );
    let old_last_use = OLD_BAD_ALIAS
        .replace(
            "    let replacement=next.a\n",
            "    view()\n    let replacement=next.a\n",
        )
        .replace(
            "    next.a=replacement\n    view()",
            "    next.a=replacement",
        );
    accepted("old-single-alias-last-use", &old_last_use);
    let old_sibling = OLD_BAD_ALIAS
        .replace("let replacement=next.a", "let replacement=next.b")
        .replace("next.a=replacement", "next.b=replacement");
    accepted("old-single-alias-sibling", &old_sibling);
}

#[test]
fn raw_unused_mixed_metadata_checks_every_selector_and_both_heads() {
    let core = expanded("raw-header-source-control", ORIGINAL);
    let index = core
        .functions
        .iter()
        .position(|f| f.name == "replace")
        .unwrap();
    let control = &core.functions[index];
    assert_eq!(control.state_parameters.len(), 1);
    assert_eq!(control.callable_parameters.len(), 1);
    assert_eq!(control.state_parameters[0].bindings.len(), 1);
    assert_eq!(control.state_parameters[0].bindings[0].fields, vec![1, 2]);
    types_pair("raw-unused-header-control", &core, None);
    for attack in [
        "missing-second",
        "reverse-fields",
        "duplicate-field",
        "ordinary-field",
        "out-of-range",
        "same-parameter",
        "replacement-index",
        "binding-index",
        "replacement-name",
        "replacement-kind",
        "replacement-arguments",
        "replacement-result",
        "borrowed-replacement",
        "extra-ordinary-binder",
    ] {
        let mut changed = core.clone();
        let f = &mut changed.functions[index];
        match attack {
            "missing-second" => f.state_parameters[0].bindings[0].fields = vec![1],
            "reverse-fields" => f.state_parameters[0].bindings[0].fields = vec![2, 1],
            "duplicate-field" => f.state_parameters[0].bindings[0].fields = vec![1, 1],
            "ordinary-field" => f.state_parameters[0].bindings[0].fields = vec![0, 2],
            "out-of-range" => f.state_parameters[0].bindings[0].fields = vec![1, 99],
            "same-parameter" => f.callable_parameters[0].index = 0,
            "replacement-index" => f.callable_parameters[0].index = 99,
            "binding-index" => f.state_parameters[0].bindings[0].generic_index = 1,
            "replacement-name" => f.callable_parameters[0].generic_name = "G".into(),
            "replacement-kind" => f.callable_parameters[0].kind = CallableKind::FnOnce,
            "replacement-arguments" => f.callable_parameters[0].parameters.push(Type::I32),
            "replacement-result" => f.callable_parameters[0].result = Type::I64,
            "borrowed-replacement" => f.parameters[1].mode = PassingMode::Borrow,
            "extra-ordinary-binder" => f.type_parameters.push(CoreTypeParameter {
                index: 0,
                name: "T".into(),
                bound: CoreTypeBound::StoreElement,
            }),
            _ => unreachable!(),
        }
        types_pair(
            &format!("raw-header-{attack}"),
            &changed,
            Some("E-STATE-PARAMETERS"),
        );
    }
}

fn raw_name(name: &str, id: u32, span: Span) -> CoreExpr {
    CoreExpr {
        id: NodeId(id),
        span,
        kind: CoreExprKind::Name(name.into()),
    }
}
fn raw_field(owner: &str, field: &str, id: u32, span: Span) -> CoreExpr {
    CoreExpr {
        id: NodeId(id),
        span,
        kind: CoreExprKind::Member {
            object: Box::new(raw_name(owner, id + 1, span)),
            member: field.into(),
        },
    }
}
fn raw_bind(name: &str, mutable: bool, initializer: CoreExpr, id: u32) -> CoreStatement {
    CoreStatement {
        id: NodeId(id),
        span: initializer.span,
        kind: CoreStatementKind::Binding {
            name: name.into(),
            mutable,
            ty: None,
            initializer: Some(initializer),
        },
    }
}
fn raw_assign(field: &str, value: &str, id: u32, span: Span) -> CoreStatement {
    CoreStatement {
        id: NodeId(id),
        span,
        kind: CoreStatementKind::Assign {
            target: raw_field("next", field, id + 1, span),
            value: raw_name(value, id + 3, span),
        },
    }
}
fn raw_call(value: CoreExpr, id: u32) -> CoreStatement {
    CoreStatement {
        id: NodeId(id),
        span: value.span,
        kind: CoreStatementKind::Expression(CoreExpr {
            id: NodeId(id + 1),
            span: value.span,
            kind: CoreExprKind::Call {
                callee: Box::new(value),
                arguments: vec![],
            },
        }),
    }
}
fn raw_return_next(span: Span) -> CoreStatement {
    CoreStatement {
        id: NodeId(980060),
        span,
        kind: CoreStatementKind::Return(Some(raw_name("next", 980061, span))),
    }
}
fn raw_rejection(label: &str, core: &CoreModule, codes: &[&str]) -> bool {
    let case = Case::new(label);
    case.write("core.txt", format!("{core:#?}"));
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let a = zil_types::check(core, &target, &limits);
    let b = zil_types::check_program(&program(core.clone()), &target, &limits).map(|p| p.module);
    case.write("single.txt", format!("{a:#?}"));
    case.write("program.txt", format!("{b:#?}"));
    let mut accepted = false;
    for result in [a, b] {
        match result {
            Ok(_) => accepted = true,
            Err(errors) => assert!(
                errors.iter().any(|d| d.phase == Phase::Types
                    && d.category == ResultCategory::SafeReject
                    && codes.contains(&d.code.as_str())),
                "{label}: {errors:#?}"
            ),
        }
    }
    accepted
}

#[test]
fn unused_raw_three_values_never_share_one_consumption_slot() {
    let original = expanded("raw-three-position-header", ORIGINAL);
    let index = original
        .functions
        .iter()
        .position(|f| f.name == "replace")
        .unwrap();
    let mut unexpectedly_accepted = Vec::new();
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let mut control = original.clone();
        let f = &mut control.functions[index];
        let span = f.span;
        f.state_parameters[0].bindings[0].kind = kind;
        f.callable_parameters[0].kind = kind;
        f.body.statements = vec![
            raw_bind("next", true, raw_name("state", 980001, span), 980000),
            raw_bind("first", false, raw_field("next", "a", 980011, span), 980010),
            raw_bind(
                "second",
                false,
                raw_field("next", "b", 980021, span),
                980020,
            ),
            raw_bind(
                "third",
                false,
                raw_name("replacement", 980031, span),
                980030,
            ),
            raw_assign("a", "first", 980040, span),
            raw_assign("b", "second", 980050, span),
            raw_return_next(span),
        ];
        types_pair(&format!("three-independent-takes-{kind:?}"), &control, None);
        for position in ["first", "second", "third", "whole-state"] {
            let mut attack = control.clone();
            let f = &mut attack.functions[index];
            let value = match position {
                "first" => raw_field("next", "a", 981001, span),
                "second" => raw_field("next", "b", 981001, span),
                "third" => raw_name("replacement", 981001, span),
                _ => raw_name("state", 981001, span),
            };
            f.body
                .statements
                .insert(4, raw_bind("repeated", false, value, 981000));
            let label = format!("repeated-real-position-{kind:?}-{position}");
            if raw_rejection(&label, &attack, &["E-STATE-MOVE", "E-STATE-ORIGIN"]) {
                unexpectedly_accepted.push(label)
            }
        }
        let mut attack = control.clone();
        let f = &mut attack.functions[index];
        f.body.statements[4] = raw_assign("a", "third", 980040, span);
        f.body.statements[5] = raw_assign("b", "third", 980050, span);
        let label = format!("publish-third-twice-{kind:?}");
        if raw_rejection(&label, &attack, &["E-STATE-MOVE", "E-STATE-ORIGIN"]) {
            unexpectedly_accepted.push(label)
        }
    }
    assert!(
        unexpectedly_accepted.is_empty(),
        "unused moved positions were accepted: {unexpectedly_accepted:?}"
    );
}

#[test]
fn unused_raw_declared_once_call_is_per_value_and_refill_changes_the_position() {
    let mut core = expanded("raw-once-header", ORIGINAL);
    let index = core
        .functions
        .iter()
        .position(|f| f.name == "replace")
        .unwrap();
    let f = &mut core.functions[index];
    let span = f.span;
    f.state_parameters[0].bindings[0].kind = CallableKind::FnOnce;
    f.callable_parameters[0].kind = CallableKind::FnOnce;
    let start = raw_bind("next", true, raw_name("state", 982001, span), 982000);
    f.body.statements = vec![
        start.clone(),
        raw_call(raw_field("next", "a", 982012, span), 982010),
        raw_assign("a", "replacement", 982020, span),
        raw_return_next(span),
    ];
    types_pair("once-field-then-replacement-refill", &core, None);
    let mut unused = core.clone();
    unused.functions[index].body.statements = vec![
        start.clone(),
        raw_call(raw_name("replacement", 982012, span), 982010),
        raw_return_next(span),
    ];
    types_pair("once-replacement-leaves-two-fields", &unused, None);
    let mut unexpectedly_accepted = Vec::new();
    for position in ["a", "b", "replacement"] {
        let mut attack = core.clone();
        let f = &mut attack.functions[index];
        let callee = |id| {
            if position == "replacement" {
                raw_name(position, id, span)
            } else {
                raw_field("next", position, id, span)
            }
        };
        f.body.statements = vec![
            start.clone(),
            raw_call(callee(982012), 982010),
            raw_call(callee(982032), 982030),
        ];
        if position != "replacement" {
            f.body
                .statements
                .push(raw_assign(position, "replacement", 982040, span));
        }
        f.body.statements.push(raw_return_next(span));
        let label = format!("once-repeated-{position}");
        if raw_rejection(&label, &attack, &["E-STATE-MOVE", "E-STATE-ONCE"]) {
            unexpectedly_accepted.push(label)
        }
    }
    assert!(
        unexpectedly_accepted.is_empty(),
        "declared Once did not consume actual values: {unexpectedly_accepted:?}"
    );
}

#[test]
fn unused_old_state_alias_in_loop_condition_cannot_survive_take_and_refill() {
    for (label, original) in [("copy-t", OLD_BAD_ALIAS), ("non-t", OLD_NON_T_BAD_ALIAS)] {
        let bad=original.replace(
            "    let replacement=next.a\n    next.a=replacement\n    view()",
            "    while view()==0 {\n        let replacement=next.a\n        next.a=replacement\n    }",
        );
        assert!(bad.contains("while view()==0"));
        let break_only = bad.replace(
            "        next.a=replacement\n",
            "        next.a=replacement\n        break\n",
        );
        accepted(
            &format!("old-alias-condition-break-only-{label}"),
            &break_only,
        );
        let sibling = bad
            .replace("let replacement=next.a", "let replacement=next.b")
            .replace("next.a=replacement", "next.b=replacement");
        accepted(&format!("old-alias-condition-sibling-{label}"), &sibling);
        reject(
            &format!("old-alias-condition-reused-{label}"),
            &bad,
            &[Phase::Expand],
            ResultCategory::SafeReject,
            None,
        );
    }
}

#[test]
fn unused_mixed_callee_alias_is_saved_before_argument_preparation() {
    let prefix = "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}\nfn revise<T:Copy,F:FnMut(i32)->i32>(state:State<T,F>,replacement:F)->State<T,F>{\nvar next=state;\n";
    let tail = "\nnext.a=replacement;return next}\nfn main(){}";
    let positive =
        format!("{prefix}let argument=next.a(0);let view=&mut next.a;view(argument);{tail}");
    let sibling = format!("{prefix}let view=&mut next.a;view(next.b(0));{tail}");
    let bad = format!("{prefix}let view=&mut next.a;view(next.a(0));{tail}");
    accepted("mixed-callee-last-use-before-borrow", &positive);
    accepted("mixed-callee-sibling-argument", &sibling);
    reject(
        "mixed-callee-conflicting-argument",
        &bad,
        &[Phase::Expand],
        ResultCategory::SafeReject,
        None,
    );
}

#[test]
fn raw_legacy_single_state_alias_cannot_depend_on_expand_having_checked_the_body() {
    let good = OLD_NON_T_BAD_ALIAS
        .replace(
            "    let replacement=next.a\n",
            "    view()\n    let replacement=next.a\n",
        )
        .replace(
            "    next.a=replacement\n    view()",
            "    next.a=replacement",
        );
    let mut core = expanded("legacy-raw-alias-last-use-control", &good);
    types_pair("legacy-raw-alias-last-use-control", &core, None);
    let f = core
        .functions
        .iter_mut()
        .find(|f| f.name == "recycle")
        .unwrap();
    assert!(f.state_parameters.is_empty());
    assert_eq!(f.callable_parameters.len(), 1);
    let call=f.body.statements.iter().position(|s|matches!(&s.kind,CoreStatementKind::Expression(CoreExpr{kind:CoreExprKind::Call{callee,..},..}) if matches!(&callee.kind,CoreExprKind::Name(name) if name=="view"))).unwrap();
    let statement = f.body.statements.remove(call);
    let end = f
        .body
        .statements
        .iter()
        .position(|s| matches!(s.kind, CoreStatementKind::Return(_)))
        .unwrap();
    f.body.statements.insert(end, statement);
    assert!(
        !raw_rejection(
            "legacy-raw-alias-after-refill",
            &core,
            &["E-STATE-ALIAS", "E-STATE-MOVE"]
        ),
        "the public Types entry accepted an unused old single-State definition with an alias used after take/refill"
    );
}

fn verify_pair(label: &str, module: &TypedModule, candidate: &CandidatePlan, accept: bool) {
    let case = Case::new(label);
    let limits = Limits::default();
    case.write("typed.txt", format!("{module:#?}"));
    case.write("candidate.txt", format!("{candidate:#?}"));
    let wire = zil_ir::wire::encode_module_candidate(module, candidate, &limits).unwrap();
    case.write("candidate.wire", &wire);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap(),
        (module.clone(), candidate.clone())
    );
    let raw = zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits);
    let bytes = zil_verify::verify_module_bytes(&wire, &module.target, &limits);
    case.write("verify.txt", format!("raw={raw:#?}\nwire={bytes:#?}"));
    for result in [raw, bytes] {
        if accept {
            result.unwrap();
        } else {
            let errors = result.unwrap_err();
            assert!(
                !errors.iter().any(|d| d.code == "E-VERIFY-PLAN-IDENTITY"),
                "{label}: identity alone does not exercise the ownership kernel: {errors:#?}"
            );
            assert!(
                errors
                    .iter()
                    .any(|d| d.phase == Phase::Verify && d.category == ResultCategory::SafeReject),
                "{label}: {errors:#?}"
            );
        }
    }
}
fn typed_control(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let core = expanded(label, source);
    let case = Case::new(&format!("{label}-typed"));
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default());
    case.write("typed.txt", format!("{typed:#?}"));
    let typed = typed.unwrap();
    let candidate = zil_analyze::analyze(&typed, &Limits::default());
    case.write("analyze.txt", format!("{candidate:#?}"));
    let candidate = candidate.unwrap();
    verify_pair(&format!("{label}-control"), &typed, &candidate, true);
    (typed, candidate)
}
fn typed_attack(label: &str, module: &TypedModule, original: &CandidatePlan) {
    let case = Case::new(&format!("{label}-fresh-producer"));
    case.write("typed.txt", format!("{module:#?}"));
    let fresh = zil_analyze::analyze(module, &Limits::default());
    case.write("analyze.txt", format!("{fresh:#?}"));
    if let Ok(candidate) = fresh {
        verify_pair(
            &format!("{label}-fresh-candidate"),
            module,
            &candidate,
            false,
        );
    }
    // Candidate fields are untrusted. Give the hostile module its matching
    // public identity so this attack reaches the independent semantic kernel.
    let mut forged = original.clone();
    forged.module_fingerprint = module.fingerprint();
    verify_pair(&format!("{label}-forged-candidate"), module, &forged, false);
}

#[test]
fn source_produced_three_positions_reject_duplicate_transfers_in_raw_and_wire() {
    let source = format!(
        r#"{RESOURCE}{STATE}
fn revise<F:Fn()->i32>(replacement:F,state:State<i32,F>)->State<i32,F>{{
    var next=state;let first=next.a;let second=next.b;
    next.a=replacement;next.b=first;return next
}}
fn main()->i32{{let state=revise(make(67),pack(make(65),make(66),0));emit(77);return state.a()+state.b()-132}}
"#
    );
    native("raw-transpose-source-control", &source, "BMZCA", 0);
    let (typed, candidate) = typed_control("raw-transpose", &source);
    let index = typed
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter(|i| matches!(i.kind, InstructionKind::StorePackageField { .. }))
                .count()
                == 2
        })
        .unwrap();
    let stores = typed.functions[index]
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .filter_map(move |(i, instruction)| match instruction.kind {
                    InstructionKind::StorePackageField { value, .. } => Some((b, i, value)),
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(stores.len(), 2);
    let mut duplicate = typed.clone();
    let InstructionKind::StorePackageField { value, .. } =
        &mut duplicate.functions[index].blocks[stores[1].0].instructions[stores[1].1].kind
    else {
        unreachable!()
    };
    *value = stores[0].2;
    typed_attack("one-ssa-published-to-two-fields", &duplicate, &candidate);
    let takes = typed.functions[index]
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .filter_map(move |(i, instruction)| match instruction.kind {
                    InstructionKind::TakePackageField { local, index } => {
                        Some((b, i, local, index))
                    }
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(takes.len(), 2);
    assert_eq!(takes[0].2, takes[1].2);
    assert_ne!(takes[0].3, takes[1].3);
    let mut repeated = typed.clone();
    let InstructionKind::TakePackageField { index: field, .. } =
        &mut repeated.functions[index].blocks[takes[1].0].instructions[takes[1].1].kind
    else {
        unreachable!()
    };
    *field = takes[0].3;
    typed_attack("same-actual-field-taken-twice", &repeated, &candidate);
    let mut access = typed.clone();
    let instruction = access.functions[index]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| i.result == Some(stores[0].2))
        .unwrap();
    let InstructionKind::TakeLocal { local } = instruction.kind else {
        panic!("{instruction:#?}")
    };
    instruction.kind = InstructionKind::Load(local);
    typed_attack(
        "borrowed-access-cannot-publish-replacement",
        &access,
        &candidate,
    );
}

#[test]
fn source_produced_replacement_must_discharge_exact_old_field_before_publication() {
    let source = format!(
        r#"{RESOURCE}{STATE}
fn revise<F:Fn()->i32>(state:State<i32,F>,replacement:F)->State<i32,F>{{var next=state;next.a=replacement;return next}}
fn main()->i32{{let state=revise(pack(make(65),make(66),0),make(67));emit(77);return state.a()+state.b()-133}}
"#
    );
    native("raw-cleanup-source-control", &source, "AMZCB", 0);
    let (typed, candidate) = typed_control("raw-cleanup", &source);
    let index = typed
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, InstructionKind::StorePackageField { .. }))
        })
        .unwrap();
    let (block, position, field) = typed.functions[index]
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .find_map(|(i, instruction)| match instruction.kind {
                    InstructionKind::DropPackageField { index, .. } => Some((b, i, index)),
                    _ => None,
                })
        })
        .unwrap();
    assert_eq!(field, 2);
    let mut missing = typed.clone();
    missing.functions[index].blocks[block]
        .instructions
        .remove(position);
    typed_attack("missing-old-field-cleanup", &missing, &candidate);
    let mut sibling = typed.clone();
    let InstructionKind::DropPackageField { index: field, .. } =
        &mut sibling.functions[index].blocks[block].instructions[position].kind
    else {
        unreachable!()
    };
    *field = 3;
    typed_attack(
        "cleanup-sibling-does-not-discharge-target",
        &sibling,
        &candidate,
    );
}

#[test]
fn nominal_and_phantom_data_identity_survives_same_f_replacement_specialization() {
    for phantom in [false, true] {
        let state = if phantom {
            STATE.replace(";tag:T", "").replace("tag:tag,", "")
        } else {
            STATE.to_owned()
        };
        let source = format!(
            r#"{RESOURCE}{state}
struct Left{{value:i32}}
struct Right{{value:i32}}
fn revise<T:Copy,F:Fn()->i32>(state:State<T,F>,replacement:F)->State<T,F>{{var next=state;next.a=replacement;return next}}
fn main()->i32{{
    let left=revise(pack(make(65),make(66),Left{{value:1}}),make(67));
    let right=revise(pack(make(68),make(69),Right{{value:1}}),make(70));
    emit(77);return left.a()+left.b()+right.a()+right.b()-272
}}
"#
        );
        native(
            &format!("nominal-phantom-{phantom}"),
            &source,
            "ADMZFEZCB",
            0,
        );
        let (typed, candidate) = typed_control(&format!("nominal-family-{phantom}"), &source);
        let instances = typed
            .functions
            .iter()
            .filter(|f| {
                f.parameters.len() == 2
                    && f.parameters
                        .iter()
                        .filter(|id| matches!(f.locals[id.0 as usize].ty, Type::Package { .. }))
                        .count()
                        == 1
                    && f.parameters
                        .iter()
                        .filter(|id| matches!(f.locals[id.0 as usize].ty, Type::Closure(_)))
                        .count()
                        == 1
            })
            .map(|f| f.id)
            .collect::<Vec<_>>();
        assert_eq!(
            instances.len(),
            2,
            "complete T must remain in each mixed instance identity"
        );
        let mut changed = typed.clone();
        let mut redirected = 0;
        for instruction in changed
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
        {
            if let InstructionKind::Call { function, .. } = &mut instruction.kind {
                if *function == instances[1] {
                    *function = instances[0];
                    redirected += 1;
                }
            }
        }
        assert_eq!(redirected, 1);
        typed_attack(
            &format!("wrong-nominal-mixed-instance-{phantom}"),
            &changed,
            &candidate,
        );
    }
}
