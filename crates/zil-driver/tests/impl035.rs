//! Generic finite enums: symbolic admission and the ordinary native pipeline.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil-generic-enum-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "generic-enums.zil", s)
}
fn native(s: &str, expected: i64) -> String {
    let dir = Temp::new();
    let r = compile_source(
        &source(s),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(dir.0.join("plain")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    for optimized in [false, true] {
        let exe = if optimized {
            let exe = dir.0.join("optimized");
            let r = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&r.artifacts["llvm"])
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
            exe
        } else {
            r.artifacts["executable"].clone()
        };
        let result = Command::new(exe).output().unwrap();
        assert!(result.status.success(), "{result:?}");
        assert_eq!(result.stdout, format!("{expected}\n").as_bytes());
        assert!(result.stderr.is_empty());
    }
    fs::read_to_string(&r.artifacts["llvm"]).unwrap()
}
fn reject(s: &str, code: &str) {
    let r = compile_source(&source(s), &CompileOptions::default());
    assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
fn expand(s: &str, limits: &Limits) -> zil_base::CompileResult<zil_expand::ExpandedModule> {
    let src = source(s);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    zil_expand::expand_with_map_limits(&src, &ast, limits)
}
#[test]
fn generic035_01_option_factories_match_and_context_native() {
    native(
        "enum Option<T>{None,Some(T)} fn some<T>(value:T)->Option<T>{return Option.Some(value)} fn empty<T>()->Option<T>{return Option.None} fn unwrap<T>(value:Option<T>,fallback:T)->T{match value{Option.Some(n)=>{return n},Option.None()=>{return fallback}}} fn result()->i64{let a:Option<i64>=some(40);let b:Option<i64>=empty();return unwrap(a,0)+unwrap(b,2)}",
        42,
    );
}
#[test]
fn generic035_02_result_two_arguments_and_explicit_numeric_context() {
    native(
        "enum Result<E,T>{Err(E),Ok(T)} fn ok<E,T>(value:T)->Result<E,T>{return Result.Ok(value)} fn resolve<E,T>(value:Result<E,T>,fallback:T)->T{match value{Result.Ok(n)=>{return n},Result.Err(_)=>{return fallback}}} fn result()->i64{let a:Result<bool,i64>=ok(5000000000);let b=Result::<bool,i64>.Err(true);let c=Result::<bool,i64>.Ok(40);return resolve(a,0)+resolve(b,2)+resolve(c,0)}",
        5000000042,
    );
}
#[test]
fn generic035_03_nested_enum_and_tuple_patterns() {
    native(
        "enum Option<T>{None,Some(T)} enum Result<E,T>{Err(E),Ok(T)} fn pick<T>(v:Option<Result<bool,T>>,fallback:T)->T{match v{Option.None()=>{return fallback},Option.Some(Result.Ok(n))=>{return n},Option.Some(Result.Err(_))=>{return fallback}}} fn result()->i64{let a:Option<Result<bool,(i64,i64)>>=Option.Some(Result.Ok((30,10)));let b:Option<Result<bool,(i64,i64)>>=Option.Some(Result.Err(false));let c:Option<Result<bool,(i64,i64)>>=Option.None;let x=pick(a,(0,0));let y=pick(b,(1,1));let z=pick(c,(0,0));return x.0+x.1+y.0+y.1+z.0+z.1}",
        42,
    );
}
#[test]
fn generic035_04_record_and_array_payloads() {
    native(
        "enum Option<T>{None,Some(T)} struct Packet{values:[i64;2];flag:bool;unit:();} fn some<T>(v:T)->Option<T>{return Option.Some(v)} fn result()->i64{let packet=Packet{values:[39,3],flag:true,unit:()};let value=some(packet);match value{Option.None()=>{return 0},Option.Some(p)=>{if p.flag{return p.values[0]+p.values[1]}return 0}}}",
        42,
    );
    native(
        "enum Option<T>{None,Some(T)} fn some<T>(v:T)->Option<T>{return Option.Some(v)} fn result()->i64{let x:Option<[i64;2]>=some([39,3]);match x{Option.None()=>{return 0},Option.Some(a)=>{return a[0]+a[1]}}}",
        42,
    );
}
#[test]
fn generic035_05_instances_arrays_updates_and_local_context() {
    native(
        "enum Option<T>{None,Some(T)} fn score(x:Option<i64>)->i64{match x{Option.None()=>{return 0},Option.Some(n)=>{return n}}} fn result()->i64{var values:[Option<i64>;3]=[Option.Some(10),Option.None,Option.Some(2)];values[1]=Option.Some(30);var x:Option<i64>=Option.None;var i=0;while i<3{x=values[i];i+=1}return score(values[0])+score(values[1])+score(x)}",
        42,
    );
}
#[test]
fn generic035_06_generic_match_move_states_and_shadowing() {
    native(
        "enum Option<T>{None,Some(T)} fn keep<T>(v:Option<T>,x:T)->T{match v{Option.None()=>{},Option.Some(x)=>{x;}}return x} fn result()->i64{return keep(Option::<i64>.Some(40),42)}",
        42,
    );
    for text in [
        "enum Option<T>{None,Some(T)} fn bad<T>(v:Option<T>)->(T,T){match v{Option.Some(x)=>{return (x,x)},Option.None()=>{return bad(v)}}} fn result()->i32=>0",
        "enum Option<T>{None,Some(T)} fn bad<T>(v:Option<i32>,x:T)->T{match v{Option.None()=>{x;let x=1;},Option.Some(_)=>{}}return x} fn result()->i32=>0",
    ] {
        reject(text, "E-GENERIC-ARGUMENT");
    }
}
#[test]
fn generic035_07_uninstantiated_bad_definitions_and_layout_reject() {
    for text in [
        "enum Bad<T>{A,A} fn result()->i32=>0",
        "enum Bad<T>{A(Missing)} fn result()->i32=>0",
        "enum Option<T>{None,Some(T)} enum Bad<T>{A(Option<T,i32>)} fn result()->i32=>0",
        "enum Phantom<T>{Unit} enum Bad<T>{A(Phantom<Missing>)} fn result()->i32=>0",
    ] {
        reject(text, "E-GENERIC-ARGUMENT");
    }
    for text in [
        "enum Bad<T>{A(Bad<T>)} fn result()->i32=>0",
        "enum Bad<T>{A(Box<Bad<T>>)} struct Box<U>{v:U;} fn result()->i32=>0",
    ] {
        reject(text, "E-INFINITE-LAYOUT");
    }
    for text in [
        "enum Bad<T>{A(&T)} fn result()->i32=>0",
        "enum Bad<T>{A([T;true])} fn result()->i32=>0",
    ] {
        let r = expand(text, &Limits::default()).unwrap_err();
        assert_eq!(r[0].category, ResultCategory::Unsupported);
    }
}
#[test]
fn generic035_08_missing_constraints_nominality_and_payload_mismatch_reject() {
    for text in [
        "enum Option<T>{None,Some(T)} fn result()->i32{let x=Option.None;return 0}",
        "enum Result<E,T>{Err(E),Ok(T)} fn result()->i32{let x=Result.Ok(1);return 0}",
        "enum Option<T>{None,Some(T)} fn result()->i32{let x:Option<i64>=Option.Some(true);return 0}",
        "enum Option<T>{None,Some(T)} fn result()->i32{let x=Option::<i32,i64>.Some(1);return 0}",
        "enum Option<T>{None,Some(T)} fn result()->i32{let x=Option::<i32>.Missing;return 0}",
        "enum Option<T>{None,Some(T)} enum Other<T>{None,Some(T)} fn result()->i32{let x:Option<i32>=Other.Some(1);return 0}",
    ] {
        reject(text, "E-GENERIC-ARGUMENT");
    }
}
#[test]
fn generic035_09_unused_generic_match_is_checked_before_erasure() {
    reject(
        "enum Option<T>{None,Some(T)} fn bad<T>(v:Option<T>)->i32{match v{Option.Some(_)=>{return 1}}} fn result()->i32=>0",
        "E-MATCH-NONEXHAUSTIVE",
    );
    reject(
        "enum Option<T>{None,Some(T)} fn bad<T>(v:Option<T>)->i32{match v{_=>{return 1},Option.Some(_)=>{return 2}}} fn result()->i32=>0",
        "E-MATCH-UNREACHABLE",
    );
    reject(
        "enum Option<T>{None,Some(T)} fn bad<T>(v:Option<T>)->i32{match v{Option.None()=>{return 1},Option.Some(x)=>{return x+1}}} fn result()->i32=>0",
        "E-UNSUPPORTED",
    );
    reject(
        "enum Option<T>{None,Some(T)} fn bad<T>(v:Option<T>)->i32{match v{Option.None()=>{return 1},Option.Some()=>{return 2}}} fn result()->i32=>0",
        "E-GENERIC-ARGUMENT",
    );
}
#[test]
fn generic035_10_expansion_budgets_cover_unused_layout_and_instances() {
    let text = "enum Option<T>{None,Some(T)} fn result()->i64{let x=Option::<i64>.Some(42);match x{Option.None()=>{return 0},Option.Some(n)=>{return n}}}";
    assert!(expand(text, &Limits::default()).is_ok());
    for limits in [
        Limits {
            max_nodes: 30,
            ..Limits::default()
        },
        Limits {
            max_analysis_steps: 80,
            ..Limits::default()
        },
        Limits {
            max_functions: 0,
            ..Limits::default()
        },
    ] {
        let errors = expand(text, &limits).unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    }
    let text = "enum Infinite<T>{More(Infinite<(T,T)>)} fn result()->i32=>0";
    let errors = expand(
        text,
        &Limits {
            max_call_depth: 12,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}
#[test]
fn generic035_11_recursive_factories_and_nested_zero_argument_calls() {
    native(
        "enum Option<T>{None,Some(T)} fn empty<T>()->Option<T>{return Option.None} fn relay<T>()->Option<T>{return empty()} fn some<T>(v:T,b:bool)->Option<T>{if b{return Option.Some(v)}return some(v,true)} fn result()->i64{let missing:Option<i64>=relay();let present=some::<i64>(42,false);match (missing,present){(Option.None(),Option.Some(n))=>{return n},(_,_)=>{return 0}}}",
        42,
    );
}
#[test]
fn generic035_12_symbolic_bool_matrix_and_integer_aliases() {
    native(
        "enum E<T>{Pair(T,bool,bool),None} fn score<T>(x:E<T>)->i64{match x{E.None()=>{return 0},E.Pair(_,true,_)=>{return 10},E.Pair(_,false,true)=>{return 20},E.Pair(_,false,false)=>{return 12}}} fn result()->i64{return score(E::<i64>.Pair(0,true,false))+score(E::<bool>.Pair(true,false,true))+score(E::<()>.Pair((),false,false))}",
        42,
    );
    reject(
        "enum E<T>{A(T,i32)} fn bad<T>(x:E<T>)->i32{match x{E.A(_,1)=>{return 1},E.A(_,0x1)=>{return 2},_=>{return 3}}} fn result()->i32=>0",
        "E-MATCH-UNREACHABLE",
    );
}
#[test]
fn generic035_13_full_example() {
    let text = include_str!("../../../examples/impl035/main.zil")
        .replace("fn answer()", "fn result()")
        .replace("answer() ==", "result() ==");
    let llvm = native(&text, 5000000042);
    assert!(!llvm.contains("malloc"));
    assert!(!llvm.contains(" undef"));
}
#[test]
fn generic035_14_record_field_context_and_generic_record_payload() {
    native(
        "enum Option<T>{None,Some(T)} struct Box<U>{value:U;} struct Holder{value:Option<i64>;} fn id<T>(x:T)->T=>x; fn result()->i64{let h=Holder{value:Option.None};let b=Option.Some(Box::<i64>{value:42});match (h.value,b){(Option.None(),Option.Some(v))=>{return id(v.value)},(_,_)=>{return 0}}}",
        42,
    );
}
#[test]
fn generic035_15_explicit_and_inferred_applications_share_one_instance() {
    let text = "enum Option<T>{None,Some(T)} fn id<T>(x:T)->T=>x; fn result()->i64{let x=Option::<i64>.Some(21);let a=id(x);let b=id::<Option<i64>>(x);match (a,b){(Option.Some(m),Option.Some(n))=>{return m+n},(_,_)=>{return 0}}}";
    let core = expand(
        text,
        &Limits {
            max_functions: 2,
            ..Limits::default()
        },
    )
    .unwrap()
    .core;
    assert!(core.unsupported.is_empty());
    assert_eq!(core.enums.len(), 1);
    assert_eq!(
        core.functions
            .iter()
            .filter(|f| f.name.starts_with("$generic_id_"))
            .count(),
        1
    );
    native(text, 42);
}
#[test]
fn generic035_16_unused_payload_literal_ranges_and_signed_minimum() {
    for text in [
        "enum Option<T>{None,Some(T)} fn bad<T>(x:T)->Option<i32>{return Option.Some(5000000000)} fn result()->i32=>0",
        "enum Option<T>{None,Some(T)} fn bad<T>(x:T)->Option<i64>{return Option.Some(-9223372036854775809)} fn result()->i32=>0",
    ] {
        reject(text, "E-GENERIC-ARGUMENT");
    }
    native(
        "enum Option<T>{None,Some(T)} fn minimum<T>(x:T)->Option<i64>{return Option.Some(-9223372036854775808)} fn result()->i64{match minimum(()){Option.None()=>{return 0},Option.Some(n)=>{return n}}}",
        i64::MIN,
    );
}
#[test]
fn generic035_17_grouped_enum_types_remain_transparent() {
    for ty in ["(Option<i64>)", "((Option<i64>))"] {
        let text = format!(
            "enum Option<T>{{None,Some(T)}} fn result()->i64{{let x:{ty}=Option.Some(42);match x{{Option.None()=>{{return 0}},Option.Some(n)=>{{return n}}}}}}"
        );
        native(&text, 42);
    }
}
