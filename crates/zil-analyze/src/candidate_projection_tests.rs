use super::*;
use zil_base::{FileId, ModuleId, SCHEMA_VERSION, Span, TargetProfile};

fn function(id: u32) -> TypedFunction {
    let span = Span::new(FileId(id), id as usize * 10, id as usize * 10 + 8);
    TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(id),
        name: format!("leaf{id}"),
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
                kind: InstructionKind::Constant(Constant::I32(7)),
                span,
            }],
            terminator: Terminator::Return(Some(ValueId(0))),
        }]
        .into(),
        entry: BlockId(0),
        span,
    }
}
fn program() -> Arc<TypedProgram> {
    let first = function(0);
    let functions = (0..16)
        .map(|id| {
            let mut f = first.clone();
            f.id = FunctionId(id);
            f.name = format!("leaf{id}");
            f.span = function(id).span;
            f
        })
        .collect();
    Arc::new(TypedProgram {
        task_interfaces: vec![],
        interfaces: vec![],
        root_functions: vec![],
        module: TypedModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::default(),
            functions,
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
fn ordinary(program: &TypedProgram, limits: &Limits) -> (CompileResult<CandidatePlan>, usize) {
    let mut budget = Budget::new(limits);
    let result = analyze_budgeted(&program.module, limits, &mut budget, None).and_then(|plan| {
        tasks::interfaces(program, &mut budget)?;
        Ok(plan)
    });
    (result, budget.remaining())
}

#[test]
fn equal_body_projection_rebinds_ids_names_and_spans_but_compares_semantics() {
    let first = function(0);
    let separate = function(1);
    let mut shared = first.clone();
    shared.id = FunctionId(2);
    shared.name = "new-name".into();
    shared.span = separate.span;
    let mut cache = Inputs::new(1_000_000);
    let (a, reused) = cache.input(&first).unwrap();
    assert!(!reused);
    for f in [&separate, &shared] {
        let (actual, reused) = cache.input(f).unwrap();
        assert!(reused);
        let (fresh, reused) = Inputs::new(0).input(f).unwrap();
        assert!(!reused);
        assert_eq!(actual, fresh);
        assert_ne!(actual.0, a.0);
        assert_eq!(actual.2, a.2);
    }
    let mut changed = function(3);
    changed.blocks[0].instructions[0].kind = InstructionKind::Constant(Constant::I32(9));
    // Deliberately collide the selector index: full field equality must reject.
    let mut selector = DefaultHasher::new();
    scalar_projection(&changed, &mut selector).unwrap();
    cache.selectors.insert(selector.finish(), vec![0]);
    assert!(!cache.input(&changed).unwrap().1);
    let mut invalid = shared.clone();
    invalid.values = vec![Type::Bool].into();
    assert!(!cache.input(&invalid).unwrap().1);
    let mut call = function(4);
    call.blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: vec![],
    };
    assert_eq!(cache.input(&call).unwrap().0.1, vec![FunctionId(0)]);
    assert!(!cache.input(&call).unwrap().1);
}

#[test]
fn current_publication_shares_projection_work_without_sharing_function_identity() {
    let source = program();
    let limits = Limits::default();
    let expected = ordinary(&source, &limits).0.unwrap();
    let mut session = AnalysisSession::default();
    let actual = session.analyze_shared_program(&source, &limits).unwrap();
    assert_eq!(*actual, expected);
    let stats = session.last_stats();
    assert_eq!(stats.candidate_inputs_built, 1);
    assert_eq!(stats.candidate_input_projection_reuses, 15);
    assert_eq!(stats.candidate_output_projection_reuses, 15);
    assert_eq!(stats.candidate_executed_functions, 16);
    let rows = session.last_function_results();
    for pair in rows.windows(2) {
        assert_ne!(pair[0].input, pair[1].input);
        assert_ne!(pair[0].output, pair[1].output);
    }
    let dependencies = rows[0].dependencies;
    let mut changed = (*source).clone();
    changed.module.module_id = ModuleId(71);
    let changed = Arc::new(changed);
    assert_eq!(
        *session.analyze_shared_program(&changed, &limits).unwrap(),
        ordinary(&changed, &limits).0.unwrap()
    );
    assert_ne!(
        session.last_function_results()[0].dependencies,
        dependencies
    );
    assert_eq!(session.last_stats().candidate_reused_functions, 0);
}

#[test]
fn projection_reuse_keeps_exact_budget_and_bad_neighbor_first_diagnostics() {
    let source = program();
    let limits = Limits::default();
    let (expected, remaining) = ordinary(&source, &limits);
    expected.unwrap();
    let used = limits.max_analysis_steps - remaining;
    for amount in [0, 1, used / 2, used - 1, used, used + 1] {
        let limits = Limits {
            max_analysis_steps: amount,
            ..limits.clone()
        };
        let expected = ordinary(&source, &limits).0;
        assert_eq!(
            AnalysisSession::default()
                .analyze_shared_program(&source, &limits)
                .as_deref(),
            expected.as_ref()
        );
    }
    let mut bad = (*source).clone();
    bad.module.functions[14].blocks[0].instructions[0].kind = InstructionKind::Load(LocalId(99));
    bad.module.functions[14].blocks[0].instructions[0].span = Span::new(FileId(99), 400, 409);
    let bad = Arc::new(bad);
    let expected = ordinary(&bad, &limits).0;
    assert!(expected.is_err());
    let mut session = AnalysisSession::default();
    session.analyze_shared_program(&source, &limits).unwrap();
    assert_eq!(
        session.analyze_shared_program(&bad, &limits).as_deref(),
        expected.as_ref()
    );
}
