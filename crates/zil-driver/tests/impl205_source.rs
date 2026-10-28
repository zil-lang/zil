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
    assert_eq!(ORIGINAL.len(), 138);
    verified("original-unused", ORIGINAL);
}

#[test]
fn refreshed_loop_alias_crosses_expand_and_types_without_changing_its_definition() {
    // Exact 242-byte source from the Expand author's join02 regression.
    let original = "package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}\nfn recycle<T:Copy,F:Fn()->i32>(state:State<T,F>)->State<T,F>{var next=state;var view=&next.a;while view()==0 {let moved=next.a;next.a=moved;view=&next.a}view();return next}fn main(){}";
    assert_eq!(original.len(), 242);
    verified("loop-refresh-original-unused", original);

    // Keep the entire generic definition unchanged, then instantiate it with
    // two distinct owned environments. Runtime takes the zero-iteration edge;
    // Types still checks the complete loop body and its join.
    let definitions = original.strip_suffix("fn main(){}").unwrap();
    let source = format!(
        r#"{RESOURCE}{definitions}
fn make(label:i32)->impl Fn()->i32{{let ticket=Ticket{{code:label}};return move()=>{{ticket.code;return 1}}}}
fn pack<T:Copy,F:Fn()->i32>(a:F,b:F,tag:T)->State<T,F>{{return State::<T,F>{{bytes:bytes_new(),a:a,b:b,tag:tag}}}}
fn main()->i32{{let state=pack(make(65),make(66),7);let next=recycle(state);return next.tag-7}}
"#
    );
    native("loop-refresh-instantiated", &source, "AB", 0);
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

fn rejected(label: &str, source: &str, phase: Phase, code: &str) {
    let (_, report) = compile(label, source, Stage::Verify);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert_eq!(
        report.verification_runs,
        usize::from(phase == Phase::Verify)
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn boundary_different_factories_cannot_overwrite_the_first_argument_constraint() {
    for reverse in [false, true] {
        let source = program("Fn", 2, reverse, false)
            .replace("fn main()", "fn other(label:i32,seed:i32)->impl Fn()->i32{let ticket=Ticket{code:label};return move()=>{ticket.code;return seed}}fn main()")
            .replace("make(67,100)", "other(67,100)");
        rejected(
            &format!("different-factory-{reverse}"),
            &source,
            Phase::Types,
            "E-TYPE-MISMATCH",
        );
    }
}

#[test]
fn boundary_argument_failure_cleans_only_values_prepared_in_actual_order() {
    for reverse in [false, true] {
        let source = program("Fn", 2, reverse, false)
            .replace(
                "fn main()->i32",
                "enum E{Fail}fn fail()->E!i64{unsafe{putchar(80)}return E.Fail}fn main()->E!i32",
            )
            .replace("#(6*7)", "fail()?");
        native(
            &format!("argument-failure-{reverse}"),
            &source,
            if reverse { "PCZAB" } else { "PZAB" },
            1,
        );
    }
}

#[test]
fn boundary_arguments_evaluate_once_before_replacing_the_old_field() {
    for reverse in [false, true] {
        let source = program("Fn", 2, reverse, false)
            .replace(
                "let ticket=Ticket{code:label};return move",
                "unsafe{putchar(label)}let ticket=Ticket{code:label};return move",
            )
            .replace(
                "fn main()->i32",
                "fn probe()->i64{unsafe{putchar(80)}return 42}fn main()->i32",
            )
            .replace("#(6*7)", "probe()");
        native(
            &format!("argument-once-{reverse}"),
            &source,
            if reverse { "ABCPAXZCB" } else { "ABPCAXZCB" },
            0,
        );
    }
}

#[test]
fn boundary_return_before_replacement_preserves_the_original_complete_state() {
    for early in [false, true] {
        let source = program("Fn", 2, false, false)
            .replace(
                "current.first=replacement",
                "if separator==0{return current}current.first=replacement",
            )
            .replace("#(6*7)", if early { "0" } else { "42" })
            .replace(
                "return answer-110",
                if early {
                    "return answer-11"
                } else {
                    "return answer-110"
                },
            );
        let source = source
            .replace(
                "fn main()->i32{",
                "fn main()->i32{let guard=Ticket{code:71};",
            )
            .replace(
                "let ticket=Ticket{code:label};return move",
                "unsafe{putchar(label+32)}let ticket=Ticket{code:label};return move",
            );
        native(
            &format!("before-replacement-return-{early}"),
            &source,
            if early { "abcCXZABG" } else { "abcAXZCBG" },
            0,
        );
    }
}

#[test]
fn boundary_concrete_rhs_error_after_replacement_keeps_all_owner_cleanup() {
    // The generic operation still returns State. A concrete caller provides
    // an existing E!i32 context for a genuinely fallible assignment RHS.
    for fail in [false, true] {
        let source = program("Fn", 2, false, false)
            .replace("fn main()->i32{", "enum E{Fail}fn check(fail:bool)->E!bool{unsafe{putchar(80)}if fail{return E.Fail}return true}fn main()->E!i32{let guard=Ticket{code:71};")
            .replace("let ticket=Ticket{code:label};return move", "unsafe{putchar(label+32)}let ticket=Ticket{code:label};return move")
            .replace("let answer=changed.first()+changed.second()", &format!("let replacement=make(68,200);changed.first=if check({fail})?{{replacement}}else{{replacement}};let answer=changed.first()+changed.second()"))
            .replace("return answer-110", "return answer-210");
        native(
            &format!("concrete-rhs-error-{fail}"),
            &source,
            if fail { "abcAdPDZCBG" } else { "abcAdPCXZDBG" },
            i32::from(fail),
        );
    }
}

#[test]
fn legacy_single_state_without_aliases_keeps_each_once_field_independent() {
    let source = format!(
        r#"{RESOURCE}
package State<F>{{manages root:Ticket;first:F;second:F}}
fn pack<F:FnOnce()->i32>(first:F,second:F)->State<F>{{return State::<F>{{root:Ticket{{code:90}},first:first,second:second}}}}
fn make(label:i32)->impl Fn()->i32{{let ticket=Ticket{{code:label}};return move()=>ticket.code}}
fn relay<F:FnOnce()->i32>(state:State<F>)->State<F>{{return state}}
fn consume<F:FnOnce()->i32>(state:State<F>)->i32{{let n=state.first()+state.second();mark();return n-131}}
fn main()->i32{{return consume(relay(pack(make(65),make(66))))}}
"#
    );
    native("legacy-no-alias-once", &source, "ABXZ", 0);
}

#[test]
fn legacy_single_state_shared_owners_keep_sibling_exclusive_windows() {
    let source = format!(
        r#"{RESOURCE}
package State<F>{{manages root:Ticket;first:F;second:F}}
fn pack<F:FnMut(i32)->i32>(first:F,second:F)->State<F>{{return State::<F>{{root:Ticket{{code:90}},first:first,second:second}}}}
fn make(label:i32,seed:i32)->impl FnMut(i32)->i32{{let ticket=Ticket{{code:label}};var n=seed;return move(delta:i32)=>{{ticket.code;n+=delta;return n}}}}
fn run<F:FnMut(i32)->i32>(state:State<F>)->i32{{var current=state;let first=&mut current.first;let b=current.second(1);let a=first(1);let again=first(1);return a+b+again-14}}
fn main()->i32{{return run(pack(make(65,0),make(66,10)))}}
"#
    );
    native("legacy-sibling-exclusive", &source, "ZAB", 0);
}

#[test]
fn legacy_single_state_lending_remains_legal_for_two_distinct_fields() {
    let source = r#"fn make(seed:u8)->impl FnMut()->&[u8]{let b=bytes_new();return move()=>{bytes_reserve(b,bytes_len(b)+1);bytes_push(b,seed);return bytes_view(b)}}
package State<F>{manages bytes:ByteBuffer;first:F;second:F}
fn pack<F:FnMut()->&[u8]>(a:F,b:F)->State<F>{return State::<F>{bytes:bytes_new(),first:a,second:b}}
fn run<F:FnMut()->&[u8]>(state:State<F>)->i32{var current=state;let a=current.first();let b=current.second();return i32(a[0])+i32(b[0])-16}
fn main()->i32{return run(pack(make(7u8),make(9u8)))}"#;
    native("legacy-lending", source, "", 0);
}
