//! Request-private static layouts only. Dynamic State, effects, Sum bridges and
//! real validation still execute; raw layout inventories are component fixtures.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::{CallableKind, ClosureDefinition, ClosureId};
include!("reference.rs");
const ROOM: usize = 10_000_000;
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
            protocol_contract: None,
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

// Compare all sixteen slot fields and both complete immutable plans,
// including every coordinate and interval rather than only table lengths.
#[derive(Debug, PartialEq, Eq)]
struct SlotsSnapshot {
    managed_local: Vec<Option<usize>>,
    managed_value: Vec<Option<usize>>,
    package_fields: Vec<Vec<Option<usize>>>,
    package_fresh_plan: field_fresh::LayoutSnapshot,
    product: product::LayoutSnapshot,
    desc_local: Vec<Option<usize>>,
    desc_value: Vec<Option<usize>>,
    capture_debt: Vec<Option<usize>>,
    package_view: Vec<Option<u32>>,
    package_capture_local: Vec<Option<LocalId>>,
    retained_debt: Vec<bool>,
    capture_start: usize,
    local_count: usize,
    debt_count: usize,
    desc_count: usize,
    retirement: bool,
}

fn snapshot_slots(slots: &Slots) -> SlotsSnapshot {
    SlotsSnapshot {
        managed_local: slots.managed_local.clone(),
        managed_value: slots.managed_value.clone(),
        package_fields: slots.package_fields.clone(),
        package_fresh_plan: slots.package_fresh_plan.layout_snapshot(),
        product: slots.product.layout_snapshot(),
        desc_local: slots.desc_local.clone(),
        desc_value: slots.desc_value.clone(),
        capture_debt: slots.capture_debt.clone(),
        package_view: slots.package_view.clone(),
        package_capture_local: slots.package_capture_local.clone(),
        retained_debt: slots.retained_debt.clone(),
        capture_start: slots.capture_start,
        local_count: slots.local_count,
        debt_count: slots.debt_count,
        desc_count: slots.desc_count,
        retirement: slots.retirement,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Step {
    result: Check<BTreeSet<Root>>,
    inputs: Option<Vec<Option<State>>>,
}
fn captured(run: impl FnOnce() -> Check<BTreeSet<Root>>) -> Step {
    TEST_RESOURCE_INPUTS.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
    let result = run();
    let inputs = TEST_RESOURCE_INPUTS.with(|slot| slot.borrow_mut().take());
    Step { result, inputs }
}
fn complete_ok(old: Step, new: Step) {
    assert_eq!(old, new);
    old.result.unwrap();
    new.result.unwrap();
}
fn configure(module: &TypedModule, b: &mut Budget) {
    b.function_count = module.functions.len();
}
fn empty_effects(module: &TypedModule) -> Vec<BTreeSet<Root>> {
    vec![BTreeSet::new(); module.functions.len()]
}
fn raw_shapes(module: &TypedModule, b: &mut Budget) {
    configure(module, b);
    let limits = b.limits.clone();
    for f in &module.functions {
        b.current_function = Some(f.id.0 as usize);
        for ty in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            check_type_shape(ty, &module.target, &limits, b).unwrap();
        }
    }
    b.current_function = None;
    for ty in module.closures.iter().flat_map(|closure| &closure.captures) {
        check_type_shape(ty, &module.target, &limits, b).unwrap();
    }
}

// Raw classifier fixture only: unused SSA declarations exercise physical
// inventory and do not constitute a whole-module/Source acceptance witness.
fn layout_fixture() -> (TypedModule, Vec<FunctionGraph>) {
    let owner = Type::Managed(Box::new(payload()));
    let package = Type::Package {
        name: "SlotsPacket".into(),
        fields: vec![("owner".into(), owner.clone()), ("copy".into(), Type::I32)],
    };
    let nested = Type::Package {
        name: "SlotsNested".into(),
        fields: vec![
            ("child".into(), package.clone()),
            ("copy".into(), Type::Bool),
        ],
    };
    let sum = Type::Enum {
        name: "SlotsStoreChoice".into(),
        variants: vec![
            ("Empty".into(), vec![]),
            (
                "Owned".into(),
                vec![Type::Managed(Box::new(Type::OwnedStore {
                    element: Box::new(Type::I32),
                }))],
            ),
        ],
    };
    let types = vec![
        package.clone(),
        package.clone(),
        owner.clone(),
        desc(),
        nested,
        Type::Array {
            element: Box::new(package.clone()),
            length: 2,
        },
        Type::Closure(ClosureId(1)),
        Type::Address {
            mutable: false,
            inner: Box::new(Type::Closure(ClosureId(1))),
        },
        sum.clone(),
        Type::Address {
            mutable: true,
            inner: Box::new(sum.clone()),
        },
        Type::Tuple(vec![owner.clone(), Type::Bool]),
    ];
    let mut module = effect_fixture(&[vec![], vec![]], &[0, 0]);
    let f = &mut module.functions[0];
    f.environment = Some(ClosureId(0));
    f.parameters.clear();
    f.return_type = Type::Unit;
    f.locals = types
        .iter()
        .enumerate()
        .map(|(i, ty)| LocalDecl {
            id: LocalId(i as u32),
            name: format!("slot{i}"),
            ty: ty.clone(),
            mutable: true,
            span: span(),
        })
        .collect();
    f.values = types;
    f.blocks[0].instructions.clear();
    f.blocks[0].terminator = Terminator::Trap;
    module.closures = vec![
        ClosureDefinition {
            storage_life: None,
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![package, owner, sum, desc()],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::FnMut,
            lending_captures: vec![],
        },
        ClosureDefinition {
            storage_life: None,
            id: ClosureId(1),
            function: FunctionId(1),
            captures: vec![desc()],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        },
    ];
    let mut gs = graphs(&module);
    gs[0].package_views = packages::CaptureViews {
        by_local: (0..module.functions[0].locals.len())
            .map(|i| (i == 0).then_some(0))
            .collect(),
        by_capture: vec![Some(LocalId(0)), None, None, None],
    };
    (module, gs)
}

#[test]
fn slot_layout_complete_fields_plans_and_real_root_inventory_match_old() {
    let (module, gs) = layout_fixture();
    let effects = empty_effects(&module);
    for mode in 0..8 {
        let mut a = budget(ROOM);
        let mut b = budget(ROOM);
        for budget in [&mut a, &mut b] {
            raw_shapes(&module, budget);
            assert!(!budget.inline_store_roots.is_empty());
            // Each different context is installed BEFORE its separate Request.
            budget.sums = mode & 1 != 0;
            budget.sum_functions.fill(mode & 2 != 0);
            if mode & 4 != 0 {
                budget.inline_store_roots.clear();
                budget.inline_store_addresses = false;
                budget.pod_stores = false;
                budget.head_descriptions = false;
            }
        }
        let expected =
            Slots::old_layout_new(&module, &module.functions[0], &gs[0].package_views, &mut a)
                .unwrap();
        let expected = snapshot_slots(&expected);
        assert!(!expected.package_fields.is_empty());
        assert!(!expected.retained_debt.is_empty());
        let mut old_binding = desc_liveness::Binding::new(&module.functions[0], &gs[0]);
        let mut request = Request::new(&module, &gs, None, None, &mut b).unwrap();
        assert!(request.slots(0).is_none());
        for _ in 0..2 {
            let old = captured(|| {
                old_run(
                    &module,
                    &mut old_binding,
                    &effects,
                    false,
                    None,
                    &mut a,
                    None,
                )
            });
            let new = captured(|| request.run(0, &effects, false, None));
            complete_ok(old, new);
            assert_eq!(snapshot_slots(request.slots(0).unwrap()), expected);
        }
    }
}

#[test]
fn slot_layout_actual_module_function_graph_and_capture_views_stay_bound() {
    let (module, gs) = layout_fixture();
    let copied = module.clone();
    let alternate = FunctionGraph {
        successors: gs[0].successors.clone(),
        predecessors: gs[0].predecessors.clone(),
        reachable: gs[0].reachable.clone(),
        role_data_uses: gs[0].role_data_uses.clone(),
        package_views: packages::CaptureViews {
            by_local: (0..module.functions[0].locals.len())
                .map(|i| (i == 1).then_some(0))
                .collect(),
            by_capture: vec![Some(LocalId(1)), None, None, None],
        },
    };
    let refined = BTreeMap::from([(0usize, alternate)]);
    for (module, selected) in [(&module, None), (&copied, None), (&module, Some(&refined))] {
        let mut b = budget(ROOM);
        raw_shapes(module, &mut b);
        let graph = selected.and_then(|m| m.get(&0)).unwrap_or(&gs[0]);
        let expected =
            Slots::old_layout_new(module, &module.functions[0], &graph.package_views, &mut b)
                .unwrap();
        let expected = snapshot_slots(&expected);
        let mut request = Request::new(module, &gs, selected, None, &mut b).unwrap();
        assert!(request.slots(0).is_none());
        assert!(std::ptr::eq(
            request.binding(0).function(),
            &module.functions[0]
        ));
        assert!(std::ptr::eq(request.binding(0).graph(), graph));
        request.run(0, &empty_effects(module), false, None).unwrap();
        assert_eq!(snapshot_slots(request.slots(0).unwrap()), expected);
        assert_eq!(
            request.slots(0).unwrap().package_capture_local[0],
            graph.package_views.by_capture[0]
        );
    }
}

fn endpoint_fixture(bad_task: bool) -> TypedModule {
    let mut module = effect_fixture(&[vec![]], &[0]);
    let f = &mut module.functions[0];
    f.parameters.clear();
    f.values.clear();
    f.return_type = Type::Unit;
    f.blocks[0].instructions.clear();
    f.blocks[0].terminator = Terminator::Trap;
    let mut types = vec![
        Type::Managed(Box::new(payload())),
        Type::Tuple(vec![Type::Managed(Box::new(payload()))]),
    ];
    if bad_task {
        types.push(Type::Managed(Box::new(Type::Task {
            result: Box::new(Type::Address {
                mutable: false,
                inner: Box::new(Type::ForeignHandle),
            }),
            retained_life: zil_ir::RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        })));
    }
    f.locals = types
        .into_iter()
        .enumerate()
        .map(|(i, ty)| LocalDecl {
            id: LocalId(i as u32),
            name: format!("endpoint{i}"),
            ty,
            mutable: true,
            span: span(),
        })
        .collect();
    module
}

#[test]
fn slot_layout_actual_endpoint_mark_sum_rebuilds_and_keeps_task_context_rejection() {
    for bad_task in [false, true] {
        let module = endpoint_fixture(bad_task);
        let gs = graphs(&module);
        let mut b = budget(ROOM);
        configure(&module, &mut b);
        let mut request = Request::new(&module, &gs, None, None, &mut b).unwrap();
        let effects = empty_effects(&module);
        request.run(0, &effects, false, None).unwrap();
        let before = snapshot_slots(request.slots(0).unwrap());
        assert!(before.managed_local[1].is_none());
        // Call the real qualifier, rather than setting a cache flag or a fake
        // true bit. It reaches mark_sum only after the complete endpoint proof.
        let target = crate::place_replacements::actual::Target {
            base: crate::place_replacements::actual::Base::Local(LocalId(1)),
            steps: vec![],
            mutable: true,
        };
        sums::replacements::at_type(&module, &module.functions[0], &target, request.budget)
            .unwrap();
        assert!(request.budget.sums && request.budget.sum_function(FunctionId(0)));
        let mut a = budget(ROOM);
        configure(&module, &mut a);
        sums::replacements::at_type(&module, &module.functions[0], &target, &mut a).unwrap();
        let mut binding = desc_liveness::Binding::new(&module.functions[0], &gs[0]);
        let old = captured(|| old_run(&module, &mut binding, &effects, false, None, &mut a, None));
        let new = captured(|| request.run(0, &effects, false, None));
        assert_eq!(old, new);
        if bad_task {
            let error = new.result.unwrap_err();
            assert_eq!(error.category, zil_base::ResultCategory::SafeReject);
            assert_eq!(error.code, "E-VERIFY-TASK-RESULT");
            assert!(error.message.contains("shared Task result"));
            assert_eq!(snapshot_slots(request.slots(0).unwrap()), before);
        } else {
            old.result.unwrap();
            new.result.unwrap();
            let expected =
                Slots::old_layout_new(&module, &module.functions[0], &gs[0].package_views, &mut a)
                    .unwrap();
            assert_eq!(
                snapshot_slots(request.slots(0).unwrap()),
                snapshot_slots(&expected)
            );
            assert!(request.slots(0).unwrap().managed_local[1].is_some());
        }
    }
}

#[test]
fn slot_layout_warm_calls_recompute_complete_states_effects_bridges_and_task_actions() {
    let mut module = full_fixture();
    let f = &mut module.functions[0];
    let drop = f.blocks[0].instructions.pop().unwrap();
    f.blocks[0].terminator = Terminator::Jump(BlockId(1));
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![instruction(None, InstructionKind::TaskYield), drop],
        terminator: Terminator::Return(Some(ValueId(2))),
    });
    let mut gs = graphs(&module);
    gs[0].successors = vec![vec![1], vec![]];
    gs[0].predecessors = vec![vec![], vec![0]];
    let f = &module.functions[0];
    let mut bridge: Vec<Vec<sums::Bridge>> = f
        .blocks
        .iter()
        .map(|block| vec![sums::Bridge::default(); block.instructions.len()])
        .collect();
    bridge[0][0].unpack.push((LocalId(2), FRESH, vec![]));
    let mut a = budget(ROOM);
    let mut b = budget(ROOM);
    for budget in [&mut a, &mut b] {
        configure(&module, budget);
        budget.task_workers.insert(FunctionId(0));
    }
    let mut old_binding = desc_liveness::Binding::new(f, &gs[0]);
    let mut request = Request::new(&module, &gs, None, None, &mut b).unwrap();
    let mut prior_without = None;
    for effect in [BTreeSet::new(), BTreeSet::from([Root::Parameter(1)])] {
        let effects = vec![BTreeSet::new(), effect];
        for rows in [None, Some(bridge.as_slice())] {
            for validate in [false, true] {
                let old = captured(|| {
                    old_run(
                        &module,
                        &mut old_binding,
                        &effects,
                        validate,
                        rows,
                        &mut a,
                        None,
                    )
                });
                let new = captured(|| request.run(0, &effects, validate, rows));
                assert_eq!(old, new);
                old.result.unwrap();
                new.result.as_ref().unwrap();
                let inputs = new.inputs.as_ref().unwrap();
                assert_eq!(inputs.len(), 2);
                assert!(inputs.iter().all(Option::is_some));
                if rows.is_none() {
                    prior_without = Some(inputs.clone());
                } else {
                    assert_ne!(inputs, prior_without.as_ref().unwrap());
                    let owner = request.slots(0).unwrap().managed_local[2].unwrap();
                    assert_eq!(inputs[1].as_ref().unwrap().fresh[owner], FRESH);
                }
                assert_eq!(
                    format!("{:?}", a.task_ownership),
                    format!("{:?}", request.budget.task_ownership)
                );
            }
        }
    }
    assert!(
        !request.budget.task_ownership[&FunctionId(0)]
            .boundaries()
            .is_empty()
    );
}

#[test]
fn slot_layout_cached_layout_preserves_real_grounding_cleanup_and_live_description_errors() {
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
        let effects = empty_effects(&module);
        let mut a = budget(ROOM);
        let mut b = budget(ROOM);
        configure(&module, &mut a);
        configure(&module, &mut b);
        let mut old_binding = desc_liveness::Binding::new(&module.functions[0], &gs[0]);
        let mut request = Request::new(&module, &gs, None, None, &mut b).unwrap();
        complete_ok(
            captured(|| {
                old_run(
                    &module,
                    &mut old_binding,
                    &effects,
                    false,
                    None,
                    &mut a,
                    None,
                )
            }),
            captured(|| request.run(0, &effects, false, None)),
        );
        let before = snapshot_slots(request.slots(0).unwrap());
        let old = captured(|| {
            old_run(
                &module,
                &mut old_binding,
                &effects,
                true,
                None,
                &mut a,
                None,
            )
        });
        let new = captured(|| request.run(0, &effects, true, None));
        assert_eq!(old, new);
        let error = new.result.unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::SafeReject);
        assert!(error.message.contains(text), "{error:?}");
        assert_eq!(snapshot_slots(request.slots(0).unwrap()), before);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FullTrace {
    written: Vec<BTreeSet<Root>>,
    states: Vec<Option<Vec<Option<State>>>>,
    task_ownership: String,
}
fn exact_sequence(
    module: &TypedModule,
    gs: &[FunctionGraph],
    old: bool,
    n: usize,
    count: usize,
) -> Check<(FullTrace, usize)> {
    let mut b = budget(n);
    configure(module, &mut b);
    let effects = empty_effects(module);
    let mut trace = FullTrace {
        written: vec![],
        states: vec![],
        task_ownership: String::new(),
    };
    if old {
        let mut bindings = old_bindings(module, gs, None, &mut b)?;
        for _ in 0..count {
            let step =
                captured(|| old_run(module, &mut bindings[0], &effects, true, None, &mut b, None));
            trace.written.push(step.result?);
            trace.states.push(step.inputs);
        }
    } else {
        let mut request = Request::new(module, gs, None, None, &mut b)?;
        for _ in 0..count {
            let step = captured(|| request.run(0, &effects, true, None));
            trace.written.push(step.result?);
            trace.states.push(step.inputs);
        }
    }
    trace.task_ownership = format!("{:?}", b.task_ownership);
    Ok((trace, b.remaining))
}

#[test]
fn slot_layout_cold_publication_warm_failure_and_complete_requests_have_exact_fresh_boundaries() {
    // Small real resource transfer keeps exhaustive warm-prefix probing bounded.
    let mut module = full_fixture();
    module.functions[0].locals.truncate(3);
    let gs = graphs(&module);
    let effects = empty_effects(&module);
    let (expected, _) = exact_sequence(&module, &gs, true, ROOM, 2).unwrap();
    let (actual, remaining) = exact_sequence(&module, &gs, false, ROOM, 2).unwrap();
    assert_eq!(actual, expected);
    let n = ROOM - remaining;
    let (_, left) = exact_sequence(&module, &gs, false, ROOM, 1).unwrap();
    let first = ROOM - left;
    let mut a = budget(ROOM);
    let slots =
        Slots::old_layout_new(&module, &module.functions[0], &gs[0].package_views, &mut a).unwrap();
    let old_constructor = ROOM - a.remaining;
    let expected_slots = snapshot_slots(&slots);
    let mut setup_budget = budget(ROOM);
    configure(&module, &mut setup_budget);
    let setup = {
        let request = Request::new(&module, &gs, None, None, &mut setup_budget).unwrap();
        assert!(request.slots(0).is_none());
        drop(request);
        ROOM - setup_budget.remaining
    };
    let mut last_unpublished = None;
    let mut publication = None;
    // Probe the complete cold prefix, including the final paid publication.
    // Failures after publication may retain only the complete independent
    // layout; failures before it must leave the entry absent.
    for available in setup..first {
        let mut b = budget(available);
        configure(&module, &mut b);
        let mut request = Request::new(&module, &gs, None, None, &mut b).unwrap();
        assert_eq!(
            request.run(0, &effects, true, None).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        if let Some(slots) = request.slots(0) {
            publication.get_or_insert(available);
            assert_eq!(snapshot_slots(slots), expected_slots);
        } else {
            assert!(publication.is_none());
            last_unpublished = Some(available);
        }
    }
    let publication = publication.expect("a complete layout publishes before transfer finishes");
    assert!(publication > setup + old_constructor);
    assert_eq!(last_unpublished, Some(publication - 1));
    // Repeat the final publication failure, then make a fresh complete call.
    let mut b = budget(publication - 1);
    configure(&module, &mut b);
    let mut request = Request::new(&module, &gs, None, None, &mut b).unwrap();
    assert_eq!(
        request.run(0, &effects, true, None).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert!(request.slots(0).is_none());
    drop(request);
    let (fresh, left) = exact_sequence(&module, &gs, false, first, 1).unwrap();
    let (old_first, _) = exact_sequence(&module, &gs, true, ROOM, 1).unwrap();
    assert_eq!(left, 0);
    assert_eq!(fresh, old_first);
    assert!(n > first);
    for available in first..n {
        let mut b = budget(available);
        configure(&module, &mut b);
        let mut request = Request::new(&module, &gs, None, None, &mut b).unwrap();
        request.run(0, &effects, true, None).unwrap();
        assert_eq!(snapshot_slots(request.slots(0).unwrap()), expected_slots);
        assert_eq!(
            request.run(0, &effects, true, None).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(snapshot_slots(request.slots(0).unwrap()), expected_slots);
    }
    // Both are fresh complete requests after the last N-1 failure; there is no
    // borrowed-cache restart with a replacement Budget inside a live Request.
    for _ in 0..2 {
        let (fresh, left) = exact_sequence(&module, &gs, false, n, 2).unwrap();
        assert_eq!(left, 0);
        assert_eq!(fresh, expected);
    }
    let (_, old_left) = exact_sequence(&module, &gs, true, ROOM, 2).unwrap();
    let old_n = ROOM - old_left;
    assert_eq!(
        exact_sequence(&module, &gs, true, old_n - 1, 2)
            .unwrap_err()
            .category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(
        exact_sequence(&module, &gs, true, old_n, 2).unwrap().0,
        expected
    );
    eprintln!(
        "resource Slots complete two-call request N{n}, old N{old_n}; N-1 and fresh full States"
    );
}

#[test]
fn slot_layout_actual_frontier_and_replacement_context_match_complete_old_run() {
    use crate::place_replacements::fixture;
    let module = fixture::module_with_relay_and_caller(true);
    let gs = graphs(&module);
    let mut a = budget(ROOM);
    let mut b = budget(ROOM);
    for budget in [&mut a, &mut b] {
        configure(&module, budget);
        budget.replacements = true;
        budget.sums = true;
        budget.head_descriptions = true;
        budget.sum_functions = vec![true; module.functions.len()];
    }
    let old_sums = sums::check_module(&module, &gs, &mut a).unwrap();
    let new_sums = sums::check_module(&module, &gs, &mut b).unwrap();
    let old_plan = replacements::Plan::new(
        &module,
        &gs,
        &old_sums.replacement_writes,
        &old_sums.replacement_targets,
        &old_sums.retirement_targets,
        &mut a,
    )
    .unwrap();
    let new_plan = replacements::Plan::new(
        &module,
        &gs,
        &new_sums.replacement_writes,
        &new_sums.replacement_targets,
        &new_sums.retirement_targets,
        &mut b,
    )
    .unwrap();
    let mut bindings = old_bindings(&module, &gs, None, &mut a).unwrap();
    TEST_EFFECT_SOLVES.with(|n| n.set(0));
    let old_effects = old_infer(&module, &gs, &mut a, Some(&old_plan), &mut bindings).unwrap();
    let old_solves = TEST_EFFECT_SOLVES.with(|n| n.get());
    let mut request = Request::new(&module, &gs, None, Some(&new_plan), &mut b).unwrap();
    TEST_EFFECT_SOLVES.with(|n| n.set(0));
    let new_effects = request.infer_effects().unwrap();
    let new_solves = TEST_EFFECT_SOLVES.with(|n| n.get());
    assert_eq!(new_effects, old_effects);
    assert_eq!(new_solves, old_solves);
    assert!(new_solves >= module.functions.len());
    assert!(!old_sums.retirement_targets.is_empty() || !old_sums.replacement_targets.is_empty());
    for index in 0..module.functions.len() {
        let mut expected = Slots::old_layout_new(
            &module,
            &module.functions[index],
            &gs[index].package_views,
            &mut a,
        )
        .unwrap();
        old_plan
            .extend_slots(&module.functions[index], &mut expected, &mut a)
            .unwrap();
        assert!(expected.retirement);
        assert_eq!(
            snapshot_slots(request.slots(index).unwrap()),
            snapshot_slots(&expected)
        );
        let old_rows = old_sums
            .rows
            .get(index)
            .filter(|r| !r.is_empty())
            .map(Vec::as_slice);
        let new_rows = new_sums
            .rows
            .get(index)
            .filter(|r| !r.is_empty())
            .map(Vec::as_slice);
        complete_ok(
            captured(|| {
                old_run(
                    &module,
                    &mut bindings[index],
                    &old_effects,
                    true,
                    old_rows,
                    &mut a,
                    Some(&old_plan),
                )
            }),
            captured(|| request.run(index, &new_effects, true, new_rows)),
        );
    }
    old_plan.close_pairs(&module, &mut a).unwrap();
    request.close_pairs().unwrap();
}

#[test]
fn slot_layout_foreign_and_empty_functions_keep_the_original_early_returns() {
    for foreign in [false, true] {
        let mut module = effect_fixture(&[vec![]], &[0]);
        let f = &mut module.functions[0];
        f.parameters.clear();
        f.locals.clear();
        f.values.clear();
        f.return_type = Type::Unit;
        f.blocks[0].instructions.clear();
        f.blocks[0].terminator = Terminator::Return(None);
        if foreign {
            f.foreign = Some(zil_ir::ForeignDeclaration {
                abi: "C".into(),
                symbol: "slot_control".into(),
                noescape_parameters: vec![],
                static_callbacks: false,
                thread_driver_transfer: false,
                scoped_callback: None,
            });
            // Invalid resource payload must remain unvisited by this early
            // foreign return. Full foreign definition checking is separate.
            f.locals = endpoint_fixture(true).functions.remove(0).locals;
        }
        let gs = graphs(&module);
        let effects = empty_effects(&module);
        let mut a = budget(ROOM);
        let mut b = budget(ROOM);
        configure(&module, &mut a);
        configure(&module, &mut b);
        a.sum_functions = vec![true];
        b.sum_functions = vec![true];
        let mut binding = desc_liveness::Binding::new(&module.functions[0], &gs[0]);
        let mut request = Request::new(&module, &gs, None, None, &mut b).unwrap();
        for _ in 0..2 {
            complete_ok(
                captured(|| old_run(&module, &mut binding, &effects, true, None, &mut a, None)),
                captured(|| request.run(0, &effects, true, None)),
            );
        }
        assert_eq!(request.slots(0).is_none(), foreign);
    }
}
