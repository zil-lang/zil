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
    let bytes = include_bytes!("fixtures/dyn-context/context-exclusive-factory.candidate.wire");
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
                if let zil_ir::InstructionKind::Call {
                    function,
                    arguments,
                } = &instruction.kind
                    && *function == creator
                {
                    witness = Some((f, b, call, arguments[0], instruction.span));
                }
            }
        }
    }
    let (f, b, call, argument, span) = witness.expect("actual Source must call create");
    assert!(matches!(
        module.functions[f].values[argument.0 as usize],
        zil_ir::Type::Address { mutable: true, .. }
    ));
    let value = zil_ir::ValueId(module.functions[f].values.len() as u32);
    module.functions[f].values.push(zil_ir::Type::I32);
    let function_id = module.functions[f].id;
    plan.functions
        .iter_mut()
        .find(|p| p.function == function_id)
        .unwrap()
        .holds
        .push(zil_ir::ValueHold {
            value,
            roots: vec![],
        });
    module.functions[f].blocks[b].instructions.insert(
        call + 1,
        zil_ir::Instruction {
            result: Some(value),
            kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(7)),
            span,
        },
    );
    let write = call + 2;
    module.functions[f].blocks[b].instructions.insert(
        write,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::WriteAddress {
                address: argument,
                value,
            },
            span,
        },
    );
    assert!(
        matches!(module.functions[f].blocks[b].instructions[write].kind,
        zil_ir::InstructionKind::WriteAddress { address, .. } if address == argument)
    );
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let error =
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap_err();
    assert!(
        error
            .iter()
            .any(|d| d.code == "E-VERIFY-STORED" && d.message.contains("live incompatible loan")),
        "{error:?}"
    );
    let error = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
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
