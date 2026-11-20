//! Source family: whole programs through typeck → solve → check.
//!
//! Expectations come from the language contract (`spec/source-lowering.md`,
//! `_zil_design.md`) and are written by hand. Positive cases must be
//! admitted by the checker; negative cases must be rejected by typeck with
//! the stated diagnostic code. No case inspects typeck internals.

use zil_safety_cases::*;

fn admit(src: &str) {
    let lowered = match zil_typeck::lower_source("case.zil", src) {
        Ok(l) => l,
        Err(diags) => panic!(
            "typeck rejected a valid program:\n{}",
            diags
                .iter()
                .map(|d| format!("  {} {}", d.code, d.message))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    };
    match verdict(lowered.module) {
        Verdict::Accepted => {}
        other => panic!("pipeline rejected a valid program: {other:?}"),
    }
}

/// Typeck admits the program; the kernel must refuse it for `reason`.
fn unsafe_program(src: &str, reason: &str) {
    let lowered = match zil_typeck::lower_source("case.zil", src) {
        Ok(l) => l,
        Err(diags) => panic!(
            "typeck rejected a program meant for the kernel: {:?}",
            diags
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        ),
    };
    let verdict = verdict(lowered.module);
    let text = format!("{verdict:?}");
    assert!(
        !matches!(verdict, Verdict::Accepted) && text.contains(reason),
        "expected a {reason} rejection, got {text}"
    );
}

fn reject(src: &str, code: &str) {
    match zil_typeck::lower_source("case.zil", src) {
        Ok(_) => panic!("typeck admitted an invalid program (expected {code})"),
        Err(diags) => assert!(
            diags.iter().any(|d| d.code == code),
            "expected {code}, got {:?}",
            diags
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        ),
    }
}

#[test]
fn scalar_arithmetic_and_comparison() {
    admit(
        "fn f(x:i32, y:i32)->i32 {\n\
         let s = x + y\n\
         if s > 10 && !(x == y) {return s * 2}\n\
         return s - 1\n\
         }",
    );
}

#[test]
fn records_and_fields() {
    admit(
        "struct P { x:i32; y:u32 }\n\
         fn f()->u32 {\n\
         let x = 3\n\
         let p = P{x, y:4}\n\
         if p.x != 3 {return 0}\n\
         return p.y\n\
         }",
    );
}

#[test]
fn two_variant_match() {
    admit(
        "enum E { A(i32), B(u32) }\n\
         fn f(e:E)->i32 {\n\
         var r:i32 = 0\n\
         match e {E.A(x)=>{r=x},E.B(y)=>{r=1}}\n\
         return r\n\
         }",
    );
}

#[test]
fn three_variant_match_narrows_otherwise() {
    // The last arm is reached through `otherwise`; the kernel must narrow
    // the remaining tag to that one variant before its field is projected.
    admit(
        "enum C { Small(i8), Wide(u32), Native(isize) }\n\
         fn f(v:C)->u32 {\n\
         var s:u32 = 0\n\
         match v {C.Small(x)=>{s=u32(x)},C.Wide(x)=>{s=x},C.Native(x)=>{s=u32(x)}}\n\
         return s\n\
         }",
    );
}

#[test]
fn nested_patterns_and_wildcards() {
    admit(
        "enum O { Some(i32), None }\n\
         fn f(p:(O, bool))->i32 {\n\
         var r = 0\n\
         match p {(O.Some(x), true)=>{r=x},(O.Some(_), false)=>{r=1},(O.None(), _)=>{r=2}}\n\
         return r\n\
         }",
    );
}

#[test]
fn literal_and_bool_match() {
    admit(
        "fn f(x:i32, b:bool)->i32 {\n\
         var r = 0\n\
         match x {0=>{r=10},1=>{r=11},_=>{r=12}}\n\
         match b {true=>{r=r+1},false=>{r=r+2}}\n\
         return r\n\
         }",
    );
}

#[test]
fn loops_and_compound_assignment() {
    admit(
        "fn f(n:u32)->u32 {\n\
         var i:u32 = 0\n\
         var acc:u32 = 0\n\
         while i < n {acc += i\n i += 1}\n\
         loop {if acc > 100 {break}\n acc *= 2}\n\
         return acc\n\
         }",
    );
}

#[test]
fn generic_identity_is_monomorphised() {
    admit(
        "fn identity<T:Copy>(x:T)->T {return x}\n\
         fn f()->i32 {\n\
         let a = identity(3i8)\n\
         let b = identity(4u32)\n\
         if a != 3i8 || b != 4u32 {return 1}\n\
         return identity(0)\n\
         }",
    );
}

#[test]
fn unsigned_index_and_conversions() {
    admit(
        "fn f()->i16 {\n\
         let values:[i16;3] = [-32768i16, 42i16, 32767i16]\n\
         let index:u32 = 1\n\
         let x = values[index]\n\
         if i8(f64(42u32)) != 42i8 {return 0}\n\
         return x\n\
         }",
    );
}

#[test]
fn minimum_signed_literals_fold() {
    admit(
        "fn f()->isize {\n\
         let a:i8 = -128\n\
         let d:isize = -9223372036854775808\n\
         if a != -128i8 {return 0}\n\
         return d\n\
         }",
    );
}

#[test]
fn nonexhaustive_match_is_rejected() {
    reject(
        "enum E { A, B, C }\n\
         fn f(e:E)->i32 {\n\
         var r = 0\n\
         match e {E.A()=>{r=1},E.B()=>{r=2}}\n\
         return r\n\
         }",
        "E-MATCH-NONEXHAUSTIVE",
    );
}

#[test]
fn assignment_to_let_is_rejected() {
    reject(
        "fn f()->i32 {\n let x = 1\n x = 2\n return x\n }",
        "E-ASSIGN-IMMUTABLE",
    );
}

#[test]
fn type_mismatch_is_rejected() {
    reject(
        "fn f()->i32 {\n let x:u32 = 1\n return x\n }",
        "E-TYPE-MISMATCH",
    );
}

#[test]
fn literal_out_of_range_is_rejected() {
    reject("fn f()->i8 {\n return 128i8\n }", "E-LITERAL-RANGE");
    reject("fn f()->f32 {\n return 1e39f32\n }", "E-LITERAL-RANGE");
}

#[test]
fn missing_return_is_rejected() {
    reject(
        "fn f(b:bool)->i32 {\n if b {return 1}\n }",
        "E-MISSING-RETURN",
    );
}

/// `tests/integers/complete-domains.zil` without its two compile-time
/// evaluation lines (CT is a separate stage).
#[test]
fn complete_domains_without_ct() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/integers/complete-domains.zil"
    );
    let text = std::fs::read_to_string(path).expect("fixture");
    let filtered: String = text
        .lines()
        .filter(|line| !line.contains("#(") && !line.contains("ct.") && !line.contains("pure."))
        .map(|line| format!("{line}\n"))
        .collect();
    admit(&filtered);
}

// ---- addresses -----------------------------------------------------------

const POINT: &str = "struct P { x:i32; y:i32 }\n\
    fn bump(p:&mut P) {p.x += 1}\n\
    fn read(p:&P)->i32 {return p.x}\n";

#[test]
fn exclusive_address_parameters_write_through() {
    admit(&format!(
        "{POINT}fn f()->i32 {{\n var p = P{{x:1, y:2}}\n bump(&mut p)\n bump(&mut p)\n return p.x\n }}"
    ));
}

#[test]
fn an_exclusive_address_is_reborrowed_at_each_call() {
    admit(&format!(
        "{POINT}fn g(r:&mut P)->i32 {{\n bump(r)\n bump(r)\n return r.x\n }}"
    ));
}

#[test]
fn an_exclusive_address_coerces_to_a_shared_one() {
    admit(&format!(
        "{POINT}fn h(r:&mut P)->i32 {{\n return read(r)\n }}"
    ));
}

#[test]
fn shared_addresses_index_arrays() {
    admit(
        "fn sum(a:&[i32;3])->i32 {return a[0] + a[1] + a[2]}\n\
         fn f()->i32 {\n let v = [1, 2, 3]\n return sum(&v) + sum(&v)\n }",
    );
}

#[test]
fn explicit_dereference_reads_and_writes() {
    admit("fn f()->i32 {\n var x = 1\n let r = &mut x\n *r = *r + 1\n return x\n }");
}

#[test]
fn borrowing_a_value_materialises_a_temporary() {
    admit("fn f()->i32 {\n let r = &(1 + 2)\n return *r\n }");
}

#[test]
fn returning_the_address_of_a_local_is_unsafe() {
    unsafe_program(
        "fn bad()->&i32 {\n let x = 1\n return &x\n }",
        // The scope exit ends `x`'s storage while the returned loan is
        // still live; that conflict comes before the return itself.
        "LoanConflict",
    );
}

#[test]
fn reading_while_an_exclusive_address_is_live_is_unsafe() {
    unsafe_program(
        "fn f()->i32 {\n var x = 1\n let a = &mut x\n let b = &x\n *a = 2\n return *b\n }",
        "LoanConflict",
    );
}

#[test]
fn a_moved_exclusive_address_cannot_be_used() {
    unsafe_program(
        "fn f()->i32 {\n var x = 1\n let a = &mut x\n let b = a\n *a = 2\n return *b\n }",
        "Uninit",
    );
}

#[test]
fn exclusive_borrow_of_a_let_binding_is_rejected() {
    reject(
        "fn f()->i32 {\n let x = 1\n let r = &mut x\n return *r\n }",
        "E-ASSIGN-IMMUTABLE",
    );
}

#[test]
fn writing_through_a_shared_address_is_rejected() {
    reject(
        &format!("{POINT}fn w(p:&P) {{p.x = 3}}"),
        "E-ASSIGN-IMMUTABLE",
    );
}

// ---- resources, packages, give, Drop, extern C, error unions --------------

#[test]
fn owned_option_session_fixture_is_admitted_end_to_end() {
    let text = include_str!("../../../tests/fixtures/ownership/owned-option-session.zil");
    admit(text);
}

#[test]
fn conditional_give_uses_drop_flag_and_is_admitted() {
    admit(
        "resource Mark { code: i32 }\n\
         impl Drop for Mark { fn drop(self: Self) {} }\n\
         fn consume(give m: Mark) {}\n\
         fn f(c: bool) {\n\
             let m = Mark { code: 1 }\n\
             if c { consume(give m) }\n\
         }",
    );
}

#[test]
fn conditional_give_of_parameter_is_admitted() {
    admit(
        "resource Mark { code: i32 }\n\
         fn consume(give m: Mark) {}\n\
         fn f(give m: Mark, c: bool) {\n\
             if c { consume(give m) }\n\
         }",
    );
}

#[test]
fn using_a_resource_after_give_is_unsafe() {
    unsafe_program(
        "resource Mark { code: i32 }\n\
         fn consume(give m: Mark) {}\n\
         fn f() {\n\
             let m = Mark { code: 1 }\n\
             consume(give m)\n\
             consume(give m)\n\
         }",
        "UseOfUninit",
    );
}

#[test]
fn giving_a_descriptor_is_rejected() {
    reject(
        "resource Mark { code: i32 }\n\
         fn consume(give m: Mark) {}\n\
         fn f(m: Mark) { consume(give m) }",
        "E-GIVE-DESCRIPTOR",
    );
}

#[test]
fn extern_c_without_unsafe_is_rejected() {
    reject(
        "extern \"C\" fn putchar(code: i32) -> i32;\n\
         fn f() { putchar(65) }",
        "E-FFI-UNSAFE",
    );
}

#[test]
fn duplicate_impl_drop_is_rejected() {
    reject(
        "resource Mark { code: i32 }\n\
         impl Drop for Mark { fn drop(self: Self) {} }\n\
         impl Drop for Mark { fn drop(self: Self) {} }",
        "E-DROP-DUPLICATE",
    );
}

#[test]
fn error_union_try_propagates_and_matches_arms() {
    admit(
        "fn fail_or(ok: bool) -> void!i32 {\n\
             if ok { return 42 }\n\
             return ()\n\
         }\n\
         fn caller(ok: bool) -> void!i32 {\n\
             let v = fail_or(ok)?\n\
             return v + 1\n\
         }\n\
         fn inspect(ok: bool) -> i32 {\n\
             match caller(ok) {\n\
                 .ok(x) => { return x }\n\
                 .err(_) => { return -1 }\n\
             }\n\
         }",
    );
}

#[test]
fn ambiguous_error_union_injection_is_rejected() {
    reject(
        "fn bad() -> i32!i32 { return 7 }",
        "E-ERROR-INJECTION-AMBIGUOUS",
    );
}

#[test]
fn try_outside_error_union_return_is_rejected() {
    reject(
        "fn fail_or() -> void!i32 { return () }\n\
         fn bad() -> i32 { return fail_or()? }",
        "E-TRY-RETURN-TYPE",
    );
}

// ---- compile-time evaluation (decision A2: verified body IR + zil-interp) --

struct InterpCt;

fn value_to_ct(v: zil_interp::Value) -> Result<zil_typeck::CtValue, &'static str> {
    use zil_interp::Value;
    use zil_typeck::CtValue;
    Ok(match v {
        Value::Unit => CtValue::Unit,
        Value::Bool(b) => CtValue::Bool(b),
        Value::Int { ty, bits } => CtValue::Int { ty, bits },
        Value::Float { ty, bits } => CtValue::Float { ty, bits },
        Value::Aggregate {
            variant, fields, ..
        } => CtValue::Aggregate {
            variant,
            fields: fields
                .into_iter()
                .map(value_to_ct)
                .collect::<Result<Vec<_>, _>>()?,
        },
        Value::ErrorUnion { ok, payload, .. } => CtValue::ErrorUnion {
            ok,
            payload: Box::new(value_to_ct(*payload)?),
        },
        Value::FnRef(_) => return Err("function value cannot cross CT boundary"),
    })
}

impl zil_typeck::ConstEvaluator for InterpCt {
    fn eval(
        &mut self,
        module: zil_ir::body::Module,
        entry: zil_ir::body::FnId,
        span: zil_base::Span,
    ) -> Result<zil_typeck::CtValue, zil_base::Diagnostic> {
        let cert = zil_solve::solve_module(&module, Budget { steps: STEPS }).map_err(|e| {
            zil_base::Diagnostic::error(
                zil_base::Phase::Ct,
                "E-CT-SAFETY",
                Some(span),
                format!("compile-time expression failed safety check: {e:?}"),
            )
        })?;
        let verified = zil_check::check_module(module, &cert, CheckBudget { steps: STEPS })
            .map_err(|e| {
                zil_base::Diagnostic::error(
                    zil_base::Phase::Ct,
                    "E-CT-SAFETY",
                    Some(span),
                    format!("compile-time expression failed certificate check: {e:?}"),
                )
            })?;
        let budget = zil_interp::EvalBudget {
            steps: 100_000,
            call_depth: 64,
        };
        let val =
            zil_interp::evaluate(&verified, entry, Vec::new(), budget).map_err(|e| match e {
                zil_interp::EvalError::ResourceLimit { .. } => {
                    zil_base::Diagnostic::limit(zil_base::Phase::Ct, "CT budget", Some(span))
                }
                other => zil_base::Diagnostic::error(
                    zil_base::Phase::Ct,
                    "E-CT-EVAL",
                    Some(span),
                    format!("compile-time evaluation failed: {other:?}"),
                ),
            })?;
        value_to_ct(val).map_err(|msg| {
            zil_base::Diagnostic::error(zil_base::Phase::Ct, "E-CT-EVAL", Some(span), msg)
        })
    }
}

#[test]
fn complete_domains_full_with_ct_evaluates_to_zero() {
    let text = include_str!("../../../tests/integers/complete-domains.zil");
    let mut ct = InterpCt;
    let lowered = zil_typeck::lower_source_with_ct("complete-domains.zil", text, Some(&mut ct))
        .expect("full complete-domains.zil should lower with CT");
    let result_id = lowered
        .roots
        .iter()
        .find(|(n, _)| n == "result")
        .map(|(_, id)| *id)
        .expect("result function");
    let cert = solve(&lowered.module);
    let verified = zil_check::check_module(lowered.module, &cert, CheckBudget { steps: STEPS })
        .expect("complete-domains.zil must pass checker");
    let out = zil_interp::evaluate(
        &verified,
        result_id,
        Vec::new(),
        zil_interp::EvalBudget {
            steps: 100_000,
            call_depth: 64,
        },
    )
    .expect("complete-domains.zil result() must execute cleanly");
    assert_eq!(
        out,
        zil_interp::Value::Int {
            ty: zil_ir::body::IntTy::I32,
            bits: 0,
        }
    );
}

#[test]
fn ct_eval_cannot_capture_runtime_local() {
    let mut ct = InterpCt;
    let err = zil_typeck::lower_source_with_ct(
        "ct_local.zil",
        "fn f(x: i32) -> i32 { return #(x + 1) }",
        Some(&mut ct),
    )
    .unwrap_err();
    assert!(
        err.iter().any(|d| d.code == "E-NAME-UNRESOLVED"),
        "expected E-NAME-UNRESOLVED, got {err:?}"
    );
}

#[test]
fn ct_eval_trap_is_rejected_at_compile_time() {
    let mut ct = InterpCt;
    let err = zil_typeck::lower_source_with_ct(
        "ct_trap.zil",
        "fn f() -> i8 { return #(127i8 + 1i8) }",
        Some(&mut ct),
    )
    .unwrap_err();
    assert!(
        err.iter().any(|d| d.code == "E-CT-EVAL"),
        "expected E-CT-EVAL, got {err:?}"
    );
}

// ---- multi-module imports, visibility, function pointers -------------------

#[test]
fn multi_module_stream_support_modules_are_admitted() {
    let lowered = zil_typeck::lower_sources(
        &[
            (
                "io",
                include_str!("../../../examples/mainline/stream/io.zil"),
            ),
            (
                "model",
                include_str!("../../../examples/mainline/stream/model.zil"),
            ),
            (
                "tokens",
                include_str!("../../../examples/mainline/stream/tokens.zil"),
            ),
            (
                "transform",
                include_str!("../../../examples/mainline/stream/transform.zil"),
            ),
        ],
        None,
    )
    .expect("stream support modules lower");
    assert_eq!(format!("{:?}", verdict(lowered.module)), "Accepted");
}

#[test]
fn private_item_in_imported_module_is_not_visible() {
    let err = zil_typeck::lower_sources(
        &[
            (
                "helper",
                "fn secret() -> i32 { return 1 }\npub fn ok() -> i32 { return 2 }",
            ),
            (
                "main",
                "import helper\nfn main() -> i32 { return helper.secret() }",
            ),
        ],
        None,
    )
    .unwrap_err();
    assert!(
        err.iter().any(|d| d.code == "E-NAME-UNRESOLVED"),
        "expected E-NAME-UNRESOLVED, got {err:?}"
    );
}

#[test]
fn function_pointer_array_and_indirect_call_execute_in_interp() {
    let lowered = zil_typeck::lower_sources(
        &[
            (
                "transform",
                "pub fn double(v: i32) -> i32 => v * 2\npub fn offset(v: i32) -> i32 => v + 11",
            ),
            (
                "main",
                "import transform\n\
                 fn run(i: i32, x: i32) -> i32 {\n\
                     let cbs: [fn(i32) -> i32; 2] = [transform.double, transform.offset]\n\
                     let f = cbs[i % 2]\n\
                     return f(x)\n\
                 }",
            ),
        ],
        None,
    )
    .expect("fn ptr program lowers");
    let run_id = lowered
        .roots
        .iter()
        .find(|(n, _)| n == "main.run")
        .map(|(_, id)| *id)
        .expect("main.run");
    let cert = solve(&lowered.module);
    let verified = zil_check::check_module(lowered.module, &cert, CheckBudget { steps: STEPS })
        .expect("verified");
    let budget = zil_interp::EvalBudget {
        steps: 10_000,
        call_depth: 16,
    };
    let r0 = zil_interp::evaluate(
        &verified,
        run_id,
        vec![
            zil_interp::Value::Int {
                ty: zil_ir::body::IntTy::I32,
                bits: 0,
            },
            zil_interp::Value::Int {
                ty: zil_ir::body::IntTy::I32,
                bits: 21,
            },
        ],
        budget,
    )
    .expect("run(0, 21)");
    assert_eq!(
        r0,
        zil_interp::Value::Int {
            ty: zil_ir::body::IntTy::I32,
            bits: 42,
        }
    );
    let r1 = zil_interp::evaluate(
        &verified,
        run_id,
        vec![
            zil_interp::Value::Int {
                ty: zil_ir::body::IntTy::I32,
                bits: 1,
            },
            zil_interp::Value::Int {
                ty: zil_ir::body::IntTy::I32,
                bits: 21,
            },
        ],
        budget,
    )
    .expect("run(1, 21)");
    assert_eq!(
        r1,
        zil_interp::Value::Int {
            ty: zil_ir::body::IntTy::I32,
            bits: 32,
        }
    );
}

// ---- closures (Fn / FnMut) and full examples/mainline/stream ---------------

#[test]
fn full_mainline_stream_example_is_admitted_end_to_end() {
    let lowered = zil_typeck::lower_sources(
        &[
            (
                "io",
                include_str!("../../../examples/mainline/stream/io.zil"),
            ),
            (
                "model",
                include_str!("../../../examples/mainline/stream/model.zil"),
            ),
            (
                "tokens",
                include_str!("../../../examples/mainline/stream/tokens.zil"),
            ),
            (
                "transform",
                include_str!("../../../examples/mainline/stream/transform.zil"),
            ),
            (
                "main",
                include_str!("../../../examples/mainline/stream/main.zil"),
            ),
        ],
        None,
    )
    .expect("full mainline/stream example lowers");
    assert_eq!(format!("{:?}", verdict(lowered.module)), "Accepted");
}

#[test]
fn fn_mut_closure_mutating_captured_local_executes_in_interp() {
    let lowered = zil_typeck::lower_source(
        "closure_mut.zil",
        "fn run() -> i32 {\n\
             var total: i32 = 10\n\
             let scale: i32 = 3\n\
             var add = (x: i32) => {\n\
                 total += x * scale\n\
             }\n\
             add(4)\n\
             add(5)\n\
             return total\n\
         }",
    )
    .expect("closure lowers");
    let run_id = lowered
        .roots
        .iter()
        .find(|(n, _)| n == "run")
        .map(|(_, id)| *id)
        .expect("run");
    let cert = solve(&lowered.module);
    let verified = zil_check::check_module(lowered.module, &cert, CheckBudget { steps: STEPS })
        .expect("verified");
    let out = zil_interp::evaluate(
        &verified,
        run_id,
        Vec::new(),
        zil_interp::EvalBudget {
            steps: 10_000,
            call_depth: 16,
        },
    )
    .expect("executes");
    assert_eq!(
        out,
        zil_interp::Value::Int {
            ty: zil_ir::body::IntTy::I32,
            bits: 37,
        }
    );
}
