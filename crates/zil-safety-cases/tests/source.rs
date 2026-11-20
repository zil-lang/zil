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
