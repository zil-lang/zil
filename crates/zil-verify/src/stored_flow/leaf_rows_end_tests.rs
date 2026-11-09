use super::*;
fn permission(id: usize) -> Permission {
    Permission {
        loans: vec![id, id],
        ..Permission::default()
    }
}
fn variant(tag: u32, field: u32) -> PlaceProjection {
    PlaceProjection::Variant(zil_ir::VariantProjection {
        variant: tag,
        index: field,
    })
}
fn compare(new: &Rows, old: &end_oracle::Rows, keys: &[Path]) {
    assert_eq!(
        new.iter().cloned().collect::<Vec<_>>(),
        old.iter().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        format!("{new:?}"),
        format!("{old:?}"),
        "full arena, first ordinal and payload"
    );
    for key in keys {
        let actual = new.position(key, &mut Budget::new(&Limits::default()));
        let expected = old.position(key, &mut Budget::new(&Limits::default()));
        assert_eq!(actual, expected, "full Projection key {key:?}");
    }
}
#[test]
fn complete_end_key_bounds_preserve_empty_single_mixed_and_duplicate_rows() {
    let steps = vec![
        PlaceProjection::Field(0),
        PlaceProjection::Field(1),
        PlaceProjection::Field(u32::MAX),
        variant(0, 0),
        variant(0, 1),
        variant(1, 0),
        PlaceProjection::ErrorArm(false),
        PlaceProjection::ErrorArm(true),
    ];
    let mut queries = steps.iter().copied().map(|p| vec![p]).collect::<Vec<_>>();
    queries.push(vec![]);
    queries.push(vec![
        PlaceProjection::Field(0),
        PlaceProjection::ErrorArm(false),
    ]);
    // Thirty-two duplicate rows create a real index with empty or single
    // children, so these branches are not merely tested on unindexed Rows.
    for path in std::iter::once(vec![]).chain(steps.iter().copied().map(|p| vec![p])) {
        let raw = (0..32)
            .map(|id| (path.clone(), permission(id)))
            .collect::<Vec<_>>();
        let mut b = Budget::new(&Limits::default());
        let new = Rows::from_vec(raw.clone(), &mut b).unwrap();
        let old = end_oracle::Rows::from_vec(raw, &mut b).unwrap();
        assert!(new.index.is_some());
        compare(&new, &old, &queries);
        for query in &queries {
            new.copy_work(&mut b).unwrap();
            old.copy_work(&mut b).unwrap();
            let mut changed = new.clone();
            let mut old_changed = old.clone();
            changed
                .push((query.clone(), permission(1000)), &mut b)
                .unwrap();
            old_changed
                .push((query.clone(), permission(1000)), &mut b)
                .unwrap();
            compare(&changed, &old_changed, &queries);
        }
    }
    for first in [0, 97, u32::MAX - 63] {
        let mut paths = (0u32..64)
            .map(|id| {
                vec![PlaceProjection::Field(
                    first.saturating_add(id.saturating_mul(2)),
                )]
            })
            .collect::<Vec<_>>();
        paths.extend(queries.clone());
        let mut new = Rows::default();
        let mut old = end_oracle::Rows::default();
        let mut b = Budget::new(&Limits::default());
        for (id, path) in paths.iter().rev().chain(paths.iter()).enumerate() {
            new.push((path.clone(), permission(id)), &mut b).unwrap();
            old.push((path.clone(), permission(id)), &mut b).unwrap();
        }
        let mut keys = paths.clone();
        keys.extend((0..65).map(|id| vec![PlaceProjection::Field(first.saturating_add(id))]));
        compare(&new, &old, &keys);
        new.retain(|(_, p)| p.loans[0] >= paths.len(), &mut b)
            .unwrap();
        old.retain(|(_, p)| p.loans[0] >= paths.len(), &mut b)
            .unwrap();
        compare(&new, &old, &keys);
    }
}
fn grow(steps: usize) -> Check<(Rows, usize)> {
    let mut b = Budget::new(&Limits {
        max_analysis_steps: steps,
        ..Limits::default()
    });
    let mut rows = Rows::default();
    for id in 0..512 {
        let path = vec![PlaceProjection::Field(1), PlaceProjection::Field(id)];
        assert_eq!(rows.position(&path, &mut b)?, None);
        rows.push((path, permission(id as usize)), &mut b)?;
    }
    Ok((rows, b.remaining))
}
#[test]
fn actual_growing_position_then_insert_saves_work_with_exact_fresh_failure_boundaries() {
    let limit = Limits::default().max_analysis_steps;
    let mut old_b = Budget::new(&Limits::default());
    let mut old = end_oracle::Rows::default();
    for id in 0..512 {
        let path = vec![PlaceProjection::Field(1), PlaceProjection::Field(id)];
        assert_eq!(old.position(&path, &mut old_b).unwrap(), None);
        old.push((path, permission(id as usize)), &mut old_b)
            .unwrap();
    }
    let (rows, remaining) = grow(limit).unwrap();
    assert!(remaining > old_b.remaining);
    assert_eq!(rows.len(), 512);
    let n = limit - remaining;
    println!(
        "512 complete two-level new paths, position then push: previous={} end-bound={n}",
        limit - old_b.remaining
    );
    let keys = rows.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
    compare(&rows, &old, &keys);
    for steps in [n, n - 1, n] {
        let result = grow(steps);
        if steps == n {
            let (actual, remaining) = result.unwrap();
            assert_eq!(format!("{actual:?}"), format!("{rows:?}"));
            assert_eq!(remaining, 0);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let mut b = Budget::new(&Limits::default());
    let mut expected = rows.clone();
    let mut suffix = vec![PlaceProjection::Field(1), PlaceProjection::Field(512)];
    suffix.extend(vec![PlaceProjection::ErrorArm(true); 65]);
    assert_eq!(expected.position(&suffix, &mut b).unwrap(), None);
    expected
        .push((suffix.clone(), permission(1000)), &mut b)
        .unwrap();
    let append_n = limit - b.remaining;
    for steps in [0, append_n - 1, append_n, append_n] {
        let mut current = rows.clone();
        let before = format!("{current:?}");
        let mut b = Budget::new(&Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        });
        let result = (|| {
            assert_eq!(current.position(&suffix, &mut b)?, None);
            current.push((suffix.clone(), permission(1000)), &mut b)
        })();
        if steps == append_n {
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
                "failed bound check/growth publishes nothing"
            );
        }
    }
}
