//! The bytes were produced from context-read.zil by real Expand+Types. The
//! typed-only fixture has an empty plan for raw header checks. Separate real
//! Analyze candidates exercise full independent module verification.
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
    b.stored_context = true;
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

#[test]
fn actual_explicit_error_context_factory_has_no_inactive_success_targets() {
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("../../tests/fixtures/dyn-context/context-error-only.candidate.wire"),
        &limits,
    )
    .unwrap();
    let target = module.target.clone();
    for wire in [false, true] {
        let (module, plan) = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
        } else {
            (module.clone(), plan.clone())
        };
        let result = crate::verify_module(module, plan, &target, &limits);
        assert!(
            result.is_ok(),
            "explicit error return (Wire={wire}): {result:?}"
        );
    }
}

#[test]
fn actual_context_factory_candidate_proves_error_success_class_path_and_result_sources() {
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("../../tests/fixtures/dyn-context/context-factory.candidate.wire"),
        &limits,
    )
    .unwrap();
    let target = module.target.clone();
    let copied = crate::verify_module(module.clone(), plan.clone(), &target, &limits);
    assert!(copied.is_ok(), "cloned input: {copied:?}");
    let result = crate::verify_module(module, plan, &target, &limits);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn actual_context_unused_upper_port_is_known_empty_through_call_and_cleanup() {
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("../../tests/fixtures/dyn-context/context-unused.candidate.wire"),
        &limits,
    )
    .unwrap();
    let target = module.target.clone();
    let result = crate::verify_module(module.clone(), plan.clone(), &target, &limits);
    assert!(result.is_ok(), "{result:?}");
    let mut omitted = module;
    let table = omitted
        .dyn_vtables
        .iter_mut()
        .find(|t| t.ports.len() == 2)
        .unwrap();
    assert!(table.ports[1].sources.is_empty());
    table.ports.pop();
    let mut plan = plan;
    plan.module_fingerprint = omitted.fingerprint();
    for wire in [false, true] {
        let (module, plan) = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&omitted, &plan, &limits).unwrap();
            zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
        } else {
            (omitted.clone(), plan.clone())
        };
        let failure = crate::verify_module(module, plan, &target, &limits).unwrap_err();
        assert_ne!(failure[0].code, "E-VERIFY-PLAN-IDENTITY");
    }
}

#[test]
fn actual_context_factory_unknown_empty_wrong_root_and_duplicate_sources_reject_raw_and_wire() {
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("../../tests/fixtures/dyn-context/context-factory.candidate.wire"),
        &limits,
    )
    .unwrap();
    for attack in 0..7 {
        let mut wrong = module.clone();
        let factory = wrong
            .functions
            .iter_mut()
            .find(|f| f.name == "create")
            .unwrap();
        match attack {
            0 => factory.retained_return_sources = None,
            1 => factory.retained_return_sources = Some(vec![]),
            2 => factory.retained_return_sources.as_mut().unwrap()[0]
                .sources
                .clear(),
            3 => {
                let row = factory.retained_return_sources.as_ref().unwrap()[0].clone();
                factory.retained_return_sources.as_mut().unwrap().push(row);
            }
            6 => {
                let source =
                    factory.retained_return_sources.as_ref().unwrap()[0].sources[0].clone();
                factory.retained_return_sources.as_mut().unwrap()[0]
                    .sources
                    .push(source);
            }
            other => {
                factory.retained_return_sources.as_mut().unwrap()[0].sources =
                    vec![StoragePosition {
                        base: StorageBase::Parameter(if other == 4 { 1 } else { 2 }),
                        path: vec![],
                    }];
            }
        }
        let target = wrong.target.clone();
        // Rebind only cache identity. All actual instructions and plan facts
        // stay fixed, so rejection must come from the independent kernel.
        let mut plan = plan.clone();
        plan.module_fingerprint = wrong.fingerprint();
        let failure =
            crate::verify_module(wrong.clone(), plan.clone(), &target, &limits).unwrap_err();
        assert_eq!(
            failure[0].code, "E-VERIFY-DYN-SOURCE",
            "raw attack {attack}: {failure:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&wrong, &plan, &limits).unwrap();
        let (wrong, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let failure = crate::verify_module(wrong, plan, &target, &limits).unwrap_err();
        assert_eq!(
            failure[0].code, "E-VERIFY-DYN-SOURCE",
            "Wire attack {attack}: {failure:?}"
        );
    }
}

#[test]
fn erased_owner_arm_extraction_cannot_release_dependencies_without_an_actual_tag_guard() {
    let limits = Limits::default();
    let (mut module, mut plan) = zil_ir::wire::decode_module_candidate(
        include_bytes!("../../tests/fixtures/dyn-context/context-factory.candidate.wire"),
        &limits,
    )
    .unwrap();
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "start")
        .unwrap();
    let conditions = f
        .blocks
        .iter()
        .filter_map(|block| {
            if let Terminator::Branch { condition, .. } = block.terminator {
                Some(condition)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(conditions.len(), 2);
    for (index, condition) in conditions.iter().enumerate() {
        let test = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| i.result == Some(*condition))
            .unwrap();
        assert!(matches!(
            test.kind,
            InstructionKind::Binary {
                op: BinaryOp::Equal,
                ..
            }
        ));
        // Skip the wildcard error case and select the owning success case,
        // without retaining either actual tag comparison.
        test.kind = InstructionKind::Constant(Constant::Bool(index == 1));
    }
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    for wire in [false, true] {
        let (module, plan) = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap()
        } else {
            (module.clone(), plan.clone())
        };
        let result = crate::verify_module(module, plan, &target, &limits);
        assert!(result.is_err(), "unguarded owning extraction was accepted");
        let failure = result.err().unwrap();
        assert!(
            failure.iter().any(|d| d.code == "E-VERIFY-SUM"),
            "{failure:?}"
        );
    }
}
