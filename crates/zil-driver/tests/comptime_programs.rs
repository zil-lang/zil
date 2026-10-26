//! Real source -> pure CT CFG -> independent verification -> VM -> native tests.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project, compile_source};
use zil_ir::{CtProgram, InstructionKind, Type};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-ct-program-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "ct-program.zil", text)
}
fn run(path: &Path, expected: i64) {
    let out = Command::new(path).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        expected.to_string()
    );
    assert!(out.stderr.is_empty());
}
fn native(text: &str, expected: i64) {
    let temp = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(temp.0.join("program")),
            emit_dir: Some(temp.0.join("ir")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let core = fs::read_to_string(&report.artifacts["ct-core"]).unwrap();
    assert!(!core.contains("CompileTime"), "CT did not finish: {core}");
    run(&report.artifacts["executable"], expected);
    let optimized = temp.0.join("optimized");
    let out = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    run(&optimized, expected);
}
fn rejection(text: &str, options: CompileOptions, category: ResultCategory, code: &str) {
    let report = compile_source(&source(text), &options);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn ct027_01_recursive_functions_and_local_mutation_execute_natively() {
    native(
        "fn fib(n: i64) -> i64 { if n < 2 { return n } return fib(n-1) + fib(n-2) }\nfn result() -> i64 { return #(fib(10)) }",
        55,
    );
    native(
        "fn gcd(a: i64, b: i64) -> i64 { var x = a; var y = b; while y != 0 { let t = x % y; x = y; y = t } return x }\nfn result() -> i64 { return #(gcd(252, 105)) }",
        21,
    );
}
#[test]
fn ct027_02_range_continue_break_and_nested_calls() {
    native(
        "fn square(x: i64) -> i64 { return x*x } fn sum(n: i64) -> i64 { var total: i64 = 0; for i in 0..n { if i % 2 != 0 { continue } if i > 8 { break } total += square(i) } return total } fn result() -> i64 { return #(sum(20)) }",
        120,
    );
}
#[test]
fn ct027_03_short_circuit_and_branch_skip_unexecuted_arithmetic() {
    native(
        "fn result() -> i32 { let a = #(false && (1 / 0 == 0)); let b = #(true || (1 / 0 == 0)); let c = #(if true { 42 } else { 1/0 }); if !a && b { return c } return 0 }",
        42,
    );
}
#[test]
fn ct027_04_i64_context_wrapping_and_unit_calls() {
    native(
        "fn ping() {} fn wrap(x: i64) -> i64 { ping(); return x +% 1 } fn result() -> i64 { return #(wrap(9223372036854775807)) }",
        i64::MIN,
    );
    native(
        "fn large() -> i64 { return 5000000000 } fn result() -> i64 { let v = #(large()); return v }",
        5_000_000_000,
    );
}
#[test]
fn ct027_13_unsuffixed_literals_share_runtime_type_context() {
    native(
        "fn large() -> i64 { return 5000000000 } fn result() -> i64 { let v = #(1 + large()); return v }",
        5_000_000_001,
    );
    native(
        "fn large() -> i64 { return 5000000000 } fn result() -> i64 { let v = #(if false { 0 } else { large() }); return v }",
        5_000_000_000,
    );
}
#[test]
fn ct027_05_mutual_recursion_and_nested_compile_time_marker() {
    native(
        "fn even(n: i32) -> bool { if n == 0 { return true } return odd(n-1) } fn odd(n: i32) -> bool { if n == 0 { return false } return even(n-1) } fn result() -> i32 { if #(even(#(8 + 2))) { return 91 } return 0 }",
        91,
    );
}
#[test]
fn ct027_06_calls_observe_checked_arithmetic_failures() {
    rejection(
        "fn f(x: i64) -> i64 { return x + 1 } fn result() -> i64 { return #(f(9223372036854775807)) }",
        CompileOptions::default(),
        ResultCategory::SafeReject,
        "E-CT-ARITHMETIC",
    );
    rejection(
        "fn f(x: i64) -> i64 { return x / 0 } fn result() -> i64 { return #(f(1)) }",
        CompileOptions::default(),
        ResultCategory::SafeReject,
        "E-CT-ARITHMETIC",
    );
}
#[test]
fn ct027_07_execution_and_recursion_budgets_do_not_fall_back() {
    let mut options = CompileOptions::default();
    options.limits.max_ct_steps = 80;
    rejection(
        "fn spin() -> i32 { var x = 0; while true { x += 1 } } fn result() -> i32 { return #(spin()) }",
        options,
        ResultCategory::ResourceLimit,
        "E-RESOURCE-LIMIT",
    );
    let mut options = CompileOptions::default();
    options.limits.max_call_depth = 12;
    rejection(
        "fn recurse(n: i32) -> i32 { return recurse(n + 1) } fn result() -> i32 { return #(recurse(0)) }",
        options,
        ResultCategory::ResourceLimit,
        "E-RESOURCE-LIMIT",
    );
}
#[test]
fn ct027_08_runtime_bindings_and_shadowed_function_names_are_rejected() {
    rejection(
        "fn f(x: i32) -> i32 { return x } fn result() -> i32 { let x = 42; return #(f(x)) }",
        CompileOptions::default(),
        ResultCategory::SafeReject,
        "E-CT-RUNTIME",
    );
    rejection(
        "fn f(x: i32) -> i32 { return x } fn result() -> i32 { let f = (x: i32) => x + 1; return #(f(42)) }",
        CompileOptions::default(),
        ResultCategory::SafeReject,
        "E-CT-RUNTIME",
    );
}
#[test]
fn ct027_09_multiple_queries_share_actual_execution_budget() {
    let text = "fn zero() -> i32 { return 0 } fn result() -> i32 { let a = #(zero()); let b = #(zero()); return a + b }";
    // Each query executes a six-step scoped query entry and a six-step scoped
    // callee. Scope markers are verified no-ops but their actual VM visits still
    // consume the shared logical work budget. No production limit is increased.
    let mut single = CompileOptions::default();
    single.limits.max_ct_steps = 12;
    let report = compile_source(
        &source("fn zero()->i32{return 0} fn result()->i32{return #(zero())}"),
        &single,
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    for limit in [12, 23] {
        let mut options = CompileOptions::default();
        options.limits.max_ct_steps = limit;
        rejection(
            text,
            options,
            ResultCategory::ResourceLimit,
            "E-RESOURCE-LIMIT",
        );
    }
    let mut options = CompileOptions::default();
    options.limits.max_ct_steps = 24;
    let report = compile_source(&source(text), &options);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}
#[test]
fn ct027_10_project_calls_respect_visibility_and_module_identity() {
    let temp = Temp::new();
    let mut project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import math\nfn result() -> i64 { return #(math.sum(10)) }",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "math.zil",
                "pub fn sum(n: i64) -> i64 { var v: i64 = 0; for i in 0..n { v += i } return v }",
            ),
        ],
        paths: BTreeMap::from([(vec!["math".into()], ModuleId(1))]),
    };
    let report = compile_project(
        &project,
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
    run(&report.artifacts["executable"], 45);
    project.modules[1] = SourceUnit::new(
        FileId(1),
        ModuleId(1),
        "math.zil",
        "fn sum(n: i64) -> i64 { return n }",
    );
    let report = compile_project(&project, &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Ct);
    assert_eq!(report.diagnostics[0].code, "E-MODULE-PRIVATE");
}
fn raw(text: &str) -> CtProgram {
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    CtProgram {
        entry: module
            .functions
            .iter()
            .find(|f| f.name == "result")
            .unwrap()
            .id,
        module,
        plan,
        max_steps: limits.max_ct_steps,
        max_call_depth: limits.max_call_depth,
        max_live_slots: limits.max_values + limits.max_locals,
    }
}
#[test]
fn ct027_11_raw_wire_reverification_and_forged_claims() {
    let raw = raw(
        "fn sum(n: i64) -> i64 { var v: i64 = 0; for i in 0..n { v += i } return v } fn result() -> i64 { return sum(10) }",
    );
    let limits = Limits::default();
    let target = TargetProfile::default();
    let bytes = zil_ir::wire::encode_ct_program(&raw, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_ct_program(&bytes, &limits).unwrap(),
        raw
    );
    let verified = zil_verify::verify_ct_program_bytes(&bytes, &target, &limits).unwrap();
    let value = zil_ct::evaluate_program(&verified).unwrap();
    assert_eq!(
        value.value,
        zil_ct::CompileTimeValue::Scalar(zil_ir::Constant::I64(45))
    );
    assert!(value.steps > 10);
    let mut invalid = bytes.clone();
    invalid.push(0);
    assert!(zil_verify::verify_ct_program_bytes(&invalid, &target, &limits).is_err());
    let mut forged = raw.clone();
    let f = &mut forged.module.functions[0];
    let i = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Constant(_)))
        .unwrap();
    f.values[i.result.unwrap().0 as usize] = Type::Bool;
    forged.plan.module_fingerprint = forged.module.fingerprint();
    assert!(zil_verify::verify_ct_program(forged, &target, &limits).is_err());
    let mut forged = raw.clone();
    forged.entry = zil_ir::FunctionId(99);
    assert!(zil_verify::verify_ct_program(forged, &target, &limits).is_err());
    let mut bounded = raw;
    bounded.max_live_slots = 1;
    let verified = zil_verify::verify_ct_program(bounded, &target, &limits).unwrap();
    assert_eq!(
        zil_ct::evaluate_program(&verified).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}
#[test]
fn ct027_12_ordinary_verified_addresses_do_not_grant_ct_permission() {
    let raw = raw("fn result() -> i64 { let x: i64 = 42; let p = &x; return *p }");
    let target = TargetProfile::default();
    let limits = Limits::default();
    assert!(
        zil_verify::verify_module(raw.module.clone(), raw.plan.clone(), &target, &limits).is_ok()
    );
    let errors = zil_verify::verify_ct_program(raw, &target, &limits).unwrap_err();
    assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
}

#[test]
fn ct027_14_query_selection_preserves_later_integer_context() {
    for (expression, expected) in [
        ("5000000000 + large()", 10000000000),
        ("-5000000000 + large()", 0),
    ] {
        native(
            &format!(
                "fn large() -> i64 {{ return 5000000000 }} fn result() -> i64 {{ let value = #({expression}); return value }}"
            ),
            expected,
        );
    }
}
