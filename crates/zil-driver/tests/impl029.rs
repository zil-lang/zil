//! Native tagged unions, useful/exhaustive matching, and independent raw controls.
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
            "zil-enum-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "enum.zil", text)
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
            None => assert!(
                !observed.status.success(),
                "checked enum projection must trap"
            ),
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
fn typed(text: &str) -> zil_ir::TypedModule {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
#[test]
fn enum029_01_unit_and_tuple_payloads_cross_function_native() {
    let text = include_str!("../../../examples/impl029/main.zil")
        .replace("fn answer()", "fn result()")
        .replace("answer() ==", "result() ==");
    let llvm = native(&text, Some(5000000049));
    assert!(llvm.contains("{ i32, [2 x i64] }"));
    assert!(!llvm.contains("malloc"));
    assert!(!llvm.contains(" undef"));
}
#[test]
fn enum029_02_nested_enum_tuple_bool_and_integer_matrix() {
    native(
        "enum Inner { None, Some(i32) } enum Outer { Missing, Present(Inner), Pair((bool, Inner)) } fn score(v: Outer) -> i32 { match v { Outer.Missing() => { return 0 }, Outer.Present(Inner.None()) => { return 1 }, Outer.Present(Inner.Some(n)) => { return n }, Outer.Pair((true, Inner.Some(n))) => { return n + 100 }, Outer.Pair((_, _)) => { return 200 } } } fn result() -> i32 { return score(Outer.Missing) + score(Outer.Present(Inner.None)) + score(Outer.Present(Inner.Some(42))) + score(Outer.Pair((true, Inner.Some(9)))) + score(Outer.Pair((false, Inner.None))) }",
        Some(352),
    );
}
#[test]
fn enum029_03_unit_zero_array_and_record_padding_payloads() {
    let llvm = native(
        "struct Bits { unit: (); flag: bool; n: i64; tail: (); } enum Data { Unit(()), Empty([i64; 0]), Mixed(Bits, [(); 2], (bool, i64)) } fn score(v: Data) -> i64 { match v { Data.Unit(u) => { u; return 1 }, Data.Empty(empty) => { return 2 }, Data.Mixed(bits, units, pair) => { units[0]; if bits.flag { return bits.n + pair.1 } return 0 } } } fn result() -> i64 { return score(Data.Unit(())) + score(Data.Empty([])) + score(Data.Mixed(Bits { unit: (), flag: true, n: 39, tail: () }, [(), ()], (true, 3))) }",
        Some(45),
    );
    assert!(llvm.contains("{ i32, [4 x i64] }"));
    assert!(llvm.contains("store i8"));
    assert!(!llvm.contains(" undef"));
}
#[test]
fn enum029_04_array_record_update_and_copy_return() {
    native(
        "enum Item { Empty, Number(i64), Flag(bool) } struct Holder { item: Item; n: i64; } fn id(v: Item) -> Item { return v } fn score(v: Item) -> i64 { match v { Item.Empty() => { return 0 }, Item.Number(n) => { return n }, Item.Flag(true) => { return 1 }, Item.Flag(false) => { return 2 } } } fn result() -> i64 { var values = [Item.Number(10), Item.Empty, Item.Flag(false)]; values[1] = Item.Number(30); var h = Holder { item: id(values[0]), n: 7 }; let copy = h; h.item = Item.Flag(true); var total: i64 = 0; for i in 0..3 { total += score(values[i]) } return total + score(h.item) + score(copy.item) + h.n }",
        Some(60),
    );
}
#[test]
fn enum029_05_variant_replacement_in_loop_uses_fixed_scratch() {
    let llvm = native(
        "enum State { Off, On(i64) } fn value(v: State) -> i64 { match v { State.Off() => { return 0 }, State.On(n) => { return n } } } fn result() -> i64 { var state = State.Off; var i: i64 = 0; while i < 10000 { state = State.On(i); i += 1 } return value(state) }",
        Some(9999),
    );
    for function in llvm.split("define ").skip(1) {
        if let Some((_, body)) = function.split_once("bb.0:") {
            assert!(!body.split("\n}").next().unwrap().contains("alloca"));
        }
    }
}
#[test]
fn enum029_06_scalar_match_ct_and_constructor_ct_context() {
    native(
        "enum E { N(i64) } fn pick(b: bool) -> i64 { match b { true => { return 40 }, false => { return 2 } } } fn result() -> i64 { let v = E.N(#(5000000000)); match v { E.N(n) => { return n + #(pick(true) + pick(false)) } } }",
        Some(5000000042),
    );
}
#[test]
fn enum029_07_match_input_emits_one_call() {
    let text = "enum E { A(i32), B } fn make() -> E { return E.A(42) } fn result() -> i32 { match make() { E.A(n) => { return n }, E.B() => { return 0 } } }";
    native(text, Some(42));
    let module = typed(text);
    let body = &module
        .functions
        .iter()
        .find(|f| f.name == "result")
        .unwrap()
        .blocks;
    assert_eq!(
        body.iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind, InstructionKind::Call { .. }))
            .count(),
        1
    );
}
#[test]
fn enum029_08_nonexhaustive_and_shadowed_arms_reject() {
    for (text, code) in [
        (
            "enum E { A, B } fn result() -> i32 { match E.A { E.A() => { return 1 } } }",
            "E-MATCH-NONEXHAUSTIVE",
        ),
        (
            "enum E { A(bool) } fn result() -> i32 { match E.A(true) { E.A(true) => { return 1 } } }",
            "E-MATCH-NONEXHAUSTIVE",
        ),
        (
            "fn result() -> i32 { match 4 { 0 => { return 0 }, 1 => { return 1 } } }",
            "E-MATCH-NONEXHAUSTIVE",
        ),
        (
            "enum E { A, B } fn result() -> i32 { match E.A { E.A() => { return 1 }, E.A() => { return 2 }, E.B() => { return 3 } } }",
            "E-MATCH-UNREACHABLE",
        ),
        (
            "enum E { A, B } fn result() -> i32 { match E.A { _ => { return 1 }, E.B() => { return 2 } } }",
            "E-MATCH-UNREACHABLE",
        ),
        (
            "fn result() -> i32 { match 1 { 1 => { return 1 }, 0x1 => { return 2 }, _ => { return 0 } } }",
            "E-MATCH-UNREACHABLE",
        ),
        (
            "enum E { A(bool,bool) } fn result() -> i32 { match E.A(true,true) { E.A(true,_) => { return 1 }, E.A(false,true) => { return 2 }, E.A(false,false) => { return 3 }, E.A(_,_) => { return 4 } } }",
            "E-MATCH-UNREACHABLE",
        ),
    ] {
        reject(text, code);
    }
}
#[test]
fn enum029_09_pattern_arity_names_nominality_and_infinite_layout_reject() {
    for (text, code) in [
        (
            "enum E { A(i32) } fn result() -> i32 { let e = E.A(); return 0 }",
            "E-ENUM-ARITY",
        ),
        (
            "enum E { A(i32) } fn result() -> i32 { match E.A(1) { E.A() => { return 0 } } }",
            "E-MATCH-ARITY",
        ),
        (
            "enum E { A(i32,i32) } fn result() -> i32 { match E.A(1,2) { E.A(n,n) => { return n } } }",
            "E-MATCH-BINDING",
        ),
        (
            "enum E { A } enum F { A } fn result() -> i32 { match E.A { F.A() => { return 0 } } }",
            "E-MATCH-TYPE",
        ),
        (
            "enum E { A } fn result() -> i32 { let e=E.Missing; return 0 }",
            "E-ENUM-VARIANT",
        ),
        (
            "enum E { A, A } fn result() -> i32 { return 0 }",
            "E-ENUM-VARIANT-DUPLICATE",
        ),
        (
            "enum E { A(E) } fn result() -> i32 { return 0 }",
            "E-INFINITE-LAYOUT",
        ),
        (
            "enum E { A(S) } struct S { e:E; } fn result() -> i32 { return 0 }",
            "E-INFINITE-LAYOUT",
        ),
        (
            "enum E { A(i32) } fn result() -> i32 { match E.A(1) { E.A(self) => { return 0 } } }",
            "E-RECEIVER-RESERVED-NAME",
        ),
    ] {
        reject(text, code);
    }
}
fn raw_native(verified: &zil_verify::VerifiedModule, expected: Option<i64>) -> String {
    let temp = Temp::new();
    let function = verified
        .module()
        .functions
        .iter()
        .find(|f| f.name == "result")
        .unwrap()
        .id;
    let lir = zil_lower::lower(verified).unwrap();
    let llvm = zil_backend_llvm::emit_with_options(
        verified,
        &lir,
        &zil_backend_llvm::EmitOptions {
            entry: zil_backend_llvm::EntryPoint::ReturnValue(function),
        },
    )
    .unwrap();
    let ir = temp.0.join("raw.ll");
    fs::write(&ir, &llvm).unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = temp.0.join(optimization);
        let output = Command::new("clang")
            .args(["-x", "ir", optimization])
            .arg(&ir)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(executable).output().unwrap();
        match expected {
            Some(value) => {
                assert!(output.status.success());
                assert_eq!(output.stdout, format!("{value}\n").as_bytes());
            }
            None => {
                assert!(
                    !output.status.success(),
                    "forged mismatching projection must trap"
                );
                assert!(output.stdout.is_empty());
            }
        }
    }
    llvm
}
const RAW_ENUM: &str = "enum E { A(i64), B(i64) } fn result() -> i64 { let value = E.A(42); match value { E.A(n) => { return n }, E.B(n) => { return n } } }";
#[test]
fn enum029_10_wire_and_forged_variant_types_indices_reverify() {
    let module = typed(RAW_ENUM);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let verified =
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    assert_eq!(verified.module(), &module);
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    for change in 0..4 {
        let mut altered = module.clone();
        let instructions = altered.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions);
        for instruction in instructions {
            match &mut instruction.kind {
                InstructionKind::MakeVariant { variant, .. } if change == 0 => {
                    *variant = 99;
                    break;
                }
                InstructionKind::MakeVariant { fields, .. } if change == 1 => {
                    fields.clear();
                    break;
                }
                InstructionKind::ReadVariant { index, .. } if change == 2 => {
                    *index = 99;
                    break;
                }
                InstructionKind::ReadVariant { value, .. } if change == 3 => {
                    *value = zil_ir::ValueId(0);
                    break;
                }
                _ => {}
            }
        }
        let mut stale = plan.clone();
        stale.module_fingerprint = altered.fingerprint();
        let errors =
            zil_verify::verify_module(altered, stale, &module.target, &limits).unwrap_err();
        assert_eq!(errors[0].code, "E-VERIFY-ENUM", "{errors:?}");
    }
}
#[test]
fn enum029_11_checked_raw_payload_mismatch_traps_at_o0_and_o2() {
    let mut module = typed(RAW_ENUM);
    for instruction in module.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::ReadVariant { variant, .. } = &mut instruction.kind {
            *variant = 1;
            break;
        }
    }
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let verified =
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap();
    let llvm = raw_native(&verified, None);
    assert!(llvm.contains("icmp ne i32"));
    assert!(llvm.contains("call void @llvm.trap()"));
}
#[test]
fn enum029_12_explicit_trap_is_ct_error_and_unreachable_still_rejects() {
    let mut module = typed("fn result() -> i64 { return 0 }");
    // Trap aborts with its active root; it must not retain a normal-return promise.
    module.functions[0].blocks[0].instructions.retain(|i| {
        !matches!(
            i.kind,
            InstructionKind::ScopeExit {
                kind: zil_ir::CleanupExitKind::Return,
                ..
            }
        )
    });
    module.functions[0].blocks[0].terminator = zil_ir::Terminator::Trap;
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let target = module.target.clone();
    let raw = zil_ir::CtProgram {
        entry: zil_ir::FunctionId(0),
        module: module.clone(),
        plan,
        max_steps: limits.max_ct_steps,
        max_call_depth: limits.max_call_depth,
        max_live_slots: limits.max_values + limits.max_locals,
    };
    let verified = zil_verify::verify_ct_program(raw, &target, &limits).unwrap();
    let error = zil_ct::evaluate_program(&verified).unwrap_err();
    assert_eq!(error[0].code, "E-CT-TRAP");
    assert_eq!(error[0].phase, Phase::Ct);
    module.functions[0].blocks[0].terminator = zil_ir::Terminator::Unreachable;
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module(module, plan, &target, &limits).unwrap_err()[0].code,
        "E-VERIFY-UNREACHABLE"
    );
}
#[test]
fn enum029_13_copy_enum_ct_evaluates_and_runtime_pattern_name_stays_runtime() {
    let module = typed(RAW_ENUM);
    let target = module.target.clone();
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let raw = zil_ir::CtProgram {
        entry: zil_ir::FunctionId(0),
        module,
        plan,
        max_steps: limits.max_ct_steps,
        max_call_depth: limits.max_call_depth,
        max_live_slots: limits.max_values + limits.max_locals,
    };
    // 105 permits finite Copy-enum CT only after independent verification.
    let verified = zil_verify::verify_ct_program(raw, &target, &limits).unwrap();
    assert_eq!(
        zil_ct::evaluate_program(&verified).unwrap().value,
        zil_ct::CompileTimeValue::Scalar(zil_ir::Constant::I64(42))
    );
    let report = compile_source(
        &source(
            "fn helper() -> i32 { return 42 } enum E { V(i32) } fn result() -> i32 { match E.V(1) { E.V(helper) => { return #(helper()) } } }",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.diagnostics[0].code, "E-CT-RUNTIME");
}
#[test]
fn enum029_14_union_uses_max_payload_instead_of_all_variants() {
    let llvm = native(
        "enum E { A(i64,i64), B(i32,i32,i32), C(bool), D(()) } fn score(e:E) -> i64 { match e { E.A(a,b) => { return a+b }, E.B(a,b,c) => { return 3 }, E.C(true) => { return 1 }, E.C(false) => { return 0 }, E.D(u) => { u; return 2 } } } fn result() -> i64 { return score(E.A(10,20)) + score(E.B(1,2,3)) + score(E.C(true)) + score(E.D(())) }",
        Some(36),
    );
    assert!(llvm.contains("{ i32, [2 x i64] }"));
    assert!(!llvm.contains("{ i32, [3 x i64] }"));
}
#[test]
fn enum029_15_match_break_continue_join_and_shared_result_contract_compose() {
    native(
        "enum Step { Skip, Add(i64), Stop } fn step(i:i64) -> Step { if i == 9 { return Step.Stop } if i % 2 == 0 { return Step.Skip } return Step.Add(i) } fn choose(a:&i64,b:&i64,flag:bool)->&i64 { match flag { true => { return a }, false => { return b } } } fn result()->i64 { var total:i64=0; let end:i64=20; for i in 0..end { match step(i) { Step.Skip() => { continue }, Step.Stop() => { break }, Step.Add(n) => { total += n } } } var x:i64=40; var y:i64=2; let p=choose(&x,&y,false); let saved=*p; x=100; y=200; return total+saved+x+y }",
        Some(318),
    );
}
#[test]
fn enum029_16_type_name_shadow_and_budget_fail_closed() {
    native(
        "enum E { A(i32) } struct Value { A:i32; } fn result()->i32 { let E=Value { A:42 }; return E.A }",
        Some(42),
    );
    let mut module = typed(RAW_ENUM);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let mut nested = Type::I64;
    for _ in 0..limits.max_call_depth + 2 {
        nested = Type::Enum {
            name: "Deep".into(),
            variants: vec![("V".into(), vec![nested])],
        };
    }
    module.functions[0].return_type = nested;
    assert_eq!(
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
}
