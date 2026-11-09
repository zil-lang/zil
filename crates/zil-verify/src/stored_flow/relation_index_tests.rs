use super::*;
#[allow(dead_code)]
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
    ));
}
fn compare(f: &TypedFunction, paths: &[Path], enums: bool) {
    let mut b = Budget::new(&Limits::default());
    b.stored_enum_addresses = enums;
    let mut tables = Relations::new(f, &mut b).unwrap();
    for base in [
        StorageBase::Result,
        StorageBase::Parameter(0),
        StorageBase::Parameter(9),
        StorageBase::Capture(0),
    ] {
        for path in paths {
            let mut old = Budget::new(&Limits::default());
            old.stored_enum_addresses = enums;
            assert_eq!(
                tables.get(base, path, &mut b),
                relation(f, base, path, &mut old),
                "complete binder/diagnostic at {base:?}/{path:?}"
            );
        }
    }
}
#[test]
fn final_relation_index_keeps_all_original_boundary_and_class_rules() {
    let paths = vec![
        vec![],
        vec![PlaceProjection::Field(0)],
        vec![PlaceProjection::Field(1), PlaceProjection::Field(0)],
        vec![PlaceProjection::ErrorArm(false), PlaceProjection::Field(0)],
        vec![PlaceProjection::Variant(zil_ir::VariantProjection {
            variant: 0,
            index: 0,
        })],
    ];
    for module in [
        fixture::module(),
        fixture::mutable_holder(),
        fixture::shared_two_slots(),
        fixture::wide_shared(8),
    ] {
        for f in &module.functions {
            for enums in [false, true] {
                compare(f, &paths, enums);
            }
        }
    }
    let base = fixture::module().functions.remove(0);
    let mut f = base.clone();
    let q = f.return_type.clone();
    f.return_type = Type::ErrorUnion {
        error: Box::new(q.clone()),
        result: Box::new(fixture::address(q, false)),
    };
    let c = f.storage_life.as_mut().unwrap();
    c.fixed_binder_count = 2;
    for (arm, pointee, binder) in [
        (true, false, StorageBinder::Fixed(0)),
        (false, true, StorageBinder::Fixed(1)),
    ] {
        let mut path = vec![LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm))];
        if pointee {
            path.insert(0, LifeTypeProjection::Pointee);
        }
        c.classes.push(zil_ir::SignatureLifeClassBinding {
            position: zil_ir::SignatureLifeClass {
                base: StorageBase::Result,
                path,
                class: 0,
            },
            binder,
        });
    }
    for enums in [false, true] {
        compare(
            &f,
            &[
                vec![PlaceProjection::ErrorArm(true), PlaceProjection::Field(0)],
                vec![PlaceProjection::ErrorArm(false), PlaceProjection::Field(0)],
                vec![PlaceProjection::Field(0)],
                vec![PlaceProjection::ErrorArm(true)],
            ],
            enums,
        );
    }
}
#[test]
fn complete_physical_paths_keep_queried_duplicate_policy_and_shape_identity() {
    let mut f = fixture::module().functions.remove(0);
    let mut paths = vec![
        vec![PlaceProjection::Field(0)],
        vec![PlaceProjection::Field(0), PlaceProjection::Field(1)],
        vec![PlaceProjection::ErrorArm(false)],
        vec![PlaceProjection::ErrorArm(true)],
    ];
    for variant in [0, 1] {
        for index in [0, 1] {
            paths.push(vec![PlaceProjection::Variant(zil_ir::VariantProjection {
                variant,
                index,
            })]);
        }
    }
    let Type::BorrowedStorage { life, .. } = &mut f.return_type else {
        unreachable!()
    };
    life.slots = paths
        .iter()
        .map(|path| zil_ir::StorageLifeSlot {
            path: path.clone(),
            binder: 0,
            mode: StoredBorrowMode::SharedAddress,
        })
        .collect();
    // An unrelated duplicate does not reject any other selected path.
    for _ in 0..2 {
        life.slots.push(zil_ir::StorageLifeSlot {
            path: vec![PlaceProjection::Field(99)],
            binder: u32::MAX,
            mode: StoredBorrowMode::SharedAddress,
        });
    }
    paths.push(vec![PlaceProjection::Field(99)]);
    paths.push(vec![PlaceProjection::Field(100)]);
    compare(&f, &paths, false);
    let mut duplicate = f.clone();
    let Type::BorrowedStorage { life, .. } = &mut duplicate.return_type else {
        unreachable!()
    };
    life.slots.push(life.slots[0].clone());
    compare(&duplicate, &paths, false);
    let mut other = f.clone();
    let Type::BorrowedStorage { life, .. } = &mut other.return_type else {
        unreachable!()
    };
    life.binder_count = 2;
    life.slots[0].binder = 1;
    other
        .storage_life
        .as_mut()
        .unwrap()
        .classes
        .push(zil_ir::SignatureLifeClassBinding {
            position: zil_ir::SignatureLifeClass {
                base: StorageBase::Result,
                path: vec![],
                class: 1,
            },
            binder: StorageBinder::Fixed(7),
        });
    let mut a = Budget::new(&Limits::default());
    let mut z = Budget::new(&Limits::default());
    let mut first = Relations::new(&f, &mut a).unwrap();
    let mut second = Relations::new(&other, &mut z).unwrap();
    assert_ne!(
        first.get(StorageBase::Result, &paths[0], &mut a).unwrap(),
        second.get(StorageBase::Result, &paths[0], &mut z).unwrap(),
        "same function identity in a new actual request cannot reuse old metadata"
    );
    // Equal-looking result and parameter nodes are distinct actual inventories.
    let mut two = f.clone();
    let local = two.parameters[0];
    two.locals[local.0 as usize].ty = other.return_type.clone();
    two.storage_life
        .as_mut()
        .unwrap()
        .classes
        .push(zil_ir::SignatureLifeClassBinding {
            position: zil_ir::SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![],
                class: 1,
            },
            binder: StorageBinder::Fixed(7),
        });
    let mut b = Budget::new(&Limits::default());
    let mut tables = Relations::new(&two, &mut b).unwrap();
    assert_eq!(
        tables.get(StorageBase::Result, &paths[0], &mut b).unwrap(),
        StorageBinder::Invocation(0)
    );
    assert_eq!(
        tables
            .get(StorageBase::Parameter(0), &paths[0], &mut b)
            .unwrap(),
        StorageBinder::Fixed(7)
    );
    assert_eq!(tables.shapes.len(), 2);
}
#[test]
fn final_physical_lookup_prepays_complete_builds_and_keeps_fresh_exact_limits() {
    let f = fixture::wide_shared(512).functions.remove(0);
    let Type::BorrowedStorage { life, .. } = &f.return_type else {
        unreachable!()
    };
    let paths = life
        .slots
        .iter()
        .map(|row| row.path.clone())
        .collect::<Vec<_>>();
    let mut old = Budget::new(&Limits::default());
    let mut new = Budget::new(&Limits::default());
    let mut tables = Relations::new(&f, &mut new).unwrap();
    let expected = paths
        .iter()
        .map(|path| relation(&f, StorageBase::Result, path, &mut old).unwrap())
        .collect::<Vec<_>>();
    let actual = paths
        .iter()
        .map(|path| tables.get(StorageBase::Result, path, &mut new).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(new.remaining > old.remaining);
    let n = Limits::default().max_analysis_steps - new.remaining;
    println!(
        "512 final relations old={} new={n}",
        Limits::default().max_analysis_steps - old.remaining
    );
    for steps in [n, n - 1, n] {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        });
        let result = Relations::new(&f, &mut b).and_then(|mut table| {
            paths
                .iter()
                .map(|p| table.get(StorageBase::Result, p, &mut b))
                .collect::<Check<Vec<_>>>()
        });
        if steps == n {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let mut b = Budget::new(&Limits::default());
    let mut baseline = Relations::new(&f, &mut b).unwrap();
    baseline
        .get(StorageBase::Result, &paths[0], &mut b)
        .unwrap();
    let full = baseline.shapes[0].1.rows.clone();
    for steps in [0, 1, 8, 32, 128] {
        let mut b = Budget::new(&Limits::default());
        let mut table = Relations::new(&f, &mut b).unwrap();
        let result = table.get(
            StorageBase::Result,
            &paths[0],
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        assert_eq!(
            result.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert!(
            table.shapes.is_empty(),
            "failed first build publishes no partial index"
        );
    }
    assert_eq!(full.len(), paths.len());
}
