//! Source-to-native controls for one owned State plus a separate replacement.
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

fn compile(label: &str, source: &str, stage: Stage) -> (PathBuf, CompileReport) {
    compile_optimized(label, source, stage, Optimization::O0)
}

fn compile_optimized(
    label: &str,
    source: &str,
    stage: Stage,
    optimization: Optimization,
) -> (PathBuf, CompileReport) {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = root.join(format!(
        "replacement205-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("source.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: Default::default(),
        },
        &CompileOptions {
            stop_after: stage,
            optimization,
            output: (stage == Stage::Build).then(|| directory.join("program")),
            emit_dir: Some(directory.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(directory.join("compile.json"), report.to_json()).unwrap();
    (directory, report)
}

fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let (directory, report) = compile_optimized(label, source, Stage::Build, optimization);
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
        fs::write(directory.join("stdout.log"), &output.stdout).unwrap();
        fs::write(directory.join("stderr.log"), &output.stderr).unwrap();
        fs::write(
            directory.join("native.txt"),
            format!(
                "optimization={optimization:?}\nexit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n",
                output.status.code()
            ),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

const RESOURCE: &str = r#"extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn mark(){unsafe{putchar(88)}}
"#;
const ORIGINAL: &str =
    include_str!("../../zil-expand/tests/impl205_fixtures/00_original_state_and_replacement.zil");

fn verified(label: &str, source: &str) {
    let (_, report) = compile(label, source, Stage::Verify);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify);
    assert_eq!(report.verification_runs, 1);
}

fn program(kind: &str, fields: usize, reverse: bool, mixed: bool) -> String {
    let ordinary = if mixed { "T:Copy," } else { "" };
    let family = if mixed { "State<T,F>" } else { "State<F>" };
    let constructor = if mixed { "State::<T,F>" } else { "State::<F>" };
    let data_field = if mixed { "data:T;" } else { "" };
    let data_param = if mixed { ",data:T" } else { "" };
    let data_init = if mixed { ",data:data" } else { "" };
    let data_actual = if mixed { ",7" } else { "" };
    let second_field = if fields == 2 { "second:F;" } else { "" };
    let second_param = if fields == 2 { ",second:F" } else { "" };
    let second_init = if fields == 2 { ",second:second" } else { "" };
    let second_actual = if fields == 2 { ",make(66,10)" } else { "" };
    let signature = if kind == "FnMut" {
        "(i32)->i32"
    } else {
        "()->i32"
    };
    let factory = if kind == "FnMut" {
        "fn make(label:i32,seed:i32)->impl FnMut(i32)->i32{let ticket=Ticket{code:label};var n=seed;return move(delta:i32)=>{ticket.code;n+=delta;return n}}"
    } else {
        // Actual Fn must still obey the declared FnOnce contract in the helper.
        "fn make(label:i32,seed:i32)->impl Fn()->i32{let ticket=Ticket{code:label};return move()=>{ticket.code;return seed}}"
    };
    let parameters = if reverse {
        format!("replacement:F,separator:i64,state:{family}")
    } else {
        format!("state:{family},separator:i64,replacement:F")
    };
    let arguments = if reverse {
        "make(67,100),#(6*7),state"
    } else {
        "state,#(6*7),make(67,100)"
    };
    let old_call = match kind {
        "FnOnce" => "current.first();",
        "FnMut" => "current.first(1);",
        _ => "current.first();current.first();",
    };
    let first_call = if kind == "FnMut" {
        "changed.first(1)"
    } else {
        "changed.first()"
    };
    let second_call = if fields == 2 {
        if kind == "FnMut" {
            "+changed.second(1)"
        } else {
            "+changed.second()"
        }
    } else {
        ""
    };
    let expected = match (kind, fields) {
        ("FnMut", 2) => 112,
        ("FnMut", _) => 101,
        (_, 2) => 110,
        _ => 100,
    };
    let finish = if kind == "FnOnce" {
        format!(
            "fn finish<{ordinary}F:FnOnce()->i32>(changed:{family})->i32{{let answer={first_call}{second_call};mark();return answer-{expected}}}"
        )
    } else {
        String::new()
    };
    let end = if kind == "FnOnce" {
        "return finish(changed)".to_string()
    } else {
        format!("let answer={first_call}{second_call};mark();return answer-{expected}")
    };
    format!(
        r#"{RESOURCE}
package {family}{{manages root:Ticket;first:F;{second_field}{data_field}}}
fn pack<{ordinary}F:{kind}{signature}>(first:F{second_param}{data_param})->{family}{{return {constructor}{{root:Ticket{{code:90}},first:first{second_init}{data_init}}}}}
{factory}
fn replace<{ordinary}F:{kind}{signature}>({parameters})->{family}{{var current=state;separator;{old_call}current.first=replacement;return current}}
{finish}
fn main()->i32{{let state=pack(make(65,1){second_actual}{data_actual});var changed=replace({arguments});{end}}}
"#
    )
}

#[test]
fn original_138_bytes_verify_as_an_unused_definition() {
    assert_eq!(ORIGINAL.as_bytes().len(), 138);
    verified("original-unused", ORIGINAL);
}

#[test]
fn shared_fields_replace_one_environment_in_both_parameter_orders() {
    for fields in [1, 2] {
        for reverse in [false, true] {
            let source = program("Fn", fields, reverse, false);
            native(
                &format!("fn-{fields}-{reverse}"),
                &source,
                if fields == 2 { "AXZCB" } else { "AXZC" },
                0,
            );
        }
    }
}

#[test]
fn mutable_fields_and_copy_data_keep_their_single_f_nominal_instance() {
    for reverse in [false, true] {
        for mixed in [false, true] {
            let source = program("FnMut", 2, reverse, mixed);
            native(&format!("mut-{reverse}-{mixed}"), &source, "AXZCB", 0);
        }
    }
}

#[test]
fn declared_once_consumes_the_old_field_then_refills_it() {
    for fields in [1, 2] {
        for reverse in [false, true] {
            let source = program("FnOnce", fields, reverse, false);
            native(
                &format!("once-{fields}-{reverse}"),
                &source,
                if fields == 2 { "ACBXZ" } else { "ACXZ" },
                0,
            );
        }
    }
}

#[test]
fn three_positions_move_independently_after_a_field_gets_a_new_owner() {
    let source = format!(
        r#"{RESOURCE}
package State<F>{{manages root:Ticket;first:F;second:F}}
fn pack<F:FnOnce()->i32>(first:F,second:F)->State<F>{{return State::<F>{{root:Ticket{{code:90}},first:first,second:second}}}}
fn make(label:i32)->impl Fn()->i32{{let ticket=Ticket{{code:label}};return move()=>ticket.code}}
fn replace<F:FnOnce()->i32>(replacement:F,state:State<F>)->State<F>{{var current=state;let old=current.first;current.first=replacement;current.second();current.second=old;return current}}
fn finish<F:FnOnce()->i32>(changed:State<F>)->i32{{let n=changed.first()+changed.second();mark();return n-132}}
fn main()->i32{{let state=pack(make(65),make(66));let changed=replace(make(67),state);return finish(changed)}}
"#
    );
    native("three-positions", &source, "BCAXZ", 0);
}

#[test]
fn structural_copy_is_copied_to_a_new_temporary_before_transfer() {
    let source = format!(
        r#"{RESOURCE}
package State<F>{{manages root:Ticket;first:F;second:F}}
fn pack<F:FnMut(i32)->i32>(first:F,second:F)->State<F>{{return State::<F>{{root:Ticket{{code:90}},first:first,second:second}}}}
fn make(seed:i32)->impl FnMut(i32)->i32{{var n=seed;return move(delta:i32)=>{{n+=delta;return n}}}}
fn replace<F:FnMut(i32)->i32>(state:State<F>,replacement:F)->State<F>{{var current=state;current.first=replacement;return current}}
fn main()->i32{{var original=make(0);let state=pack(original,original);var changed=replace(state,original);let n=changed.first(1)+changed.second(10)+original(100);mark();return n-111}}
"#
    );
    native("copy-independent-temporaries", &source, "XZ", 0);
}
