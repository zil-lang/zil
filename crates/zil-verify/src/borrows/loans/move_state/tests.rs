//! Complete old ownership solver and full-row comparisons. These raw transfer
//! fixtures isolate ownership; they do not claim whole-program admission.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, TargetProfile};
include!("reference.rs");

const ROOM: usize = 20_000_000;
fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn span(n: usize) -> Span {
    Span::new(FileId(0), n, n + 1)
}
fn affine() -> Type {
    Type::Address {
        mutable: true,
        inner: Box::new(Type::I32),
    }
}
fn instruction(result: Option<u32>, kind: InstructionKind, at: usize) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(at),
    }
}
fn fixture(width: usize) -> (TypedModule, FunctionGraph) {
    let f = TypedFunction {
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(0),
        name: "move_words".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0)],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: (0..2)
            .map(|i| LocalDecl {
                id: LocalId(i),
                name: format!("l{i}"),
                ty: Type::I32,
                mutable: true,
                span: span(0),
            })
            .collect(),
        values: vec![Type::I32; width].into(),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Trap,
        }]
        .into(),
        entry: BlockId(0),
        span: span(99),
    };
    let module = TypedModule {
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![f],
        closures: vec![ClosureDefinition {
            storage_life: None,
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        }],
        resources: vec![],
        unsupported: vec![],
    };
    (module, graph(&[vec![]], vec![true]))
}
fn graph(edges: &[Vec<usize>], reachable: Vec<bool>) -> FunctionGraph {
    let mut predecessors = vec![vec![]; edges.len()];
    for (from, next) in edges.iter().enumerate() {
        for &to in next {
            predecessors[to].push(from);
        }
    }
    FunctionGraph {
        successors: edges.to_vec(),
        predecessors,
        reachable,
        package_views: packages::CaptureViews::default(),
        role_data_uses: vec![],
    }
}
fn cfg(width: usize, entry: usize) -> (TypedModule, FunctionGraph) {
    let (mut module, _) = fixture(width);
    let f = &mut module.functions[0];
    f.entry = BlockId(entry as u32);
    f.blocks = (0..6)
        .map(|i| BasicBlock {
            id: BlockId(i),
            instructions: vec![],
            terminator: Terminator::Trap,
        })
        .collect();
    if width != 0 {
        let value = ValueId((width - 1) as u32);
        f.values[width - 1] = affine();
        f.blocks[entry].instructions.push(instruction(
            Some(value.0),
            InstructionKind::BorrowLocalMut { local: LocalId(0) },
            1,
        ));
        f.blocks[1].instructions.push(instruction(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value,
            },
            2,
        ));
        // Unreachable consumers must not enter the move fixed point.
        f.blocks[5].instructions.push(instruction(
            None,
            InstructionKind::MakeOwnedTuple {
                fields: vec![value, value],
            },
            3,
        ));
    }
    let (edges, reachable) = if entry == 0 {
        (
            vec![vec![1, 2], vec![3], vec![3], vec![0, 4], vec![], vec![3]],
            vec![true, true, true, true, true, false],
        )
    } else {
        (
            vec![vec![], vec![4], vec![3], vec![1, 4], vec![], vec![4]],
            vec![false, true, true, true, true, false],
        )
    };
    (module, graph(&edges, reachable))
}
type Outcome = (Check<Vec<bool>>, Vec<Snapshot>, usize);
fn run(module: &TypedModule, graph: &FunctionGraph, old: bool, n: usize) -> Outcome {
    TRACE.with(|trace| *trace.borrow_mut() = Some(vec![]));
    let mut b = budget(n);
    let result = if old {
        old_ownership(module, &module.functions[0], graph, &mut b)
    } else {
        ownership(module, &module.functions[0], graph, &mut b)
    };
    let trace = TRACE.with(|trace| trace.borrow_mut().take().unwrap());
    (result, trace, n - b.remaining)
}
fn success(
    module: &TypedModule,
    graph: &FunctionGraph,
) -> (Vec<bool>, Vec<Snapshot>, usize, usize) {
    let (old, before, old_n) = run(module, graph, true, ROOM);
    let (new, after, n) = run(module, graph, false, ROOM);
    let (old, new) = (old.unwrap(), new.unwrap());
    assert_eq!(new, old);
    assert_eq!(after, before);
    (new, after, n, old_n)
}
fn error(module: &TypedModule, graph: &FunctionGraph, text: &str, at: Span) {
    let (old, before, _) = run(module, graph, true, ROOM);
    let (new, after, _) = run(module, graph, false, ROOM);
    let (old, new) = (old.unwrap_err(), new.unwrap_err());
    assert_eq!(new, old);
    assert_eq!(after, before);
    assert_eq!(new.category, zil_base::ResultCategory::SafeReject);
    assert_eq!(new.code, "E-VERIFY-MOVE");
    assert_eq!(new.span, Some(at));
    assert!(new.message.contains(text), "{new:?}");
}

#[test]
fn move_rows_keep_complete_ssa_width_tail_and_dense_payload() {
    for width in [0, 1, 4, 8, 9, 63, 64, 65, 127, 128, 129, 513] {
        let mut actual = Row::new(width, &mut budget(ROOM)).unwrap();
        let mut other = Row::new(width, &mut budget(ROOM)).unwrap();
        let mut expected = vec![false; width];
        let mut from = vec![false; width];
        for i in 0..width {
            expected[i] = i.is_multiple_of(3);
            from[i] = i.is_multiple_of(5);
            actual.set(i, expected[i], &mut budget(ROOM)).unwrap();
            other.set(i, from[i], &mut budget(ROOM)).unwrap();
        }
        let original = actual.logical();
        let alias = actual.copy(&mut budget(ROOM)).unwrap();
        assert_eq!(
            actual.join(&other, &mut budget(ROOM)).unwrap(),
            old_bits_join(&mut expected, &from, &mut budget(ROOM)).unwrap()
        );
        assert_eq!(actual.logical(), expected);
        assert_eq!(alias.logical(), original);
        assert_eq!(other.logical(), from);
        for (i, expected) in expected.iter().enumerate() {
            assert_eq!(actual.get(i, &mut budget(ROOM)).unwrap(), *expected);
        }
        let before = actual.logical();
        for index in [width, usize::MAX] {
            assert!(actual.get(index, &mut budget(ROOM)).is_err());
            assert!(actual.set(index, true, &mut budget(ROOM)).is_err());
            assert_eq!(actual.logical(), before);
        }
        if let Row::Words { words, .. } = &actual {
            assert_eq!(words.len(), width.div_ceil(64));
            if !width.is_multiple_of(64) {
                assert_eq!(words.last().unwrap() >> (width % 64), 0);
            }
            let wrong = Row::new(width + 1, &mut budget(ROOM)).unwrap();
            assert!(actual.join(&wrong, &mut budget(ROOM)).is_err());
            assert!(actual.copy_from(&wrong, &mut budget(ROOM)).is_err());
            assert!(actual.same(&wrong, &mut budget(ROOM)).is_err());
            assert_eq!(actual.logical(), before);
        }
        let (inputs, outputs) = Row::tables(3, width, &mut budget(ROOM)).unwrap();
        assert_eq!(
            inputs.iter().map(Row::logical).collect::<Vec<_>>(),
            vec![vec![false; width]; 3]
        );
        assert_eq!(inputs, outputs);
    }
    assert_eq!(
        Row::tables(usize::MAX, 65, &mut budget(ROOM))
            .unwrap_err()
            .category,
        zil_base::ResultCategory::ResourceLimit
    );
}

#[test]
fn move_word_payment_precedes_mutation_and_preserves_the_paid_prefix() {
    let width = 129;
    let mut seed = Row::new(width, &mut budget(ROOM)).unwrap();
    seed.set(64, true, &mut budget(ROOM)).unwrap();
    let before = seed.logical();
    let mut incoming = Row::new(width, &mut budget(ROOM)).unwrap();
    incoming.set(128, true, &mut budget(ROOM)).unwrap();
    for operation in 0..6 {
        let call = |n| {
            let mut row = seed.clone();
            let mut b = budget(n);
            let result: Check<()> = (|| {
                match operation {
                    0 => row.set(128, true, &mut b)?,
                    1 => {
                        row.join(&incoming, &mut b)?;
                    }
                    2 => row.copy_from(&incoming, &mut b)?,
                    3 => {
                        assert!(!row.same(&incoming, &mut b)?);
                    }
                    4 => {
                        assert!(row.get(64, &mut b)?);
                    }
                    _ => {
                        row = row.copy(&mut b)?;
                    }
                }
                Ok(())
            })();
            (result, row.logical(), n - b.remaining)
        };
        let (ok, expected, n) = call(ROOM);
        ok.unwrap();
        let (failed, unchanged, _) = call(n - 1);
        assert_eq!(
            failed.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(unchanged, before);
        for _ in 0..2 {
            let (ok, actual, paid) = call(n);
            ok.unwrap();
            assert_eq!(actual, expected);
            assert_eq!(paid, n);
        }
        assert_eq!(seed.logical(), before);
    }
    let mut b = budget(ROOM);
    let mut actual = seed.clone();
    actual.join(&incoming, &mut b).unwrap();
    let join_n = ROOM - b.remaining;
    let mut b = budget(8 + join_n - 1);
    let mut actual = seed.clone();
    actual.set(0, true, &mut b).unwrap();
    assert_eq!(
        actual.join(&incoming, &mut b).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    let mut expected = before.clone();
    expected[0] = true;
    assert_eq!(actual.logical(), expected);
    assert_eq!(seed.logical(), before);
    for count in [0, 1, 3] {
        let mut b = budget(ROOM);
        let expected = Row::tables(count, width, &mut b).unwrap();
        let n = ROOM - b.remaining;
        assert!(Row::tables(count, width, &mut budget(n - 1)).is_err());
        assert_eq!(Row::tables(count, width, &mut budget(n)).unwrap(), expected);
        assert_eq!(Row::tables(count, width, &mut budget(n)).unwrap(), expected);
    }
}

#[test]
fn ownership_words_match_complete_bool_rounds_with_joins_backedges_and_dead_blocks() {
    for width in [0, 1, 8, 9, 63, 64, 65, 127, 128, 129, 512] {
        for entry in [0, 2] {
            let (module, graph) = cfg(width, entry);
            let (view, trace, n, old_n) = success(&module, &graph);
            assert_eq!(view, vec![false; width]);
            assert!(!trace.is_empty());
            for (inputs, outputs) in &trace {
                assert_eq!(inputs.len(), 6);
                assert_eq!(outputs.len(), 6);
                assert!(inputs.iter().chain(outputs).all(|row| row.len() == width));
                assert_eq!(inputs[entry], vec![false; width]);
                assert_eq!(inputs[5], vec![false; width]);
                assert_eq!(outputs[5], vec![false; width]);
            }
            if width > 0 {
                assert!(trace.last().unwrap().1[1][width - 1]);
            }
            if width <= SMALL {
                assert!(
                    n > old_n,
                    "dense enum inventory and dispatch are real added work"
                );
            }
        }
    }
}

fn consumer_cases() -> Vec<(InstructionKind, Vec<ValueId>, bool)> {
    let a = ValueId(0);
    let z = ValueId(1);
    let other = ValueId(2);
    let one = vec![a];
    vec![
        (
            InstructionKind::Store {
                local: LocalId(0),
                value: a,
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::StoreOwnedArrayElement {
                local: LocalId(0),
                index: 0,
                value: a,
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::StorePackageField {
                local: LocalId(0),
                index: 0,
                value: a,
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::MakeOwnedArray {
                elements: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::MakePackage {
                fields: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::MakeManaged {
                fields: one.clone(),
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::MakeAggregate {
                fields: one.clone(),
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::MakeOwnedTuple {
                fields: one.clone(),
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::MakeVariant {
                variant: 0,
                fields: one.clone(),
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::MakeErrorUnion {
                is_error: false,
                value: a,
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::ExtractField {
                aggregate: a,
                index: 0,
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::ExtractIndex {
                aggregate: a,
                index: other,
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::UpdateField {
                aggregate: a,
                index: 0,
                value: z,
            },
            vec![a, z],
            true,
        ),
        (
            InstructionKind::UpdateIndex {
                aggregate: a,
                index: other,
                value: z,
            },
            vec![a, z],
            true,
        ),
        (
            InstructionKind::CallClosureOnce {
                callee: a,
                arguments: vec![z],
            },
            vec![a, z],
            false,
        ),
        (
            InstructionKind::WithForeignCallback {
                register: FunctionId(0),
                close: FunctionId(0),
                callback: other,
                body: a,
                region: 0,
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::WithScopedThread {
                worker: FunctionId(0),
                argument: other,
                body: a,
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::WithSyncCell {
                kind: zil_ir::SyncCellKind::MutexI32,
                initial: other,
                body: a,
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::SuspendCall {
                function: FunctionId(0),
                arguments: one.clone(),
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::TaskCreate {
                worker: FunctionId(0),
                arguments: one.clone(),
                port_bindings: vec![],
            },
            one.clone(),
            true,
        ),
        (
            InstructionKind::Call {
                function: FunctionId(0),
                arguments: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::CallDynSlot {
                callee: other,
                interface: zil_ir::DynInterfaceId {
                    module: ModuleId(0),
                    index: 0,
                },
                slot: 0,
                arguments: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::CallIndirect {
                callee: other,
                arguments: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::ForeignCall {
                function: FunctionId(0),
                arguments: one.clone(),
                region: 0,
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::CallClosure {
                callee: other,
                arguments: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::CallClosureShared {
                callee: LocalId(0),
                arguments: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::CallClosureMut {
                callee: LocalId(0),
                arguments: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::CallClosureBorrowed {
                callee: other,
                kind: CallableKind::FnMut,
                arguments: one.clone(),
            },
            one.clone(),
            false,
        ),
        (
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: one.clone(),
            },
            one,
            false,
        ),
    ]
}

#[test]
fn ownership_each_actual_consumer_moves_once_and_copy_values_keep_the_old_exclusions() {
    for (kind, expected, guarded) in consumer_cases() {
        for enabled in [false, true] {
            let wanted = if guarded && !enabled {
                vec![]
            } else {
                expected.clone()
            };
            assert_eq!(
                consumers(&kind, enabled).copied().collect::<Vec<_>>(),
                wanted
            );
            assert_eq!(
                old_consumers(&kind, enabled).copied().collect::<Vec<_>>(),
                wanted
            );
        }
        let (mut module, graph) = fixture(65);
        for &value in &expected {
            module.functions[0].values[value.0 as usize] = affine();
        }
        module.functions[0].blocks[0]
            .instructions
            .push(instruction(None, kind.clone(), 4));
        let (_, trace, _, _) = success(&module, &graph);
        let mut moved = vec![false; 65];
        for &value in &expected {
            moved[value.0 as usize] = true;
        }
        assert_eq!(trace.last().unwrap().1[0], moved, "{kind:?}");
        module.functions[0].blocks[0].instructions.push(instruction(
            None,
            InstructionKind::ReadAddress {
                address: expected[0],
            },
            5,
        ));
        error(&module, &graph, "already transferred", span(5));
        // Identical operands with Copy metadata do not become affine by storage.
        module.functions[0].values.fill(Type::I32);
        let (_, trace, _, _) = success(&module, &graph);
        assert_eq!(trace.last().unwrap().1[0], vec![false; 65]);
    }
}

#[test]
fn ownership_views_return_checks_duplicate_consumers_and_error_order_remain_exact() {
    let (mut module, graph) = fixture(65);
    module.functions[0].values[0] = affine();
    module.functions[0].values[1] = affine();
    module.functions[0].blocks[0].instructions = vec![
        instruction(Some(0), InstructionKind::Load(LocalId(0)), 10),
        instruction(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(1),
            },
            11,
        ),
        instruction(
            None,
            InstructionKind::MakeOwnedTuple {
                fields: vec![ValueId(0), ValueId(1)],
            },
            12,
        ),
    ];
    // All actual operands are checked before even the first consumer/view.
    error(&module, &graph, "already transferred", span(12));
    module.functions[0].blocks[0].instructions.remove(1);
    error(&module, &graph, "access view", span(12));
    module.functions[0].blocks[0].instructions = vec![instruction(
        None,
        InstructionKind::MakeOwnedTuple {
            fields: vec![ValueId(0), ValueId(0)],
        },
        13,
    )];
    error(&module, &graph, "already transferred", span(13));
    for kind in [
        InstructionKind::Load(LocalId(0)),
        InstructionKind::AddressField {
            address: ValueId(2),
            index: 0,
        },
        InstructionKind::AddressSlice {
            address: ValueId(2),
            start: None,
            end: None,
        },
        InstructionKind::AddressIndex {
            address: ValueId(2),
            index: ValueId(3),
        },
    ] {
        module.functions[0].blocks[0].instructions = vec![instruction(Some(0), kind, 14)];
        module.functions[0].blocks[0].terminator = Terminator::Trap;
        let (view, _, _, _) = success(&module, &graph);
        assert!(view[0]);
        assert_eq!(view.iter().filter(|&&b| b).count(), 1);
        module.functions[0].blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
        error(&module, &graph, "return cannot copy", span(99));
    }
    module.functions[0].blocks[0].instructions = vec![instruction(
        None,
        InstructionKind::Store {
            local: LocalId(0),
            value: ValueId(0),
        },
        15,
    )];
    error(&module, &graph, "already transferred", span(99));
    module.functions[0].blocks[0].terminator = Terminator::Trap;
    module.functions[0].blocks[0].instructions.push(instruction(
        Some(0),
        InstructionKind::MakeOwnedTuple {
            fields: vec![ValueId(0)],
        },
        16,
    ));
    error(&module, &graph, "already transferred", span(16));
}

#[test]
fn ownership_callback_classification_keeps_unreachable_declarations_and_original_views() {
    for kind in [
        InstructionKind::MakeOwnedArray {
            elements: vec![ValueId(0)],
        },
        InstructionKind::MakePackage {
            fields: vec![ValueId(0)],
        },
        InstructionKind::StoreOwnedArrayElement {
            local: LocalId(0),
            index: 0,
            value: ValueId(0),
        },
        InstructionKind::StorePackageField {
            local: LocalId(0),
            index: 0,
            value: ValueId(0),
        },
    ] {
        let (mut module, _) = fixture(65);
        module.functions[0].values[0] = Type::Closure(ClosureId(0));
        module.functions[0].blocks[0].instructions = vec![
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
                20,
            ),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
                21,
            ),
        ];
        module.functions[0].blocks.push(BasicBlock {
            id: BlockId(1),
            instructions: vec![instruction(None, kind, 22)],
            terminator: Terminator::Trap,
        });
        let g = graph(&[vec![], vec![]], vec![true, false]);
        error(&module, &g, "already transferred", span(21));
    }
    let (mut module, graph) = fixture(65);
    module.functions[0].values[0] = Type::Closure(ClosureId(0));
    module.functions[0].blocks[0].instructions = vec![
        instruction(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(0),
            },
            23,
        ),
        instruction(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(0),
            },
            24,
        ),
    ];
    // Ordinary resource-free closures retain their old Copy profile.
    success(&module, &graph);
    module.closures[0].captures.push(affine());
    error(&module, &graph, "already transferred", span(24));
}

#[test]
fn ownership_complete_requests_have_fresh_exact_budgets_and_real_word_savings() {
    for width in [0, 1, 8, 9, 65, 129, 512] {
        let (module, graph) = cfg(width, 0);
        let original = module.clone();
        let (expected, expected_trace, n, old_n) = success(&module, &graph);
        assert!(n > 0, "even empty SSA rows now carry paid enum inventory");
        for (old, exact) in [(false, n), (true, old_n)] {
            if exact > 0 {
                let (failed, _, _) = run(&module, &graph, old, exact - 1);
                assert_eq!(
                    failed.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                );
            } else {
                assert!(old);
                assert_eq!(width, 0);
            }
            for _ in 0..2 {
                let (actual, trace, spent) = run(&module, &graph, old, exact);
                assert_eq!(actual.unwrap(), expected);
                assert_eq!(trace, expected_trace);
                assert_eq!(spent, exact);
            }
        }
        assert_eq!(module, original);
        if width <= SMALL {
            assert!(
                n > old_n,
                "dense enum inventory and dispatch are real added work"
            );
        }
        if width == 512 {
            assert!(n < old_n, "complete word ownership {n} >= old {old_n}");
        }
        eprintln!(
            "complete ownership width={width}, words N={n}, frozen bool N={old_n}; N-1 then fresh N full views/matrices"
        );
    }
}
