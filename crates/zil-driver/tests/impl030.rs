//! Real source FnMut state must persist in its actual environment position.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{CandidatePlan, InstructionKind, TypedModule};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil-030-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(s: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "impl030.zil", s)
}
fn run(p: &Path, want: i64) {
    let o = Command::new(p).output().unwrap();
    assert!(o.status.success(), "{o:?}");
    assert_eq!(String::from_utf8(o.stdout).unwrap(), format!("{want}\n"));
    assert!(o.stderr.is_empty());
}
fn native(s: &str, want: i64) -> String {
    let t = Temp::new();
    let r = compile_source(
        &source(s),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("answer".into()),
            output: Some(t.0.join("answer")),
            emit_dir: Some(t.0.join("ir")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    run(&r.artifacts["executable"], want);
    let ir = fs::read_to_string(&r.artifacts["llvm"]).unwrap();
    assert!(!ir.contains("@malloc"));
    let o = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&r.artifacts["llvm"])
        .arg("-o")
        .arg(t.0.join("optimized"))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    run(&t.0.join("optimized"), want);
    ir
}
fn reject(s: &str, category: ResultCategory, code: &str) {
    let r = compile_source(&source(s), &CompileOptions::default());
    assert_eq!(r.status, category, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("llvm"));
}
fn raw(s: &str) -> (TypedModule, CandidatePlan) {
    let input = source(s);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let m = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    (m, p)
}
const COUNTER: &str = "fn answer()->i64 { var n:i64=0; var f=move () => { n+=1; n }; let a=f(); let b=f(); var g=f; let c=g(); let d=f(); return a*1000+b*100+c*10+d+n }";
#[test]
fn mutable_counter_updates_and_independent_copy() {
    let ir = native(COUNTER, 1233);
    assert!(ir.contains("ptr %env"));
    assert!(ir.contains("ptr %local."));
}
#[test]
fn nested_record_array_tuple_environment_and_aggregate_result() {
    native(include_str!("../../../examples/impl030/main.zil"), 123060);
}
#[test]
fn inferred_array_capture_and_projected_assignment() {
    native(
        "fn answer()->i32 { var data=[1,2,3]; var f=move (index,delta) => { data[index]+=delta; data[index] }; let a=f(0,4); let b=f(0,2); var g=f; let c=g(0,1); let d=f(0,1); return a*1000+b*100+c*10+d }",
        5788,
    );
}
#[test]
fn move_aggregate_readonly_and_projected_parameter_inference() {
    native(
        "fn answer()->i64 { let pair:(i64,i64)=(5000000000,2); let read=move () => pair.0+pair.1; let add=p=>p.0+p.1; let input:(i64,i64)=(20,22); return read()+add(input) }",
        5000000044,
    );
}
#[test]
fn mutable_bool_unit_and_temporary_environments() {
    native(
        "fn answer()->i32 { var flag=false; var toggle=move () => { flag=!flag; flag }; let a=toggle(); let b=toggle(); var u=(); var unit=move () => { u=(); 2 }; var n=4; let t=(move () => { n+=1; n })(); if a && !b { return t+unit()+unit() } return 0 }",
        9,
    );
}
#[test]
fn mixed_address_and_owned_state_keep_hold_until_actual_last_call() {
    native(
        "fn answer()->i32 { var x=10; let p=&x; var n=0; var f=move () => { n+=1; *p+n }; let a=f(); let b=f(); x=20; return a*100+b*10+x }",
        1240,
    );
    reject(
        "fn answer()->i32 { var x=10; let p=&x; var n=0; var f=move () => { n+=1; *p+n }; let a=f(); x=20; return a+f() }",
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
    );
}
#[test]
fn frozen_closure_and_frozen_capture_are_rejected() {
    reject(
        "fn answer()->i32 { var n=0; let f=move () => { n+=1; n }; return f() }",
        ResultCategory::SafeReject,
        "E-CLOSURE-MUTABILITY",
    );
    reject(
        "fn answer()->i32 { let n=0; var f=move () => { n+=1; n }; return f() }",
        ResultCategory::SafeReject,
        "E-IMMUTABLE-ASSIGNMENT",
    );
}
#[test]
fn default_writes_update_external_state_and_nested_address_capture_stays_closed() {
    native(
        "fn answer()->i32 { var n=0; var f=() => { n+=1; n }; let a=f(); return a+n }",
        2,
    );
    let text = "fn answer()->i32 { let x=1; let y=2; var p=&x; let q=&y; var f=move () => { p=q; *p }; return f() }";
    let r = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::Unsupported, "{}", r.to_json());
}
#[test]
fn raw_mutability_and_by_value_mut_call_forgery_fail() {
    let (m, p) = raw(COUNTER);
    let limits = Limits::default();
    let target = TargetProfile::default();
    assert!(zil_verify::verify_module(m.clone(), p.clone(), &target, &limits).is_ok());
    let call = m.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::CallClosureMut { .. }))
        .unwrap();
    let InstructionKind::CallClosureMut { callee, .. } = call.kind else {
        unreachable!()
    };
    let mut bad = m.clone();
    bad.functions[0].locals[callee.0 as usize].mutable = false;
    let mut plan = p.clone();
    plan.module_fingerprint = bad.fingerprint();
    assert_eq!(
        zil_verify::verify_module(bad, plan, &target, &limits).unwrap_err()[0].code,
        "E-VERIFY-MUTABILITY"
    );
    let value = m.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind,InstructionKind::Load(l) if l==callee)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let mut bad = m.clone();
    let call = bad.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::CallClosureMut { .. }))
        .unwrap();
    let InstructionKind::CallClosureMut { arguments, .. } = &call.kind else {
        unreachable!()
    };
    call.kind = InstructionKind::CallClosure {
        callee: value,
        arguments: arguments.clone(),
    };
    let mut plan = p.clone();
    plan.module_fingerprint = bad.fingerprint();
    assert_eq!(
        zil_verify::verify_module(bad, plan, &target, &limits).unwrap_err()[0].code,
        "E-VERIFY-CLOSURE-CALL"
    );
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &limits).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(decoded, (m, p));
}
#[test]
fn raw_capture_write_cannot_use_readonly_entry_or_invalid_field() {
    let (m, p) = raw(COUNTER);
    for mode in 0..2 {
        let mut bad = m.clone();
        if mode == 0 {
            bad.closures[0].kind = zil_ir::CallableKind::Fn;
        } else {
            for i in bad
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.instructions)
            {
                if let InstructionKind::WriteCapture { index, .. } = &mut i.kind {
                    *index = u32::MAX;
                    break;
                }
            }
        }
        let mut plan = p.clone();
        plan.module_fingerprint = bad.fingerprint();
        assert!(
            zil_verify::verify_module(bad, plan, &TargetProfile::default(), &Limits::default())
                .is_err()
        );
    }
}

#[test]
fn compound_projection_indices_evaluate_once_before_rhs() {
    native(
        "fn answer()->i32 { var count=0; var index=move () => { count+=1; count-1 }; var data=[[1,2],[3,4]]; data[index()][index()] += 8; let calls=index(); return data[0][1]*10+calls }",
        102,
    );
}

#[test]
fn aggregate_constraint_expansion_obeys_selected_work_and_depth() {
    let mut text = String::from("fn answer()->i32 { let x0=1;");
    for i in 1..=16 {
        text.push_str(&format!("let x{i}=(x{},x{});", i - 1, i - 1));
    }
    text.push_str("let f=move () => x16; return 0 }");
    let input = source(&text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    for limits in [
        Limits {
            max_analysis_steps: 2000,
            ..Limits::default()
        },
        Limits {
            max_call_depth: 12,
            ..Limits::default()
        },
    ] {
        let error = match zil_types::infer_source_closures(&core, &limits) {
            Err(e) => e,
            Ok(_) => panic!(
                "aggregate limit was not applied: work={},depth={}",
                limits.max_analysis_steps, limits.max_call_depth
            ),
        };
        assert_eq!(
            error[0].category,
            ResultCategory::ResourceLimit,
            "{error:?}"
        );
    }
}
#[test]
fn aggregate_projection_constraints_reject_conflicts_and_bad_fields() {
    for text in [
        "fn answer()->i32 { let f=p=>p.0; f((1,2)); return f((true,false)) }",
        "fn answer()->i32 { let f=p=>p.2; return f((1,2)) }",
    ] {
        let r = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    }
}
#[test]
fn raw_capture_address_cannot_be_overwritten() {
    let (mut m, mut p) = raw(
        "fn answer()->i32 { let x=1; let p=&x; var n=0; var f=move () => { n+=1; *p+n }; return f() }",
    );
    let d = &m.closures[0];
    let field = d
        .captures
        .iter()
        .position(|t| matches!(t, zil_ir::Type::Address { .. }))
        .unwrap() as u32;
    let entry = &mut m.functions[d.function.0 as usize];
    let value = entry
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind,InstructionKind::ReadCapture{index} if index==field)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    // Put the forged write after the address definition so dominance is valid;
    // rejection must come from capability/type admission, not a missing operand.
    let block = &mut entry.blocks[0];
    let at = block
        .instructions
        .iter()
        .position(|i| i.result == Some(value))
        .unwrap()
        + 1;
    let span = block.instructions[at - 1].span;
    block.instructions.insert(
        at,
        zil_ir::Instruction {
            result: None,
            kind: InstructionKind::WriteCapture {
                index: field,
                value,
            },
            span,
        },
    );
    p.module_fingerprint = m.fingerprint();
    let error =
        zil_verify::verify_module(m, p, &TargetProfile::default(), &Limits::default()).unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-CAPTURE");
}
