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

fn large_copy_program(body: &str) -> String {
    let mut ty = "[u8;1]".to_owned();
    let mut initializers = "let a0=[3u8];".to_owned();
    for level in 1..=12 {
        ty = format!("[{ty};2]");
        initializers.push_str(&format!("let a{level}=[a{},a{}];", level - 1, level - 1));
    }
    let first = "[0]".repeat(13);
    format!(
        "type Big={ty}; fn touch(value:&mut Big)->i32{{(*value){first}+=1u8;return 1}} fn inspect(value:Big,marker:i32)->i32{{return i32(value{first})+marker}} fn result()->i32{{{initializers}{body}}}"
    )
}

#[test]
fn large_copy_snapshot_order_nested_index_and_loop_join() {
    let first = "[0]".repeat(13);
    let rest = "[0]".repeat(12);
    let text = large_copy_program(&format!(
        "var original=a12;let saved=original;var pair=(original,touch(&mut original));var index=1;pair.0[index]{rest}=4u8;let before=pair.0;pair.0[0]{rest}=8u8;let observed=inspect(original,touch(&mut original));var branch=original;var i=0;while i<2{{if i==0{{branch=saved}}else{{branch=before}}i+=1}}return i32(saved{first})*10000+i32(before{first})*1000+i32(pair.0{first})*100+i32(branch[index]{rest})*10+observed+i32(original{first});"
    ));
    let llvm = native(&text, Some(33850));
    assert!(llvm.contains("copy.value."));
    assert!(llvm.contains("llvm.memcpy.p0.p0.i64"));
    assert!(!llvm.contains("@malloc"));
}

#[test]
fn large_copy_read_address_snapshot_and_bounds_trap() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var original=a12;let p=&mut original;let before=*p;(*p){first}=7u8;let after=*p;return i32(before{first})*10+i32(after{first});"
    ));
    native(&text, Some(37));
    let text = large_copy_program(&format!(
        "var index=2;return i32(a12[index]{});",
        "[0]".repeat(12)
    ));
    native(&text, None);
}

#[test]
fn large_copy_record_unit_padding_and_tail_snapshot() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var record=Envelope{{unit:(),flag:true,body:a12,tail:9}};let before=record;record.body{first}=7u8;record.tail=11;record.flag=false;if before.flag{{return i32(before.body{first})*1000+i32(record.body{first})*100+i32(before.tail)*10+i32(record.tail)}}return 0;"
    ));
    let text = format!("struct Envelope{{unit:();flag:bool;body:Big;tail:i64}}{text}");
    native(&text, Some(3801));
}

#[test]
fn large_copy_current_codegen_budget_is_exact_and_fresh() {
    let src = source(include_str!(
        "../../../tests/language-readiness/codegen-pending/large-array-a18.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let failed = emit(low - 1).unwrap_err();
    assert_eq!(failed[0].category, ResultCategory::ResourceLimit);
    assert_eq!(failed[0].phase, Phase::Codegen);
    assert!(emit(low).is_ok());
    assert!(emit(low).is_ok());
    eprintln!("324 original a18 Codegen exact work limit: {low}");
}

#[test]
fn large_copy_a18_native_all_backends() {
    // Isolate LLVM failures and cap each compiler process. This remains one
    // existing Cargo integration target and preserves the original input bytes.
    if let Ok(profile) = std::env::var("ZIL_324_LARGE_COPY_CHILD") {
        use zil_driver::{NativeBackend, Optimization};
        let (backend, optimization) = match profile.as_str() {
            "clang-O0" => (NativeBackend::Clang, Optimization::O0),
            "clang-O2" => (NativeBackend::Clang, Optimization::O2),
            "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
            "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
            _ => panic!("unknown child profile"),
        };
        let out = Temp::new();
        let report = compile_source(
            &source(include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18.zil"
            )),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                cancellation: zil_driver::CancellationToken::with_timeout(
                    std::time::Duration::from_secs(45),
                ),
                output: Some(out.0.join("program")),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            let pipeline = format!("\"pipeline\":\"default<{optimization:?}>\"");
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| r.contains(&pipeline)));
        } else {
            let flag = if optimization == Optimization::O0 {
                "-O0"
            } else {
                "-O2"
            };
            assert!(
                report
                    .commands
                    .iter()
                    .any(|command| command.iter().any(|arg| arg == flag))
            );
        }
        let actual = Command::new("timeout")
            .args(["-s", "KILL", "5s"])
            .arg(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0), "{actual:?}");
        assert!(actual.stdout.is_empty());
        assert!(actual.stderr.is_empty());
        return;
    }
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            let actual = Command::new("timeout")
                .args([
                    "-s",
                    "KILL",
                    "55s",
                    "bash",
                    "-c",
                    "ulimit -c 0; ulimit -t 45; ulimit -v 2097152; exec \"$@\"",
                    "--",
                ])
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "large_copy_a18_native_all_backends",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(
                    "ZIL_324_LARGE_COPY_CHILD",
                    format!("{backend}-{optimization}"),
                )
                .output()
                .unwrap();
            assert!(
                actual.status.success(),
                "{backend}-{optimization}: {} {}",
                String::from_utf8_lossy(&actual.stdout),
                String::from_utf8_lossy(&actual.stderr)
            );
            eprintln!(
                "324 a18 {backend}-{optimization}: backend matched, requested pipeline, fallback=null, native_exit=0"
            );
        }
    }
}
#[test]
fn large_copy_a18_by_value_abi_native_all_backends() {
    // Isolate LLVM failures and cap each compiler process. This remains one
    // existing Cargo integration target and preserves the original input bytes.
    if let Ok(profile) = std::env::var("ZIL_333_LARGE_COPY_CHILD") {
        use zil_driver::{NativeBackend, Optimization};
        let (backend, optimization) = match profile.as_str() {
            "clang-O0" => (NativeBackend::Clang, Optimization::O0),
            "clang-O2" => (NativeBackend::Clang, Optimization::O2),
            "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
            "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
            _ => panic!("unknown child profile"),
        };
        let out = Temp::new();
        let case = std::env::var("ZIL_333_CASE").unwrap();
        let input = match case.as_str() {
            "argument" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-by-value.zil"
            ),
            "return" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-return.zil"
            ),
            "pointer" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-function-pointer.zil"
            ),
            _ => panic!("unknown ABI case"),
        };
        let report = compile_source(
            &source(input),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                cancellation: zil_driver::CancellationToken::with_timeout(
                    std::time::Duration::from_secs(45),
                ),
                output: Some(out.0.join("program")),
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
        assert!(llvm.contains(".lc1("), "versioned definition/call absent");
        assert!(llvm.contains("@zil.copy.bytes"));
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            let pipeline = format!("\"pipeline\":\"default<{optimization:?}>\"");
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| r.contains(&pipeline)));
        } else {
            let flag = if optimization == Optimization::O0 {
                "-O0"
            } else {
                "-O2"
            };
            assert!(
                report
                    .commands
                    .iter()
                    .any(|command| command.iter().any(|arg| arg == flag))
            );
        }
        let actual = Command::new("timeout")
            .args(["-s", "KILL", "5s"])
            .arg(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0), "{actual:?}");
        assert!(actual.stdout.is_empty());
        assert!(actual.stderr.is_empty());
        return;
    }
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            for case in ["argument", "return", "pointer"] {
                let actual = Command::new("timeout")
                    .args([
                        "-s",
                        "KILL",
                        "55s",
                        "bash",
                        "-c",
                        "ulimit -c 0; ulimit -t 45; ulimit -v 2097152; exec \"$@\"",
                        "--",
                    ])
                    .arg(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "large_copy_a18_by_value_abi_native_all_backends",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("ZIL_333_CASE", case)
                    .env(
                        "ZIL_333_LARGE_COPY_CHILD",
                        format!("{backend}-{optimization}"),
                    )
                    .output()
                    .unwrap();
                assert!(
                    actual.status.success(),
                    "{backend}-{optimization}: {} {}",
                    String::from_utf8_lossy(&actual.stdout),
                    String::from_utf8_lossy(&actual.stderr)
                );
                eprintln!(
                    "333 a18 ABI {case} {backend}-{optimization}: backend matched, requested pipeline, fallback=null, native_exit=0"
                );
            }
        }
    }
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
        (
            "fn result()->i32{if true{let(a,b)=(1,2)}return a}",
            "E-UNKNOWN-NAME",
        ),
        (
            "fn result()->i32{let f=()=>{let(a,b)=(missing,1);return a+b};return f()}",
            "E-UNKNOWN-NAME",
        ),
        (
            "fn result()->i32{var(a,b)=(1,2);defer{let(x,y)=(a,b);return x+y}return 0}",
            "E-DEFER-CONTROL",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
        eprintln!("review318/walker rejection: {}", report.to_json());
    }
}
#[test]
fn review318_original_generic_gap_and_mixed_copy_record_obligations() {
    native(
        include_str!("../../../tests/language-readiness/recovery/review318-generic-tuple-gap.zil"),
        Some(42),
    );
    let valid = "struct Pair<T:Copy,U>{first:T;second:U}fn hidden<T:Copy,U:Copy>(p:Pair<T,U>)->Pair<T,U>{let(x,_)=(p,());return x}fn result()->i32{return 0}";
    let report = compile_source(&source(valid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for text in [
        "struct T{n:i32}fn hidden<T>(value:T){let(_,_)=(value,());}fn result()->i32{return 0}",
        "struct Pair<T:Copy,U>{first:T;second:U}fn hidden<T:Copy,U>(p:Pair<T,U>){let(_,_)=(p,());}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        eprintln!("review318/symbolic obligation: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-GENERIC-ARGUMENT")
        );
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

#[test]
fn large_copy_abi_copy_in_and_rhs_order() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var original=a12;let pointer:fn(Big)->Big=keep;let returned=pointer(original);let observed=pair(original,touch(&mut original));return i32(returned{first})*100+i32(original{first})*10+observed;"
    ));
    let text = format!(
        "fn keep(value:Big)->Big{{var changed=value;changed{first}=9u8;return value}}fn pair(value:Big,marker:i32)->i32{{var changed=value;changed{first}=8u8;return i32(value{first})+marker}}{text}"
    );
    let llvm = native(&text, Some(344));
    assert!(llvm.contains(".lc1(ptr %lc1.result, ptr %arg.0)"));
    assert!(!llvm.contains("define internal void @zil.fn."));
}

#[test]
fn large_copy_abi_special_closure_is_explicitly_unsupported() {
    // Match the ordinary CLI's 8 MiB main-thread stack. The recursive closure
    // metadata check exceeds Rust's smaller default test-worker stack.
    std::thread::Builder::new()
        .stack_size(
            std::env::var("ZIL_333_CAPTURE_STACK_BYTES")
                .ok()
                .map(|n| n.parse().unwrap())
                .unwrap_or(8 * 1024 * 1024),
        )
        .spawn(|| {
            let first = "[0]".repeat(13);
            for (prefix, body) in [
                ("", "let f=(value:Big)->Big=>value;return 0;".to_owned()),
                ("", format!("let f=move()=>i32(a12{first});return f();")),
                (
                    "",
                    "let f=(pointer:fn(Big,i32)->i32)->i32=>0;return 0;".to_owned(),
                ),
                (
                    "",
                    "let pointer=inspect;let f=()=>{let held=pointer;return 0};return f();"
                        .to_owned(),
                ),
                (
                    "resource Holder{call:fn(Big,i32)->i32}",
                    "let holder=Holder{call:inspect};return 0;".to_owned(),
                ),
            ] {
                let text = format!("{prefix}{}", large_copy_program(&body));
                eprintln!(
                    "333 capture probe: Verify API begins; move_capture={}",
                    body.contains("move")
                );
                let verified = compile_source(&source(&text), &CompileOptions::default());
                assert_eq!(
                    verified.status,
                    ResultCategory::Success,
                    "{}",
                    verified.to_json()
                );
                eprintln!("333 capture probe: Verify API completed; Build API begins");
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        entry: Some("result".into()),
                        stop_after: Stage::Build,
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Unsupported,
                    "{}",
                    report.to_json()
                );
                assert!(report.diagnostics.iter().any(|d| d.phase == Phase::Codegen));
                assert!(!report.artifacts.contains_key("llvm"));
                assert!(!report.artifacts.contains_key("executable"));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn large_copy_abi_codegen_budget_exact_and_fresh() {
    let src = source(include_str!(
        "../../../tests/language-readiness/codegen-pending/large-array-a18-function-pointer.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    let errors = emit(low - 1).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].phase, Phase::Codegen);
    assert!(emit(low).unwrap().contains(".lc1"));
    assert!(emit(low).unwrap().contains(".lc1"));
    if cfg!(feature = "llvm-api") {
        let out = Temp::new();
        let options = CompileOptions {
            backend: zil_driver::NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            object_cache: Some(out.0.join("objects")),
            output: Some(out.0.join("program")),
            ..CompileOptions::default()
        };
        let cold = compile_source(&src, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        assert!(cold.native_codegen.as_ref().unwrap().object_cache.misses > 0);
        let failed = compile_source(
            &src,
            &CompileOptions {
                limits: Limits {
                    max_analysis_steps: low - 1,
                    ..defaults.clone()
                },
                ..options.clone()
            },
        );
        assert_eq!(
            failed.status,
            ResultCategory::ResourceLimit,
            "{}",
            failed.to_json()
        );
        assert_eq!(failed.phase, Phase::Codegen);
        assert!(!failed.artifacts.contains_key("executable"));
        let warm = compile_source(&src, &options);
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.misses, 0);
        assert!(warm.native_codegen.as_ref().unwrap().object_cache.hits > 0);
    }
    eprintln!("333 a18 function-pointer Codegen exact work limit: {low}");
}

#[test]
fn large_copy_abi_cgu_cache_and_canonical_function_reference() {
    use zil_driver::{
        CodegenOptions, NativeBackend, Optimization, ProjectSources, compile_project,
    };
    let mut ty = "[u8;1]".to_owned();
    let mut initialization = "let a0=[3u8];".to_owned();
    for n in 1..=12 {
        ty = format!("[{ty};2]");
        initialization.push_str(&format!("let a{n}=[a{},a{}];", n - 1, n - 1));
    }
    let first = "[0]".repeat(13);
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            for (round, changed) in [(0, false), (1, false), (2, true)] {
                let body = if changed {
                    format!("var changed=value;changed{first}=9u8;return changed")
                } else {
                    "return value".into()
                };
                let modules=[
              ("main",format!("import model;import factory;fn main()->i32{{{initialization}let p=factory.pointer();let value=p(a12);return i32(value{first})}}")),
              ("model",format!("pub type Big={ty};pub fn identity(value:Big)->Big{{{body}}}")),
              ("factory","import model;pub fn pointer()->fn(model.Big)->model.Big{return model.identity}".into()),
            ];
                let project = ProjectSources {
                    root: ModuleId(0),
                    modules: modules
                        .iter()
                        .enumerate()
                        .map(|(n, (name, text))| {
                            SourceUnit::new(
                                FileId(n as u32),
                                ModuleId(n as u32),
                                format!("{name}.zil"),
                                text.clone(),
                            )
                        })
                        .collect(),
                    paths: [
                        (vec!["model".into()], ModuleId(1)),
                        (vec!["factory".into()], ModuleId(2)),
                    ]
                    .into_iter()
                    .collect(),
                };
                let report = compile_project(
                    &project,
                    &CompileOptions {
                        backend,
                        optimization,
                        incremental_o2: optimization == Optimization::O2,
                        stop_after: Stage::Build,
                        object_cache: (backend == NativeBackend::LlvmApi)
                            .then(|| out.0.join("objects")),
                        codegen: CodegenOptions { units: 3, jobs: 1 },
                        output: Some(out.0.join(format!("program{round}"))),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let actual = Command::new("timeout")
                    .args(["-s", "KILL", "5s"])
                    .arg(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(if changed { 9 } else { 3 }),
                    "{actual:?}"
                );
                let codegen = report.native_codegen.as_ref().unwrap();
                assert!(codegen.actual_units >= 2, "{}", report.to_json());
                assert!(codegen.fallback.is_none(), "{}", report.to_json());
                if backend == NativeBackend::LlvmApi {
                    assert!(codegen.object_cache.enabled, "{}", report.to_json());
                    if round == 1 {
                        assert_eq!(codegen.object_cache.hits, codegen.actual_units);
                        assert_eq!(codegen.object_cache.misses, 0);
                    } else {
                        assert!(codegen.object_cache.misses > 0);
                    }
                }
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains(".lc1"));
            }
        }
    }
}

#[test]
fn large_copy_abi_record_tuple_unit_padding_roundtrip_all_backends() {
    use zil_driver::{NativeBackend, Optimization};
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "let pair=(a12,());var record=Envelope{{unit:(),flag:true,body:a12,tail:9}};let pointer:fn((Big,()),Envelope)->(Envelope,Big)=relay;let kept=pointer(pair,record);record.body{first}=7u8;return i32(kept.0.body{first})*1000+i32(kept.1{first})*100+i32(record.body{first})*10+i32(kept.0.tail);"
    ));
    let text = format!(
        "struct Envelope{{unit:();flag:bool;body:Big;tail:i64}}fn relay(pair:(Big,()),record:Envelope)->(Envelope,Big){{var changed=record;changed.body{first}=9u8;changed.tail=11;return (changed,pair.0)}}{text}"
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            let report = compile_source(
                &source(&text),
                &CompileOptions {
                    backend,
                    optimization,
                    entry: Some("result".into()),
                    stop_after: Stage::Build,
                    output: Some(out.0.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let observed = Command::new("timeout")
                .args(["-s", "KILL", "5s"])
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(observed.status.success(), "{observed:?}");
            assert_eq!(observed.stdout, b"9381\n");
            let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
            assert!(llvm.contains(".lc1(ptr %lc1.result, ptr %arg.0, ptr %arg.1)"));
        }
    }
}

#[test]
fn large_copy_abi_unit_erasure_small_signature_and_void_return() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(
        "var observed=0;consume((),a12,2,&mut observed);let scalar=small([4u8]);return observed+i32(scalar[0]);",
    );
    let text = format!(
        "fn consume(unit:(),value:Big,marker:i32,output:&mut i32)->(){{*output=i32(value{first})+marker;return ()}}fn small(value:[u8;1])->[u8;1]{{return value}}{text}"
    );
    let llvm = native(&text, Some(9));
    assert!(llvm.contains(".lc1(ptr %arg.1, i32 %arg.2, ptr %arg.3)"));
    let small = llvm
        .lines()
        .find(|line| line.starts_with("define [1 x i8] @zil.fn."))
        .unwrap();
    assert!(!small.contains(".lc1"));
}

#[test]
fn large_copy_abi_borrow_capture_keeps_pointer_environment() {
    // The captured source variable is Big, but its mutable borrowed environment
    // field is one pointer. LargeCopy guards must inspect the physical value.
    std::thread::Builder::new().stack_size(8*1024*1024).spawn(|| {
        use zil_driver::{NativeBackend,Optimization};
        let first="[0]".repeat(13);
        let text=large_copy_program(&format!("var original=a12;var f=()=>{{original{first}+=1u8;return i32(original{first})}};return f()*10+f();"));
        for backend in [NativeBackend::Clang,NativeBackend::LlvmApi] {
            if backend==NativeBackend::LlvmApi && !cfg!(feature="llvm-api") {continue;}
            for optimization in [Optimization::O0,Optimization::O2] {
                let out=Temp::new();
                let report=compile_source(&source(&text),&CompileOptions {backend,optimization,entry:Some("result".into()),stop_after:Stage::Build,output:Some(out.0.join("program")),..CompileOptions::default()});
                assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
                let actual=Command::new("timeout").args(["-s","KILL","5s"]).arg(&report.artifacts["executable"]).output().unwrap();
                assert!(actual.status.success(),"{actual:?}");
                assert_eq!(actual.stdout,b"45\n");
            }
        }
    }).unwrap().join().unwrap();
}

#[test]
fn large_copy_abi_recursive_carrier_symbols_and_old_link_rejection() {
    use zil_driver::{NativeBackend, Optimization};
    let input = include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-version-carriers.zil"
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            let report = compile_source(
                &source(input),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(out.0.join("program")),
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
            for id in 0..=12 {
                let base = format!("@zil.fn.{id}");
                let expected = if id <= 8 {
                    format!("{base}.lc1(")
                } else {
                    format!("{base}(")
                };
                assert!(
                    llvm.lines()
                        .any(|line| line.starts_with("define ") && line.contains(&expected)),
                    "missing {expected}"
                );
            }
            assert!(llvm.contains("ptr @zil.fn.1.lc1()"));
            assert!(llvm.contains("i32 @zil.fn.2.lc1(ptr %arg.0)"));
            let actual = Command::new("timeout")
                .args(["-s", "KILL", "5s"])
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(actual.status.code(), Some(0), "{actual:?}");
            let symbols = Command::new("nm")
                .arg("-g")
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(symbols.status.success());
            let symbols = String::from_utf8(symbols.stdout).unwrap();
            for id in 0..=8 {
                assert!(
                    symbols
                        .lines()
                        .any(|line| line.ends_with(&format!(" T zil.fn.{id}.lc1")))
                );
                assert!(
                    !symbols
                        .lines()
                        .any(|line| line.ends_with(&format!(" T zil.fn.{id}")))
                );
            }
            if backend == NativeBackend::Clang && optimization == Optimization::O0 {
                let object = out.0.join("definitions.o");
                let built = Command::new("clang")
                    .args(["-x", "ir", "-O0", "-c"])
                    .arg(&report.artifacts["llvm"])
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert!(built.status.success(), "{built:?}");
                for (suffix, success) in [("", false), (".lc1", true)] {
                    let probe = out.0.join("consumer.c");
                    fs::write(&probe,format!("extern void *factory(void) __asm__(\"zil.fn.1{suffix}\");void *consume_factory(void){{return factory();}}\n")).unwrap();
                    let linked = Command::new("clang")
                        .arg(&probe)
                        .arg(&object)
                        .arg("-o")
                        .arg(out.0.join("consumer"))
                        .output()
                        .unwrap();
                    assert_eq!(linked.status.success(), success, "{linked:?}");
                    if !success {
                        let error = String::from_utf8_lossy(&linked.stderr);
                        assert!(
                            error.contains("undefined reference") && error.contains("zil.fn.1"),
                            "{error}"
                        );
                    }
                }
            }
        }
    }
}
