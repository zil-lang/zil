//! Untrusted bytes produced by actual Source, Types and Analyze. Verify must
//! independently reconstruct every mutable port, receiver and parent loan.
use zil_base::Limits;

#[test]
fn actual_exclusive_context_source_preserves_writes_and_releases_after_cleanup() {
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/context-exclusive.candidate.wire"),
        &limits,
    )
    .unwrap();
    let target = module.target.clone();
    let result = zil_verify::verify_module(module, plan, &target, &limits);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn actual_exclusive_context_factory_transports_complete_argument_ancestry() {
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/context-exclusive-factory.candidate.wire"),
        &limits,
    )
    .unwrap();
    let target = module.target.clone();
    let result = zil_verify::verify_module(module, plan, &target, &limits);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn real_factory_original_argument_ssa_write_cannot_delegate_its_retained_child() {
    let limits = Limits::default();
    let bytes =
        include_bytes!("fixtures/dyn-context/context-exclusive-factory-old-argument-write.wire");
    let (module, plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    let error = zil_verify::verify_module(module, plan, &target, &limits).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.message.contains("live incompatible loan")),
        "{error:?}"
    );
}
