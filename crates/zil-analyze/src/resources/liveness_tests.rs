use super::*;
use crate::place_replacements::fixture;
use zil_base::ResultCategory;
fn instruction(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    fixture::instruction(result, kind)
}
fn fixture_function() -> (TypedFunction, Shapes, Vec<bool>) {
    let mut f = fixture::module().functions.remove(0);
    let Type::Managed(payload) = f.locals[1].ty.clone() else {
        unreachable!()
    };
    let desc = Type::Desc(payload);
    f.locals[0].ty = desc.clone();
    f.locals[1].ty = desc.clone();
    f.values = vec![
        desc.clone(),
        desc.clone(),
        Type::I32,
        Type::Bool,
        Type::I32,
        desc,
        Type::I32,
    ];
    let body = |id, instructions, terminator| BasicBlock {
        id: BlockId(id),
        instructions,
        terminator,
    };
    f.blocks = vec![
        body(
            0,
            vec![
                instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
                instruction(
                    Some(ValueId(3)),
                    InstructionKind::Constant(Constant::Bool(true)),
                ),
            ],
            Terminator::Branch {
                condition: ValueId(3),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        ),
        body(
            1,
            vec![instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(0),
                },
            )],
            Terminator::Jump(BlockId(3)),
        ),
        body(
            2,
            vec![
                instruction(Some(ValueId(1)), InstructionKind::Load(LocalId(1))),
                instruction(
                    Some(ValueId(2)),
                    InstructionKind::ReadDesc {
                        desc: ValueId(1),
                        index: 0,
                    },
                ),
            ],
            Terminator::Jump(BlockId(3)),
        ),
        body(
            3,
            vec![instruction(
                Some(ValueId(4)),
                InstructionKind::ReadDesc {
                    desc: ValueId(0),
                    index: 0,
                },
            )],
            Terminator::Branch {
                condition: ValueId(3),
                then_block: BlockId(1),
                else_block: BlockId(4),
            },
        ),
        body(
            4,
            vec![instruction(None, InstructionKind::StorageDead(LocalId(0)))],
            Terminator::Return(None),
        ),
        body(
            5,
            vec![
                instruction(Some(ValueId(5)), InstructionKind::Load(LocalId(1))),
                instruction(
                    Some(ValueId(6)),
                    InstructionKind::ReadDesc {
                        desc: ValueId(5),
                        index: 0,
                    },
                ),
            ],
            Terminator::Return(None),
        ),
    ];
    let mut shapes = Shapes::new();
    for place in [
        Place::Local(LocalId(0)),
        Place::Local(LocalId(1)),
        Place::Value(ValueId(0)),
        Place::Value(ValueId(1)),
        Place::Value(ValueId(5)),
    ] {
        shapes.insert(
            place,
            Shape {
                owned: false,
                description: true,
            },
        );
    }
    (f, shapes, vec![true, true, true, true, true, false])
}
type Snapshot = (Vec<Live>, Vec<Live>, Vec<Vec<Live>>);
fn snapshot(
    f: &TypedFunction,
    reachable: &[bool],
    shapes: &Shapes,
    reserved: Option<&[Vec<Vec<ValueId>>]>,
    frontier: bool,
) -> CompileResult<Snapshot> {
    let mut budget = Budget::new(&Limits::default());
    let (entry, exit) = if frontier {
        liveness_frontier(f, reachable, shapes, reserved, &mut budget)?
    } else {
        liveness_full_scan(f, reachable, shapes, reserved, &mut budget)?
    };
    let after = liveness_history(f, shapes, reserved, exit.clone(), &mut budget)?;
    Ok((entry, exit, after))
}
fn compare(
    f: &TypedFunction,
    reachable: &[bool],
    shapes: &Shapes,
    reserved: Option<&[Vec<Vec<ValueId>>]>,
) -> Snapshot {
    let fast = snapshot(f, reachable, shapes, reserved, true).unwrap();
    let full = snapshot(f, reachable, shapes, reserved, false).unwrap();
    assert_eq!(fast, full);
    fast
}
#[test]
fn resource_liveness_frontier_preserves_loop_entries_exits_reserved_and_unreachable_history() {
    let (f, shapes, reachable) = fixture_function();
    let normal = compare(&f, &reachable, &shapes, None);
    assert!(normal.0[5].is_empty() && normal.1[5].is_empty());
    assert!(
        !normal.2[5][0].is_empty(),
        "unreachable history still follows the original reverse walk"
    );
    let mut reserved: Vec<_> = f
        .blocks
        .iter()
        .map(|b| vec![Vec::new(); b.instructions.len()])
        .collect();
    reserved[0][1] = vec![ValueId(0)];
    reserved[2][1] = vec![ValueId(0)];
    compare(&f, &reachable, &shapes, Some(&reserved));
    for order in [[5usize, 3, 1, 4, 2, 0], [0, 2, 4, 1, 3, 5]] {
        let mut inverse = [0usize; 6];
        for (new, old) in order.iter().copied().enumerate() {
            inverse[old] = new;
        }
        let mut p = f.clone();
        p.blocks = order.iter().map(|&old| f.blocks[old].clone()).collect();
        p.entry = BlockId(inverse[f.entry.0 as usize] as u32);
        for (new, b) in p.blocks.iter_mut().enumerate() {
            b.id = BlockId(new as u32);
            match &mut b.terminator {
                Terminator::Jump(target) => *target = BlockId(inverse[target.0 as usize] as u32),
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    *then_block = BlockId(inverse[then_block.0 as usize] as u32);
                    *else_block = BlockId(inverse[else_block.0 as usize] as u32);
                }
                _ => {}
            }
        }
        let reach: Vec<_> = order.iter().map(|&old| reachable[old]).collect();
        let pins: Vec<_> = order.iter().map(|&old| reserved[old].clone()).collect();
        let found = compare(&p, &reach, &shapes, Some(&pins));
        let expected = snapshot(&f, &reachable, &shapes, Some(&reserved), false).unwrap();
        for (new, old) in order.iter().copied().enumerate() {
            assert_eq!(found.0[new], expected.0[old]);
            assert_eq!(found.1[new], expected.1[old]);
            assert_eq!(found.2[new], expected.2[old]);
        }
    }
}
#[test]
fn resource_liveness_publishes_changed_exits_even_when_a_definition_kills_entry_changes() {
    let (mut f, shapes, _) = fixture_function();
    f.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![instruction(
                Some(ValueId(2)),
                InstructionKind::ReadDesc {
                    desc: ValueId(0),
                    index: 0,
                },
            )],
            terminator: Terminator::Return(None),
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![instruction(
                Some(ValueId(0)),
                InstructionKind::Load(LocalId(0)),
            )],
            terminator: Terminator::Jump(BlockId(0)),
        },
    ];
    f.entry = BlockId(1);
    let result = compare(&f, &[true, true], &shapes, None);
    assert_eq!(result.0[1], Live::from([Place::Local(LocalId(0))]));
    assert_eq!(result.1[1], Live::from([Place::Value(ValueId(0))]));
    assert_eq!(result.2[1][0], result.1[1]);
}
#[test]
fn resource_liveness_keeps_reserved_only_uses_request_local_and_single_block_fees_unchanged() {
    let (mut f, shapes, _) = fixture_function();
    f.blocks.truncate(1);
    f.blocks[0].terminator = Terminator::Return(None);
    f.entry = BlockId(0);
    let pins = vec![vec![Vec::new(), vec![ValueId(0)]]];
    let normal = compare(&f, &[true], &shapes, None);
    let pinned = compare(&f, &[true], &shapes, Some(&pins));
    assert_ne!(normal, pinned);
    assert!(normal.2[0][0].is_empty());
    assert_eq!(pinned.2[0][0], Live::from([Place::Value(ValueId(0))]));
    assert_eq!(normal, snapshot(&f, &[true], &shapes, None, true).unwrap());
    let run = |production, work| -> CompileResult<()> {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: work,
            ..Limits::default()
        });
        if production {
            afters_reserved(&f, &[true], &shapes, Some(&pins), &mut b)?;
        } else {
            let (_, exit) = liveness_full_scan(&f, &[true], &shapes, Some(&pins), &mut b)?;
            liveness_history(&f, &shapes, Some(&pins), exit, &mut b)?;
        }
        Ok(())
    };
    let minimum = |production| {
        let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if run(production, mid).is_ok() {
                hi = mid
            } else {
                lo = mid + 1
            }
        }
        run(production, lo).unwrap();
        assert!(
            run(production, lo - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        run(production, lo).unwrap();
        lo
    };
    assert_eq!(minimum(true), minimum(false));
}
#[test]
fn resource_liveness_frontier_preserves_actual_invalid_edge_rejection() {
    let (mut f, shapes, reachable) = fixture_function();
    f.blocks[3].terminator = Terminator::Jump(BlockId(99));
    for frontier in [false, true] {
        let errors = snapshot(&f, &reachable, &shapes, None, frontier).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::SafeReject)
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("invalid resource CFG edge")),
            "{errors:?}"
        );
    }
}
