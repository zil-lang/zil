//! Raw independent-checker oracles. These do not constitute producer or native
//! evidence; the product API is exercised with deliberately untrusted rows.
use super::*;

fn inputs(module: TypedModule, plan: CandidatePlan) -> VerifiedInputs {
    VerifiedInputs::SharedProgram {
        program: Arc::new(TypedProgram {
            module,
            interfaces: vec![],
            task_interfaces: vec![],
            root_functions: vec![],
        }),
        plan: Arc::new(plan),
    }
}
fn product(
    session: &mut VerifySession,
    module: &TypedModule,
    plan: &CandidatePlan,
    limits: &Limits,
) -> CompileResult<VerifiedModule> {
    let VerifiedInputs::SharedProgram { program, plan } = inputs(module.clone(), plan.clone())
    else {
        unreachable!()
    };
    session.verify_product_program_module(program, plan, &module.target, limits)
}
fn oracle(
    session: &mut VerifySession,
    module: &TypedModule,
    plan: &CandidatePlan,
    limits: &Limits,
) {
    let fresh = verify_inputs_with_identity(
        inputs(module.clone(), plan.clone()),
        &module.target,
        limits,
        None,
        VerificationIdentity::Product,
    );
    let warm = product(session, module, plan, limits);
    match (fresh, warm) {
        (Ok(fresh), Ok(warm)) => {
            assert_eq!(fresh.module(), warm.module());
            assert_eq!(fresh.plan(), warm.plan());
            assert_eq!(fresh.fingerprint(), warm.fingerprint());
        }
        (Err(fresh), Err(warm)) => assert_eq!(fresh, warm),
        (fresh, warm) => panic!("current raw oracle differs: {fresh:?} {warm:?}"),
    }
}
fn attempt<'a>(
    session: &'a VerifySession,
    inputs: &VerifiedInputs,
    limits: &Limits,
) -> Attempt<'a> {
    Attempt {
        previous: &session.flows,
        previous_empty_borrows: &session.empty_borrows,
        previous_shapes: &session.shapes,
        previous_inventory: &session.inventory,
        previous_inputs: session.verified.as_ref(),
        delta: RawDelta::new(
            session.verified.as_ref(),
            inputs.module(),
            inputs.plan(),
            &inputs.module().target,
            limits,
        ),
        previous_shape_tree: session.shape_tree.as_ref(),
        next_shape_tree: None,
        previous_resource_inventory: session.resource_inventory.as_ref(),
        next_resource_inventory: None,
        next: BTreeMap::new(),
        next_empty_borrows: BTreeMap::new(),
        next_shapes: session.shapes.clone(),
        next_inventory: session.inventory.clone(),
        bound_plans: BTreeMap::new(),
        bound_closures: None,
        bound_functions: BTreeMap::new(),
        stats: VerifyStats::default(),
    }
}
fn raw_budget(limits: &Limits, count: usize) -> Budget {
    let mut budget = Budget::new(limits);
    budget.stored_context = true;
    budget.function_count = count;
    budget
}

#[test]
fn persistent_private_delta_reuses_ranges_and_keeps_current_authority() {
    let (mut module, mut plan) = pair();
    for index in 2..16 {
        let mut function = module.functions[0].clone();
        function.id = FunctionId(index);
        function.name = format!("private_{index}");
        module.functions.push(function);
        let mut row = plan.functions[0].clone();
        row.function = FunctionId(index);
        plan.functions.push(row);
    }
    plan.module_fingerprint = module.structural_fingerprint();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    product(&mut session, &module, &plan, &limits).unwrap();
    let initial = session.verified.clone().unwrap();
    module.functions[7].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(41));
    plan.module_fingerprint = module.structural_fingerprint();
    oracle(&mut session, &module, &plan, &limits);
    let stats = session.stats();
    assert_eq!(stats.raw_result_modes_checked, 1);
    assert_eq!(stats.raw_result_modes_reused, 15);
    assert_eq!(stats.raw_shape_functions_aggregated, 15);
    assert_eq!(stats.raw_shape_functions_reused, 1);
    assert_eq!(stats.raw_inventory_functions_reused, 15);
    assert_eq!(stats.raw_inventory_functions_checked, 1);
    assert_eq!(stats.resource_inventory_functions_checked, 1);
    assert_eq!(stats.resource_inventory_functions_reused, 15);
    assert!(stats.raw_shape_ranges_reused < 15);
    assert_ne!(
        initial.fingerprint(),
        session.verified.as_ref().unwrap().fingerprint()
    );
    assert!(Arc::ptr_eq(
        initial.module().functions.owner(0).unwrap(),
        module.functions.owner(0).unwrap()
    ));

    // The shared plan tree is still an untrusted claim after a same-ID raw edit.
    let before_failure = session.verified.clone().unwrap();
    let mut invalid = module.clone();
    invalid.functions[0].blocks[0].instructions[3].kind = InstructionKind::StorageDead(LocalId(0));
    let mut stale = plan.clone();
    stale.module_fingerprint = invalid.structural_fingerprint();
    assert!(stale.functions.ptr_eq(&plan.functions));
    assert!(product(&mut session, &invalid, &stale, &limits).is_err());
    oracle(&mut session, &invalid, &stale, &limits);
    assert_eq!(
        session.verified.as_ref().unwrap().fingerprint(),
        before_failure.fingerprint()
    );
    oracle(&mut session, &module, &plan, &limits);
    assert_eq!(session.stats().raw_shape_functions_aggregated, 16);
}

#[test]
fn product_and_wire_identity_domains_cannot_share_whole_authority() {
    let (module, mut plan) = pair();
    plan.module_fingerprint = module.structural_fingerprint();
    let program = Arc::new(TypedProgram {
        module,
        interfaces: vec![],
        task_interfaces: vec![],
        root_functions: vec![],
    });
    let plan = Arc::new(plan);
    let limits = Limits::default();
    let mut session = VerifySession::new();
    session
        .verify_product_program_module(
            program.clone(),
            plan.clone(),
            &program.module.target,
            &limits,
        )
        .unwrap();
    assert!(
        session
            .verify_shared_program_module(
                program.clone(),
                plan.clone(),
                &program.module.target,
                &limits
            )
            .is_err()
    );
    assert_eq!(session.stats().full_receipts_reused, 0);
    session
        .verify_product_program_module(
            program.clone(),
            plan.clone(),
            &program.module.target,
            &limits,
        )
        .unwrap();
    assert_eq!(session.stats().full_receipts_reused, 1);
    let mut changed = (*program).clone();
    changed.module.source_fingerprint = Fingerprint::of(b"changed current source");
    oracle(&mut session, &changed.module, &plan, &limits);
    assert!(product(&mut session, &changed.module, &plan, &limits).is_err());
    let mut limits2 = limits.clone();
    limits2.max_ct_steps -= 1;
    session
        .verify_product_program_module(program.clone(), plan, &program.module.target, &limits2)
        .unwrap();
    assert_eq!(session.stats().full_receipts_reused, 0);
    assert_eq!(session.stats().raw_shape_functions_aggregated, 0);
}

#[test]
fn rich_unused_raw_shape_changes_are_checked_and_failed_epochs_do_not_publish() {
    let (mut module, mut plan) = pair();
    let span = module.functions[0].span;
    module.functions[1].locals.push(LocalDecl {
        id: LocalId(1),
        name: "unused_rich_raw".into(),
        ty: Type::Record {
            name: "RawRecord".into(),
            fields: vec![(
                "array".into(),
                Type::Array {
                    element: Box::new(Type::I32),
                    length: 3,
                },
            )],
        },
        mutable: false,
        span,
    });
    plan.module_fingerprint = module.structural_fingerprint();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    product(&mut session, &module, &plan, &limits).unwrap();
    let original = module.clone();
    let original_plan = plan.clone();
    // A module header edit keeps all rich leaf allocations, but grants no proof.
    module.source_fingerprint = Fingerprint::of(b"second header");
    plan.module_fingerprint = module.structural_fingerprint();
    oracle(&mut session, &module, &plan, &limits);
    assert_eq!(session.stats().raw_shape_functions_aggregated, 2);
    // Same ID and same plan, a hidden unused source type now exceeds depth.
    let mut ty = Type::I32;
    for _ in 0..=limits.max_call_depth {
        ty = Type::Array {
            element: Box::new(ty),
            length: 1,
        };
    }
    module.functions[1].locals[1].ty = ty;
    // Deliberately leave the old digest: raw shape errors still precede identity.
    oracle(&mut session, &module, &plan, &limits);
    assert!(product(&mut session, &module, &plan, &limits).is_err());
    oracle(&mut session, &original, &original_plan, &limits);
}

#[test]
fn shape_aggregate_exact_work_and_short_credit_preserve_first_error_and_balance() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let current = VerifiedInputs::SharedModule {
        module: Arc::new(module.clone()),
        plan: Arc::new(plan),
    };
    let mut measured = raw_budget(&limits, module.functions.len());
    for (i, f) in module.functions.iter().enumerate() {
        check_function_raw_shapes(&module, i, f, &mut measured, &limits, None).unwrap();
    }
    let work = limits.max_analysis_steps - measured.remaining;
    for available in [work, work - 1, 0] {
        let mut fresh = raw_budget(&limits, module.functions.len());
        fresh.remaining = available;
        let expected = module.functions.iter().enumerate().try_for_each(|(i, f)| {
            check_function_raw_shapes(&module, i, f, &mut fresh, &limits, None)
        });
        let mut warm = raw_budget(&limits, module.functions.len());
        warm.remaining = available;
        let mut call = attempt(&session, &current, &limits);
        let actual = call.check_shape_tree(current.module(), &mut warm, &limits);
        assert_eq!(expected, actual);
        assert_eq!(fresh.remaining, warm.remaining);
        if available == work {
            assert_eq!(call.stats.raw_shape_functions_aggregated, 2);
        }
    }
}

#[test]
fn instruction_inventory_count_boundary_and_raw_variant_use_fresh_oracle() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let current = VerifiedInputs::SharedModule {
        module: Arc::new(module.clone()),
        plan: Arc::new(plan.clone()),
    };
    let f = &current.module().functions[0];
    let count: usize = f.blocks.iter().map(|b| b.instructions.len()).sum();
    for initial in [
        limits.max_instructions - count,
        limits.max_instructions - count + 1,
    ] {
        let mut fresh = raw_budget(&limits, 2);
        let mut warm = raw_budget(&limits, 2);
        let mut fresh_count = initial;
        let mut warm_count = initial;
        let expected = inventory::check_current(
            current.module(),
            f,
            &mut fresh,
            &limits,
            &mut fresh_count,
            false,
        )
        .map(|_| ());
        let mut call = attempt(&session, &current, &limits);
        let actual = call.check_inventory(current.module(), f, &mut warm, &limits, &mut warm_count);
        assert_eq!(expected, actual);
        assert_eq!(fresh.remaining, warm.remaining);
        assert_eq!(fresh_count, warm_count);
        assert_eq!(
            call.stats.raw_inventory_functions_reused,
            usize::from(initial + count == limits.max_instructions)
        );
    }
    let mut invalid = module.clone();
    invalid.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::StaticUtf8 { bytes: vec![0xff] };
    let mut candidate = plan;
    candidate.module_fingerprint = invalid.fingerprint();
    let expected =
        verify_module(invalid.clone(), candidate.clone(), &invalid.target, &limits).unwrap_err();
    assert_eq!(
        verify(&mut session, &invalid, &candidate, &limits).unwrap_err(),
        expected
    );
    assert_eq!(expected[0].code, "E-VERIFY-UTF8");
}

#[test]
fn complete_delta_includes_append_delete_and_reorder_with_unchanged_candidate_rows() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let mut appended = module.clone();
    let mut appended_plan = plan.clone();
    let mut function = appended.functions[0].clone();
    function.id = FunctionId(2);
    function.name = "appended".into();
    appended.functions.push(function);
    let mut row = appended_plan.functions[0].clone();
    row.function = FunctionId(2);
    appended_plan.functions.push(row);
    appended_plan.module_fingerprint = appended.fingerprint();
    verify_module(
        appended.clone(),
        appended_plan.clone(),
        &appended.target,
        &limits,
    )
    .unwrap();
    verify(&mut session, &appended, &appended_plan, &limits).unwrap();
    assert_eq!(session.stats().raw_result_modes_checked, 1);
    assert_eq!(session.stats().raw_result_modes_reused, 2);
    verify(&mut session, &module, &plan, &limits).unwrap();
    let mut removed = module.clone();
    removed.functions.remove(0);
    let mut candidate = plan.clone();
    candidate.functions.remove(0);
    candidate.module_fingerprint = removed.fingerprint();
    let fresh =
        verify_module(removed.clone(), candidate.clone(), &removed.target, &limits).unwrap_err();
    assert_eq!(
        verify(&mut session, &removed, &candidate, &limits).unwrap_err(),
        fresh
    );
    let mut invalid = module.clone();
    invalid.functions[1].result_mode = PassingMode::Create;
    let mut candidate = plan;
    candidate.module_fingerprint = invalid.fingerprint();
    let fresh =
        verify_module(invalid.clone(), candidate.clone(), &invalid.target, &limits).unwrap_err();
    assert_eq!(
        verify(&mut session, &invalid, &candidate, &limits).unwrap_err(),
        fresh
    );
    assert_eq!(session.stats().raw_result_modes_checked, 1);
}

#[test]
fn resource_entry_aggregate_replays_exact_work_and_original_short_balance() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    assert!(session.resource_inventory.is_some());
    let current = VerifiedInputs::SharedModule {
        module: Arc::new(module.clone()),
        plan: Arc::new(plan),
    };
    let work: usize = module
        .functions
        .iter()
        .map(|f| f.locals.len() + f.values.len())
        .sum();
    for available in [work, work - 1, 0] {
        let mut fresh = raw_budget(&limits, module.functions.len());
        fresh.remaining = available;
        let expected = module.functions.iter().try_fold(false, |owns, f| {
            resources::raw_function_owns(f, &mut fresh).map(|value| owns || value)
        });
        let mut warm = raw_budget(&limits, module.functions.len());
        warm.remaining = available;
        let mut call = attempt(&session, &current, &limits);
        let actual = call.check_resource_ownership(current.module(), &mut warm);
        assert_eq!(expected, actual);
        assert_eq!(fresh.remaining, warm.remaining);
        if available == work {
            assert_eq!(call.stats.resource_inventory_functions_reused, 2);
            assert_eq!(call.stats.resource_inventory_ranges_reused, 1);
            assert_eq!(call.stats.resource_inventory_functions_checked, 0);
        }
    }
}

#[test]
fn resource_entry_raw_ownership_change_is_not_authorized_by_shared_candidate() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let mut changed = module.clone();
    changed.functions[0].locals[0].ty = Type::OwnedDyn {
        interface: DynInterfaceId(0),
    };
    let current = VerifiedInputs::SharedModule {
        module: Arc::new(changed),
        plan: Arc::new(plan),
    };
    // This tests the raw gate alone, not module admission of the absent dyn
    // definition. In full verification the actual definition is still required.
    let mut fresh = raw_budget(&limits, 2);
    let expected = current
        .module()
        .functions
        .iter()
        .try_fold(false, |owns, f| {
            resources::raw_function_owns(f, &mut fresh).map(|value| owns || value)
        });
    let mut warm = raw_budget(&limits, 2);
    let mut call = attempt(&session, &current, &limits);
    assert_eq!(
        call.check_resource_ownership(current.module(), &mut warm),
        expected
    );
    assert_eq!(expected, Ok(true));
    assert_eq!(fresh.remaining, warm.remaining);
    assert_eq!(call.stats.resource_inventory_functions_checked, 1);
    assert_eq!(call.stats.resource_inventory_functions_reused, 1);
    assert!(verify(&mut session, current.module(), current.plan(), &limits).is_err());
    // A failed module request cannot publish the tentative true predicate.
    let mut restored = attempt(
        &session,
        &VerifiedInputs::SharedModule {
            module: Arc::new(module.clone()),
            plan: Arc::new(session.verified.as_ref().unwrap().plan().clone()),
        },
        &limits,
    );
    assert_eq!(
        restored.check_resource_ownership(&module, &mut raw_budget(&limits, 2)),
        Ok(false)
    );
    assert_eq!(restored.stats.resource_inventory_functions_reused, 2);
}
