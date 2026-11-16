use super::*;

fn fixture() -> VerifiedModule {
    let mut leaf = function(
        0,
        "first",
        Type::I32,
        vec![Type::I32, Type::I32, Type::Bool, Type::I32, Type::I32],
        vec![
            block(
                0,
                vec![
                    load(0, 0),
                    constant(1, Constant::I32(0)),
                    binary(2, BinaryOp::Less, 0, 1),
                ],
                Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(
                1,
                vec![constant(3, Constant::I32(7))],
                Terminator::Return(Some(ValueId(3))),
            ),
            block(
                2,
                vec![constant(4, Constant::I32(9))],
                Terminator::Return(Some(ValueId(4))),
            ),
        ],
    );
    leaf.parameters = vec![LocalId(0)];
    leaf.locals = vec![local(0, Type::I32, false)].into();
    let mut second = leaf.clone();
    second.id = FunctionId(1);
    second.name = "different_source_name".into();
    second.span.start = 123;
    second.span.end = 124;
    second.locals[0].span = second.span;
    for block in &mut second.blocks {
        for instruction in &mut block.instructions {
            instruction.span = second.span;
        }
    }
    let mut changed = leaf.clone();
    changed.id = FunctionId(2);
    changed.name = "changed_literal".into();
    changed.blocks[2].instructions[0].kind = InstructionKind::Constant(Constant::I32(11));
    checked(
        vec![leaf, second, changed],
        vec![vec![(vec![0], vec![0]); 3]; 3],
    )
}

fn lower(
    verified: &VerifiedModule,
    raw: &LirModule,
    policy: bool,
    session: Option<&mut crate::emit::incremental::Session>,
) -> zil_base::CompileResult<(zil_llvm_api::model::Module, bool, usize)> {
    crate::emit::lower_output_with_session(
        verified,
        &raw.as_view(),
        &EmitOptions {
            entry: EntryPoint::None,
        },
        policy,
        false,
        1,
        session,
        false,
    )
    .map(|(machine, collected, work)| (machine.to_module(), collected, work))
}

#[test]
fn restart_templates_match_fresh_multi_block_emission_and_changed_literals() {
    let verified = fixture();
    let raw = lir(&verified);
    for policy in [false, true] {
        let expected = lower(&verified, &raw, policy, None).unwrap();
        // Independent empty sessions model process restart: no historical raw
        // function, query, machine, transform or partition product is present.
        for _ in 0..2 {
            let mut session = crate::emit::incremental::Session::default();
            let actual = lower(&verified, &raw, policy, Some(&mut session)).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(session.reused_body_templates, 1);
            assert_eq!(session.produced_body_templates, 2);
            if policy {
                assert_eq!(session.source_projection_admission_reuses, 3);
                assert_eq!(session.source_projection_traversals, 0);
                assert_eq!(session.source_projection_instruction_visits, 0);
            }
        }
    }
}

#[test]
fn restart_admission_projection_tracks_actual_current_call_slots() {
    let leafs = fixture();
    let mut functions = leafs.module().functions.iter().cloned().collect::<Vec<_>>();
    let mut caller = function(
        3,
        "caller",
        Type::I32,
        vec![Type::I32, Type::I32],
        vec![block(
            0,
            vec![
                load(0, 0),
                instruction(
                    Some(1),
                    InstructionKind::Call {
                        function: FunctionId(0),
                        arguments: vec![ValueId(0)],
                    },
                ),
            ],
            Terminator::Return(Some(ValueId(1))),
        )],
    );
    caller.parameters = vec![LocalId(0)];
    caller.locals = vec![local(0, Type::I32, false)].into();
    functions.push(caller);
    let mut claims = vec![vec![(vec![0], vec![0]); 3]; 3];
    claims.push(vec![(vec![0], vec![0])]);
    let original = checked(functions.clone(), claims.clone());
    let mut session = crate::emit::incremental::Session::default();
    let raw = lir(&original);
    assert_eq!(
        lower(&original, &raw, true, Some(&mut session)).unwrap(),
        lower(&original, &raw, true, None).unwrap()
    );
    let products = session.products.clone();
    functions[3].blocks[0].instructions[1].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: vec![ValueId(0)],
    };
    let current = checked(functions, claims);
    let raw = lir(&current);
    assert_eq!(
        lower(&current, &raw, true, Some(&mut session)).unwrap(),
        lower(&current, &raw, true, None).unwrap()
    );
    assert_ne!(products, session.products);
    assert_eq!(session.source_projection_admission_reuses, 4);
    assert_eq!(session.source_projection_traversals, 0);
    assert_eq!(session.source_projection_instruction_visits, 0);
}

#[test]
fn restart_templates_preserve_original_budget_errors_and_unbound_lir_rejection() {
    let verified = fixture();
    let raw = lir(&verified);
    let expected = lower(&verified, &raw, false, None).unwrap();
    let used = Limits::default().max_analysis_steps - expected.2;
    for budget in [0, used.saturating_sub(1), used, used.saturating_add(1)] {
        let mut session = crate::emit::incremental::Session::default();
        let expected = crate::emit::incremental::with_initial_work(budget, || {
            lower(&verified, &raw, false, None)
        });
        let actual = crate::emit::incremental::with_initial_work(budget, || {
            lower(&verified, &raw, false, Some(&mut session))
        });
        assert_eq!(actual, expected, "budget {budget}");
    }
    let mut session = crate::emit::incremental::Session::default();
    lower(&verified, &raw, false, Some(&mut session)).unwrap();
    let mut forged = raw;
    forged.functions[1].blocks[2].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(55));
    let actual = lower(&verified, &forged, false, Some(&mut session));
    assert_eq!(actual, lower(&verified, &forged, false, None));
    assert_eq!(actual.unwrap_err()[0].code, "E-CODEGEN-UNBOUND-LIR");
    assert_eq!(session.reused_body_templates, 0);
}
