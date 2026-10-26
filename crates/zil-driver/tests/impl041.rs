//! Real Copy aggregate programs, LLVM bounds checks, and forged raw controls.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-aggregate-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "aggregate.zil", text)
}
fn native(text: &str, expected: Option<i64>) -> String {
    let out = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(out.0.join("plain")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    for optimized in [false, true] {
        let executable = if optimized {
            let p = out.0.join("optimized");
            let built = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&p)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            p
        } else {
            report.artifacts["executable"].clone()
        };
        let observed = Command::new(executable).output().unwrap();
        match expected {
            Some(value) => {
                assert!(observed.status.success(), "{observed:?}");
                assert_eq!(observed.stdout, format!("{value}\n").as_bytes());
                assert!(observed.stderr.is_empty());
            }
            None => assert!(!observed.status.success(), "out-of-bounds access must trap"),
        }
    }
    llvm
}

#[test]
fn ordinary_shared_projections_are_still_available_with_unrelated_closures() {
    native(
        "struct Data{values:[i64;2];} fn read(xs:&[i64;2])->i64=>xs[0];fn result()->i64{let data=Data{values:[40,2]};let f=()=>1;return read(&data.values)+f()}",
        Some(41),
    );
    native(
        "struct Data{values:[i64;2];} fn read(xs:&[i64;2])->i64=>xs[0];fn result()->i64{let data=Data{values:[40,2]};let p=&data;let f=()=>1;return read(&p.values)+f()}",
        Some(41),
    );
}
#[test]
fn closure_body_projects_shared_capture_and_shared_parameter() {
    native(
        "struct R{values:[i64;2];} fn read(x:&i64)->i64=>*x;fn result()->i64{let data=R{values:[5000000000,2]};let f=()=>read(&data.values[1]);let p=&data;let g=move()=>read(&p.values[0]);return f()+g()}",
        Some(5000000002),
    );
    native(
        "struct R{x:i64;} fn score(p:&R)->i64{let f=()=>1;let q=&p.x;return *(&*q)+p.x+f()} fn result()->i64{let r=R{x:21};return score(&r)}",
        Some(43),
    );
    native(
        "fn result()->i64{let xs:[i64;2]=[40,2];let p=&xs;let f=move()=>p[0]+*(&p[1]);return f()}",
        Some(42),
    );
}
#[test]
fn dynamic_borrow_index_runs_once_with_fnmut_constraints() {
    native(
        "fn result()->i64{let xs:[i64;2]=[40,2];var index:i64=0;var next=move()=>{index+=1;index-1};let p=&xs[next()];let add=()=>1;return *p+next()*10+add()}",
        Some(51),
    );
    native(
        "fn result()->i64{let xs:[i64;1]=[40];let f=()=>*(&xs[1]);return f()}",
        None,
    );
}
#[test]
fn source_inference_does_not_grant_borrow_or_write_permissions() {
    let text = "struct R{x:i64;y:i64;} fn result()->i64{var r=R{x:40,y:2};let f=()=>1;let p=&r.x;r.y=7;return *p+f()}";
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.diagnostics[0].code,
        "E-BORROW-CONFLICT",
        "{}",
        report.to_json()
    );
    for text in [
        "fn result()->i64{let f=()=>1;let p=&(40+1);return *p+f()}",
        "fn result()->i64{let n:i64=41;let p=&n;let f=()=>1;let q=&p;return **q+f()}",
        "struct R{x:i64;} fn result()->i64{let r=R{x:41};let p=&r;let f=()=>1;p.x=3;return p.x+f()}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
