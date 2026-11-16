use super::*;

fn pair() -> (TypedModule, CandidatePlan) {
    let (mut module, mut plan) = crate::tests::baseline();
    let mut sibling = module.functions[0].clone();
    sibling.id = FunctionId(1);
    sibling.name = "second-current-leaf".into();
    let shift = |s: &mut Span| {
        s.file = zil_base::FileId(9);
        s.start += 100;
        s.end += 100;
    };
    shift(&mut sibling.span);
    for l in &mut sibling.locals {
        shift(&mut l.span);
    }
    for b in &mut sibling.blocks {
        for i in &mut b.instructions {
            shift(&mut i.span);
        }
    }
    module.functions.push(sibling);
    let mut p = plan.functions[0].clone();
    p.function = FunctionId(1);
    plan.functions.push(p);
    (module, plan)
}

fn product(
    session: &mut VerifySession,
    module: &TypedModule,
    plan: &CandidatePlan,
    limits: &Limits,
) -> CompileResult<VerifiedModule> {
    let mut plan = plan.clone();
    plan.module_fingerprint = module.structural_fingerprint();
    session.verify_product_program_module(
        Arc::new(TypedProgram {
            module: module.clone(),
            interfaces: vec![],
            task_interfaces: vec![],
            root_functions: vec![],
        }),
        Arc::new(plan),
        &module.target,
        limits,
    )
}

#[test]
fn fresh_success_shares_only_local_proof_and_seals_current_complete_inputs() {
    let (module, plan) = pair();
    let limits = Limits::default();
    let mut session = VerifySession::new();
    for _ in 0..2 {
        let checked = product(&mut session, &module, &plan, &limits).unwrap();
        assert_eq!(checked.module(), &module);
        assert_eq!(checked.plan().functions, plan.functions);
        assert_eq!(session.stats().full_receipts_reused, 0);
        assert_eq!(session.stats().function_proofs_checked, 1);
        assert_eq!(session.stats().function_proofs_reused, 1);
    }
    // Restart has exactly the same independent fresh-success requirement.
    let mut restarted = VerifySession::new();
    product(&mut restarted, &module, &plan, &limits).unwrap();
    assert_eq!(restarted.stats().function_proofs_checked, 1);
    assert_eq!(restarted.stats().function_proofs_reused, 1);
}

#[test]
fn current_bad_body_plan_and_headers_match_the_ordinary_checker() {
    let limits = Limits::default();
    for case in 0..7 {
        let (mut m, mut p) = pair();
        match case {
            0 => {
                m.functions[1].blocks[0].instructions[2].kind = InstructionKind::Binary {
                    op: BinaryOp::Add,
                    left: ValueId(99),
                    right: ValueId(1),
                }
            }
            1 => p.functions[1].blocks[0].initialized_in.push(LocalId(0)),
            2 => p.functions[1].holds[0]
                .roots
                .push(HoldRoot::Local(LocalId(0))),
            3 => p.functions[1].function = FunctionId(0),
            4 => m.functions[1].id = FunctionId(0),
            5 => m.functions[1].name = m.functions[0].name.clone(),
            6 => m.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(99))),
            _ => unreachable!(),
        }
        let actual = product(&mut VerifySession::new(), &m, &p, &limits).unwrap_err();
        p.module_fingerprint = m.fingerprint();
        let expected = verify_module(m.clone(), p, &m.target, &limits).unwrap_err();
        assert_eq!(actual, expected, "counterexample {case}");
    }
}

#[test]
fn relative_span_mismatch_declines_template_and_changed_target_is_checked() {
    let (mut m, p) = pair();
    // A diagnostic span can be valid while no longer matching the template.
    m.functions[1].blocks[0].instructions[0].span.end = 100;
    let mut session = VerifySession::new();
    product(&mut session, &m, &p, &Limits::default()).unwrap();
    assert_eq!(session.stats().function_proofs_checked, 2);
    assert_eq!(session.stats().function_proofs_reused, 0);
    m.target.checked_arithmetic = false;
    let error = product(&mut session, &m, &p, &Limits::default()).unwrap_err();
    assert_eq!(error[0].code, "E-VERIFY-ARITHMETIC");
}

#[test]
fn exact_fee_and_one_less_preserve_the_ordinary_first_failure() {
    let (m, p) = pair();
    let limits = Limits::default();
    let mut memo = Templates::default();
    let mut first = Budget::new(&limits);
    memo.check(
        &m,
        &m.functions[0],
        &p.functions[0],
        &mut first,
        &limits,
        None,
    )
    .unwrap();
    let fee = limits.max_analysis_steps - first.remaining;
    assert!(fee > 0);
    for available in [fee, fee - 1] {
        let mut warm = Budget::new(&limits);
        warm.remaining = available;
        let actual = memo.check(
            &m,
            &m.functions[1],
            &p.functions[1],
            &mut warm,
            &limits,
            None,
        );
        let mut cold = Budget::new(&limits);
        cold.remaining = available;
        let expected = check_function_with_reuse(
            &m,
            &m.functions[1],
            &p.functions[1],
            &mut cold,
            &limits,
            None,
        );
        assert_eq!(warm.remaining, cold.remaining);
        match (actual, expected) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.successors, b.successors);
                assert_eq!(a.predecessors, b.predecessors);
                assert_eq!(a.reachable, b.reachable);
                assert_eq!(a.role_data_uses, b.role_data_uses);
            }
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("template changed the fee boundary"),
        }
    }
}

#[test]
fn changed_world_limits_and_nonleaf_work_cannot_use_the_local_receipt() {
    let (m, p) = pair();
    let limits = Limits::default();
    let changed_limits = Limits {
        max_ct_steps: limits.max_ct_steps - 1,
        ..limits.clone()
    };
    let other = m.clone();
    let mut memo = Templates::default();
    memo.check(
        &m,
        &m.functions[0],
        &p.functions[0],
        &mut Budget::new(&limits),
        &limits,
        None,
    )
    .unwrap();
    // Distinct current World, even with equal bytes, resets request-local rows.
    memo.check(
        &other,
        &other.functions[1],
        &p.functions[1],
        &mut Budget::new(&limits),
        &limits,
        None,
    )
    .unwrap();
    assert!(memo.rows.values().all(|r| r.function.id == FunctionId(1)));
    memo.check(
        &other,
        &other.functions[0],
        &p.functions[0],
        &mut Budget::new(&changed_limits),
        &changed_limits,
        None,
    )
    .unwrap();
    assert!(memo.rows.values().all(|r| r.function.id == FunctionId(0)));
    let mut nonleaf = m.functions[1].clone();
    nonleaf.blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: vec![],
    };
    assert!(selector(&nonleaf, &p.functions[1], &Budget::new(&limits)).is_none());
    nonleaf = m.functions[1].clone();
    nonleaf.reference_return_bound = Some(vec![]);
    assert!(selector(&nonleaf, &p.functions[1], &Budget::new(&limits)).is_none());
}
