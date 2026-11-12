use super::*;
use zil_base::{FileId, ModuleId, SCHEMA_VERSION, Span, TargetProfile};

fn function(id: u32, value: i32) -> TypedFunction {
    TypedFunction {
        id: FunctionId(id),
        name: format!("f{id}"),
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
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
                kind: InstructionKind::Constant(Constant::I32(value)),
                span: Span::new(FileId(0), 10, 20),
            }],
            terminator: Terminator::Return(Some(ValueId(0))),
        }]
        .into(),
        entry: BlockId(0),
        span: Span::new(FileId(0), 0, 20),
    }
}
fn program() -> Arc<TypedProgram> {
    let mut caller = function(1, 0);
    caller.blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: vec![],
    };
    Arc::new(TypedProgram {
        interfaces: vec![],
        root_functions: vec![],
        task_interfaces: vec![],
        module: TypedModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::default(),
            functions: vec![function(0, 3), caller, function(2, 7)].into(),
            closures: vec![],
            resources: vec![],
            unsupported: vec![],
            life_declarations: vec![],
            life_instances: vec![],
            dyn_declarations: vec![],
            dyn_interfaces: vec![],
            dyn_vtables: vec![],
        },
    })
}
fn expected(program: &TypedProgram, limits: &Limits) -> CompileResult<CandidatePlan> {
    analyze_program(program, limits).map(|mut plan| {
        plan.module_fingerprint = program.module.structural_fingerprint();
        plan
    })
}
fn run(
    session: &mut AnalysisSession,
    program: &Arc<TypedProgram>,
    limits: &Limits,
) -> Arc<CandidatePlan> {
    let actual = session.analyze_product(program, limits);
    assert_eq!(
        actual.as_ref().map(|p| p.as_ref()),
        expected(program, limits).as_ref()
    );
    actual.unwrap()
}
fn edit(source: &Arc<TypedProgram>, value: i32) -> Arc<TypedProgram> {
    let mut next = (**source).clone();
    next.module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(value));
    next.module.source_fingerprint = Fingerprint::of(&value.to_le_bytes());
    Arc::new(next)
}
#[test]
fn product_consumes_only_changed_raw_items_and_preserves_equal_plan_owners() {
    let source = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let cold = run(&mut session, &source, &limits);
    let cold_results = session.last_product_changes().to_vec();
    let old_initialization = session.product.as_ref().unwrap().rows[0]
        .initialization
        .as_ref()
        .unwrap()
        .clone();
    assert_eq!(session.last_stats().product_admission_functions_checked, 3);
    let warm = run(&mut session, &source, &limits);
    assert!(Arc::ptr_eq(&cold, &warm));
    assert_eq!(session.last_stats().product_function_queries_executed, 0);
    let changed = edit(&source, 11);
    let next = run(&mut session, &changed, &limits);
    assert_ne!(cold.module_fingerprint, next.module_fingerprint);
    assert_eq!(session.last_stats().product_admission_functions_checked, 1);
    assert_eq!(session.last_stats().product_function_queries_executed, 1);
    assert_eq!(session.last_stats().executed_functions, 0);
    assert_eq!(session.last_stats().reused_functions, 1);
    assert!(Arc::ptr_eq(
        &old_initialization,
        session.product.as_ref().unwrap().rows[0]
            .initialization
            .as_ref()
            .unwrap()
    ));
    assert_eq!(session.last_stats().product_function_queries_reused, 2);
    assert_eq!(session.last_stats().product_global_obligations_reused, 1);
    assert_eq!(session.last_stats().product_plan_rows_replaced, 0);
    assert!(next.functions.changed_indices(&cold.functions).is_empty());
    assert_eq!(session.last_product_changes().len(), 1);
    assert_eq!(session.last_product_changes()[0].function, FunctionId(0));
    assert_eq!(
        session.last_product_changes()[0].output,
        cold_results[0].output
    );
    assert_ne!(
        session.last_product_changes()[0].input,
        cold_results[0].input
    );

    assert!(session.last_function_results().is_empty());
    // Existing public raw APIs keep their original complete identity contract.
    let raw = analyze_program(&changed, &limits).unwrap();
    assert_eq!(raw.module_fingerprint, changed.module.fingerprint());
    assert_ne!(raw.module_fingerprint, next.module_fingerprint);
}
#[test]
fn current_signature_reads_trigger_only_actual_callers() {
    let source = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let old = run(&mut session, &source, &limits);
    let mut changed = (*source).clone();
    changed.module.functions[0].name.push_str("_current");
    let changed = Arc::new(changed);
    let next = run(&mut session, &changed, &limits);
    assert_eq!(session.last_stats().product_function_queries_executed, 2);
    assert_eq!(
        session
            .last_product_changes()
            .iter()
            .map(|r| r.function)
            .collect::<Vec<_>>(),
        vec![FunctionId(0), FunctionId(1)]
    );
    assert!(Arc::ptr_eq(
        old.functions.owner(2).unwrap(),
        next.functions.owner(2).unwrap()
    ));
    assert_eq!(session.last_stats().product_plan_rows_replaced, 0);
}
#[test]
fn foreign_declarations_and_foreign_calls_are_complete_local_inputs() {
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
    source.module.functions[1].blocks[0].instructions[0].kind = InstructionKind::ForeignCall {
        function: FunctionId(3),
        arguments: vec![],
        region: 0,
    };
    let source = Arc::new(source);
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &limits);
    assert!(session.product.is_some());
    run(&mut session, &edit(&source, 8), &limits);
    assert_eq!(session.last_stats().product_function_queries_executed, 1);
    let mut signature = (*source).clone();
    signature.module.functions[3]
        .foreign
        .as_mut()
        .unwrap()
        .symbol = "read_current".into();
    run(&mut session, &Arc::new(signature), &limits);
    assert_eq!(session.last_stats().product_function_queries_executed, 3); // f0 restoration + foreign + its actual caller
}
#[test]
fn aggregate_quota_failure_cannot_publish_a_partially_executed_epoch() {
    let source = program();
    let limits = Limits {
        max_values: 3,
        ..Limits::default()
    };
    let mut session = AnalysisSession::default();
    let old = run(&mut session, &source, &limits);
    let mut changed = (*source).clone();
    let f = &mut changed.module.functions[0];
    f.values.push(Type::I32);
    f.blocks[0].instructions.push(Instruction {
        result: Some(ValueId(1)),
        kind: InstructionKind::Constant(Constant::I32(99)),
        span: f.span,
    });
    let changed = Arc::new(changed);
    assert_eq!(
        session
            .analyze_product(&changed, &limits)
            .map(|p| (*p).clone()),
        expected(&changed, &limits)
    );
    assert!(expected(&changed, &limits).is_err());
    assert!(Arc::ptr_eq(&old, &run(&mut session, &source, &limits)));
}
#[test]
fn logical_work_exact_boundaries_and_first_errors_match_original_order() {
    let source = program();
    let normal = Limits::default();
    let mut warm = AnalysisSession::default();
    run(&mut warm, &source, &normal);
    let epoch = warm.product.as_ref().unwrap();
    let cost = epoch.total.total.cost + epoch.global_cost;
    for allowed in [cost, cost - 1, cost + 2] {
        let limits = Limits {
            max_analysis_steps: allowed,
            ..normal.clone()
        };
        let mut session = AnalysisSession::default();
        let actual = session
            .analyze_product(&source, &limits)
            .map(|p| (*p).clone());
        assert_eq!(actual, expected(&source, &limits));
        if allowed >= cost {
            let private = edit(&source, 17);
            run(&mut session, &private, &limits);
            assert_eq!(session.last_stats().product_global_obligations_reused, 1);
            let mut more = (*private).clone();
            more.module.functions[0]
                .name
                .push_str("longer_current_name");
            let more = Arc::new(more);
            assert_eq!(
                session
                    .analyze_product(&more, &limits)
                    .map(|p| (*p).clone()),
                expected(&more, &limits)
            );
        }
    }
}
#[test]
fn changed_aggregate_counts_update_fees_and_result_rows_without_replaying_other_functions() {
    let source = program();
    let normal = Limits::default();
    let mut session = AnalysisSession::default();
    let cold = run(&mut session, &source, &normal);
    let mut changed = (*source).clone();
    let f = &mut changed.module.functions[0];
    f.locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "unused0".into(),
            ty: Type::I32,
            mutable: false,
            span: f.span,
        },
        LocalDecl {
            id: LocalId(1),
            name: "unused1".into(),
            ty: Type::I32,
            mutable: false,
            span: f.span,
        },
    ]
    .into();
    f.values.push(Type::I32);
    f.blocks[0].instructions.push(Instruction {
        result: Some(ValueId(1)),
        kind: InstructionKind::Constant(Constant::I32(88)),
        span: f.span,
    });
    let changed = Arc::new(changed);
    let next = run(&mut session, &changed, &normal);
    assert_eq!(session.last_stats().product_function_queries_executed, 1);
    assert_eq!(next.functions.changed_indices(&cold.functions), vec![0]);
    let current_initialization = session.product.as_ref().unwrap().rows[0]
        .initialization
        .as_ref()
        .unwrap()
        .clone();
    let mut literal_only = (*changed).clone();
    literal_only.module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(901));
    run(&mut session, &Arc::new(literal_only), &normal);
    assert_eq!(session.last_stats().executed_functions, 0);
    assert_eq!(session.last_stats().reused_functions, 1);
    assert!(Arc::ptr_eq(
        &current_initialization,
        session.product.as_ref().unwrap().rows[0]
            .initialization
            .as_ref()
            .unwrap()
    ));
    let epoch = session.product.as_ref().unwrap();
    let exact = epoch.total.total.cost + epoch.global_cost;
    for work in [exact, exact - 1] {
        let limits = Limits {
            max_analysis_steps: work,
            ..normal.clone()
        };
        let mut replay = AnalysisSession::default();
        assert_eq!(
            replay
                .analyze_product(&changed, &limits)
                .map(|p| (*p).clone()),
            expected(&changed, &limits)
        );
    }
}
#[test]
fn limits_removal_global_changes_and_error_recovery_rebind_every_current_input() {
    let source = program();
    let normal = Limits::default();
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &normal);
    let limits = Limits {
        max_ct_steps: normal.max_ct_steps - 1,
        ..normal.clone()
    };
    run(&mut session, &source, &limits);
    assert_eq!(session.last_stats().product_global_obligations_executed, 1);
    let mut removed = (*source).clone();
    removed.module.functions.pop();
    let removed = Arc::new(removed);
    let rows = run(&mut session, &removed, &limits);
    assert_eq!(rows.functions.len(), 2);
    let mut global = (*removed).clone();
    global.module.module_id = ModuleId(9);
    run(&mut session, &Arc::new(global), &limits);
    assert_eq!(session.last_stats().product_global_obligations_executed, 1);
    let mut invalid = (*source).clone();
    invalid.module.functions[0].id = FunctionId(99);
    let invalid = Arc::new(invalid);
    assert_eq!(
        session
            .analyze_product(&invalid, &limits)
            .map(|p| (*p).clone()),
        expected(&invalid, &limits)
    );
    run(&mut session, &source, &limits);
}
#[test]
fn rich_global_contracts_remain_complete_and_exact_whole_products_can_reuse_them() {
    let mut rich = (*program()).clone();
    rich.module.resources.push(ResourceDefinition {
        name: "Token".into(),
        payload: Type::Record {
            name: "Token".into(),
            fields: vec![("n".into(), Type::I32)],
        },
        teardown: None,
    });
    let rich = Arc::new(rich);
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let old = run(&mut session, &rich, &limits);
    assert!(session.product.is_none());
    assert_eq!(session.last_stats().product_global_obligations_executed, 1);
    assert!(Arc::ptr_eq(&old, &run(&mut session, &rich, &limits)));
    assert_eq!(session.last_stats().product_global_obligations_reused, 1);
    run(&mut session, &edit(&rich, 14), &limits);
    assert_eq!(session.last_stats().product_global_obligations_executed, 1);
}

#[test]
fn later_malformed_callee_metadata_is_admitted_before_any_current_caller_solve() {
    let mut input = (*program()).clone();
    input.module.functions[0].blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(2),
        arguments: vec![],
    };
    input.module.functions[1].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(0));
    let input = Arc::new(input);
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let before = run(&mut session, &input, &limits);
    let mut invalid = (*input).clone();
    invalid.module.functions[2].parameters.push(LocalId(999));
    let invalid = Arc::new(invalid);
    let actual = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        session.analyze_product(&invalid, &limits)
    }))
    .expect("unadmitted callee must not be indexed by an early caller");
    assert_eq!(
        actual.map(|plan| (*plan).clone()),
        expected(&invalid, &limits)
    );
    assert!(Arc::ptr_eq(&before, &run(&mut session, &input, &limits)));
}
#[test]
fn shared_provisional_allowance_never_gives_each_item_a_full_work_limit() {
    let input = program();
    let limits = Limits::default();
    let full = admit(
        &input.module,
        0,
        &input.module.functions[0],
        &limits,
        limits.max_analysis_steps,
    )
    .unwrap();
    assert!(full.cost > 0);
    assert!(
        admit(
            &input.module,
            0,
            &input.module.functions[0],
            &limits,
            full.cost - 1
        )
        .is_err()
    );
    let remaining = limits.max_analysis_steps - full.cost;
    let mut scoped = full.budget;
    scoped.restrict_product_balance(remaining - 1);
    let before = scoped.remaining();
    scoped.restrict_product_balance(limits.max_analysis_steps);
    assert_eq!(
        scoped.remaining(),
        before,
        "a later item cannot replenish the shared allowance"
    );
}

fn shift_function(function: &mut TypedFunction, offset: usize) {
    function.span.start += offset;
    function.span.end += offset;
    for local in &mut function.locals {
        local.span.start += offset;
        local.span.end += offset;
    }
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            instruction.span.start += offset;
            instruction.span.end += offset;
        }
    }
}
#[test]
fn current_diagnostic_bindings_reuse_only_completely_equal_semantic_inputs() {
    let source = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let previous = run(&mut session, &source, &limits);
    let mut relocated = (*source).clone();
    for function in &mut relocated.module.functions {
        shift_function(function, 25);
    }
    relocated.module.source_fingerprint = Fingerprint::of(b"positions only");
    let relocated = Arc::new(relocated);
    let result = run(&mut session, &relocated, &limits);
    assert_eq!(session.last_stats().product_function_queries_executed, 0);
    assert_eq!(session.last_stats().product_relocated_function_bindings, 3);
    assert_eq!(session.last_stats().product_semantic_functions_compared, 3);
    assert!(
        result
            .functions
            .changed_indices(&previous.functions)
            .is_empty()
    );
    assert_eq!(session.last_product_changes().len(), 3);
    assert!(session.last_product_changes().iter().all(|r| r.reused));
    // A real metadata input must not be mistaken for diagnostic rebinding.
    let mut renamed = (*relocated).clone();
    renamed.module.functions[0].name.push_str("actual_metadata");
    run(&mut session, &Arc::new(renamed), &limits);
    assert_eq!(session.last_stats().product_function_queries_executed, 2);
}
#[test]
fn variable_width_private_edit_rebinds_siblings_without_reexecuting_their_queries() {
    let source = program();
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    let old = run(&mut session, &source, &limits);
    let mut changed = (*edit(&source, 123456)).clone();
    for function in changed.module.functions.iter_mut().skip(1) {
        shift_function(function, 5);
    }
    let next = run(&mut session, &Arc::new(changed), &limits);
    assert_eq!(session.last_stats().product_function_queries_executed, 1);
    assert_eq!(session.last_stats().product_relocated_function_bindings, 2);
    assert!(next.functions.changed_indices(&old.functions).is_empty());
    assert_eq!(
        session
            .last_product_changes()
            .iter()
            .filter(|r| !r.reused)
            .count(),
        1
    );
}
#[test]
fn ordinary_unsafe_lexical_spans_are_never_ignored_as_diagnostic_relocations() {
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
    let function = &mut source.module.functions[0];
    function.span = Span::new(FileId(0), 0, 200);
    function.blocks[0].instructions = vec![
        Instruction {
            result: None,
            kind: InstructionKind::UnsafeEnter { region: 0 },
            span: Span::new(FileId(0), 100, 150),
        },
        Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::ForeignCall {
                function: FunctionId(3),
                arguments: vec![],
                region: 0,
            },
            span: Span::new(FileId(0), 110, 120),
        },
        Instruction {
            result: None,
            kind: InstructionKind::UnsafeExit { region: 0 },
            span: Span::new(FileId(0), 100, 150),
        },
    ];
    let source = Arc::new(source);
    let limits = Limits::default();
    let mut session = AnalysisSession::default();
    run(&mut session, &source, &limits);
    assert!(session.product.is_some());
    let previous = session.last_product_changes()[0];
    let mut escaped = (*source).clone();
    escaped.module.functions[0].blocks[0].instructions[1].span = Span::new(FileId(0), 160, 170);
    let escaped = Arc::new(escaped);
    // Ordinary foreign lexical safety belongs to independent current Verify.
    // Analyze must keep its existing StopAfterAnalyze/raw-candidate behavior,
    // while rebinding the actual current input instead of granting permission.
    assert!(expected(&escaped, &limits).is_ok());
    run(&mut session, &escaped, &limits);
    assert_eq!(session.last_stats().product_function_queries_executed, 1);
    assert_eq!(session.last_stats().product_relocated_function_bindings, 0);
    let current = session.last_product_changes()[0];
    assert!(!current.reused);
    assert_ne!(current.input, previous.input);
}
