//! Pattern conditions run through ordinary typed CFG and independent verification.
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{CorePattern, CorePatternKind, CoreStatementKind};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil037-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(s: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "impl037.zil", s)
}
fn native(s: &str, expected: i64) {
    let t = Temp::new();
    let report = compile_source(
        &source(s),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(t.0.join("plain")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let optimized = t.0.join("optimized");
    let result = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    for executable in [&report.artifacts["executable"], &optimized] {
        let observed = Command::new(executable).output().unwrap();
        assert!(observed.status.success(), "{observed:?}");
        assert_eq!(observed.stdout, format!("{expected}\n").as_bytes());
        assert!(observed.stderr.is_empty());
    }
}
fn reject(s: &str, code: &str) {
    let r = compile_source(&source(s), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
#[test]
fn pattern037_example_break_continue_and_unit_variant() {
    native(include_str!("../../../examples/impl037/main.zil"), 1607);
}
#[test]
fn pattern037_nested_patterns_else_if_and_outer_shadow() {
    native(
        "enum Inner {None,Some(i64)} enum Outer {Empty,Pair((bool,Inner))} fn score(v:Outer)->i64 {let n:i64=100;if let Outer.Pair((true,Inner.Some(n)))=v {return n} else if let Outer.Pair((_,Inner.None()))=v {return 20} else {return n}} fn result()->i64 {return score(Outer.Empty)+score(Outer.Pair((false,Inner.None)))+score(Outer.Pair((true,Inner.Some(42))))}",
        162,
    );
}
#[test]
fn pattern037_fnmut_condition_once_and_failed_match_rechecks() {
    native(
        "enum E {End,N(i64)} fn step(i:i64)->E {if i<4 {return E.N(i)} return E.End} fn result()->i64 {var calls:i64=0;var produce=move () => {calls+=1;(step(calls),calls)};var total:i64=0;while let (E.N(n),count)=produce() {if count==2 {continue} total+=n} let last=produce();return total*10+last.1+calls}",
        45,
    );
}
#[test]
fn pattern037_closure_body_inferred_tuple_and_enum_conditions() {
    native(
        "enum E {End,N(i64)} fn step(i:i64)->E {if i>0 {return E.N(i)} return E.End} fn result()->i64 {let score=(value:E) => {if let E.N(n)=value {n+1} else {0}};let pair=p => {if let (true,n)=p {n} else {0}};let p:(bool,i64)=(true,5000000000);return score(step(41))+pair(p)}",
        5000000042,
    );
}
#[test]
fn pattern037_closure_loop_conditions_and_local_lifetimes() {
    native(
        "fn result()->i64 {var state:i64=0;var work=move (end:i64) => {var total:i64=0;while let true=state<end {state+=1;let n=state;if let (true,value)=(n%2==0,n) {let p=&value;total+=*p}} total};let a=work(6);let b=work(8);return a*100+b}",
        1208,
    );
}
#[test]
fn pattern037_match_inside_closure_and_closure_inside_match() {
    native(
        "enum Packet {Empty,Data((bool,i64))} fn packet(value:i64)->Packet {return Packet.Data((true,value))} fn score(value:Packet)->i64 {let bonus:i64=2;match value {Packet.Empty()=>{return 0},Packet.Data((true,n))=>{let inc=move x=>x+n+bonus;return inc(1)},Packet.Data((false,_))=>{return 0}}} fn result()->i64 {let pick=(value:Packet)=>{match value {Packet.Empty()=>{return 0},Packet.Data((true,n))=>{return n},Packet.Data((false,n))=>{return -n}}};return score(packet(39))+pick(packet(10))}",
        52,
    );
    reject(
        "fn result()->i32 {let f=(value:bool)=>{match value {true=>{return 1}}};return f(true)}",
        "E-MATCH-NONEXHAUSTIVE",
    );
    reject(
        "fn result()->i32 {let f=(value:bool)=>{match value {_=>{return 1},true=>{return 2}}};return f(true)}",
        "E-MATCH-UNREACHABLE",
    );
    reject(
        "fn result()->i32 {let f=(value:(bool,i32))=>{match value {(true,n)=>{return n},(false,_)=>{return n}}};return f((true,1))}",
        "E-UNKNOWN-NAME",
    );
}
#[test]
fn pattern037_irrefutable_conditions_and_return_paths() {
    native(
        "enum Only {Value(i64)} fn take(v:Only)->i64 {if let Only.Value(n)=v {return n}} fn looped()->i64 {var n:i64=0;while let (a,())=(n,()) {n+=1;if a==3 {break}} return n} fn result()->i64 {if let (a,b)=(take(Only.Value(38)),looped()) {return a+b}}",
        42,
    );
}
#[test]
fn pattern037_array_record_and_callback_payloads() {
    native(
        "struct R {values:[i64;2];} enum Packet {Empty,Data(R,fn(i64)->i64)} fn add(x:i64)->i64=>x+2;fn result()->i64 {let packet=Packet.Data(R{values:[10,30]},add);if let Packet.Data(record,callback)=packet {return callback(record.values[0]+record.values[1])} return 0}",
        42,
    );
}
#[test]
fn pattern037_scalar_ct_loops_and_closed_queries() {
    native(
        "fn sum(end:i64)->i64 {var i:i64=0;var total:i64=0;while let true=i<end {i+=1;if let 0=i%2 {continue} total+=i} return total} fn result()->i64 {if let 42=#(6*7) {return #(sum(7))} return 0}",
        16,
    );
    let report = compile_source(
        &source("fn spin()->i32 {while let _=true {continue}} fn result()->i32 {return #(spin())}"),
        &CompileOptions {
            limits: Limits {
                max_ct_steps: 100,
                ..Limits::default()
            },
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::ResourceLimit,
        "{}",
        report.to_json()
    );
    assert_eq!(report.diagnostics[0].phase, Phase::Ct);
}
#[test]
fn pattern037_bindings_are_immutable_and_confined_to_success() {
    for text in [
        "fn result()->i32 {if let n=1 {} return n}",
        "fn result()->i32 {if let (true,n)=(false,1) {return n} else {return n}}",
        "fn result()->i32 {while let (true,n)=(false,1) {} return n}",
    ] {
        reject(text, "E-UNKNOWN-NAME");
    }
    reject(
        "fn result()->i32 {if let n=1 {n=2;return n} return 0}",
        "E-IMMUTABLE-ASSIGNMENT",
    );
    reject(
        "fn result()->i32 {let f=()=>{if let n=1 {n=2;return n} return 0};return f()}",
        "E-IMMUTABLE-ASSIGNMENT",
    );
}
#[test]
fn pattern037_invalid_shapes_names_and_nominality_reject() {
    for (text, code) in [
        (
            "fn result()->i32 {if let (x,x)=(1,2) {return x} return 0}",
            "E-MATCH-BINDING",
        ),
        (
            "fn result()->i32 {if let (x,y)=(1,2,3) {return x} return 0}",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 {if let true=1 {return 1} return 0}",
            "E-MATCH-TYPE",
        ),
        (
            "enum A {N(i32)} enum B {N(i32)} fn result()->i32 {if let B.N(n)=A.N(1) {return n} return 0}",
            "E-MATCH-TYPE",
        ),
        (
            "enum A {N(i32)} fn result()->i32 {if let A.Missing(n)=A.N(1) {return n} return 0}",
            "E-ENUM-VARIANT",
        ),
        (
            "fn result()->i32 {if let self=1 {return 1} return 0}",
            "E-RECEIVER-RESERVED-NAME",
        ),
    ] {
        reject(text, code);
    }
}
#[test]
fn pattern037_bound_storage_cannot_escape_and_ct_shadow_stays_runtime() {
    reject(
        "fn escape()->&i32 {if let n=1 {return &n}} fn result()->i32 {return *escape()}",
        "E-BORROW-ESCAPE",
    );
    reject(
        "fn helper()->i32=>7;fn result()->i32 {if let helper=1 {return #(helper())} return 0}",
        "E-CT-RUNTIME",
    );
    reject(
        "fn helper()->i32=>7;fn result()->i32 {while let (true,helper)=(false,1) {return #(helper())} return 0}",
        "E-CT-RUNTIME",
    );
    native(
        "fn helper()->i32=>7;fn result()->i32 {if let (true,helper)=(false,1) {return helper} else {return #(helper())}}",
        7,
    );
}
#[test]
fn pattern037_raw_core_pattern_depth_is_bounded_before_cloning() {
    let src = source("fn result()->i32 {if let n=1 {return n} return 0}");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let mut core = zil_expand::expand(&src, &ast).unwrap();
    let span = core.functions[0].span;
    let mut pattern = CorePattern {
        kind: CorePatternKind::Wildcard,
        span,
    };
    for _ in 0..40 {
        pattern = CorePattern {
            kind: CorePatternKind::Tuple(vec![pattern]),
            span,
        };
    }
    let CoreStatementKind::IfLet {
        pattern: original, ..
    } = &mut core.functions[0].body.statements[0].kind
    else {
        unreachable!()
    };
    *original = pattern;
    let limits = Limits {
        max_call_depth: 8,
        ..Limits::default()
    };
    let error = zil_types::infer_source_closures(&core, &limits).unwrap_err();
    assert_eq!(error[0].category, ResultCategory::ResourceLimit);
    let error =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &limits).unwrap_err();
    assert_eq!(error[0].category, ResultCategory::ResourceLimit);
    let mut ty = zil_ir::Type::I32;
    for _ in 0..40 {
        ty = zil_ir::Type::Tuple(vec![ty]);
    }
    let CoreStatementKind::IfLet { pattern, .. } = &mut core.functions[0].body.statements[0].kind
    else {
        unreachable!()
    };
    *pattern = CorePattern {
        kind: CorePatternKind::Variant {
            ty,
            variant: "N".into(),
            fields: vec![],
        },
        span,
    };
    let error =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &limits).unwrap_err();
    assert_eq!(error[0].category, ResultCategory::ResourceLimit);
}
