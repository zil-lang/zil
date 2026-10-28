//! Source Store elements keep each environment's real position and responsibility.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
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
            "store201-{label}-{}-{}",
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
fn compile(case: &Case, source: &str, stage: Stage, optimization: Optimization) -> CompileReport {
    fs::write(case.0.join("main.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: Default::default(),
        },
        &CompileOptions {
            stop_after: stage,
            optimization,
            output: (stage == Stage::Build).then(|| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
    report
}
fn types(label: &str, source: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, Stage::Types, Optimization::O0);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(report.verification_runs, 0);
}
fn native(label: &str, source: &str, expected_stdout: &str, expected_exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Stage::Build, optimization);
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
        fs::write(case.0.join("stdout.log"), &output.stdout).unwrap();
        fs::write(case.0.join("stderr.log"), &output.stderr).unwrap();
        fs::write(case.0.join("native.txt"), format!("optimization={optimization:?}\nexit={:?}\nexpected_exit={expected_exit}\nexpected_stdout={expected_stdout:?}\n", output.status.code())).unwrap();
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "{label}: {output:?}"
        );
        assert_eq!(
            output.stdout,
            expected_stdout.as_bytes(),
            "{label}: {output:?}"
        );
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

fn reject(label: &str, source: &str, phases: &[Phase], message: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, Stage::Verify, Optimization::O0);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{label}: {}",
        report.to_json()
    );
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

const ORIGINAL02: &str = include_str!("impl200_fixtures/02_store_of_callback_state.zil");
const PAIR_MUT: &str = include_str!("impl201_fixtures/01_pair_store_mut.zil");

#[test]
fn original02_types() {
    types("original02-types", ORIGINAL02);
}

#[test]
fn store_field_fnmut_types() {
    types("pair-mut-types", PAIR_MUT);
}

#[test]
fn original02_verify_native() {
    native("original02-native", ORIGINAL02, "", 0);
}

#[test]
fn store_field_fnmut_verify_native() {
    native("pair-mut-native", PAIR_MUT, "1A2BZ", 0);
}

const STORE_HELPERS: &str = r#"
fn empty<T: StoreElement>() -> Store<T> contract { result(create) } { return store_new() }
fn reserve<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }
fn push<T: StoreElement>(items: Store<T>, value: T) -> T!void { return store_push(items, value) }
fn pop<T: StoreElement>(items: Store<T>) -> void!T { return store_pop(items) }
"#;

#[test]
fn ordinary_store_helper_push_needs_202_type_binding() {
    let source = format!(
        "{STORE_HELPERS}\n{}",
        PAIR_MUT.replace("store_push(items,", "push(items,")
    );
    // helpers01 preserves the original unsuccessful expectation. 201's frozen
    // entry does not invent a role or inline this generated function; 202 must
    // retain its complete source T binding. This is not a safety acceptance.
    let case = Case::new("generic-push-boundary");
    let report = compile(&case, &source, Stage::Types, Optimization::O0);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("error union payloads"))
    );
}

#[test]
fn ordinary_store_helper_factory_needs_202_type_binding() {
    let source = format!(
        "{STORE_HELPERS}\n{}",
        PAIR_MUT
            .replace("= store_new()", "= empty()")
            .replace("store_reserve(items, 2)", "reserve(items, 2)")
    );
    let case = Case::new("generic-factory-boundary");
    let report = compile(&case, &source, Stage::Types, Optimization::O0);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TYPE-MISMATCH" && d.message.contains("Named(\"F\")"))
    );
}

#[test]
fn shared_fields_and_zero_calls_keep_all_element_cleanup() {
    let shared = PAIR_MUT
        .replace("FnMut()", "Fn()")
        .replace("count += 1; return count", "return count")
        .replace("store_at_mut", "store_at")
        .replace("!= 11", "!= 10")
        .replace("!= 12", "!= 10")
        .replace("!= 21", "!= 20")
        .replace("!= 13", "!= 10");
    native("shared-two-elements", &shared, "1A2BZ", 0);
    let start = PAIR_MUT.find("    let first = store_at_mut").unwrap();
    let end = PAIR_MUT[start..].find("    return 0").unwrap() + start;
    let zero = format!("{}{}", &PAIR_MUT[..start], &PAIR_MUT[end..]);
    native("zero-element-calls", &zero, "1A2BZ", 0);
}

#[test]
fn fnonce_consumes_only_owned_popped_elements() {
    let prefix = &PAIR_MUT[..PAIR_MUT.find("fn run<").unwrap()];
    let body = r#"fn run<F:FnOnce()->i32>(pair:Pair<F>)->void!i32 {
let items:Store<Item<i32,F>>=store_new()
if !store_reserve(items,2){return ()}
store_push(items,Item::<i32,F>{root:Ticket{code:49},callback:pair.first,value:10})
store_push(items,Item::<i32,F>{root:Ticket{code:50},callback:pair.second,value:20})
let last=store_pop(items)?
return last.callback()-20
}
fn main()->void!i32{return run(pair(make(65,10),make(66,20)))}"#;
    for actual_once in [true, false] {
        let producer = if actual_once {
            "let taken = give ticket; return count"
        } else {
            "ticket.code; return count"
        };
        let prefix = prefix
            .replace("FnMut()", "FnOnce()")
            .replace("ticket.code; count += 1; return count", producer);
        let prefix = if actual_once {
            prefix
        } else {
            prefix.replacen("-> impl FnOnce()", "-> impl Fn()", 1)
        };
        let source = format!("{prefix}{body}");
        native(&format!("pop-once-{actual_once}"), &source, "B21AZ", 0);
        let borrowed = source.replace(
            "let last=store_pop(items)?",
            "let last=store_at_mut(items,1)",
        );
        reject(
            &format!("borrowed-once-{actual_once}"),
            &borrowed,
            &[Phase::Expand],
            "FnOnce requires an owned Store element",
        );
        let repeated = source.replace(
            "return last.callback()-20",
            "last.callback();return last.callback()-20",
        );
        reject(
            &format!("repeat-once-{actual_once}"),
            &repeated,
            &[Phase::Expand],
            "callback field used after",
        );
    }
}

#[test]
fn shared_window_cannot_upgrade_and_active_windows_lock_structure() {
    reject(
        "shared-mut-call",
        &PAIR_MUT.replace("store_at_mut", "store_at"),
        &[Phase::Expand],
        "mutable callback field",
    );
    let invalid = PAIR_MUT.replace(
        "let first = store_at_mut(items, 0)",
        "let first = store_at_mut(items, 0)\n    store_reserve(items, 8)",
    );
    reject(
        "reserve-before-window-call",
        &invalid,
        &[Phase::Analyze, Phase::Verify],
        "",
    );
    let invalid = PAIR_MUT.replace(
        "let first = store_at_mut(items, 0)",
        "let first = store_at_mut(items, 0)\n    store_pop(items)",
    );
    reject(
        "pop-before-window-call",
        &invalid,
        &[Phase::Analyze, Phase::Verify],
        "",
    );
}

const LENDING: &str = r#"package State<F>{manages storage:ByteBuffer;callback:F}
fn make()->impl FnMut()->&[u8]{let b=bytes_new();return move()=>{bytes_reserve(b,4);bytes_push(b,7u8);return bytes_view(b)}}
fn pack<F:FnMut()->&[u8]>(callback:F)->State<F>{return State::<F>{storage:bytes_new(),callback:callback}}
fn run<F:FnMut()->&[u8]>(state:State<F>)->i32{
    let items:Store<State<F>>=store_new()
    if !store_reserve(items,1){return 1}
    store_push(items,state)
    let item=store_at_mut(items,0)
    let view=item.callback()
    return i32(view[0])-7
}
fn main()->i32{return run(pack(make()))}
"#;

#[test]
fn lending_result_keeps_its_element_and_parent_store_alive() {
    native("lending", LENDING, "", 0);
    let separate = LENDING
        .replace(
            "let item=store_at_mut(items,0)",
            "let other:Store<State<F>>=store_new()\n    let item=store_at_mut(items,0)",
        )
        .replace(
            "return i32(view[0])-7",
            "store_reserve(other,2);return i32(view[0])-7",
        );
    native("lending-unrelated-store", &separate, "", 0);
    for (label, effect) in [
        ("reserve", "store_reserve(items,2)"),
        ("pop", "store_pop(items)"),
        ("recall", "item.callback()"),
        ("reborrow", "store_at_mut(items,0).callback()"),
    ] {
        let source = LENDING.replace(
            "return i32(view[0])-7",
            &format!("{effect};return i32(view[0])-7"),
        );
        reject(
            &format!("lending-{label}"),
            &source,
            &[Phase::Analyze, Phase::Verify],
            "",
        );
    }
}

#[test]
fn callee_field_is_live_while_callback_arguments_are_prepared() {
    let prefix = PAIR_MUT[..PAIR_MUT.find("fn run<").unwrap()]
        .replace("FnMut()", "FnMut(bool)")
        .replace("return move () =>", "return move (flag:bool) =>");
    let body = r#"fn run<F:FnMut(bool)->i32>(pair:Pair<F>)->i32{
let items:Store<Item<i32,F>>=store_new()
if !store_reserve(items,1){return 1}
store_push(items,Item::<i32,F>{root:Ticket{code:49},callback:pair.first,value:10})
let item=store_at_mut(items,0)
return item.callback(false)-11
}
fn main()->i32{return run(pair(make(65,10),make(66,20)))}"#;
    let source = format!("{prefix}{body}");
    native("callee-preparation-control", &source, "1AZB", 0);
    reject(
        "callee-argument-reserve",
        &source.replace(
            "item.callback(false)",
            "item.callback(store_reserve(items,2))",
        ),
        &[Phase::Analyze, Phase::Verify],
        "",
    );
}
