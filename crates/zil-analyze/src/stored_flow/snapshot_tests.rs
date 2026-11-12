use super::*;
fn budget(tasks: bool) -> Budget {
    let mut b = Budget::new(&Limits::default());
    b.task_borrowed_results = tasks;
    b
}
fn payload() -> Permission {
    Permission {
        targets: BTreeSet::from([
            Target {
                retained_port: None,
                base: HoldRoot::Local(LocalId(9)),
                stored: vec![PlaceProjection::Field(511); 8],
                place: vec![PlaceProjection::Field(2); 8],
                heap_parents: Vec::new(),
                heap_element: None,
            },
            Target {
                retained_port: Some(PortOrigin::Task(3)),
                base: HoldRoot::Parameter(0),
                stored: vec![PlaceProjection::ErrorArm(false)],
                place: vec![],
                heap_parents: Vec::new(),
                heap_element: Some(vec![PlaceProjection::Field(4); 8]),
            },
        ]),
        loans: BTreeSet::from([0, 2, 9]),
    }
}
#[test]
fn linear_snapshot_equals_original_union_for_every_fact_and_projection() {
    let p = payload();
    for tasks in [false, true] {
        for complete in [None, Some(false), Some(true)] {
            let mut value = Value {
                pointer: p.clone(),
                store_description: p.clone(),
                held: p.clone(),
                creation_complete: complete,
                ..Value::default()
            };
            value.enum_variants = enum_address::Variants::runtime_poll(&mut budget(tasks)).unwrap();
            value.option_variants =
                option_address::Variants::parameter(3, &mut budget(tasks)).unwrap();
            value
                .option_variants
                .join(
                    &option_address::Variants::parameter(1, &mut budget(tasks)).unwrap(),
                    &mut budget(tasks),
                )
                .unwrap();
            value
                .slots
                .insert(vec![PlaceProjection::Field(511)], p.clone());
            value
                .slots
                .insert(vec![PlaceProjection::Field(2)], Permission::default());
            value.environment.insert(vec![], p.clone());
            value
                .environment
                .insert(vec![PlaceProjection::Field(7)], Permission::default());
            value
                .task_ports
                .insert(vec![zil_ir::ContractProjection::RetainedPort(3)], p.clone());
            let before = value.clone();
            let expected = snapshot_by_union(&value, &mut budget(tasks)).unwrap();
            let actual = snapshot(&value, &mut budget(tasks)).unwrap();
            assert_eq!(
                actual, expected,
                "all fact fields including variants, empty keyed rows and retained projection"
            );
            assert_eq!(value, before, "snapshot cannot mutate the current solution");
            let mut changed = actual.clone();
            changed.pointer.targets.clear();
            changed
                .slots
                .get_mut(&vec![PlaceProjection::Field(511)])
                .unwrap()
                .loans
                .clear();
            assert_eq!(value, before, "mutating the clone cannot mutate its source");
            value.held.loans.insert(77);
            assert_eq!(
                actual, expected,
                "mutating the source cannot mutate the clone"
            );
            assert_eq!(actual.creation_complete, complete);
            assert_eq!(actual.task_ports.is_empty(), !tasks);
            assert!(actual.slots.contains_key(&vec![PlaceProjection::Field(2)]));
        }
        assert_eq!(
            snapshot(&Value::default(), &mut budget(tasks)).unwrap(),
            snapshot_by_union(&Value::default(), &mut budget(tasks)).unwrap()
        );
    }
}
#[test]
fn large_tree_copy_pays_actual_linear_work_and_has_a_fresh_exact_boundary() {
    let mut value = Value::default();
    for index in 0..512 {
        value.slots.insert(
            vec![PlaceProjection::Field(1), PlaceProjection::Field(index)],
            payload(),
        );
    }
    let mut old = budget(false);
    let mut new = budget(false);
    let expected = snapshot_by_union(&value, &mut old).unwrap();
    assert_eq!(snapshot(&value, &mut new).unwrap(), expected);
    assert!(
        new.remaining() > old.remaining(),
        "linear tree clone removes actual search work"
    );
    let n = Limits::default().max_analysis_steps - new.remaining();
    for steps in [n, n - 1, n] {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        });
        let result = snapshot(&value, &mut b);
        if steps == n {
            assert_eq!(result.unwrap(), expected);
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
