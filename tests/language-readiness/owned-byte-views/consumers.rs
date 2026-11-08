//! Actual owned ByteBuffer payload views beside stored address loans.
use super::*;
use zil_base::TargetProfile;
use zil_ir::{CandidatePlan, InstructionKind, TypedModule};

const LOCAL: &str = include_str!("local-field.zil");

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "owned-byte-view.zil", text)
}

fn typed(text: &str) -> TypedModule {
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits(&input, &syntax, &Limits::default())
        .unwrap()
        .core;
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

fn control(module: &TypedModule) -> CandidatePlan {
    let limits = Limits::default();
    let plan = zil_analyze::analyze(module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &limits).unwrap();
    plan
}

#[test]
fn actual_owned_byte_view_reads_the_payload_and_ends_before_relocation() {
    let report = compile_source(&source(LOCAL), &CompileOptions::default());
    println!("owned byte view: {}", report.to_json());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    control(&typed(LOCAL));
}

#[test]
fn actual_owned_byte_view_survives_index_and_shared_reborrow() {
    let module = typed(LOCAL);
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    assert!(
        main.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                InstructionKind::ByteBufferBorrow { mutable: false, .. }
            ))
    );
    assert!(
        main.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, InstructionKind::AddressIndex { .. }))
    );
    control(&module);
}
