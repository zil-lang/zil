use super::*;

fn fixture() -> (TypedModule, CandidatePlan) {
    let function = TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(0),
        name: "raw".into(),
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
                kind: InstructionKind::Constant(Constant::I32(42)),
                span: Span::default(),
            }],
            terminator: Terminator::Return(Some(ValueId(0))),
        }]
        .into(),
        entry: BlockId(0),
        span: Span::default(),
    };
    let module = TypedModule {
        life_declarations: vec![],
        life_instances: vec![],
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(9),
        source_fingerprint: Fingerprint::of(b"raw fixture"),
        target: TargetProfile::default(),
        functions: (0..3)
            .map(|index| TypedFunction {
                id: FunctionId(index),
                name: format!("raw{index}"),
                ..function.clone()
            })
            .collect(),
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
    };
    let plan = CandidatePlan::empty_for(&module);
    (module, plan)
}

fn slots(module: &TypedModule, limits: &Limits) -> Vec<OnceLock<RawFunctionEncoding>> {
    module
        .functions
        .iter()
        .map(|function| {
            let slot = OnceLock::new();
            let mut writer = Writer::new(limits);
            if writer.function(function).is_ok() {
                assert!(
                    slot.set(RawFunctionEncoding {
                        bytes: writer.output.into(),
                        fingerprint: OnceLock::new(),
                        limits: limits.clone(),
                        nodes: limits.max_nodes - writer.nodes,
                    })
                    .is_ok()
                );
            }
            slot
        })
        .collect()
}

fn cached(
    module: &TypedModule,
    plan: &CandidatePlan,
    limits: &Limits,
    slots: &[OnceLock<RawFunctionEncoding>],
) -> (CompileResult<Vec<u8>>, FunctionEncodingStats) {
    let functions: Vec<_> = module.functions.iter().zip(slots).collect();
    let mut stats = FunctionEncodingStats::default();
    let bytes = encode_module_candidate_with_function_encodings(
        module, plan, &functions, limits, &mut stats,
    );
    (bytes, stats)
}

#[test]
fn raw_fragments_preserve_bytes_and_replay_exact_aggregate_nodes_and_bytes() {
    let (module, plan) = fixture();
    let normal = Limits::default();
    let expected = encode_module_candidate(&module, &plan, &normal).unwrap();
    let mut writer = Writer::new(&normal);
    writer.header(MODULE_MAGIC).unwrap();
    writer.module(&module).unwrap();
    // This fixture has three empty collection counts after its functions.
    let function_end = writer.output.len() - 12;
    writer.plan(&plan).unwrap();
    let node_count = normal.max_nodes - writer.nodes;
    let mut node_exact = normal.clone();
    node_exact.max_nodes = node_count;
    let mut node_short = node_exact.clone();
    node_short.max_nodes -= 1;
    let mut byte_exact = normal.clone();
    byte_exact.max_source_bytes = expected.len();
    let mut byte_short = byte_exact.clone();
    byte_short.max_source_bytes -= 1;
    let mut inside_function = normal.clone();
    inside_function.max_source_bytes = function_end - 1;
    for limits in [
        &normal,
        &node_exact,
        &node_short,
        &byte_exact,
        &byte_short,
        &inside_function,
    ] {
        let slots = slots(&module, limits);
        let (actual, stats) = cached(&module, &plan, limits, &slots);
        assert_eq!(actual, encode_module_candidate(&module, &plan, limits));
        if limits == &normal || limits == &node_exact || limits == &byte_exact {
            assert_eq!(actual.unwrap(), expected);
            assert_eq!(
                stats,
                FunctionEncodingStats {
                    serialized_functions: 0,
                    reused_functions: 3,
                }
            );
        } else {
            assert!(actual.is_err());
            if limits == &node_short || limits == &inside_function {
                assert!(stats.reused_functions > 0);
                assert_eq!(stats.serialized_functions, 1);
            }
        }
    }
}

#[test]
fn cold_fragments_are_saved_once_and_changed_limits_use_original_traversal() {
    let (module, plan) = fixture();
    let normal = Limits::default();
    let slots: Vec<_> = (0..3).map(|_| OnceLock::new()).collect();
    let (first, cold) = cached(&module, &plan, &normal, &slots);
    assert_eq!(
        cold,
        FunctionEncodingStats {
            serialized_functions: 3,
            reused_functions: 0
        }
    );
    assert_eq!(first, encode_module_candidate(&module, &plan, &normal));
    assert_eq!(
        cached(&module, &plan, &normal, &slots).1,
        FunctionEncodingStats {
            serialized_functions: 0,
            reused_functions: 3
        }
    );
    let mut changed = normal.clone();
    changed.max_tokens -= 1; // Even a limit unused by Wire must bind exactly.
    for _ in 0..2 {
        let (actual, stats) = cached(&module, &plan, &changed, &slots);
        assert_eq!(actual, first);
        assert_eq!(stats, cold);
    }
    assert!(
        slots
            .iter()
            .all(|slot| slot.get().unwrap().limits == normal)
    );
}

#[test]
fn partial_publication_cannot_accumulate_one_byte_quota_per_entry() {
    let (mut module, plan) = fixture();
    // Ensure the uncached first function fits after the module header, even
    // though existing fragments leave no room for another cached fragment.
    for function in &mut module.functions {
        function.name.push_str(&"x".repeat(256));
    }
    let mut limits = Limits::default();
    let mut standalone = Writer::new(&limits);
    standalone.function(&module.functions[0]).unwrap();
    limits.max_source_bytes = standalone.output.len() * 2;
    // Old, unchanged entries may already have fragments after an earlier epoch.
    let mut slots = slots(&module, &limits);
    slots[0] = OnceLock::new();
    module.functions[0].name.push_str("edited");
    let retained = slots
        .iter()
        .filter_map(OnceLock::get)
        .map(RawFunctionEncoding::byte_len)
        .sum::<usize>();
    assert_eq!(retained, limits.max_source_bytes);
    let (actual, stats) = cached(&module, &plan, &limits, &slots);
    assert!(actual.is_err());
    assert_eq!(stats.serialized_functions, 2);
    assert!(slots[0].get().is_none());
    assert_eq!(
        slots
            .iter()
            .filter_map(OnceLock::get)
            .map(RawFunctionEncoding::byte_len)
            .sum::<usize>(),
        retained
    );
}

#[test]
fn segmented_publication_preserves_stream_limits_and_shared_fragments() {
    let (module, plan) = fixture();
    let limits = Limits::default();
    let expected = encode_module_candidate(&module, &plan, &limits).unwrap();
    let slots: Vec<_> = module.functions.iter().map(|_| OnceLock::new()).collect();
    let functions: Vec<_> = module.functions.iter().zip(&slots).collect();
    let mut stats = FunctionEncodingStats::default();
    let cold =
        encode_module_candidate_segments(&module, &plan, &functions, &limits, &mut stats).unwrap();
    assert_eq!(stats.serialized_functions, 3);
    let warm =
        encode_module_candidate_segments(&module, &plan, &functions, &limits, &mut stats).unwrap();
    for segments in [&cold, &warm] {
        let actual: Vec<u8> = segments
            .iter()
            .flat_map(|s| s.bytes.iter().copied())
            .collect();
        assert_eq!(actual, expected);
        assert!(
            segments
                .iter()
                .all(|s| s.fingerprint == Fingerprint::of(&s.bytes))
        );
    }
    for (a, b) in cold[1..4].iter().zip(&warm[1..4]) {
        assert!(Arc::ptr_eq(&a.bytes, &b.bytes));
    }
    for bytes in [expected.len(), expected.len() - 1, expected.len() / 2] {
        for nodes in [limits.max_nodes, 0, 1, 9] {
            let selected = Limits {
                max_source_bytes: bytes,
                max_nodes: nodes,
                ..limits.clone()
            };
            let result =
                encode_module_candidate_segments(&module, &plan, &functions, &selected, &mut stats)
                    .map(|segments| {
                        segments
                            .iter()
                            .flat_map(|s| s.bytes.iter().copied())
                            .collect::<Vec<_>>()
                    });
            assert_eq!(result, encode_module_candidate(&module, &plan, &selected));
        }
    }
}

fn plans_fixture() -> (TypedModule, CandidatePlan) {
    let (module, mut plan) = fixture();
    plan.functions = module
        .functions
        .iter()
        .map(|function| FunctionPlan {
            function: function.id,
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: vec![LocalId(0)],
                initialized_out: vec![LocalId(0), LocalId(1)],
            }],
            holds: vec![ValueHold {
                value: ValueId(0),
                roots: vec![
                    HoldRoot::Local(LocalId(0)),
                    HoldRoot::Parameter(0),
                    HoldRoot::Static,
                ],
            }],
        })
        .collect();
    (module, plan)
}

fn cached_plans(
    module: &TypedModule,
    plan: &CandidatePlan,
    limits: &Limits,
    functions: &[OnceLock<RawFunctionEncoding>],
    plans: &mut [Option<Arc<RawFunctionPlanEncoding>>],
) -> (CompileResult<Vec<u8>>, FunctionPlanEncodingStats) {
    let functions: Vec<_> = module.functions.iter().zip(functions).collect();
    let mut stats = FunctionPlanEncodingStats::default();
    let result = encode_module_candidate_plan_segments(
        module,
        plan,
        &functions,
        plans,
        limits,
        &mut FunctionEncodingStats::default(),
        &mut stats,
    )
    .map(|segments| {
        segments
            .iter()
            .flat_map(|s| s.bytes.iter().copied())
            .collect()
    });
    (result, stats)
}

#[test]
fn plan_fragments_bind_exact_rows_and_reuse_equal_outputs_after_module_changes() {
    let (mut module, mut plan) = plans_fixture();
    let limits = Limits::default();
    let functions = slots(&module, &limits);
    let mut plans = vec![None, None, None];
    let (cold, stats) = cached_plans(&module, &plan, &limits, &functions, &mut plans);
    assert_eq!(cold, encode_module_candidate(&module, &plan, &limits));
    assert_eq!(stats.serialized_function_plans, 3);
    let old = plans.clone();
    module.source_fingerprint = Fingerprint::of(b"different current source");
    plan.module_fingerprint = module.fingerprint();
    let (edited, stats) = cached_plans(&module, &plan, &limits, &functions, &mut plans);
    assert_eq!(edited, encode_module_candidate(&module, &plan, &limits));
    assert_ne!(cold, edited);
    assert_eq!(
        (stats.serialized_function_plans, stats.reused_function_plans),
        (0, 3)
    );
    assert_eq!(
        stats.reused_function_plan_bytes,
        old.iter()
            .flatten()
            .map(|raw| raw.encoding.byte_len())
            .sum::<usize>()
    );
    for (old, current) in old.iter().zip(&plans) {
        assert!(Arc::ptr_eq(
            old.as_ref().unwrap(),
            current.as_ref().unwrap()
        ));
    }
    // No public digest or unchanged enclosing module binding can mask a changed
    // fact. A stale private slot is checked against the complete actual row.
    plan.functions[1].blocks[0].initialized_out.pop();
    let (changed, stats) = cached_plans(&module, &plan, &limits, &functions, &mut plans);
    assert_eq!(changed, encode_module_candidate(&module, &plan, &limits));
    assert_eq!(
        (stats.serialized_function_plans, stats.reused_function_plans),
        (1, 2)
    );
    assert!(!Arc::ptr_eq(
        old[1].as_ref().unwrap(),
        plans[1].as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        old[0].as_ref().unwrap(),
        plans[0].as_ref().unwrap()
    ));
    let mut changed_limits = limits.clone();
    changed_limits.max_tokens -= 1;
    assert_eq!(
        cached_plans(&module, &plan, &changed_limits, &functions, &mut plans)
            .1
            .serialized_function_plans,
        3
    );
    assert_eq!(
        cached_plans(&module, &plan, &changed_limits, &functions, &mut plans)
            .1
            .reused_function_plans,
        3
    );
    assert!(
        plans
            .iter()
            .flatten()
            .all(|raw| raw.encoding.limits == changed_limits)
    );
    // Global claims and target binding remain current even when every local
    // row is equal. They are deliberately outside the fragment cache.
    plan.target_fingerprint = Fingerprint::of(b"different target claim");
    plan.origins.push(OriginClaim {
        origin: OriginId(0),
        source: NodeId(0),
        multiplicity: OriginMultiplicity::Many,
    });
    plan.responsibilities.push(ResponsibilityClaim {
        function: FunctionId(0),
        carrier: LocalId(0),
        origin: OriginId(0),
        state: ResponsibilityState::Live,
    });
    let (actual, stats) = cached_plans(&module, &plan, &changed_limits, &functions, &mut plans);
    assert_eq!(
        actual,
        encode_module_candidate(&module, &plan, &changed_limits)
    );
    assert_eq!(stats.reused_function_plans, 3);
}

#[test]
fn plan_fragment_replay_preserves_aggregate_first_errors_and_exact_boundaries() {
    let (module, plan) = plans_fixture();
    let normal = Limits::default();
    let mut writer = Writer::new(&normal);
    writer.header(MODULE_MAGIC).unwrap();
    writer.module(&module).unwrap();
    writer.plan(&plan).unwrap();
    let length = writer.output.len();
    let node_count = normal.max_nodes - writer.nodes;
    for bytes in [length, length - 1, length - 13, length / 2] {
        for nodes in [node_count, node_count - 1, node_count - 4, 0] {
            let limits = Limits {
                max_source_bytes: bytes,
                max_nodes: nodes,
                ..normal.clone()
            };
            let functions = slots(&module, &limits);
            let mut plans = vec![None, None, None];
            let (cold, _) = cached_plans(&module, &plan, &limits, &functions, &mut plans);
            assert_eq!(cold, encode_module_candidate(&module, &plan, &limits));
            let (warm, _) = cached_plans(&module, &plan, &limits, &functions, &mut plans);
            assert_eq!(warm, cold);
            let cached_bytes = functions
                .iter()
                .filter_map(OnceLock::get)
                .map(RawFunctionEncoding::byte_len)
                .chain(plans.iter().flatten().map(|raw| raw.encoding.byte_len()))
                .sum::<usize>();
            assert!(cached_bytes <= limits.max_source_bytes);
        }
    }
    // Warm every row under exactly the selected limit, then consume additional
    // aggregate allowance before it without changing any cached plan's facts.
    for (extra_bytes, extra_nodes) in [(13, 0), (0, 4)] {
        let limits = Limits {
            max_source_bytes: length,
            max_nodes: node_count,
            ..normal.clone()
        };
        let functions = slots(&module, &limits);
        let mut plans = vec![None, None, None];
        assert!(
            cached_plans(&module, &plan, &limits, &functions, &mut plans)
                .0
                .is_ok()
        );
        let checkpoint = plans.clone();
        let mut changed = module.clone();
        changed.functions[0].name.push_str(&"x".repeat(extra_bytes));
        changed.functions[0]
            .values
            .extend(std::iter::repeat_n(Type::I32, extra_nodes));
        let changed_functions = slots(&changed, &limits);
        let (actual, stats) =
            cached_plans(&changed, &plan, &limits, &changed_functions, &mut plans);
        assert!(actual.is_err());
        assert_eq!(actual, encode_module_candidate(&changed, &plan, &limits));
        assert!(stats.reused_function_plans > 0);
        assert!(stats.serialized_function_plans > 0);
        // Failed work cannot mutate the immutable checkpoint's fragments.
        assert!(
            checkpoint
                .iter()
                .flatten()
                .all(|raw| raw.encoding.limits == limits)
        );
    }
}
