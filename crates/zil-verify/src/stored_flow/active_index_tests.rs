use super::*;

fn permission(id: usize) -> Permission {
    Permission {
        loans: vec![id],
        ..Permission::default()
    }
}

#[test]
fn current_ssa_loan_index_equals_full_live_scan_with_nested_rows_and_aliases() {
    let mut b = Budget::new(&Limits::default());
    let mut values = vec![Payload::default(); 7];
    values[0].address = permission(0);
    values[1]
        .leaves
        .push((vec![PlaceProjection::Field(0)], permission(1)), &mut b)
        .unwrap();
    values[2].task_ports.push((Vec::new(), permission(2)));
    values[3].store_description = permission(3);
    values[4].store_description.sources.push(Source {
        retained_port: None,
        base: HoldRoot::Local(LocalId(99)),
        leaf: Vec::new(),
        inline: Vec::new(),
        heap_element: None,
    });
    values[5].address = permission(0);
    let mut state = Frame {
        locals: vec![Payload::default(); 2],
        ..Frame::default()
    };
    state.locals[0].address = permission(4);
    let mut external = Payload::default();
    external
        .leaves
        .push((Vec::new(), permission(3)), &mut b)
        .unwrap();
    state.external.push(external);
    let mut loans = vec![Loan::default(); 5];
    loans[1].parents.push(0);
    loans[4].parents.push(3);
    let solution = Solution {
        values,
        outputs: Vec::new(),
        entry: Frame::default(),
        loans,
        sites: Vec::new(),
    };
    for task_ports in [false, true] {
        b.task_borrowed_results = task_ports;
        b.stored_slot_updates = true;
        let ids = contributing_values(&solution.values, &mut b).unwrap();
        assert_eq!(ids, [0, 1, 2, 3, 4, 5]);
        for excluded in [None, Some(ValueId(0)), Some(ValueId(1)), Some(ValueId(5))] {
            for inactive in [None, Some(1), Some(2)] {
                let mut live = vec![true; solution.values.len() + 2];
                if let Some(id) = inactive {
                    live[id] = false;
                }
                let mut reference = live.clone();
                if let Some(id) = excluded {
                    reference[id.0 as usize] = false;
                }
                let expected =
                    live_loans(&solution, &state, &reference, None, None, &mut b).unwrap();
                let indexed =
                    live_loans(&solution, &state, &live, excluded, Some(&ids), &mut b).unwrap();
                assert_eq!(indexed, expected);
                if excluded == Some(ValueId(0)) {
                    assert!(indexed.contains(&0));
                }
            }
        }
    }
}

#[test]
fn current_ssa_index_rebuilds_for_each_latest_solution() {
    let mut b = Budget::new(&Limits::default());
    for generation in 0..4 {
        let mut values = vec![Payload::default(); 2];
        values[generation % 2]
            .leaves
            .push((Vec::new(), permission(generation)), &mut b)
            .unwrap();
        let solution = Solution {
            values,
            outputs: Vec::new(),
            entry: Frame::default(),
            loans: vec![Loan::default(); 4],
            sites: Vec::new(),
        };
        let ids = contributing_values(&solution.values, &mut b).unwrap();
        assert_eq!(ids, [generation % 2]);
        let indexed = live_loans(
            &solution,
            &Frame::default(),
            &[true, true],
            None,
            Some(&ids),
            &mut b,
        )
        .unwrap();
        let reference = live_loans(
            &solution,
            &Frame::default(),
            &[true, true],
            None,
            None,
            &mut b,
        )
        .unwrap();
        assert_eq!(indexed, reference);
        assert_eq!(indexed, [generation]);
    }
}
