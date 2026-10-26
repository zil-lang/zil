//! Higher-order closure constraints execute actual indirect calls.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, ProjectSources, Stage, compile_project, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-036-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "impl036.zil", text)
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
    execute(&report, &out, expected)
}
fn execute(report: &CompileReport, out: &Temp, expected: Option<i64>) -> String {
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
fn inferred_higher_order_closure_and_mutable_pointer_fields_execute() {
    let llvm = native(
        include_str!("../../../examples/impl036/main.zil"),
        Some(5000000043),
    );
    assert!(llvm.contains("call i64 %v."));
    assert!(llvm.contains("ptr %env"));
    assert!(!llvm.contains("@malloc"));
}
#[test]
fn shared_pointer_capture_retains_whole_place_hold() {
    native(
        "fn inc(x:i64)->i64=>x+1;fn double(x:i64)->i64=>x*2;fn result()->i64{var p=inc;let apply=(x)=>p(x);let a=apply(5000000000);p=double;return a+p(4)}",
        Some(5000000009),
    );
    reject(
        "fn inc(x:i32)->i32=>x+1;fn double(x:i32)->i32=>x*2;fn result()->i32{var p=inc;let apply=(x)=>p(x);p=double;return apply(4)}",
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}
#[test]
fn module_function_values_and_static_members_work_inside_closures() {
    let out = Temp::new();
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import ops;fn result()->i64{let cb=ops.inc;let apply=(f,x)=>f(x);let local=(x)=>ops.inc(x);let choose=(flag)=>if flag {ops.inc}else{ops.double};return apply(cb,5000000000)+local(1)+choose(false)(3)}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "ops.zil",
                "pub fn inc(x:i64)->i64=>x+1;pub fn double(x:i64)->i64=>x*2;",
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["ops".into()], ModuleId(1))]),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(out.0.join("plain")),
            ..CompileOptions::default()
        },
    );
    execute(&report, &out, Some(5000000009));
    native(
        "struct P{x:i64;} impl P{fn make(x:i64)->Self=>P{x:x};fn get(self:Self)->i64=>.x;} fn result()->i64{let factory=()=>P.make;let read=p=>P.get(p);return read(factory()(5000000000))}",
        Some(5000000000),
    );
}
fn reject(text: &str, category: ResultCategory, code: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
}
#[test]
fn pointer_constraints_reject_conflicting_signature_arity_and_recursion() {
    reject(
        "fn result()->i32{let apply=(f,x)=>{f(x);f()};return 0}",
        ResultCategory::SafeReject,
        "E-CALL-ARITY",
    );
    reject(
        "fn one(x:i64)->i64=>x;fn flag(x:bool)->bool=>x;fn result()->i32{let apply=(f,x)=>f(x);apply(one,1);apply(flag,true);return 0}",
        ResultCategory::SafeReject,
        "E-TYPE-MISMATCH",
    );
    reject(
        "fn result()->i32{let recurse=f=>f(f);return 0}",
        ResultCategory::SafeReject,
        "E-INFINITE-LAYOUT",
    );
    reject(
        "fn result()->i32{let apply=(f,x)=>f(x);return 0}",
        ResultCategory::SafeReject,
        "E-CLOSURE-INFERENCE",
    );
    reject(
        "fn result()->i32{let n=3;let captured=move(x:i32)=>x+n;let apply=(f,x)=>f(x);return apply(captured,4)}",
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
}
#[test]
fn nested_function_results_tuple_values_and_unit_abi_execute() {
    native(
        "fn id(x:i64)->i64=>x;fn factory()->fn(i64)->i64=>id;fn result()->i64{let run=(f,x)=>f()(x);return run(factory,9223372036854775807)}",
        Some(i64::MAX),
    );
    native(
        "fn inc(x:i64)->i64=>x+1;fn unit(x:())->(){} fn result()->i64{let pack=f=>(f,42);let pair=pack(inc);let call=(f,x)=>f(x);call(unit,());return pair.0(5000000000)+pair.1}",
        Some(5000000043),
    );
}
#[test]
fn explicit_shared_address_capture_and_shared_aggregate_capture_execute() {
    native(
        "fn inc(x:i64)->i64=>x+1;fn result()->i64{let callback=inc;let p=&callback;let invoke=move x=>(*p)(x);return invoke(5000000000)}",
        Some(5000000001),
    );
    native(
        "fn inc(x:i64)->i64=>x+1;struct Job{f:fn(i64)->i64;n:i64;} fn result()->i64{var job=Job{f:inc,n:3};let run=x=>job.f(x)+job.n;let a=run(5000000000);job.n=9;return a+job.n}",
        Some(5000000013),
    );
    reject(
        "fn inc(x:i32)->i32=>x+1;struct Job{f:fn(i32)->i32;n:i32;} fn result()->i32{var job=Job{f:inc,n:3};let run=x=>job.f(x);job.n=9;return run(4)}",
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}
#[test]
fn expanded_callable_constraint_dag_obeys_work_and_depth_limits() {
    let mut text = String::from("fn result()->i32{let run=(");
    for i in 0..=12 {
        text.push_str(&format!("f{i},"));
    }
    text.push_str("x:i32)=>{let y0:i32=f0(x);");
    for i in 1..=12 {
        text.push_str(&format!("let y{i}:i32=f{i}(f{},f{});", i - 1, i - 1));
    }
    text.push_str("return y12};return 0}");
    let src = source(&text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    for limits in [
        Limits {
            max_analysis_steps: 2000,
            ..Limits::default()
        },
        Limits {
            max_call_depth: 8,
            ..Limits::default()
        },
    ] {
        let errors = zil_types::infer_source_closures(&core, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "{errors:?}"
        );
        assert_eq!(errors[0].phase, Phase::Types);
    }
}
#[test]
fn raw_callable_capture_shape_remains_independently_checked() {
    let src = source(
        "fn inc(x:i64)->i64=>x+1;fn result()->i64{let p=inc;let apply=x=>p(x);return apply(5000000000)}",
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &raw
    );
    raw.closures[0].captures[0] = zil_ir::Type::Address {
        mutable: false,
        inner: Box::new(zil_ir::Type::FunctionPointer {
            parameters: vec![zil_ir::Type::Bool],
            result: Box::new(zil_ir::Type::Bool),
        }),
    };
    plan.module_fingerprint = raw.fingerprint();
    let errors = zil_verify::verify_module(raw, plan, &target, &limits).unwrap_err();
    assert_eq!(errors[0].phase, Phase::Verify);
    assert!(errors[0].code.starts_with("E-VERIFY-"), "{errors:?}");
}
#[test]
fn enum_constructors_and_callback_payloads_work_before_and_inside_closures() {
    native(
        "enum Job{None,Run([i64;2],fn(i64)->i64)} fn inc(x:i64)->i64=>x+1;fn read(job:Job)->i64{match job {Job.None()=>{return 0}, Job.Run(xs,callback)=>{return callback(xs[0])+xs[1]}}} fn result()->i64{let seed=Job.Run([5000000000,7],inc);let make=x=>Job.Run([x,9],inc);let none=()=>Job.None;let Job=(inc,1);let shadowed=()=>Job.0(10);return read(seed)+read(make(6000000000))+read(none())+shadowed()}",
        Some(11000000029),
    );
    reject(
        "enum E{Value(i64)} fn result()->i32{let make=()=>E.Value;return 0}",
        ResultCategory::SafeReject,
        "E-ENUM-ARITY",
    );
    reject(
        "enum E{Value(fn(i64)->i64)} fn bad(x:bool)->bool=>x;fn result()->i32{let make=()=>E.Value(bad);return 0}",
        ResultCategory::SafeReject,
        "E-TYPE-MISMATCH",
    );
    reject(
        "enum E{Value(i64)} fn result()->i32{let make=()=>E.Missing(3);return 0}",
        ResultCategory::SafeReject,
        "E-ENUM-VARIANT",
    );
}
