use super::*;

#[test]
fn scalar_header_reuse_rebinds_real_owner_and_shares_only_equal_bodies() {
    let (module, _) = fixture();
    let limits = Limits::default();
    let mut original = module.functions[0].clone();
    original.blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(7),
        arguments: vec![],
    };
    let encoded = encode_raw_function(&original, &limits).unwrap();
    let (owner, raw) =
        decode_raw_function_shared_owner(encoded.shared_bytes(), &limits, limits.max_nodes)
            .unwrap();
    let selector =
        RawFunctionEncoding::scalar_header_selector(&raw.shared_bytes(), &limits).unwrap();
    for (id, name) in [
        (original.id, "diagnostic-only"),
        (FunctionId(99), original.name.as_str()),
        (FunctionId(100), "当前诊断名字"),
    ] {
        let mut current = original.clone();
        current.id = id;
        current.name = name.into();
        let bytes = encode_raw_function(&current, &limits)
            .unwrap()
            .shared_bytes();
        assert_eq!(
            RawFunctionEncoding::scalar_header_selector(&bytes, &limits),
            Some(selector)
        );
        let (actual, rebound) = raw
            .reuse_decoded_scalar_header(bytes.clone(), &limits, raw.node_cost())
            .unwrap();
        assert_eq!(actual.as_ref(), &current);
        assert!(actual.blocks.ptr_eq(&owner.blocks));
        assert!(actual.locals.ptr_eq(&owner.locals));
        assert!(actual.values.ptr_eq(&owner.values));
        assert!(Arc::ptr_eq(rebound.decoded_function().unwrap(), &actual));
        assert!(Arc::ptr_eq(&rebound.shared_bytes(), &bytes));
        assert_eq!(rebound.content_fingerprint(), Fingerprint::of(&bytes));
        assert_eq!(rebound.node_cost(), raw.node_cost());
        assert_eq!(
            decode_raw_function(&bytes, &limits).unwrap(),
            (current, raw.node_cost())
        );
        if id != original.id {
            assert!(
                raw.reuse_decoded_function(bytes.clone(), &limits, limits.max_nodes)
                    .is_none()
            );
        }
        assert!(
            encoded
                .reuse_decoded_scalar_header(bytes, &limits, limits.max_nodes)
                .is_none()
        );
    }

    let mut siblings = Vec::new();
    let mut changed = original.clone();
    changed.blocks[0].instructions[0].kind = InstructionKind::Constant(Constant::I32(7));
    siblings.push(changed);
    let mut changed = original.clone();
    changed.blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(8),
        arguments: vec![],
    };
    siblings.push(changed);
    let mut changed = original.clone();
    changed.span.end += 1;
    siblings.push(changed);
    let mut changed = original.clone();
    changed.blocks[0].instructions[0].span.end += 1;
    siblings.push(changed);
    let mut changed = original.clone();
    changed.return_type = Type::I64;
    siblings.push(changed);
    let mut changed = original.clone();
    changed.result_mode = PassingMode::Copy;
    siblings.push(changed);
    let mut changed = original.clone();
    changed.call_kind = CallKind::Suspend;
    siblings.push(changed);
    let mut changed = original.clone();
    changed.reference_return_bound = Some(vec![]);
    siblings.push(changed);
    for mut changed in siblings {
        changed.id = FunctionId(99);
        changed.name = "different-header".into();
        let bytes = encode_raw_function(&changed, &limits)
            .unwrap()
            .shared_bytes();
        assert_ne!(
            RawFunctionEncoding::scalar_header_selector(&bytes, &limits),
            Some(selector)
        );
        assert!(
            raw.reuse_decoded_scalar_header(bytes, &limits, limits.max_nodes)
                .is_none()
        );
    }
}

#[test]
fn scalar_header_reuse_declines_bad_profiles_versions_strings_and_quotas() {
    let (module, plan) = fixture();
    let limits = Limits::default();
    let encoded = encode_raw_function(&module.functions[0], &limits).unwrap();
    let (owner, raw) =
        decode_raw_function_shared_owner(encoded.shared_bytes(), &limits, limits.max_nodes)
            .unwrap();
    let bytes = raw.shared_bytes();
    let range = raw.decoded.as_ref().unwrap().name.clone();
    assert_eq!(
        range.start, 10,
        "actual Reader boundary for the guarded v47 profile"
    );
    assert_eq!(raw_scalar_function_id(&bytes), Some(owner.id));
    assert!(
        raw.reuse_decoded_scalar_header(bytes.clone(), &limits, raw.node_cost() - 1)
            .is_none()
    );
    let changed_limits = Limits {
        max_tokens: limits.max_tokens - 1,
        ..limits.clone()
    };
    assert!(
        raw.reuse_decoded_scalar_header(bytes.clone(), &changed_limits, limits.max_nodes)
            .is_none()
    );
    let mut bad_utf8 = bytes.to_vec();
    bad_utf8[range.start + 4] = 0xff;
    let mut bad_length = bytes.to_vec();
    bad_length[range.start..range.start + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut bad_profile = bytes.to_vec();
    bad_profile[0] = 2;
    for bad in [bytes[..9].to_vec(), bad_utf8, bad_length, bad_profile] {
        assert!(RawFunctionEncoding::scalar_header_selector(&bad, &limits).is_none());
        assert!(decode_raw_function(&bad, &limits).is_err());
        assert!(
            raw.reuse_decoded_scalar_header(bad.into(), &limits, limits.max_nodes)
                .is_none()
        );
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(decode_raw_function(&trailing, &limits).is_err());
    assert!(
        raw.reuse_decoded_scalar_header(trailing.into(), &limits, limits.max_nodes)
            .is_none()
    );
    // Raw fragments have no embedded version: enclosing versioned records must
    // never be interpreted as a current scalar header, including future ones.
    let mut future = encode_module_candidate(&module, &plan, &limits).unwrap();
    future[8..12].copy_from_slice(&(WIRE_VERSION + 1).to_le_bytes());
    assert!(decode_module_candidate(&future, &limits).is_err());
    assert!(RawFunctionEncoding::scalar_header_selector(&future, &limits).is_none());
    assert!(
        raw.reuse_decoded_scalar_header(future.into(), &limits, limits.max_nodes)
            .is_none()
    );
    let tight = Limits {
        max_source_bytes: bytes.len(),
        ..limits.clone()
    };
    let (_, tight_raw) =
        decode_raw_function_shared_owner(bytes.clone(), &tight, tight.max_nodes).unwrap();
    let mut longer = owner.as_ref().clone();
    longer.name.push('x');
    let longer = encode_raw_function(&longer, &limits)
        .unwrap()
        .shared_bytes();
    assert!(
        tight_raw
            .reuse_decoded_scalar_header(longer, &tight, tight.max_nodes)
            .is_none()
    );
}

#[test]
fn decoded_owner_relabel_matches_real_writer_and_refuses_every_other_change() {
    let (module, _) = fixture();
    let limits = Limits::default();
    let source = &module.functions[0];
    let raw = encode_raw_function(source, &limits).unwrap();
    assert!(
        raw.bind_decoded_owner(Arc::new(source.clone()), &limits, usize::MAX)
            .is_none()
    );
    let (decoded, raw) =
        decode_raw_function_shared_owner(raw.shared_bytes(), &limits, limits.max_nodes).unwrap();
    for name in [
        source.name.as_str(),
        "long.current.diagnostic.name",
        "当前名字",
    ] {
        let mut current = decoded.as_ref().clone();
        current.name = name.into();
        let current = Arc::new(current);
        let expected = encode_raw_function(&current, &limits).unwrap();
        let bytes = expected.shared_bytes().len();
        let bound = raw
            .bind_decoded_owner(current.clone(), &limits, bytes)
            .unwrap();
        assert_eq!(bound.shared_bytes(), expected.shared_bytes());
        assert_eq!(bound.node_cost(), expected.node_cost());
        assert!(Arc::ptr_eq(bound.decoded_function().unwrap(), &current));
        assert!(
            raw.bind_decoded_owner(current.clone(), &limits, bytes - 1)
                .is_none()
        );
        let changed_limits = Limits {
            max_nodes: limits.max_nodes - 1,
            ..limits.clone()
        };
        assert!(
            raw.bind_decoded_owner(current, &changed_limits, usize::MAX)
                .is_none()
        );
    }
    let mut mutants = Vec::new();
    let mut changed = decoded.as_ref().clone();
    changed.blocks[0].instructions[0].kind = InstructionKind::Constant(Constant::I32(43));
    mutants.push(changed);
    let mut changed = decoded.as_ref().clone();
    changed.span.end += 1;
    mutants.push(changed);
    let mut changed = decoded.as_ref().clone();
    changed.id.0 += 1;
    mutants.push(changed);
    let mut changed = decoded.as_ref().clone();
    changed.return_type = Type::I64;
    mutants.push(changed);
    for changed in mutants {
        assert!(
            raw.bind_decoded_owner(Arc::new(changed), &limits, usize::MAX)
                .is_none()
        );
    }
}

#[test]
fn decoded_owner_fragments_keep_original_aggregate_n_and_n_minus_one_errors() {
    let (mut module, plan) = fixture();
    let limits = Limits::default();
    let normal = encode_module_candidate(&module, &plan, &limits).unwrap();
    let mut writer = Writer::new(&limits);
    writer.header(MODULE_MAGIC).unwrap();
    writer.module(&module).unwrap();
    writer.plan(&plan).unwrap();
    assert_eq!(writer.output, normal);
    let nodes = limits.max_nodes - writer.nodes;
    module.functions[0].name = "longer-current-name".into();
    let size = encode_module_candidate(&module, &plan, &limits)
        .unwrap()
        .len();
    for current in [
        Limits {
            max_nodes: nodes,
            ..limits.clone()
        },
        Limits {
            max_nodes: nodes - 1,
            ..limits.clone()
        },
        Limits {
            max_source_bytes: size,
            ..limits.clone()
        },
        Limits {
            max_source_bytes: size - 1,
            ..limits.clone()
        },
    ] {
        let slots = module
            .functions
            .iter()
            .map(|function| {
                // Prime each independently under the exact limits when it fits;
                // aggregate exhaustion must still use the original inner failure.
                let mut old = function.clone();
                old.name = "old".into();
                let raw = encode_raw_function(&old, &current).unwrap();
                let (_, raw) = decode_raw_function_shared_owner(
                    raw.shared_bytes(),
                    &current,
                    current.max_nodes,
                )
                .unwrap();
                OnceLock::from(
                    raw.bind_decoded_owner(
                        Arc::new(function.clone()),
                        &current,
                        current.max_source_bytes,
                    )
                    .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let actual = encode_module_candidate_with_function_encodings(
            &module,
            &plan,
            &module.functions.iter().zip(&slots).collect::<Vec<_>>(),
            &current,
            &mut FunctionEncodingStats::default(),
        );
        assert_eq!(actual, encode_module_candidate(&module, &plan, &current));
    }
}

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
                        decoded: None,
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
            }]
            .into(),
            holds: vec![ValueHold {
                value: ValueId(0),
                roots: vec![
                    HoldRoot::Local(LocalId(0)),
                    HoldRoot::Parameter(0),
                    HoldRoot::Static,
                ],
            }]
            .into(),
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
fn complete_decode_restores_shared_function_and_plan_receipts_once() {
    let (module, plan) = plans_fixture();
    let limits = Limits::default();
    let functions = slots(&module, &limits);
    let inputs: Vec<_> = module.functions.iter().zip(&functions).collect();
    let segments = encode_module_candidate_plan_segments(
        &module,
        &plan,
        &inputs,
        &mut vec![None; plan.functions.len()],
        &limits,
        &mut FunctionEncodingStats::default(),
        &mut FunctionPlanEncodingStats::default(),
    )
    .unwrap();
    let bytes: Vec<_> = segments
        .iter()
        .flat_map(|segment| segment.bytes.iter().copied())
        .collect();
    let decoded = decode_module_candidate_with_encodings(&bytes, &segments, &limits).unwrap();
    assert_eq!(decoded.module, module);
    assert_eq!(decoded.plan, plan);
    let receipts = decoded.encodings.unwrap();
    for (index, encoding) in receipts.functions.iter().enumerate() {
        assert!(Arc::ptr_eq(
            encoding.decoded_function().unwrap(),
            decoded.module.functions.owner(index).unwrap(),
        ));
        assert!(Arc::ptr_eq(
            &encoding.shared_bytes(),
            &segments[index + 1].bytes
        ));
        assert_eq!(
            encoding.node_cost(),
            functions[index].get().unwrap().node_cost()
        );
        assert!(encoding.matches_limits(&limits));
    }
    for (index, raw) in receipts.plans.iter().enumerate() {
        assert!(Arc::ptr_eq(
            &raw.shared_bytes(),
            &segments[module.functions.len() + 3 + index].bytes
        ));
        assert!(Arc::ptr_eq(
            raw,
            &raw.bind_owner(decoded.plan.functions.owner(index).unwrap())
                .unwrap()
        ));
        let mut writer = Writer::new(&limits);
        writer
            .function_plan(&decoded.plan.functions[index])
            .unwrap();
        assert_eq!(raw.encoding.node_cost(), limits.max_nodes - writer.nodes);
        let mut changed_row = decoded.plan.functions[index].clone();
        changed_row.holds[0].roots.push(HoldRoot::Static);
        assert!(raw.bind_owner(&Arc::new(changed_row)).is_none());
    }
    let functions: Vec<_> = receipts.functions.into_iter().map(OnceLock::from).collect();
    let inputs: Vec<_> = decoded.module.functions.iter().zip(&functions).collect();
    let mut plans: Vec<_> = receipts.plans.into_iter().map(Some).collect();
    let mut stats = FunctionEncodingStats::default();
    let mut plan_stats = FunctionPlanEncodingStats::default();
    let replay = encode_module_candidate_plan_segments(
        &decoded.module,
        &decoded.plan,
        &inputs,
        &mut plans,
        &limits,
        &mut stats,
        &mut plan_stats,
    )
    .unwrap();
    assert_eq!(stats.serialized_functions, 0);
    assert_eq!(stats.reused_functions, module.functions.len());
    assert_eq!(plan_stats.serialized_function_plans, 0);
    assert_eq!(plan_stats.reused_function_plans, plan.functions.len());
    assert_eq!(
        replay
            .iter()
            .flat_map(|segment| segment.bytes.iter().copied())
            .collect::<Vec<_>>(),
        bytes
    );
    let mut changed = limits.clone();
    changed.max_tokens -= 1;
    stats = FunctionEncodingStats::default();
    plan_stats = FunctionPlanEncodingStats::default();
    encode_module_candidate_plan_segments(
        &decoded.module,
        &decoded.plan,
        &inputs,
        &mut plans,
        &changed,
        &mut stats,
        &mut plan_stats,
    )
    .unwrap();
    assert_eq!(stats.serialized_functions, module.functions.len());
    assert_eq!(plan_stats.serialized_function_plans, plan.functions.len());

    let mut writer = Writer::new(&limits);
    writer.header(MODULE_MAGIC).unwrap();
    writer.module(&module).unwrap();
    writer.plan(&plan).unwrap();
    let nodes = limits.max_nodes - writer.nodes;
    for (maximum_nodes, maximum_bytes, success) in [
        (nodes, bytes.len(), true),
        (nodes - 1, bytes.len(), false),
        (nodes, bytes.len() - 1, false),
    ] {
        let selected = Limits {
            max_nodes: maximum_nodes,
            max_source_bytes: maximum_bytes,
            ..limits.clone()
        };
        let actual = decode_module_candidate_with_encodings(&bytes, &segments, &selected);
        assert_eq!(actual.is_ok(), success);
        assert_eq!(
            actual.map(|decoded| (decoded.module, decoded.plan)),
            decode_module_candidate(&bytes, &selected)
        );
        let replay = encode_module_candidate_plan_segments(
            &decoded.module,
            &decoded.plan,
            &inputs,
            &mut plans,
            &selected,
            &mut FunctionEncodingStats::default(),
            &mut FunctionPlanEncodingStats::default(),
        )
        .map(|segments| {
            segments
                .iter()
                .flat_map(|segment| segment.bytes.iter().copied())
                .collect::<Vec<_>>()
        });
        assert_eq!(replay, encode_module_candidate(&module, &plan, &selected));
    }
}

#[test]
fn decoded_owner_reuse_matches_complete_bytes_and_only_rebinds_the_name() {
    let (module, _) = fixture();
    let limits = Limits::default();
    let original = &module.functions[0];
    let encoded = encode_raw_function(original, &limits).unwrap();
    assert!(encoded.decoded_function().is_none());
    let data = encoded.shared_bytes();
    let (owner, receipt) =
        decode_raw_function_shared_owner(data.clone(), &limits, limits.max_nodes).unwrap();
    assert_eq!(raw_scalar_function_id(&data), Some(owner.id));
    let (same, same_receipt) = receipt
        .reuse_decoded_function(data.clone(), &limits, limits.max_nodes)
        .unwrap();
    assert!(Arc::ptr_eq(&same, &owner));
    assert!(Arc::ptr_eq(&same_receipt.shared_bytes(), &data));
    let mut renamed = original.clone();
    renamed.name = "@module17:function0".into();
    let renamed_bytes = encode_raw_function(&renamed, &limits)
        .unwrap()
        .shared_bytes();
    let (actual, renamed_receipt) = receipt
        .reuse_decoded_function(renamed_bytes.clone(), &limits, receipt.node_cost())
        .unwrap();
    assert_eq!(actual.as_ref(), &renamed);
    assert!(actual.blocks.ptr_eq(&owner.blocks));
    assert!(actual.locals.ptr_eq(&owner.locals));
    assert!(actual.values.ptr_eq(&owner.values));
    assert!(Arc::ptr_eq(
        renamed_receipt.decoded_function().unwrap(),
        &actual
    ));
    assert_eq!(renamed_receipt.node_cost(), receipt.node_cost());
    assert_eq!(
        decode_raw_function(&renamed_bytes, &limits).unwrap(),
        (renamed, renamed_receipt.node_cost()),
    );
    let mut changed = original.clone();
    changed.blocks[0].instructions[0].kind = InstructionKind::Constant(Constant::I32(99));
    let changed = encode_raw_function(&changed, &limits)
        .unwrap()
        .shared_bytes();
    assert!(
        receipt
            .reuse_decoded_function(changed, &limits, limits.max_nodes)
            .is_none()
    );
    let mut changed = limits.clone();
    changed.max_tokens -= 1;
    assert!(
        receipt
            .reuse_decoded_function(data.clone(), &changed, limits.max_nodes)
            .is_none()
    );
    assert!(
        receipt
            .reuse_decoded_function(data.clone(), &limits, receipt.node_cost() - 1)
            .is_none()
    );
    let mut trailing = data.to_vec();
    trailing.push(0);
    assert!(
        receipt
            .reuse_decoded_function(trailing.into(), &limits, limits.max_nodes)
            .is_none()
    );
    assert!(
        receipt
            .reuse_decoded_function(data[..data.len() - 1].into(), &limits, limits.max_nodes)
            .is_none()
    );
    let name = receipt.decoded.as_ref().unwrap().name.clone();
    let mut bad_utf8 = data.to_vec();
    bad_utf8[name.start + 4] = 0xff;
    assert!(decode_raw_function(&bad_utf8, &limits).is_err());
    assert!(
        receipt
            .reuse_decoded_function(bad_utf8.into(), &limits, limits.max_nodes)
            .is_none()
    );
    let mut bad_length = data.to_vec();
    bad_length[name.start..name.start + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(
        receipt
            .reuse_decoded_function(bad_length.into(), &limits, limits.max_nodes)
            .is_none()
    );
    let mut wrong_id = data.to_vec();
    wrong_id[6..10].copy_from_slice(&99u32.to_le_bytes());
    assert_eq!(raw_scalar_function_id(&wrong_id), Some(FunctionId(99)));
    assert!(
        receipt
            .reuse_decoded_function(wrong_id.into(), &limits, limits.max_nodes)
            .is_none()
    );
    assert!(
        encoded
            .reuse_decoded_function(data, &limits, limits.max_nodes)
            .is_none()
    );
}

#[test]
fn decoded_owner_cannot_be_retargeted_by_mutating_the_public_module() {
    let (module, plan) = fixture();
    let limits = Limits::default();
    let slots: Vec<_> = module.functions.iter().map(|_| OnceLock::new()).collect();
    let inputs: Vec<_> = module.functions.iter().zip(&slots).collect();
    let mut plans = vec![None; plan.functions.len()];
    let segments = encode_module_candidate_plan_segments(
        &module,
        &plan,
        &inputs,
        &mut plans,
        &limits,
        &mut FunctionEncodingStats::default(),
        &mut FunctionPlanEncodingStats::default(),
    )
    .unwrap();
    let bytes: Vec<_> = segments
        .iter()
        .flat_map(|segment| segment.bytes.iter().copied())
        .collect();
    let mut decoded = decode_module_candidate_with_encodings(&bytes, &segments, &limits).unwrap();
    let receipt = &decoded.encodings.as_ref().unwrap().functions[0];
    let before = receipt.decoded_function().unwrap().clone();
    decoded.module.functions[0].blocks[0].instructions[0].kind =
        InstructionKind::Constant(Constant::I32(77));
    assert!(!Arc::ptr_eq(
        &before,
        decoded.module.functions.owner(0).unwrap()
    ));
    let (actual, _) = receipt
        .reuse_decoded_function(segments[1].bytes.clone(), &limits, limits.max_nodes)
        .unwrap();
    assert!(Arc::ptr_eq(&actual, &before));
    assert_eq!(actual.as_ref(), &module.functions[0]);
    let wrong = encode_raw_function(&decoded.module.functions[0], &limits)
        .unwrap()
        .shared_bytes();
    assert!(
        receipt
            .reuse_decoded_function(wrong, &limits, limits.max_nodes)
            .is_none()
    );
}

#[test]
fn decoder_boundaries_and_bytes_override_forged_segment_tables() {
    let (module, plan) = plans_fixture();
    let limits = Limits::default();
    let functions = slots(&module, &limits);
    let inputs: Vec<_> = module.functions.iter().zip(&functions).collect();
    let segments = encode_module_candidate_plan_segments(
        &module,
        &plan,
        &inputs,
        &mut vec![None; plan.functions.len()],
        &limits,
        &mut FunctionEncodingStats::default(),
        &mut FunctionPlanEncodingStats::default(),
    )
    .unwrap();
    let bytes: Vec<_> = segments
        .iter()
        .flat_map(|segment| segment.bytes.iter().copied())
        .collect();
    let mut shifted = segments.clone();
    let mut header = shifted[0].bytes.to_vec();
    header.push(shifted[1].bytes[0]);
    shifted[0] = WireSegment {
        fingerprint: Fingerprint::of(&header),
        bytes: header.into(),
    };
    let fragment = shifted[1].bytes[1..].to_vec();
    shifted[1] = WireSegment {
        fingerprint: Fingerprint::of(&fragment),
        bytes: fragment.into(),
    };
    assert_eq!(
        shifted
            .iter()
            .flat_map(|segment| segment.bytes.iter().copied())
            .collect::<Vec<_>>(),
        bytes
    );
    let mut changed = segments.clone();
    let mut fragment = changed[1].bytes.to_vec();
    *fragment.last_mut().unwrap() ^= 1;
    // Even retaining the original fingerprint cannot bind changed bytes.
    changed[1].bytes = fragment.into();
    for supplied in [&shifted[..], &changed[..], &segments[..1]] {
        let decoded = decode_module_candidate_with_encodings(&bytes, supplied, &limits).unwrap();
        assert_eq!(decoded.module, module);
        assert_eq!(decoded.plan, plan);
        assert!(decoded.encodings.is_none());
    }
    let mut unknown = bytes.clone();
    unknown[8..12].copy_from_slice(&(WIRE_VERSION + 1).to_le_bytes());
    let mut trailing = bytes.clone();
    trailing.push(0);
    for corrupt in [&unknown[..], &bytes[..bytes.len() - 1], &trailing[..]] {
        let actual = decode_module_candidate_with_encodings(corrupt, &segments, &limits);
        assert!(actual.is_err());
        assert_eq!(
            actual.map(|decoded| (decoded.module, decoded.plan)),
            decode_module_candidate(corrupt, &limits)
        );
    }
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

#[test]
fn shared_plan_columns_isolate_mutations_and_row_ids_still_bind_encoding_slots() {
    let (module, plan) = plans_fixture();
    let limits = Limits::default();
    let functions = slots(&module, &limits);
    let mut slots = vec![None, None, None];
    let before = cached_plans(&module, &plan, &limits, &functions, &mut slots)
        .0
        .unwrap();
    let retained = slots.clone();
    let old_row_bytes = retained[1]
        .as_ref()
        .unwrap()
        .encoding
        .shared_bytes()
        .to_vec();
    for (row, encoding) in plan.functions.iter().zip(&retained) {
        let bound = &encoding.as_ref().unwrap().function;
        assert!(row.blocks.ptr_eq(&bound.blocks));
        assert!(row.holds.ptr_eq(&bound.holds));
    }
    let mut changed = plan.clone();
    changed.functions[1].blocks[0]
        .initialized_in
        .push(LocalId(99));
    assert!(
        !changed.functions[1]
            .blocks
            .ptr_eq(&plan.functions[1].blocks)
    );
    assert!(changed.functions[1].holds.ptr_eq(&plan.functions[1].holds));
    let (actual, stats) = cached_plans(&module, &changed, &limits, &functions, &mut slots);
    assert_eq!(actual, encode_module_candidate(&module, &changed, &limits));
    assert_eq!(
        (stats.serialized_function_plans, stats.reused_function_plans),
        (1, 2)
    );
    assert_eq!(
        before,
        encode_module_candidate(&module, &plan, &limits).unwrap()
    );
    assert_eq!(
        retained[1]
            .as_ref()
            .unwrap()
            .encoding
            .shared_bytes()
            .as_ref(),
        old_row_bytes.as_slice()
    );

    // Equal array allocations alone do not bind the function owner.
    let mut renamed = changed.clone();
    renamed.functions[1].function = FunctionId(77);
    assert!(
        renamed.functions[1]
            .blocks
            .ptr_eq(&changed.functions[1].blocks)
    );
    assert!(
        renamed.functions[1]
            .holds
            .ptr_eq(&changed.functions[1].holds)
    );
    let (actual, stats) = cached_plans(&module, &renamed, &limits, &functions, &mut slots);
    assert_eq!(actual, encode_module_candidate(&module, &renamed, &limits));
    assert_eq!(
        (stats.serialized_function_plans, stats.reused_function_plans),
        (1, 2)
    );
    assert_ne!(
        retained[1].as_ref().unwrap().function.function,
        FunctionId(77)
    );

    // Owned iteration and Vec conversion must isolate nested children too.
    let mut blocks: Vec<BlockPlan> = plan.functions[0].blocks.clone().into();
    blocks[0].initialized_out.push(LocalId(88));
    assert!(
        !plan.functions[0].blocks[0]
            .initialized_out
            .contains(&LocalId(88))
    );
    let mut holds = plan.functions[0]
        .holds
        .clone()
        .into_iter()
        .collect::<Vec<_>>();
    holds[0].roots.push(HoldRoot::Local(LocalId(88)));
    assert!(
        !plan.functions[0].holds[0]
            .roots
            .contains(&HoldRoot::Local(LocalId(88)))
    );
    assert_eq!(
        before,
        encode_module_candidate(&module, &plan, &limits).unwrap()
    );
}

#[test]
fn persistent_plan_slots_bind_exact_owners_without_rechecking_shared_rows() {
    let (module, mut plan) = plans_fixture();
    let limits = Limits::default();
    let functions = slots(&module, &limits);
    let mut plans: PersistentVec<_> = vec![None, None, None].into();
    let encode = |plan: &CandidatePlan,
                  plans: &mut PersistentVec<Option<Arc<RawFunctionPlanEncoding>>>,
                  limits: &Limits| {
        let mut stats = FunctionPlanEncodingStats::default();
        let bytes = encode_module_candidate_persistent_segments(
            &module,
            plan,
            || module.functions.iter().zip(&functions),
            plans,
            limits,
            &mut FunctionEncodingStats::default(),
            &mut stats,
        )
        .map(|segments| {
            segments
                .iter()
                .flat_map(|s| s.bytes.iter().copied())
                .collect::<Vec<_>>()
        });
        (bytes, stats)
    };
    assert_eq!(
        encode(&plan, &mut plans, &limits).0,
        encode_module_candidate(&module, &plan, &limits)
    );
    let cold = plans.clone();
    let (_, warm) = encode(&plan, &mut plans, &limits);
    assert_eq!(
        (warm.serialized_function_plans, warm.compared_function_plans),
        (0, 0)
    );
    assert!(plans.ptr_eq(&cold));
    let row = plan.functions[1].clone();
    plan.functions.replace(1, row);
    let (bytes, equal) = encode(&plan, &mut plans, &limits);
    assert_eq!(bytes, encode_module_candidate(&module, &plan, &limits));
    assert_eq!(
        (
            equal.serialized_function_plans,
            equal.compared_function_plans
        ),
        (0, 1)
    );
    assert!(Arc::ptr_eq(
        &cold[1].as_ref().unwrap().shared_bytes(),
        &plans[1].as_ref().unwrap().shared_bytes()
    ));
    let rebound = plans.clone();
    assert_eq!(
        encode(&plan, &mut plans, &limits).1.compared_function_plans,
        0
    );
    assert!(plans.ptr_eq(&rebound));
    // A valid public fingerprint or same function ID never authorizes stale
    // fact reuse. Complete raw inequality requires a new serialization.
    plan.functions[1].blocks[0].initialized_out.pop();
    let (bytes, changed) = encode(&plan, &mut plans, &limits);
    assert_eq!(bytes, encode_module_candidate(&module, &plan, &limits));
    assert_eq!(
        (
            changed.serialized_function_plans,
            changed.compared_function_plans
        ),
        (1, 1)
    );
    assert!(Arc::ptr_eq(cold.owner(0).unwrap(), plans.owner(0).unwrap()));
    assert!(Arc::ptr_eq(cold.owner(2).unwrap(), plans.owner(2).unwrap()));
}

#[test]
fn persistent_plan_slots_preserve_exact_aggregate_wire_errors() {
    let (module, plan) = plans_fixture();
    let normal = Limits::default();
    let mut writer = Writer::new(&normal);
    writer.header(MODULE_MAGIC).unwrap();
    writer.module(&module).unwrap();
    writer.plan(&plan).unwrap();
    let bytes = writer.output.len();
    let nodes = normal.max_nodes - writer.nodes;
    for limits in [
        Limits {
            max_source_bytes: bytes,
            ..normal.clone()
        },
        Limits {
            max_source_bytes: bytes - 1,
            ..normal.clone()
        },
        Limits {
            max_nodes: nodes,
            ..normal.clone()
        },
        Limits {
            max_nodes: nodes - 1,
            ..normal.clone()
        },
    ] {
        let functions = slots(&module, &normal);
        let mut plans: PersistentVec<_> = vec![None, None, None].into();
        // Warm under a different complete Limits value; it cannot license a
        // shortcut under the exact N/N-1 boundary being tested.
        encode_module_candidate_persistent_segments(
            &module,
            &plan,
            || module.functions.iter().zip(&functions),
            &mut plans,
            &normal,
            &mut FunctionEncodingStats::default(),
            &mut FunctionPlanEncodingStats::default(),
        )
        .unwrap();
        let actual = encode_module_candidate_persistent_segments(
            &module,
            &plan,
            || module.functions.iter().zip(&functions),
            &mut plans,
            &limits,
            &mut FunctionEncodingStats::default(),
            &mut FunctionPlanEncodingStats::default(),
        )
        .map(|segments| {
            segments
                .iter()
                .flat_map(|s| s.bytes.iter().copied())
                .collect::<Vec<_>>()
        });
        assert_eq!(actual, encode_module_candidate(&module, &plan, &limits));
    }
}

#[test]
fn current_shared_plan_columns_rebind_ids_and_preserve_ordered_quotas() {
    let (module, mut plan) = plans_fixture();
    let first = plan.functions[0].clone();
    for index in 1..plan.functions.len() {
        plan.functions[index].blocks = first.blocks.clone();
        plan.functions[index].holds = first.holds.clone();
    }
    let limits = Limits::default();
    let functions = slots(&module, &limits);
    let mut rows = vec![None; plan.functions.len()];
    let (actual, stats) = cached_plans(&module, &plan, &limits, &functions, &mut rows);
    let reference = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(actual.unwrap(), reference);
    assert_eq!(stats.serialized_function_plans, 1);
    assert_eq!(stats.reused_function_plans, plan.functions.len() - 1);
    for (function, row) in plan.functions.iter().zip(&rows) {
        let raw = row.as_ref().unwrap();
        assert_eq!(raw.function.as_ref(), function);
        assert_eq!(&raw.encoding.bytes[..4], &function.function.0.to_le_bytes());
    }
    let retained = rows.clone();
    plan.functions[1].function = FunctionId(912);
    plan.functions[2].holds[0].roots.push(HoldRoot::Capture(97));
    let (actual, stats) = cached_plans(&module, &plan, &limits, &functions, &mut rows);
    assert_eq!(actual, encode_module_candidate(&module, &plan, &limits));
    assert_eq!(
        (stats.serialized_function_plans, stats.reused_function_plans),
        (1, 2)
    );
    assert_ne!(
        retained[1].as_ref().unwrap().function.function,
        FunctionId(912)
    );

    // With insufficient aggregate allowance, run the original per-field writer
    // so a byte failure cannot be reordered with a node/count failure.
    let mut writer = Writer::new(&limits);
    writer.header(MODULE_MAGIC).unwrap();
    writer.module(&module).unwrap();
    writer.plan(&plan).unwrap();
    let used = limits.max_nodes - writer.nodes;
    for nodes in 0..=used + 1 {
        for maximum in [
            1,
            reference.len() / 2,
            reference.len() - 1,
            writer.output.len(),
            writer.output.len() + 1,
        ] {
            let restricted = Limits {
                max_nodes: nodes,
                max_source_bytes: maximum,
                ..limits.clone()
            };
            let functions: Vec<_> = module.functions.iter().map(|_| OnceLock::new()).collect();
            let mut rows = vec![None; plan.functions.len()];
            let (actual, _) = cached_plans(&module, &plan, &restricted, &functions, &mut rows);
            assert_eq!(
                actual,
                encode_module_candidate(&module, &plan, &restricted),
                "nodes={nodes}, bytes={maximum}"
            );
        }
    }
}
