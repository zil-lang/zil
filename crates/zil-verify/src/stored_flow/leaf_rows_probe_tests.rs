use super::*;
fn permission(id: usize) -> Permission {
    Permission {
        sources: vec![Source {
            static_dependency: None,
            retained_port: Some(PortOrigin::Task {
                carrier: Vec::new(),
                port: id as u32,
            }),
            base: HoldRoot::Parameter(0),
            leaf: vec![],
            inline: vec![PlaceProjection::Field(id as u32)],
            heap_parents: Vec::new(),
            heap_element: None,
        }],
        loans: vec![id, id],
    }
}
fn compare(new: &Rows, old: &probe_oracle::Rows, keys: &[Path]) {
    assert_eq!(
        new.iter().cloned().collect::<Vec<_>>(),
        old.iter().cloned().collect::<Vec<_>>()
    );
    // Both independently implemented structures retain the exact same arena.
    assert_eq!(format!("{new:?}"), format!("{old:?}"));
    for key in keys {
        let expected = old.position(key, &mut Budget::new(&Limits::default()));
        let actual = new.position(key, &mut Budget::new(&Limits::default()));
        assert_eq!(actual, expected, "complete key {key:?}");
    }
}
fn variant(tag: u32, field: u32) -> PlaceProjection {
    PlaceProjection::Variant(zil_ir::VariantProjection {
        variant: tag,
        index: field,
    })
}
#[test]
fn exact_key_probe_preserves_dense_sparse_mixed_and_integer_boundary_lookups() {
    for width in [31u32, 32, 33, 64, 512] {
        for start in [0, 97, u32::MAX - width] {
            for sparse in [false, true] {
                let mut keys = (0..width)
                    .map(|i| {
                        vec![PlaceProjection::Field(start.saturating_add(if sparse {
                            i.saturating_mul(2)
                        } else {
                            i
                        }))]
                    })
                    .collect::<Vec<_>>();
                keys.extend([
                    vec![],
                    vec![variant(0, 1)],
                    vec![variant(1, 0)],
                    vec![PlaceProjection::ErrorArm(false)],
                    vec![PlaceProjection::ErrorArm(true)],
                    vec![
                        PlaceProjection::Field(start),
                        PlaceProjection::ErrorArm(true),
                    ],
                ]);
                let mut b = Budget::new(&Limits::default());
                let mut new = Rows::default();
                let mut old = probe_oracle::Rows::default();
                // Reverse insertion, duplicate paths and unrelated Projection kinds.
                for (id, key) in keys.iter().rev().chain(keys.iter().take(3)).enumerate() {
                    new.push((key.clone(), permission(id)), &mut b).unwrap();
                    old.push((key.clone(), permission(id)), &mut b).unwrap();
                }
                let mut queries = keys.clone();
                queries.extend([
                    vec![PlaceProjection::Field(0)],
                    vec![PlaceProjection::Field(u32::MAX)],
                    vec![PlaceProjection::Field(start.saturating_sub(1))],
                    vec![PlaceProjection::Field(start.saturating_add(1))],
                    vec![variant(0, 0)],
                    vec![variant(0, 2)],
                    vec![PlaceProjection::ErrorArm(true), PlaceProjection::Field(0)],
                ]);
                compare(&new, &old, &queries);
                // Misses must preserve the original child insertion ordinal too.
                for (id, path) in queries.iter().enumerate() {
                    if id.is_multiple_of(3) {
                        new.push((path.clone(), permission(10_000 + id)), &mut b)
                            .unwrap();
                        old.push((path.clone(), permission(10_000 + id)), &mut b)
                            .unwrap();
                    }
                }
                compare(&new, &old, &queries);
            }
        }
    }
}
#[test]
fn candidate_probe_keeps_rows_clone_delete_duplicate_first_and_rebuild_behavior() {
    let paths = (0..64)
        .map(|id| vec![PlaceProjection::Field(7), PlaceProjection::Field(id)])
        .collect::<Vec<_>>();
    let raw = paths
        .iter()
        .enumerate()
        .map(|(id, path)| (path.clone(), permission(id)))
        .collect::<Vec<_>>();
    let mut b = Budget::new(&Limits::default());
    let mut new = Rows::from_vec(raw.clone(), &mut b).unwrap();
    let mut old = probe_oracle::Rows::from_vec(raw, &mut b).unwrap();
    new.push((paths[0].clone(), permission(1000)), &mut b)
        .unwrap();
    old.push((paths[0].clone(), permission(1000)), &mut b)
        .unwrap();
    compare(&new, &old, &paths);
    new.copy_work(&mut b).unwrap();
    old.copy_work(&mut b).unwrap();
    let mut copied = new.clone();
    let mut old_copied = old.clone();
    copied.permission_mut(1).loans.push(999);
    old_copied.permission_mut(1).loans.push(999);
    copied.retain(|(_, p)| p.loans[0] != 0, &mut b).unwrap();
    old_copied.retain(|(_, p)| p.loans[0] != 0, &mut b).unwrap();
    compare(&copied, &old_copied, &paths);
    assert_eq!(
        copied[copied.position(&paths[0], &mut b).unwrap().unwrap()]
            .1
            .loans,
        [1000, 1000]
    );
    compare(&new, &old, &paths);
    let raw = copied.into_vec(&mut b).unwrap();
    let old_raw = old_copied.into_vec(&mut b).unwrap();
    assert_eq!(raw, old_raw);
    compare(
        &Rows::from_vec(raw, &mut b).unwrap(),
        &probe_oracle::Rows::from_vec(old_raw, &mut b).unwrap(),
        &paths,
    );
    let old_work = new.index.as_ref().unwrap().copy_work;
    let mut partial = new.clone();
    let error = partial
        .retain(
            |(_, p)| p.loans[0] > 0,
            &mut Budget::new(&Limits {
                max_analysis_steps: old_work,
                ..Limits::default()
            }),
        )
        .unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    assert!(partial.index.is_none());
    assert_eq!(partial.position(&paths[0], &mut b).unwrap(), Some(63));
    assert_eq!(partial[63].1.loans, [1000, 1000]);
}
#[test]
fn actual_dense_probe_saves_comparisons_and_prepays_fresh_fallback_and_insert_work() {
    let raw = (0..512)
        .map(|id| {
            (
                vec![PlaceProjection::Field(1), PlaceProjection::Field(id)],
                permission(id as usize),
            )
        })
        .collect::<Vec<_>>();
    let keys = raw.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
    let mut setup = Budget::new(&Limits::default());
    let rows = Rows::from_vec(raw.clone(), &mut setup).unwrap();
    let old = probe_oracle::Rows::from_vec(raw, &mut setup).unwrap();
    let limit = Limits::default().max_analysis_steps;
    let mut old_b = Budget::new(&Limits::default());
    let mut new_b = Budget::new(&Limits::default());
    for key in &keys {
        assert_eq!(
            rows.position(key, &mut new_b).unwrap(),
            old.position(key, &mut old_b).unwrap()
        );
    }
    assert!(new_b.remaining > old_b.remaining);
    println!(
        "512 actual two-level leaves old={} new={}",
        limit - old_b.remaining,
        limit - new_b.remaining
    );
    for queries in [
        keys,
        vec![
            vec![PlaceProjection::Field(1), PlaceProjection::Field(u32::MAX)],
            vec![PlaceProjection::Field(1), variant(0, 0)],
            vec![PlaceProjection::Field(0)],
        ],
    ] {
        let mut measured = Budget::new(&Limits::default());
        for key in &queries {
            rows.position(key, &mut measured).unwrap();
        }
        let n = limit - measured.remaining;
        for steps in [n, n - 1, n] {
            let mut b = Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            });
            let result = queries
                .iter()
                .try_for_each(|key| rows.position(key, &mut b).map(|_| ()));
            if steps == n {
                result.unwrap();
                assert_eq!(b.remaining, 0);
            } else {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }
    // Preserve the old deep-trie cost as a control and measure the real new
    // eligibility checks too; the production lookup does not skip this work.
    let deep_path = vec![PlaceProjection::Field(0); 17];
    let deep_raw = (0..32)
        .map(|id| (deep_path.clone(), permission(id)))
        .collect::<Vec<_>>();
    let deep = Rows::from_vec(deep_raw.clone(), &mut setup).unwrap();
    let deep_old = probe_oracle::Rows::from_vec(deep_raw, &mut setup).unwrap();
    let mut old_cost = Budget::new(&Limits::default());
    let mut new_cost = Budget::new(&Limits::default());
    assert_eq!(
        deep_old.position(&deep_path, &mut old_cost).unwrap(),
        Some(0)
    );
    assert_eq!(deep.position(&deep_path, &mut new_cost).unwrap(), Some(0));
    assert_eq!(limit - old_cost.remaining, 69);
    let deep_n = limit - new_cost.remaining;
    println!("17-step trie old=69 new={deep_n}; actual eligibility work retained");
    for steps in [deep_n, deep_n - 1, deep_n] {
        let result = deep.position(
            &deep_path,
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == deep_n {
            assert_eq!(result.unwrap(), Some(0));
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let mut suffix = vec![PlaceProjection::Field(1), PlaceProjection::Field(0)];
    suffix.extend(vec![PlaceProjection::ErrorArm(true); 65]);
    let mut expected = rows.clone();
    let mut measured = Budget::new(&Limits::default());
    expected
        .push((suffix.clone(), permission(1000)), &mut measured)
        .unwrap();
    let n = limit - measured.remaining;
    for steps in [0, n - 1, n, n] {
        let mut current = rows.clone();
        let before = format!("{current:?}");
        let result = current.push(
            (suffix.clone(), permission(1000)),
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == n {
            result.unwrap();
            assert_eq!(format!("{current:?}"), format!("{expected:?}"));
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
            assert_eq!(
                format!("{current:?}"),
                before,
                "failed probe/suffix publishes no partial arena"
            );
        }
    }
}
