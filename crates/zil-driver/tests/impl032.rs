//! Real Copy aggregate programs, LLVM bounds checks, and forged raw controls.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::InstructionKind;
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
fn shared_projection_and_receiver_end_to_end() {
    let llvm = native(
        include_str!("../../../examples/impl032/main.zil"),
        Some(8075),
    );
    assert!(llvm.contains("address.ok."));
    assert!(!llvm.contains("@malloc"));
}
#[test]
fn tuple_unit_field_and_zero_length_bounds() {
    native(
        "fn get(x:&((),bool,i64))->&i64=>&x.2; fn result()->i64 {let t:((),bool,i64)=((),true,7000000000);let u=&t.0;let z=*u;return *get(&t)}",
        Some(7000000000),
    );
    for text in [
        "fn f(a:&[i32;2],i:i64)->&i32=>&a[i]; fn result()->i32 {let a=[2,3];return *f(&a,-1)}",
        "fn result()->i32 {let a:[i32;0]=[];return *(&a[0])}",
    ] {
        native(text, None);
    }
}
#[test]
fn projected_borrows_keep_whole_object_origins() {
    for text in [
        "struct P{x:i64;y:i64;} fn result()->i64 {var p=P{x:1,y:2};let x=&p.x;p.y=3;return *x}",
        "struct P{x:i64;} fn get(p:&P)->&i64=>&p.x; fn bad()->&i64 {let p=P{x:4};return get(&p)} fn result()->i64=>0",
        "fn bad()->&i64 {let a:[i64;1]=[7];return &a[0]} fn result()->i64=>0",
    ] {
        let r = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
        assert_eq!(r.phase, Phase::Verify, "{}", r.to_json());
    }
}
#[test]
fn raw_projection_type_and_hold_claims_are_independently_checked() {
    let src = source(
        "struct P{x:i64;y:bool;} fn get(p:&P)->&i64=>&p.x; fn result()->i64 {let p=P{x:9,y:true};return *get(&p)}",
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &module
    );
    for bad in [1, 99] {
        let mut raw = module.clone();
        let projection = raw.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::AddressField { .. }))
            .unwrap();
        if let InstructionKind::AddressField { index, .. } = &mut projection.kind {
            *index = bad
        };
        let mut claims = plan.clone();
        claims.module_fingerprint = raw.fingerprint();
        let r = zil_verify::verify_module(raw, claims, &target, &limits).unwrap_err();
        assert_eq!(r[0].phase, Phase::Verify);
    }
    let mut claims = plan.clone();
    for f in &mut claims.functions {
        for h in &mut f.holds {
            h.roots.clear()
        }
    }
    let r = zil_verify::verify_module(module, claims, &target, &limits).unwrap_err();
    assert_eq!(r[0].code, "E-VERIFY-HOLD");
}
