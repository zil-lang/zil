//! New source -> independent Verify -> LLVM/native, plus hostile raw/wire mutations.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};
use zil_ir::{InstructionKind, Type};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "truncate137.zil", text)
}
fn check(text: &str) -> zil_driver::CompileReport {
    compile_source(&input(text), &CompileOptions::default())
}
fn pass(text: &str) {
    let r = check(text);
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert_eq!(r.verification_runs, 1);
}
fn reject(text: &str, code: &str) {
    let r = check(text);
    assert_ne!(r.status, ResultCategory::Success, "{text}");
    assert!(
        r.diagnostics.iter().any(|d| d.code == code),
        "{}",
        r.to_json()
    );
    assert!(!r.artifacts.contains_key("executable"));
}
const SIMPLE: &str = "fn main()->i32{let b=bytes_new();bytes_reserve(b,4);bytes_push(b,7u8);bytes_truncate(b,0);return i32(bytes_len(b))}";

#[test]
fn truncate_is_an_explicit_source_operation() {
    pass(SIMPLE);
}
#[test]
fn truncate_rejects_wrong_arity_and_operand_type() {
    reject(
        "fn main(){let b=bytes_new();bytes_truncate(b)}",
        "E-CALL-ARITY",
    );
    let r = check("fn main(){let b=bytes_new();bytes_truncate(b,true)}");
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
}
#[test]
fn truncate_conflicts_with_live_views_even_when_it_would_fail_or_do_nothing() {
    for length in ["-1", "0", "1", "9223372036854775807"] {
        for borrow in ["bytes_view", "bytes_view_mut"] {
            reject(
                &format!(
                    "fn main()->i32{{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,7u8);let view={borrow}(b);bytes_truncate(b,{length});return i32(view[0])}}"
                ),
                "E-BORROW-CONFLICT",
            );
        }
    }
}
#[test]
fn truncate_after_last_view_use_and_disjoint_package_field_are_allowed() {
    pass(
        "fn main()->i32{let b=bytes_new();let view=bytes_view(b);let n=view.len();bytes_truncate(b,0);return i32(n)}",
    );
    pass(
        "package Pair{manages a:ByteBuffer;manages b:ByteBuffer} fn main()->i32{let p=Pair{a:bytes_new(),b:bytes_new()};let view=bytes_view(p.a);bytes_truncate(p.b,0);return i32(view.len())}",
    );
}
#[test]
fn helper_write_effect_reaches_captured_receiver() {
    pass(
        "fn shrink(b:ByteBuffer)->bool{return bytes_truncate(b,0)} fn main()->i32{let b=bytes_new();var cb=move()=>{shrink(b);i32(bytes_len(b))};return cb()+cb()}",
    );
    reject(
        "fn shrink(b:ByteBuffer){bytes_truncate(b,0)} fn main()->i32{let b=bytes_new();let v=bytes_view(b);shrink(b);return i32(v.len())}",
        "E-BORROW-CONFLICT",
    );
}
#[test]
fn generic_builtin_signature_is_available_before_instantiation() {
    pass(
        "fn shrink<T>(b:ByteBuffer,x:T)->T{bytes_truncate(b,0);return x} fn main()->i32{let b=bytes_new();return shrink(b,7)}",
    );
}
fn typed() -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let s = input(SIMPLE);
    let parsed = zil_syntax::parse(&s).into_result().unwrap();
    let core = zil_expand::expand(&s, &parsed).unwrap();
    let module = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    (module, plan)
}
#[test]
fn truncate_roundtrip_and_raw_operand_corruption_are_independently_checked() {
    let (m, p) = typed();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &limits).unwrap();
    let v = zil_verify::verify_module_bytes(&bytes, &m.target, &limits).unwrap();
    assert_eq!(v.module(), &m);
    let original = m.fingerprint();
    for wrong in [0, 1] {
        let mut bad = m.clone();
        let f = &mut bad.functions[0];
        let bool_value = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| i.result.filter(|v| f.values[v.0 as usize] == Type::Bool))
            .unwrap();
        let i = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::ByteBufferTruncate { .. }))
            .unwrap();
        if let InstructionKind::ByteBufferTruncate { buffer, length } = &mut i.kind {
            if wrong == 0 {
                *length = bool_value
            } else {
                *buffer = bool_value
            }
        }
        assert_ne!(bad.fingerprint(), original);
        let mut plan = p.clone();
        plan.module_fingerprint = bad.fingerprint();
        let errs =
            zil_verify::verify_module(bad.clone(), plan.clone(), &bad.target, &limits).unwrap_err();
        assert_eq!(errs[0].code, "E-VERIFY-BYTE-BUFFER", "{errs:?}");
        let encoded = zil_ir::wire::encode_module_candidate(&bad, &plan, &limits).unwrap();
        assert_eq!(
            zil_verify::verify_module_bytes(&encoded, &bad.target, &limits).unwrap_err()[0].code,
            "E-VERIFY-BYTE-BUFFER"
        );
    }
    let mut old = bytes.clone();
    old[8..12].copy_from_slice(&17u32.to_le_bytes());
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&old, &limits).unwrap_err()[0].code,
        "E-WIRE-VERSION"
    );
    for n in 0..bytes.len() {
        assert!(zil_ir::wire::decode_module_candidate(&bytes[..n], &limits).is_err());
    }
}

const NATIVE: &str = r#"
fn main()->i32 {
    let b=bytes_new()
    if !bytes_reserve(b,64) {return 1}
    for size in 0..32 {
        for wanted in -2..35 {
            if !bytes_truncate(b,0) {return 2}
            for i in 0..size {if !bytes_push(b,u8(i)){return 3}}
            let before=bytes_capacity(b)
            let ok=bytes_truncate(b,i64(wanted))
            let valid=wanted>=0 && wanted<=size
            if ok!=valid {return 4}
            var expected=i64(size)
            if valid {expected=i64(wanted)}
            if bytes_len(b)!=expected || bytes_capacity(b)!=before {return 5}
            let retained=bytes_view(b)
            for i in 0..expected {if retained[i]!=u8(i){return 6}}
            if !bytes_push(b,255u8){return 7}
            let fresh=bytes_view(b)
            if fresh[expected]!=255u8 || fresh.len()!=expected+1 {return 8}
        }
    }
    if bytes_truncate(b,-9223372036854775808) {return 9}
    if bytes_truncate(b,9223372036854775807) {return 10}
    return 0
}
"#;
#[test]
fn truncate_native_all_short_prefixes_failure_and_reinitialization_o0_o2() {
    for opt in [Optimization::O0, Optimization::O2] {
        let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!(
            "truncate137-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("input.zil"), NATIVE).unwrap();
        let exe = dir.join("program");
        let r = compile_source(
            &input(NATIVE),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(exe.clone()),
                emit_dir: Some(dir.join("phases")),
                optimization: opt,
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), r.to_json()).unwrap();
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        let output = Command::new(exe).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}

#[test]
fn compile_time_cannot_execute_truncate_or_construct_a_buffer() {
    let r = check(
        "fn illegal()->i64{let b=bytes_new();bytes_truncate(b,0);return bytes_len(b)} fn main()->i64{let n=#(illegal());return n}",
    );
    assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert_eq!(r.phase, zil_base::Phase::Ct, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
