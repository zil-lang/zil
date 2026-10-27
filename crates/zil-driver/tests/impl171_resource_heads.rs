//! Actual shared resource heads through independent verification and native code.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
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
        "resource-heads171-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(files: &[(&str, &str)]) -> CompileReport {
    let dir = directory();
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}

fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
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
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
    }
}

const RESOURCE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
impl R{fn peek(self:&Self)->i32=>.n;fn consume(give self:Self)->i32=>.n}
fn read(r:&R)->i32=>r.n
fn write(r:R,n:i32){r.n=n}
"#;

#[test]
fn original_159_resource_address_receiver_probe_is_unchanged_and_now_supported() {
    let report=check(&[("main", "trait Read{fn read(self:&Self)->i32;}resource C{x:i32}impl Read for C{fn read()->i32=>.x}fn main(){}")]);
    assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
}

#[test]
fn actual_head_trait_and_inherent_methods_and_plain_parameters() {
    native(&[("main", r#"
trait Read{fn read(self:&Self)->i32;}
resource C{x:i32}
impl Read for C{fn read()->i32=>.x}
impl C{fn twice(self:&Self)->i32=>.x+.x}
fn inspect(p:&C)->i32=>p.read()+p.twice()
fn main()->i32{let c=C{x:7};return inspect(&c)-21}
"#)],b"",0);
    let source=format!("{RESOURCE} fn main()->i32{{let r=R{{code:65,n:42}};let p=&r;if p.peek()!=42||read(p)!=42{{return 1}};return (give r).consume()-42}}");
    native(&[("main",&source)],b"A",0);
}

#[test]
fn last_use_then_descriptor_write_or_head_replacement_is_allowed() {
    let source=format!("{RESOURCE} fn main()->i32{{var r=R{{code:65,n:1}};let p=&r;if p.n!=1{{return 1}};write(r,42);if r.n!=42{{return 2}};r=R{{code:66,n:43}};return r.peek()-43}}");
    native(&[("main",&source)],b"AB",0);
}

#[test]
fn active_head_prevents_write_move_replacement_and_old_address_revival() {
    for action in [
        "write(r,43)",
        "r.n=43",
        "let d=r;write(d,43)",
        "let n=(give r).consume()",
        "r=R{code:66,n:43}",
        "let n=(give r).consume();r=R{code:66,n:43}",
    ] {
        let source=format!("{RESOURCE} fn main()->i32{{var r=R{{code:65,n:42}};let p=&r;{action};return p.n}}");
        let report=check(&[("main",&source)]);
        assert_eq!(report.status,ResultCategory::SafeReject,"{action}: {}",report.to_json());
        assert!(report.diagnostics.iter().any(|d| d.code=="E-BORROW-CONFLICT"),"{action}: {}",report.to_json());
    }
}

#[test]
fn heads_are_not_descriptors_mutable_windows_returns_or_captures() {
    for body in [
        "fn main(){let r=R{code:65,n:42};let d=r;let p=&d}",
        "fn main(){var r=R{code:65,n:42};let p=&mut r}",
        "fn bad(p:&R)->&R{return p}fn main(){}",
        "fn bad(p:&R)->&i32{return &p.n}fn main(){}",
        "fn main(){let r=R{code:65,n:42};let p=&r;let f=()=>p.n}",
        "fn main(){let r=R{code:65,n:42};let f=move()=>{let p=&r;return p.n}}",
        "fn main(){let b=bytes_new();let p=&b}",
    ] {
        let source=format!("{RESOURCE}{body}");
        let report=check(&[("main",&source)]);
        assert_eq!(report.status,ResultCategory::Unsupported,"{body}: {}",report.to_json());
        assert_eq!(report.phase,zil_base::Phase::Types,"{body}: {}",report.to_json());
    }
}

#[test]
fn descriptor_and_head_parameters_do_not_assume_distinct_origins() {
    for relay in [
        "fn effect(d:R,h:&R)->i32{write(d,43);return h.n}",
        "fn helper(d:R){write(d,43)}fn effect(d:R,h:&R)->i32{helper(d);return h.n}",
    ] {
        let source=format!("{RESOURCE}{relay}fn main()->i32{{let r=R{{code:65,n:42}};return effect(r,&r)}}");
        let report=check(&[("main",&source)]);
        assert_eq!(report.status,ResultCategory::SafeReject,"{}",report.to_json());
        assert!(report.diagnostics.iter().any(|d|d.code=="E-BORROW-CONFLICT"),"{}",report.to_json());
        let disjoint=source.replace("return effect(r,&r)","let other=R{code:66,n:42};return effect(r,&other)-42");
        native(&[("main",&disjoint)],b"BA",0);
    }
}

#[test]
fn descriptor_capturing_callbacks_cannot_write_under_a_live_head() {
    for make_call in ["let d=r;let f=()=>write(d,43);f()", "let d=r;let f=()=>write(d,43);let g=move()=>f();g()"] {
        let source=format!("{RESOURCE}fn main()->i32{{let r=R{{code:65,n:42}};let p=&r;{make_call};return p.n}}");
        let report=check(&[("main",&source)]);
        assert_ne!(report.status,ResultCategory::Success,"{}",report.to_json());
        assert_ne!(report.phase,zil_base::Phase::Parse,"{}",report.to_json());
    }
}

#[test]
fn unrelated_owned_callback_and_shared_head_compose() {
    let source=format!("{RESOURCE}fn factory()->impl Fn()->i32{{let r=R{{code:66,n:2}};return move()=>r.n}}fn main()->i32{{let r=R{{code:65,n:40}};let p=&r;let f=factory();return p.n+f()-42}}");
    native(&[("main",&source)],b"BA",0);
}
