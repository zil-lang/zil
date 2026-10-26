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
fn generic_records_recursive_instances_and_comptime_compose() {
    native(
        include_str!("../../../examples/mainline/generic_records.zil"),
        Some(5507),
    );
}

#[test]
fn enum_payload_uses_thin_pointer_layout_and_preserves_callable_identity() {
    native(
        "enum Action { Empty, Run(fn(i64)->i64), } fn twice(x:i64)->i64{return x*2} fn result()->i64{let a=Action.Run(twice);match a {Action.Run(f)=>{return f(5000000000)},Action.Empty()=>{return 0},}}",
        Some(10000000000),
    );
}

#[test]
fn comptime_aggregates_compose_with_enum_dispatch_and_stateful_closures() {
    native(
        "enum Job { None, Run([i64;2],fn(i64)->i64), } fn scale(x:i64)->i64{return x*2} fn table()->[i64;2]{return [5000000000,3]} fn count(xs:[i64;2])->i64{var state=xs;var bump=move n=>{state[1]+=n;state};let a=bump(4);let b=bump(2);return a[1]+b[1]} fn result()->i64{let input=#(table()); let j=Job.Run(input,scale);match j {Job.None()=>{return 0},Job.Run(xs,transform)=>{return transform(xs[0])+count(xs);},}}",
        Some(10000000016),
    );
}

#[test]
fn comptime_enum_and_stateful_closure_in_pattern_arm_compose() {
    native(
        "enum Job { None, Run([i64;2],fn(i64)->i64), } fn scale(x:i64)->i64{return x*2} fn table()->[i64;2]{return [5000000000,3]} fn result()->i64{let input=#(table()); let j=Job.Run(input,scale);match j {Job.None()=>{return 0},Job.Run(xs,transform)=>{var state=xs; var bump=move n=>{state[1]+=n;state}; let a=bump(4);let b=bump(2); return transform(a[0])+a[1]+b[1];},}}",
        Some(10000000016),
    );
}
