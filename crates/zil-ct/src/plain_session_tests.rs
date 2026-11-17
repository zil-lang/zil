use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, NodeId};

fn expr(kind: CoreExprKind) -> CoreExpr {
    CoreExpr {
        id: NodeId(50),
        span: Span::new(FileId(0), 10, 20),
        kind,
    }
}
fn function(id: u32, name: &str, value: CoreExpr) -> CoreFunction {
    let span = Span::new(FileId(0), 0, 80);
    CoreFunction {
        storage_life: None,
        call_kind: None,
        life: None,
        type_parameters: vec![],
        state_parameters: vec![],
        protocol_contract: None,
        callable_return: None,
        id: NodeId(id),
        name: name.into(),
        callable_parameters: vec![],
        foreign: None,
        parameters: vec![],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        body: CoreBlock {
            span,
            statements: vec![CoreStatement {
                id: NodeId(100 + id),
                span,
                kind: CoreStatementKind::Return(Some(value)),
            }],
        },
        receiver: None,
        span,
    }
}
fn program() -> CoreProgram {
    let mut callee = function(1, "callee", expr(CoreExprKind::Name("x".into())));
    callee.parameters.push(CoreParameter {
        name: "x".into(),
        ty: Type::I32,
        mode: PassingMode::Infer,
        span: callee.span,
    });
    let functions = vec![
        function(
            0,
            "caller",
            expr(CoreExprKind::Call {
                callee: Box::new(expr(CoreExprKind::Name("callee".into()))),
                arguments: vec![expr(CoreExprKind::Integer("7".into()))],
            }),
        ),
        callee,
        function(2, "sibling", expr(CoreExprKind::Integer("3".into()))),
    ];
    CoreProgram {
        root: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        modules: vec![ProgramModule {
            core: Arc::new(CoreModule {
                dyn_declarations: vec![],
                dyn_interfaces: vec![],
                dyn_vtables: vec![],
                life_declarations: vec![],
                life_instances: vec![],
                closure_life_contexts: vec![],
                schema_version: SCHEMA_VERSION,
                module_id: ModuleId(0),
                source_fingerprint: Fingerprint::ZERO,
                functions: functions.into(),
                aggregates: vec![],
                aggregate_templates: vec![],
                aggregate_applications: vec![],
                enums: vec![],
                unsupported: vec![],
            }),
            imports: vec![],
            exports: vec![],
        }],
    }
}
fn replace_return(program: &mut CoreProgram, function: usize, kind: CoreExprKind) {
    Arc::make_mut(&mut program.modules[0].core).functions[function]
        .body
        .statements[0]
        .kind = CoreStatementKind::Return(Some(expr(kind)));
}
fn charge(limits: &mut Limits, usage: SourceEvaluationUsage, functions: usize) {
    limits.max_nodes = limits.max_nodes.checked_sub(usage.nodes).unwrap();
    limits.max_ct_steps = limits.max_ct_steps.checked_sub(usage.steps).unwrap();
    limits.max_functions = limits.max_functions.checked_sub(functions).unwrap();
}
fn fresh(
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<(CoreProgram, Vec<SourceEvaluationUsage>)> {
    let context = SourceProgramContext::new(program, limits)?;
    let mut remaining = limits.clone();
    let mut result = program.clone();
    let mut usages = Vec::new();
    for (source, output) in program.modules.iter().zip(&mut result.modules) {
        let (core, usage) =
            evaluate_source_constants_with_context(&source.core, target, &remaining, &context)?;
        charge(&mut remaining, usage, core.functions.len());
        output.core = Arc::new(core);
        usages.push(usage);
    }
    Ok((result, usages))
}
fn run(
    session: &mut PlainSourceSession,
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
) -> CompileResult<(CoreProgram, Vec<SourceEvaluationUsage>)> {
    let context = SourceProgramContext::new(program, limits)?;
    if let Some(usages) = session.validate_program(program, target, limits, &context) {
        return Ok((program.clone(), usages));
    }
    fresh(program, target, limits)
}
fn checked(
    session: &mut PlainSourceSession,
    program: &CoreProgram,
    target: &TargetProfile,
    limits: &Limits,
) {
    assert_eq!(
        run(session, program, target, limits),
        fresh(program, target, limits)
    );
}

#[test]
fn fused_cold_shape_costs_match_classifier_and_depth_boundary_declines_safely() {
    let limits = Limits {
        max_call_depth: 256,
        ..Limits::default()
    };
    let target = TargetProfile::default();
    for depth in [1, 127, 128, 129] {
        let mut input = program();
        let mut value = expr(CoreExprKind::Integer("1".into()));
        // Return expressions begin at depth one in both visitors.
        for _ in 1..depth {
            value = expr(CoreExprKind::Unary {
                op: UnaryOp::Negate,
                operand: Box::new(value),
            });
        }
        replace_return(&mut input, 2, value.kind);
        let mut session = PlainSourceSession::default();
        checked(&mut session, &input, &target, &limits);
        if depth > 128 {
            assert_eq!(session.stats.fallbacks, 1);
            assert!(session.current.is_none());
        } else {
            assert_eq!(session.stats.fallbacks, 0);
            let epoch = session.current.as_ref().unwrap();
            let mut fuel = limits.max_nodes.min(1_000_000);
            let mut stack = Vec::new();
            for (index, function) in input.modules[0].core.functions.iter().enumerate() {
                let before = fuel;
                unchanged_function_shape(function, &limits, &mut fuel, &mut stack).unwrap();
                assert_eq!(epoch.modules[0].functions[index].shape_cost, before - fuel);
            }
        }
    }
}

#[test]
fn fused_lookup_observation_includes_inactive_branch_misses_and_original_errors() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut input = program();
    replace_return(
        &mut input,
        0,
        CoreExprKind::If {
            condition: Box::new(expr(CoreExprKind::Bool(false))),
            then_value: Box::new(expr(CoreExprKind::Call {
                callee: Box::new(expr(CoreExprKind::Name("missing".into()))),
                arguments: vec![expr(CoreExprKind::Integer("7".into()))],
            })),
            else_value: Box::new(expr(CoreExprKind::Integer("3".into()))),
        },
    );
    let mut session = PlainSourceSession::default();
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        session.current.as_ref().unwrap().modules[0].functions[0]
            .hints
            .as_ref(),
        &[(ModuleId(0), "missing".into())]
    );
    Arc::make_mut(&mut input.modules[0].core).functions[1].name = "missing".into();
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (2, 1)
    );

    // A later unsupported child must not hide an earlier evaluator error.
    replace_return(
        &mut input,
        0,
        CoreExprKind::IntegerConvert {
            target: Type::F32,
            value: Box::new(expr(CoreExprKind::Integer("7".into()))),
        },
    );
    replace_return(
        &mut input,
        2,
        CoreExprKind::CompileTime(Box::new(expr(CoreExprKind::Integer("4".into())))),
    );
    let expected = fresh(&input, &target, &limits).unwrap_err();
    assert_eq!(
        run(&mut session, &input, &target, &limits).unwrap_err(),
        expected
    );
    assert_eq!(session.stats.fallbacks, 1);
}

#[test]
fn unchanged_body_consumers_reuse_only_exact_inputs_under_current_declarations() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut session = PlainSourceSession::default();
    let initial = program();
    checked(&mut session, &initial, &target, &limits);
    assert_eq!(session.stats.body_checks, 3);
    assert_eq!(session.stats.shape_checks, 3);
    checked(&mut session, &initial, &target, &limits);
    assert_eq!(session.stats.body_reuses, 3);
    assert_eq!(session.stats.body_checks, 0);
    assert_eq!(
        session.stats.declaration_modules, 1,
        "global checks still execute"
    );
    let mut edited = initial.clone();
    replace_return(&mut edited, 2, CoreExprKind::Integer("4".into()));
    // Public diagnostic fingerprints can remain equal and never authorize reuse.
    checked(&mut session, &edited, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (1, 2)
    );
    assert_eq!(
        (session.stats.shape_checks, session.stats.shape_reuses),
        (1, 2)
    );
    let mut independent = edited.clone();
    let f = independent.modules[0].core.functions[0].clone();
    Arc::make_mut(&mut independent.modules[0].core)
        .functions
        .replace(0, f);
    checked(&mut session, &independent, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (1, 2),
        "a separately materialized equal body must actually execute this round"
    );
}

#[test]
fn changed_callee_header_and_duplicate_resolution_invalidate_body_fees() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let initial = program();
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &limits);
    let mut changed = initial.clone();
    Arc::make_mut(&mut changed.modules[0].core).functions[1].parameters[0].ty = Type::I64;
    checked(&mut session, &changed, &target, &limits);
    assert_eq!(
        (session.stats.context_reuses, session.stats.body_reuses),
        (1, 1)
    );
    assert_eq!(session.stats.body_checks, 2);
    let mut duplicate = initial.clone();
    Arc::make_mut(&mut duplicate.modules[0].core).functions[2].name = "callee".into();
    let ordinary = fresh(&initial, &target, &limits).unwrap().1;
    let ambiguous = fresh(&duplicate, &target, &limits).unwrap().1;
    assert_ne!(
        ordinary, ambiguous,
        "the real call-hint consumer observes ambiguity"
    );
    checked(&mut session, &duplicate, &target, &limits);
    assert_eq!(session.stats.context_reuses, 1);
    assert_eq!(session.stats.body_reuses, 0);
}

#[test]
fn negative_hint_reads_follow_add_ambiguity_delete_and_reorder() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut input = program();
    replace_return(
        &mut input,
        0,
        CoreExprKind::Call {
            callee: Box::new(expr(CoreExprKind::Name("future".into()))),
            arguments: vec![expr(CoreExprKind::Integer("7".into()))],
        },
    );
    let mut session = PlainSourceSession::default();
    checked(&mut session, &input, &target, &limits);
    let mut future = input.modules[0].core.functions[1].clone();
    future.name = "future".into();
    future.id = NodeId(3);
    Arc::make_mut(&mut input.modules[0].core)
        .functions
        .push(future.clone());
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (2, 2)
    );
    future.id = NodeId(4);
    Arc::make_mut(&mut input.modules[0].core)
        .functions
        .push(future);
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (2, 3)
    );
    Arc::make_mut(&mut input.modules[0].core)
        .functions
        .truncate(4);
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (1, 3)
    );
    Arc::make_mut(&mut input.modules[0].core)
        .functions
        .truncate(3);
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (1, 2)
    );
    Arc::make_mut(&mut input.modules[0].core)
        .functions
        .reverse();
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (3, 0)
    );
}

#[test]
fn lexical_shadowing_excludes_unread_headers_and_imported_hints_stay_current() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    for parameter in [false, true] {
        let mut input = program();
        let caller = &mut Arc::make_mut(&mut input.modules[0].core).functions[0];
        if parameter {
            caller.parameters.push(CoreParameter {
                name: "callee".into(),
                ty: Type::I32,
                mode: PassingMode::Infer,
                span: caller.span,
            });
        } else {
            caller.body.statements.insert(
                0,
                CoreStatement {
                    id: NodeId(900),
                    span: caller.span,
                    kind: CoreStatementKind::Binding {
                        name: "callee".into(),
                        mutable: false,
                        ty: Some(Type::I32),
                        initializer: Some(expr(CoreExprKind::Integer("1".into()))),
                    },
                },
            );
        }
        let mut session = PlainSourceSession::default();
        checked(&mut session, &input, &target, &limits);
        Arc::make_mut(&mut input.modules[0].core).functions[1].parameters[0].ty = Type::I64;
        checked(&mut session, &input, &target, &limits);
        assert_eq!(
            (session.stats.body_checks, session.stats.body_reuses),
            (1, 2)
        );
    }

    let mut input = program();
    let mut dependency = input.modules[0].clone();
    Arc::make_mut(&mut dependency.core).module_id = ModuleId(1);
    input.modules.push(dependency);
    input.modules[0].imports.push(CoreImport {
        alias: "dep".into(),
        module: ModuleId(1),
        span: Span::default(),
    });
    replace_return(
        &mut input,
        0,
        CoreExprKind::Call {
            callee: Box::new(expr(CoreExprKind::Member {
                object: Box::new(expr(CoreExprKind::Name("dep".into()))),
                member: "callee".into(),
                owner: None,
            })),
            arguments: vec![expr(CoreExprKind::Integer("7".into()))],
        },
    );
    let mut session = PlainSourceSession::default();
    checked(&mut session, &input, &target, &limits);
    Arc::make_mut(&mut input.modules[1].core).functions[1].parameters[0].ty = Type::I64;
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        (session.stats.body_checks, session.stats.body_reuses),
        (3, 3)
    );
    input.modules[0].imports[0].alias = "missing".into();
    checked(&mut session, &input, &target, &limits);
    assert_eq!(
        session.stats.body_reuses, 0,
        "current imports remain a full binding"
    );
}

#[test]
fn wrong_live_context_target_or_full_limits_cannot_authorize_replay() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let initial = program();
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &limits);
    let owner = session.current.clone().unwrap();
    let other = initial.clone();
    let other_context = SourceProgramContext::new(&other, &limits).unwrap();
    assert!(
        session
            .validate_program(&initial, &target, &limits, &other_context)
            .is_none()
    );
    assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &owner));
    let different_limit = Limits {
        max_source_bytes: limits.max_source_bytes - 1,
        ..limits.clone()
    };
    let original_context = SourceProgramContext::new(&initial, &limits).unwrap();
    assert!(
        session
            .validate_program(&initial, &target, &different_limit, &original_context)
            .is_none()
    );
    checked(&mut session, &initial, &target, &different_limit);
    assert_eq!(session.stats.body_reuses, 0);
    let mut other_target = target.clone();
    other_target.triple.push_str("-other");
    checked(&mut session, &initial, &other_target, &different_limit);
    assert_eq!(session.stats.body_reuses, 0);
}

#[test]
fn body_error_and_ct_rewrite_do_not_publish_a_plain_epoch() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let initial = program();
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &limits);
    let owner = session.current.clone().unwrap();
    let mut invalid = initial.clone();
    replace_return(
        &mut invalid,
        2,
        CoreExprKind::IntegerConvert {
            value: Box::new(expr(CoreExprKind::Integer("4".into()))),
            target: Type::F32,
        },
    );
    let failure = run(&mut session, &invalid, &target, &limits);
    assert!(failure.is_err());
    assert_eq!(failure, fresh(&invalid, &target, &limits));
    assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &owner));
    let mut ct = initial.clone();
    replace_return(
        &mut ct,
        2,
        CoreExprKind::CompileTime(Box::new(expr(CoreExprKind::Integer("4".into())))),
    );
    checked(&mut session, &ct, &target, &limits);
    assert_eq!(session.stats.fallbacks, 1);
    assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &owner));
    checked(&mut session, &initial, &target, &limits);
    assert_eq!(session.stats.body_reuses, 3);
}

#[test]
fn exact_n_and_n_minus_one_preserve_the_original_first_error_and_recovery() {
    let target = TargetProfile::default();
    let initial = program();
    let defaults = Limits::default();
    let mut low = 0;
    let mut high = defaults.max_analysis_steps;
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..defaults.clone()
        };
        if fresh(&initial, &target, &limits).is_ok() {
            high = middle;
        } else {
            low = middle;
        }
    }
    let exact = Limits {
        max_analysis_steps: high,
        ..defaults.clone()
    };
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &exact);
    checked(&mut session, &initial, &target, &exact);
    assert_eq!(session.stats.body_reuses, 3);
    let owner = session.current.clone().unwrap();
    let insufficient = Limits {
        max_analysis_steps: high - 1,
        ..defaults
    };
    let failure = run(&mut session, &initial, &target, &insufficient);
    assert!(failure.is_err());
    assert_eq!(failure, fresh(&initial, &target, &insufficient));
    assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &owner));
    checked(&mut session, &initial, &target, &exact);
    assert_eq!(session.stats.body_reuses, 3);
}

#[test]
fn changed_prefix_usage_debits_reused_bodies_from_current_allowance() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut initial = program();
    let mut next = initial.modules[0].clone();
    Arc::make_mut(&mut next.core).module_id = ModuleId(1);
    initial.modules.push(next);
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &limits);
    let mut edited = initial.clone();
    let first = &mut Arc::make_mut(&mut edited.modules[0].core).functions[0];
    first.body.statements.push(CoreStatement {
        id: NodeId(500),
        span: first.span,
        kind: CoreStatementKind::Expression(expr(CoreExprKind::Integer("9".into()))),
    });
    checked(&mut session, &edited, &target, &limits);
    assert_eq!(
        session.stats.body_reuses, 5,
        "successful fees must be debited from current, never restored old, allowances"
    );
    assert_eq!(session.stats.body_checks, 1);
}

#[test]
fn checkpoint_lifetime_and_attachment_require_complete_actual_module_owners() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let initial = program();
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &limits);
    let weak = Arc::downgrade(session.current.as_ref().unwrap());
    let checkpoint = session.checkpoint_for(&initial).unwrap();
    let mut unrelated = initial.clone();
    unrelated.modules[0].core = Arc::new(initial.modules[0].core.as_ref().clone());
    assert!(
        session.checkpoint_for(&unrelated).is_none(),
        "equal public values/fingerprints cannot attach a private raw epoch"
    );
    let mut changed_exports = initial.clone();
    changed_exports.modules[0].exports.push(NodeId(999));
    assert!(session.checkpoint_for(&changed_exports).is_none());
    let mut changed_root = initial.clone();
    changed_root.root = ModuleId(9);
    assert!(session.checkpoint_for(&changed_root).is_none());
    let mut changed_identity = initial.clone();
    changed_identity.source_fingerprint = Fingerprint::of(b"new-source-identity");
    assert!(session.checkpoint_for(&changed_identity).is_none());
    session.clear();
    assert!(weak.upgrade().is_some());
    drop(checkpoint);
    assert!(
        weak.upgrade().is_none(),
        "no hidden global receipt/history keeps the epoch alive"
    );
}

#[test]
fn callee_diagnostic_positions_do_not_invalidate_unchanged_callers() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let initial = program();
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &limits);
    let mut moved = initial.clone();
    let callee = &mut Arc::make_mut(&mut moved.modules[0].core).functions[1];
    callee.span = Span::new(FileId(3), 400, 480);
    callee.parameters[0].span = Span::new(FileId(3), 403, 410);
    checked(&mut session, &moved, &target, &limits);
    assert_eq!(session.stats.context_reuses, 1);
    assert_eq!(session.stats.body_checks, 1);
    assert_eq!(session.stats.body_reuses, 2);
    // Names still affect the callee's local scope and declaration charges.
    Arc::make_mut(&mut moved.modules[0].core).functions[1].parameters[0].name = "changed".into();
    checked(&mut session, &moved, &target, &limits);
    assert_eq!(session.stats.context_reuses, 1);
    assert_eq!(session.stats.body_checks, 2);
    assert_eq!(session.stats.body_reuses, 1);
}

#[test]
fn changed_prefix_n_and_n_minus_one_replay_costs_without_renewing_budget() {
    let initial = program();
    let mut edited = initial.clone();
    let first = &mut Arc::make_mut(&mut edited.modules[0].core).functions[0];
    for n in 0..16 {
        first.body.statements.push(CoreStatement {
            id: NodeId(500 + n as u32),
            span: Span::new(FileId(0), 200 + n, 201 + n),
            kind: CoreStatementKind::Expression(expr(CoreExprKind::Integer("12345".into()))),
        });
    }
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let mut low = 0;
    let mut high = defaults.max_analysis_steps;
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..defaults.clone()
        };
        if fresh(&edited, &target, &limits).is_ok() {
            high = middle;
        } else {
            low = middle;
        }
    }
    for allowance in [high, high - 1] {
        let limits = Limits {
            max_analysis_steps: allowance,
            ..defaults.clone()
        };
        let mut session = PlainSourceSession::default();
        checked(&mut session, &initial, &target, &limits);
        assert_eq!(session.stats.fallbacks, 0);
        let successful = session.current.clone().unwrap();
        let actual = run(&mut session, &edited, &target, &limits);
        assert_eq!(actual, fresh(&edited, &target, &limits));
        assert_eq!(session.stats.context_reuses, 1);
        if allowance == high {
            assert!(actual.is_ok());
            assert_eq!(session.stats.body_checks, 1);
            assert_eq!(session.stats.body_reuses, 2);
        } else {
            assert!(actual.is_err());
            assert_eq!(session.stats.fallbacks, 1);
            assert!(Arc::ptr_eq(session.current.as_ref().unwrap(), &successful));
            checked(&mut session, &initial, &target, &limits);
            assert_eq!(session.stats.body_reuses, 3);
        }
    }
}

#[test]
fn raw_declaration_numbering_does_not_change_plain_hint_environment() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut initial = program();
    initial.modules[0].exports = vec![NodeId(1)];
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &limits);
    let checkpoint = session.checkpoint_for(&initial).unwrap();
    let mut renumbered = initial.clone();
    Arc::make_mut(&mut renumbered.modules[0].core).functions[1].id = NodeId(101);
    renumbered.modules[0].exports = vec![NodeId(101)];
    assert!(checkpoint.checkpoint_for(&renumbered).is_none());
    checked(&mut session, &renumbered, &target, &limits);
    assert_eq!(session.stats.context_reuses, 1);
    assert_eq!(session.stats.body_checks, 1);
    assert_eq!(session.stats.body_reuses, 2);
    assert_eq!(session.stats.declaration_modules, 1);
    assert!(session.checkpoint_for(&renumbered).is_some());
    assert!(session.checkpoint_for(&initial).is_none());

    // Exports do not participate in CT's hint lookup; the current raw export
    // table must still be returned and cannot attach to the old checkpoint.
    let checkpoint = session.clone();
    let mut exports_only = renumbered.clone();
    exports_only.modules[0].exports = vec![NodeId(2)];
    assert!(checkpoint.checkpoint_for(&exports_only).is_none());
    let actual = run(&mut session, &exports_only, &target, &limits).unwrap();
    assert_eq!(actual, fresh(&exports_only, &target, &limits).unwrap());
    assert_eq!(actual.0.modules[0].exports, vec![NodeId(2)]);
    assert_eq!(session.stats.body_checks, 0);
    assert_eq!(session.stats.body_reuses, 3);
    assert_eq!(session.stats.declaration_modules, 1);
    assert!(checkpoint.checkpoint_for(&renumbered).is_some());
}

#[test]
fn renumbering_does_not_hide_changed_foreign_or_real_hint_headers() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut initial = program();
    let callee = &mut Arc::make_mut(&mut initial.modules[0].core).functions[1];
    callee.body.statements.clear();
    callee.foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "callee_external".into(),
        noescape_parameters: vec![],
        static_callbacks: false,
        scoped_callback: None,
    });
    let mut session = PlainSourceSession::default();
    checked(&mut session, &initial, &target, &limits);
    let previous = session.clone();
    let mut changed = initial.clone();
    let callee = &mut Arc::make_mut(&mut changed.modules[0].core).functions[1];
    callee.id = NodeId(100);
    callee.foreign.as_mut().unwrap().symbol = "a_different_external_symbol".into();
    changed.modules[0].exports = vec![NodeId(100)];
    checked(&mut session, &changed, &target, &limits);
    assert_eq!(session.stats.context_reuses, 1);
    assert_eq!(session.stats.body_checks, 2);
    assert_eq!(session.stats.body_reuses, 1);
    assert_eq!(session.stats.declaration_modules, 1);
    assert!(previous.checkpoint_for(&changed).is_none());
    let callee = &mut Arc::make_mut(&mut changed.modules[0].core).functions[1];
    callee.id = NodeId(101);
    callee.parameters[0].ty = Type::I64;
    changed.modules[0].exports = vec![NodeId(101)];
    checked(&mut session, &changed, &target, &limits);
    assert_eq!(session.stats.context_reuses, 1);
    assert_eq!(session.stats.body_reuses, 1);
}

#[test]
fn current_prefix_allowances_replay_fees_without_changing_the_user_contract() {
    fn with_allowance(
        session: &mut PlainSourceSession,
        program: &CoreProgram,
        target: &TargetProfile,
        user: &Limits,
        allowance: &Limits,
    ) -> CompileResult<(CoreProgram, Vec<SourceEvaluationUsage>)> {
        let context = SourceProgramContext::new(program, allowance)?;
        if let Some(usages) =
            session.validate_program_with_allowance(program, target, user, allowance, &context)
        {
            return Ok((program.clone(), usages));
        }
        fresh(program, target, allowance)
    }
    let program = program();
    let user = Limits::default();
    let target = TargetProfile::default();
    let mut session = PlainSourceSession::default();
    checked(&mut session, &program, &target, &user);
    let mut current = user.clone();
    current.max_nodes -= 5;
    current.max_ct_steps = 0;
    assert_eq!(
        with_allowance(&mut session, &program, &target, &user, &current),
        fresh(&program, &target, &current)
    );
    assert_eq!(session.stats.context_reuses, 1);
    assert_eq!(session.stats.body_checks, 0);
    assert_eq!(session.stats.body_reuses, 3);
    assert_eq!(session.stats.declaration_modules, 1);

    // Exercise the actual node boundary from both sides with a retained epoch:
    // insufficiency must run the original path and preserve its exact error.
    let mut saw_body_fallback = false;
    let mut saw_successful_reuse = false;
    for nodes in 0..200 {
        current.max_nodes = nodes;
        let context_exists = SourceProgramContext::new(&program, &current).is_ok();
        let expected = fresh(&program, &target, &current);
        let actual = with_allowance(&mut session, &program, &target, &user, &current);
        assert_eq!(actual, expected, "current node allowance {nodes}");
        saw_body_fallback |= context_exists && actual.is_err() && session.stats.fallbacks == 1;
        saw_successful_reuse |= actual.is_ok() && session.stats.body_reuses == 3;
    }
    assert!(saw_body_fallback && saw_successful_reuse);

    let wrong_context = SourceProgramContext::new(&program, &user).unwrap();
    assert!(
        session
            .validate_program_with_allowance(&program, &target, &user, &current, &wrong_context)
            .is_none()
    );
    let mut unsupported = user.clone();
    unsupported.max_analysis_steps -= 1;
    let context = SourceProgramContext::new(&program, &unsupported).unwrap();
    assert!(
        session
            .validate_program_with_allowance(&program, &target, &user, &unsupported, &context)
            .is_none()
    );
    let mut excessive = user.clone();
    excessive.max_nodes += 1;
    let context = SourceProgramContext::new(&program, &excessive).unwrap();
    assert!(
        session
            .validate_program_with_allowance(&program, &target, &user, &excessive, &context)
            .is_none()
    );
    let mut changed_user = user.clone();
    changed_user.max_nodes -= 1;
    assert_eq!(
        with_allowance(&mut session, &program, &target, &changed_user, &current),
        fresh(&program, &target, &current)
    );
    assert_eq!(session.stats.context_reuses, 0);
    assert_eq!(session.stats.body_checks, 3);
}
