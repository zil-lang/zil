use super::*;
#[path = "liveness_bool_oracle.rs"]
mod oracle;
fn compare(words: &Bits, reference: &[bool]) {
    assert_eq!(words.width, reference.len());
    for (index, expected) in reference.iter().enumerate() {
        assert_eq!(words.get(index), Some(*expected));
    }
    assert_eq!(
        words.ones().collect::<Vec<_>>(),
        reference
            .iter()
            .enumerate()
            .filter_map(|(i, yes)| yes.then_some(i))
            .collect::<Vec<_>>()
    );
    assert_eq!(words.get(reference.len()), None);
    if !words.width.is_multiple_of(64) {
        assert_eq!(words.words.last().unwrap() >> (words.width % 64), 0);
    }
}
#[test]
fn packed_liveness_matches_full_bool_at_word_and_local_boundaries() {
    let mut b = Budget::new(&Limits::default());
    for width in [0, 1, 63, 64, 65, 127, 128, 129] {
        let mut packed = Bits::new(width);
        let mut reference = vec![false; width];
        for (index, slot) in reference.iter_mut().enumerate() {
            *slot = index % 3 == 0;
            packed.assign(index, *slot).unwrap();
        }
        let mut other = Bits::new(width);
        let mut old_other = vec![false; width];
        for (index, slot) in old_other.iter_mut().enumerate() {
            *slot = index % 5 == 0;
            other.assign(index, *slot).unwrap();
        }
        assert_eq!(
            packed.join(&other, &mut b).unwrap(),
            oracle::bits_join(&mut reference, &old_other, &mut b).unwrap()
        );
        compare(&packed, &reference);
        for index in (0..width).step_by(7) {
            packed.assign(index, false).unwrap();
            reference[index] = false;
        }
        compare(&packed, &reference);
        let before = packed.clone();
        assert!(packed.assign(width, true).is_err());
        assert!(packed.contains(width).is_err());
        assert_eq!(packed, before);
    }
}
#[test]
fn packed_reverse_matches_original_kill_gen_and_retained_order() {
    let mut b = Budget::new(&Limits::default());
    for width in [63, 64, 65, 127, 128, 129] {
        let values = if width > 64 { 63 } else { width / 2 };
        let local = LocalId((width - values - 1) as u32);
        let schedule = crate::place_replacements::Schedule {
            entries: vec![None],
            retained: vec![vec![vec![ValueId(1), ValueId((values - 1) as u32)]]],
        };
        for kind in [
            InstructionKind::Store {
                local,
                value: ValueId(0),
            },
            InstructionKind::StorageLive(local),
            InstructionKind::StorageDead(local),
            InstructionKind::Load(local),
            InstructionKind::TakeLocal { local },
            InstructionKind::BorrowLocalMut { local },
            InstructionKind::BorrowLocal { local },
            InstructionKind::BorrowDynOwned {
                local,
                mutable: true,
            },
            InstructionKind::CallClosureMut {
                callee: local,
                arguments: vec![ValueId(0)],
            },
            InstructionKind::CallClosureShared {
                callee: local,
                arguments: vec![ValueId(0)],
            },
        ] {
            let mut packed = Bits::new(width);
            let mut reference = vec![false; width];
            for (index, slot) in reference.iter_mut().enumerate() {
                *slot = index % 3 == 0;
                packed.assign(index, *slot).unwrap();
            }
            let instruction = Instruction {
                result: Some(ValueId(1)),
                kind,
                span: Span::default(),
            };
            let terminator = Terminator::Return(Some(ValueId(2)));
            packed.term_use(&terminator).unwrap();
            oracle::term_use(&mut reference, &terminator);
            compare(&packed, &reference);
            packed.reverse(&instruction, values).unwrap();
            oracle::reverse(&mut reference, &instruction, values);
            compare(&packed, &reference);
            packed.retain(Some(&schedule), 0, 0, &mut b).unwrap();
            oracle::retain_replacement_values(&mut reference, Some(&schedule), 0, 0, &mut b)
                .unwrap();
            compare(&packed, &reference);
            assert!(
                packed.contains(1).unwrap(),
                "retained schedule runs after result kill"
            );
        }
    }
}

#[test]
fn packed_cfg_inputs_outputs_and_instruction_rows_match_full_bool_with_backedges() {
    // Raw static-dataflow oracle, not a Source acceptance claim. Include a
    // diamond, a real backedge, a dead block and hidden retained SSA uses.
    for width in [63, 64, 65, 127, 128, 129] {
        let values = if width > 64 { 63 } else { width / 2 };
        let last_local = LocalId((width - values - 1) as u32);
        let instruction = |result, kind| Instruction {
            result,
            kind,
            span: Span::default(),
        };
        let blocks = [
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
                    instruction(
                        Some(ValueId((values - 1) as u32)),
                        InstructionKind::Load(last_local),
                    ),
                ],
                terminator: Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![instruction(
                    Some(ValueId(1)),
                    InstructionKind::BorrowLocal { local: LocalId(0) },
                )],
                terminator: Terminator::Jump(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![instruction(
                    Some(ValueId(2)),
                    InstructionKind::TakeLocal { local: last_local },
                )],
                terminator: Terminator::Jump(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![instruction(
                    None,
                    InstructionKind::CallClosureMut {
                        callee: last_local,
                        arguments: vec![ValueId(0)],
                    },
                )],
                terminator: Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(0),
                    else_block: BlockId(4),
                },
            },
            BasicBlock {
                id: BlockId(4),
                instructions: vec![instruction(
                    Some(ValueId(3)),
                    InstructionKind::Load(last_local),
                )],
                terminator: Terminator::Return(Some(ValueId(3))),
            },
            BasicBlock {
                id: BlockId(5),
                instructions: vec![instruction(
                    Some(ValueId(4)),
                    InstructionKind::BorrowDynOwned {
                        local: last_local,
                        mutable: true,
                    },
                )],
                terminator: Terminator::Return(Some(ValueId(4))),
            },
        ];
        let successors = [vec![1, 2], vec![3], vec![3], vec![0, 4], vec![], vec![]];
        let reachable = [true, true, true, true, true, false];
        let mut schedule = crate::place_replacements::Schedule {
            entries: vec![None; blocks.len()],
            retained: blocks
                .iter()
                .map(|block| vec![vec![]; block.instructions.len()])
                .collect(),
        };
        schedule.retained[3][0].push(ValueId((values - 1) as u32));
        let mut input = vec![Bits::new(width); blocks.len()];
        let mut output = input.clone();
        let mut old_input = vec![vec![false; width]; blocks.len()];
        let mut old_output = old_input.clone();
        let mut b = Budget::new(&Limits::default());
        let mut rounds = 0;
        loop {
            rounds += 1;
            let mut changed = false;
            for block in (0..blocks.len()).rev() {
                if !reachable[block] {
                    continue;
                }
                let mut next = Bits::new(width);
                let mut old = vec![false; width];
                for &child in &successors[block] {
                    assert_eq!(
                        next.join(&input[child], &mut b).unwrap(),
                        oracle::bits_join(&mut old, &old_input[child], &mut b).unwrap()
                    );
                }
                output[block].clone_from(&next);
                old_output[block].clone_from(&old);
                next.term_use(&blocks[block].terminator).unwrap();
                oracle::term_use(&mut old, &blocks[block].terminator);
                for (position, i) in blocks[block].instructions.iter().enumerate().rev() {
                    next.reverse(i, values).unwrap();
                    oracle::reverse(&mut old, i, values);
                    next.retain(Some(&schedule), block, position, &mut b)
                        .unwrap();
                    oracle::retain_replacement_values(
                        &mut old,
                        Some(&schedule),
                        block,
                        position,
                        &mut b,
                    )
                    .unwrap();
                }
                compare(&next, &old);
                compare(&output[block], &old_output[block]);
                assert_eq!(next != input[block], old != old_input[block]);
                if next != input[block] {
                    input[block] = next;
                    old_input[block] = old;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
            assert!(rounds < 32);
        }
        assert!(rounds > 1);
        for (block, data) in blocks.iter().enumerate() {
            if !reachable[block] {
                continue;
            }
            let mut next = output[block].clone();
            let mut old = old_output[block].clone();
            next.term_use(&data.terminator).unwrap();
            oracle::term_use(&mut old, &data.terminator);
            for (position, i) in data.instructions.iter().enumerate().rev() {
                compare(&next, &old); // after[position], before reverse
                next.reverse(i, values).unwrap();
                oracle::reverse(&mut old, i, values);
                next.retain(Some(&schedule), block, position, &mut b)
                    .unwrap();
                oracle::retain_replacement_values(
                    &mut old,
                    Some(&schedule),
                    block,
                    position,
                    &mut b,
                )
                .unwrap();
            }
        }
    }
}
type Round = (Vec<Vec<bool>>, Vec<Vec<bool>>, bool);
#[derive(Debug, PartialEq, Eq)]
struct Trace {
    input: Vec<Vec<bool>>,
    output: Vec<Vec<bool>>,
    rounds: Vec<Round>,
    afters: Vec<Option<Vec<Vec<bool>>>>,
}
fn expand_all(rows: &[Bits]) -> Vec<Vec<bool>> {
    rows.iter()
        .map(|row| (0..row.width).map(|i| row.get(i).unwrap()).collect())
        .collect()
}
fn packed_parent(
    f: &TypedFunction,
    graph: &FunctionGraph,
    schedule: Option<&crate::place_replacements::Schedule>,
    budget: &mut Budget,
) -> Check<Trace> {
    let n = f.blocks.len();
    let v = f.values.len();
    let m = f.locals.len();
    let reachable = &graph.reachable;
    let successors = &graph.successors;
    let width = v
        .checked_add(m)
        .ok_or_else(|| limit("borrow liveness width overflow"))?;
    let units = Bits::word_count(width);
    budget.spend(2)?; // Both outer row containers, including logical width zero.
    reserve(budget, &[n, units + 1, 2])?;
    let mut live_in: Vec<_> = (0..n).map(|_| Bits::new(width)).collect();
    let mut live_out = live_in.clone();
    let mut rounds = Vec::new();
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            budget.spend(1)?;
            if !reachable[b] {
                continue;
            }
            budget.spend(units + 1)?;
            let mut live = Bits::new(width);
            for &next in &successors[b] {
                live.join(&live_in[next], budget)?;
            }
            budget.spend(live.work())?;
            live_out[b].clone_from(&live);
            live.term_use(&f.blocks[b].terminator)?;
            for (position, instruction) in f.blocks[b].instructions.iter().enumerate().rev() {
                budget.spend(1 + instruction_operands(&instruction.kind).count())?;
                live.reverse(instruction, v)?;
                live.retain(schedule, b, position, budget)?;
            }
            budget.spend(live.work())?;
            if live != live_in[b] {
                live_in[b] = live;
                changed = true;
            }
        }
        rounds.push((expand_all(&live_in), expand_all(&live_out), changed));
        if !changed {
            break;
        }
    }

    let mut afters = Vec::new();
    for (b, block) in f.blocks.iter().enumerate() {
        if !reachable[b] {
            afters.push(None);
            continue;
        }
        // Placeholder records and full snapshots are both prepaid.
        budget.spend(1)?;
        reserve(budget, &[block.instructions.len(), units + 2])?;
        let mut after = vec![Bits::default(); block.instructions.len()];
        budget.spend(live_out[b].work())?;
        let mut live = live_out[b].clone();
        live.term_use(&block.terminator)?;
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            budget.spend(1 + instruction_operands(&instruction.kind).count())?;
            after[i] = live.clone();
            live.reverse(instruction, v)?;
            live.retain(schedule, b, i, budget)?;
        }
        afters.push(Some(expand_all(&after)));
    }
    Ok(Trace {
        input: expand_all(&live_in),
        output: expand_all(&live_out),
        rounds,
        afters,
    })
}
fn packed_loans(
    f: &TypedFunction,
    graph: &FunctionGraph,
    schedule: Option<&crate::place_replacements::Schedule>,
    budget: &mut Budget,
) -> Check<Trace> {
    let n = f.blocks.len();
    let v = f.values.len();
    let width = v
        .checked_add(f.locals.len())
        .ok_or_else(|| limit("loan liveness overflow"))?;
    let units = Bits::word_count(width);
    budget.spend(2)?;
    reserve(budget, &[n, units + 1, 2])?;
    let mut live_in: Vec<_> = (0..n).map(|_| Bits::new(width)).collect();
    let mut live_out = live_in.clone();
    let mut rounds = Vec::new();
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            if !graph.reachable[b] {
                continue;
            }
            budget.spend(units + 1)?;
            let mut live = Bits::new(width);
            for &next in &graph.successors[b] {
                live.join(&live_in[next], budget)?;
            }
            budget.spend(live.work())?;
            live_out[b].clone_from(&live);
            live.term_use(&f.blocks[b].terminator)?;
            for (position, instruction) in f.blocks[b].instructions.iter().enumerate().rev() {
                budget.spend(1 + instruction_operands(&instruction.kind).count())?;
                live.reverse(instruction, v)?;
                live.retain(schedule, b, position, budget)?;
            }
            budget.spend(live.work())?;
            if live != live_in[b] {
                live_in[b] = live;
                changed = true;
            }
        }
        rounds.push((expand_all(&live_in), expand_all(&live_out), changed));
        if !changed {
            break;
        }
    }
    let mut afters = Vec::new();
    for (b, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[b] {
            afters.push(None);
            continue;
        }
        budget.spend(1)?;
        reserve(budget, &[block.instructions.len(), units + 2])?;
        let mut after = vec![Bits::default(); block.instructions.len()];
        budget.spend(live_out[b].work())?;
        let mut live = live_out[b].clone();
        live.term_use(&block.terminator)?;
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            budget.spend(1 + instruction_operands(&instruction.kind).count())?;
            after[i] = live.clone();
            live.reverse(instruction, v)?;
            live.retain(schedule, b, i, budget)?;
        }
        afters.push(Some(expand_all(&after)));
    }
    Ok(Trace {
        input: expand_all(&live_in),
        output: expand_all(&live_out),
        rounds,
        afters,
    })
}

fn fixture(
    width: usize,
) -> (
    TypedFunction,
    FunctionGraph,
    crate::place_replacements::Schedule,
) {
    let (values, locals, blocks, successors, reachable, schedule) = if width < 5 {
        let values = width;
        let blocks = vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: if values == 0 {
                Terminator::Return(None)
            } else {
                Terminator::Return(Some(ValueId(0)))
            },
        }];
        (
            values,
            0,
            blocks,
            vec![vec![]],
            vec![true],
            crate::place_replacements::Schedule {
                entries: vec![None],
                retained: vec![vec![]],
            },
        )
    } else {
        let values = if width > 64 { 63 } else { width / 2 };
        let last_local = LocalId((width - values - 1) as u32);
        let instruction = |result, kind| Instruction {
            result,
            kind,
            span: Span::default(),
        };
        let blocks = vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
                    instruction(
                        Some(ValueId((values - 1) as u32)),
                        InstructionKind::Load(last_local),
                    ),
                ],
                terminator: Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![instruction(
                    Some(ValueId(1)),
                    InstructionKind::BorrowLocal { local: LocalId(0) },
                )],
                terminator: Terminator::Jump(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![instruction(
                    Some(ValueId(2)),
                    InstructionKind::TakeLocal { local: last_local },
                )],
                terminator: Terminator::Jump(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![instruction(
                    None,
                    InstructionKind::CallClosureMut {
                        callee: last_local,
                        arguments: vec![ValueId(0)],
                    },
                )],
                terminator: Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(0),
                    else_block: BlockId(4),
                },
            },
            BasicBlock {
                id: BlockId(4),
                instructions: vec![instruction(
                    Some(ValueId(3)),
                    InstructionKind::Load(last_local),
                )],
                terminator: Terminator::Return(Some(ValueId(3))),
            },
            BasicBlock {
                id: BlockId(5),
                instructions: vec![instruction(
                    Some(ValueId(4)),
                    InstructionKind::BorrowDynOwned {
                        local: last_local,
                        mutable: true,
                    },
                )],
                terminator: Terminator::Jump(BlockId(4)),
            },
        ];
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![0, 4], vec![], vec![4]];
        let reachable = vec![true, true, true, true, true, false];
        let mut schedule = crate::place_replacements::Schedule {
            entries: vec![None; blocks.len()],
            retained: blocks
                .iter()
                .map(|block| vec![vec![]; block.instructions.len()])
                .collect(),
        };
        schedule.retained[3][0].push(ValueId((values - 1) as u32));

        (
            values,
            width - values,
            blocks,
            successors,
            reachable,
            schedule,
        )
    };
    let mut predecessors = vec![vec![]; blocks.len()];
    for (from, targets) in successors.iter().enumerate() {
        for &to in targets {
            predecessors[to].push(from);
        }
    }
    let f = TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        resume_bound: zil_ir::TaskResumeBound::Local,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contracts: Vec::new(),
        id: FunctionId(0),
        name: "word-liveness-oracle".into(),
        foreign: None,
        environment: None,
        parameters: vec![],
        return_type: Type::I64,
        result_mode: PassingMode::Infer,
        locals: (0..locals)
            .map(|i| LocalDecl {
                id: LocalId(i as u32),
                name: format!("l{i}"),
                ty: Type::I64,
                mutable: true,
                span: Span::default(),
            })
            .collect(),
        values: vec![Type::I64; values],
        blocks,
        entry: BlockId(0),
        span: Span::default(),
    };
    let graph = FunctionGraph {
        successors,
        predecessors,
        reachable,
        package_views: Default::default(),
        role_data_uses: vec![],
    };
    (f, graph, schedule)
}
fn budget(steps: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: steps,
        ..Limits::default()
    })
}
#[test]
fn both_complete_cfg_kernels_match_frozen_bool_every_round_and_after() {
    for width in [0, 1, 4, 63, 64, 65, 127, 128, 129, 512] {
        let (f, graph, schedule) = fixture(width);
        for schedule in [None, Some(&schedule)] {
            let old_parent = oracle::parent(&f, &graph, schedule, &mut budget(10_000_000)).unwrap();
            let new_parent = packed_parent(&f, &graph, schedule, &mut budget(10_000_000)).unwrap();
            assert_eq!(new_parent, old_parent);
            let old_loans = oracle::loans(&f, &graph, schedule, &mut budget(10_000_000)).unwrap();
            let new_loans = packed_loans(&f, &graph, schedule, &mut budget(10_000_000)).unwrap();
            assert_eq!(new_loans, old_loans);
            assert_eq!(new_parent, new_loans);
            if width >= 5 {
                assert!(new_parent.rounds.len() > 1);
                assert!(new_parent.afters[5].is_none());
                assert!(graph.predecessors[4].contains(&5));
                // The current result stays present in a full after row; it is
                // killed only by reverse, never before immediate access checks.
                assert!(new_parent.afters[4].as_ref().unwrap()[0][3]);
            }
        }
    }
}
#[test]
fn fresh_word_solver_budgets_include_empty_containers_and_actual_operands() {
    for width in [0, 1, 65, 512] {
        let (f, graph, schedule) = fixture(width);
        for solver in [packed_parent, packed_loans] {
            let mut large = budget(10_000_000);
            let expected = solver(&f, &graph, Some(&schedule), &mut large).unwrap();
            let n = 10_000_000 - large.remaining;
            assert!(n > 0, "zero-width containers still consume real work");
            for _ in 0..2 {
                let mut exact = budget(n);
                assert_eq!(
                    solver(&f, &graph, Some(&schedule), &mut exact).unwrap(),
                    expected
                );
                assert_eq!(exact.remaining, 0);
                let error = solver(&f, &graph, Some(&schedule), &mut budget(n - 1)).unwrap_err();
                assert_eq!(error.code, "E-RESOURCE-LIMIT");
            }
        }
    }
    // Repeated operands have width one but must incur their actual visits.
    let (mut f, graph, mut schedule) = fixture(1);
    let run = |count, f: &mut TypedFunction, schedule: &mut crate::place_replacements::Schedule| {
        f.blocks[0].instructions = vec![Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![ValueId(0); count],
            },
            span: Span::default(),
        }];
        schedule.retained[0] = vec![vec![]];
        let mut b = budget(10_000_000);
        packed_loans(f, &graph, Some(schedule), &mut b).unwrap();
        10_000_000 - b.remaining
    };
    assert!(run(512, &mut f, &mut schedule) > run(1, &mut f, &mut schedule) + 511);
}
#[test]
fn paid_ascending_enumeration_preserves_first_carrier_and_failure_state() {
    for width in [0, 1, 63, 64, 65, 127, 128, 129, 512] {
        let mut row = Bits::new(width);
        for i in (0..width).step_by(3) {
            row.assign(i, true).unwrap();
        }
        let expected: Vec<_> = row.ones().collect();
        let n = row.work() + expected.len();
        for _ in 0..2 {
            let mut b = budget(n);
            let mut cursor = row.paid_ones(&mut b).unwrap();
            let mut actual = vec![];
            while let Some(i) = cursor.next(&mut b).unwrap() {
                actual.push(i);
            }
            assert_eq!(actual, expected);
            assert_eq!(b.remaining, 0);
            if expected.is_empty() {
                assert!(row.paid_ones(&mut budget(n - 1)).is_err());
            } else {
                let mut low = budget(n - 1);
                let mut cursor = row.paid_ones(&mut low).unwrap();
                for &i in &expected[..expected.len() - 1] {
                    assert_eq!(cursor.next(&mut low).unwrap(), Some(i));
                }
                assert_eq!(cursor.next(&mut low).unwrap_err().code, "E-RESOURCE-LIMIT");
                // Failed extraction must not consume the pending bit.
                assert_eq!(
                    cursor.next(&mut budget(1)).unwrap(),
                    expected.last().copied()
                );
            }
        }
        let mut mismatched = Bits::new(width + 1);
        let before = mismatched.clone();
        assert_eq!(
            mismatched
                .join(&row, &mut budget(10_000_000))
                .unwrap_err()
                .code,
            "E-VERIFY-CFG"
        );
        assert_eq!(mismatched, before);
        assert_eq!(row.get(width), None);
        assert_eq!(row.get(usize::MAX), None);
        let values = width / 2;
        assert_eq!(
            row.ones().take_while(|&i| i < values).collect::<Vec<_>>(),
            expected
                .iter()
                .copied()
                .filter(|&i| i < values)
                .collect::<Vec<_>>()
        );
    }
}
