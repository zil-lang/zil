//! Independent 205 review. Declared generic obligations, concrete raw safety
//! and actual native cleanup are separate checks. Unsafe negatives are never run.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
use zil_ir::*;

const ORIGINAL: &str = include_str!("impl205_independent_fixtures/00_original_state_and_replacement.zil");
const MIXED_BAD_ALIAS: &str = include_str!("impl205_independent_fixtures/01_mixed_unused_copy_alias.zil");
const OLD_BAD_ALIAS: &str = include_str!("impl205_independent_fixtures/02_single_state_unused_copy_alias.zil");
const MIXED_LAST_USE: &str = include_str!("impl205_independent_fixtures/03_mixed_unused_copy_alias_last_use.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let path = retained.clone().map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
            .join(format!("independent205-{label}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 { let _ = fs::remove_dir_all(&self.0); }
    }
}
fn compile(case: &Case, source: &str, optimization: Option<Optimization>) -> CompileReport {
    case.write("source.zil", source);
    let report = compile_source(&SourceUnit::new(FileId(0), ModuleId(0), "independent205.zil", source),
        &CompileOptions {
            stop_after: if optimization.is_some() { Stage::Build } else { Stage::Verify },
            optimization: optimization.unwrap_or(Optimization::O0),
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        });
    case.write("compile.json", report.to_json());
    report
}
fn accepted(label: &str, source: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("executable"));
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Some(optimization));
        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
        let output = Command::new(&report.artifacts["executable"]).output().unwrap();
        case.write("stdout.log", &output.stdout);
        case.write("stderr.log", &output.stderr);
        case.write("native.txt", format!("optimization={optimization:?}\nactual_exit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}
fn reject(label: &str, source: &str, phases: &[Phase], category: ResultCategory, code: Option<&str>) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert!(phases.contains(&report.phase), "{}", report.to_json());
    if let Some(code) = code { assert!(report.diagnostics.iter().any(|d| d.code == code), "{}", report.to_json()); }
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
    CoreProgram { root: core.module_id, source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule { imports: vec![], exports: core.functions.iter().filter(|f| f.name == "main").map(|f| f.id).collect(), core }] }
}
fn types_pair(label: &str, core: &CoreModule, expected: Option<&str>) {
    let case = Case::new(label);
    case.write("core.txt", format!("{core:#?}"));
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let single = zil_types::check(core, &target, &limits);
    let whole = zil_types::check_program(&program(core.clone()), &target, &limits).map(|p| p.module);
    case.write("single.txt", format!("{single:#?}"));
    case.write("program.txt", format!("{whole:#?}"));
    for result in [single, whole] {
        if let Some(code) = expected {
            let errors = result.unwrap_err();
            assert!(errors.iter().any(|d| d.phase == Phase::Types && d.category == ResultCategory::SafeReject && d.code == code), "{errors:#?}");
        } else { result.unwrap(); }
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
    let header = format!(r#"{RESOURCE}{STATE}
fn revise<T:Copy,F:Fn()->i32>(state:State<T,F>,replacement:F,left:bool)->State<T,F>{{
    var next=state;
    next.a();next.b();replacement();
    if left{{next.a=replacement}}else{{next.b=replacement}}
    return next
}}
"#);
    for (left, output, value) in [(true, "AMZCB", 154), (false, "BMZAC", 153)] {
        let source = format!("{header}fn main()->i32{{let state=revise(pack(make(65),make(66),21),make(67),{left});emit(77);return state.a()+state.b()+state.tag-{value}}}");
        native(&format!("three-distinct-{left}"), &source, output, 0);
    }
}

#[test]
fn reversed_nonadjacent_fnmut_values_preserve_three_independent_states() {
    let prefix = format!(r#"{RESOURCE}
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
"#);
    native("reversed-three-fnmut", &prefix, "PAMZCB", 0);
}

#[test]
fn declared_once_consumes_only_the_chosen_actual_position_before_next_statement() {
    let prefix = format!(r#"{RESOURCE}{STATE}
fn revise<F:FnOnce()->i32>(state:State<i32,F>,replacement:F,replace_old:bool)->State<i32,F>{{
    var next=state;
    if replace_old{{next.a();next.a=replacement}}else{{replacement()}}
    return next
}}
"#);
    for (replace_old, expected, value) in [(true, "AMZCB", 133), (false, "CMZAB", 131)] {
        let source = format!("{prefix}fn main()->i32{{let state=revise(pack(make(65),make(66),0),make(67),{replace_old});emit(77);return state.a()+state.b()-{value}}}");
        native(&format!("weaker-once-{replace_old}"), &source, expected, 0);
    }
}

#[test]
fn one_field_unit_callback_remains_an_actual_owning_replacement() {
    let source = format!(r#"{RESOURCE}
package State<F>{{manages root:Ticket;a:F}}
fn pack<F:Fn(i32)->void>(a:F)->State<F>{{return State::<F>{{root:Ticket{{code:90}},a:a}}}}
fn make(code:i32)->impl Fn(i32)->void{{let ticket=Ticket{{code:code}};return move(delta:i32)=>{{emit(ticket.code+delta)}}}}
fn revise<F:Fn(i32)->void>(state:State<F>,replacement:F)->State<F>{{var next=state;replacement(0);next.a=replacement;return next}}
fn main()->i32{{let state=revise(pack(make(65)),make(67));state.a(1);emit(77);return 0}}
"#);
    native("one-field-unit", &source, "CADMZC", 0);
}

#[test]
fn later_argument_failures_clean_prepared_state_or_replacement_without_a_field_commit() {
    let header = format!(r#"{RESOURCE}{STATE}
enum E{{Fail}}
fn fail()->E!i32{{emit(80);return E.Fail}}
fn revise<F:Fn()->i32>(state:State<i32,F>,replacement:F,mark:i32)->State<i32,F>{{var next=state;emit(88);next.a=replacement;return next}}
fn reverse<F:Fn()->i32>(replacement:F,mark:i32,state:State<i32,F>)->State<i32,F>{{var next=state;emit(88);next.a=replacement;return next}}
"#);
    for (label, expression, expected) in [
        ("replacement-not-acquired", "revise(pack(make(65),make(66),0),make(fail()?),0)", "PZAB"),
        ("state-and-replacement-acquired", "revise(pack(make(65),make(66),0),make(67),fail()?)", "PCZAB"),
        ("only-replacement-acquired", "reverse(make(67),fail()?,pack(make(65),make(66),0))", "PC"),
    ] {
        native(label, &format!("{header}fn main()->E!i32{{let state={expression};return 0}}"), expected, 1);
    }
}

#[test]
fn different_factories_are_rejected_in_both_actual_parameter_orders() {
    let prefix = format!(r#"{RESOURCE}{STATE}
fn other(code:i32)->impl Fn()->i32{{let ticket=Ticket{{code:code}};return move()=>ticket.code}}
fn forward<F:Fn()->i32>(state:State<i32,F>,replacement:F)->State<i32,F>{{var next=state;next.b=replacement;return next}}
fn reverse<F:Fn()->i32>(replacement:F,flag:bool,state:State<i32,F>)->State<i32,F>{{var next=state;next.b=replacement;return next}}
"#);
    for (label, expression) in [
        ("forward", "forward(pack(make(65),make(66),0),other(67))"),
        ("reverse", "reverse(other(67),true,pack(make(65),make(66),0))"),
    ] {
        let bad = format!("{prefix}fn main()->i32{{let state={expression};return 0}}");
        let good = bad.replace("other(67)", "make(67)");
        native(&format!("matching-factory-{label}"), &good, "BZAC", 0);
        reject(&format!("different-factory-{label}"), &bad, &[Phase::Types], ResultCategory::SafeReject, Some("E-TYPE-MISMATCH"));
    }
}

#[test]
fn unused_copy_state_alias_originals_are_checked_before_any_instance_exists() {
    accepted("mixed-alias-original-last-use", MIXED_LAST_USE);
    reject("mixed-alias-original-bad", MIXED_BAD_ALIAS, &[Phase::Expand], ResultCategory::SafeReject, None);
    reject("old-single-alias-original-bad", OLD_BAD_ALIAS, &[Phase::Expand], ResultCategory::SafeReject, None);
    let old_last_use = OLD_BAD_ALIAS.replace("    let replacement=next.a\n", "    view()\n    let replacement=next.a\n").replace("    next.a=replacement\n    view()", "    next.a=replacement");
    accepted("old-single-alias-last-use", &old_last_use);
    let old_sibling = OLD_BAD_ALIAS.replace("let replacement=next.a", "let replacement=next.b").replace("next.a=replacement", "next.b=replacement");
    accepted("old-single-alias-sibling", &old_sibling);
}

#[test]
fn raw_unused_mixed_metadata_checks_every_selector_and_both_heads() {
    let core = expanded("raw-header-source-control", ORIGINAL);
    let index = core.functions.iter().position(|f| f.name == "replace").unwrap();
    let control = &core.functions[index];
    assert_eq!(control.state_parameters.len(), 1);
    assert_eq!(control.callable_parameters.len(), 1);
    assert_eq!(control.state_parameters[0].bindings.len(), 1);
    assert_eq!(control.state_parameters[0].bindings[0].fields, vec![1,2]);
    types_pair("raw-unused-header-control", &core, None);
    for attack in ["missing-second", "reverse-fields", "duplicate-field", "ordinary-field", "out-of-range", "same-parameter", "replacement-index", "binding-index", "replacement-name", "replacement-kind", "replacement-arguments", "replacement-result", "borrowed-replacement", "extra-ordinary-binder"] {
        let mut changed = core.clone();
        let f = &mut changed.functions[index];
        match attack {
            "missing-second" => f.state_parameters[0].bindings[0].fields = vec![1],
            "reverse-fields" => f.state_parameters[0].bindings[0].fields = vec![2,1],
            "duplicate-field" => f.state_parameters[0].bindings[0].fields = vec![1,1],
            "ordinary-field" => f.state_parameters[0].bindings[0].fields = vec![0,2],
            "out-of-range" => f.state_parameters[0].bindings[0].fields = vec![1,99],
            "same-parameter" => f.callable_parameters[0].index = 0,
            "replacement-index" => f.callable_parameters[0].index = 99,
            "binding-index" => f.state_parameters[0].bindings[0].generic_index = 1,
            "replacement-name" => f.callable_parameters[0].generic_name = "G".into(),
            "replacement-kind" => f.callable_parameters[0].kind = CallableKind::FnOnce,
            "replacement-arguments" => f.callable_parameters[0].parameters.push(Type::I32),
            "replacement-result" => f.callable_parameters[0].result = Type::I64,
            "borrowed-replacement" => f.parameters[1].mode = PassingMode::Borrow,
            "extra-ordinary-binder" => f.type_parameters.push(CoreTypeParameter { index:0, name:"T".into(), bound:CoreTypeBound::StoreElement }),
            _ => unreachable!(),
        }
        types_pair(&format!("raw-header-{attack}"), &changed, Some("E-STATE-PARAMETERS"));
    }
}
