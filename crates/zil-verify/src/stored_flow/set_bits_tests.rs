use super::*;

fn old_full_scan(
    solution: &Solution,
    state: &Frame,
    live: &WordRow,
    definition: Option<ValueId>,
    indexed: Option<&[usize]>,
    b: &mut Budget,
) -> Check<Vec<usize>> {
    b.spend(indexed.map_or(live.len(), |ids| {
        ids.len().saturating_add(live.len() - solution.values.len())
    }))?;
    let mut loans = live_loan_ids::SortedLoanIds::default();
    let mut collect = |id: usize| -> Check<()> {
        if !live.is_live(id)? || definition.is_some_and(|value| value.0 as usize == id) {
            return Ok(());
        }
        let payload = if id < solution.values.len() {
            &solution.values[id]
        } else {
            &state.locals[id - solution.values.len()]
        };
        loans.add(&payload.address.loans, b)?;
        if b.task_borrowed_results {
            for (_, p) in &payload.task_ports {
                loans.add(&p.loans, b)?;
            }
        }
        for (_, p) in &payload.leaves {
            loans.add(&p.loans, b)?;
        }
        Ok(())
    };
    if let Some(ids) = indexed {
        for &id in ids {
            collect(id)?;
        }
        for id in solution.values.len()..live.len() {
            collect(id)?;
        }
    } else {
        for id in 0..live.len() {
            collect(id)?;
        }
    }
    if b.task_borrowed_results {
        // The actual frame retains its inputs until its owning carrier moves
        // or is discharged; dead SSA uses of a head do not discharge a frame.
        b.spend(state.locals.len())?;
        for payload in &state.locals {
            for (_, permission) in &payload.task_ports {
                loans.add(&permission.loans, b)?;
            }
        }
    }
    // Only an actual slot write can create a new external loan that is not
    // already represented by the original parameter relation.
    if b.stored_slot_updates {
        b.spend(state.external.len())?;
        for holder in &state.external {
            for (_, p) in &holder.leaves {
                loans.add(&p.loans, b)?;
            }
        }
    }
    ancestry(loans.as_slice(), &solution.loans, b)
}

fn budget(tasks: bool, external: bool, steps: usize) -> Budget {
    let mut b = Budget::new(&Limits {
        max_analysis_steps: steps,
        ..Limits::default()
    });
    b.task_borrowed_results = tasks;
    b.stored_slot_updates = external;
    b
}
fn permission(ids: &[usize]) -> Permission {
    Permission {
        loans: ids.to_vec(),
        ..Permission::default()
    }
}
fn fixture(ssa: usize, locals: usize, generation: usize) -> (Solution, Frame) {
    let mut values = vec![Payload::default(); ssa];
    let mut b = budget(false, false, Limits::default().max_analysis_steps);
    if let Some(value) = values.first_mut() {
        value.address = permission(if generation == 2 {
            &[usize::MAX]
        } else {
            &[2, 1, 2]
        });
        value
            .leaves
            .push((vec![], permission(&[3, 3])), &mut b)
            .unwrap();
        value.task_ports.push((vec![], permission(&[4])));
    }
    if ssa > 1 {
        values[ssa - 1].address = permission(&[2, 1]);
    }
    let mut frame = Frame {
        locals: vec![Payload::default(); locals],
        external: vec![Payload::default(); 2],
        ..Frame::default()
    };
    if let Some(value) = frame.locals.first_mut() {
        value.address = permission(&[5]);
        value.task_ports.push((vec![], permission(&[6])));
        value
            .leaves
            .push((vec![], permission(&[7])), &mut b)
            .unwrap();
    }
    frame.external[0]
        .leaves
        .push((vec![], permission(&[8])), &mut b)
        .unwrap();
    let mut loans = vec![Loan::default(); 9];
    loans[1].parents = vec![if generation == 1 { usize::MAX } else { 2 }];
    loans[2].parents = vec![1];
    (
        Solution {
            values,
            loans,
            entry: Frame::default(),
            outputs: vec![],
            sites: vec![],
            dyn_sites: dyn_leases::Sites::default(),
        },
        frame,
    )
}
#[test]
fn set_bits_match_frozen_none_and_preserve_indexed_costs_and_errors() {
    let limit = Limits::default().max_analysis_steps;
    for (ssa, locals) in [
        (0, 0),
        (0, 65),
        (1, 0),
        (31, 34),
        (63, 2),
        (64, 1),
        (65, 0),
        (369, 56),
    ] {
        for generation in 0..3 {
            let (solution, state) = fixture(ssa, locals, generation);
            for tasks in [false, true] {
                for external in [false, true] {
                    for pattern in 0..3 {
                        let flags = (0..ssa + locals)
                            .map(|id| match pattern {
                                0 => false,
                                1 => id % 7 == 0 || id == ssa || id + 1 == ssa + locals,
                                _ => true,
                            })
                            .collect::<Vec<_>>();
                        let live = WordRow::from_bools(&flags);
                        let ids = (0..ssa).collect::<Vec<_>>();
                        for definition in [None, Some(ValueId(0)), Some(ValueId(u32::MAX))] {
                            for indexed in [None, Some(ids.as_slice())] {
                                let mut old = budget(tasks, external, limit);
                                let expected = old_full_scan(
                                    &solution, &state, &live, definition, indexed, &mut old,
                                );
                                let mut new = budget(tasks, external, limit);
                                let actual = live_loans(
                                    &solution, &state, &live, definition, indexed, &mut new,
                                );
                                assert_eq!(
                                    actual, expected,
                                    "ssa={ssa} locals={locals} generation={generation} pattern={pattern} tasks={tasks} external={external} definition={definition:?}"
                                );
                                if indexed.is_some() {
                                    assert_eq!(
                                        new.remaining, old.remaining,
                                        "indexed work unchanged"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Short logical rows must preserve the existing None-lane behavior, not
    // import a new assumption that every SSA/local is represented in the row.
    let (solution, state) = fixture(65, 1, 0);
    for width in [0, 1, 63] {
        let row = WordRow::from_bools(&vec![false; width]);
        assert_eq!(
            live_loans(
                &solution,
                &state,
                &row,
                None,
                None,
                &mut budget(true, true, limit)
            ),
            old_full_scan(
                &solution,
                &state,
                &row,
                None,
                None,
                &mut budget(true, true, limit)
            )
        );
    }
}
#[test]
fn set_bits_pay_word_scan_and_live_visits_with_exact_fresh_limits() {
    let limit = Limits::default().max_analysis_steps;
    let solution = Solution {
        values: vec![Payload::default(); 369],
        loans: vec![],
        entry: Frame::default(),
        outputs: vec![],
        sites: vec![],
        dyn_sites: dyn_leases::Sites::default(),
    };
    let state = Frame {
        locals: vec![Payload::default(); 56],
        ..Frame::default()
    };
    for selected in [vec![], vec![0, 63, 64, 368, 369, 424], (0..425).collect()] {
        let mut flags = vec![false; 425];
        for &id in &selected {
            flags[id] = true;
        }
        let row = WordRow::from_bools(&flags);
        let mut old = budget(false, false, limit);
        let expected = old_full_scan(&solution, &state, &row, None, None, &mut old).unwrap();
        let mut new = budget(false, false, limit);
        assert_eq!(
            live_loans(&solution, &state, &row, None, None, &mut new).unwrap(),
            expected
        );
        let before = limit - old.remaining;
        let n = limit - new.remaining;
        assert_eq!(n + row.len(), before + row.work() + selected.len());
        if selected.len() < 100 {
            assert!(n < before, "actual sparse scan must do less work");
        }
        for steps in [n, n - 1, n] {
            let mut b = budget(false, false, steps);
            let result = live_loans(&solution, &state, &row, None, None, &mut b);
            if steps == n {
                assert_eq!(result.unwrap(), expected);
                assert_eq!(b.remaining, 0);
            } else {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }
}
