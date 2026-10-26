//! Function values, indirect calls, and independent raw CFG rejection.
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
use zil_ir::{InstructionKind, Type};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-function-pointer-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "function-pointer.zil", text)
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
fn function_factory_aggregation_and_indirect_calls_execute() {
    let llvm = native(
        include_str!("../../../examples/impl033/main.zil"),
        Some(8320),
    );
    assert!(llvm.contains("freeze ptr @zil.fn."));
    assert!(llvm.contains("call i64 %v."));
    assert!(llvm.contains("call { i64, i64 } %v."));
    assert!(!llvm.contains("@malloc"));
}
#[test]
fn unit_signature_and_function_returned_by_indirect_call_execute() {
    native(
        "fn noop(x:())->(){} fn id(x:i64)->i64=>x; fn factory()->fn(i64)->i64=>id; fn result()->i64{let n=noop;n(());let f=factory;return f()(7000000000)}",
        Some(7000000000),
    );
}

#[test]
fn indirect_call_context_preserves_large_integer_and_static_method_value() {
    native(
        "struct P{x:i64;} impl P{fn make(x:i64)->Self=>P{x:x};fn get(self:Self)->i64=>.x;} fn result()->i64{let make=P.make;let get=P.get;let f=if true {make}else{P.make};return get(f(5000000000))+6000000000}",
        Some(11000000000),
    );
    native(
        "fn id(x:i64)->i64=>x;fn result()->i64{let f=id;return 6000000000+f(5000000000)}",
        Some(11000000000),
    );
}

#[test]
fn source_function_pointer_signature_and_environment_boundaries() {
    for (text, category, code) in [
        (
            "fn f(x:i64)->i64=>x;fn result()->i32{let p:fn(i32)->i32=f;return 0}",
            ResultCategory::SafeReject,
            "E-TYPE-MISMATCH",
        ),
        (
            "fn f(x:i64)->i64=>x;fn result()->i64{let p=f;return p()}",
            ResultCategory::SafeReject,
            "E-CALL-ARITY",
        ),
        (
            "fn f(x:i64)->i64=>x;fn result()->i64{let p=f;return p(true)}",
            ResultCategory::SafeReject,
            "E-TYPE-MISMATCH",
        ),
        (
            "fn f(x:&i32)->i32=>*x;fn result()->i32{let p=f;return 0}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "fn apply(p:fn(&i32)->i32,x:&i32)->i32=>p(x);fn result()->i32=>0",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "fn result()->i32{let n=3;let p:fn(i32)->i32=move(x:i32)=>x+n;return p(1)}",
            ResultCategory::SafeReject,
            "E-CAPTURED-FN-POINTER",
        ),
        (
            "struct P{x:i64;} impl P{fn get(self:Self)->i64=>.x;} fn result()->i64{let p=P{x:1};let f=p.get;return 0}",
            ResultCategory::SafeReject,
            "E-RECEIVER-METHOD-VALUE",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(report.status, category, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
        assert!(!report.artifacts.contains_key("executable"));
    }
}

fn typed(text: &str) -> zil_ir::TypedModule {
    let src = source(text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

#[test]
fn independent_verify_rejects_forged_function_identity_signature_and_operands() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let module = typed(
        "fn one(x:i64)->i64=>x+1;fn flag(x:bool)->bool=>x;fn result()->i64{let p=one;let n=p(9);let later=one;return n}",
    );
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &module
    );
    for edit in 0..6 {
        let mut raw = module.clone();
        let f = &mut raw.functions[2];
        let future = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .rfind(|i| matches!(i.kind, InstructionKind::FunctionRef { .. }))
            .unwrap()
            .result
            .unwrap();
        let instruction = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| {
                if edit < 2 {
                    matches!(i.kind, InstructionKind::FunctionRef { .. })
                } else {
                    matches!(i.kind, InstructionKind::CallIndirect { .. })
                }
            })
            .unwrap();
        match &mut instruction.kind {
            InstructionKind::FunctionRef { function } => {
                *function = zil_ir::FunctionId(if edit == 0 { 999 } else { 1 })
            }
            InstructionKind::CallIndirect { callee, arguments } => match edit {
                2 => arguments.clear(),
                3 => arguments[0] = *callee,
                4 => *callee = arguments[0],
                5 => *callee = future,
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        let mut claims = plan.clone();
        claims.module_fingerprint = raw.fingerprint();
        let errors = zil_verify::verify_module(raw, claims, &target, &limits).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Verify);
        assert!(errors[0].code.starts_with("E-VERIFY-"), "{errors:?}");
        if edit == 5 {
            assert_eq!(errors[0].code, "E-VERIFY-DOMINANCE");
        }
    }
}

#[test]
fn independent_verify_rejects_environment_target_and_embedded_address_contract() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut raw = typed("fn result()->i32{let c=move(x:i32)=>x+1;return c(2)}");
    let mut plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let closure = raw
        .functions
        .iter()
        .find(|f| f.environment.is_some())
        .unwrap()
        .id;
    let reference = raw.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    reference.kind = InstructionKind::FunctionRef { function: closure };
    plan.module_fingerprint = raw.fingerprint();
    let errors = zil_verify::verify_module(raw, plan, &target, &limits).unwrap_err();
    assert_eq!(errors[0].code, "E-VERIFY-FUNCTION-REF");

    let ordinary = typed("fn result()->i32=>1");
    let plan = zil_analyze::analyze(&ordinary, &limits).unwrap();
    for invalid in [
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Record {
            name: "R".into(),
            fields: vec![(
                "x".into(),
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
            )],
        },
        Type::Closure(zil_ir::ClosureId(0)),
    ] {
        let mut raw = ordinary.clone();
        raw.functions[0].values[0] = Type::FunctionPointer {
            parameters: vec![invalid],
            result: Box::new(Type::I32),
        };
        let mut claims = plan.clone();
        claims.module_fingerprint = raw.fingerprint();
        let errors = zil_verify::verify_module(raw, claims, &target, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::Unsupported,
            "{errors:?}"
        );
        assert_eq!(errors[0].phase, Phase::Verify);
    }
}

#[test]
fn raw_function_signature_is_bounded_before_hashing() {
    let ordinary = typed("fn result()->i32=>1");
    let defaults = Limits::default();
    let plan = zil_analyze::analyze(&ordinary, &defaults).unwrap();
    let mut deep = Type::I32;
    for _ in 0..32 {
        deep = Type::FunctionPointer {
            parameters: vec![deep],
            result: Box::new(Type::I32),
        }
    }
    for (shape, limits) in [
        (
            deep,
            Limits {
                max_call_depth: 16,
                ..defaults.clone()
            },
        ),
        (
            Type::FunctionPointer {
                parameters: vec![Type::Array {
                    element: Box::new(Type::I32),
                    length: u32::MAX,
                }],
                result: Box::new(Type::I32),
            },
            Limits {
                max_analysis_steps: 1024,
                ..defaults.clone()
            },
        ),
    ] {
        let mut raw = ordinary.clone();
        raw.functions[0].values[0] = shape;
        assert_eq!(
            zil_analyze::analyze(&raw, &limits).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        let errors =
            zil_verify::verify_module(raw, plan.clone(), &TargetProfile::default(), &limits)
                .unwrap_err();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert_eq!(errors[0].phase, Phase::Verify);
    }
}

fn project(root: &str, library: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            source(root),
            SourceUnit::new(FileId(1), ModuleId(1), "ops.zil", library),
        ],
        paths: std::collections::BTreeMap::from([(vec!["ops".into()], ModuleId(1))]),
    }
}
#[test]
fn module_function_values_keep_identity_visibility_and_signature_hash() {
    let out = Temp::new();
    let input = project(
        include_str!("../../../examples/impl033/modules/main.zil"),
        include_str!("../../../examples/impl033/modules/ops.zil"),
    );
    let options = CompileOptions {
        stop_after: Stage::Build,
        entry: Some("result".into()),
        output: Some(out.0.join("plain")),
        ..CompileOptions::default()
    };
    let report = compile_project(&input, &options);
    execute(&report, &out, Some(8016));
    let hidden = compile_project(
        &project(
            "import ops;fn result()->i64{let f=ops.double;return f(1)}",
            &input.modules[1].text,
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        hidden.status,
        ResultCategory::SafeReject,
        "{}",
        hidden.to_json()
    );
    assert_eq!(hidden.diagnostics[0].code, "E-MODULE-PRIVATE");
    let first = compile_project(
        &project(
            "import ops;fn result()->i32=>0",
            "fn id(x:i64)->i64=>x;pub fn factory()->fn(i64)->i64=>id",
        ),
        &CompileOptions::default(),
    );
    let second = compile_project(
        &project(
            "import ops;fn result()->i32=>0",
            "fn id(x:i32)->i32=>x;pub fn factory()->fn(i32)->i32=>id",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
    assert_eq!(
        second.status,
        ResultCategory::Success,
        "{}",
        second.to_json()
    );
    assert_ne!(
        first.scalar_interfaces[1].signature_fingerprint,
        second.scalar_interfaces[1].signature_fingerprint
    );
}

#[test]
fn runtime_pointer_capability_does_not_grant_ct_execution_authority() {
    let limits = Limits::default();
    let module = typed("fn id(x:i32)->i32=>x;fn result()->i32{let f=id;return f(1)}");
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let raw = zil_ir::CtProgram {
        module,
        plan,
        entry: zil_ir::FunctionId(1),
        max_steps: limits.max_ct_steps,
        max_call_depth: limits.max_call_depth,
        max_live_slots: limits.max_locals + limits.max_values,
    };
    let errors =
        zil_verify::verify_ct_program(raw, &TargetProfile::default(), &limits).unwrap_err();
    assert_eq!(errors[0].phase, Phase::Ct);
    assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
}

#[test]
fn copying_through_a_shared_pointer_slot_retains_the_function_identity() {
    native(
        "fn id(x:i64)->i64=>x;fn read(p:&(fn(i64)->i64))->fn(i64)->i64=>*p;fn result()->i64{var callback=id;let original=read(&callback);callback=id;return original(5000000001)}",
        Some(5000000001),
    );
}
