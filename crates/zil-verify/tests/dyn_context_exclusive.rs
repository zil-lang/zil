//! Untrusted bytes produced by actual Source, Types and Analyze. Verify must
//! independently reconstruct every mutable port, receiver and parent loan.
use zil_base::Limits;

fn positive(bytes: &[u8]) {
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    let result = zil_verify::verify_module(module, plan, &target, &limits);
    assert!(result.is_ok(), "raw: {result:?}");
    let result = zil_verify::verify_module_bytes(bytes, &target, &limits);
    assert!(result.is_ok(), "Wire: {result:?}");
}

#[test]
fn actual_exclusive_context_source_preserves_writes_and_releases_after_cleanup() {
    positive(include_bytes!(
        "fixtures/dyn-context/context-exclusive.candidate.wire"
    ));
}

#[test]
fn actual_exclusive_context_factory_transports_complete_argument_ancestry() {
    positive(include_bytes!(
        "fixtures/dyn-context/context-exclusive-factory.candidate.wire"
    ));
}

#[test]
fn real_factory_original_argument_ssa_write_cannot_delegate_its_retained_child() {
    let limits = Limits::default();
    let bytes =
        include_bytes!("fixtures/dyn-context/context-exclusive-factory-old-argument-write.wire");
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    let creator = module
        .functions
        .iter()
        .find(|f| f.name == "create")
        .unwrap()
        .id;
    let mut witness = None;
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (call, instruction) in block.instructions.iter().enumerate() {
                let zil_ir::InstructionKind::Call {
                    function,
                    arguments,
                } = &instruction.kind
                else {
                    continue;
                };
                if *function != creator {
                    continue;
                }
                let argument = arguments[0];
                for (write, instruction) in block.instructions.iter().enumerate().skip(call + 1) {
                    if matches!(instruction.kind, zil_ir::InstructionKind::WriteAddress { address, .. } if address == argument)
                    {
                        witness = Some((f, b, write));
                        break;
                    }
                }
            }
        }
    }
    let (f, b, write) =
        witness.expect("attack must reuse exactly Call.arguments[0], not its ancestor SSA");
    let error =
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.code == "E-VERIFY-STORED" && d.message.contains("live incompatible loan")),
        "{error:?}"
    );
    let error = zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.code == "E-VERIFY-STORED" && d.message.contains("live incompatible loan")),
        "Wire: {error:?}"
    );
    // Only remove the conflicting write. All actual source, header, constructor
    // and SSA definitions (including its constant operand) remain unchanged.
    module.functions[f].blocks[b].instructions.remove(write);
    plan.module_fingerprint = module.fingerprint();
    let result = zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits);
    assert!(
        result.is_ok(),
        "same candidate without the write: {result:?}"
    );
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}
