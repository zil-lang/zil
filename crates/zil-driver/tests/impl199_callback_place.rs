//! Direct callback calls preserve the actual field and its source call mode.
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
        let saved = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = saved
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "callback199-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, saved.is_some())
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
    fs::write(case.0.join("main.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: Default::default(),
        },
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
    fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Some(optimization));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let out = Command::new(case.0.join("program")).output().unwrap();
        fs::write(case.0.join("stdout"), &out.stdout).unwrap();
        fs::write(case.0.join("stderr"), &out.stderr).unwrap();
        fs::write(
            case.0.join("exit.txt"),
            format!("{:?}\n", out.status.code()),
        )
        .unwrap();
        assert_eq!(out.status.code(), Some(exit), "{label}: {out:?}");
        assert_eq!(out.stdout, stdout.as_bytes(), "{label}: {out:?}");
        assert!(out.stderr.is_empty(), "{label}: {out:?}");
    }
}
fn rejected(label: &str, source: &str, phase: Phase, message: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.phase, phase, "{label}: {}", report.to_json());
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

// Byte-for-byte original source from the 197 direct-field rejection fixture.
const ORIGINAL: &str = r#"
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn pack<T:Copy,F:FnMut(&mut T)->T>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn spin<T:Copy,G:FnMut(&mut T)->T>(state:State<T,G>,count:i32)->T{
var current=state;var i=0;while i<count{i+=1;if i==2{continue}current.callback(&mut current.value);if i==4{break}}return current.value}
fn factory()->impl FnMut(&mut i32)->i32{var hits=0;return move(value:&mut i32)=>{hits+=1;*value+=1;return *value}}
fn main()->i32{return spin(pack(factory(),10),7)-13}
"#;
const RESOURCE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource Ticket{code:i32}impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
"#;
const MUTABLE: &str = r#"
package State<F>{manages ticket:Ticket;callback:F;value:i32}
fn pack<F:FnMut(i32)->i32>(callback:F)->State<F>{return State::<F>{ticket:Ticket{code:84},callback:callback,value:0}}
fn make()->impl FnMut(i32)->i32{let r=Ticket{code:65};var value=0;return move(delta:i32)=>{r.code;value+=delta;return value}}
"#;

#[test]
fn original_197_source_and_explicit_sibling_borrow_control_execute() {
    native("original197", ORIGINAL, "", 0);
    native(
        "explicit197",
        &ORIGINAL.replace(
            "current.callback(&mut current.value);",
            "let callback=&mut current.callback;let value=&mut current.value;callback(value);",
        ),
        "",
        0,
    );
    native(
        "zero197",
        &ORIGINAL.replace("pack(factory(),10),7)-13", "pack(factory(),10),0)-10"),
        "",
        0,
    );
    native(
        "one197",
        &ORIGINAL.replace("pack(factory(),10),7)-13", "pack(factory(),10),1)-11"),
        "",
        0,
    );
}

#[test]
fn plain_fnmut_state_zero_one_many_calls_and_whole_transfer() {
    for count in [0, 1, 4] {
        let source = format!(
            "{RESOURCE}{MUTABLE}fn run<G:FnMut(i32)->i32>(state:State<G>,count:i32)->i32{{var current=state;var i=0;var sum=0;while i<count{{sum+=current.callback(1);i+=1}}return sum}}fn main()->i32{{return run(pack(make()),{count})-{} }}",
            count * (count + 1) / 2
        );
        native(&format!("plain-mut-{count}"), &source, "TA", 0);
    }
    native(
        "whole-transfer",
        &format!(
            "{RESOURCE}{MUTABLE}fn tick<G:FnMut(i32)->i32>(state:State<G>)->State<G>{{var next=state;next.value=next.callback(1);return next}}fn main()->i32{{var state=tick(tick(pack(make())));return state.callback(1)+state.value-5}}"
        ),
        "TA",
        0,
    );
}

#[test]
fn plain_and_mixed_shared_fields_retain_environment_through_many_calls() {
    for mixed in [false, true] {
        let prefix = if mixed {
            "package State<T,F>{manages ticket:Ticket;callback:F;value:T}fn pack<T:Copy,F:Fn(i32)->i32>(callback:F,value:T)->State<T,F>{return State::<T,F>{ticket:Ticket{code:84},callback:callback,value:value}}fn run<T:Copy,G:Fn(i32)->i32>(state:State<T,G>,count:i32)->i32{var i=0;var sum=0;while i<count{sum+=state.callback(2);i+=1}return sum}"
        } else {
            "package State<F>{manages ticket:Ticket;callback:F;value:i32}fn pack<F:Fn(i32)->i32>(callback:F,value:i32)->State<F>{return State::<F>{ticket:Ticket{code:84},callback:callback,value:value}}fn run<G:Fn(i32)->i32>(state:State<G>,count:i32)->i32{var i=0;var sum=0;while i<count{sum+=state.callback(2);i+=1}return sum}"
        };
        for count in [0, 1, 4] {
            native(
                &format!("shared-{mixed}-{count}"),
                &format!(
                    "{RESOURCE}{prefix}fn make()->impl Fn(i32)->i32{{let r=Ticket{{code:65}};return move(delta:i32)=>{{r.code;return 5+delta}}}}fn main()->i32{{return run(pack(make(),3),{count})-{} }}",
                    count * 7
                ),
                "TA",
                0,
            );
        }
    }
}

#[test]
fn consuming_field_and_fn_to_once_adapter_settle_before_following_statement() {
    let prefix = format!(
        "{RESOURCE}package State<F>{{manages ticket:Ticket;callback:F}}fn pack<F:FnOnce()->i32>(callback:F)->State<F>{{return State::<F>{{ticket:Ticket{{code:84}},callback:callback}}}}fn mark(){{unsafe{{putchar(88)}}}}fn run<G:FnOnce()->i32>(state:State<G>)->i32{{let result=state.callback();mark();return result}} "
    );
    for factory in [
        "fn make()->impl FnOnce()->i32{let r=Ticket{code:65};return move()=>{let taken=give r;return taken.code}}",
        "fn make()->impl Fn()->i32{let r=Ticket{code:65};return move()=>r.code}",
    ] {
        native(
            "once-mode",
            &format!("{prefix}{factory}fn main()->i32{{return run(pack(make()))-65}}"),
            "AXT",
            0,
        );
        native(
            "once-zero",
            &format!("{prefix}{factory}fn main()->i32{{let state=pack(make());return 0}}"),
            "TA",
            0,
        );
        rejected(
            "once-reuse",
            &format!(
                "{prefix}{factory}fn twice<G:FnOnce()->i32>(state:State<G>)->i32{{state.callback();return state.callback()}}fn main()->i32{{return twice(pack(make()))}}"
            ),
            Phase::Expand,
            "callback field used after",
        );
    }
}

#[test]
fn ordinary_field_reads_still_take_and_preparation_errors_keep_owned_field() {
    native(
        "ordinary-take-refill",
        &format!(
            "{RESOURCE}{MUTABLE}fn main()->i32{{var state=pack(make());var taken=state.callback;state.callback=make();return taken(1)+state.callback(1)-2}}"
        ),
        "ATA",
        0,
    );
    rejected(
        "ordinary-read-moved",
        &format!(
            "{RESOURCE}{MUTABLE}fn main()->i32{{var state=pack(make());var taken=state.callback;return state.callback(1)}}"
        ),
        Phase::Analyze,
        "field",
    );
    native(
        "prepare-error",
        &format!(
            "{RESOURCE}{MUTABLE}enum E{{Fail}}fn fail()->E!i32{{unsafe{{putchar(80)}}return E.Fail}}fn main()->E!i32{{var state=pack(make());return state.callback(fail()?)}}"
        ),
        "PTA",
        1,
    );
    native(
        "argument-once",
        &format!(
            "{RESOURCE}{MUTABLE}fn argument()->i32{{unsafe{{putchar(80)}}return 2}}fn main()->i32{{var state=pack(make());return state.callback(argument())-2}}"
        ),
        "PTA",
        0,
    );
}

#[test]
fn callee_window_precedes_arguments_and_blocks_reentry_or_parent_move() {
    for (label, body, phase) in [
        (
            "reentrant-argument",
            "return state.callback(state.callback(1))",
            Phase::Verify,
        ),
        (
            "live-exclusive",
            "let other=&mut state.callback;state.callback(1);return other(1)",
            Phase::Verify,
        ),
        (
            "parent-argument",
            "return state.callback(consume(state))",
            Phase::Analyze,
        ),
    ] {
        rejected(
            label,
            &format!(
                "{RESOURCE}{MUTABLE}fn consume<G:FnMut(i32)->i32>(state:State<G>)->i32{{return 1}}fn main()->i32{{var state=pack(make());{body}}}"
            ),
            phase,
            "",
        );
    }
    rejected(
        "immutable-fnmut",
        &format!(
            "{RESOURCE}{MUTABLE}fn main()->i32{{let state=pack(make());return state.callback(1)}}"
        ),
        Phase::Types,
        "mutable Package",
    );
}

#[test]
fn lending_from_direct_callback_keeps_actual_field_and_parent_alive() {
    let prefix = "fn bytes()->impl FnMut()->&[u8]{let b=bytes_new();return move()=>{bytes_reserve(b,4);bytes_push(b,7u8);return bytes_view(b)}}package State<F>{manages storage:ByteBuffer;callback:F;count:i32}fn pack<F:FnMut()->&[u8]>(callback:F)->State<F>{return State::<F>{storage:bytes_new(),callback:callback,count:0}}";
    native(
        "lending-sibling",
        &format!(
            "{prefix}fn main()->i32{{var state=pack(bytes());let view=state.callback();state.count+=1;return i32(view[0])+state.count-8}}"
        ),
        "",
        0,
    );
    native(
        "lending-ended",
        &format!(
            "{prefix}fn main()->i32{{var state=pack(bytes());let first=state.callback();let old=i32(first.len());let next=state.callback();return old+i32(next.len())-3}}"
        ),
        "",
        0,
    );
    for (label, body, phase) in [
        (
            "lending-recall",
            "let view=state.callback();state.callback();return i32(view.len())",
            Phase::Verify,
        ),
        (
            "lending-move",
            "let view=state.callback();let moved=state;return i32(view.len())",
            Phase::Verify,
        ),
        (
            "lending-replace",
            "let view=state.callback();state.callback=bytes();return i32(view.len())",
            Phase::Verify,
        ),
    ] {
        rejected(
            label,
            &format!("{prefix}fn main()->i32{{var state=pack(bytes());{body}}}"),
            phase,
            "",
        );
    }
}
