use super::*;

fn live_loans_full_scan(
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
        for payload in &state.locals {
            for (_, permission) in &payload.task_ports {
                loans.add(&permission.loans, b)?;
            }
        }
    }
    // Only an actual slot write can create a new external loan that is not
    // already represented by the original parameter relation.
    if b.stored_slot_updates {
        for holder in &state.external {
            for (_, p) in &holder.leaves {
                loans.add(&p.loans, b)?;
            }
        }
    }
    ancestry(loans.as_slice(), &solution.loans, b)
}
fn permission(ids: &[usize]) -> Permission {
    Permission {
        loans: ids.to_vec(),
        ..Permission::default()
    }
}
fn solution(values: Vec<Payload>, loans: Vec<Loan>) -> Solution {
    Solution {
        values,
        loans,
        entry: Frame::default(),
        outputs: Vec::new(),
        sites: Vec::new(),
        dyn_sites: dyn_leases::Sites::default(),
    }
}
fn budget(steps: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: steps,
        ..Limits::default()
    })
}
#[test]
fn support_words_match_bool_intersection_at_ssa_local_boundaries_and_exact_limits() {
    for width in [0usize, 1, 63, 64, 65, 127, 128, 129] {
        let support = (0..width)
            .map(|id| id.is_multiple_of(3))
            .collect::<Vec<_>>();
        let row = WordRow::from_bools(&support);
        for live_width in [0, width.saturating_sub(1), width, width + 2] {
            let live_flags = (0..live_width)
                .map(|id| !id.is_multiple_of(2))
                .collect::<Vec<_>>();
            let live = WordRow::from_bools(&live_flags);
            for excluded in [
                None,
                Some(ValueId(0)),
                Some(ValueId(63)),
                Some(ValueId(width as u32)),
                Some(ValueId(u32::MAX)),
            ] {
                let expected = (0..width.min(live_width)).find(|&id| {
                    support[id] && live_flags[id] && excluded.is_none_or(|v| v.0 as usize != id)
                });
                assert_eq!(
                    row.first_common(&live, excluded, &mut Budget::new(&Limits::default()))
                        .unwrap(),
                    expected
                );
            }
        }
    }
    let mut flags = vec![false; 129];
    flags[128] = true;
    let row = WordRow::from_bools(&flags);
    let mut measured = Budget::new(&Limits::default());
    assert_eq!(
        row.first_common(&row, None, &mut measured).unwrap(),
        Some(128)
    );
    let n = Limits::default().max_analysis_steps - measured.remaining;
    for steps in [n, n - 1, n] {
        let result = row.first_common(&row, None, &mut budget(steps));
        if steps == n {
            assert_eq!(result.unwrap(), Some(128));
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let mut local_only = vec![false; 66];
    local_only[65] = true;
    let ssa = WordRow::from_bools(&[true; 65]);
    assert_eq!(
        ssa.first_common(
            &WordRow::from_bools(&local_only),
            None,
            &mut Budget::new(&Limits::default())
        )
        .unwrap(),
        None,
        "local65 shares a word but is outside SSA support"
    );
}
#[test]
fn representatives_preserve_complete_fullscan_ancestry_flags_and_excluded_aliases() {
    for invalid in 0..5 {
        let mut values = vec![Payload::default(); 513];
        for id in [0, 1, 63, 64, 65, 128, 512] {
            values[id].address = permission(&[3, 1, 3]);
        }
        let mut setup = Budget::new(&Limits::default());
        values[5].address = permission(&[7]);
        values[5]
            .leaves
            .push(
                (vec![PlaceProjection::Field(0)], permission(&[4, 2, 4])),
                &mut setup,
            )
            .unwrap();
        values[6].task_ports.push((Vec::new(), permission(&[5, 1])));
        values[7].store_description = permission(&[9]);
        values[8].creation_complete = Some(false);
        values[8].address.sources.push(Source {
            retained_port: None,
            base: HoldRoot::Static,
            leaf: Vec::new(),
            inline: Vec::new(),
            heap_parents: Vec::new(),
            heap_element: None,
        });
        if invalid == 1 {
            values[128].address.loans = vec![usize::MAX, 3, usize::MAX];
        }
        let mut state = Frame {
            locals: vec![Payload::default(); 2],
            ..Frame::default()
        };
        state.locals[0]
            .task_ports
            .push((Vec::new(), permission(&[6])));
        state.locals[1].address = permission(&[8]);
        let mut external = Payload::default();
        external
            .leaves
            .push((Vec::new(), permission(&[12])), &mut setup)
            .unwrap();
        state.external.push(external);
        if invalid == 3 {
            state.locals[1].address.loans.push(usize::MAX);
        }
        if invalid == 4 {
            state.external[0]
                .leaves
                .permission_mut(0)
                .loans
                .push(usize::MAX);
        }
        let mut arena = vec![Loan::default(); 16];
        arena[3].parents = vec![0, 2];
        arena[6].parents = vec![10];
        arena[10].parents = vec![11];
        arena[11].parents = vec![10];
        if invalid == 2 {
            arena[3].parents.push(usize::MAX);
        }
        let solution = solution(values, arena);
        let sources = Sources::collect(&solution.values, &mut setup)
            .unwrap()
            .expect("actual sparse support inventory is useful");
        assert_eq!(sources._values[8].creation_complete, Some(false));
        for pattern in 0..4 {
            let mut flags = vec![false; 515];
            for id in match pattern {
                0 => vec![0, 1, 5, 6, 7, 8, 128, 514],
                1 => vec![512, 5, 6, 514],
                2 => vec![514],
                _ => vec![0],
            } {
                flags[id] = true;
            }
            for task in [false, true] {
                for external in [false, true] {
                    for definition in [None, Some(ValueId(0)), Some(ValueId(1)), Some(ValueId(5))] {
                        let mut old_b = Budget::new(&Limits::default());
                        let mut new_b = Budget::new(&Limits::default());
                        old_b.task_borrowed_results = task;
                        new_b.task_borrowed_results = task;
                        old_b.stored_slot_updates = external;
                        new_b.stored_slot_updates = external;
                        let live = WordRow::from_bools(&flags);
                        let selected = sources
                            .select(&live, definition, &mut new_b)
                            .unwrap()
                            .unwrap();
                        assert!(
                            !selected.as_slice().contains(&7),
                            "weak descriptions are not direct loan contributors"
                        );
                        assert!(
                            !selected.as_slice().contains(&8),
                            "source/tag facts remain in values, not erased or treated as permission"
                        );
                        let old = live_loans_full_scan(
                            &solution, &state, &live, definition, None, &mut old_b,
                        );
                        let new = live_loans(
                            &solution,
                            &state,
                            &live,
                            definition,
                            Some(selected.as_slice()),
                            &mut new_b,
                        );
                        assert_eq!(
                            new, old,
                            "invalid={invalid} pattern={pattern} flags={task}/{external} definition={definition:?}"
                        );
                        if invalid == 0 && pattern == 0 && definition == Some(ValueId(0)) {
                            assert!(
                                new.unwrap().contains(&3),
                                "other SSA carries the excluded definition's same loan"
                            );
                        }
                    }
                }
            }
        }
        for width in [0, 512] {
            let live = WordRow::from_bools(&vec![true; width]);
            let mut b = Budget::new(&Limits::default());
            assert!(sources.select(&live, None, &mut b).unwrap().is_none());
            assert_eq!(
                live_loans(&solution, &state, &live, None, None, &mut b),
                live_loans_full_scan(
                    &solution,
                    &state,
                    &live,
                    None,
                    None,
                    &mut Budget::new(&Limits::default())
                )
            );
        }
    }
}
#[test]
fn contribution_inventory_falls_back_and_rebuilds_without_caching_dynamic_loan_metadata() {
    let unique = (0..65)
        .map(|id| Payload {
            address: permission(&[id]),
            ..Payload::default()
        })
        .collect::<Vec<_>>();
    assert!(
        Sources::collect(&unique, &mut Budget::new(&Limits::default()))
            .unwrap()
            .is_none()
    );
    for generation in 0..3 {
        let values = vec![
            Payload {
                address: permission(&[generation, generation]),
                ..Payload::default()
            };
            65
        ];
        let mut solution = solution(values, vec![Loan::default(); 5]);
        let sources = Sources::collect(&solution.values, &mut Budget::new(&Limits::default()))
            .unwrap()
            .unwrap();
        let live = WordRow::from_bools(&[true; 65]);
        for invalid_parent in [false, true, false] {
            solution.loans[generation].parents = if invalid_parent {
                vec![usize::MAX]
            } else {
                vec![4]
            };
            let mut b = Budget::new(&Limits::default());
            let selected = sources
                .select(&live, Some(ValueId(0)), &mut b)
                .unwrap()
                .unwrap();
            assert_eq!(selected.as_slice(), &[1]);
            assert_eq!(
                live_loans(
                    &solution,
                    &Frame::default(),
                    &live,
                    Some(ValueId(0)),
                    Some(selected.as_slice()),
                    &mut b
                ),
                live_loans_full_scan(
                    &solution,
                    &Frame::default(),
                    &live,
                    Some(ValueId(0)),
                    None,
                    &mut Budget::new(&Limits::default())
                )
            );
        }
    }
}
fn alias_solution() -> Solution {
    let mut values = vec![
        Payload {
            address: permission(&[0]),
            ..Payload::default()
        };
        513
    ];
    values[512] = Payload::default();
    let mut setup = Budget::new(&Limits::default());
    for id in 0..512 {
        values[512]
            .leaves
            .push(
                (
                    vec![PlaceProjection::Field(id)],
                    permission(&[id as usize + 1]),
                ),
                &mut setup,
            )
            .unwrap();
    }
    let mut arena = vec![Loan::default(); 513];
    for row in arena.iter_mut().skip(1) {
        row.parents.push(0);
    }
    solution(values, arena)
}
fn indexed_sequence(steps: usize) -> Check<usize> {
    let solution = alias_solution();
    let mut b = budget(steps);
    let sources =
        Sources::collect(&solution.values, &mut b)?.expect("repeated scalar contributions");
    for at in 0..513 {
        let mut flags = vec![false; 513];
        let definition = if at < 512 {
            flags[..=at].fill(true);
            Some(ValueId(at as u32))
        } else {
            flags[512] = true;
            None
        };
        let live = WordRow::from_bools(&flags);
        let selected = sources.select(&live, definition, &mut b)?.unwrap();
        live_loans(
            &solution,
            &Frame::default(),
            &live,
            definition,
            Some(selected.as_slice()),
            &mut b,
        )?;
    }
    Ok(b.remaining)
}
#[test]
fn repeated_actual_contributions_reduce_total_work_and_prepay_atomic_fresh_boundaries() {
    let solution = alias_solution();
    let limit = Limits::default().max_analysis_steps;
    let mut old_b = Budget::new(&Limits::default());
    for at in 0..513 {
        let mut flags = vec![false; 513];
        let definition = if at < 512 {
            flags[..=at].fill(true);
            Some(ValueId(at as u32))
        } else {
            flags[512] = true;
            None
        };
        live_loans_full_scan(
            &solution,
            &Frame::default(),
            &WordRow::from_bools(&flags),
            definition,
            None,
            &mut old_b,
        )
        .unwrap();
    }
    let remaining = indexed_sequence(limit).unwrap();
    let n = limit - remaining;
    assert!(remaining > old_b.remaining);
    println!(
        "513 actual SSA, 512 scalar aliases plus512 future field loans: old={} grouped={n}",
        limit - old_b.remaining
    );
    for steps in [n, n - 1, n] {
        let result = indexed_sequence(steps);
        if steps == n {
            assert_eq!(result.unwrap(), 0);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let mut measured = Budget::new(&Limits::default());
    let sources = Sources::collect(&solution.values, &mut measured)
        .unwrap()
        .unwrap();
    assert_eq!(sources.groups.len(), 1);
    assert_eq!(sources.ordinary, [512]);
    let build_n = limit - measured.remaining;
    for steps in [0, build_n - 1, build_n, build_n] {
        let result = Sources::collect(&solution.values, &mut budget(steps));
        if steps == build_n {
            assert!(result.unwrap().is_some());
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let live = WordRow::from_bools(&[true; 513]);
    let mut b = Budget::new(&Limits::default());
    sources
        .select(&live, Some(ValueId(0)), &mut b)
        .unwrap()
        .unwrap();
    let select_n = limit - b.remaining;
    let before = format!("{sources:?}");
    for steps in [select_n, select_n - 1, select_n] {
        let result = sources.select(&live, Some(ValueId(0)), &mut budget(steps));
        if steps == select_n {
            assert_eq!(result.unwrap().unwrap().as_slice(), &[1, 512]);
        } else {
            assert_eq!(
                result.err().unwrap().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
        assert_eq!(
            format!("{sources:?}"),
            before,
            "failed selection never mutates or publishes a partial index"
        );
    }
}
