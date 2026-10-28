//! Two persistent fields share a concrete type, never a runtime value or place.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

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
            "state200-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn compile(case: &Case, source: &str, stage: Stage, opt: Optimization) -> CompileReport {
    fs::write(case.0.join("main.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: Default::default(),
        },
        &CompileOptions {
            stop_after: stage,
            optimization: opt,
            output: (stage == Stage::Build).then(|| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for opt in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Stage::Build, opt);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let out = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(case.0.join("stdout.log"), &out.stdout).unwrap();
        fs::write(case.0.join("stderr.log"), &out.stderr).unwrap();
        fs::write(case.0.join("native.txt"), format!("optimization={opt:?}\nexit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n", out.status.code())).unwrap();
        assert_eq!(out.status.code(), Some(exit), "{label}: {out:?}");
        assert_eq!(out.stdout, stdout.as_bytes(), "{label}: {out:?}");
        assert!(out.stderr.is_empty(), "{label}: {out:?}");
    }
}
fn reject(label: &str, source: &str, category: ResultCategory, phases: &[Phase], message: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, Stage::Verify, Optimization::O0);
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert!(
        phases.contains(&report.phase),
        "{label}: {}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
const ORIGINALS: [&str; 3] = [
    include_str!("impl200_fixtures/00_single_callback_state.zil"),
    include_str!("impl200_fixtures/01_same_f_two_callback_state.zil"),
    include_str!("impl200_fixtures/02_store_of_callback_state.zil"),
];
const RESOURCE: &str = r#"extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
"#;
const ONCE: &str = r#"package State<F>{manages root:Ticket;first:F;second:F}
fn pack<F:FnOnce()->i32>(first:F,second:F)->State<F>{return State::<F>{root:Ticket{code:84},first:first,second:second}}
"#;
const MUTABLE: &str = r#"package State<F>{manages root:Ticket;first:F;second:F}
fn pack<F:FnMut(i32)->i32>(first:F,second:F)->State<F>{return State::<F>{root:Ticket{code:84},first:first,second:second}}
fn make(label:i32,seed:i32)->impl FnMut(i32)->i32{let ticket=Ticket{code:label};var n=seed;return move(delta:i32)=>{ticket.code;n+=delta;return n}}
"#;

#[test]
fn original_three_inputs_reach_the_expected_frontend_boundaries() {
    // This intentionally stops at Types, and is not safety acceptance.
    for (i, source) in ORIGINALS.iter().enumerate() {
        let case = Case::new(&format!("original-types-{i}"));
        let report = compile(&case, source, Stage::Types, Optimization::O0);
        if i < 2 {
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.phase, Phase::Types);
        } else {
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert_eq!(report.phase, Phase::Expand);
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "E-GENERIC-ARGUMENT")
            );
        }
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn original_two_positive_inputs_verify_and_execute_unchanged() {
    for (i, source) in ORIGINALS[..2].iter().enumerate() {
        native(&format!("original-native-{i}"), source, "", 0);
    }
    reject(
        "original-store",
        ORIGINALS[2],
        ResultCategory::SafeReject,
        &[Phase::Expand],
        "StoreElement",
    );
}

#[test]
fn fnmut_fields_keep_distinct_state_through_complete_moves_and_loop_exits() {
    for mixed in [false, true] {
        let generics = if mixed { "T:Copy," } else { "" };
        let data = if mixed { "T" } else { "i32" };
        let family = if mixed { "State<T,F>" } else { "State<F>" };
        let construct = if mixed { "State::<T,F>" } else { "State::<F>" };
        let prefix = format!(
            "{RESOURCE}package {family}{{manages root:Ticket;first:F;second:F;value:{data}}}fn pack<{generics}F:FnMut(&mut {data})->i32>(first:F,second:F,value:{data})->{family}{{return {construct}{{root:Ticket{{code:84}},first:first,second:second,value:value}}}}fn relay<{generics}F:FnMut(&mut {data})->i32>(state:{family})->{family}{{return state}}fn make(label:i32,seed:i32)->impl FnMut(&mut i32)->i32{{let ticket=Ticket{{code:label+32}};var n=seed;return move(value:&mut i32)=>{{ticket.code;n+=1;*value+=n;unsafe{{putchar(label)}}return n}}}}fn run<{generics}F:FnMut(&mut {data})->i32>(state:{family},count:i32)->{data}{{var current=relay(state);var i=0;while i<count{{i+=1;if i==2{{continue}}current.first(&mut current.value);current.second(&mut current.value);if i==4{{break}}}}return current.value}}"
        );
        for (count, rounds, value) in [(0, 0, 10), (1, 1, 112), (2, 1, 112), (7, 3, 322)] {
            native(
                &format!("mut-{mixed}-{count}"),
                &format!(
                    "{prefix}fn main()->i32{{return run(pack(make(65,0),make(66,100),10),{count})-{value}}}"
                ),
                &format!("{}Tab", "AB".repeat(rounds)),
                0,
            );
        }
    }
}

#[test]
fn once_and_fn_to_once_use_the_selected_field_and_clean_the_other() {
    for factory in [
        "fn make(label:i32)->impl FnOnce()->i32{let ticket=Ticket{code:label};return move()=>{let taken=give ticket;return taken.code}}",
        "fn make(label:i32)->impl Fn()->i32{let ticket=Ticket{code:label};return move()=>ticket.code}",
    ] {
        for (field, expected, output) in [("first", 65, "AXTB"), ("second", 66, "BXTA")] {
            let source = format!(
                "{RESOURCE}{ONCE}{factory}fn run<G:FnOnce()->i32>(state:State<G>)->i32{{let value=state.{field}();unsafe{{putchar(88)}}return value}}fn main()->i32{{return run(pack(make(65),make(66)))-{expected}}}"
            );
            native(&format!("once-{field}"), &source, output, 0);
        }
        native(
            "once-zero",
            &format!(
                "{RESOURCE}{ONCE}{factory}fn main()->i32{{let state=pack(make(65),make(66));return 0}}"
            ),
            "TAB",
            0,
        );
        reject(
            "once-repeat",
            &format!(
                "{RESOURCE}{ONCE}{factory}fn repeat<G:FnOnce()->i32>(state:State<G>)->i32{{state.second();return state.second()}}fn main()->i32{{return repeat(pack(make(65),make(66)))}}"
            ),
            ResultCategory::SafeReject,
            &[Phase::Expand],
            "callback field used after",
        );
        reject(
            "once-partial",
            &format!(
                "{RESOURCE}{ONCE}{factory}fn partial<G:FnOnce()->i32>(state:State<G>)->State<G>{{state.first();return state}}fn main()->i32{{let state=partial(pack(make(65),make(66)));return 0}}"
            ),
            ResultCategory::SafeReject,
            &[Phase::Expand],
            "partial callable State",
        );
    }
}

#[test]
fn actual_copy_can_fill_two_inputs_but_noncopy_cannot_duplicate_responsibility() {
    native(
        "copy-once",
        &format!(
            "{RESOURCE}{ONCE}fn make(seed:i32)->impl Fn()->i32{{return move()=>seed}}fn use_both<G:FnOnce()->i32>(state:State<G>)->i32{{return state.first()+state.second()}}fn main()->i32{{let f=make(21);return use_both(pack(f,f))-42}}"
        ),
        "T",
        0,
    );
    reject(
        "noncopy-duplicate",
        &format!(
            "{RESOURCE}{ONCE}fn make()->impl FnOnce()->i32{{let ticket=Ticket{{code:65}};return move()=>{{let taken=give ticket;return taken.code}}}}fn main()->i32{{let f=make();let state=pack(f,f);return 0}}"
        ),
        ResultCategory::SafeReject,
        &[Phase::Analyze, Phase::Verify],
        "mov",
    );
}

#[test]
fn ordinary_type_families_aliases_and_lexical_callable_names_stay_distinct() {
    let prefix = "package State<T,F>{manages bytes:ByteBuffer;first:F;second:F;value:T}fn make()->impl Fn()->i32{return move()=>1}fn pack<T:Copy,F:Fn()->i32>(a:F,b:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),first:a,second:b,value:value}}fn run<T:Copy,G:Fn()->i32>(state:State<T,G>)->T{state.first();state.second();return state.value}fn relay<U:Copy,H:Fn()->i32>(state:State<U,H>)->U{return run(state)}";
    native(
        "ordinary-families",
        &format!(
            "{prefix}struct Nest<T>{{value:T}}struct A{{n:i32}}struct B{{n:i32}}type Alias=Nest<Nest<A>>;fn main()->i32{{let a:Alias=relay(pack(make(),make(),Nest::<Nest<A>>{{value:Nest::<A>{{value:A{{n:7}}}}}}));let b=run(pack(make(),make(),Nest::<B>{{value:B{{n:11}}}}));return a.value.value.n+b.value.n-18}}"
        ),
        "",
        0,
    );
    let phantom = "package State<F,T>{manages bytes:ByteBuffer;first:F;second:F}fn make()->impl Fn()->i32{return move()=>1}fn pack<T:Copy,F:Fn()->i32>(a:F,b:F,data:T)->State<F,T>{return State::<F,T>{bytes:bytes_new(),first:a,second:b}}fn run<U:Copy,G:Fn()->i32>(state:State<G,U>)->i32{return state.first()+state.second()}";
    native(
        "phantom-f-first",
        &format!(
            "{phantom}fn main()->i32{{return run(pack(make(),make(),1))+run(pack(make(),make(),true))-4}}"
        ),
        "",
        0,
    );
    let source = ORIGINALS[1]
        .replace(
            "fn main()",
            "fn other(seed:i32)->impl Fn()->i32{return move()=>seed}\nfn main()",
        )
        .replace("make_callback(23)", "other(23)");
    reject(
        "different-factory",
        &source,
        ResultCategory::SafeReject,
        &[Phase::Types],
        "identity",
    );
}

#[test]
fn sibling_fields_remain_disjoint_while_same_field_and_parent_windows_reject() {
    native(
        "nested-sibling",
        &format!(
            "{RESOURCE}{MUTABLE}fn main()->i32{{var state=pack(make(65,0),make(66,10));return state.first(state.second(1))-11}}"
        ),
        "TAB",
        0,
    );
    native(
        "held-sibling",
        &format!(
            "{RESOURCE}{MUTABLE}fn main()->i32{{var state=pack(make(65,0),make(66,10));let first=&mut state.first;let b=state.second(1);return first(2)+b-13}}"
        ),
        "TAB",
        0,
    );
    native(
        "replace-sibling",
        &format!(
            "{RESOURCE}{MUTABLE}fn main()->i32{{var state=pack(make(65,0),make(66,10));let first=&mut state.first;state.second=make(67,20);return first(2)+state.second(1)-23}}"
        ),
        "BTAC",
        0,
    );
    for (label, body) in [
        ("same-callee", "return state.first(state.first(1))"),
        (
            "held-first",
            "let first=&mut state.first;state.first(1);return first(2)",
        ),
        ("parent-move", "return state.first(consume(state))"),
    ] {
        reject(
            label,
            &format!(
                "{RESOURCE}{MUTABLE}fn consume<G:FnMut(i32)->i32>(state:State<G>)->i32{{return 1}}fn main()->i32{{var state=pack(make(65,0),make(66,10));{body}}}"
            ),
            ResultCategory::SafeReject,
            &[Phase::Verify],
            "address",
        );
    }
}

#[test]
fn constructor_arguments_prepare_once_and_only_acquired_values_clean_on_error() {
    let prefix = format!(
        "{RESOURCE}enum E{{Fail}}fn fail()->E!i32{{unsafe{{putchar(80)}}return E.Fail}}package State<T,F>{{manages root:Ticket;first:F;second:F;value:T}}fn pack<T:Copy,F:Fn()->i32>(a:F,b:F,value:T)->State<T,F>{{return State::<T,F>{{root:Ticket{{code:84}},first:a,second:b,value:value}}}}fn make(label:i32)->impl Fn()->i32{{let ticket=Ticket{{code:label}};return move()=>ticket.code}}"
    );
    native(
        "second-prepare-error",
        &format!("{prefix}fn main()->E!i32{{let state=pack(make(65),make(fail()?),0);return 0}}"),
        "PA",
        1,
    );
    native(
        "after-two-prepare-error",
        &format!("{prefix}fn main()->E!i32{{let state=pack(make(65),make(66),fail()?);return 0}}"),
        "PBA",
        1,
    );
    native(
        "data-argument-once",
        &format!(
            "{prefix}fn value()->i32{{unsafe{{putchar(80)}}return 7}}fn main()->i32{{let state=pack(make(65),make(66),value());return state.value-7}}"
        ),
        "PTAB",
        0,
    );
}

#[test]
fn uncalled_definitions_and_closed_owning_shapes_do_not_gain_permissions() {
    for (label, source) in [
        (
            "three-fields",
            "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;c:F;value:T}fn pack<T:Copy,F:Fn()->i32>(a:F,b:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),a:a,b:b,c:a,value:value}}fn main(){}",
        ),
        (
            "state-and-replacement",
            "package State<F>{manages bytes:ByteBuffer;a:F;b:F}fn replace<F:Fn()->i32>(state:State<F>,replacement:F)->State<F>{return state}fn main(){}",
        ),
        (
            "two-states",
            "package State<F>{manages bytes:ByteBuffer;a:F;b:F}fn combine<F:Fn()->i32>(left:State<F>,right:State<F>)->State<F>{return left}fn main(){}",
        ),
        (
            "two-types",
            "package State<F,G>{manages bytes:ByteBuffer;a:F;b:G}fn pack<F:Fn()->i32,G:Fn()->i32>(a:F,b:G)->State<F,G>{return State::<F,G>{bytes:bytes_new(),a:a,b:b}}fn main(){}",
        ),
        (
            "borrowed-inputs",
            "package State<F>{manages bytes:ByteBuffer;a:F;b:F}fn pack<F:Fn()->i32>(a:&F,b:&F)->State<F>{return State::<F>{bytes:bytes_new(),a:a,b:b}}fn main(){}",
        ),
    ] {
        reject(
            label,
            source,
            ResultCategory::Unsupported,
            &[Phase::Expand],
            "State",
        );
    }
    reject(
        "unused-copy-operation",
        "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;value:T}fn pack<T:Copy,F:Fn()->i32>(a:F,b:F,value:T)->State<T,F>{let bad=value+value;return State::<T,F>{bytes:bytes_new(),a:a,b:b,value:bad}}fn main(){}",
        ResultCategory::SafeReject,
        &[Phase::Expand],
        "Copy",
    );
    let prefix = "package State<F>{manages bytes:ByteBuffer;a:F;b:F}fn pack<F:Fn()->i32>(a:F,b:F)->State<F>{return State::<F>{bytes:bytes_new(),a:a,b:b}}";
    reject(
        "borrowed-environments",
        &format!(
            "{prefix}fn main()->i32{{let value=3;let f=()=>value;let state=pack(f,f);return 0}}"
        ),
        ResultCategory::Unsupported,
        &[Phase::Types],
        "borrowed",
    );
}

fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>, maximum: usize) -> usize {
    assert!(run(maximum).is_ok());
    let (mut low, mut high) = (0, maximum);
    while low + 1 < high {
        let n = low + (high - low) / 2;
        if run(n).is_ok() { high = n } else { low = n }
    }
    assert!(run(high).is_ok());
    assert!(
        run(high - 1)
            .unwrap_err()
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    assert!(run(high).is_ok());
    high
}
#[test]
fn field_identity_and_instance_preparation_have_actual_budget_boundaries() {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget200.zil", ORIGINALS[1]);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let types = boundary(
        |n| {
            zil_types::check(
                &core,
                &TargetProfile::host_llvm(),
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
            .map(|_| ())
        },
        Limits::default().max_analysis_steps,
    );
    let case = Case::new("budgets");
    fs::write(case.0.join("source.zil"), ORIGINALS[1]).unwrap();
    fs::write(case.0.join("boundaries.json"),format!("{{\"types\":{types},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"analyze_verify\":\"separate independent suites\"}}\n")).unwrap();
}
