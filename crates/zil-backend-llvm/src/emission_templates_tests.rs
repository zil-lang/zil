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

fn shared_callers(count: u32) -> (Vec<TypedFunction>, BoundaryClaims) {
    let base = fixture().module().functions[0].clone();
    let mut caller = function(
        count,
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
    let functions = (0..2 * count)
        .map(|id| {
            let mut f = if id < count {
                base.clone()
            } else {
                caller.clone()
            };
            f.id = FunctionId(id);
            f.name = format!("shared_{id}");
            f.span.start = id as usize;
            f.span.end = id as usize + 1;
            f
        })
        .collect();
    let claims = (0..2 * count)
        .map(|id| vec![(vec![0], vec![0]); if id < count { 3 } else { 1 }])
        .collect();
    (functions, claims)
}

#[test]
fn restart_shared_raw_bodies_skip_representation_admission_and_caller_emission() {
    let (functions, claims) = shared_callers(32);
    let verified = checked(functions, claims);
    let raw = lir(&verified);
    for policy in [false, true] {
        let expected = lower(&verified, &raw, policy, None).unwrap();
        for _ in 0..2 {
            let mut session = crate::emit::incremental::Session::default();
            let actual = lower(&verified, &raw, policy, Some(&mut session)).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                (
                    session.representation_checked,
                    session.representation_reused
                ),
                (2, 62)
            );
            assert_eq!(session.shared_admission_reuses, 62);
            assert_eq!(session.shared_body_template_reuses, 62);
            assert_eq!(
                (
                    session.produced_body_templates,
                    session.reused_body_templates
                ),
                (2, 62)
            );
            let bodies: Vec<_> = actual
                .0
                .items
                .iter()
                .filter_map(|item| {
                    if let zil_llvm_api::model::Item::Function(f) = item
                        && f.name.starts_with("zil.fn.")
                    {
                        Some(f.blocks.as_ref().unwrap())
                    } else {
                        None
                    }
                })
                .collect();
            assert!(bodies[0].ptr_eq(bodies[31]));
            assert!(bodies[32].ptr_eq(bodies[63]));
            // Exercise the actual downstream transform chain with both shared
            // and independently emitted machine bodies, including full checks.
            fn transform(
                mut m: zil_llvm_api::model::Module,
                raw: &LirModule,
            ) -> zil_llvm_api::model::Module {
                // Same body owner, different current optimization header. Its
                // NoInline role must survive every class/purity consumer.
                for item in &mut m.items {
                    if let zil_llvm_api::model::Item::Function(f) = item
                        && f.name == "zil.fn.7"
                    {
                        f.attrs.push(zil_llvm_api::model::Attr::NoInline);
                    }
                }
                let mut preflight = crate::machine_preflight::CurrentPreflight::default();
                let protected = std::collections::BTreeSet::new();
                crate::machine_integer_ranges::fold(&mut m, &protected, &mut preflight);
                crate::machine_checked_add_chain::fold_checked_add_chains_cached_with_preflight(
                    &mut m,
                    &mut Default::default(),
                    &mut preflight,
                );
                crate::machine_scalar::fold_known_leaf_calls_with_preflight(
                    &mut m,
                    &raw.as_view(),
                    &mut preflight,
                );
                preflight.check(&m).unwrap();
                crate::machine_call_classes::canonicalize_stable_cached(
                    &mut m,
                    &protected,
                    &|| false,
                    None,
                )
                .unwrap();
                crate::machine_integer_ranges::mark_purity(&mut m, &protected, &mut preflight);
                zil_llvm_api::validate_module(&m).unwrap();
                assert!(m.items.iter().any(|item| {
                    matches!(item, zil_llvm_api::model::Item::Function(f)
                        if f.name == "zil.fn.7" && f.attrs.contains(&zil_llvm_api::model::Attr::NoInline))
                }));
                m
            }
            assert_eq!(
                transform(actual.0, &raw),
                transform(expected.0.clone(), &raw)
            );
        }
    }
}

#[test]
fn shared_caller_templates_keep_actual_call_ids_arguments_and_current_callee_facts() {
    let (mut functions, claims) = shared_callers(3);
    // COW on one caller and one leaf must split both admission and emission.
    functions[1].blocks[2].instructions[0].kind = InstructionKind::Constant(Constant::I32(23));
    functions[4].blocks[0].instructions[1].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: vec![ValueId(0)],
    };
    let first = checked(functions.clone(), claims.clone());
    let mut session = crate::emit::incremental::Session::default();
    for policy in [false, true] {
        let raw = lir(&first);
        assert_eq!(
            lower(&first, &raw, policy, Some(&mut session)),
            lower(&first, &raw, policy, None)
        );
    }
    // A new request cannot carry a previous callee's result through a template.
    functions[1].blocks[2].instructions[0].kind = InstructionKind::Constant(Constant::I32(41));
    let changed = checked(functions.clone(), claims.clone());
    let raw = lir(&changed);
    assert_eq!(
        lower(&changed, &raw, true, Some(&mut session)),
        lower(&changed, &raw, true, None)
    );
    let mut forged = raw.clone();
    forged.functions[5].blocks[0].instructions[1].kind = InstructionKind::Call {
        function: FunctionId(1),
        arguments: vec![ValueId(0)],
    };
    let actual = lower(&changed, &forged, true, Some(&mut session));
    assert_eq!(actual, lower(&changed, &forged, true, None));
    assert_eq!(actual.unwrap_err()[0].code, "E-CODEGEN-UNBOUND-LIR");
    assert_eq!(session.shared_admission_reuses, 0);
    assert_eq!(session.shared_body_template_reuses, 0);
    // Rebind an actual callee ABI and its only caller together. The remaining
    // siblings still own the original I32 bodies; no class can alias these.
    functions[1].return_type = Type::I64;
    functions[1].locals[0].ty = Type::I64;
    functions[1].values = vec![Type::I64, Type::I64, Type::Bool, Type::I64, Type::I64].into();
    functions[1].blocks[0].instructions[1].kind = InstructionKind::Constant(Constant::I64(0));
    functions[1].blocks[1].instructions[0].kind = InstructionKind::Constant(Constant::I64(7));
    functions[1].blocks[2].instructions[0].kind = InstructionKind::Constant(Constant::I64(41));
    functions[4].return_type = Type::I64;
    functions[4].locals[0].ty = Type::I64;
    functions[4].values = vec![Type::I64, Type::I64].into();
    let changed = checked(functions, claims);
    let raw = lir(&changed);
    for policy in [false, true] {
        assert_eq!(
            lower(&changed, &raw, policy, Some(&mut session)),
            lower(&changed, &raw, policy, None)
        );
    }
}

#[test]
fn cold_5105_current_functions_produce_only_two_shared_body_templates() {
    let (mut functions, mut claims) = shared_callers(2552);
    let mut last = functions[2552].clone();
    last.id = FunctionId(5104);
    last.name = "last_current_caller".into();
    functions.push(last);
    claims.push(vec![(vec![0], vec![0])]);
    let verified = checked(functions, claims);
    let raw = lir(&verified);
    let mut session = crate::emit::incremental::Session::default();
    assert_eq!(
        lower(&verified, &raw, true, Some(&mut session)),
        lower(&verified, &raw, true, None)
    );
    assert_eq!((session.hits, session.misses), (0, 5105));
    assert_eq!(
        (
            session.produced_body_templates,
            session.reused_body_templates
        ),
        (2, 5103)
    );
    assert_eq!(session.shared_body_template_reuses, 5103);
    assert_eq!(session.shared_admission_reuses, 5103);
    assert_eq!(
        (
            session.representation_checked,
            session.representation_reused
        ),
        (2, 5103)
    );
}

#[test]
fn restart_shared_planning_preserves_every_small_budget_boundary() {
    let (functions, claims) = shared_callers(3);
    let verified = checked(functions, claims);
    let raw = lir(&verified);
    let used =
        Limits::default().max_analysis_steps - lower(&verified, &raw, false, None).unwrap().2;
    // Includes failures in later siblings, not just before the first producer.
    // Each restart is cold: no per-ID history can hide same-request replay.
    for budget in 0..=used + 1 {
        let mut session = crate::emit::incremental::Session::default();
        let expected = crate::emit::incremental::with_initial_work(budget, || {
            lower(&verified, &raw, false, None)
        });
        let actual = crate::emit::incremental::with_initial_work(budget, || {
            lower(&verified, &raw, false, Some(&mut session))
        });
        assert_eq!(actual, expected, "budget {budget}");
    }
}
