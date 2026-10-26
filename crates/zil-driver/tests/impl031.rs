//! Real Copy aggregate programs, LLVM bounds checks, and forged raw controls.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::Type;
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
fn aggregates_ct_recursive_borrow_and_last_use_compose() {
    let llvm = native(
        include_str!("../../../examples/impl031/main.zil"),
        Some(61008),
    );
    assert!(llvm.contains("define ptr"));
    assert!(!llvm.contains("@malloc"));
}
#[test]
fn tuple_and_array_borrow_signatures_preserve_copy_values() {
    native(
        "fn id(x:&[i64;2])->&[i64;2]=>x; fn sum(x:&[i64;2])->i64 {return (*x)[0]+(*x)[1]} fn from(t:&(bool,[i64;2]))->i64 {let x=*t;if x.0 {let a=x.1;return sum(&a)}return 0} fn result()->i64 {let x:[i64;2]=[12,34];let t=(true,x);return sum(id(&x))+from(&t)}",
        Some(92),
    );
}
#[test]
fn whole_object_borrow_rejects_projected_writes_and_escaping_locals() {
    for text in [
        "struct P{x:i64;} fn result()->i64 {var p=P{x:1};let a=&p;p.x=2;return (*a).x}",
        "struct P{x:i64;} fn id(x:&P)->&P=>x; fn bad()->&P {let p=P{x:1};return id(&p)} fn result()->i64=>0",
        "fn bad(x:[i64;2])->&[i64;2] {return &x} fn result()->i64=>0",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
    }
}
#[test]
fn raw_address_cannot_hide_invalid_nominal_shape() {
    let src = source(
        "struct P{x:i64;} fn get(p:&P)->i64=> (*p).x; fn result()->i64 {let p=P{x:3};return get(&p)}",
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    for bad in [
        Type::Record {
            name: "".into(),
            fields: vec![],
        },
        Type::Record {
            name: "P".into(),
            fields: vec![("x".into(), Type::I64), ("x".into(), Type::I64)],
        },
    ] {
        let mut forged = module.clone();
        forged.functions[0].locals[0].ty = Type::Address {
            mutable: false,
            inner: Box::new(bad),
        };
        let mut claims = plan.clone();
        claims.module_fingerprint = forged.fingerprint();
        let errors = zil_verify::verify_module(forged, claims, &target, &limits).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Verify);
        assert_eq!(errors[0].code, "E-VERIFY-AGGREGATE");
    }
}
