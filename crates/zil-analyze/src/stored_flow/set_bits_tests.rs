use super::*;

fn old_full_scan(
    values: &[Value],
    state: &State,
    live: &LiveRow,
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<Loans> {
    let mut direct = Loans::new();
    fee(b, live.len())?;
    for i in 0..live.len() {
        if !live.at(i)? {
            continue;
        }
        let value = if i < values.len() {
            Some(&values[i])
        } else {
            ordered(b, state.locals.len(), 0)?;
            state.locals.get(&LocalId((i - values.len()) as u32))
        };
        if let Some(value) = value {
            active_loans_union(&mut direct, &value.pointer, b)?;
            active_loans_union(&mut direct, &value.held, b)?;
            if b.task_borrowed_results {
                for permission in value.task_ports.values() {
                    active_loans_union(&mut direct, permission, b)?;
                }
            }
            for leaf in value.slots.values() {
                active_loans_union(&mut direct, leaf, b)?;
            }
        }
    }
    if b.task_borrowed_results {
        // Retained ports are frame obligations, even when this particular
        // borrowed head has no further SSA use in the callee.
        fee(b, state.locals.len())?;
        for value in state.locals.values() {
            for permission in value.task_ports.values() {
                active_loans_union(&mut direct, permission, b)?;
            }
        }
    }
    for value in state.captures.values() {
        active_loans_union(&mut direct, &value.pointer, b)?;
        for leaf in value.slots.values() {
            active_loans_union(&mut direct, leaf, b)?;
        }
    }
    if b.stored_slot_updates {
        // An empty inner map still consumes its actual outer frame visit.
        fee(b, state.external.len())?;
        for slots in state.external.values() {
            for leaf in slots.values() {
                active_loans_union(&mut direct, leaf, b)?;
            }
        }
    }
    ancestry(&direct, loans, b)
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
        loans: ids.iter().copied().collect(),
        ..Permission::default()
    }
}
fn fixture(ssa: usize, locals: usize, generation: usize) -> (Vec<Value>, State, Vec<Loan>) {
    let mut values = vec![Value::default(); ssa];
    if let Some(value) = values.first_mut() {
        value.pointer = permission(if generation == 2 {
            &[usize::MAX]
        } else {
            &[2, 1, 2]
        });
        value.held = permission(&[3]);
        value
            .slots
            .insert(vec![PlaceProjection::Field(0)], permission(&[4]));
        value.task_ports.insert(vec![], permission(&[5]));
    }
    if ssa > 1 {
        values[ssa - 1].pointer = permission(&[2, 1]);
    }
    let mut state = State::default();
    if locals > 0 {
        state.locals.insert(
            LocalId(0),
            Value {
                pointer: permission(&[6]),
                held: permission(&[7]),
                slots: BTreeMap::from([(vec![], permission(&[8]))]),
                task_ports: BTreeMap::from([(vec![], permission(&[9]))]),
                ..Value::default()
            },
        );
    }
    state.captures.insert(
        0,
        Value {
            pointer: permission(&[10]),
            slots: BTreeMap::from([(vec![], permission(&[11]))]),
            // Original capture collection does not read this field.
            held: permission(&[usize::MAX]),
            ..Value::default()
        },
    );
    state.external.insert(
        HoldRoot::Parameter(0),
        BTreeMap::from([(vec![], permission(&[12]))]),
    );
    let mut loans = vec![Loan::default(); 13];
    loans[1]
        .parents
        .insert(if generation == 1 { usize::MAX } else { 2 });
    loans[2].parents.insert(1);
    (values, state, loans)
}
#[test]
fn set_bits_match_frozen_active_on_sparse_coordinates_and_current_frames() {
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
        let width = ssa + locals;
        for generation in 0..3 {
            let (values, state, loans) = fixture(ssa, locals, generation);
            for tasks in [false, true] {
                for external in [false, true] {
                    for pattern in 0..3 {
                        let flags = (0..width)
                            .map(|id| match pattern {
                                0 => false,
                                1 => id % 7 == 0 || id == ssa || id + 1 == width,
                                _ => true,
                            })
                            .collect::<Vec<_>>();
                        let row = LiveRow::from_flags(&flags);
                        assert_eq!(
                            row.ones().collect::<Vec<_>>(),
                            flags
                                .iter()
                                .enumerate()
                                .filter_map(|(id, yes)| yes.then_some(id))
                                .collect::<Vec<_>>()
                        );
                        assert_eq!(
                            active(
                                &values,
                                &state,
                                &row,
                                &loans,
                                &mut budget(tasks, external, limit)
                            ),
                            old_full_scan(
                                &values,
                                &state,
                                &row,
                                &loans,
                                &mut budget(tasks, external, limit)
                            ),
                            "ssa={ssa} locals={locals} generation={generation} pattern={pattern} tasks={tasks} external={external}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn set_bits_pay_words_and_actual_visits_at_exact_fresh_boundaries() {
    let limit = Limits::default().max_analysis_steps;
    let values = vec![Value::default(); 369];
    let state = State {
        locals: (0..56).map(|id| (LocalId(id), Value::default())).collect(),
        ..State::default()
    };
    for selected in [vec![], vec![0, 63, 64, 368, 369, 424], (0..425).collect()] {
        let mut flags = vec![false; 425];
        for &id in &selected {
            flags[id] = true;
        }
        let live = LiveRow::from_flags(&flags);
        let mut old = budget(false, false, limit);
        let expected = old_full_scan(&values, &state, &live, &[], &mut old).unwrap();
        let mut new = budget(false, false, limit);
        assert_eq!(
            active(&values, &state, &live, &[], &mut new).unwrap(),
            expected
        );
        let before = limit - old.remaining();
        let n = limit - new.remaining();
        assert_eq!(n + live.len(), before + live.work() + selected.len());
        if selected.len() < 100 {
            assert!(n < before, "actual sparse scan must do less work");
        }
        for steps in [n, n - 1, n] {
            let mut b = budget(false, false, steps);
            let result = active(&values, &state, &live, &[], &mut b);
            if steps == n {
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
        }
    }
}
