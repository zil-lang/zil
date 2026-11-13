//! Real fat-slice getter inventory, with an independently written raw callback
//! cycle. The original trait, provider, slot and Self class maps stay intact.
use zil_base::Limits;
use zil_ir::*;

fn getter() -> (TypedModule, CandidatePlan) {
    wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/context-long-slice-getter.candidate.wire"),
        &Limits::default(),
    )
    .unwrap()
}
fn checked(module: &TypedModule, plan: &CandidatePlan) {
    let limits = Limits::default();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let bytes = wire::encode_module_candidate(module, plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap();
}
#[test]
fn real_original_slice_getter_and_context_callback_sources_are_verified() {
    let (module, plan) = getter();
    assert!(module.dyn_interfaces.iter().any(|interface| {
        interface.slots.iter().any(|slot| {
            slot.result
                == Type::Address {
                    mutable: false,
                    inner: Box::new(Type::Slice {
                        element: Box::new(Type::U8),
                    }),
                }
                && slot
                    .storage_life
                    .as_ref()
                    .unwrap()
                    .bindings
                    .iter()
                    .any(|row| {
                        row.position.base == StorageBase::Result
                            && row.binder == StorageBinder::Fixed(0)
                    })
        })
    }));
    checked(&module, &plan);
    let (module, plan) = wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/context-captured-owner.candidate.wire"),
        &Limits::default(),
    )
    .unwrap();
    checked(&module, &plan);
}

#[test]
fn real_fixed_capture_mutation_and_closed_owner_companion_keep_actual_domains() {
    for bytes in [
        include_bytes!("fixtures/dyn-context/context-captured-owner-mut.candidate.wire").as_slice(),
        include_bytes!("fixtures/dyn-context/closed-owner-and-borrowed-slice.candidate.wire")
            .as_slice(),
    ] {
        let (module, plan) = wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        assert_eq!(module.closures.len(), 1);
        assert_eq!(
            module.closures[0]
                .storage_life
                .as_ref()
                .unwrap()
                .fixed_binder_count,
            1
        );
        // The immutable saved slice's declared Fixed class remains present even
        // when this particular caller supplies a program-static literal.
        checked(&module, &plan);
    }
}

fn callback_source() -> (TypedModule, CandidatePlan) {
    wire::decode_module_candidate(
        include_bytes!("fixtures/dyn-context/static-callback-getter-cycle.candidate.wire"),
        &Limits::default(),
    )
    .unwrap()
}

/// Keep the real original trait, provider, callback definition, complete
/// signatures, CFG edges and branch conditions. Replace only one or both real
/// callback literal producers by the existing feedback function. The literal
/// passed to feedback is deliberately unused by create: it is in the canonical
/// upper, but cannot anchor the callback's actual Result dependency.
fn callback_cycle(literal_edge: bool) -> (TypedModule, CandidatePlan) {
    let (mut module, mut candidate) = callback_source();
    let feedback = module
        .functions
        .iter()
        .find(|f| f.name == "feedback")
        .unwrap()
        .id;
    let definition = &module.closures[0];
    assert!(definition.captures.is_empty());
    assert_eq!(definition.parameters, [Type::Bool]);
    let entry = definition.function;
    let function = &mut module.functions[entry.0 as usize];
    let parameter = function.parameters[0];
    let flag = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction.kind {
            InstructionKind::Load(local) if local == parameter => instruction.result,
            _ => None,
        })
        .unwrap();
    let plan = candidate
        .functions
        .iter_mut()
        .find(|plan| plan.function == entry)
        .unwrap();
    let mut literal = 0;
    for block in &mut function.blocks {
        let mut instructions = Vec::new();
        for instruction in &block.instructions {
            if matches!(instruction.kind, InstructionKind::StaticUtf8 { .. }) {
                literal += 1;
                if !literal_edge || literal != 1 {
                    let seed = ValueId(function.values.len() as u32);
                    function.values.push(function.return_type.clone());
                    let mut input = instruction.clone();
                    input.result = Some(seed);
                    input.kind = InstructionKind::StaticUtf8 {
                        bytes: b"unused".to_vec(),
                    };
                    instructions.push(input);
                    instructions.push(Instruction {
                        result: instruction.result,
                        kind: InstructionKind::Call {
                            function: feedback,
                            arguments: vec![seed, flag],
                        },
                        span: instruction.span,
                    });
                    plan.holds.push(ValueHold {
                        value: seed,
                        roots: vec![HoldRoot::Static],
                    });
                    continue;
                }
            }
            instructions.push(instruction.clone());
        }
        block.instructions = instructions;
    }
    assert_eq!(literal, 2);
    candidate.module_fingerprint = module.fingerprint();
    (module, candidate)
}

#[test]
fn original_constant_result_callback_source_is_verified() {
    let (module, plan) = callback_source();
    checked(&module, &plan);
}

#[test]
fn an_actual_parameter_result_does_not_inherit_an_unused_static_upper_obligation() {
    let (mut module, mut plan) = callback_source();
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.name == "create")
        .unwrap();
    let seed = function.parameters[0];
    let mut replaced = false;
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            if matches!(instruction.kind, InstructionKind::CallClosureShared { .. }) {
                instruction.kind = InstructionKind::Load(seed);
                replaced = true;
            }
        }
    }
    assert!(replaced);
    // Every actual borrowed result of create now comes from the real seed;
    // the extra Static upper is a category, not another actual dependency.
    for hold in &mut plan.functions[function.id.0 as usize].holds {
        for root in &mut hold.roots {
            if *root == HoldRoot::Static {
                *root = HoldRoot::Parameter(0);
            }
        }
        hold.roots.sort_unstable();
        hold.roots.dedup();
    }
    plan.module_fingerprint = module.fingerprint();
    checked(&module, &plan);
}

#[test]
fn context_canonical_upper_cannot_seed_a_callback_cycle_from_an_unused_literal() {
    let (module, plan) = callback_cycle(true);
    let directory = std::env::var_os("ZIL_TEST_REPORT_DIR").map(|root| {
        let directory = std::path::PathBuf::from(root)
            .join(format!("context-static-cycle-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    });
    if let Some(directory) = &directory {
        std::fs::write(
            directory.join("control-unverified.wire"),
            wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap(),
        )
        .unwrap();
    }
    checked(&module, &plan);
    let (module, plan) = callback_cycle(false);
    let limits = Limits::default();
    if let Some(directory) = &directory {
        std::fs::write(
            directory.join("cycle-unverified.wire"),
            wire::encode_module_candidate(&module, &plan, &limits).unwrap(),
        )
        .unwrap();
    }
    for wire in [false, true] {
        let errors = if wire {
            let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                .unwrap_err()
        };
        assert!(
            errors.iter().any(|error| error.code == "E-VERIFY-STORED"
                && error
                    .message
                    .contains("dependency cycle has no actual program-static root")),
            "wire={wire}: {errors:?}"
        );
    }
}
