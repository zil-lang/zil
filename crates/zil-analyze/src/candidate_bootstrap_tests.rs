use super::*;
use zil_base::{FileId, ModuleId, SCHEMA_VERSION, Span, TargetProfile};

fn function(id: u32) -> TypedFunction {
    let span = Span::new(FileId(id), id as usize * 100, id as usize * 100 + 80);
    let instruction = |result, kind| Instruction {
        result: Some(ValueId(result)),
        kind,
        span,
    };
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
        parameters: vec![LocalId(0)],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "argument".into(),
            ty: Type::I32,
            mutable: false,
            span,
        }]
        .into(),
        values: vec![Type::I32, Type::I32, Type::Bool, Type::I32, Type::I32].into(),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    instruction(0, InstructionKind::Load(LocalId(0))),
                    instruction(1, InstructionKind::Constant(Constant::I32(0))),
                    instruction(
                        2,
                        InstructionKind::Binary {
                            op: BinaryOp::Less,
                            left: ValueId(0),
                            right: ValueId(1),
                        },
                    ),
                ],
                terminator: Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![instruction(3, InstructionKind::Constant(Constant::I32(7)))],
                terminator: Terminator::Return(Some(ValueId(3))),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![instruction(4, InstructionKind::Constant(Constant::I32(9)))],
                terminator: Terminator::Return(Some(ValueId(4))),
            },
        ]
        .into(),
        entry: BlockId(0),
        span,
    }
}

fn program() -> Arc<TypedProgram> {
    let mut functions = (0..16).map(function).collect::<Vec<_>>();
    functions[15].blocks[2].instructions[0].kind = InstructionKind::Constant(Constant::I32(11));
    for id in [16, 17] {
        let mut caller = function(id);
        caller.values = vec![Type::I32, Type::I32].into();
        caller.blocks = vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Load(LocalId(0)),
                    span: caller.span,
                },
                Instruction {
                    result: Some(ValueId(1)),
                    kind: InstructionKind::Call {
                        function: FunctionId(0),
                        arguments: vec![ValueId(0)],
                    },
                    span: caller.span,
                },
            ],
            terminator: Terminator::Return(Some(ValueId(1))),
        }]
        .into();
        functions.push(caller);
    }
    Arc::new(TypedProgram {
        task_interfaces: vec![],
        interfaces: vec![],
        root_functions: vec![],
        module: TypedModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::default(),
            functions: functions.into(),
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

fn original(program: &TypedProgram, limits: &Limits) -> (CompileResult<CandidatePlan>, usize) {
    let mut budget = Budget::new(limits);
    budget.product_mode = true;
    let result = analyze_budgeted(&program.module, limits, &mut budget, None).and_then(|plan| {
        tasks::interfaces(program, &mut budget)?;
        Ok(plan)
    });
    (result, budget.remaining())
}

#[test]
fn cold_current_templates_share_real_stages_and_rebind_each_candidate_identity() {
    let source = program();
    let limits = Limits::default();
    let expected = original(&source, &limits).0.unwrap();
    for _ in 0..2 {
        let mut session = AnalysisSession::default();
        let actual = session.analyze_product(&source, &limits).unwrap();
        assert_eq!(*actual, expected);
        let stats = session.last_stats();
        assert_eq!(stats.bootstrap_templates, 2);
        assert_eq!(stats.bootstrap_initialization_reuses, 14);
        assert_eq!(stats.bootstrap_local_reuses, 14);
        assert_eq!(stats.bootstrap_scope_reuses, 14);
        assert_eq!(stats.bootstrap_hold_reuses, 14);
        assert_eq!(stats.initialization_inputs_built, 4);
        assert_eq!(stats.dependency_signatures_checked, 2);
        // Calls do not enter this same-request body product. The 18 current
        // candidate queries and their complete current global obligations still
        // publish distinct IDs, input/output fingerprints and real epoch rows.
        assert_eq!(stats.candidate_executed_functions, 18);
        assert_eq!(stats.product_function_queries_executed, 18);
        assert_eq!(stats.product_global_obligations_executed, 1);
        for (index, row) in actual.functions.iter().enumerate() {
            assert_eq!(row.function, FunctionId(index as u32));
        }
        assert!(actual.functions[0].holds.ptr_eq(&actual.functions[1].holds));
        let changes = session.last_product_changes();
        assert_ne!(changes[0].input, changes[1].input);
        assert_ne!(changes[0].output, changes[1].output);
        let checkpoint = session.product_checkpoint_for(&source, &actual).unwrap();
        assert!(checkpoint.last_stats().bootstrap_hold_reuses > 0);
        let mut edited = (*source).clone();
        edited.module.functions[1].blocks[2].instructions[0].kind =
            InstructionKind::Constant(Constant::I32(23));
        let edited = Arc::new(edited);
        assert_eq!(
            *session.analyze_product(&edited, &limits).unwrap(),
            original(&edited, &limits).0.unwrap()
        );
        assert_eq!(session.last_stats().product_function_queries_executed, 1);
    }
}

#[test]
fn current_templates_keep_original_budget_first_errors_and_successful_epoch_on_failure() {
    let source = program();
    let limits = Limits::default();
    let (expected, remaining) = original(&source, &limits);
    expected.unwrap();
    let used = limits.max_analysis_steps - remaining;
    for amount in [
        0,
        1,
        used / 4,
        used / 2,
        used * 3 / 4,
        used - 1,
        used,
        used + 1,
    ] {
        let current = Limits {
            max_analysis_steps: amount,
            ..limits.clone()
        };
        let expected = original(&source, &current).0;
        let mut session = AnalysisSession::default();
        let actual = session.analyze_product(&source, &current);
        assert_eq!(actual.as_deref(), expected.as_ref(), "work {amount}");
    }
    let mut session = AnalysisSession::default();
    let good = session.analyze_product(&source, &limits).unwrap();
    let mut malformed = (*source).clone();
    let span = malformed.module.functions[14].span;
    for _ in 0..2 {
        malformed.module.functions[14].blocks[0]
            .instructions
            .push(Instruction {
                result: None,
                kind: InstructionKind::ScopeEnter { scope: ScopeId(0) },
                span,
            });
    }
    let malformed = Arc::new(malformed);
    let expected = original(&malformed, &limits).0;
    assert!(expected.is_err());
    assert_eq!(
        session.analyze_product(&malformed, &limits).as_deref(),
        expected.as_ref()
    );
    assert!(session.product_checkpoint_for(&source, &good).is_some());
    assert_eq!(*session.analyze_product(&source, &limits).unwrap(), *good);
    let short = Limits {
        max_blocks: source
            .module
            .functions
            .iter()
            .map(|f| f.blocks.len())
            .sum::<usize>()
            - 1,
        ..limits
    };
    assert_eq!(
        AnalysisSession::default()
            .analyze_product(&source, &short)
            .as_deref(),
        original(&source, &short).0.as_ref()
    );
}

#[test]
fn selector_collisions_compare_complete_body_and_optional_fuel_never_admits_work() {
    let first = function(0);
    let equal = function(1);
    let mut changed = function(2);
    changed.blocks[2].instructions[0].kind = InstructionKind::Constant(Constant::I32(99));
    let mut templates = Templates::new(1_000_000);
    assert_eq!(templates.bind(&first, 0), None);
    assert_eq!(templates.bind(&equal, 0), Some(first.id));
    assert_eq!(templates.bind(&changed, 0), None);
    let mut locals = function(3);
    locals.locals[0].mutable = true;
    assert_eq!(templates.bind(&locals, 0), None);
    let mut empty = Templates::new(0);
    assert_eq!(empty.bind(&first, 0), None);
    assert_eq!(empty.bind(&equal, 0), None);
    assert_eq!(empty.len(), 0);
}
