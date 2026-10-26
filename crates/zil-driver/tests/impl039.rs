//! Transparent aliases and generic conditional patterns reach native code.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-039-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "impl039.zil", text)
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
fn generic_enum_conditions_compose_with_inferred_closure_and_ct_values() {
    native(
        "enum Option<T>{None,Some(T)} fn nth(n:i64)->Option<i64>{if n<3{return Option.Some(n)} return Option.None} fn result()->i64{var i:i64=0;var total:i64=0;let apply=(f,x)=>f(x);while let Option.Some(n)=nth(i){total+=apply(double,n);i+=1} let pair:(i64,i64)=#((2,3));if let (a,b)=pair {return total+a+b}} fn double(x:i64)->i64{return x*2}",
        Some(11),
    );
}
#[test]
fn nested_generic_if_let_bindings_feed_calls_without_escaping_success_scope() {
    native(
        "enum Result<E,T>{Err(E),Ok(T)} fn id<T>(x:T)->T=>x; fn pair()->Result<bool,(i64,i64)>{return Result.Ok((20,22))} fn result()->i64{let a:i64=40;var score:i64=2;if let Result.Ok((a,b))=pair(){score=id(a)+b}return a+score}",
        Some(82),
    );
    let r = compile_source(
        &source(
            "enum Option<T>{None,Some(T)} fn result()->i64{let x:Option<i64>=Option.None;if let Option.Some(n)=x{return n}else{return n}}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, "E-UNKNOWN-NAME", "{}", r.to_json());
}

#[test]
fn aliases_are_transparent_in_signatures_aggregates_addresses_and_callbacks() {
    let llvm = native(
        "type Number=Next; type Next=i64; type Pair=(Number,Number); type Vector=[Number;2]; type Callback=fn(Number)->Number; type View=&Number; fn plus(x:Number)->Number{return x+1} fn read(p:View)->Number{return *p} fn result()->Number{let pair:Pair=(20,21);let vector:Vector=[pair.0,pair.1];let f:Callback=plus;return f(vector[0])+read(&vector[1])}",
        Some(42),
    );
    assert!(!llvm.contains("@malloc"));
}

#[test]
fn concrete_nominal_aliases_construct_and_pattern_match_without_new_identity() {
    native(
        "type Row=Cell<i64>; type Maybe=(Option<Row>); enum Option<T>{None,Some(T)} struct Cell<T>{value:T;} fn make()->Maybe{return Maybe.Some(Row{value:42})} fn result()->i64{if let Maybe.Some(row)=make(){return row.value}return 0}",
        Some(42),
    );
    native(
        "enum Flag{Off,On(i64)} type Alias=Flag; fn result()->i64{let a:Alias=Flag.On(42);match a{Alias.Off()=>{return 0},Alias.On(n)=>{return n}}}",
        Some(42),
    );
    for text in [
        "enum Option<T>{None,Some(T)} type Small=Option<i32>; fn result()->i64{let x:Option<i64>=Option.Some(42);match x{Small.None()=>{return 0},Small.Some(n)=>{return n}}}",
        "enum Option<T>{None,Some(T)} type Small=Option<i32>; fn unused<T>(x:Option<T>)->i64{match x{Small.None()=>{return 0},Small.Some(n)=>{return 1}}} fn result()->i64=>0",
        "enum Option<T>{None,Some(T)} type Small=Option<i32>; fn result()->i64{let x:Option<i64>=Small.Some(42);return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-GENERIC-ARGUMENT"),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn generic_parameters_and_lexical_values_shadow_aliases_in_their_namespace() {
    native(
        "type T=i64; fn id<T>(x:T)->T{return x} struct Box<T>{value:T;} fn result()->i64{let b:Box<bool>=Box::<bool>{value:id(true)};if b.value{return 42}return 0}",
        Some(42),
    );
    native(
        "struct Payload{value:i64;} type Packet=Payload; fn read(Packet:Packet)->i64{return Packet.value} fn result()->i64{let Packet=Payload{value:40};let f=(Packet:Payload)=>Packet.value;return read(Packet)+f(Payload{value:2})}",
        Some(42),
    );
    let numeric = compile_source(
        &source("type T=i64;fn bad<T>(x:T)->T{return x+x}fn result()->i64=>0"),
        &CompileOptions::default(),
    );
    assert_eq!(
        numeric.status,
        ResultCategory::SafeReject,
        "{}",
        numeric.to_json()
    );
}

#[test]
fn aliases_normalize_method_receivers_and_preserve_borrow_escape_rejection() {
    native(
        "type Alias=Record;struct Record{value:i64;}impl Alias(self:&Alias){fn get()->i64=>.value}fn result()->i64{let r=Alias{value:21};return r.get()+Alias.get(&r)}",
        Some(42),
    );
    let report = compile_source(
        &source("type View=&i64;fn bad()->View{let x:i64=42;return &x}fn result()->i64=>0"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-ESCAPE"),
        "{}",
        report.to_json()
    );
}

#[test]
fn unused_aliases_are_checked_without_erasing_invalid_definitions() {
    native(
        "type Alias<T> = T;fn result()->Alias<i64>{return 42}",
        Some(42),
    );
    for text in [
        "type A=Missing; fn result()->i64=>0",
        "type A=B;type B=A;fn result()->i64=>0",
        "type A=A;fn result()->i64=>0",
        "type A=i64;type A=bool;fn result()->i64=>0",
        "struct A{x:i64;}type A=i64;fn result()->i64=>0",
        "type i64=bool;fn result()->i64=>0",
        "struct Box<T>{value:T;}type A=Box;fn result()->i64=>0",
        "type A=i64<bool>;fn result()->i64=>0",
        "type A=i64;fn result()->A<bool> =>0",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
    for text in [
        "#[doc = \"help\"] type Alias=i64;fn result()->i64=>0",
        "type Alias=i64?;fn result()->i64=>0",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn alias_expansion_checks_depth_and_growth_before_copying_expanded_types() {
    for (text, limits) in [
        (
            "type A0=i64;type A1=(A0,A0);type A2=(A1,A1);type A3=(A2,A2);type A4=(A3,A3);type A5=(A4,A4);fn result()->i64=>0",
            Limits {
                max_nodes: 140,
                ..Limits::default()
            },
        ),
        (
            "type A0=i64;type A1=(A0,);type A2=(A1,);type A3=(A2,);type A4=(A3,);fn result()->i64=>0",
            Limits {
                max_call_depth: 3,
                ..Limits::default()
            },
        ),
    ] {
        let unit = source(text);
        let parsed = zil_syntax::parse_with_limits(&unit, &Limits::default());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let errors =
            zil_expand::expand_with_map_limits(&unit, &parsed.module, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "E-RESOURCE-LIMIT" && e.phase == Phase::Expand),
            "{errors:?}"
        );
    }
}

#[test]
fn alias_example_executes_concrete_generic_while_let() {
    native(include_str!("../../../examples/impl039/main.zil"), Some(42));
}
