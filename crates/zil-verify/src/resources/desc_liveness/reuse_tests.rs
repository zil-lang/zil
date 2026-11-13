//! Full frozen resource transfer and liveness comparison, never cached facts.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
include!("reuse_reference.rs");
fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn payload() -> Type {
    Type::Record {
        name: "EffectToken".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}
fn desc() -> Type {
    Type::Desc(Box::new(payload()))
}
fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(),
    }
}
fn effect_fixture(edges: &[Vec<(usize, bool)>], writes: &[u8]) -> TypedModule {
    let mut functions = Vec::new();
    for (id, calls) in edges.iter().enumerate() {
        let mut values = vec![desc(), desc(), Type::I32];
        let mut instructions = vec![
            instruction(Some(0), InstructionKind::Load(LocalId(0))),
            instruction(Some(1), InstructionKind::Load(LocalId(1))),
            instruction(Some(2), InstructionKind::Constant(Constant::I32(1))),
        ];
        for p in 0..2 {
            if writes[id] & (1 << p) != 0 {
                instructions.push(instruction(
                    None,
                    InstructionKind::WriteDesc {
                        desc: ValueId(p),
                        index: 0,
                        value: ValueId(2),
                    },
                ));
            }
        }
        for &(callee, swap) in calls {
            instructions.push(instruction(
                Some(values.len() as u32),
                InstructionKind::Call {
                    function: FunctionId(callee as u32),
                    arguments: if swap {
                        vec![ValueId(1), ValueId(0)]
                    } else {
                        vec![ValueId(0), ValueId(1)]
                    },
                },
            ));
            values.push(Type::I32);
        }
        functions.push(TypedFunction {
            dyn_instance: None,
            retained_return_sources: None,
            storage_life: None,
            call_kind: zil_ir::CallKind::Direct,
            resume_bound: zil_ir::TaskResumeBound::Local,
            reference_return_bound: None,
            id: FunctionId(id as u32),
            name: format!("f{id}"),
            foreign: None,
            environment: None,
            callable_contract: None,
            protocol_contracts: Vec::new(),
            callable_return_contract: None,
            parameters: vec![LocalId(0), LocalId(1)],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: (0..2)
                .map(|id| LocalDecl {
                    id: LocalId(id),
                    name: format!("p{id}"),
                    ty: desc(),
                    mutable: false,
                    span: span(),
                })
                .collect(),
            values,
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator: Terminator::Return(Some(ValueId(2))),
            }],
            entry: BlockId(0),
            span: span(),
        });
    }
    TypedModule {
        semantic: None,
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions,
        closures: vec![],
        resources: vec![ResourceDefinition {
            name: "EffectToken".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn graphs(module: &TypedModule) -> Vec<FunctionGraph> {
    module
        .functions
        .iter()
        .map(|f| FunctionGraph {
            role_data_uses: Vec::new(),
            successors: vec![vec![]; f.blocks.len()],
            predecessors: vec![vec![]; f.blocks.len()],
            reachable: vec![true; f.blocks.len()],
            package_views: packages::CaptureViews::default(),
        })
        .collect()
}

fn history_instruction(result: Option<usize>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(|n| ValueId(n as u32)),
        kind,
        span: span(),
    }
}

fn history_fixture(
    count: usize,
    edges: Vec<Vec<usize>>,
    reachable: Vec<bool>,
) -> (TypedModule, FunctionGraph, Slots) {
    let local_count = count.div_ceil(2);
    let value_count = count - local_count;
    let ty = Type::Desc(Box::new(Type::Record {
        name: "HistoryToken".into(),
        fields: vec![("n".into(), Type::I32)],
    }));
    let mut blocks = Vec::new();
    for id in 0..edges.len() {
        let mut instructions = vec![history_instruction(
            None,
            InstructionKind::ScopeEnter { scope: ScopeId(0) },
        )];
        if local_count > 0 {
            let local = LocalId((id % local_count) as u32);
            instructions.push(history_instruction(
                None,
                InstructionKind::StorageLive(local),
            ));
            instructions.push(history_instruction(
                (value_count > 0).then_some(id % value_count.max(1)),
                InstructionKind::Load(local),
            ));
            if value_count > 0 {
                instructions.push(history_instruction(
                    None,
                    InstructionKind::Store {
                        local,
                        value: ValueId((id % value_count) as u32),
                    },
                ));
                instructions.push(history_instruction(
                    None,
                    InstructionKind::ReadDesc {
                        desc: ValueId(((id + value_count - 1) % value_count) as u32),
                        index: 0,
                    },
                ));
            }
            instructions.push(history_instruction(
                None,
                InstructionKind::BorrowLocal { local },
            ));
            instructions.push(history_instruction(
                None,
                InstructionKind::BorrowLocalMut { local },
            ));
            instructions.push(history_instruction(
                None,
                InstructionKind::TakeLocal { local },
            ));
            instructions.push(history_instruction(
                None,
                InstructionKind::StorageDead(local),
            ));
        }
        // An actually empty block and an unreachable block still get histories.
        if id == 4 {
            instructions.clear();
        }
        blocks.push(BasicBlock {
            id: BlockId(id as u32),
            instructions,
            terminator: Terminator::Trap,
        });
    }
    let function = TypedFunction {
        dyn_instance: None,
        retained_return_sources: None,
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        resume_bound: zil_ir::TaskResumeBound::Local,
        reference_return_bound: None,
        id: FunctionId(0),
        name: "history".into(),
        foreign: None,
        environment: None,
        callable_contract: None,
        protocol_contracts: Vec::new(),
        callable_return_contract: None,
        parameters: (0..local_count).map(|n| LocalId(n as u32)).collect(),
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: (0..local_count)
            .map(|n| LocalDecl {
                id: LocalId(n as u32),
                name: format!("d{n}"),
                ty: ty.clone(),
                mutable: true,
                span: span(),
            })
            .collect(),
        values: vec![ty; value_count],
        blocks,
        entry: BlockId(0),
        span: span(),
    };
    let module = TypedModule {
        semantic: None,
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![function],
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
    };
    let mut predecessors = vec![vec![]; edges.len()];
    for (from, row) in edges.iter().enumerate() {
        for &to in row {
            predecessors[to].push(from);
        }
    }
    let graph = FunctionGraph {
        successors: edges,
        predecessors,
        reachable,
        package_views: packages::CaptureViews::default(),
        role_data_uses: vec![],
    };
    let slots = Slots::new(
        &module,
        &module.functions[0],
        &graph.package_views,
        &mut budget(1_000_000),
    )
    .unwrap();
    assert_eq!(slots.desc_count, count);
    (module, graph, slots)
}

fn logical(history: &History, f: &TypedFunction) -> Vec<Vec<Vec<bool>>> {
    f.blocks
        .iter()
        .enumerate()
        .map(|(b, block)| {
            block
                .instructions
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let row = history.row(b, i);
                    (0..row.len())
                        .map(|n| row.get(n, &mut budget(1_000_000)).unwrap())
                        .collect()
                })
                .collect()
        })
        .collect()
}
fn compare_after(binding: &mut Binding<'_>, slots: &Slots) {
    let f = binding.function();
    let graph = binding.graph();
    let expected = old_liveness(f, graph, slots, &mut budget(10_000_000)).unwrap();
    let actual = binding.after(slots, &mut budget(10_000_000)).unwrap();
    assert_eq!(logical(actual.history(), f), logical(&expected, f));
}
#[test]
fn bound_liveness_all_after_rows_match_frozen_cfg_solver() {
    for count in [0, 1, 7, 8, 9, WORD - 1, WORD, WORD + 1, 72, WORD * 2 + 1] {
        let (module, graph, slots) = history_fixture(
            count,
            vec![vec![1, 3], vec![2], vec![1, 3], vec![4], vec![], vec![5]],
            vec![true, true, true, true, true, false],
        );
        let f = &module.functions[0];
        let mut binding = Binding::new(f, &graph);
        compare_after(&mut binding, &slots);
        compare_after(&mut binding, &slots);
        assert_eq!(binding.entry.is_some(), count > SMALL);
    }
}
#[test]
fn bound_liveness_exact_coordinates_and_fixed_actual_objects_do_not_alias() {
    let (module, graph, mut slots) = history_fixture(72, vec![vec![1], vec![]], vec![true, true]);
    let f = &module.functions[0];
    let mut binding = Binding::new(f, &graph);
    compare_after(&mut binding, &slots);
    slots.desc_local.swap(0, 1);
    compare_after(&mut binding, &slots);
    slots.desc_value.swap(0, 1);
    compare_after(&mut binding, &slots);
    slots.desc_local[0] = None;
    compare_after(&mut binding, &slots);
    slots.desc_value[0] = None;
    compare_after(&mut binding, &slots);
    slots.desc_count += 1;
    compare_after(&mut binding, &slots);
    // A same-ID module and another selected graph create separate bindings;
    // the private API cannot replace the actual objects inside a live binding.
    let copied = module.clone();
    let mut other = Binding::new(&copied.functions[0], &graph);
    assert!(other.entry.is_none());
    compare_after(&mut other, &slots);
    let alternate = FunctionGraph {
        successors: vec![vec![], vec![]],
        predecessors: vec![vec![], vec![]],
        reachable: vec![true, false],
        package_views: packages::CaptureViews::default(),
        role_data_uses: vec![],
    };
    let mut refined = Binding::new(f, &alternate);
    assert!(refined.entry.is_none());
    compare_after(&mut refined, &slots);
    // Coordinate lengths as well as None/ordinal positions are part of the key.
    slots.desc_local.push(None);
    slots.desc_value.push(None);
    compare_after(&mut binding, &slots);
}
fn full_fixture() -> TypedModule {
    let mut module = effect_fixture(&[vec![(1, false)], vec![]], &[1, 2]);
    let f = &mut module.functions[0];
    f.locals.push(LocalDecl {
        id: LocalId(2),
        name: "owned".into(),
        ty: Type::Managed(Box::new(payload())),
        mutable: true,
        span: span(),
    });
    f.parameters.push(LocalId(2));
    for id in 3..71 {
        f.locals.push(LocalDecl {
            id: LocalId(id),
            name: format!("unused{id}"),
            ty: desc(),
            mutable: true,
            span: span(),
        });
    }
    f.blocks[0].instructions.push(instruction(
        None,
        InstructionKind::DropManaged { local: LocalId(2) },
    ));
    module
}
fn captured_run(
    module: &TypedModule,
    binding: &mut Binding<'_>,
    effects: &[BTreeSet<Root>],
    validate: bool,
    old: bool,
) -> (Check<BTreeSet<Root>>, Option<Vec<Option<State>>>) {
    TEST_RESOURCE_INPUTS.with(|v| *v.borrow_mut() = Some(Vec::new()));
    let result = if old {
        old_run(
            module,
            binding.function(),
            binding.graph(),
            effects,
            validate,
            None,
            &mut budget(10_000_000),
            None,
        )
    } else {
        run_function_data(
            module,
            binding,
            effects,
            validate,
            None,
            &mut budget(10_000_000),
            None,
        )
    };
    let inputs = TEST_RESOURCE_INPUTS.with(|v| v.borrow_mut().take());
    (result, inputs)
}
#[test]
fn bound_liveness_full_resource_states_writes_and_growing_effects_match_old_run() {
    let module = full_fixture();
    let gs = graphs(&module);
    let mut binding = Binding::new(&module.functions[0], &gs[0]);
    for effect in [BTreeSet::new(), BTreeSet::from([Root::Parameter(1)])] {
        let effects = vec![BTreeSet::new(), effect];
        for validate in [false, true] {
            let old = captured_run(&module, &mut binding, &effects, validate, true);
            let new = captured_run(&module, &mut binding, &effects, validate, false);
            assert_eq!(old, new);
            assert!(!new.0.unwrap().is_empty());
            let inputs = new.1.unwrap();
            let state = inputs[0].as_ref().unwrap();
            assert!(state.debt.contains(&LIVE));
            assert!(state.fresh.contains(&INPUT));
            assert!(state.desc.iter().any(|row| !row.is_empty()));
        }
    }
    assert!(binding.entry.is_some());
}
#[test]
fn bound_liveness_warm_validation_keeps_initialization_unpaid_and_old_desc_rejections() {
    let mut missing = full_fixture();
    missing.functions[0].parameters.remove(0);
    let mut unpaid = full_fixture();
    unpaid.functions[0].blocks[0].instructions.pop();
    let mut retired = full_fixture();
    let f = &mut retired.functions[0];
    let value = f.values.len() as u32;
    f.values.extend([desc(), Type::I32]);
    let at = f.blocks[0].instructions.len() - 1;
    f.blocks[0].instructions.insert(
        at,
        instruction(
            Some(value),
            InstructionKind::DescribeLocal { local: LocalId(2) },
        ),
    );
    f.blocks[0].instructions.push(instruction(
        Some(value + 1),
        InstructionKind::ReadDesc {
            desc: ValueId(value),
            index: 0,
        },
    ));
    for (module, text) in [
        (missing, "grounded"),
        (unpaid, "outstanding"),
        (retired, "description"),
    ] {
        let gs = graphs(&module);
        let mut binding = Binding::new(&module.functions[0], &gs[0]);
        let effects = vec![BTreeSet::new(); module.functions.len()];
        // May may collect origins, but never licenses the subsequent full check.
        let a = captured_run(&module, &mut binding, &effects, false, true);
        let b = captured_run(&module, &mut binding, &effects, false, false);
        assert_eq!(a, b);
        let a = captured_run(&module, &mut binding, &effects, true, true);
        let b = captured_run(&module, &mut binding, &effects, true, false);
        assert_eq!(a, b);
        let error = b.0.unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::SafeReject);
        assert!(error.message.contains(text), "{error:?}");
        assert!(binding.entry.is_some());
    }
}
fn assert_published(binding: &Binding<'_>, slots: &Slots, expected: &[Vec<Vec<bool>>]) {
    let entry = binding.entry.as_ref().unwrap();
    assert_eq!(entry.count, slots.desc_count);
    assert_eq!(entry.local, slots.desc_local);
    assert_eq!(entry.value, slots.desc_value);
    assert_eq!(logical(&entry.history, binding.function()), expected);
}
#[test]
fn bound_liveness_cold_publication_warm_guard_and_tiny_costs_have_fresh_boundaries() {
    const ROOM: usize = 1_000_000;
    let (module, graph, slots) = history_fixture(72, vec![vec![1], vec![]], vec![true, true]);
    let f = &module.functions[0];
    // This is the original complete old solve, moved before publication so the
    // expected table never comes from the candidate cache or an alias of it.
    let mut b = budget(ROOM);
    let expected = old_liveness(f, &graph, &slots, &mut b).unwrap();
    let old = ROOM - b.remaining;
    let expected = logical(&expected, f);
    let mut binding = Binding::new(f, &graph);
    let mut b = budget(ROOM);
    let actual = binding.after(&slots, &mut b).unwrap();
    let cold = ROOM - b.remaining;
    assert_eq!(logical(actual.history(), f), expected);
    assert_published(&binding, &slots, &expected);
    let mut fresh = Binding::new(f, &graph);
    for n in [0, 1, 2, cold - 1] {
        let error = fresh.after(&slots, &mut budget(n)).err().unwrap();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert!(fresh.entry.is_none());
    }
    // The last attempt failed at N-1. A fresh Budget on that same unpublished
    // binding must now return the entire independently reconstructed table.
    let actual = fresh.after(&slots, &mut budget(cold)).unwrap();
    assert_eq!(logical(actual.history(), f), expected);
    assert_published(&fresh, &slots, &expected);
    let mut b = budget(ROOM);
    let actual = fresh.after(&slots, &mut b).unwrap();
    let warm = ROOM - b.remaining;
    assert_eq!(logical(actual.history(), f), expected);
    for n in 0..warm {
        let before = logical(&fresh.entry.as_ref().unwrap().history, f);
        let error = fresh.after(&slots, &mut budget(n)).err().unwrap();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(logical(&fresh.entry.as_ref().unwrap().history, f), before);
        assert_published(&fresh, &slots, &expected);
    }
    for _ in 0..2 {
        let actual = fresh.after(&slots, &mut budget(warm)).unwrap();
        assert!(matches!(&actual, After::Borrowed(_)));
        assert_eq!(logical(actual.history(), f), expected);
        assert_published(&fresh, &slots, &expected);
    }
    assert!(
        warm < old,
        "actual guard work must be below the complete old build"
    );
    eprintln!(
        "bound liveness cold N{cold}, warm N{warm}, old complete build N{old}; N-1/fresh guarded"
    );
    for count in [0, 1, 7, 8] {
        let (module, graph, slots) = history_fixture(count, vec![vec![]], vec![true]);
        let f = &module.functions[0];
        let mut binding = Binding::new(f, &graph);
        let mut b = budget(ROOM);
        let expected = old_liveness(f, &graph, &slots, &mut b).unwrap();
        let old = ROOM - b.remaining;
        let expected = logical(&expected, f);
        for _ in 0..2 {
            let mut b = budget(ROOM);
            let actual = binding.after(&slots, &mut b).unwrap();
            assert!(matches!(&actual, After::Owned(_)));
            assert_eq!(logical(actual.history(), f), expected);
            let wrapper =
                std::mem::size_of::<After<'_>>().div_ceil(std::mem::size_of::<usize>()) * 2 + 3;
            assert_eq!(ROOM - b.remaining, old + wrapper);
            assert!(binding.entry.is_none());
        }
    }
}
