//! A real mutable Holder's shared leaf can be copied independently. An explicit
//! bounded reborrow additionally retains its exact head, without retagging data.
use zil_base::Limits;
use zil_ir::{ContractProjection, PlaceProjection, StorageBase, StorageSource};
#[test]
fn actual_holder_leaf_factory_preserves_data_class_and_optional_bounded_head() {
    let limits = Limits::default();
    let bytes = include_bytes!("fixtures/dyn-context/context-holder-leaf.candidate.wire");
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(bytes, &target, &limits).unwrap();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let StorageSource::Exact(source) = module.functions[create]
        .retained_return_sources
        .as_ref()
        .unwrap()[0]
        .sources[0]
        .clone()
    else {
        panic!("actual Holder source must select its stored leaf")
    };
    assert_eq!(source.base, StorageBase::Parameter(0));
    assert_eq!(
        source.path,
        [ContractProjection::Place(PlaceProjection::Field(0))]
    );
    let original_life = module.functions[create].storage_life.clone();
    module.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources[0] = StorageSource::ReborrowedLeaf {
        source,
        head: zil_ir::StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        },
    };
    plan.module_fingerprint = module.fingerprint();
    // This is an explicit conservative raw operation over the same actual
    // Source body, data permission and signature, not a producer success claim.
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    assert_eq!(module.functions[create].storage_life, original_life);
    use zil_ir::{HoldRoot, Instruction, InstructionKind, ValueHold, ValueId};
    let creator = module.functions[create].id;
    let run = module
        .functions
        .iter()
        .position(|f| f.name == "run")
        .unwrap();
    let next = module.functions[run].parameters[1];
    let next_type = module.functions[run].locals[next.0 as usize].ty.clone();
    let mut site = None;
    for (block, body) in module.functions[run].blocks.iter().enumerate() {
        for (offset, i) in body.instructions.iter().enumerate() {
            if let InstructionKind::Call {
                function,
                arguments,
            } = &i.kind
                && *function == creator
            {
                site = Some((block, offset, arguments[0], i.span));
            }
        }
    }
    let (block, offset, head, span) = site.unwrap();
    let value = ValueId(module.functions[run].values.len() as u32);
    module.functions[run].values.push(next_type);
    module.functions[run].blocks[block].instructions.splice(
        offset + 1..offset + 1,
        [
            Instruction {
                result: Some(value),
                kind: InstructionKind::Load(next),
                span,
            },
            Instruction {
                result: None,
                kind: InstructionKind::StoreStoredBorrow {
                    storage: head,
                    path: vec![PlaceProjection::Field(0)],
                    value,
                },
                span,
            },
        ],
    );
    let id = module.functions[run].id;
    plan.functions
        .iter_mut()
        .find(|p| p.function == id)
        .unwrap()
        .holds
        .push(ValueHold {
            value,
            roots: vec![HoldRoot::Parameter(1)],
        });
    plan.module_fingerprint = module.fingerprint();
    for wire in [false, true] {
        let errors = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap_err()
        };
        assert!(
            errors.iter().any(
                |d| d.code == "E-VERIFY-STORED" && d.message.contains("live incompatible loan")
            ),
            "{wire}: {errors:?}"
        );
    }
    // Keep the identical replacement, actual head SSA, data roots and Types.
    // Copying a shared leaf with Exact may detach from this mutable holder;
    // the explicitly head-bounded ReborrowedLeaf above may not.
    let StorageSource::ReborrowedLeaf { source, .. } = module.functions[create]
        .retained_return_sources
        .as_ref()
        .unwrap()[0]
        .sources[0]
        .clone()
    else {
        unreachable!()
    };
    module.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources[0] = StorageSource::Exact(source);
    plan.module_fingerprint = module.fingerprint();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}
