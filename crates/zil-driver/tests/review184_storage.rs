//! Non-author storage identity, field separation, and result lifetime controls.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}
fn directory() -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "review184-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn compile(files: &[(&str, &str)], options: CompileOptions, dir: &Path) -> CompileReport {
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..options
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn check(source: &str) -> CompileReport {
    let dir = directory();
    compile(&[("main", source)], CompileOptions::default(), &dir)
}
fn native(files: &[(&str, &str)], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        let executable = dir.join("program");
        let report = compile(
            files,
            CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                ..CompileOptions::default()
            },
            &dir,
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        fs::write(dir.join("exit.txt"), format!("{:?}", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert!(output.stdout.is_empty());
    }
}

const COUNT: &str =
    "fn counter(start:i32)->impl FnMut()->&i32{var n=start;return move()=>{n+=1;return &n}}";

#[test]
fn copying_a_shared_observed_environment_creates_distinct_storage() {
    let source = format!(
        "{COUNT}fn main()->i32{{var f=counter(4);let first=f();var g=f;let second=g();return *first+*second-11}}"
    );
    native(&[("main", &source)], 0);
}

#[test]
fn a_live_result_blocks_only_its_actual_mutable_environment() {
    let source = format!(
        "{COUNT}fn main()->i32{{var f=counter(4);var g=counter(40);let a=f();let b=g();return *a+*b-46}}"
    );
    native(&[("main", &source)], 0);
    let bad = source.replace("let b=g()", "let b=f()");
    let report = check(&bad);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.phase,
        zil_base::Phase::Verify,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT")
    );
}

#[test]
fn a_lent_callback_payload_keeps_its_field_while_another_storage_field_grows() {
    let factory = "pub fn make(n:u8)->impl Fn()->&[u8]{let b=bytes_new();bytes_reserve(b,2);bytes_push(b,n);return move()=>bytes_view(b)}";
    let state = "pub package State<F>{manages other:ByteBuffer;pub callback:F;pub count:i32}pub fn state<F:Fn()->&[u8]>(f:F)->State<F>{return State::<F>{other:bytes_new(),callback:f,count:0}}pub fn inspect<F:Fn()->&[u8]>(s:State<F>)->i32{let cb=&s.callback;let view=cb();bytes_reserve(s.other,4);return i32(view[0])-7}";
    let main = "import factory;import state;fn main()->i32{return state.inspect(state.state(factory.make(7u8)))}";
    native(&[("main", main), ("factory", factory), ("state", state)], 0);
    let bad = state.replace("bytes_reserve(s.other,4)", "let moved=s");
    let dir = directory();
    let report = compile(
        &[("main", main), ("factory", factory), ("state", &bad)],
        CompileOptions::default(),
        &dir,
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.phase,
        zil_base::Phase::Verify,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT")
    );
}

#[test]
fn ordinary_reference_forwarding_does_not_erase_the_environment_origin() {
    let helper = "fn one(p:&i32)->&i32{return p}fn two(p:&i32)->&i32{return one(p)}pub fn peek<F:Fn()->&i32>(f:&F)->&i32{return two(f())}";
    let factory = "pub fn make(n:i32)->impl Fn()->&i32{return move()=>&n}";
    native(
        &[
            (
                "main",
                "import helper;import factory;fn main()->i32{let f=factory.make(19);let a=helper.peek(&f);let b=f();return *a+*b-38}",
            ),
            ("helper", helper),
            ("factory", factory),
        ],
        0,
    );
    for body in [
        "let value=n;return &value",
        "if flag{let value=n;return &value};return &n",
    ] {
        let source = format!(
            "fn make(n:i32)->impl Fn(bool)->&i32{{return move(flag)=>{{{body}}}}}fn main(){{}}"
        );
        let report = check(&source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, zil_base::Phase::Types, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-LENDING-ORIGIN")
        );
    }
}
