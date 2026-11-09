use super::*;
#[allow(dead_code)]
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
    ));
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Round {
    outputs: Vec<Vec<Rows>>,
    progressed: bool,
    contract: Contract,
}
thread_local! {
    static TRACE: std::cell::RefCell<Option<Vec<Round>>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn record_dense(outputs: &[Vec<Rows>], progressed: bool, result: &Contract) {
    TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().as_mut() {
            trace.push(Round {
                outputs: outputs.to_vec(),
                progressed,
                contract: result.clone(),
            });
        }
    });
}
pub(super) fn record_sparse(outputs: &[LocalRows], progressed: bool, result: &Contract) {
    TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().as_mut() {
            trace.push(Round {
                outputs: outputs.iter().map(LocalRows::dense).collect(),
                progressed,
                contract: result.clone(),
            });
        }
    });
}
fn budget() -> Budget {
    Budget::new(&Limits::default())
}
fn graph(f: &TypedFunction) -> FunctionGraph {
    let mut successors = vec![vec![]; f.blocks.len()];
    let mut predecessors = successors.clone();
    for (id, block) in f.blocks.iter().enumerate() {
        successors[id] = match block.terminator {
            Terminator::Jump(to) => vec![to.0 as usize],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block.0 as usize, else_block.0 as usize],
            _ => vec![],
        };
        for &to in &successors[id] {
            predecessors[to].push(id);
        }
    }
    let mut reachable = vec![false; f.blocks.len()];
    let mut pending = vec![f.entry.0 as usize];
    while let Some(id) = pending.pop() {
        if !reachable[id] {
            reachable[id] = true;
            pending.extend(&successors[id]);
        }
    }
    FunctionGraph {
        successors,
        predecessors,
        reachable,
        package_views: packages::CaptureViews::default(),
        role_data_uses: vec![],
    }
}
fn compare(module: &TypedModule, f: &TypedFunction, contracts: &[Contract]) -> Contract {
    let graph = graph(f);
    TRACE.with(|trace| *trace.borrow_mut() = Some(Vec::new()));
    let expected = body_dense(module, f, &graph, contracts, &mut budget()).unwrap();
    let old_trace = TRACE.with(|trace| trace.borrow_mut().replace(Vec::new()).unwrap());
    let actual = body(module, f, &graph, contracts, &mut budget()).unwrap();
    let new_trace = TRACE.with(|trace| trace.borrow_mut().take().unwrap());
    assert_eq!(
        new_trace, old_trace,
        "all block outputs, progressed and complete Contract per round"
    );
    assert_eq!(
        actual, expected,
        "complete returned and writes, including discovery order"
    );
    actual
}
#[test]
fn sparse_local_rows_preserve_empty_constraints_order_bounds_and_clear() {
    for width in [0, 1, 63, 64, 65, 129] {
        let mut b = budget();
        let before = b.remaining;
        let mut row = LocalRows::new(width, &mut b).unwrap();
        assert!(b.remaining < before, "zero width still pays a table record");
        assert!(row.get(width, &mut b).is_err());
        assert!(row.set(width, Rows::new(), &mut b).is_err());
        if width == 0 {
            continue;
        }
        let last = width - 1;
        let payload = vec![
            (vec![3, 2], vec![]),
            (
                vec![],
                vec![Obligation::Input(0, vec![5]), Obligation::ReadOnly],
            ),
        ];
        row.set(last, payload.clone(), &mut b).unwrap();
        assert_eq!(row.get(last, &mut b).unwrap(), &payload);
        let cloned = row.duplicate(&mut b).unwrap();
        assert!(row.same(&cloned, &mut b).unwrap());
        let mut merged = LocalRows::new(width, &mut b).unwrap();
        merged.merge_from(&row, &mut b).unwrap();
        assert_eq!(merged.dense(), row.dense());
        assert_eq!(
            merged.get(last, &mut b).unwrap()[0],
            (vec![3, 2], vec![]),
            "nonempty Rows with empty constraints cannot disappear"
        );
        row.set(last, Rows::new(), &mut b).unwrap();
        assert!(row.get(last, &mut b).unwrap().is_empty());
        assert!(row.dense().iter().all(Vec::is_empty));
        let before = row.dense();
        assert!(
            row.merge_from(&LocalRows::new(width + 1, &mut b).unwrap(), &mut b)
                .is_err()
        );
        assert_eq!(row.dense(), before);
    }
}
#[test]
fn sparse_head_body_equals_original_dense_rounds_and_contracts() {
    for module in [
        fixture::module(),
        fixture::exclusive_returns(false),
        fixture::mutable_holder(),
        fixture::shared_two_slots(),
        fixture::captured_fixed(CallableKind::FnMut, false),
        fixture::captured_owned_fixed(false),
    ] {
        let mut contracts = vec![Contract::default(); module.functions.len()];
        // Exercise evolving callees without changing the original call order.
        for _ in 0..3 {
            for f in module.functions.iter().rev() {
                let result = compare(&module, f, &contracts);
                merge(
                    &mut contracts[f.id.0 as usize].returned,
                    &result.returned,
                    &mut budget(),
                )
                .unwrap();
                merge_constraints(
                    &mut contracts[f.id.0 as usize].writes,
                    &result.writes,
                    &mut budget(),
                )
                .unwrap();
            }
        }
    }
    let module = fixture::module();
    for width in [2, 65, 129] {
        // Algorithm oracle, not a claim that these synthetic raw operations
        // pass the public type checker: join a cleared local with a real
        // readonly row, then preserve it through a backedge and mutating use.
        let mut f = module.functions[0].clone();
        let shared = fixture::address(Type::I64, false);
        f.values = vec![shared.clone(), shared.clone(), Type::I64];
        f.locals.resize(width, f.locals[0].clone());
        for (id, local) in f.locals.iter_mut().enumerate() {
            local.id = LocalId(id as u32);
            local.ty = shared.clone();
        }
        let local = LocalId((width - 1) as u32);
        f.blocks = vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![fixture::instruction(
                    Some(0),
                    InstructionKind::Load(LocalId(0)),
                )],
                terminator: Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![fixture::instruction(
                    None,
                    InstructionKind::Store {
                        local,
                        value: ValueId(0),
                    },
                )],
                terminator: Terminator::Jump(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![fixture::instruction(
                    None,
                    InstructionKind::StorageDead(local),
                )],
                terminator: Terminator::Jump(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![
                    fixture::instruction(Some(1), InstructionKind::TakeLocal { local }),
                    fixture::instruction(
                        None,
                        InstructionKind::ByteBufferReserve {
                            buffer: ValueId(1),
                            capacity: ValueId(2),
                        },
                    ),
                ],
                terminator: Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(4),
                },
            },
            BasicBlock {
                id: BlockId(4),
                instructions: vec![],
                terminator: Terminator::Return(Some(ValueId(1))),
            },
            BasicBlock {
                id: BlockId(5),
                instructions: vec![fixture::instruction(
                    None,
                    InstructionKind::StorageDead(LocalId(0)),
                )],
                terminator: Terminator::Jump(BlockId(3)),
            },
        ];
        let result = compare(
            &module,
            &f,
            &vec![Contract::default(); module.functions.len()],
        );
        assert!(
            result.writes.contains(&Obligation::ReadOnly),
            "one cleared predecessor must not hide the readonly write"
        );
    }
}
