use super::*;
#[allow(dead_code)]
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
    ));
}
#[allow(clippy::too_many_arguments)]
fn address_field_by_scan(
    f: &TypedFunction,
    address: ValueId,
    index: u32,
    parent: &Ports,
    out: &Ports,
    e: &mut Equations,
    outer: Option<&array_outer::Flow>,
    b: &mut Budget,
) -> Check<()> {
    if !out.array_elements.is_empty() {
        let shape = shape(&f.values[address.0 as usize])
            .ok_or_else(|| bad("array field projection has no actual qualified parent"))?;
        b.spend(
            out.array_elements
                .len()
                .saturating_mul(shape.slots.len().saturating_mul(3).saturating_add(1)),
        )?;
        for (ordinal, upper) in out.array_elements.iter().enumerate() {
            let path = [
                PlaceProjection::Field(index),
                PlaceProjection::Field(ordinal as u32),
            ];
            let row = shape
                .slots
                .iter()
                .find(|row| row.path == path)
                .ok_or_else(|| bad("array field lacks its actual physical class row"))?;
            array_outer::transport(e, *upper, parent.classes[row.binder as usize], outer, b)?;
        }
    }

    if let (Some(a), Some(z)) = (out.outer, parent.outer) {
        array_outer::transport(e, a, z, outer, b)?;
    }
    link_projection(
        &f.values[address.0 as usize],
        parent,
        out,
        LifeTypeProjection::Place(PlaceProjection::Field(index)),
        e,
        b,
    )?;
    Ok(())
}
type EquationState = (
    Vec<usize>,
    Option<Vec<Vec<usize>>>,
    Vec<Option<usize>>,
    Option<Vec<bool>>,
    usize,
);
fn state(e: &Equations) -> EquationState {
    (
        e.representative.clone(),
        e.members.clone(),
        e.rigid.clone(),
        e.grounded.clone(),
        e.next_rigid,
    )
}
fn copied(e: &Equations) -> Equations {
    Equations {
        representative: e.representative.clone(),
        members: e.members.clone(),
        rigid: e.rigid.clone(),
        grounded: e.grounded.clone(),
        next_rigid: e.next_rigid,
    }
}
fn flow_state(flow: &Flow) -> impl std::fmt::Debug + PartialEq {
    (
        flow.edges.clone(),
        flow.seeds.clone(),
        flow.selections.clone(),
        flow.call_requirements
            .iter()
            .map(|r| (r.outputs.clone(), r.inputs.clone()))
            .collect::<Vec<_>>(),
        flow.affected.clone(),
        flow.uses
            .iter()
            .map(|r| {
                (
                    r.site,
                    r.port,
                    match r.consumer {
                        Consumer::Copy => 0u8,
                        Consumer::Observe => 1,
                        Consumer::Bound => 2,
                        Consumer::Forbidden => 3,
                    },
                )
            })
            .collect::<Vec<_>>(),
        flow.boundaries
            .borrow()
            .iter()
            .map(|r| (r.site, r.actual, r.expected))
            .collect::<Vec<_>>(),
        flow.site.get(),
    )
}
fn setup(count: usize, members: bool, grounded: bool) -> (TypedFunction, Ports, Ports, Equations) {
    let mut b = Budget::new(&Limits::default());
    let mut e = Equations {
        representative: vec![],
        members: members.then(Vec::new),
        rigid: vec![],
        grounded: None,
        next_rigid: 0,
    };
    let classes = (0..count.max(1))
        .map(|_| e.fresh(true, &mut b).unwrap())
        .collect::<Vec<_>>();
    let parent = Ports {
        outer: Some(e.fresh(false, &mut b).unwrap()),
        classes,
        keys: vec![(vec![], 0)],
        ..Ports::default()
    };
    let out = Ports {
        outer: Some(e.fresh(false, &mut b).unwrap()),
        classes: vec![e.fresh(false, &mut b).unwrap()],
        keys: vec![(vec![], 0)],
        array_elements: (0..count)
            .map(|_| e.fresh(false, &mut b).unwrap())
            .collect(),
        ..Ports::default()
    };
    if grounded {
        e.track_grounding(&mut b).unwrap();
    }
    let mut f = fixture::module().functions[0].clone();
    f.values = vec![Type::BorrowedStorage {
        storage: Box::new(Type::Record {
            name: "ArrayField".into(),
            fields: vec![],
        }),
        life: StorageLifeShape {
            binder_count: count.max(1) as u32,
            slots: (0..count)
                .rev()
                .map(|id| zil_ir::StorageLifeSlot {
                    path: vec![PlaceProjection::Field(3), PlaceProjection::Field(id as u32)],
                    binder: id as u32,
                    mode: StoredBorrowMode::SharedAddress,
                })
                .collect(),
            retained: vec![zil_ir::StorageRetainedBinding {
                path: vec![LifeTypeProjection::Place(PlaceProjection::Field(3))],
                class: 0,
                binder: 0,
            }],
            nominal_arguments: vec![],
        },
    }];
    (f, parent, out, e)
}
fn flow(e: &Equations, parent: &Ports, out: &Ports, missing_edge: bool) -> Flow {
    let mut b = Budget::new(&Limits::default());
    let mut f = Flow::new(e.representative.len(), &mut b).unwrap();
    for (i, upper) in out.array_elements.iter().enumerate() {
        if i % 2 == 1 {
            f.affected[*upper] = true;
            if !missing_edge || i != 3 {
                f.edge(*upper, parent.classes[i], &mut b).unwrap();
            }
        }
    }
    f.affected[out.outer.unwrap()] = true;
    f.edge(out.outer.unwrap(), parent.outer.unwrap(), &mut b)
        .unwrap();
    f.enter(4, 7);
    f.boundaries.borrow_mut().push(Boundary {
        site: (2, 3),
        actual: parent.classes[0],
        expected: out.classes[0],
    });
    f
}
fn compare(
    f: &TypedFunction,
    parent: &Ports,
    out: &Ports,
    initial: &Equations,
    with_flow: bool,
    missing_edge: bool,
) -> Check<()> {
    let mut old = copied(initial);
    let mut new = copied(initial);
    let old_flow = with_flow.then(|| flow(&old, parent, out, missing_edge));
    let new_flow = with_flow.then(|| flow(&new, parent, out, missing_edge));
    let old_result = address_field_by_scan(
        f,
        ValueId(0),
        3,
        parent,
        out,
        &mut old,
        old_flow.as_ref(),
        &mut Budget::new(&Limits::default()),
    );
    let new_result = super::super::address_field(
        f,
        ValueId(0),
        3,
        parent,
        out,
        &mut new,
        new_flow.as_ref(),
        &mut Budget::new(&Limits::default()),
    );
    assert_eq!(new_result, old_result);
    assert_eq!(
        state(&new),
        state(&old),
        "all equations, including error prefix"
    );
    if let (Some(a), Some(z)) = (&old_flow, &new_flow) {
        assert_eq!(flow_state(a), flow_state(z), "all actual Flow constraints");
    }
    new_result
}
#[test]
fn local_array_field_table_keeps_complete_keys_first_match_and_all_equations() {
    for count in [0, 1, 2, 32, 512] {
        for members in [false, true] {
            for grounded in [false, true] {
                let (mut f, parent, out, e) = setup(count, members, grounded);
                if count > 0 {
                    let Type::BorrowedStorage { life, .. } = &mut f.values[0] else {
                        unreachable!()
                    };
                    let unrelated = [
                        vec![],
                        vec![PlaceProjection::Field(3)],
                        vec![
                            PlaceProjection::Field(3),
                            PlaceProjection::Field(0),
                            PlaceProjection::Field(0),
                        ],
                        vec![PlaceProjection::Field(99), PlaceProjection::Field(0)],
                        vec![PlaceProjection::Field(3), PlaceProjection::Field(u32::MAX)],
                        vec![
                            PlaceProjection::Variant(zil_ir::VariantProjection {
                                variant: 3,
                                index: 0,
                            }),
                            PlaceProjection::Field(0),
                        ],
                        vec![PlaceProjection::Field(3), PlaceProjection::ErrorArm(false)],
                        vec![PlaceProjection::Field(3), PlaceProjection::ErrorArm(true)],
                    ];
                    for path in unrelated {
                        life.slots.insert(
                            0,
                            zil_ir::StorageLifeSlot {
                                path,
                                binder: u32::MAX,
                                mode: StoredBorrowMode::SharedAddress,
                            },
                        );
                    }
                    // A later exact duplicate must never override the first real physical row.
                    life.slots.push(zil_ir::StorageLifeSlot {
                        path: vec![PlaceProjection::Field(3), PlaceProjection::Field(0)],
                        binder: u32::MAX,
                        mode: StoredBorrowMode::SharedAddress,
                    });
                }
                for outer in [false, true] {
                    compare(&f, &parent, &out, &e, outer, false).unwrap();
                }
            }
        }
    }
}
#[test]
fn local_field_table_keeps_missing_row_and_transport_error_prefixes() {
    for outer in [false, true] {
        let (mut f, parent, mut out, e) = setup(5, true, true);
        let Type::BorrowedStorage { life, .. } = &mut f.values[0] else {
            unreachable!()
        };
        life.slots
            .retain(|r| r.path != [PlaceProjection::Field(3), PlaceProjection::Field(2)]);
        assert!(
            compare(&f, &parent, &out, &e, outer, false)
                .unwrap_err()
                .message
                .contains("array field lacks its actual physical class row")
        );
        let (f, parent, fresh, e) = setup(5, true, true);
        out = fresh;
        out.array_elements[2] = parent.classes[3];
        assert!(compare(&f, &parent, &out, &e, outer, false).is_err());
        let (f, parent, out, e) = setup(5, true, true);
        assert!(compare(&f, &parent, &out, &e, true, true).is_err());
        let mut absent = f.clone();
        absent.values[0] = Type::I32;
        assert!(
            compare(&absent, &parent, &out, &e, outer, false)
                .unwrap_err()
                .message
                .contains("no actual qualified parent")
        );
    }
}
#[test]
fn local_field_table_removes_real_quadratic_lookup_with_fresh_exact_work() {
    let (f, parent, out, e) = setup(512, true, true);
    let mut old = copied(&e);
    let mut new = copied(&e);
    let mut old_b = Budget::new(&Limits::default());
    let mut new_b = Budget::new(&Limits::default());
    address_field_by_scan(&f, ValueId(0), 3, &parent, &out, &mut old, None, &mut old_b).unwrap();
    super::super::address_field(&f, ValueId(0), 3, &parent, &out, &mut new, None, &mut new_b)
        .unwrap();
    assert_eq!(state(&new), state(&old));
    assert!(new_b.remaining > old_b.remaining);
    let old_work = Limits::default().max_analysis_steps - old_b.remaining;
    let n = Limits::default().max_analysis_steps - new_b.remaining;
    assert_eq!(old_work - n, 786_944 - 5_121);
    println!("512 AddressField complete old={old_work} new={n}; metadata scan 786944 -> 5121");
    for steps in [n, n - 1, n] {
        let mut fresh = copied(&e);
        let result = super::super::address_field(
            &f,
            ValueId(0),
            3,
            &parent,
            &out,
            &mut fresh,
            None,
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == n {
            result.unwrap();
            assert_eq!(state(&fresh), state(&old));
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
}
