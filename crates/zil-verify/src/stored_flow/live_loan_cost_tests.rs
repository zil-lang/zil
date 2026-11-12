use super::*;
fn budget(tasks: bool, external: bool, steps: usize) -> Budget {
    let mut b = Budget::new(&Limits {
        max_analysis_steps: steps,
        ..Limits::default()
    });
    b.task_borrowed_results = tasks;
    b.stored_slot_updates = external;
    b
}
fn solution(values: Vec<Payload>, loans: Vec<Loan>) -> Solution {
    Solution {
        values,
        loans,
        outputs: vec![],
        entry: Frame::default(),
        sites: vec![],
        dyn_sites: dyn_leases::Sites::default(),
    }
}
fn permission(ids: &[usize]) -> Permission {
    Permission {
        loans: ids.to_vec(),
        ..Permission::default()
    }
}
#[test]
fn full_and_indexed_enumeration_match_the_complete_independent_main_collector() {
    let limit = Limits::default().max_analysis_steps;
    for tasks in [false, true] {
        for external in [false, true] {
            for malformed in 0..3 {
                let mut values = vec![Payload::default(); 128];
                for value in &mut values {
                    value.address = permission(&[2, 1, 2]);
                }
                let mut b = budget(tasks, external, limit);
                values[126]
                    .leaves
                    .push((vec![], permission(&[3])), &mut b)
                    .unwrap();
                values[127].task_ports.push((vec![], permission(&[4])));
                if malformed == 1 {
                    values[0].address.loans = vec![usize::MAX];
                }
                let mut frame = Frame {
                    locals: vec![Payload::default(); 2],
                    external: vec![Payload::default(); 2],
                    ..Frame::default()
                };
                frame.locals[0].task_ports.push((vec![], permission(&[5])));
                frame.external[0]
                    .leaves
                    .push((vec![], permission(&[6])), &mut b)
                    .unwrap();
                let mut loans = vec![Loan::default(); 7];
                loans[1].parents = if malformed == 2 {
                    vec![usize::MAX]
                } else {
                    vec![2]
                };
                loans[2].parents = vec![1];
                let solution = solution(values, loans);
                let before = (
                    solution.values.clone(),
                    frame.clone(),
                    solution.loans.clone(),
                );
                for definition in [None, Some(ValueId(0)), Some(ValueId(127))] {
                    for sparse in [false, true] {
                        let mut flags = vec![!sparse; 130];
                        flags[0] = true;
                        flags[1] = true;
                        flags[128] = false;
                        flags[129] = false;
                        let live = WordRow::from_bools(&flags);
                        let mut original_flags = flags.clone();
                        if let Some(id) = definition {
                            original_flags[id.0 as usize] = false;
                        }
                        let expected = live_loans_main_fullscan(
                            &solution,
                            &frame,
                            &original_flags,
                            &mut budget(tasks, external, limit),
                        );
                        let selected = (0..solution.values.len()).collect::<Vec<_>>();
                        for indexed in [None, Some(selected.as_slice())] {
                            let actual = live_loans(
                                &solution,
                                &frame,
                                &live,
                                definition,
                                indexed,
                                &mut budget(tasks, external, limit),
                            );
                            assert_eq!(
                                actual, expected,
                                "tasks={tasks} external={external} malformed={malformed} definition={definition:?} sparse={sparse}"
                            );
                            if malformed == 0 && definition == Some(ValueId(0)) {
                                assert!(
                                    actual.unwrap().contains(&2),
                                    "the other SSA still retains the excluded producer's contribution"
                                );
                            }
                        }
                    }
                }
                assert_eq!(
                    (&solution.values, &frame, &solution.loans),
                    (&before.0, &before.1, &before.2)
                );
            }
        }
    }
}
fn empty_rows(mode: usize, count: usize) -> (Solution, Frame, Vec<bool>) {
    let mut values = Vec::new();
    let mut frame = Frame::default();
    let mut b = Budget::new(&Limits::default());
    match mode {
        0 | 1 => {
            let mut value = Payload::default();
            for id in 0..count {
                let row = Permission {
                    sources: if mode == 1 {
                        vec![Source {
                            retained_port: None,
                            base: HoldRoot::Parameter(0),
                            leaf: vec![PlaceProjection::Field(id as u32)],
                            inline: vec![],
                            heap_element: None,
                        }]
                    } else {
                        vec![]
                    },
                    loans: vec![],
                };
                value
                    .leaves
                    .push((vec![PlaceProjection::Field(id as u32)], row), &mut b)
                    .unwrap();
            }
            values.push(value);
        }
        2 | 4 => {
            frame.locals = vec![Payload::default(); count];
            if mode == 4 {
                for payload in &mut frame.locals {
                    payload.task_ports.push((vec![], Permission::default()));
                }
            }
        }
        3 => frame.external = vec![Payload::default(); count],
        5 => {
            let mut payload = Payload::default();
            for id in 0..count {
                payload
                    .leaves
                    .push(
                        (
                            vec![PlaceProjection::Field(id as u32)],
                            Permission::default(),
                        ),
                        &mut b,
                    )
                    .unwrap();
            }
            frame.external.push(payload);
        }
        _ => unreachable!(),
    }
    let mut flags = vec![true; values.len()];
    flags.resize(values.len() + frame.locals.len(), false);
    (solution(values, vec![]), frame, flags)
}
#[test]
fn permission_rows_and_empty_outer_visits_are_paid_in_indexed_and_full_lanes() {
    let limit = Limits::default().max_analysis_steps;
    for mode in 0..6 {
        for count in [0, 1, 2, 8, 32, 128, 512] {
            let (solution, frame, flags) = empty_rows(mode, count);
            let live = WordRow::from_bools(&flags);
            let ids = (0..solution.values.len()).collect::<Vec<_>>();
            let expected =
                live_loans_main_fullscan(&solution, &frame, &flags, &mut budget(true, true, limit))
                    .unwrap();
            assert!(expected.is_empty());
            let projection_work = match mode {
                0 | 1 => count + 2,
                2 | 3 => count + 1,
                4 => 2 * count + 1,
                5 => count + 2,
                _ => unreachable!(),
            };
            for indexed in [None, Some(ids.as_slice())] {
                // The indexed lane still visits every selected SSA/local.
                // The full lane now visits words and actual set bits only;
                // all Permission and outer-frame visits remain independent.
                let scan_work = if indexed.is_some() {
                    ids.len() + frame.locals.len()
                } else {
                    flags.len().div_ceil(64) + 1 + flags.iter().filter(|&&live| live).count()
                };
                let declared_work = scan_work + projection_work;
                let mut b = budget(true, true, limit);
                assert_eq!(
                    live_loans(&solution, &frame, &live, None, indexed, &mut b).unwrap(),
                    expected
                );
                let n = limit - b.remaining;
                assert_eq!(
                    n,
                    declared_work,
                    "mode={mode} rows={count} indexed={}",
                    indexed.is_some()
                );
                for steps in [n, n - 1, n] {
                    let result = live_loans(
                        &solution,
                        &frame,
                        &live,
                        None,
                        indexed,
                        &mut budget(true, true, steps),
                    );
                    if steps == n {
                        assert_eq!(result.unwrap(), expected);
                    } else {
                        assert_eq!(
                            result.unwrap_err().category,
                            zil_base::ResultCategory::ResourceLimit
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn actual_representatives_match_the_complete_independent_main_collector() {
    let limit = Limits::default().max_analysis_steps;
    for tasks in [false, true] {
        for external in [false, true] {
            for malformed in 0..3 {
                let mut values = vec![Payload::default(); 256];
                for value in &mut values {
                    value.address = permission(&[2, 1, 2]);
                }
                let mut b = budget(tasks, external, limit);
                values[126]
                    .leaves
                    .push((vec![], permission(&[3])), &mut b)
                    .unwrap();
                values[127].task_ports.push((vec![], permission(&[4])));
                if malformed == 1 {
                    values[0].address.loans = vec![usize::MAX];
                }
                let mut frame = Frame {
                    locals: vec![Payload::default(); 2],
                    external: vec![Payload::default(); 2],
                    ..Frame::default()
                };
                frame.locals[0].task_ports.push((vec![], permission(&[5])));
                frame.external[0]
                    .leaves
                    .push((vec![], permission(&[6])), &mut b)
                    .unwrap();
                let mut loans = vec![Loan::default(); 7];
                loans[1].parents = if malformed == 2 {
                    vec![usize::MAX]
                } else {
                    vec![2]
                };
                loans[2].parents = vec![1];
                let solution = solution(values, loans);
                let before = (
                    solution.values.clone(),
                    frame.clone(),
                    solution.loans.clone(),
                );
                let groups = ssa_loan_groups::Sources::collect(&solution.values, &mut b).unwrap();
                assert!(
                    groups.is_some(),
                    "real repeated complete sequences select the bounded index"
                );
                for definition in [None, Some(ValueId(0)), Some(ValueId(127))] {
                    for sparse in [false, true] {
                        let mut flags = vec![!sparse; 258];
                        flags[0] = true;
                        flags[1] = true;
                        flags[256] = false;
                        flags[257] = false;
                        let live = WordRow::from_bools(&flags);
                        let mut original_flags = flags.clone();
                        if let Some(id) = definition {
                            original_flags[id.0 as usize] = false;
                        }
                        let expected = live_loans_main_fullscan(
                            &solution,
                            &frame,
                            &original_flags,
                            &mut budget(tasks, external, limit),
                        );
                        let selected = groups
                            .as_ref()
                            .unwrap()
                            .select(&live, definition, &mut b)
                            .unwrap()
                            .unwrap();
                        let actual = live_loans(
                            &solution,
                            &frame,
                            &live,
                            definition,
                            Some(selected.as_slice()),
                            &mut budget(tasks, external, limit),
                        );
                        assert_eq!(
                            actual, expected,
                            "tasks={tasks} external={external} malformed={malformed} definition={definition:?} sparse={sparse}"
                        );
                        if malformed == 0 && definition == Some(ValueId(0)) {
                            assert!(
                                actual.unwrap().contains(&2),
                                "the other SSA still retains the excluded producer's contribution"
                            );
                        }
                    }
                }
                assert_eq!(
                    (&solution.values, &frame, &solution.loans),
                    (&before.0, &before.1, &before.2)
                );
            }
        }
    }
}
