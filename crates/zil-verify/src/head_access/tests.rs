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
        f.values = vec![shared.clone(), shared.clone(), Type::I64].into();
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
        ]
        .into();
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

// Frozen complete outer driver; only the called body/name differ from389.
fn check_dense(module: &TypedModule, graphs: &[FunctionGraph], b: &mut Budget) -> Check<()> {
    if !b.head_descriptions {
        return Ok(());
    }
    charge(b, module.functions.len())?;
    let mut contracts = vec![Contract::default(); module.functions.len()];
    loop {
        let mut changed = false;
        for id in (0..module.functions.len()).rev() {
            let new = body_dense(module, &module.functions[id], &graphs[id], &contracts, b)?;
            changed |= merge(&mut contracts[id].returned, &new.returned, b)?;
            changed |= merge_constraints(&mut contracts[id].writes, &new.writes, b)?;
        }
        if !changed {
            break;
        }
    }
    for c in contracts {
        constraints_cost(&c.writes, b)?;
        if c.writes.contains(&Obligation::ReadOnly) {
            return Err(reject(
                "E-VERIFY-HEAD-PERMISSION",
                None,
                "actual shared resource head reaches a mutating access through descriptor forwarding or captures",
            ));
        }
    }
    Ok(())
}

fn exact_budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn table_case(case: u8, b: &mut Budget) -> Check<Vec<Rows>> {
    // The complete input payloads/expected dense state are test setup; only
    // the table operation under test consumes b. No language cost claim.
    let width = 65;
    let mut setup = budget();
    let mut row = LocalRows::new(width, &mut setup)?;
    let mut expected = vec![Rows::new(); width];
    let payload = |id: usize| {
        vec![
            (vec![id as u32, 3], vec![]),
            (
                vec![],
                vec![Obligation::ReadOnly, Obligation::Input(0, vec![1])],
            ),
        ]
    };
    for id in [64, 32, 0] {
        let value = payload(id);
        row.set(id, value.clone(), &mut setup)?;
        expected[id] = value;
    }
    match case {
        0 => {
            row = LocalRows::new(width, b)?;
            expected.fill(Rows::new());
        }
        1 => {
            assert!(row.get(1, b)?.is_empty());
            assert_eq!(row.get(32, b)?, &expected[32]);
            row.set(1, payload(1), b)?;
            expected[1] = payload(1);
        }
        2 => {
            let value = vec![(vec![7], vec![])];
            row.set(32, value.clone(), b)?;
            expected[32] = value;
            row.set(0, Rows::new(), b)?;
            expected[0].clear();
        }
        3 => {
            let copied = row.duplicate(b)?;
            row.set(0, Rows::new(), &mut setup)?;
            row = copied;
        }
        4 => {
            let mut from = LocalRows::new(width, &mut setup)?;
            for id in [1, 32, 64] {
                let value = vec![(
                    vec![],
                    vec![Obligation::Input(1, vec![9]), Obligation::ReadOnly],
                )];
                from.set(id, value.clone(), &mut setup)?;
                merge(&mut expected[id], &value, &mut setup)?;
            }
            row.merge_from(&from, b)?;
        }
        5 => {
            let empty = LocalRows::new(width, &mut setup)?;
            assert!(!row.same(&empty, b)?);
            let copied = row.duplicate(&mut setup)?;
            assert!(row.same(&copied, b)?);
        }
        6 => {
            row = LocalRows::new(width, b)?;
            for id in (0..width).rev() {
                row.set(id, payload(id), b)?;
                expected[id] = payload(id);
            }
        }
        _ => unreachable!(),
    }
    let actual = row.dense();
    assert_eq!(actual, expected);
    Ok(actual)
}
#[test]
fn sparse_table_and_body_have_exact_fresh_budget_boundaries() {
    for case in 0..7 {
        let mut ample = exact_budget(1_000_000);
        let expected = table_case(case, &mut ample).unwrap();
        let n = 1_000_000 - ample.remaining;
        assert!(n > 0);
        for _ in 0..2 {
            let mut b = exact_budget(n);
            assert_eq!(table_case(case, &mut b).unwrap(), expected);
            assert_eq!(b.remaining, 0);
            assert_eq!(
                table_case(case, &mut exact_budget(n - 1)).unwrap_err().code,
                "E-RESOURCE-LIMIT"
            );
        }
    }
    let module = fixture::module();
    let f = &module.functions[0];
    let g = graph(f);
    let contracts = vec![Contract::default(); module.functions.len()];
    let mut ample = exact_budget(1_000_000);
    let expected = body(&module, f, &g, &contracts, &mut ample).unwrap();
    let n = 1_000_000 - ample.remaining;
    for _ in 0..2 {
        let mut b = exact_budget(n);
        assert_eq!(body(&module, f, &g, &contracts, &mut b).unwrap(), expected);
        assert_eq!(b.remaining, 0);
        assert_eq!(
            body(&module, f, &g, &contracts, &mut exact_budget(n - 1))
                .unwrap_err()
                .code,
            "E-RESOURCE-LIMIT"
        );
    }
    // Single publish boundary: an insert rejected by fees keeps the table.
    let mut setup = budget();
    let mut row = LocalRows::new(2, &mut setup).unwrap();
    let before = row.dense();
    let payload = vec![(vec![], vec![])];
    assert!(row.set(1, payload, &mut exact_budget(0)).is_err());
    assert_eq!(row.dense(), before);
}
#[test]
fn complete_dense_driver_preserves_readonly_and_early_error_diagnostics() {
    // Kernel-level raw fixtures, not public-IR admission claims. The full old
    // driver and body independently reach the same complete diagnostic.
    for attack in 0..4 {
        let mut module = fixture::module();
        let mut f = module.functions[0].clone();
        f.id = FunctionId(0);
        f.environment = None;
        f.parameters = vec![LocalId(0)];
        f.locals.truncate(1);
        f.locals[0].ty = fixture::address(Type::ByteBuffer, false);
        f.values = vec![f.locals[0].ty.clone(), Type::I64].into();
        f.blocks = vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![fixture::instruction(
                Some(0),
                InstructionKind::Load(LocalId(0)),
            )],
            terminator: Terminator::Return(Some(ValueId(0))),
        }]
        .into();
        f.entry = BlockId(0);
        match attack {
            0 => {}
            1 => f.blocks[0].instructions.push(fixture::instruction(
                None,
                InstructionKind::ByteBufferReserve {
                    buffer: ValueId(0),
                    capacity: ValueId(1),
                },
            )),
            2 => f.blocks[0].instructions.insert(
                0,
                fixture::instruction(Some(0), InstructionKind::ReadCapture { index: 0 }),
            ),
            3 => {
                f.values[0] = Type::Closure(ClosureId(u32::MAX));
                f.blocks[0].instructions = vec![fixture::instruction(
                    None,
                    InstructionKind::CallClosure {
                        callee: ValueId(0),
                        arguments: vec![],
                    },
                )];
            }
            _ => unreachable!(),
        }
        module.functions = vec![f].into();
        let graphs: Vec<_> = module.functions.iter().map(graph).collect();
        let mut old_budget = budget();
        old_budget.head_descriptions = true;
        let mut new_budget = budget();
        new_budget.head_descriptions = true;
        let old = check_dense(&module, &graphs, &mut old_budget);
        let new = check(&module, &graphs, &mut new_budget);
        assert_eq!(new, old);
        if attack == 0 {
            assert!(new.is_ok());
        } else {
            let error = new.unwrap_err();
            assert_eq!(
                error.code,
                if attack == 1 {
                    "E-VERIFY-HEAD-PERMISSION"
                } else {
                    "E-VERIFY-HEAD-ACCESS"
                }
            );
            assert_eq!(
                error.message,
                match attack {
                    1 =>
                        "actual shared resource head reaches a mutating access through descriptor forwarding or captures",
                    2 => "capture without an environment",
                    3 => "invocation closure definition absent",
                    _ => unreachable!(),
                }
            );
        }
    }
}
