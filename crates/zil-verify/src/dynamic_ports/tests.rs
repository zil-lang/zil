//! The bytes were produced from context-read.zil by real Expand+Types. The
//! enclosed plan is deliberately empty; these tests prove independent raw
//! header validation, not Analyze, Verify-module, or native execution.
use super::*;
fn candidate() -> TypedModule {
    zil_ir::wire::decode_module_candidate(
        include_bytes!("../../tests/fixtures/dyn-context/context-read.typed.wire"),
        &Limits::default(),
    )
    .unwrap()
    .0
}
fn headers(module: &TypedModule) -> Result<(), Diagnostic> {
    let limits = Limits::default();
    let mut b = Budget::new(&limits);
    crate::stored_flow::inventory(module, &limits, &mut b)?;
    crate::dynamic_dispatch::inventory(module, &mut b)
}
#[test]
fn source_typed_context_provider_header_passes_independent_raw_and_wire_checks() {
    let module = candidate();
    headers(&module).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(
        &module,
        &CandidatePlan::empty_for(&module),
        &Limits::default(),
    )
    .unwrap();
    let (module, _) = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
    headers(&module).unwrap();
}
#[test]
fn source_typed_context_wrong_root_empty_omission_and_class_rebindings_reject() {
    let module = candidate();
    for attack in 0..5 {
        let mut wrong = module.clone();
        let table = &mut wrong.dyn_vtables[0];
        match attack {
            0 => table.ports[0].sources[0].base = StorageBase::Parameter(0),
            1 => table.ports[0].sources.clear(),
            2 => table.ports.clear(),
            3 => table.self_classes[0] = StorageBinder::Fixed(1),
            _ => {
                wrong
                    .dyn_declarations
                    .iter_mut()
                    .find(|d| d.kind == DynDeclarationKind::Impl)
                    .unwrap()
                    .implemented_trait = None
            }
        }
        assert!(headers(&wrong).is_err(), "raw attack {attack}");
        let bytes = zil_ir::wire::encode_module_candidate(
            &wrong,
            &CandidatePlan::empty_for(&wrong),
            &Limits::default(),
        )
        .unwrap();
        let (wrong, _) = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
        assert!(headers(&wrong).is_err(), "Wire attack {attack}");
    }
}

#[test]
fn actual_context_read_candidate_reaches_independent_memory_kernel() {
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("../../tests/fixtures/dyn-context/context-read.candidate.wire"),
        &limits,
    )
    .unwrap();
    let target = module.target.clone();
    let result = crate::verify_module(module, plan, &target, &limits);
    assert!(result.is_ok(), "{result:?}");
}
