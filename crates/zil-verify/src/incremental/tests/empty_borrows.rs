//! Independent raw/checker tests. These are not source or native evidence.
use super::*;

fn attempt(session: &VerifySession) -> Attempt<'_> {
    Attempt {
        previous: &session.flows,
        previous_empty_borrows: &session.empty_borrows,
        previous_shapes: &session.shapes,
        previous_inventory: &session.inventory,
        previous_shape_tree: session.shape_tree.as_ref(),
        next_shape_tree: None,
        delta: None,
        previous_inputs: session.verified.as_ref(),
        next: BTreeMap::new(),
        next_empty_borrows: BTreeMap::new(),
        next_shapes: session.shapes.clone(),
        next_inventory: session.inventory.clone(),
        bound_functions: BTreeMap::new(),
        bound_plans: BTreeMap::new(),
        bound_closures: None,
        stats: VerifyStats::default(),
    }
}

fn graphs(module: &TypedModule, plan: &CandidatePlan) -> Vec<FunctionGraph> {
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    module
        .functions
        .iter()
        .zip(&plan.functions)
        .map(|(f, p)| check_function(module, f, p, &mut budget, &limits).unwrap())
        .collect()
}

fn oracle(module: &TypedModule, plan: &CandidatePlan, session: &mut VerifySession) {
    let limits = Limits::default();
    let fresh = verify_module(module.clone(), plan.clone(), &module.target, &limits);
    let warm = verify(session, module, plan, &limits);
    match (fresh, warm) {
        (Ok(fresh), Ok(warm)) => {
            assert_eq!(fresh.module(), warm.module());
            assert_eq!(fresh.plan(), warm.plan());
        }
        (Err(fresh), Err(warm)) => assert_eq!(fresh, warm),
        (fresh, warm) => panic!("fresh/warm acceptance differs: {fresh:?} {warm:?}"),
    }
}

#[test]
fn cold_private_edit_and_recovery_consume_empty_facts_without_dense_rows() {
    let (original, original_plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    oracle(&original, &original_plan, &mut session);
    assert_eq!(session.stats().empty_borrow_proofs_checked, 2);
    assert_eq!(session.stats().empty_borrow_values_avoided, 8);
    let mut edited = original.clone();
    edited.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(7));
    let mut edited_plan = original_plan.clone();
    edited_plan.module_fingerprint = edited.fingerprint();
    oracle(&edited, &edited_plan, &mut session);
    assert_eq!(session.stats().empty_borrow_proofs_checked, 1);
    assert_eq!(session.stats().empty_borrow_proofs_reused, 1);
    assert_eq!(session.stats().empty_borrow_values_avoided, 8);
    oracle(&original, &original_plan, &mut session);
    assert_eq!(session.stats().empty_borrow_proofs_checked, 1);
    assert_eq!(session.stats().empty_borrow_proofs_reused, 1);
    let mut changed_limits = limits;
    changed_limits.max_ct_steps -= 1;
    verify(&mut session, &original, &original_plan, &changed_limits).unwrap();
    assert_eq!(session.stats().empty_borrow_proofs_reused, 0);
    assert_eq!(session.stats().empty_borrow_proofs_checked, 2);
}

#[test]
fn every_residual_budget_matches_dense_oracle_at_each_original_stage() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let graphs = graphs(&module, &plan);
    // Ordinary fee = descriptor(1) + module(2) + reconstruct(8) + Hold(8).
    // Static rows add eight separate spends; shared-thread presence adds two.
    for (text, threads, total) in [(false, false, 19), (true, false, 27), (false, true, 21)] {
        for remaining in 0..=total + 1 {
            let mut fresh_budget = Budget::new(&limits);
            fresh_budget.remaining = remaining;
            fresh_budget.static_utf8 = text;
            fresh_budget.shared_thread_functions = threads.then(|| vec![false; 2]);
            let mut warm_budget = Budget::new(&limits);
            warm_budget.remaining = remaining;
            warm_budget.static_utf8 = text;
            warm_budget.shared_thread_functions = threads.then(|| vec![false; 2]);
            let fresh = borrows::check_module(&module, &plan, &graphs, &mut fresh_budget);
            let mut attempt = attempt(&session);
            let warm = borrows::check_module_with_reuse(
                &module,
                &plan,
                &graphs,
                &mut warm_budget,
                Some(&mut attempt),
            );
            assert_eq!(
                warm, fresh,
                "remaining={remaining}, text={text}, threads={threads}"
            );
            assert_eq!(warm_budget.remaining, fresh_budget.remaining);
            assert_eq!(warm.is_ok(), remaining >= total);
            if remaining >= total && !text && !threads {
                assert_eq!(attempt.stats.empty_borrow_proofs_reused, 2);
                assert_eq!(attempt.stats.empty_borrow_values_avoided, 8);
            }
        }
    }
}

#[test]
fn changed_holds_keep_error_order_and_failed_proofs_unpublished() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let checkpoint = session.clone();
    let graphs = graphs(&module, &plan);
    for wrong_identity in [false, true] {
        let mut invalid = plan.clone();
        if wrong_identity {
            invalid.functions[1].holds[0].value = ValueId(3);
        } else {
            invalid.functions[1].holds[0]
                .roots
                .push(HoldRoot::Local(LocalId(0)));
        }
        for remaining in 0..25 {
            let mut fresh_budget = Budget::new(&limits);
            fresh_budget.remaining = remaining;
            let mut warm_budget = Budget::new(&limits);
            warm_budget.remaining = remaining;
            let fresh = borrows::check_module(&module, &invalid, &graphs, &mut fresh_budget);
            let mut attempt = attempt(&session);
            let warm = borrows::check_module_with_reuse(
                &module,
                &invalid,
                &graphs,
                &mut warm_budget,
                Some(&mut attempt),
            );
            assert_eq!(warm, fresh);
            assert_eq!(warm_budget.remaining, fresh_budget.remaining);
        }
        oracle(&module, &invalid, &mut session);
        assert!(Arc::ptr_eq(
            &session.empty_borrows,
            &checkpoint.empty_borrows
        ));
    }
    oracle(&module, &plan, &mut session);
    assert_eq!(session.stats().empty_borrow_proofs_reused, 2);
}

#[test]
fn raw_source_type_same_id_and_current_global_mode_cannot_use_old_absence() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let graphs = graphs(&module, &plan);
    for parameter in [false, true] {
        let mut raw = module.clone();
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        if parameter {
            raw.functions[0].parameters.push(LocalId(0));
            raw.functions[0].locals[0].ty = address;
        } else {
            raw.functions[0].values[0] = address;
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = raw.fingerprint();
        oracle(&raw, &candidate, &mut session);
        let mut fresh_budget = Budget::new(&limits);
        let fresh = borrows::check_module(&raw, &candidate, &graphs, &mut fresh_budget);
        let mut warm_budget = Budget::new(&limits);
        let mut attempt = attempt(&session);
        let warm = borrows::check_module_with_reuse(
            &raw,
            &candidate,
            &graphs,
            &mut warm_budget,
            Some(&mut attempt),
        );
        assert_eq!(warm, fresh);
        assert_eq!(warm_budget.remaining, fresh_budget.remaining);
        assert!(!attempt.next_empty_borrows.contains_key(&FunctionId(0)));
    }
    // These flags select actual dense-fact consumers. No previous absence
    // proof may suppress them, even when the function allocation is unchanged.
    for mode in 0..5 {
        let configure = |budget: &mut Budget| match mode {
            0 => budget.task_borrowed_results = true,
            1 => budget.store_option_results = true,
            2 => budget.invocation_callbacks = true,
            3 => budget.borrowed_function_pointers = true,
            _ => budget.generic_mutex = true,
        };
        let mut fresh_budget = Budget::new(&limits);
        configure(&mut fresh_budget);
        let fresh = borrows::check_module(&module, &plan, &graphs, &mut fresh_budget);
        let mut warm_budget = Budget::new(&limits);
        configure(&mut warm_budget);
        let mut attempt = attempt(&session);
        let warm = borrows::check_module_with_reuse(
            &module,
            &plan,
            &graphs,
            &mut warm_budget,
            Some(&mut attempt),
        );
        assert_eq!(warm, fresh);
        assert_eq!(warm_budget.remaining, fresh_budget.remaining);
        assert_eq!(attempt.stats.empty_borrow_proofs_reused, 0);
        assert_eq!(attempt.stats.empty_borrow_values_avoided, 0);
    }
}

#[test]
fn closure_capture_and_entry_changes_are_current_raw_dependencies() {
    let (mut module, mut plan) = pair();
    module.functions[1].environment = Some(ClosureId(0));
    module.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(0),
        function: FunctionId(1),
        captures: Vec::new(),
        parameters: Vec::new(),
        result: Type::I32,
        kind: CallableKind::Fn,
        lending_captures: Vec::new(),
    });
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    assert_eq!(session.stats().empty_borrow_proofs_checked, 2);
    let graphs = graphs(&module, &plan);
    for mutation in 0..4 {
        let mut raw = module.clone();
        match mutation {
            0 => raw.closures[0].captures.push(Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }),
            1 => raw.closures[0].lending_captures.push(0),
            2 => raw.closures[0].function = FunctionId(0),
            _ => raw.closures[0].kind = CallableKind::FnMut,
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = raw.fingerprint();
        // Phase oracle explicitly exercises dependency matching even when an
        // earlier full-module structural check rejects this raw mutation.
        let mut fresh_budget = Budget::new(&limits);
        let fresh = borrows::check_module(&raw, &candidate, &graphs, &mut fresh_budget);
        let mut warm_budget = Budget::new(&limits);
        let mut attempt = attempt(&session);
        let warm = borrows::check_module_with_reuse(
            &raw,
            &candidate,
            &graphs,
            &mut warm_budget,
            Some(&mut attempt),
        );
        assert_eq!(warm, fresh);
        assert_eq!(warm_budget.remaining, fresh_budget.remaining);
        assert_eq!(attempt.stats.empty_borrow_proofs_reused, 0);
        let mut fork = session.clone();
        oracle(&raw, &candidate, &mut fork);
    }
}

#[test]
fn source_absence_does_not_skip_affine_local_loan_work() {
    let (mut module, mut plan) = pair();
    let mut local = module.functions[0].locals[0].clone();
    local.id = LocalId(1);
    local.name = "exclusive_slot".into();
    local.ty = Type::Address {
        mutable: true,
        inner: Box::new(Type::I32),
    };
    module.functions[0].locals.push(local);
    plan.module_fingerprint = module.fingerprint();
    let mut session = VerifySession::new();
    // Only the second function has a complete empty source AND loan proof.
    oracle(&module, &plan, &mut session);
    assert!(session.verified.is_some());
    assert_eq!(session.stats().empty_borrow_proofs_checked, 1);
    assert_eq!(session.stats().empty_borrow_values_avoided, 4);
    assert!(!session.empty_borrows.contains_key(&FunctionId(0)));
    oracle(&module, &plan, &mut session);
    assert_eq!(session.stats().empty_borrow_proofs_reused, 1);
    assert_eq!(session.stats().empty_borrow_values_avoided, 4);
}

#[test]
fn return_frontier_and_resource_origins_stay_fresh_after_an_empty_epoch() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let graphs = graphs(&module, &plan);
    for frontier in [false, true] {
        let mut raw = module.clone();
        if frontier {
            raw.functions[1].return_type = Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            };
        } else {
            raw.resources.push(ResourceDefinition {
                name: "UnusedResource".into(),
                payload: Type::Record {
                    name: "UnusedResource".into(),
                    fields: vec![("value".into(), Type::I32)],
                },
                teardown: None,
            });
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = raw.fingerprint();
        let mut fresh_budget = Budget::new(&limits);
        let fresh = borrows::check_module(&raw, &candidate, &graphs, &mut fresh_budget);
        let mut warm_budget = Budget::new(&limits);
        let mut attempt = attempt(&session);
        let warm = borrows::check_module_with_reuse(
            &raw,
            &candidate,
            &graphs,
            &mut warm_budget,
            Some(&mut attempt),
        );
        assert_eq!(warm, fresh);
        assert_eq!(warm_budget.remaining, fresh_budget.remaining);
        assert_eq!(attempt.stats.empty_borrow_proofs_reused, 0);
        assert_eq!(attempt.stats.empty_borrow_values_avoided, 0);
        let mut fork = session.clone();
        oracle(&raw, &candidate, &mut fork);
    }
}
