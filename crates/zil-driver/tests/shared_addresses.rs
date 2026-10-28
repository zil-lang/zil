//! S1a actual source, LLVM/native behavior and untrusted Hold controls.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{HoldRoot, InstructionKind, LocalId, Type, TypedModule};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-shared-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "shared.zil", text)
}
fn execute(path: &Path, expected: i64) {
    let mut child = Command::new(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native shared-address program timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, format!("{expected}\n").as_bytes());
    assert!(output.stderr.is_empty());
}
fn native(text: &str, expected: i64) -> String {
    let temp = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(temp.0.join("program")),
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
    assert!(
        llvm.contains("getelementptr i8, ptr %local."),
        "missing real source address: {llvm}"
    );
    execute(&report.artifacts["executable"], expected);
    let optimized = temp.0.join("optimized");
    let clang = Command::new("clang")
        .args(["--target=x86_64-unknown-linux-gnu", "-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(
        clang.status.success(),
        "{}",
        String::from_utf8_lossy(&clang.stderr)
    );
    execute(&optimized, expected);
    llvm
}
fn reject(text: &str, category: ResultCategory, phase: Phase, code: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
}
fn typed(text: &str) -> TypedModule {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

#[test]
fn scalar_address_copies_overwrites_and_rhs_last_use() {
    native(
        "fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; var p: &i64 = &x; let q = p; let first = *q; p = &y; x = 100; y = *p + 1; return first + x + y; }",
        143,
    );
    native(
        "fn result() -> i32 { var x = 40; let p = &x; let q = p; x = *q + 2; return x; }",
        42,
    );
}

#[test]
fn branch_shadow_loop_and_scalar_parameter_storage() {
    native(
        "fn calc(x: i64) -> i64 { let p = &x; return *p; } fn result() -> i64 { var x: i64 = 40; let p = &x; var total: i64 = 0; if true { var x: i64 = 7; let inner = &x; total = *inner + *p; x = 8; total = total + x; } let end: i64 = 3; for i in 0..end { let q = &i; total = total + *q; } x = 100; return total + x + calc(2); }",
        160,
    );
}

#[test]
fn bool_unit_and_i64_reference_types_preserve_operand_inference() {
    native(
        "fn result() -> i64 { let yes = true; let flag = &yes; let unit = (); let u = &unit; *u; let big: i64 = 2147483648; let p = &big; if *flag { return 1 + *p; } return 0; }",
        2147483649,
    );
}

#[test]
fn mixed_move_environment_keeps_pointer_and_snapshot_distinct() {
    let llvm = native(
        "fn result() -> i64 { var x: i64 = 40; var other: i64 = 8; var borrowed = &x; var snapshot: i64 = 2; let f = move () => *borrowed + snapshot; let g = f; borrowed = &other; snapshot = 99; let total = f() + g() + *borrowed; x = 100; other = 10; return total + x + snapshot + other; }",
        301,
    );
    assert!(
        llvm.contains("type { ptr, i64 }"),
        "mixed environment did not store a pointer and scalar: {llvm}"
    );
    assert!(
        llvm.contains("load i64, ptr %v."),
        "closure must load through the captured pointer"
    );
}

#[test]
fn address_constraints_apply_before_ct_and_inside_closure_body() {
    native(
        "fn result() -> i64 { let base = #(2147483647 + 1); let p = &base; let f = move (delta) => { let q = p; return *q + delta; }; return f(2); }",
        2147483650,
    );
    native(
        "fn result() -> i64 { let x: i64 = 40; let p = &x; let f = move (delta) => { let q: &i64 = p; return *q + delta; }; return f(#(1 + 1)); }",
        42,
    );
    native(
        "fn result() -> i64 { var x: i64 = 40; let f = () => { let p = &x; return *p + 2; }; let result = f(); x = 100; return result + x; }",
        142,
    );
}

#[test]
fn every_live_address_and_mixed_closure_alias_keeps_its_source() {
    for text in [
        "fn result() -> i32 { var x = 1; let p = &x; x = 2; return *p; }",
        "fn result() -> i32 { var x = 1; let p = &x; let q = p; *p; x = 2; return *q; }",
        "fn result() -> i32 { var x = 1; let p = &x; for i in 0..2 { *p; x = 2; } return 0; }",
        "fn result() -> i32 { var x = 1; var y = 2; var p = &x; if true { p = &y; } x = 3; return *p; }",
        "fn result() -> i32 { var x = 1; let p = &x; let f = move () => *p; let g = f; f(); x = 2; return g(); }",
        "fn result() -> i32 { var x = 1; var y = 2; var p = &x; let f = move () => *p; p = &y; x = 3; return f(); }",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn storage_end_cannot_leave_an_address_or_captured_address_live() {
    for text in [
        "fn result() -> i32 { let x = 1; var p = &x; if true { let inner = 2; p = &inner; } return *p; }",
        "fn result() -> i32 { let x = 1; var p = &x; while true { let inner = 2; p = &inner; break; } return *p; }",
        "fn result() -> i32 { let x = 1; var p = &x; if true { let inner = 2; p = &inner; } let f = move () => *p; return f(); }",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn unsupported_reference_contracts_and_authority_remain_explicit() {
    // Exact previously closed inputs are now admitted by189 and184.
    for text in [
        "fn result() -> i32 { let f = (p: &i32) => *p; return 0; }",
        "fn result() -> i32 { let x = 1; let f = () => &x; return 0; }",
    ] {
        // These exact inputs do not invoke their closure. Their signature
        // admission must not require the local-address GEP used by native().
        for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
            let temp = Temp::new();
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    optimization,
                    entry: Some("result".into()),
                    output: Some(temp.0.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            execute(&report.artifacts["executable"], 0);
        }
    }
    native(
        "fn result() -> i32 { var x = 1; let p = &mut x; *p=42; return x; }",
        42,
    );
    native(
        "fn result() -> i32 { let x = 42; let p = &x; let f = () => *p; return f(); }",
        42,
    );
    reject(
        "fn result() -> i32 { var x=1; let p=&x; let f=()=>*p; x=2; return f(); }",
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-CONFLICT",
    );
    reject(
        "fn result() -> i32 { var x = 1; let p = &x; *p = 2; return x; }",
        ResultCategory::SafeReject,
        Phase::Types,
        "E-ADDRESS-PERMISSION",
    );
    for text in [
        "fn result() -> i32 { let x = 1; let p = &x; let q = &p; return 0; }",
        "fn result() -> i32 { let p = &(1 + 2); return *p; }",
    ] {
        reject(
            text,
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        );
    }
}

#[test]
fn mixed_environment_and_capture_hold_cannot_be_erased_or_forged() {
    let module = typed(
        "fn result() -> i32 { let x = 40; let p = &x; let snapshot = 2; let f = move () => *p + snapshot; return f(); }",
    );
    assert_eq!(
        module.closures[0].captures,
        vec![
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32)
            },
            Type::I32
        ]
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let make = module.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap()
        .result
        .unwrap();
    assert_eq!(
        plan.functions[0].holds[make.0 as usize].roots,
        [HoldRoot::Local(LocalId(0))]
    );
    assert!(
        plan.functions[1]
            .holds
            .iter()
            .any(|h| h.roots == [HoldRoot::Capture(0)])
    );
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    for mode in 0..3 {
        let mut bad = plan.clone();
        match mode {
            0 => bad.functions[0].holds[make.0 as usize].roots.clear(),
            1 => bad.functions[1]
                .holds
                .iter_mut()
                .for_each(|h| h.roots.clear()),
            _ => bad.functions[0].holds[make.0 as usize].roots = vec![HoldRoot::Local(LocalId(2))],
        }
        assert_eq!(
            zil_verify::verify_module(module.clone(), bad, &module.target, &Limits::default())
                .unwrap_err()[0]
                .code,
            "E-VERIFY-HOLD"
        );
    }
}
