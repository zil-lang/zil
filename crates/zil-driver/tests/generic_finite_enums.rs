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
fn generic_enum_option_factories_match_and_context_native() {
    native(
        "enum LegacyOption<T>{None,Some(T)} fn some<T>(value:T)->LegacyOption<T>{return LegacyOption.Some(value)} fn empty<T>()->LegacyOption<T>{return LegacyOption.None} fn unwrap<T>(value:LegacyOption<T>,fallback:T)->T{match value{LegacyOption.Some(n)=>{return n},LegacyOption.None()=>{return fallback}}} fn result()->i64{let a:LegacyOption<i64>=some(40);let b:LegacyOption<i64>=empty();return unwrap(a,0)+unwrap(b,2)}",
        42,
    );
}
#[test]
fn generic_enum_result_two_arguments_and_explicit_numeric_context() {
    native(
        "enum Result<E,T>{Err(E),Ok(T)} fn ok<E,T>(value:T)->Result<E,T>{return Result.Ok(value)} fn resolve<E,T>(value:Result<E,T>,fallback:T)->T{match value{Result.Ok(n)=>{return n},Result.Err(_)=>{return fallback}}} fn result()->i64{let a:Result<bool,i64>=ok(5000000000);let b=Result::<bool,i64>.Err(true);let c=Result::<bool,i64>.Ok(40);return resolve(a,0)+resolve(b,2)+resolve(c,0)}",
        5000000042,
    );
}
#[test]
fn generic_enum_nested_enum_and_tuple_patterns() {
    native(
        "enum LegacyOption<T>{None,Some(T)} enum Result<E,T>{Err(E),Ok(T)} fn pick<T>(v:LegacyOption<Result<bool,T>>,fallback:T)->T{match v{LegacyOption.None()=>{return fallback},LegacyOption.Some(Result.Ok(n))=>{return n},LegacyOption.Some(Result.Err(_))=>{return fallback}}} fn result()->i64{let a:LegacyOption<Result<bool,(i64,i64)>>=LegacyOption.Some(Result.Ok((30,10)));let b:LegacyOption<Result<bool,(i64,i64)>>=LegacyOption.Some(Result.Err(false));let c:LegacyOption<Result<bool,(i64,i64)>>=LegacyOption.None;let x=pick(a,(0,0));let y=pick(b,(1,1));let z=pick(c,(0,0));return x.0+x.1+y.0+y.1+z.0+z.1}",
        42,
    );
}
#[test]
fn generic_enum_record_and_array_payloads() {
    native(
        "enum LegacyOption<T>{None,Some(T)} struct Packet{values:[i64;2];flag:bool;unit:();} fn some<T>(v:T)->LegacyOption<T>{return LegacyOption.Some(v)} fn result()->i64{let packet=Packet{values:[39,3],flag:true,unit:()};let value=some(packet);match value{LegacyOption.None()=>{return 0},LegacyOption.Some(p)=>{if p.flag{return p.values[0]+p.values[1]}return 0}}}",
        42,
    );
    native(
        "enum LegacyOption<T>{None,Some(T)} fn some<T>(v:T)->LegacyOption<T>{return LegacyOption.Some(v)} fn result()->i64{let x:LegacyOption<[i64;2]>=some([39,3]);match x{LegacyOption.None()=>{return 0},LegacyOption.Some(a)=>{return a[0]+a[1]}}}",
        42,
    );
}
#[test]
fn generic_enum_instances_arrays_updates_and_local_context() {
    native(
        "enum LegacyOption<T>{None,Some(T)} fn score(x:LegacyOption<i64>)->i64{match x{LegacyOption.None()=>{return 0},LegacyOption.Some(n)=>{return n}}} fn result()->i64{var values:[LegacyOption<i64>;3]=[LegacyOption.Some(10),LegacyOption.None,LegacyOption.Some(2)];values[1]=LegacyOption.Some(30);var x:LegacyOption<i64>=LegacyOption.None;var i=0;while i<3{x=values[i];i+=1}return score(values[0])+score(values[1])+score(x)}",
        42,
    );
}
#[test]
fn generic_enum_generic_match_move_states_and_shadowing() {
    native(
        "enum LegacyOption<T>{None,Some(T)} fn keep<T>(v:LegacyOption<T>,x:T)->T{match v{LegacyOption.None()=>{},LegacyOption.Some(x)=>{x;}}return x} fn result()->i64{return keep(LegacyOption::<i64>.Some(40),42)}",
        42,
    );
    for text in [
        "enum LegacyOption<T>{None,Some(T)} fn bad<T>(v:LegacyOption<T>)->(T,T){match v{LegacyOption.Some(x)=>{return (x,x)},LegacyOption.None()=>{return bad(v)}}} fn result()->i32=>0",
        "enum LegacyOption<T>{None,Some(T)} fn bad<T>(v:LegacyOption<i32>,x:T)->T{match v{LegacyOption.None()=>{x;let x=1;},LegacyOption.Some(_)=>{}}return x} fn result()->i32=>0",
    ] {
        reject(text, "E-GENERIC-ARGUMENT");
    }
}
#[test]
fn generic_enum_uninstantiated_bad_definitions_and_layout_reject() {
    for text in [
        "enum Bad<T>{A,A} fn result()->i32=>0",
        "enum Bad<T>{A(Missing)} fn result()->i32=>0",
        "enum LegacyOption<T>{None,Some(T)} enum Bad<T>{A(LegacyOption<T,i32>)} fn result()->i32=>0",
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
    {
        let text = "enum Bad<T>{A(&T)} fn result()->i32=>0";
        let r = expand(text, &Limits::default()).unwrap_err();
        assert_eq!(r[0].category, ResultCategory::Unsupported, "{text}: {r:?}");
    }
    reject(
        "enum Bad<T>{A([T;true])} fn result()->i32=>0",
        "E-GENERIC-ARGUMENT",
    );
}
#[test]
fn generic_enum_missing_constraints_nominality_and_payload_mismatch_reject() {
    for text in [
        "enum LegacyOption<T>{None,Some(T)} fn result()->i32{let x=LegacyOption.None;return 0}",
        "enum Result<E,T>{Err(E),Ok(T)} fn result()->i32{let x=Result.Ok(1);return 0}",
        "enum LegacyOption<T>{None,Some(T)} fn result()->i32{let x:LegacyOption<i64>=LegacyOption.Some(true);return 0}",
        "enum LegacyOption<T>{None,Some(T)} fn result()->i32{let x=LegacyOption::<i32,i64>.Some(1);return 0}",
        "enum LegacyOption<T>{None,Some(T)} fn result()->i32{let x=LegacyOption::<i32>.Missing;return 0}",
        "enum LegacyOption<T>{None,Some(T)} enum Other<T>{None,Some(T)} fn result()->i32{let x:LegacyOption<i32>=Other.Some(1);return 0}",
    ] {
        reject(text, "E-GENERIC-ARGUMENT");
    }
}
#[test]
fn generic_enum_unused_generic_match_is_checked_before_erasure() {
    reject(
        "enum LegacyOption<T>{None,Some(T)} fn bad<T>(v:LegacyOption<T>)->i32{match v{LegacyOption.Some(_)=>{return 1}}} fn result()->i32=>0",
        "E-MATCH-NONEXHAUSTIVE",
    );
    reject(
        "enum LegacyOption<T>{None,Some(T)} fn bad<T>(v:LegacyOption<T>)->i32{match v{_=>{return 1},LegacyOption.Some(_)=>{return 2}}} fn result()->i32=>0",
        "E-MATCH-UNREACHABLE",
    );
    reject(
        "enum LegacyOption<T>{None,Some(T)} fn bad<T>(v:LegacyOption<T>)->i32{match v{LegacyOption.None()=>{return 1},LegacyOption.Some(x)=>{return x+1}}} fn result()->i32=>0",
        "E-UNSUPPORTED",
    );
    reject(
        "enum LegacyOption<T>{None,Some(T)} fn bad<T>(v:LegacyOption<T>)->i32{match v{LegacyOption.None()=>{return 1},LegacyOption.Some()=>{return 2}}} fn result()->i32=>0",
        "E-GENERIC-ARGUMENT",
    );
}
#[test]
fn generic_enum_expansion_budgets_cover_unused_layout_and_instances() {
    let text = "enum LegacyOption<T>{None,Some(T)} fn result()->i64{let x=LegacyOption::<i64>.Some(42);match x{LegacyOption.None()=>{return 0},LegacyOption.Some(n)=>{return n}}}";
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
fn generic_enum_recursive_factories_and_nested_zero_argument_calls() {
    native(
        "enum LegacyOption<T>{None,Some(T)} fn empty<T>()->LegacyOption<T>{return LegacyOption.None} fn relay<T>()->LegacyOption<T>{return empty()} fn some<T>(v:T,b:bool)->LegacyOption<T>{if b{return LegacyOption.Some(v)}return some(v,true)} fn result()->i64{let missing:LegacyOption<i64>=relay();let present=some::<i64>(42,false);match (missing,present){(LegacyOption.None(),LegacyOption.Some(n))=>{return n},(_,_)=>{return 0}}}",
        42,
    );
}
#[test]
fn generic_enum_symbolic_bool_matrix_and_integer_aliases() {
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
fn generic_enum_full_example() {
    let text = include_str!("../../../examples/generic-option-result/main.zil")
        .replace("fn answer()", "fn result()")
        .replace("answer() ==", "result() ==");
    let llvm = native(&text, 5000000042);
    assert!(!llvm.contains("malloc"));
    assert!(!llvm.contains(" undef"));
}
#[test]
fn generic_enum_record_field_context_and_generic_record_payload() {
    native(
        "enum LegacyOption<T>{None,Some(T)} struct Box<U>{value:U;} struct Holder{value:LegacyOption<i64>;} fn id<T>(x:T)->T=>x; fn result()->i64{let h=Holder{value:LegacyOption.None};let b=LegacyOption.Some(Box::<i64>{value:42});match (h.value,b){(LegacyOption.None(),LegacyOption.Some(v))=>{return id(v.value)},(_,_)=>{return 0}}}",
        42,
    );
}
#[test]
fn generic_enum_explicit_and_inferred_applications_share_one_instance() {
    let text = "enum LegacyOption<T>{None,Some(T)} fn id<T>(x:T)->T=>x; fn result()->i64{let x=LegacyOption::<i64>.Some(21);let a=id(x);let b=id::<LegacyOption<i64>>(x);match (a,b){(LegacyOption.Some(m),LegacyOption.Some(n))=>{return m+n},(_,_)=>{return 0}}}";
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
fn generic_enum_unused_payload_literal_ranges_and_signed_minimum() {
    for text in [
        "enum LegacyOption<T>{None,Some(T)} fn bad<T>(x:T)->LegacyOption<i32>{return LegacyOption.Some(5000000000)} fn result()->i32=>0",
        "enum LegacyOption<T>{None,Some(T)} fn bad<T>(x:T)->LegacyOption<i64>{return LegacyOption.Some(-9223372036854775809)} fn result()->i32=>0",
    ] {
        reject(text, "E-GENERIC-ARGUMENT");
    }
    native(
        "enum LegacyOption<T>{None,Some(T)} fn minimum<T>(x:T)->LegacyOption<i64>{return LegacyOption.Some(-9223372036854775808)} fn result()->i64{match minimum(()){LegacyOption.None()=>{return 0},LegacyOption.Some(n)=>{return n}}}",
        i64::MIN,
    );
}
#[test]
fn generic_enum_grouped_enum_types_remain_transparent() {
    for ty in ["(LegacyOption<i64>)", "((LegacyOption<i64>))"] {
        let text = format!(
            "enum LegacyOption<T>{{None,Some(T)}} fn result()->i64{{let x:{ty}=LegacyOption.Some(42);match x{{LegacyOption.None()=>{{return 0}},LegacyOption.Some(n)=>{{return n}}}}}}"
        );
        native(&text, 42);
    }
}
