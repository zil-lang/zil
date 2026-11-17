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
fn edit(source: &Arc<TypedProgram>, value: i32) -> Arc<TypedProgram> {
    let mut next = (**source).clone();
    next.module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(value));
    next.module.source_fingerprint = Fingerprint::of(&value.to_le_bytes());
    Arc::new(next)
}
fn expected(program: &TypedProgram, limits: &Limits) -> CompileResult<CandidatePlan> {
    analyze_program(program, limits).map(|mut plan| {
        plan.module_fingerprint = program.module.structural_fingerprint();
        plan
    })
}
fn seed(source: &Arc<TypedProgram>, limits: &Limits) -> RawAnalysisBase {
    let mut producer = AnalysisSession::default();
    let candidate = producer.analyze_product(source, limits).unwrap();
    producer.raw_product_for(source, &candidate).unwrap()
}
fn check(
    session: &mut AnalysisSession,
    source: &Arc<TypedProgram>,
    limits: &Limits,
    raw: &RawAnalysisBase,
) -> CompileResult<Arc<CandidatePlan>> {
    let result = session.analyze_product_with_raw(source, limits, Some(raw));
    assert_eq!(
        result.as_ref().map(|plan| plan.as_ref()),
        expected(source, limits).as_ref()
    );
    result
}

#[test]
fn raw_restart_private_edit_executes_only_changed_body_and_rebinds_current_plan() {
    let source = program();
    let limits = Limits::default();
    let raw = seed(&source, &limits);
    let mut current = (*edit(&source, 12345)).clone();
    for function in current.module.functions.iter_mut().skip(1) {
        function.span.start += 4;
        function.span.end += 4;
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                instruction.span.start += 4;
                instruction.span.end += 4;
            }
        }
    }
    let current = Arc::new(current);
    let mut restored = AnalysisSession::default();
    let result = check(&mut restored, &current, &limits, &raw).unwrap();
    let stats = restored.last_stats();
    assert_eq!(stats.product_function_queries_executed, 1);
    assert_eq!(stats.product_admission_functions_checked, 1);
    assert_eq!(stats.product_function_queries_reused, 2);
    assert_eq!(stats.product_global_obligations_reused, 1);
    assert_eq!(stats.raw_functions_restored, 2);
    assert!(restored.product_checkpoint_for(&source, &result).is_none());
    assert!(restored.product_checkpoint_for(&current, &result).is_some());
    let exported = restored.raw_product_for(&current, &result).unwrap();
    assert_eq!(exported.global_cost, raw.global_cost);
    for (index, row) in exported.rows.iter().enumerate() {
        assert_eq!(row.input.as_ref(), &current.module.functions[index]);
        assert_eq!(
            row.costs.iter().sum::<usize>(),
            restored
                .product
                .as_ref()
                .unwrap()
                .total
                .get(index)
                .unwrap()
                .cost
        );
    }
}

#[test]
fn raw_restart_actual_body_aliases_share_facts_comparisons_and_keep_each_fee() {
    let mut source = (*program()).clone();
    let template = function(0, 7);
    source.module.functions = (0..128)
        .map(|id| {
            let mut function = template.clone();
            function.id = FunctionId(id);
            function.name = format!("alias_{id}");
            function
        })
        .collect();
    let source = Arc::new(source);
    let limits = Limits::default();
    let raw = seed(&source, &limits);
    let mut restored = AnalysisSession::default();
    // Relocate every outer diagnostic binding so all rows require a semantic
    // comparison while unchanged immutable columns still share one owner.
    let mut current = (*edit(&source, 8)).clone();
    for function in &mut current.module.functions {
        function.span.start += 4;
        function.span.end += 4;
    }
    check(&mut restored, &Arc::new(current), &limits, &raw).unwrap();
    let stats = restored.last_stats();
    assert_eq!(stats.raw_functions_restored, 127);
    assert_eq!(stats.raw_body_facts_built, 1);
    assert_eq!(stats.raw_body_facts_reused, 255);
    assert_eq!(stats.product_semantic_body_comparisons, 2);
    assert_eq!(stats.product_semantic_body_comparison_reuses, 126);
}

#[test]
fn raw_restart_complete_context_changes_and_malformed_rows_fall_back() {
    let source = program();
    let limits = Limits::default();
    let raw = seed(&source, &limits);
    for mutation in 0..6 {
        let mut current = (*source).clone();
        match mutation {
            0 => current.module.functions[0].name.push_str("_changed_header"),
            1 => current.module.module_id = ModuleId(9),
            2 => current
                .module
                .target
                .rule_version
                .push_str("_changed_target"),
            3 => current.module.schema_version += 1,
            4 => current.module.functions[0].span.file = FileId(9),
            _ => current.module.resources.push(ResourceDefinition {
                name: "Token".into(),
                payload: Type::I32,
                teardown: None,
            }),
        }
        let current = Arc::new(current);
        assert!(raw.epoch(&current, &limits).is_none());
        let _ = check(&mut AnalysisSession::default(), &current, &limits, &raw);
    }
    let changed_limits = Limits {
        max_ct_steps: limits.max_ct_steps - 1,
        ..limits.clone()
    };
    assert!(raw.epoch(&source, &changed_limits).is_none());
    check(
        &mut AnalysisSession::default(),
        &source,
        &changed_limits,
        &raw,
    )
    .unwrap();
    for mutation in 0..5 {
        let mut malformed = raw.clone();
        match mutation {
            0 => malformed.rows[0].costs = [usize::MAX; 5],
            1 => malformed.rows[0].costs = [0; 5],
            2 => Arc::make_mut(&mut malformed.rows[0].plan).function = FunctionId(9),
            3 => Arc::make_mut(&mut malformed.rows[0].plan).holds.clear(),
            _ => {
                malformed.rows.pop();
            }
        }
        assert!(malformed.epoch(&source, &limits).is_none());
        check(
            &mut AnalysisSession::default(),
            &source,
            &limits,
            &malformed,
        )
        .unwrap();
    }
}

#[test]
fn raw_restart_exact_fee_boundaries_and_original_first_errors_recover() {
    let source = program();
    let normal = Limits::default();
    let raw = seed(&source, &normal);
    let exact = raw.global_cost + raw.rows.iter().flat_map(|row| row.costs).sum::<usize>();
    for allowed in [exact - 1, exact, exact + 1] {
        let limits = Limits {
            max_analysis_steps: allowed,
            ..normal.clone()
        };
        let mut restored = AnalysisSession::default();
        let _ = check(&mut restored, &source, &limits, &raw);
        if allowed < exact {
            continue;
        }
        let bounded = seed(&source, &limits);
        check(
            &mut AnalysisSession::default(),
            &edit(&source, 21),
            &limits,
            &bounded,
        )
        .unwrap();
        let mut more = (*source).clone();
        more.module.functions[0].values.push(Type::I32);
        more.module.functions[0].blocks[0]
            .instructions
            .push(Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::Constant(Constant::I32(22)),
                span: Span::new(FileId(0), 11, 19),
            });
        let more = Arc::new(more);
        let _ = check(&mut AnalysisSession::default(), &more, &limits, &bounded);
    }
    let mut restored = AnalysisSession::default();
    let valid = check(&mut restored, &source, &normal, &raw).unwrap();
    let mut invalid = (*source).clone();
    invalid.module.functions[0].blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(2),
        arguments: vec![],
    };
    invalid.module.functions[2].parameters.push(LocalId(999));
    assert!(check(&mut restored, &Arc::new(invalid), &normal, &raw).is_err());
    assert!(restored.product_checkpoint_for(&source, &valid).is_some());
    check(&mut restored, &edit(&source, 9), &normal, &raw).unwrap();
    assert_eq!(restored.last_stats().product_function_queries_executed, 1);
}

#[test]
fn raw_restart_foreign_unsafe_diagnostic_placement_is_exact() {
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
    function.blocks[0].instructions = vec![
        Instruction {
            result: None,
            kind: InstructionKind::UnsafeEnter { region: 0 },
            span: function.span,
        },
        Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::ForeignCall {
                function: FunctionId(3),
                arguments: vec![],
                region: 0,
            },
            span: function.span,
        },
        Instruction {
            result: None,
            kind: InstructionKind::UnsafeExit { region: 0 },
            span: function.span,
        },
    ];
    let source = Arc::new(source);
    let limits = Limits::default();
    let raw = seed(&source, &limits);
    let mut shifted = (*source).clone();
    shifted.module.functions[0].span.start += 1;
    let shifted = Arc::new(shifted);
    let mut restored = AnalysisSession::default();
    let _ = check(&mut restored, &shifted, &limits, &raw);
    // A changed unsafe lexical input must either execute its original query or
    // fall back to the full original traversal; it is never relocated as equal.
    assert!(
        restored.last_stats().product_function_queries_executed != 0
            || restored.last_stats().product_global_obligations_executed != 0
    );
}

#[test]
fn raw_plan_shape_reuse_tracks_actual_lengths_and_rejects_other_local_bounds() {
    let mut source = (*program()).clone();
    source.module.functions = (0..3).map(|id| function(id, 7 + id as i32)).collect();
    let source = Arc::new(source);
    let limits = Limits::default();
    let mut raw = seed(&source, &limits);
    let first = raw.rows[0].plan.clone();
    for row in &mut raw.rows {
        assert_eq!(row.plan.blocks, first.blocks);
        assert_eq!(row.plan.holds, first.holds);
        let mut plan = row.plan.as_ref().clone();
        plan.blocks = first.blocks.clone();
        plan.holds = first.holds.clone();
        row.plan = Arc::new(plan);
    }
    let mut restored = AnalysisSession::default();
    check(&mut restored, &source, &limits, &raw).unwrap();
    assert_eq!(restored.last_stats().raw_plan_shapes_checked, 1);
    assert_eq!(restored.last_stats().raw_plan_shapes_reused, 2);
    // The same immutable plan column references local0. It fits the first
    // candidate's local collection but must miss for the next empty collection.
    let mut input = raw.rows[0].input.as_ref().clone();
    input.locals.push(LocalDecl {
        id: LocalId(0),
        name: "bound".into(),
        ty: Type::I32,
        mutable: false,
        span: Span::default(),
    });
    raw.rows[0].input = Arc::new(input);
    let mut plan = first.as_ref().clone();
    plan.blocks[0].initialized_in = vec![LocalId(0)];
    plan.blocks[0].initialized_out = vec![LocalId(0)];
    for row in &mut raw.rows {
        let mut shared = plan.clone();
        shared.function = row.input.id;
        row.plan = Arc::new(shared);
    }
    assert!(raw.epoch(&source, &limits).is_none());
}
