//! Runtime helper loops preserve real callback ordering, windows and cleanup.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const INPUT: &str =
    r#"extern "C" fn getchar()->i32;fn input()->i32{var n=0;unsafe{n=getchar()};return n-48}"#;
const EMIT: &str = r#"extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}"#;
const DROP: &str = "resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}";
const COUNTS: [u8; 4] = [0, 1, 2, 7];

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "impl190-flow-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn compile(
    label: &str,
    files: &[(&str, &str)],
    options: CompileOptions,
    dir: &Path,
) -> CompileReport {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, source))| {
                fs::write(dir.join(format!("{name}.zil")), source).unwrap();
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
    };
    fs::write(dir.join("label.txt"), label).unwrap();
    let report = compile_project(
        &project,
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..options
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(label: &str, files: &[(&str, &str)], cases: &[(u8, String, i32)], markers: &[&str]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        let program = dir.join("program");
        let report = compile(
            label,
            files,
            CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(program.clone()),
                ..CompileOptions::default()
            },
            &dir,
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let typed = fs::read_to_string(dir.join("phases/typed.txt")).unwrap();
        for marker in markers {
            assert!(
                typed.contains(marker),
                "{label}: missing actual contract {marker}"
            );
        }
        for (count, expected, code) in cases {
            let case = dir.join(format!("count-{count}"));
            fs::create_dir(&case).unwrap();
            fs::write(case.join("stdin.txt"), [b'0' + count]).unwrap();
            let mut child = Command::new(&program)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&[b'0' + count])
                .unwrap();
            let started = Instant::now();
            let mut timed_out = false;
            while child.try_wait().unwrap().is_none() {
                if started.elapsed() > Duration::from_secs(10) {
                    timed_out = true;
                    child.kill().unwrap();
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let output = child.wait_with_output().unwrap();
            fs::write(case.join("stdout.log"), &output.stdout).unwrap();
            fs::write(case.join("stderr.log"), &output.stderr).unwrap();
            fs::write(case.join("native.json"), format!(
                "{{\"optimization\":\"{optimization:?}\",\"input\":{count},\"timed_out\":{timed_out},\"exit_code\":{},\"expected_exit\":{code},\"expected_stdout\":{expected:?}}}\n",
                output.status.code().map_or("null".into(), |n| n.to_string())
            )).unwrap();
            assert!(!timed_out, "{label}: count {count} did not finish");
            assert_eq!(
                output.status.code(),
                Some(*code),
                "{label}: count {count}: {output:?}"
            );
            assert_eq!(output.stdout, expected.as_bytes(), "{label}: count {count}");
            assert!(
                output.stderr.is_empty(),
                "{label}: count {count}: {output:?}"
            );
        }
    }
}
fn cases(output: impl Fn(u8) -> String) -> Vec<(u8, String, i32)> {
    COUNTS.into_iter().map(|n| (n, output(n), 0)).collect()
}
fn reject(label: &str, source: &str, phase: Phase, category: ResultCategory, code: &str) {
    let dir = directory(label);
    let report = compile(label, &[("main", source)], CompileOptions::default(), &dir);
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert_eq!(report.phase, phase, "{label}: {}", report.to_json());
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn shared_while_runs_zero_one_two_and_many_actual_calls() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{{var i=0;var total=0;while i<n{{total+=f();i+=1}}return total}}
fn main()->i32{{let n=input();let f=move()=>{{emit(65);return 3}};return run(&f,n)-3*n}}
"#
    );
    native(
        "shared-while",
        &[("main", &source)],
        &cases(|n| "A".repeat(n.into())),
        &["traces: Flow("],
    );
}

#[test]
fn shared_for_passes_the_runtime_index_in_order() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn run<F:Fn(i32)->i32>(f:&F,n:i32)->i32{{var total=0;for index in 0..n{{total+=f(index)}}return total}}
fn main()->i32{{let n=input();let f=move(value:i32)=>{{emit(65+value);return value+1}};return run(&f,n)-n*(n+1)/2}}
"#
    );
    native(
        "shared-for",
        &[("main", &source)],
        &cases(|n| (0..n).map(|i| char::from(b'A' + i)).collect()),
        &["traces: Flow("],
    );
}

#[test]
fn mutable_while_reuses_the_actual_environment_and_ends_its_window() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{{var total=0;var i=0;while i<n{{total+=f();i+=1}}return total}}
fn main()->i32{{let n=input();var count=0;var f=move()=>{{count+=1;emit(64+count);return count}};let total=run(&mut f,n);let after=f();return total-n*(n+1)/2+after-n-1}}
"#
    );
    native(
        "mutable-while",
        &[("main", &source)],
        &cases(|n| (0..=n).map(|i| char::from(b'A' + i)).collect()),
        &["traces: Flow("],
    );
}

#[test]
fn mutable_for_keeps_the189_current_explicit_address_across_calls() {
    let source = format!(
        r#"{INPUT}
fn run<F:FnMut(&mut i32)->i32>(f:&mut F,p:&mut i32,n:i32)->i32{{var total=0;for index in 0..n{{total+=f(p)}}return total}}
fn main()->i32{{let n=input();var step=0;var f=move(p:&mut i32)=>{{step+=1;*p+=step;return *p}};var value=20;let total=run(&mut f,&mut value,n);return total-20*n-n*(n+1)*(n+2)/6+value-20-n*(n+1)/2}}
"#
    );
    native(
        "mutable-for-address",
        &[("main", &source)],
        &cases(|_| String::new()),
        &["traces: Flow("],
    );
}

#[test]
fn finite_leaf_and_outer_call_sites_compose_with_a_flow_child() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn leaf<F:Fn()->i32>(f:&F)->i32{{let a=f();return a+f()}}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{{var total=0;var i=0;while i<n{{total+=leaf(f);i+=1}}return total}}
fn outer<F:Fn()->i32>(f:&F,n:i32)->i32{{let before=leaf(f);let middle=run(f,n);return before+middle+leaf(f)}}
fn main()->i32{{let n=input();let f=move()=>{{emit(65);return 3}};return outer(&f,n)-3*(4+2*n)}}
"#
    );
    native(
        "finite-flow-finite",
        &[("main", &source)],
        &cases(|n| "A".repeat(4 + 2 * usize::from(n))),
        &["traces: Flow(", "traces: Finite("],
    );
}

#[test]
fn two_flow_call_sites_keep_their_own_continuations_inside_an_outer_loop() {
    let source = format!(
        r#"{INPUT}
fn leaf<F:FnMut()->i32>(f:&mut F,n:i32)->i32{{var total=0;var i=0;while i<n{{total+=f();i+=1}}return total}}
fn outer<F:FnMut()->i32>(f:&mut F,n:i32)->i32{{var total=0;var i=0;while i<n{{total+=leaf(f,1);total+=leaf(f,2);i+=1}}return total}}
fn main()->i32{{let n=input();var step=0;var f=move()=>{{step+=1;return step}};let total=outer(&mut f,n);return total-(3*n)*(3*n+1)/2+f()-3*n-1}}
"#
    );
    native(
        "flow-in-flow",
        &[("main", &source)],
        &cases(|_| String::new()),
        &["traces: Flow("],
    );
}

#[test]
fn runtime_iteration_count_does_not_spend_the_static_helper_depth() {
    let source = format!(
        r#"{INPUT}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{{var total=0;var i=0;while i<n{{total+=f();i+=1}}return total}}
fn main()->i32{{let n=input()*1000;let f=move()=>1;return run(&f,n)-n}}
"#
    );
    native(
        "runtime-not-depth",
        &[("main", &source)],
        &cases(|_| String::new()),
        &["traces: Flow("],
    );
}

#[test]
fn owned_and_borrowed_environments_cleanup_once_even_on_zero_iterations() {
    for (label, parameter, argument, suffix) in [
        ("owned-cleanup", "F", "f", "HZ"),
        ("borrowed-cleanup", "&F", "&f", "ZH"),
    ] {
        let source = format!(
            r#"{INPUT}{EMIT}{DROP}
fn run<F:Fn()->i32>(f:{parameter},n:i32)->i32{{var total=0;var i=0;while i<n{{total+=f();i+=1}}return total}}
fn main()->i32{{let n=input();let owned=R{{code:72}};let f=move()=>{{emit(65);return owned.code}};let total=run({argument},n);emit(90);return total-72*n}}
"#
        );
        native(
            label,
            &[("main", &source)],
            &cases(|n| format!("{}{suffix}", "A".repeat(n.into()))),
            &["traces: Flow("],
        );
    }
}

#[test]
fn a_zero_callback_loop_keeps_its_cycle_and_owned_cleanup() {
    let source = format!(
        r#"{INPUT}{EMIT}{DROP}
fn run<F:Fn()->i32>(f:F,n:i32)->i32{{var i=0;while i<n{{i+=1}}return i}}
fn main()->i32{{let n=input();let owned=R{{code:72}};let f=move()=>{{emit(88);return owned.code}};let answer=run(f,n);emit(90);return answer-n}}
"#
    );
    native(
        "zero-callback-cycle",
        &[("main", &source)],
        &cases(|_| "HZ".into()),
        &["traces: Flow("],
    );
}

#[test]
fn early_return_stops_later_calls_without_resetting_the_mutable_environment() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{{var i=0;while i<n{{let value=f();if value==2{{return value}}i+=1}}return 0}}
fn main()->i32{{let n=input();var step=0;var f=move()=>{{step+=1;emit(64+step);return step}};let answer=run(&mut f,n);var expected=0;var calls=n;if n>=2{{expected=2;calls=2}}return answer-expected+f()-calls-1}}
"#
    );
    native(
        "early-return",
        &[("main", &source)],
        &cases(|n| (0..=n.min(2)).map(|i| char::from(b'A' + i)).collect()),
        &["traces: Flow("],
    );
}

#[test]
fn copy_error_return_cleans_each_live_iteration_and_the_outer_owners() {
    let source = format!(
        r#"{INPUT}{EMIT}{DROP}enum E{{Fail}}
fn run<F:FnMut()->E!i32>(f:&mut F,n:i32)->E!i32{{let guard=R{{code:67}};var i=0;var total=0;while i<n{{let per=R{{code:66}};let value=f()?;total+=value;i+=1}}return total}}
fn main()->E!i32{{let n=input();let held=R{{code:65}};var step=0;var f=move()->E!i32=>{{held.code;step+=1;if step==2{{return E.Fail}}return 1}};return run(&mut f,n)?-n}}
"#
    );
    let cases: Vec<_> = COUNTS
        .into_iter()
        .map(|n| {
            (
                n,
                format!("{}CA", "B".repeat(usize::from(n.min(2)))),
                i32::from(n >= 2),
            )
        })
        .collect();
    native(
        "error-cleanup",
        &[("main", &source)],
        &cases,
        &["traces: Flow("],
    );
}

#[test]
fn finite_defensive_trap_exits_survive_copying_into_a_flow_parent() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn leaf<F:Fn()->i32>(f:&F,flag:bool)->i32{{match flag{{true=>{{return f()}},false=>{{return 0}}}}}}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{{var total=0;var i=0;while i<n{{total+=leaf(f,true);i+=1}}return total}}
fn main()->i32{{let n=input();let f=move()=>{{emit(65);return 3}};return run(&f,n)-3*n}}
"#
    );
    native(
        "finite-trap-child",
        &[("main", &source)],
        &cases(|n| "A".repeat(n.into())),
        &["traces: Flow(", "exit: Trap"],
    );
}

#[test]
fn lending_results_end_at_last_use_before_the_next_iteration() {
    let source = format!(
        r#"{INPUT}
fn make()->impl FnMut()->&i32{{var step=0;return move()=>{{step+=1;return &step}}}}
fn run<F:FnMut()->&i32>(f:&mut F,n:i32)->i32{{var total=0;var i=0;while i<n{{let view=f();total+=*view;i+=1}}return total}}
fn main()->i32{{let n=input();var f=make();let total=run(&mut f,n);let after=f();return total-n*(n+1)/2+*after-n-1}}
"#
    );
    native(
        "lending-last-use",
        &[("main", &source)],
        &cases(|_| String::new()),
        &["traces: Flow("],
    );
}

#[test]
fn flow_calls_do_not_release_a_lending_window_still_needed_later() {
    let definitions = "fn make()->impl FnMut()->&i32{var step=0;return move()=>{step+=1;return &step}}fn run<F:FnMut()->&i32>(f:&mut F,n:i32)->i32{var total=0;var i=0;while i<n{let view=f();total+=*view;i+=1}return total}";
    reject(
        "lending-current-call",
        &format!(
            "{definitions}fn main()->i32{{var f=make();let held=f();let later=run(&mut f,2);return *held+later}}"
        ),
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
    let source = "fn make()->impl FnMut()->&i32{var step=0;return move()=>{step+=1;return &step}}fn bad<F:FnMut()->&i32>(f:&mut F,n:i32)->i32{let held=f();var total=0;var i=0;while i<n{let later=f();total+=*held+*later;i+=1}return total}fn main()->i32{var f=make();return bad(&mut f,2)}";
    reject(
        "lending-across-loop",
        source,
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn actual_capture_and_explicit_address_still_must_be_separate() {
    let source = "fn run<F:Fn(&mut i32)->i32>(f:&F,p:&mut i32,n:i32)->i32{var total=0;var i=0;while i<n{total+=f(p);i+=1}return total}fn main()->i32{var value=1;let f=(p:&mut i32)=>{*p+=1;return value};return run(&f,&mut value,2)}";
    reject(
        "explicit-capture-alias",
        source,
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn a_cyclic_once_profile_is_not_made_repeatable_by_zero_callback_events() {
    let source = "fn run<F:FnOnce()->i32>(f:F,n:i32)->i32{var i=0;while i<n{i+=1}return i}fn main()->i32{let f=move()=>7;return run(f,2)-2}";
    reject(
        "zero-call-once",
        source,
        Phase::Types,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
}

#[test]
fn four_module_consumer_contains_a_real_flow_helper_and_owned_state() {
    let files = [
        (
            "main",
            include_str!("../../../examples/mainline/loop_helpers190/main.zil"),
        ),
        (
            "factory",
            include_str!("../../../examples/mainline/loop_helpers190/factory.zil"),
        ),
        (
            "helpers",
            include_str!("../../../examples/mainline/loop_helpers190/helpers.zil"),
        ),
        (
            "state",
            include_str!("../../../examples/mainline/loop_helpers190/state.zil"),
        ),
    ];
    let dir = directory("four-module");
    let report = compile("four-module", &files, CompileOptions::default(), &dir);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    assert_eq!(report.source_files.len(), 4);
    assert!(
        fs::read_to_string(dir.join("phases/typed.txt"))
            .unwrap()
            .contains("traces: Flow(")
    );
}
