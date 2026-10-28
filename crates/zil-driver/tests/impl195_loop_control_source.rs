//! Loop-control helpers preserve actual step targets, windows and exit cleanup.
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
        "impl195-control-{label}-{}-{}",
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
fn original_preflight_helper_programs_now_execute_with_their_exact_inputs() {
    native(
        "06-helper-while-break",
        &[(
            "main",
            r#"fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var total=0;while i<n{if i==2{break};total+=f();i+=1}return total}fn main()->i32{let f=move()=>3;return run(&f,5)-6}
"#,
        )],
        &[(0, "".into(), 0)],
        &["traces: Flow("],
    );
    native(
        "07-helper-while-continue",
        &[(
            "main",
            r#"fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var total=0;while i<n{i+=1;if i==2{continue};total+=f()}return total}fn main()->i32{let f=move()=>1;return run(&f,5)-4}
"#,
        )],
        &[(0, "".into(), 0)],
        &["traces: Flow("],
    );
    native(
        "08-helper-for-continue",
        &[(
            "main",
            r#"fn run<F:Fn(i32)->i32>(f:&F,n:i32)->i32{var total=0;for index in 0..n{if index==2{continue};total+=f(index)}return total}fn main()->i32{let f=move(index:i32)=>index;return run(&f,5)-8}
"#,
        )],
        &[(0, "".into(), 0)],
        &["traces: Flow("],
    );
    native(
        "09-helper-for-break-mut-arg",
        &[(
            "main",
            r#"fn run<F:FnMut(&mut i32)->i32>(f:&mut F,p:&mut i32,n:i32)->i32{var total=0;for index in 0..n{if index==2{break};total+=f(p)}return total}fn main()->i32{var step=0;var f=move(p:&mut i32)=>{step+=1;*p+=1;return *p};var value=0;return run(&mut f,&mut value,5)-3+value-2}
"#,
        )],
        &[(0, "".into(), 0)],
        &["traces: Flow("],
    );
    native(
        "10-helper-nested-for-while",
        &[(
            "main",
            r#"fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var total=0;for outer in 0..n{var inner=0;while inner<3{inner+=1;if inner==1{continue};if inner==3{break};total+=f()}}return total}fn main()->i32{var value=0;var f=move()=>{value+=1;return value};return run(&mut f,2)-3}
"#,
        )],
        &[(0, "".into(), 0)],
        &["traces: Flow("],
    );
    native(
        "11-helper-break-resource-cleanup",
        &[(
            "main",
            r#"extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}fn run<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var total=0;while i<n{let guard=R{code:65+i};total+=f();if i==1{break};i+=1}return total}fn main()->i32{let f=move()=>{emit(67);return 1};return run(&f,3)-2}
"#,
        )],
        &[(0, "CACB".into(), 0)],
        &["traces: Flow("],
    );
    native(
        "12-helper-continue-resource-cleanup",
        &[(
            "main",
            r#"extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}resource R{code:i32}impl Drop for R{fn drop(self:Self){emit(.code)}}fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{var total=0;for index in 0..n{let guard=R{code:65+index};if index==1{continue};total+=f() }return total}fn main()->i32{var calls=0;var f=move()=>{calls+=1;emit(67);return calls};return run(&mut f,3)-3}
"#,
        )],
        &[(0, "CABCC".into(), 0)],
        &["traces: Flow("],
    );
    native(
        "13-helper-break-after-lending-last-use",
        &[(
            "main",
            r#"fn make()->impl FnMut()->&i32{var value=0;return move()=>{value+=1;return &value}}fn run<F:FnMut()->&i32>(f:&mut F,n:i32)->i32{var total=0;for index in 0..n{let view=f();total+=*view;if index==1{break}}return total}fn main()->i32{var f=make();return run(&mut f,3)-3}
"#,
        )],
        &[(0, "".into(), 0)],
        &["traces: Flow("],
    );
}

#[test]
fn both_original_held_lending_inputs_reach_the_real_borrow_conflict() {
    reject(
        "14-helper-continue-held-lending-negative",
        r#"fn make()->impl FnMut()->&i32{var value=0;return move()=>{value+=1;return &value}}fn bad<F:FnMut()->&i32>(f:&mut F,n:i32)->i32{let held=f();var total=0;var i=0;while i<n{i+=1;if i==1{continue};let later=f();total+=*held+*later}return total}fn main()->i32{var f=make();return bad(&mut f,2)}
"#,
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
    reject(
        "15-helper-existing-held-lending-negative",
        r#"fn make()->impl FnMut()->&i32{var value=0;return move()=>{value+=1;return &value}}fn bad<F:FnMut()->&i32>(f:&mut F,n:i32)->i32{let held=f();var total=0;var i=0;while i<n{let later=f();total+=*held+*later;i+=1}return total}fn main()->i32{var f=make();return bad(&mut f,2)}
"#,
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}

#[test]
fn original_once_consumption_before_continue_remains_closed() {
    reject(
        "16-original-once",
        r#"fn run<F:FnOnce()->i32>(f:F,n:i32)->i32{var total=0;for index in 0..n{total+=f();continue}return total}fn main()->i32{let f=move()=>1;return run(f,2)}
"#,
        Phase::Expand,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
}

#[test]
fn runtime_while_control_runs_only_the_selected_callbacks() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn run<F:Fn(i32)->i32>(f:&F,n:i32)->i32{{var index=0;var total=0;while index<n{{index+=1;if index==2{{continue}}if index==5{{break}}total+=f(index)}}return total}}
fn main()->i32{{let n=input();let f=move(value:i32)=>{{emit(48+value);return value}};let total=run(&f,n);emit(65+total);return 0}}
"#
    );
    native(
        "runtime-while",
        &[("main", &source)],
        &[
            (0, "A".into(), 0),
            (1, "1B".into(), 0),
            (2, "1B".into(), 0),
            (7, "134I".into(), 0),
        ],
        &["traces: Flow("],
    );
}

#[test]
fn nested_while_and_for_control_only_the_nearest_loop() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{{var outer=0;var total=0;while outer<n{{outer+=1;for inner in 0..4{{if inner==0{{continue}}if inner==2{{break}}total+=f()}}}}return total}}
fn main()->i32{{let n=input();var calls=0;var f=move()=>{{calls+=1;emit(64+calls);return calls}};return run(&mut f,n)-n*(n+1)/2}}
"#
    );
    native(
        "nearest-loop",
        &[("main", &source)],
        &cases(|n| (0..n).map(|i| char::from(b'A' + i)).collect()),
        &["traces: Flow("],
    );
}

#[test]
fn mixed_match_return_break_and_continue_keep_real_cleanup_edges() {
    let source = format!(
        r#"{INPUT}{EMIT}{DROP}
fn run<F:FnMut()->i32>(f:&mut F,n:i32)->i32{{var total=0;for index in 0..n{{let guard=R{{code:48+index}};match index==0{{true=>{{if n==1{{return f()}}else{{continue}}}},false=>{{if index==3{{break}}total+=f()}}}}}}return total}}
fn main()->i32{{let n=input();var count=0;var f=move()=>{{count+=1;emit(64+count);return count}};let total=run(&mut f,n);emit(65+total);return 0}}
"#
    );
    native(
        "mixed-match",
        &[("main", &source)],
        &[
            (0, "A".into(), 0),
            (1, "A0B".into(), 0),
            (2, "0A1B".into(), 0),
            (7, "0A1B23D".into(), 0),
        ],
        &["traces: Flow("],
    );
}

#[test]
fn owned_and_borrowed_environments_cleanup_after_all_loop_exit_kinds() {
    for (label, parameter, argument, suffix) in
        [("owned", "F", "f", "HZ"), ("borrowed", "&F", "&f", "ZH")]
    {
        let source = format!(
            r#"{INPUT}{EMIT}{DROP}
fn run<F:Fn()->i32>(f:{parameter},n:i32)->i32{{var total=0;for index in 0..n{{let guard=R{{code:48+index}};if index==0{{continue}}if index==3{{break}}total+=f()}}return total}}
fn main()->i32{{let n=input();let owned=R{{code:72}};let f=move()=>{{emit(67);return owned.code}};let total=run({argument},n);emit(90);var calls=0;if n>1{{calls=1}}if n>2{{calls=2}}return total-72*calls}}
"#
        );
        let inputs = COUNTS
            .into_iter()
            .map(|n| {
                (
                    n,
                    format!(
                        "{}{suffix}",
                        match n {
                            0 => "",
                            1 => "0",
                            2 => "0C1",
                            _ => "0C1C23",
                        }
                    ),
                    0,
                )
            })
            .collect::<Vec<_>>();
        native(label, &[("main", &source)], &inputs, &["traces: Flow("]);
    }
}

#[test]
fn an_error_after_continue_cleans_the_live_iteration_and_outer_owners() {
    let source = format!(
        r#"{INPUT}{EMIT}{DROP}enum E{{Fail}}
fn run<F:FnMut()->E!i32>(f:&mut F,n:i32)->E!i32{{let guard=R{{code:67}};var total=0;for index in 0..n{{let per=R{{code:66}};if index==0{{continue}}let value=f()?;total+=value;if index==5{{break}}}}return total}}
fn main()->E!i32{{let n=input();let held=R{{code:65}};var step=0;var f=move()->E!i32=>{{held.code;step+=1;if step==2{{return E.Fail}}return 1}};var expected=0;if n>0{{expected=n-1}}return run(&mut f,n)?-expected}}
"#
    );
    native(
        "error-after-continue",
        &[("main", &source)],
        &[
            (0, "CA".into(), 0),
            (1, "BCA".into(), 0),
            (2, "BBCA".into(), 0),
            (7, "BBBCA".into(), 1),
        ],
        &["traces: Flow("],
    );
}

#[test]
fn for_continue_steps_once_at_the_integer_maximum_and_keeps_outer_bindings() {
    let source = format!(
        r#"{EMIT}
fn lower(value:i64)->i64{{emit(76);return value}}fn upper(value:i64)->i64{{emit(72);return value}}
fn run<F:Fn(i64)->i32>(f:&F)->i32{{let index:i64=7;var total=0;for index in lower(9223372036854775805i64)..upper(9223372036854775807i64){{total+=f(index);continue}}emit(i32(index)+48);return total}}
fn main()->i32{{let f=move(index:i64)=>{{if index==9223372036854775805i64{{emit(65)}}else{{emit(66)}}return 1}};return run(&f)-2}}
"#
    );
    native(
        "maximum-step-once",
        &[("main", &source)],
        &[(0, "LHAB7".into(), 0)],
        &["traces: Flow("],
    );
}

#[test]
fn an_unconditional_break_keeps_an_actually_finite_once_contract() {
    let source = format!(
        r#"{INPUT}{EMIT}{DROP}
fn run<F:FnOnce()->i32>(f:F,n:i32)->i32{{var total=0;for index in 0..n{{total=f();break}}return total}}
fn main()->i32{{let n=input();let held=R{{code:72}};let f=move()=>{{emit(67);return held.code}};let total=run(f,n);emit(90);if n>0{{return total-72}}return total}}
"#
    );
    native(
        "finite-once-break",
        &[("main", &source)],
        &cases(|n| if n == 0 { "HZ".into() } else { "CHZ".into() }),
        &["traces: Finite("],
    );
}

#[test]
fn flow_children_keep_their_continuations_when_the_parent_skips_or_breaks() {
    let source = format!(
        r#"{INPUT}{EMIT}
fn leaf<F:Fn()->i32>(f:&F,n:i32)->i32{{var total=0;for index in 0..n{{if index==0{{continue}}if index==3{{break}}total+=f()}}return total}}
fn run<F:Fn()->i32>(f:&F,n:i32)->i32{{var left=n;var total=0;while left>0{{left-=1;if left==1{{continue}}total+=leaf(f,5);if left==0{{break}}}}return total}}
fn main()->i32{{let n=input();let f=move()=>{{emit(65);return 3}};var count=n;if n>=2{{count=n-1}}return run(&f,n)-6*count}}
"#
    );
    native(
        "flow-child-control",
        &[("main", &source)],
        &cases(|n| "A".repeat(2 * usize::from(if n >= 2 { n - 1 } else { n }))),
        &["traces: Flow("],
    );
}

#[test]
fn control_flow_does_not_separate_an_explicit_argument_from_the_same_capture() {
    reject(
        "same-current-argument",
        r#"
fn run<F:Fn(&mut i32)->i32>(f:&F,value:&mut i32,n:i32)->i32{var total=0;for index in 0..n{if index==0{continue}total+=f(value);break}return total}
fn main()->i32{var value=1;let f=(p:&mut i32)=>{*p+=1;return value};return run(&f,&mut value,2)}
"#,
        Phase::Verify,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}
