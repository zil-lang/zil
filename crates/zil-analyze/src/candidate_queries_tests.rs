use super::*;
use zil_base::{FileId, ModuleId, SCHEMA_VERSION, Span, TargetProfile};

fn function(id: u32, literal: i32) -> TypedFunction {
    TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(id),
        name: format!("function{id}"),
        foreign: None,
        environment: None,
        parameters: vec![],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: vec![].into(),
        values: vec![Type::I32].into(),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::Constant(Constant::I32(literal)),
                span: Span::new(FileId(0), 0, 10),
            }],
            terminator: Terminator::Return(Some(ValueId(0))),
        }]
        .into(),
        entry: BlockId(0),
        span: Span::new(FileId(0), 0, 10),
    }
}

fn program() -> Arc<TypedProgram> {
    let mut caller = function(1, 0);
    caller.blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: vec![],
    };
    Arc::new(TypedProgram {
        task_interfaces: vec![],
        interfaces: vec![],
        root_functions: vec![],
        module: TypedModule {
            dyn_declarations: vec![],
            dyn_interfaces: vec![],
            dyn_vtables: vec![],
            life_declarations: vec![],
            life_instances: vec![],
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::default(),
            functions: vec![function(0, 4), caller, function(2, 7)],
            closures: vec![],
            resources: vec![],
            unsupported: vec![],
        },
    })
}

fn run(
    session: &mut AnalysisSession,
    program: &Arc<TypedProgram>,
    limits: &Limits,
) -> Arc<CandidatePlan> {
    let actual = session.analyze_shared_program(program, limits).unwrap();
    assert_eq!(*actual, analyze_program(program, limits).unwrap());
    actual
}

#[test]
fn unchanged_immutable_program_reuses_the_complete_candidate() {
    let program = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let first = run(&mut session, &program, &limits);
    assert_eq!(session.last_stats().candidate_executed_functions, 3);
    assert_eq!(session.last_stats().initialization_inputs_built, 3);
    let second = run(&mut session, &program, &limits);
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(session.last_stats().shared_program_reuses, 1);
    assert_eq!(session.last_stats().initialization_inputs_built, 0);
    assert_eq!(session.last_stats().candidate_inputs_built, 0);
    assert!(
        session
            .last_function_results()
            .iter()
            .all(|result| result.reused)
    );

    // Equal content in another allocation still proves its actual function keys.
    let copied = Arc::new((*program).clone());
    run(&mut session, &copied, &limits);
    assert_eq!(session.last_stats().shared_program_reuses, 0);
    assert_eq!(session.last_stats().candidate_reused_functions, 3);
    assert_eq!(session.last_stats().initialization_inputs_built, 0);
    assert_eq!(session.last_stats().candidate_inputs_built, 0);
}

#[test]
fn private_body_edits_recompute_one_item_and_equal_candidates_stop_propagation() {
    let source = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &limits);
    let previous = session.last_function_results().to_vec();
    let mut edited = (*source).clone();
    edited.module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(99));
    let edited = Arc::new(edited);
    run(&mut session, &edited, &limits);
    let stats = session.last_stats();
    assert_eq!(
        (
            stats.candidate_executed_functions,
            stats.candidate_reused_functions
        ),
        (1, 2)
    );
    assert_eq!(
        (stats.equal_function_results, stats.changed_function_results),
        (3, 0)
    );
    assert_eq!(stats.initialization_inputs_built, 1);
    assert_eq!(stats.candidate_inputs_built, 1);
    let results = session.last_function_results();
    assert_ne!(results[0].input, previous[0].input);
    for index in 0..3 {
        assert_eq!(results[index].output, previous[index].output);
    }
    assert_eq!(results[1].dependencies, previous[1].dependencies);
    assert!(results[1].reused && results[2].reused);
}

#[test]
fn actual_callee_binding_and_signature_changes_invalidate_only_their_consumers() {
    let source = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &limits);
    let previous = session.last_function_results().to_vec();
    let mut changed = (*source).clone();
    changed.module.functions[0].name = "replacement_binding".into();
    let changed = Arc::new(changed);
    run(&mut session, &changed, &limits);
    assert_eq!(
        (
            session.last_stats().candidate_executed_functions,
            session.last_stats().candidate_reused_functions
        ),
        (2, 1)
    );
    let results = session.last_function_results();
    assert_eq!(results[1].input, previous[1].input);
    assert_ne!(results[1].dependencies, previous[1].dependencies);
    assert!(results[2].reused);
    let mut signature = (*changed).clone();
    signature.module.functions[0].return_type = Type::I64;
    signature.module.functions[0].values[0] = Type::I64;
    signature.module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I64(99));
    // This is raw IR; Analyze's candidate equality must match fresh Analyze.
    // Independent Verify still owns the caller's declared result-type check.
    run(&mut session, &Arc::new(signature), &limits);
    assert_eq!(
        (
            session.last_stats().candidate_executed_functions,
            session.last_stats().candidate_reused_functions
        ),
        (2, 1)
    );
}

#[test]
fn failed_epochs_and_interface_changes_cannot_publish_or_reuse_a_candidate() {
    let source = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let valid = run(&mut session, &source, &limits);
    let previous = session.last_function_results().to_vec();
    let mut invalid = (*source).clone();
    invalid.module.functions[2].blocks[0].terminator = Terminator::Jump(BlockId(99));
    let invalid = Arc::new(invalid);
    assert_eq!(
        session
            .analyze_shared_program(&invalid, &limits)
            .map(|plan| (*plan).clone()),
        analyze_program(&invalid, &limits)
    );
    assert_eq!(session.last_function_results(), previous);
    let recovered = run(&mut session, &source, &limits);
    assert!(Arc::ptr_eq(&valid, &recovered));
    // Recovery is itself a successful warm epoch. The following failure must
    // preserve that epoch's evidence, including its actual reuse flags.
    let previous = session.last_function_results().to_vec();
    assert!(previous.iter().all(|result| result.reused));

    let mut interfaces = (*source).clone();
    interfaces.task_interfaces = vec![
        TaskInterface {
            module: ModuleId(0),
            exports: vec![]
        };
        2
    ];
    let interfaces = Arc::new(interfaces);
    let fresh = analyze_program(&interfaces, &limits);
    assert!(fresh.is_err());
    assert_eq!(
        session
            .analyze_shared_program(&interfaces, &limits)
            .map(|plan| (*plan).clone()),
        fresh
    );
    assert_eq!(session.last_function_results(), previous);
    assert!(Arc::ptr_eq(&valid, &run(&mut session, &source, &limits)));
}

#[test]
fn exact_budget_boundaries_and_insufficient_aggregate_balance_keep_the_first_error() {
    let source = program();
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        if analyze_program(&source, &limits).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let exact = Limits {
        max_analysis_steps: low,
        ..Limits::default()
    };
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &exact);
    // A different Arc with identical input exercises all item-level cost replay.
    run(&mut session, &Arc::new((*source).clone()), &exact);
    assert_eq!(session.last_stats().candidate_reused_functions, 3);
    for steps in [0, low - 1, low, low + 1] {
        let limits = Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        };
        assert_eq!(
            session
                .analyze_shared_program(&source, &limits)
                .map(|plan| (*plan).clone()),
            analyze_program(&source, &limits)
        );
    }
    let mut larger = (*source).clone();
    larger.module.functions.push(function(3, 8));
    let larger = Arc::new(larger);
    assert!(analyze_program(&larger, &exact).is_err());
    assert_eq!(
        session
            .analyze_shared_program(&larger, &exact)
            .map(|plan| (*plan).clone()),
        analyze_program(&larger, &exact)
    );
}

#[test]
fn complete_module_dependencies_keep_complex_domains_fresh_and_shared_inputs_reusable() {
    let mut source = (*program()).clone();
    source.module.resources.push(ResourceDefinition {
        name: "unused".into(),
        payload: Type::I32,
        teardown: None,
    });
    let source = Arc::new(source);
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let first = run(&mut session, &source, &limits);
    assert!(
        session
            .last_function_results()
            .iter()
            .all(|result| !result.function_scoped)
    );
    assert_eq!(session.last_stats().candidate_executed_functions, 3);
    assert!(Arc::ptr_eq(&first, &run(&mut session, &source, &limits)));
    let mut changed = (*source).clone();
    changed.module.resources[0].name = "different_resource_binding".into();
    run(&mut session, &Arc::new(changed), &limits);
    assert_eq!(session.last_stats().candidate_executed_functions, 3);
    assert_eq!(session.last_stats().candidate_reused_functions, 0);
    assert_eq!(session.last_stats().equal_function_results, 3);
}

#[test]
fn ordinary_analysis_does_not_enable_the_research_queries() {
    let source = program();
    let mut session = AnalysisSession::default();
    session
        .analyze_program(&source, &Limits::default())
        .unwrap();
    session
        .analyze_program(&source, &Limits::default())
        .unwrap();
    assert_eq!(session.last_stats().candidate_reused_functions, 0);
    assert_eq!(session.last_stats().candidate_executed_functions, 0);
    assert_eq!(session.last_stats().initialization_inputs_built, 3);
    assert!(session.last_function_results().is_empty());
}

#[test]
fn foreign_getters_do_not_disable_independent_scalar_queries_or_source_rebinding() {
    let mut source = (*program()).clone();
    let mut foreign = function(3, 0);
    foreign.name = "getchar".into();
    foreign.blocks.clear();
    foreign.values.clear();
    foreign.foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "getchar".into(),
        noescape_parameters: vec![],
        static_callbacks: false,
        scoped_callback: None,
    });
    source.module.functions.push(foreign);
    let source = Arc::new(source);
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &limits);
    let previous = session.last_function_results().to_vec();
    let mut edited = (*source).clone();
    edited.module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(12345));
    // A real insertion shifts every following diagnostic binding. It must not
    // invalidate successful semantic candidate rows in otherwise equal items.
    for function in &mut edited.module.functions[1..] {
        function.span = Span::new(FileId(8), 500, 510);
        for local in &mut function.locals {
            local.span = function.span;
        }
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                instruction.span = function.span;
            }
        }
    }
    edited.module.source_fingerprint = Fingerprint::of(b"longer current source");
    run(&mut session, &Arc::new(edited), &limits);
    assert_eq!(session.last_stats().candidate_executed_functions, 2); // edited leaf and foreign declaration
    assert_eq!(session.last_stats().candidate_reused_functions, 2);
    assert_eq!(session.last_stats().initialization_inputs_built, 1);
    assert_eq!(session.last_stats().candidate_inputs_built, 1);
    let results = session.last_function_results();
    assert!(results[1].reused && results[2].reused);
    for index in [1, 2] {
        assert_eq!(results[index].input, previous[index].input);
    }
    assert!(!results[3].function_scoped);
}

#[test]
fn an_unclosed_foreign_call_recomputes_itself_without_disabling_scalar_callers() {
    let mut source = (*program()).clone();
    let mut foreign = function(3, 0);
    foreign.blocks.clear();
    foreign.values.clear();
    foreign.foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "getchar".into(),
        noescape_parameters: vec![],
        static_callbacks: false,
        scoped_callback: None,
    });
    source.module.functions.push(foreign);
    let span = source.module.functions[0].span;
    source.module.functions[0].blocks[0].instructions = vec![
        Instruction {
            result: None,
            kind: InstructionKind::UnsafeEnter { region: 7 },
            span,
        },
        Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::ForeignCall {
                function: FunctionId(3),
                arguments: vec![],
                region: 7,
            },
            span,
        },
        Instruction {
            result: None,
            kind: InstructionKind::UnsafeExit { region: 7 },
            span,
        },
    ];
    let source = Arc::new(source);
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &limits);
    run(&mut session, &Arc::new((*source).clone()), &limits);
    assert_eq!(session.last_stats().candidate_reused_functions, 2);
    assert!(session.last_function_results()[1].reused);
    assert!(!session.last_function_results()[0].function_scoped);
    assert!(!session.last_function_results()[3].function_scoped);
}

fn resource_program(write: bool) -> Arc<TypedProgram> {
    let mut source = (*program()).clone();
    let payload = Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    let desc = Type::Desc(Box::new(payload.clone()));
    source.module.resources.push(ResourceDefinition {
        name: "Token".into(),
        payload,
        teardown: None,
    });
    for index in 0..2 {
        let function = &mut source.module.functions[index];
        function.locals = vec![LocalDecl {
            id: LocalId(0),
            name: "arg".into(),
            ty: desc.clone(),
            mutable: false,
            span: function.span,
        }]
        .into();
        function.parameters = vec![LocalId(0)];
        function.values = vec![desc.clone(), Type::I32].into();
        function.blocks[0].instructions = vec![
            Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::Load(LocalId(0)),
                span: function.span,
            },
            Instruction {
                result: Some(ValueId(1)),
                kind: if index == 0 {
                    InstructionKind::ReadDesc {
                        desc: ValueId(0),
                        index: 0,
                    }
                } else {
                    InstructionKind::Call {
                        function: FunctionId(0),
                        arguments: vec![ValueId(0)],
                    }
                },
                span: function.span,
            },
        ];
        function.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    }
    if write {
        let function = &mut source.module.functions[0];
        let span = function.span;
        function.blocks[0].instructions.push(Instruction {
            result: None,
            kind: InstructionKind::WriteDesc {
                desc: ValueId(0),
                index: 0,
                value: ValueId(1),
            },
            span,
        });
    }
    Arc::new(source)
}

#[test]
fn resource_transfers_bind_actual_callee_effects_and_skip_unaffected_bodies() {
    let source = resource_program(true);
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &limits);
    assert_eq!(session.last_stats().resource_transfers_executed, 6);
    assert_eq!(session.last_stats().resource_transfers_reused, 0);
    run(&mut session, &Arc::new((*source).clone()), &limits);
    assert_eq!(session.last_stats().resource_transfers_executed, 0);
    assert_eq!(session.last_stats().resource_transfers_reused, 6);
    assert_eq!(session.last_stats().initialization_inputs_built, 0);

    let mut edited = (*source).clone();
    edited.module.functions[2].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(99));
    run(&mut session, &Arc::new(edited), &limits);
    assert_eq!(session.last_stats().resource_transfers_executed, 2);
    assert_eq!(session.last_stats().resource_transfers_reused, 4);
    assert_eq!(session.last_stats().initialization_inputs_built, 1);

    // The caller's raw body is identical, but the exact observed callee effect
    // changes from write to read-only and must invalidate its validation query.
    run(&mut session, &resource_program(false), &limits);
    assert!(session.last_stats().resource_transfers_executed >= 4);
}

#[test]
fn resource_query_costs_preserve_n_minus_one_and_failed_epochs() {
    let source = resource_program(true);
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        if analyze_program(&source, &limits).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let exact = Limits {
        max_analysis_steps: low,
        ..Limits::default()
    };
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &exact);
    run(&mut session, &Arc::new((*source).clone()), &exact);
    assert_eq!(session.last_stats().resource_transfers_executed, 0);
    assert_eq!(session.last_stats().resource_transfers_reused, 6);
    let short = Limits {
        max_analysis_steps: low - 1,
        ..Limits::default()
    };
    assert_eq!(
        session
            .analyze_shared_program(&source, &short)
            .map(|plan| (*plan).clone()),
        analyze_program(&source, &short)
    );
    let mut invalid = (*source).clone();
    let span = invalid.module.functions[0].span;
    invalid.module.functions[0].blocks[0]
        .instructions
        .push(Instruction {
            result: None,
            kind: InstructionKind::WriteDesc {
                desc: ValueId(0),
                index: 99,
                value: ValueId(1),
            },
            span,
        });
    let invalid = Arc::new(invalid);
    let fresh = analyze_program(&invalid, &exact);
    assert!(fresh.is_err());
    assert_eq!(
        session
            .analyze_shared_program(&invalid, &exact)
            .map(|plan| (*plan).clone()),
        fresh
    );
    run(&mut session, &Arc::new((*source).clone()), &exact);
    assert_eq!(session.last_stats().resource_transfers_reused, 6);
}

#[test]
fn reused_initialization_and_holds_are_the_actual_candidate_arrays_and_mutate_in_isolation() {
    let source = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let first = run(&mut session, &source, &limits);
    let mut moved = (*source).clone();
    moved.module.source_fingerprint = Fingerprint::of(b"current unchanged function products");
    let moved = Arc::new(moved);
    let second = run(&mut session, &moved, &limits);
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(session.last_stats().candidate_reused_functions, 3);
    for (old, current) in first.functions.iter().zip(&second.functions) {
        assert!(old.blocks.ptr_eq(&current.blocks));
        assert!(old.holds.ptr_eq(&current.holds));
    }
    let before = zil_ir::wire::encode_module_candidate(&moved.module, &second, &limits).unwrap();
    let fresh = analyze_program(&moved, &limits).unwrap();
    assert_eq!(
        before,
        zil_ir::wire::encode_module_candidate(&moved.module, &fresh, &limits).unwrap()
    );

    let mut changed = (*second).clone();
    changed.functions[0].blocks[0]
        .initialized_in
        .push(LocalId(99));
    assert!(
        !changed.functions[0]
            .blocks
            .ptr_eq(&second.functions[0].blocks)
    );
    assert!(
        changed.functions[0]
            .holds
            .ptr_eq(&second.functions[0].holds)
    );
    assert!(second.functions[0].blocks[0].initialized_in.is_empty());
    changed.functions[0].holds[0]
        .roots
        .push(HoldRoot::Local(LocalId(99)));
    assert!(
        !changed.functions[0]
            .holds
            .ptr_eq(&second.functions[0].holds)
    );
    assert!(second.functions[0].holds[0].roots.is_empty());
    assert!(
        changed.functions[1]
            .blocks
            .ptr_eq(&second.functions[1].blocks)
    );
    assert!(
        changed.functions[1]
            .holds
            .ptr_eq(&second.functions[1].holds)
    );
    assert_eq!(
        before,
        zil_ir::wire::encode_module_candidate(&moved.module, &second, &limits).unwrap()
    );
    let third = run(&mut session, &Arc::new((*moved).clone()), &limits);
    for (old, current) in second.functions.iter().zip(&third.functions) {
        assert!(old.blocks.ptr_eq(&current.blocks));
        assert!(old.holds.ptr_eq(&current.holds));
    }
}
