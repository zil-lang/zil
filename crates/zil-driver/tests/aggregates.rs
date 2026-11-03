//! Real Copy aggregate programs, LLVM bounds checks, and forged raw controls.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{InstructionKind, Type};
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
    native_entry(text, "result", expected)
}
fn native_entry(text: &str, entry: &str, expected: Option<i64>) -> String {
    let out = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some(entry.into()),
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
fn reject(text: &str, code: &str) {
    let r = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
#[test]
fn records_nested_update_parameter_and_return() {
    let llvm = native(
        include_str!("../../../examples/aggregates/main.zil"),
        Some(6483),
    );
    assert!(llvm.contains("insertvalue"));
    assert!(llvm.contains("extractvalue"));
    assert!(llvm.contains("{ { i64, i64 }, { i64, i64 }, i64 }"));
}
#[test]
fn arrays_nested_fields_loop_and_copy_return() {
    let llvm = native(
        include_str!("../../../examples/aggregates/arrays.zil"),
        Some(7498),
    );
    assert!(llvm.contains("[3 x { i64, i64 }]"));
    assert!(llvm.contains("icmp ult i64"));
    assert!(llvm.contains("label %zil.trap"));
    let entry = llvm.split("entry:").nth(1).unwrap();
    assert!(
        entry
            .split("bb.0:")
            .next()
            .unwrap()
            .contains("%index.slot.")
    );
}
#[test]
fn empty_unit_bool_tuples_and_nested_arrays() {
    native(
        "struct Empty {} struct Bits { unit: (); flag: bool; } fn id(x: Empty) -> Empty { return x; } fn result() -> i32 { let e = id(Empty {}); let z: [i64; 0] = []; var t = (Bits { unit: (), flag: true }, [3, 7]); t.1[0] = 5; t.0.unit = (); if t.0.flag { return t.1[0] * 10 + t.1[1] }; return 0; }",
        Some(57),
    );
}
#[test]
fn tuple_binding_original_readiness_source() {
    native_entry(
        include_str!("../../../tests/language-readiness/pending/runtime-tuple-binding.zil"),
        "main",
        Some(0),
    );
}
#[test]
fn review318_tuple_rhs_order_and_ct_traversal() {
    native(
        "fn step(c:&mut i32)->i32{*c+=1;return *c}fn result()->i32{var count=0;let(a,(b,z))=(step(&mut count),(step(&mut count),#(2+3)));return count*1000+a*100+b*10+z}",
        Some(2125),
    );
}
#[test]
fn review318_tuple_defer_capture_and_scoped_initialization() {
    native(
        "fn result()->i32{var(a,b)=(1,2);if true{defer{let(x,y)=(b,1);a+=x+y}}return a*10+b}",
        Some(42),
    );
    native(
        "fn result()->i32{var outer=5;var f=()=>{let(a,b)=(outer,1);outer=a+b;return outer};let first=f();return first*10+f()}",
        Some(67),
    );
    for (text, code) in [
        ("fn result()->i32{if true{let(a,b)=(1,2)}return a}", "E-UNKNOWN-NAME"),
        ("fn result()->i32{let f=()=>{let(a,b)=(missing,1);return a+b};return f()}", "E-UNKNOWN-NAME"),
        ("fn result()->i32{var(a,b)=(1,2);defer{let(x,y)=(a,b);return x+y}return 0}", "E-DEFER-CONTROL"),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
        assert!(report.diagnostics.iter().any(|d| d.code == code), "{}", report.to_json());
        assert!(!report.artifacts.contains_key("executable"));
        eprintln!("review318/walker rejection: {}", report.to_json());
    }
}
#[test]
fn review318_original_generic_gap_and_mixed_copy_record_obligations() {
    native(include_str!("../../../tests/language-readiness/recovery/review318-generic-tuple-gap.zil"), Some(42));
    let valid = "struct Pair<T:Copy,U>{first:T;second:U}fn hidden<T:Copy,U:Copy>(p:Pair<T,U>)->Pair<T,U>{let(x,_)=(p,());return x}fn result()->i32{return 0}";
    let report = compile_source(&source(valid), &CompileOptions::default());
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    assert_eq!(report.verification_runs, 1);
    for text in [
        "struct T{n:i32}fn hidden<T>(value:T){let(_,_)=(value,());}fn result()->i32{return 0}",
        "struct Pair<T:Copy,U>{first:T;second:U}fn hidden<T:Copy,U>(p:Pair<T,U>){let(_,_)=(p,());}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        eprintln!("review318/symbolic obligation: {}", report.to_json());
        assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(report.diagnostics.iter().any(|d| d.code == "E-GENERIC-ARGUMENT"));
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn tuple_binding_nested_mutable_typed_discard_and_single_evaluation() {
    let llvm = native(
        "fn pair(count: &mut i32) -> (i32,(i32,i32)) { *count += 1; return (*count,(*count+1,*count+2)); } fn result()->i32 { var count=0; let (a,(b,_)):(i32,(i32,i32))=pair(&mut count); var (c,(d,e))=(a,(b,7)); c+=10; d+=20; e+=30; let ()=(); let (_,)= (9,); return count*1000+a*100+b*10+c+d+e; }",
        Some(1190),
    );
    assert!(llvm.contains("extractvalue"));
    assert!(!llvm.contains("malloc"));
}
#[test]
fn tuple_binding_shadow_closure_and_copy_fields() {
    native(
        "struct P { x:i32; } fn result()->i32 { let x=5; if true { let (x,y)=(x+1,2); let f=()=>{ let (a,b)=(x,y); return a+b; }; var (p,a,flag,u)=(P{x:f()},[3,4],true,()); p.x+=1; a[1]=7; if flag { return p.x*100+a[0]*10+a[1]; } }; return 0; }",
        Some(937),
    );
    native(
        "fn result()->i32 { var (a,b)=(1,2); var f=()=>{ a+=b; return a; }; let first=f(); return first*10+f(); }",
        Some(35),
    );
    native(
        "fn result()->i64 { let (a,b):(i64,i64)=(3000000000,2); return a+b; }",
        Some(3000000002),
    );
}
#[test]
fn tuple_binding_shape_and_permission_neighbors() {
    for (text, code) in [
        (
            "fn result()->i32 { let (a,)=(1,2); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,b,c)=(1,2); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,(b,))=(1,(2,3)); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,b)=1; return a; }",
            "E-MATCH-TYPE",
        ),
        (
            "fn result()->i32 { let ()=(1,2); return 0; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,a)=(1,2); return a; }",
            "E-MATCH-BINDING",
        ),
        (
            "fn result()->i32 { let (a,b):(i32,bool)=(1,2); return a; }",
            "E-TYPE-MISMATCH",
        ),
        (
            "fn result()->i32 { let (a,b)=(1,2); a=3; return a; }",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "fn result()->i32 { let (self,b)=(1,2); return b; }",
            "E-RECEIVER-RESERVED-NAME",
        ),
        (
            "fn result()->i32 { var (a,b)=(1,2); let f=()=>{a+=b;return a;}; return f(); }",
            "E-CLOSURE-MUTABILITY",
        ),
    ] {
        reject(text, code);
    }
    for text in [
        "fn result()->i32 { let (ref a,b)=(1,2); return b; }",
        "fn result()->i32 { let (ref mut a,b)=(1,2); return b; }",
        "fn result()->i32 { let (1,b)=(1,2); return b; }",
        "fn result()->i32 { var a=1; let (_,_) = (&mut a,2); return a; }",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn tuple_binding_existing_ir_wire_and_independent_projection_check() {
    let src = source("fn result()->i32 { let (a,(b,_))=(1,(2,3)); return a+b; }");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    let mut damaged = typed.clone();
    let field = damaged.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|i| matches!(i.kind, InstructionKind::ExtractField { .. }))
        .unwrap();
    if let InstructionKind::ExtractField { index, .. } = &mut field.kind {
        *index = 99;
    }
    let mut claims = plan;
    claims.module_fingerprint = damaged.fingerprint();
    let error = zil_verify::verify_module(damaged, claims, &target, &limits).unwrap_err();
    assert_eq!(error[0].phase, Phase::Verify);
    assert!(error[0].code.starts_with("E-VERIFY-"));
}
#[test]
fn tuple_binding_pattern_work_has_exact_fresh_limit() {
    let src = source("fn result()->i32 { let (a,(b,_))=(1,(2,3)); return a+b; }");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let succeeds = |steps| {
        zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            },
        )
    };
    let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
    assert!(succeeds(hi).is_ok());
    while lo < hi {
        let middle = lo + (hi - lo) / 2;
        if succeeds(middle).is_ok() {
            hi = middle;
        } else {
            lo = middle + 1;
        }
    }
    let failed = succeeds(lo - 1).unwrap_err();
    assert_eq!(failed[0].category, ResultCategory::ResourceLimit);
    assert!(succeeds(lo).is_ok());
    assert!(succeeds(lo).is_ok());
    eprintln!("tuple binding Types exact analysis limit: {lo}");
}
#[test]
fn record_defaults_context_and_inferred_updates() {
    native(
        "struct Pair { x: i64 = 7; y: i64 = 9; } fn result() -> i64 { let p: Pair = .{ y: 20 }; let q = .{ ...p, x: 4 }; return p.x * 100 + q.x * 10 + q.y; }",
        Some(760),
    );
}
#[test]
fn static_and_value_receiver_methods() {
    native(
        "struct Item { price: i64; quantity: i64; } impl Item { fn make(price: i64, quantity: i64) -> Self => Item { price, quantity }; fn total(self: Self) -> i64 => .price * .quantity; } fn result() -> i64 { let item = Item.make(7, 3); return item.total() + Item.total(Item.make(12, 4)); }",
        Some(69),
    );
}
#[test]
fn malformed_record_fields_and_nominal_mismatch() {
    for (text, code) in [
        (
            "struct P { x: i32; } fn result() -> i32 { let p=P { x:1,x:2 }; return p.x; }",
            "E-FIELD-DUPLICATE",
        ),
        (
            "struct P { x: i32; y:i32; } fn result() -> i32 { let p=P { x:1 }; return p.x; }",
            "E-FIELD-MISSING",
        ),
        (
            "struct P { x: i32; } fn result() -> i32 { let p=P { x:1 }; return p.y; }",
            "E-UNKNOWN-FIELD",
        ),
        (
            "struct P { x: i32; } struct Q { x:i32; } fn f(p:P)->i32{return p.x;} fn result()->i32{return f(Q{x:1});}",
            "E-TYPE-MISMATCH",
        ),
        (
            "struct P { x:P; } fn result()->i32{return 0;}",
            "E-INFINITE-LAYOUT",
        ),
        (
            "struct P { x:i32; } fn result()->i32{let p=P{x:1};p.x=2;return p.x;}",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "fn result()->i32{let p:[i32;2]=[1];return p[0];}",
            "E-ARRAY-LENGTH",
        ),
        (
            "fn result()->i32{let p=[1,2];return p[true];}",
            "E-INDEX-TYPE",
        ),
        (
            "fn result()->i32{let p=(1,2);return p.2;}",
            "E-UNKNOWN-FIELD",
        ),
    ] {
        reject(text, code);
    }
}
#[test]
fn signed_and_upper_bounds_reads_and_writes_trap() {
    for source in [
        "fn get(i:i64)->i32 {let values=[3,4];return values[i];} fn result()->i32{return get(-1);}",
        "fn get(i:i32)->i32 {let values=[3,4];return values[i];} fn result()->i32{return get(2);}",
        "fn result()->i32{var values=[3,4];values[2]=9;return values[0];}",
        "fn result()->i32{let values:[i32;0]=[];return values[0];}",
    ] {
        native(source, None);
    }
}
#[test]
fn instance_method_value_is_not_silently_bound() {
    reject(
        "struct P{x:i32;} impl P {fn get(self:Self)->i32=>.x;} fn result()->i32{let p=P{x:1};let f=p.get;return 0;}",
        "E-RECEIVER-METHOD-VALUE",
    );
}
#[test]
fn wire_roundtrip_and_independent_field_forgery_rejection() {
    let src = source(
        "struct P { x:i64; y:i64; } fn result()->i64 { var p=P{x:1,y:2};p.x=3;return p.y; }",
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let checked = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    assert_eq!(checked.module(), &typed);
    for modification in 0..3 {
        let mut damaged = typed.clone();
        let f = &mut damaged.functions[0];
        if modification == 2 {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::MakeAggregate { .. }))
                .unwrap();
            if let InstructionKind::MakeAggregate { fields } = &mut i.kind {
                fields.pop();
            }
        } else {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::ExtractField { .. }))
                .unwrap();
            if modification == 0 {
                if let InstructionKind::ExtractField { index, .. } = &mut i.kind {
                    *index = 99;
                }
            } else {
                f.values[i.result.unwrap().0 as usize] = Type::Bool;
            }
        }
        let mut claims = plan.clone();
        claims.module_fingerprint = damaged.fingerprint();
        let errors = zil_verify::verify_module(damaged, claims, &target, &limits).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Verify);
        assert!(errors[0].code.starts_with("E-VERIFY-"));
    }
}

#[test]
fn raw_aggregate_shapes_obey_work_and_depth_limits_before_identity_checks() {
    let src = source("fn result()->i32{return 3;}");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
    let mut nested = Type::I32;
    for _ in 0..32 {
        nested = Type::Tuple(vec![nested]);
    }
    for (shape, limits) in [
        (
            nested,
            Limits {
                max_call_depth: 16,
                ..defaults.clone()
            },
        ),
        (
            Type::Array {
                element: Box::new(Type::I32),
                length: u32::MAX,
            },
            Limits {
                max_analysis_steps: 1024,
                ..defaults.clone()
            },
        ),
        (
            Type::Record {
                name: "x".repeat(2048),
                fields: vec![],
            },
            Limits {
                max_analysis_steps: 1024,
                ..defaults.clone()
            },
        ),
    ] {
        let mut damaged = typed.clone();
        damaged.functions[0].values[0] = shape;
        let analyzed = zil_analyze::analyze(&damaged, &limits).unwrap_err();
        assert_eq!(analyzed[0].category, ResultCategory::ResourceLimit);
        // Keep the old identity claim: shape limits must be reached before
        // computing the raw candidate fingerprint or trusting any certificate.
        let checked =
            zil_verify::verify_module(damaged, plan.clone(), &target, &limits).unwrap_err();
        assert_eq!(checked[0].category, ResultCategory::ResourceLimit);
        assert_eq!(checked[0].phase, Phase::Verify);
    }
}
