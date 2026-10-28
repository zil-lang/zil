//! Finite nested helpers keep exact actual environment origins and call paths.
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
fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "impl188-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, source))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *source,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect(),
    }
}
fn check(label: &str, source: &str) -> CompileReport {
    let dir = directory(label);
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_project(
        &project(&[("main", source)]),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn accepted(label: &str, source: &str) {
    let report = check(label, source);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn rejected(label: &str, source: &str, phase: Phase, category: ResultCategory, message: &str) {
    let report = check(label, source);
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert_eq!(report.phase, phase, "{label}: {}", report.to_json());
    assert!(!report.artifacts.contains_key("executable"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{label}: {}",
        report.to_json()
    );
}
fn native(label: &str, files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        for (name, source) in files {
            fs::write(dir.join(format!("{name}.zil")), source).unwrap();
        }
        let executable = dir.join("program");
        let report = compile_project(
            &project(files),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout.log"), &output.stdout).unwrap();
        fs::write(dir.join("stderr.log"), &output.stderr).unwrap();
        fs::write(
            dir.join("native.json"),
            format!(
                "{{\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"expected_exit\":{exit},\"stdout_hex\":\"{}\",\"expected_stdout_hex\":\"{}\"}}\n",
                output.status.code().map_or("null".into(), |code| code.to_string()),
                output.stdout.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                stdout.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            ),
        ).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

const DROP: &str = r#"extern "C" fn putchar(n:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}"#;

#[test]
fn the_original_minimal_helper_composes_through_multiple_levels() {
    let source = r#"
fn leaf<F:Fn(i32)->i32>(f:&F,x:i32)->i32=>f(x)
fn inner<F:Fn(i32)->i32>(f:&F,x:i32)->i32=>leaf(f,x)
fn outer<F:Fn(i32)->i32>(f:&F,x:i32)->i32=>inner(f,x)
fn main()->i32{let f=move(n:i32)=>n+1;return outer(&f,6)-7}
"#;
    accepted("minimal", source);
    native("minimal", &[("main", source)], b"", 0);
}

#[test]
fn mutable_reborrows_keep_call_order_and_release_after_the_outer_call() {
    let source = r#"
fn leaf<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32=>f(n)
fn inner<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32{let first=leaf(f,n);return leaf(f,n+1)+first}
fn outer<F:FnMut(i32)->i32>(f:&mut F,n:i32)->i32{let first=inner(f,n);return leaf(f,n+2)+first}
fn main()->i32{var total=0;var f=move(n:i32)=>{total+=n;return total};if outer(&mut f,1)!=10{return 1};return f(0)-6}
"#;
    native("mut-order", &[("main", source)], b"", 0);
}

#[test]
fn branches_keep_zero_one_and_multiple_actual_calls() {
    for flag in [false, true] {
        let source = format!(
            r#"
fn leaf<F:Fn(i32)->i32>(f:&F,n:i32,flag:bool)->i32{{if flag{{return f(n)}};return 0}}
fn inner<F:Fn(i32)->i32>(f:&F,n:i32,flag:bool)->i32{{let a=leaf(f,n,flag);if flag{{return a}};return f(n+1)+f(n+2)}}
fn outer<F:Fn(i32)->i32>(f:&F,n:i32,flag:bool)->i32=>inner(f,n,flag)+leaf(f,n,!flag)
fn main()->i32{{let f=move(n:i32)=>n+1;return outer(&f,3,{flag})-{expected}}}
"#,
            expected = if flag { 4 } else { 15 }
        );
        native("branch-sequence", &[("main", &source)], b"", 0);
    }
}

#[test]
fn nested_exhaustive_matches_keep_their_defensive_trap_prefix() {
    for flag in [false, true] {
        let source = format!(
            r#"
fn leaf<F:Fn()->i32>(f:&F,flag:bool)->i32{{match flag{{true=>{{return f()}},false=>{{return 0}}}}}}
fn middle<F:Fn()->i32>(f:&F,flag:bool)->i32{{let n=leaf(f,flag);return f()+n}}
fn outer<F:Fn()->i32>(f:&F,flag:bool)->i32=>middle(f,flag)
fn main()->i32{{let f=move()=>4;return outer(&f,{flag})-{expected}}}
"#,
            expected = if flag { 8 } else { 4 }
        );
        native("match-paths", &[("main", &source)], b"", 0);
    }
}

#[test]
fn consumed_environments_are_settled_once_even_when_the_leaf_never_calls() {
    for invoke in [false, true] {
        let source = format!(
            r#"{DROP}
fn eat(give r:R)->i32=>7
fn leaf<F:FnOnce()->i32>(f:F,invoke:bool)->i32{{if invoke{{return f()}};return 0}}
fn inner<F:FnOnce()->i32>(f:F,invoke:bool)->i32=>leaf(f,invoke)
fn outer<F:FnOnce()->i32>(f:F,invoke:bool)->i32=>inner(f,invoke)
fn main()->i32{{let r=R{{code:65}};let f=move()=>eat(give r);return outer(f,{invoke})-{expected}}}
"#,
            expected = if invoke { 7 } else { 0 }
        );
        native("once-cleanup", &[("main", &source)], b"A", 0);
    }
}

#[test]
fn an_error_return_cuts_off_later_nested_calls_and_cleans_owned_locals() {
    for fail in [false, true] {
        let source = format!(
            r#"{DROP}enum E{{Fail}}
fn leaf<F:Fn()->E!i32>(f:&F)->E!i32=>f()
fn inner<F:Fn()->E!i32>(f:&F)->E!i32{{let guard=R{{code:66}};let a=leaf(f)?;return leaf(f)?+a}}
fn outer<F:Fn()->E!i32>(f:&F)->E!i32{{let guard=R{{code:67}};return inner(f)?}}
fn main()->E!i32{{let held=R{{code:65}};let f=move()->E!i32=>{{let observed=held.code;if {fail}{{return E.Fail}};return 7}};return outer(&f)?-14}}
"#
        );
        native("error-exit", &[("main", &source)], b"BCA", i32::from(fail));
    }
}

#[test]
fn an_unused_recursive_definition_is_rejected_without_instantiating_it() {
    for definitions in [
        "fn bad<F:Fn()->i32>(f:&F)->i32=>bad(f)",
        "fn left<F:Fn()->i32>(f:&F)->i32=>right(f);fn right<F:Fn()->i32>(f:&F)->i32=>left(f)",
    ] {
        rejected(
            "unused-recursive",
            &format!("{definitions}\nfn main(){{}}"),
            Phase::Expand,
            ResultCategory::Unsupported,
            "recursive callable helper dependency",
        );
    }
}

#[test]
fn cross_module_state_uses_nested_lending_helpers_and_a_disjoint_counter() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/nested_helpers188");
    let contents: Vec<_> = ["factory", "helpers", "state"]
        .into_iter()
        .map(|name| {
            (
                name,
                fs::read_to_string(root.join(format!("{name}.zil"))).unwrap(),
            )
        })
        .collect();
    let main = r#"import factory
import helpers
import state
fn main()->i32 {
    let snapshot=factory.snapshot(65u8)
    let first=helpers.peek(&snapshot)
    helpers.emit(i32(first[0]))
    let current=state.make(factory.grow())
    let answer=state.run(current,3)
    if answer!=0{return answer}
    let second=helpers.peek(&snapshot)
    helpers.emit(i32(second[0]))
    return i32(first[0])-i32(second[0])
}"#;
    let mut files = vec![("main", main)];
    files.extend(
        contents
            .iter()
            .map(|(name, source)| (*name, source.as_str())),
    );
    native("state-lending", &files, b"ABCDHAZ", 0);
}

#[test]
fn finite_nesting_does_not_admit_recursion_loops_or_mixed_kinds() {
    rejected(
        "used-recursion",
        "fn run<F:Fn()->i32>(f:&F)->i32=>run(f)\nfn main()->i32{let f=move()=>7;return run(&f)}",
        Phase::Expand,
        ResultCategory::Unsupported,
        "recursive callable helper dependency",
    );
    rejected(
        "nested-loop",
        "fn leaf<F:Fn()->i32>(f:&F)->i32=>f()\nfn run<F:Fn()->i32>(f:&F)->i32{var n=0;while n<3{n+=leaf(f)};return n}\nfn main()->i32{let f=move()=>1;return run(&f)}",
        Phase::Types,
        ResultCategory::Unsupported,
        "cyclic callable helper trace",
    );
    rejected(
        "mixed-bound-kind",
        "fn leaf<F:Fn()->i32>(f:&F)->i32=>f()\nfn run<F:FnMut()->i32>(f:&mut F)->i32=>leaf(f)\nfn main(){}",
        Phase::Expand,
        ResultCategory::Unsupported,
        "same declared callable kind",
    );
    rejected(
        "nested-once-twice",
        "fn leaf<F:FnOnce()->i32>(f:F)->i32=>f()\nfn run<F:FnOnce()->i32>(f:F)->i32=>leaf(f)+leaf(f)\nfn main(){}",
        Phase::Expand,
        ResultCategory::SafeReject,
        "used after move",
    );
}

#[test]
fn nested_helpers_keep_current_call_and_lending_result_loans() {
    rejected(
        "argument-writes-live-capture",
        "fn set(n:&mut i32)->i32{*n=9;return 0}\nfn leaf<F:FnMut()->i32>(f:&mut F,n:i32)->i32=>f()+n\nfn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32=>leaf(f,n)\nfn main()->i32{var n=0;var f=()=>{n+=1;return n};return run(&mut f,set(&mut n))}",
        Phase::Verify,
        ResultCategory::SafeReject,
        "active address window",
    );
    rejected(
        "returned-view-overlaps-next-nested-call",
        "fn make(n:i32)->impl FnMut()->&i32{var value=n;return move()=>{value+=1;return &value}}\nfn leaf<F:FnMut()->&i32>(f:&mut F)->&i32=>f()\nfn run<F:FnMut()->&i32>(f:&mut F)->&i32=>leaf(f)\nfn main()->i32{var f=make(7);let view=run(&mut f);let later=run(&mut f);return *view+*later}",
        Phase::Verify,
        ResultCategory::SafeReject,
        "active address window",
    );
}
