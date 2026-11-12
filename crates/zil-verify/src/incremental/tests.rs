//! Raw/Wire authority tests; these do not claim source/native coverage.
use super::*;

fn pair() -> (TypedModule, CandidatePlan) {
    let (mut module, mut plan) = crate::tests::baseline();
    let mut second = module.functions[0].clone();
    second.id = FunctionId(1);
    second.name = "other".into();
    module.functions.push(second);
    let mut second_plan = plan.functions[0].clone();
    second_plan.function = FunctionId(1);
    plan.functions.push(second_plan);
    plan.module_fingerprint = module.fingerprint();
    (module, plan)
}

fn verify(
    session: &mut VerifySession,
    module: &TypedModule,
    plan: &CandidatePlan,
    limits: &Limits,
) -> CompileResult<VerifiedModule> {
    session.verify_shared_module(
        Arc::new(module.clone()),
        Arc::new(plan.clone()),
        &module.target,
        limits,
    )
}

#[test]
fn private_edit_reuses_unchanged_function_and_rebinds_current_spans() {
    let (mut module, mut plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    assert_eq!(session.stats().function_proofs_checked, 2);
    module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(41));
    let shifted = Span::new(zil_base::FileId(17), 100, 200);
    module.functions[1].span = shifted;
    for local in &mut module.functions[1].locals {
        local.span = shifted;
    }
    for instruction in &mut module.functions[1].blocks[0].instructions {
        instruction.span = shifted;
    }
    plan.module_fingerprint = module.fingerprint();
    let checked = verify(&mut session, &module, &plan, &limits).unwrap();
    assert_eq!(checked.module(), &module);
    assert_eq!(checked.plan(), &plan);
    assert_eq!(
        session.stats(),
        VerifyStats {
            full_receipts_reused: 0,
            function_proofs_checked: 1,
            function_proofs_reused: 1,
            instruction_proofs_reused: 4,
            empty_borrow_proofs_checked: 1,
            empty_borrow_proofs_reused: 1,
            empty_borrow_values_avoided: 8,
        }
    );
}

#[test]
fn whole_receipt_requires_both_live_allocations_and_all_limits() {
    let (module, plan) = pair();
    let mut module = Arc::new(module);
    let mut plan = Arc::new(plan);
    let target = module.target.clone();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    session
        .verify_shared_module(module.clone(), plan.clone(), &target, &limits)
        .unwrap();
    let checked = session
        .verify_shared_module(module.clone(), plan.clone(), &target, &limits)
        .unwrap();
    assert_eq!(session.stats().full_receipts_reused, 1);
    assert!(std::ptr::eq(checked.module(), module.as_ref()));
    assert!(Arc::get_mut(&mut module).is_none());
    assert!(Arc::get_mut(&mut plan).is_none());
    let mut changed_limits = limits.clone();
    changed_limits.max_ct_steps -= 1;
    session
        .verify_shared_module(module.clone(), plan.clone(), &target, &changed_limits)
        .unwrap();
    assert_eq!(session.stats().full_receipts_reused, 0);
    assert_eq!(session.stats().function_proofs_checked, 2);
    let mut wrong_target = target.clone();
    wrong_target.checked_arithmetic = false;
    let error = session
        .verify_shared_module(module.clone(), plan.clone(), &wrong_target, &changed_limits)
        .unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-TARGET");
    // make_mut cannot alter a sealed receipt's retained allocation.
    Arc::make_mut(&mut plan).functions[0].blocks[0]
        .initialized_in
        .push(LocalId(0));
    let error = session
        .verify_shared_module(module, plan, &target, &changed_limits)
        .unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-INITIALIZATION-CLAIM");
    assert_eq!(session.stats().full_receipts_reused, 0);
}

#[test]
fn independent_wire_inputs_reuse_only_the_local_proof() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let (decoded, candidate) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    verify(&mut session, &decoded, &candidate, &limits).unwrap();
    assert_eq!(session.stats().function_proofs_reused, 2);
    assert_eq!(session.stats().full_receipts_reused, 0);
    let mut counterfeit = candidate;
    counterfeit.functions[1].holds[0]
        .roots
        .push(HoldRoot::Local(LocalId(0)));
    let fresh = verify_module(
        decoded.clone(),
        counterfeit.clone(),
        &module.target,
        &limits,
    )
    .unwrap_err();
    let warm = verify(&mut session, &decoded, &counterfeit, &limits).unwrap_err();
    assert_eq!(warm, fresh);
}

#[test]
fn changed_callee_signature_is_checked_even_when_caller_is_unchanged() {
    let (mut module, mut plan) = pair();
    module.functions[0].blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: Vec::new(),
    };
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    module.functions[1].parameters.push(LocalId(0));
    plan.module_fingerprint = module.fingerprint();
    let fresh = verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap_err();
    let warm = verify(&mut session, &module, &plan, &limits).unwrap_err();
    assert_eq!(warm, fresh);
    assert_eq!(warm[0].code, "E-VERIFY-CALL");
}

#[test]
fn call_signature_proof_survives_only_private_callee_body_edits() {
    let (mut module, mut plan) = pair();
    module.functions[0].blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: Vec::new(),
    };
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    module.functions[1].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(43));
    plan.module_fingerprint = module.fingerprint();
    verify(&mut session, &module, &plan, &limits).unwrap();
    assert_eq!(session.stats().function_proofs_reused, 1);
    // The unchanged caller has Call, Constant, Binary and Store receipts. Its
    // Load still takes the complete context-sensitive path.
    assert_eq!(session.stats().instruction_proofs_reused, 4);
    module.functions[1].environment = Some(ClosureId(0));
    plan.module_fingerprint = module.fingerprint();
    let fresh = verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap_err();
    let warm = verify(&mut session, &module, &plan, &limits).unwrap_err();
    assert_eq!(warm, fresh);
}

#[test]
fn insufficient_remaining_work_falls_back_at_the_original_check_position() {
    let (module, plan) = crate::tests::baseline();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let work = session.flows[&FunctionId(0)].work;
    assert!(work > 0);
    let function = &module.functions[0];
    let mut definitions = vec![None; function.values.len()];
    for (index, instruction) in function.blocks[0].instructions.iter().enumerate() {
        if let Some(result) = instruction.result {
            definitions[result.0 as usize] = Some((0, index));
        }
    }
    let edges = vec![Vec::new()];
    let parameters = vec![false; function.locals.len()];
    let inputs = FlowInputs {
        function,
        plan: &plan.functions[0],
        predecessors: &edges,
        successors: &edges,
        definitions: &definitions,
        parameters: &parameters,
        addressed_sums: false,
        mixed_address_sums: false,
    };
    let mut fresh_budget = Budget::new(&limits);
    fresh_budget.remaining = work - 1;
    let fresh = check_flow(inputs, &mut fresh_budget, &limits)
        .err()
        .unwrap();
    let mut attempt = Attempt {
        previous: &session.flows,
        previous_empty_borrows: &session.empty_borrows,
        previous_inputs: session.verified.as_ref(),
        next: BTreeMap::new(),
        next_empty_borrows: BTreeMap::new(),
        bound_plans: BTreeMap::new(),
        bound_closures: None,
        bound_functions: BTreeMap::new(),
        stats: VerifyStats::default(),
    };
    let mut warm_budget = Budget::new(&limits);
    warm_budget.remaining = work - 1;
    let warm = attempt
        .check_flow(inputs, &module.target, &mut warm_budget, &limits)
        .err()
        .unwrap();
    assert_eq!(warm, fresh);
    assert_eq!(warm_budget.remaining, fresh_budget.remaining);
    assert_eq!(attempt.stats.function_proofs_checked, 1);
    assert_eq!(attempt.stats.function_proofs_reused, 0);
}

#[test]
fn program_receipt_binds_current_task_interfaces_not_just_embedded_module() {
    let (module, plan) = pair();
    let target = module.target.clone();
    let mut program = Arc::new(TypedProgram {
        module,
        interfaces: Vec::new(),
        task_interfaces: Vec::new(),
        root_functions: Vec::new(),
    });
    let plan = Arc::new(plan);
    let limits = Limits::default();
    let mut session = VerifySession::new();
    session
        .verify_shared_program_module(program.clone(), plan.clone(), &target, &limits)
        .unwrap();
    session
        .verify_shared_program_module(program.clone(), plan.clone(), &target, &limits)
        .unwrap();
    assert_eq!(session.stats().full_receipts_reused, 1);
    Arc::make_mut(&mut program)
        .task_interfaces
        .push(TaskInterface {
            module: zil_base::ModuleId(0),
            exports: vec![TaskExport {
                name: "forged".into(),
                worker: FunctionId(0),
                parameter: (PassingMode::Infer, Type::Unit),
                result: (PassingMode::Infer, Type::I32),
                abi_version: 1,
            }],
        });
    let fresh =
        verify_shared_program_module(program.clone(), plan.clone(), &target, &limits).unwrap_err();
    let warm = session
        .verify_shared_program_module(program, plan, &target, &limits)
        .unwrap_err();
    assert_eq!(warm, fresh);
    assert_eq!(session.stats().full_receipts_reused, 0);
    assert_eq!(session.stats().function_proofs_reused, 2);
}

#[test]
fn exact_work_boundary_and_failed_retry_match_fresh_verification() {
    let (module, plan) = pair();
    let mut limits = Limits::default();
    let mut low = 0;
    let mut high = limits.max_analysis_steps;
    while low + 1 < high {
        limits.max_analysis_steps = low + (high - low) / 2;
        if verify_module(module.clone(), plan.clone(), &module.target, &limits).is_ok() {
            high = limits.max_analysis_steps;
        } else {
            low = limits.max_analysis_steps;
        }
    }
    limits.max_analysis_steps = high;
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    verify(&mut session, &module, &plan, &limits).unwrap();
    assert_eq!(session.stats().function_proofs_reused, 2);
    limits.max_analysis_steps = high - 1;
    let fresh = verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap_err();
    let warm = verify(&mut session, &module, &plan, &limits).unwrap_err();
    assert_eq!(warm, fresh);
    limits.max_analysis_steps = high;
    verify(&mut session, &module, &plan, &limits).unwrap();
    assert_eq!(session.stats().function_proofs_reused, 2);
}

#[test]
fn failed_epoch_does_not_publish_partial_function_proofs_or_replace_bindings() {
    let (mut module, mut plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    verify(&mut session, &module, &plan, &limits).unwrap();
    let checkpoint = session.clone();
    module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(99));
    plan.functions[1].blocks[0].initialized_in.push(LocalId(0));
    plan.module_fingerprint = module.fingerprint();
    assert!(verify(&mut session, &module, &plan, &limits).is_err());
    assert!(Arc::ptr_eq(&session.flows, &checkpoint.flows));
    assert!(same_allocations(
        &session.verified.as_ref().unwrap().inputs,
        &checkpoint.verified.as_ref().unwrap().inputs,
    ));
    plan.functions[1].blocks[0].initialized_in.clear();
    verify(&mut session, &module, &plan, &limits).unwrap();
    assert_eq!(session.stats().function_proofs_checked, 1);
    assert_eq!(session.stats().function_proofs_reused, 1);
}

#[test]
fn shared_body_mutation_cannot_retain_the_old_instruction_or_flow_authority() {
    let (mut module, mut plan) = crate::tests::baseline();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    let verified = verify(&mut session, &module, &plan, &limits).unwrap();
    assert!(
        module.functions[0]
            .values
            .ptr_eq(&verified.module().functions[0].values)
    );
    assert!(
        module.functions[0]
            .blocks
            .ptr_eq(&verified.module().functions[0].blocks)
    );
    module.functions[0].values[0] = Type::Bool;
    assert!(
        !module.functions[0]
            .values
            .ptr_eq(&verified.module().functions[0].values)
    );
    assert!(
        module.functions[0]
            .blocks
            .ptr_eq(&verified.module().functions[0].blocks)
    );
    plan.module_fingerprint = module.fingerprint();
    let fresh = verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap_err();
    let warm = verify(&mut session, &module, &plan, &limits).unwrap_err();
    assert_eq!(warm, fresh);
    assert_eq!(warm[0].code, "E-VERIFY-TYPE");
    assert_eq!(session.stats().instruction_proofs_reused, 0);
    assert_eq!(session.stats().function_proofs_reused, 0);
}

mod empty_borrows;

#[test]
fn shared_candidate_columns_preserve_verified_bytes_without_proving_current_cfgs() {
    let (module, plan) = pair();
    let module = Arc::new(module);
    let plan = Arc::new(plan);
    let limits = Limits::default();
    let target = module.target.clone();
    let mut session = VerifySession::new();
    let sealed = session
        .verify_shared_module(module.clone(), plan.clone(), &target, &limits)
        .unwrap();
    let before =
        zil_ir::wire::encode_module_candidate(sealed.module(), sealed.plan(), &limits).unwrap();
    let copied = Arc::new((*plan).clone());
    for (old, current) in plan.functions.iter().zip(&copied.functions) {
        assert!(old.blocks.ptr_eq(&current.blocks));
        assert!(old.holds.ptr_eq(&current.holds));
    }
    session
        .verify_shared_module(module.clone(), copied.clone(), &target, &limits)
        .unwrap();
    assert_eq!(session.stats().full_receipts_reused, 0);
    assert_eq!(session.stats().function_proofs_reused, 2);

    let mut false_claim = (*copied).clone();
    false_claim.functions[0].blocks[0]
        .initialized_in
        .push(LocalId(0));
    assert!(
        !false_claim.functions[0]
            .blocks
            .ptr_eq(&copied.functions[0].blocks)
    );
    assert!(
        false_claim.functions[0]
            .holds
            .ptr_eq(&copied.functions[0].holds)
    );
    let fresh =
        verify_module((*module).clone(), false_claim.clone(), &target, &limits).unwrap_err();
    let actual = session
        .verify_shared_module(module.clone(), Arc::new(false_claim), &target, &limits)
        .unwrap_err();
    assert_eq!(actual, fresh);
    assert_eq!(
        before,
        zil_ir::wire::encode_module_candidate(sealed.module(), sealed.plan(), &limits).unwrap()
    );

    let mut false_hold = (*copied).clone();
    false_hold.functions[0].holds[0]
        .roots
        .push(HoldRoot::Local(LocalId(0)));
    assert!(
        false_hold.functions[0]
            .blocks
            .ptr_eq(&copied.functions[0].blocks)
    );
    assert!(
        !false_hold.functions[0]
            .holds
            .ptr_eq(&copied.functions[0].holds)
    );
    let fresh = verify_module((*module).clone(), false_hold.clone(), &target, &limits).unwrap_err();
    assert_eq!(
        session
            .verify_shared_module(module.clone(), Arc::new(false_hold), &target, &limits)
            .unwrap_err(),
        fresh
    );
    assert_eq!(
        before,
        zil_ir::wire::encode_module_candidate(sealed.module(), sealed.plan(), &limits).unwrap()
    );

    // Both candidate columns remain shared with the old successful claim, but
    // the same-ID current raw function no longer initializes its local.
    let mut broken = (*module).clone();
    broken.functions[0].blocks[0].instructions[3].kind = InstructionKind::StorageDead(LocalId(0));
    let mut stale = (*copied).clone();
    stale.module_fingerprint = broken.fingerprint();
    assert!(
        stale.functions[0]
            .blocks
            .ptr_eq(&copied.functions[0].blocks)
    );
    assert!(stale.functions[0].holds.ptr_eq(&copied.functions[0].holds));
    let fresh = verify_module(broken.clone(), stale.clone(), &target, &limits).unwrap_err();
    assert_eq!(
        session
            .verify_shared_module(Arc::new(broken), Arc::new(stale), &target, &limits)
            .unwrap_err(),
        fresh
    );
    assert_eq!(
        before,
        zil_ir::wire::encode_module_candidate(sealed.module(), sealed.plan(), &limits).unwrap()
    );
}
