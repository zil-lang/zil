use super::*;

fn permission(id: usize) -> Permission {
    Permission {
        loans: BTreeSet::from([id]),
        ..Permission::default()
    }
}

#[test]
fn current_ssa_loan_index_equals_full_live_scan_with_nested_rows_and_aliases() {
    let mut values = vec![Value::default(); 9];
    values[0].pointer = permission(0);
    values[1].held = permission(1);
    values[2]
        .slots
        .insert(vec![PlaceProjection::Field(0)], permission(2));
    values[3].task_ports.insert(Vec::new(), permission(3));
    values[4].store_description = permission(4);
    values[5].environment.insert(Vec::new(), permission(5));
    values[6].store_description.targets.insert(Target {
        retained_port: None,
        base: HoldRoot::Local(LocalId(99)),
        stored: Vec::new(),
        place: Vec::new(),
        heap_element: None,
    });
    values[7].pointer = permission(0);
    let mut state = State::default();
    state.locals.insert(
        LocalId(0),
        Value {
            pointer: permission(6),
            ..Value::default()
        },
    );
    state.locals.insert(LocalId(1), Value::default());
    state.captures.insert(
        0,
        Value {
            pointer: permission(7),
            ..Value::default()
        },
    );
    state.external.insert(
        HoldRoot::Parameter(0),
        BTreeMap::from([(Vec::new(), permission(4))]),
    );
    let mut loans = vec![Loan::default(); 8];
    loans[2].parents.insert(1);
    loans[6].parents.insert(5);
    for task_ports in [false, true] {
        let mut b = Budget::new(&Limits::default());
        b.task_borrowed_results = task_ports;
        b.stored_slot_updates = true;
        let ids = loan_value_ids(&values, &mut b).unwrap();
        assert_eq!(ids, [0, 1, 2, 3, 4, 5, 6, 7]);
        for excluded in [None, Some(ValueId(0)), Some(ValueId(2)), Some(ValueId(7))] {
            for inactive in [None, Some(2), Some(3)] {
                let mut live = vec![true; values.len() + 2];
                if let Some(id) = inactive {
                    live[id] = false;
                }
                let mut reference = live.clone();
                if let Some(id) = excluded {
                    reference[id.0 as usize] = false;
                }
                let expected =
                    active(&values, &state, &reference, None, None, &loans, &mut b).unwrap();
                let indexed =
                    active(&values, &state, &live, excluded, Some(&ids), &loans, &mut b).unwrap();
                assert_eq!(indexed, expected);
                if inactive != Some(3) && excluded == Some(ValueId(0)) {
                    assert!(
                        indexed.contains(&0),
                        "an alias still carries the excluded producer's loan"
                    );
                }
            }
        }
    }
}

#[test]
fn current_ssa_index_rebuilds_for_each_latest_solution() {
    let state = State::default();
    let loans = vec![Loan::default(); 4];
    let mut b = Budget::new(&Limits::default());
    for generation in 0..4 {
        let mut values = vec![Value::default(); 2];
        values[generation % 2]
            .slots
            .insert(Vec::new(), permission(generation));
        let ids = loan_value_ids(&values, &mut b).unwrap();
        assert_eq!(ids, [generation % 2]);
        let indexed = active(
            &values,
            &state,
            &[true, true],
            None,
            Some(&ids),
            &loans,
            &mut b,
        )
        .unwrap();
        let reference = active(&values, &state, &[true, true], None, None, &loans, &mut b).unwrap();
        assert_eq!(indexed, reference);
        assert_eq!(indexed, BTreeSet::from([generation]));
    }
}
