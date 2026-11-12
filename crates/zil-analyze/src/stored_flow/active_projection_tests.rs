use super::*;

fn permission(id: usize) -> Permission {
    Permission {
        targets: BTreeSet::from([Target {
            retained_port: None,
            base: HoldRoot::Parameter(id as u32),
            stored: vec![PlaceProjection::Field(511); 8],
            place: vec![PlaceProjection::Field(3); 8],
            heap_parents: Vec::new(),
            heap_element: None,
        }]),
        loans: Loans::from([id]),
    }
}
fn budget(tasks: bool, external: bool) -> Budget {
    let mut b = Budget::new(&Limits::default());
    b.task_borrowed_results = tasks;
    b.stored_slot_updates = external;
    b
}
#[test]
fn loan_only_projection_matches_original_full_scan_and_dynamic_frame_obligations() {
    let mut values = vec![Value::default(); 6];
    values[0].pointer = permission(0);
    values[1].pointer = permission(0);
    values[2].held = permission(1);
    values[3]
        .slots
        .insert(vec![PlaceProjection::Field(0)], permission(2));
    values[4].task_ports.insert(vec![], permission(3));
    // This target-only payload contributes no loan, but remains a real fact.
    values[5].pointer = permission(7);
    values[5].pointer.loans.clear();
    let mut state = State::default();
    state.locals.insert(
        LocalId(0),
        Value {
            pointer: permission(4),
            task_ports: BTreeMap::from([(vec![], permission(5))]),
            ..Value::default()
        },
    );
    state.locals.insert(
        LocalId(1),
        Value {
            held: permission(6),
            ..Value::default()
        },
    );
    state.captures.insert(
        0,
        Value {
            pointer: permission(7),
            slots: BTreeMap::from([(vec![], permission(8))]),
            ..Value::default()
        },
    );
    state.external.insert(
        HoldRoot::Parameter(0),
        BTreeMap::from([(vec![], permission(9))]),
    );
    let before = (values.clone(), state.clone());
    let mut loans = vec![Loan::default(); 10];
    loans[2].parents.insert(1);
    loans[7].parents.insert(8);
    loans[8].parents.insert(7);
    for tasks in [false, true] {
        for external in [false, true] {
            for mask in 0..256 {
                let flags = (0..8).map(|i| mask & (1 << i) != 0).collect::<Vec<_>>();
                let expected = active_by_permission(
                    &values,
                    &state,
                    &flags,
                    &loans,
                    &mut budget(tasks, external),
                );
                let actual = active(
                    &values,
                    &state,
                    &LiveRow::from_flags(&flags),
                    &loans,
                    &mut budget(tasks, external),
                );
                assert_eq!(
                    actual, expected,
                    "tasks={tasks} external={external} mask={mask}"
                );
                let actual = actual.unwrap();
                if !flags[0] && flags[1] {
                    assert!(
                        actual.contains(&0),
                        "another live SSA retains the excluded producer's loan"
                    );
                }
                if tasks {
                    assert!(
                        actual.contains(&5),
                        "dead-local retained ports are still frame obligations"
                    );
                }
                assert_eq!(actual.contains(&9), external);
            }
        }
    }
    assert_eq!(
        (values, state),
        before,
        "temporary loan projection never changes real permission targets"
    );
}
#[test]
fn loan_only_projection_preserves_invalid_ancestry_and_pays_a_fresh_exact_boundary() {
    let state = State::default();
    let flags = [true];
    let row = LiveRow::from_flags(&flags);
    for id in [7, usize::MAX] {
        let mut value = Value::default();
        value.pointer.loans.insert(id);
        assert_eq!(
            active(
                &[value.clone()],
                &state,
                &row,
                &[],
                &mut budget(false, false)
            ),
            active_by_permission(&[value], &state, &flags, &[], &mut budget(false, false))
        );
    }
    let mut value = Value::default();
    for id in 0..512 {
        value.slots.insert(
            vec![PlaceProjection::Field(id)],
            permission(id as usize % 4),
        );
    }
    let values = [value];
    let mut loans = vec![Loan::default(); 4];
    loans[0].parents.insert(1);
    let mut old = budget(false, false);
    let expected = active_by_permission(&values, &state, &flags, &loans, &mut old).unwrap();
    let mut new = budget(false, false);
    assert_eq!(
        active(&values, &state, &row, &loans, &mut new).unwrap(),
        expected
    );
    assert!(
        new.remaining() > old.remaining(),
        "the temporary accumulator no longer clones and searches unused targets"
    );
    let n = Limits::default().max_analysis_steps - new.remaining();
    for steps in [n, n - 1, n] {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        });
        let actual = active(&values, &state, &row, &loans, &mut b);
        if steps == n {
            assert_eq!(actual.unwrap(), expected);
            assert_eq!(b.remaining(), 0);
        } else {
            assert!(
                actual
                    .unwrap_err()
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }
    loans[1].parents.insert(usize::MAX);
    assert_eq!(
        active(&values, &state, &row, &loans, &mut budget(false, false)),
        active_by_permission(&values, &state, &flags, &loans, &mut budget(false, false))
    );
}

fn empty_projection_fixture(mode: usize, count: usize) -> (Vec<Value>, State, Vec<bool>) {
    let mut state = State::default();
    let mut values = Vec::new();
    match mode {
        0 | 1 => {
            let mut value = Value::default();
            for id in 0..count {
                let mut row = if mode == 1 {
                    permission(id)
                } else {
                    Permission::default()
                };
                row.loans.clear();
                value
                    .slots
                    .insert(vec![PlaceProjection::Field(id as u32)], row);
            }
            values.push(value);
        }
        2 | 5 => {
            for id in 0..count {
                let mut value = Value::default();
                if mode == 5 {
                    value.task_ports.insert(Vec::new(), Permission::default());
                }
                state.locals.insert(LocalId(id as u32), value);
            }
        }
        3 => {
            for id in 0..count {
                state
                    .external
                    .insert(HoldRoot::Local(LocalId(id as u32)), BTreeMap::new());
            }
        }
        4 => {
            for id in 0..count {
                state.captures.insert(id as u32, Value::default());
            }
        }
        6 => {
            let rows = (0..count)
                .map(|id| {
                    (
                        vec![PlaceProjection::Field(id as u32)],
                        Permission::default(),
                    )
                })
                .collect();
            state.external.insert(HoldRoot::Parameter(0), rows);
        }
        _ => unreachable!(),
    }
    let mut flags = vec![true; values.len()];
    flags.resize(values.len() + state.locals.len(), false);
    (values, state, flags)
}
#[test]
fn every_empty_permission_and_empty_outer_frame_visit_consumes_real_fresh_work() {
    for mode in 0..7 {
        let mut previous = None;
        for count in [0, 1, 2, 8, 32, 128, 512] {
            let (values, state, flags) = empty_projection_fixture(mode, count);
            let before = (values.clone(), state.clone());
            let live = LiveRow::from_flags(&flags);
            let expected =
                active_by_permission(&values, &state, &flags, &[], &mut budget(true, true))
                    .unwrap();
            assert!(expected.is_empty());
            let mut b = budget(true, true);
            assert_eq!(
                active(&values, &state, &live, &[], &mut b).unwrap(),
                expected
            );
            let actual_work = Limits::default().max_analysis_steps - b.remaining();
            // Every packed word/container and actual set bit is visited.
            // Permission projections and empty frame entries retain their
            // complete outer visits; unused target payloads are not read.
            let scan_work =
                flags.len().div_ceil(64) + 1 + flags.iter().filter(|&&live| live).count();
            let projection_work = match mode {
                0 | 1 => count + 2, // pointer, held and each slot Permission
                2 => count,         // outer local visit, including empty task ports
                3 | 4 => count,
                5 => count * 2, // outer local and retained Permission
                6 => count + 1,
                _ => unreachable!(),
            };
            let declared_work = scan_work + projection_work;
            assert_eq!(actual_work, declared_work, "mode={mode} rows={count}");
            if let Some(old) = previous {
                assert!(
                    actual_work > old,
                    "additional real rows cannot be a free scan"
                );
            }
            previous = Some(actual_work);
            if actual_work > 0 {
                for steps in [actual_work, actual_work - 1, actual_work] {
                    let mut b = Budget::new(&Limits {
                        max_analysis_steps: steps,
                        ..Limits::default()
                    });
                    b.task_borrowed_results = true;
                    b.stored_slot_updates = true;
                    let result = active(&values, &state, &live, &[], &mut b);
                    if steps == actual_work {
                        assert_eq!(result.unwrap(), expected);
                        assert_eq!(b.remaining(), 0);
                    } else {
                        assert!(
                            result
                                .unwrap_err()
                                .iter()
                                .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
                        );
                    }
                    assert_eq!((&values, &state), (&before.0, &before.1));
                }
            }
        }
    }
}
