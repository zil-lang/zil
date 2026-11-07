//! Generation is only syntax. Runtime debt and CT execution remain independently checked.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, compile_source};
use zil_ir::{Constant, CtProgram};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "boundary219.zil", text)
}

#[test]
fn selected_ownership_and_borrow_errors_have_the_same_ordinary_gate() {
    for (plain, selected) in [
        (
            "fn eat(give b:ByteBuffer)->i64=>bytes_len(b);fn main()->i32{let b=bytes_new();eat(give b);eat(give b);return 0}",
            "fn eat(give b:ByteBuffer)->i64=>bytes_len(b);fn main()->i32{#if(true){let b=bytes_new()}eat(give b);eat(give b);return 0}",
        ),
        (
            "fn bad()->&[u8]{let b=bytes_new();bytes_reserve(b,1);return bytes_view(b)}fn main()->i32=>0",
            "fn bad()->&[u8]{#if(true){let b=bytes_new()}bytes_reserve(b,1);return bytes_view(b)}fn main()->i32=>0",
        ),
    ] {
        let ordinary = compile_source(&source(plain), &CompileOptions::default());
        let generated = compile_source(&source(selected), &CompileOptions::default());
        assert_ne!(
            ordinary.status,
            ResultCategory::Success,
            "{}",
            ordinary.to_json()
        );
        assert_eq!(generated.status, ordinary.status, "{}", generated.to_json());
        assert_eq!(generated.phase, ordinary.phase, "{}", generated.to_json());
        assert!(matches!(
            generated.phase,
            Phase::Types | Phase::Analyze | Phase::Verify
        ));
    }
}

#[test]
fn unrelated_runtime_generic_bodies_do_not_become_condition_dependencies() {
    for text in [
        "fn id<T:Copy>(x:T)->T=>x;fn runtime()->i32=>id::<i32>(7);fn main()->i32{#if(true){return runtime()-7}else{return absent}}",
        "struct Pair{a:i32;b:i32}fn runtime()->i32{let p=Pair{a:3,b:4};return p.a+p.b}fn main()->i32{#if(true){return runtime()-7}else{return absent}}",
        "#if(true){struct Pair{a:i32;b:i32}}fn runtime()->i32{let p=Pair{a:3,b:4};return p.a+p.b}fn main()->i32{#if(true){return runtime()-7}else{return absent}}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

fn raw(text: &str) -> CtProgram {
    let source = source(text);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let parsed = zil_syntax::parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let core = zil_expand::expand_with_map_limits(&source, &parsed.module, &limits)
        .unwrap()
        .core;
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let entry = module
        .functions
        .iter()
        .find(|f| f.name == "query")
        .unwrap()
        .id;
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    CtProgram {
        module,
        plan,
        entry,
        max_steps: limits.max_ct_steps,
        max_call_depth: limits.max_call_depth,
        max_live_slots: limits.max_locals + limits.max_values,
    }
}

#[test]
fn raw_runtime_resource_candidate_cannot_gain_ct_authority() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let accepted =
        zil_verify::verify_ct_program(raw("fn query()->bool=>true"), &target, &limits).unwrap();
    assert_eq!(
        zil_ct::evaluate_program(&accepted).unwrap().value,
        zil_ct::CompileTimeValue::Scalar(Constant::Bool(true))
    );
    let rejected = zil_verify::verify_ct_program(
        raw("fn query()->bool{let data=bytes_new();return bytes_len(data)==0}"),
        &target,
        &limits,
    )
    .unwrap_err();
    assert!(
        rejected.iter().any(|d| d.code == "E-CT-FORBIDDEN"),
        "{rejected:?}"
    );
}
