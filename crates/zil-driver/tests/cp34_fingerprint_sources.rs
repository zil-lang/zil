//! Real parsed/expanded sources bind the current identity to the frozen encoder.
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompilationSession, CompileOptions, Stage};
use zil_ir::{CoreProgram, TypedModule};

// The v27/v28/v29/v30 oracles remain archived byte-for-byte; current source uses v31.
#[path = "../../zil-ir/tests/support/cp34_legacy_v31.rs"]
mod legacy;

fn check_identity_and_verify(module: TypedModule) {
    let old = Fingerprint::of(&legacy::bytes(&module));
    assert_eq!(module.fingerprint(), old);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    assert_eq!(plan.module_fingerprint, old);
    let target = module.target.clone();
    let verified = zil_verify::verify_module(module, plan, &target, &limits).unwrap();
    assert_eq!(verified.fingerprint(), old);
}

#[test]
fn source_modules_match_frozen_identity_and_each_request_verifies() {
    let texts = [
        "fn main()->i32=>7",
        "// Unicode λ changes positions\nfn main()->i32 { let x:i32=11; return x+2; }",
        "fn main()->i32 { let f=(x)=>x; return f(7); }",
        "fn main()->i32 { defer {}; return 7; }",
        "struct Cell { value:i32 }\nfn get(c:Cell)->i32=>c.value\nfn main()->i32=>0",
    ];
    let mut session = CompilationSession::default();
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    for text in texts.into_iter().chain([texts[0], texts[0]]) {
        let source = SourceUnit::new(FileId(13), ModuleId(17), "source.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &parsed).unwrap();
        let module = zil_types::check(&core, &options.target, &options.limits).unwrap();
        check_identity_and_verify(module);
        let report = session.compile_source(&source, &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{:?}",
            report.diagnostics
        );
        assert_eq!(report.phase, Phase::Verify);
        assert_eq!(report.verification_runs, 1);
        assert_eq!(session.active_query_count(), 0);
    }
}

#[test]
fn actual_project_typed_output_matches_the_frozen_encoder() {
    let texts = [
        "fn main()->i32=>3",
        "fn leaf(x:i32)->i32=>x+7",
        "struct Cell { value:i32 }\nfn read(c:Cell)->i32=>c.value",
    ];
    let modules = texts
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let source = SourceUnit::new(
                FileId(i as u32),
                ModuleId(i as u32),
                format!("m{i}.zil"),
                text,
            );
            let parsed = zil_syntax::parse(&source).into_result().unwrap();
            zil_expand::expand_program_module(&source, &parsed, vec![])
                .unwrap()
                .0
        })
        .collect();
    let core = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"three source modules"),
        modules,
    };
    let typed =
        zil_types::check_program(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    check_identity_and_verify(typed.module);
}
