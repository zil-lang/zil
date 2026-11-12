use super::*;
fn permission(id: usize) -> Permission {
    Permission {
        loans: vec![id, id],
        ..Permission::default()
    }
}
fn paths() -> Vec<Path> {
    let mut rows = vec![
        vec![],
        vec![PlaceProjection::Field(0)],
        vec![PlaceProjection::Field(0), PlaceProjection::Field(1)],
        vec![PlaceProjection::ErrorArm(false)],
        vec![PlaceProjection::ErrorArm(true)],
    ];
    for variant in [0, 1] {
        for index in [0, 1] {
            rows.push(vec![PlaceProjection::Variant(zil_ir::VariantProjection {
                variant,
                index,
            })]);
        }
    }
    for id in 0..64 {
        rows.push(vec![PlaceProjection::Field(7), PlaceProjection::Field(id)]);
    }
    rows
}
fn compare(new: &Rows, old: &oracle::Rows, keys: &[Path]) {
    if let Some(index) = &new.index {
        let edges = index.nodes.iter().map(|n| n.children.len()).sum::<usize>();
        assert_eq!(index.copy_work, index.nodes.len() + edges + 1);
    }
    assert_eq!(
        new.iter().cloned().collect::<Vec<_>>(),
        old.iter().cloned().collect::<Vec<_>>()
    );
    for path in keys {
        let expected = old
            .position(path, &mut Budget::new(&Limits::default()))
            .unwrap();
        assert_eq!(
            new.position(path, &mut Budget::new(&Limits::default()))
                .unwrap(),
            expected,
            "complete path {path:?}"
        );
    }
    assert_eq!(
        new.position(
            &[PlaceProjection::Field(u32::MAX)],
            &mut Budget::new(&Limits::default())
        )
        .unwrap(),
        None
    );
}
#[test]
fn trie_matches_old_index_for_every_row_transition_and_complete_projection() {
    let keys = paths();
    let mut new = Rows::default();
    let mut old = oracle::Rows::default();
    let mut b = Budget::new(&Limits::default());
    for (id, path) in keys.iter().rev().chain(keys.iter()).enumerate() {
        new.push((path.clone(), permission(id)), &mut b).unwrap();
        old.push((path.clone(), permission(id)), &mut b).unwrap();
        compare(&new, &old, &keys);
    }
    new.permission_mut(0).sources.push(Source {
        retained_port: Some(PortOrigin::Task { carrier: Vec::new(), port: 3 }),
        base: HoldRoot::Static,
        leaf: vec![],
        inline: vec![],
        heap_parents: Vec::new(),
        heap_element: None,
    });
    old.permission_mut(0).sources = new[0].1.sources.clone();
    compare(&new, &old, &keys);
    new.copy_work(&mut b).unwrap();
    old.copy_work(&mut b).unwrap();
    let mut copied = new.clone();
    let mut old_copy = old.clone();
    copied
        .retain(|(_, p)| p.loans[0] >= keys.len(), &mut b)
        .unwrap();
    old_copy
        .retain(|(_, p)| p.loans[0] >= keys.len(), &mut b)
        .unwrap();
    compare(&copied, &old_copy, &keys);
    compare(&new, &old, &keys);
    copied.push((vec![], permission(999)), &mut b).unwrap();
    old_copy.push((vec![], permission(999)), &mut b).unwrap();
    compare(&copied, &old_copy, &keys);
    let vec = copied.into_vec(&mut b).unwrap();
    let old_vec = old_copy.into_vec(&mut b).unwrap();
    assert_eq!(vec, old_vec);
    compare(
        &Rows::from_vec(vec, &mut b).unwrap(),
        &oracle::Rows::from_vec(old_vec, &mut b).unwrap(),
        &keys,
    );
    let small = (0..31)
        .map(|id| (vec![PlaceProjection::Field(id)], permission(id as usize)))
        .collect::<Vec<_>>();
    let small_new = Rows::from_vec(small.clone(), &mut b).unwrap();
    let small_old = oracle::Rows::from_vec(small, &mut b).unwrap();
    let mut new_b = Budget::new(&Limits::default());
    let mut old_b = Budget::new(&Limits::default());
    assert_eq!(
        small_new.position(&[PlaceProjection::Field(30)], &mut new_b),
        small_old.position(&[PlaceProjection::Field(30)], &mut old_b)
    );
    assert_eq!(
        new_b.remaining, old_b.remaining,
        "small Rows retain their original work"
    );
}
#[test]
fn trie_insert_is_atomic_on_budget_failure_and_retain_keeps_original_fallback() {
    let raw = (0..64)
        .map(|id| (vec![PlaceProjection::Field(id)], permission(id as usize)))
        .collect::<Vec<_>>();
    let initial = Rows::from_vec(raw, &mut Budget::new(&Limits::default())).unwrap();
    let mut suffix = vec![PlaceProjection::Field(0)];
    suffix.extend(vec![PlaceProjection::ErrorArm(true); 129]);
    let mut full = initial.clone();
    let mut budget = Budget::new(&Limits::default());
    full.push((suffix.clone(), permission(1000)), &mut budget)
        .unwrap();
    let n = Limits::default().max_analysis_steps - budget.remaining;
    for steps in [0, n - 1, n, n] {
        let mut rows = initial.clone();
        let before = format!("{rows:?}");
        let result = rows.push(
            (suffix.clone(), permission(1000)),
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == n {
            result.unwrap();
            assert_eq!(rows, full);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
            assert_eq!(
                format!("{rows:?}"),
                before,
                "failed suffix publishes no nodes, edges, terminal, copy_work or permission"
            );
        }
    }
    // Exact old-index deletion charge succeeds, but no work remains to rebuild.
    let old_work = initial.index.as_ref().unwrap().copy_work;
    let mut changed = initial.clone();
    let error = changed
        .retain(
            |(_, p)| p.loans[0] != 0,
            &mut Budget::new(&Limits {
                max_analysis_steps: old_work,
                ..Limits::default()
            }),
        )
        .unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    assert!(changed.index.is_none());
    assert_eq!(changed.len(), 63);
    assert_eq!(
        changed
            .position(
                &[PlaceProjection::Field(1)],
                &mut Budget::new(&Limits::default())
            )
            .unwrap(),
        Some(0)
    );
    assert_eq!(
        initial
            .position(
                &[PlaceProjection::Field(0)],
                &mut Budget::new(&Limits::default())
            )
            .unwrap(),
        Some(0)
    );
}
#[test]
fn trie_shares_real_prefix_work_and_clone_space_with_fresh_exact_lookup() {
    let raw = (0..512)
        .map(|id| {
            let mut path = vec![PlaceProjection::Field(7); 17];
            path.push(PlaceProjection::Field(id));
            (path, permission(id as usize))
        })
        .collect::<Vec<_>>();
    let mut b = Budget::new(&Limits::default());
    let rows = Rows::from_vec(raw.clone(), &mut b).unwrap();
    let old = oracle::Rows::from_vec(raw, &mut b).unwrap();
    let keys = (0..512)
        .map(|id| {
            let mut path = vec![PlaceProjection::Field(7); 17];
            path.push(PlaceProjection::Field(id));
            path
        })
        .collect::<Vec<_>>();
    let mut old_b = Budget::new(&Limits::default());
    let mut new_b = Budget::new(&Limits::default());
    for key in &keys {
        assert_eq!(
            rows.position(key, &mut new_b).unwrap(),
            old.position(key, &mut old_b).unwrap()
        );
    }
    assert!(
        new_b.remaining > old_b.remaining,
        "trie traverses a common prefix once, not for each binary comparison"
    );
    let n = Limits::default().max_analysis_steps - new_b.remaining;
    for steps in [n, n - 1, n] {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        });
        let result = keys
            .iter()
            .try_for_each(|path| rows.position(path, &mut b).map(|_| ()));
        if steps == n {
            result.unwrap();
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let mut new_copy = Budget::new(&Limits::default());
    let mut old_copy = Budget::new(&Limits::default());
    rows.copy_work(&mut new_copy).unwrap();
    old.copy_work(&mut old_copy).unwrap();
    assert!(new_copy.remaining > old_copy.remaining);
    let deep = vec![PlaceProjection::Field(9); 4097];
    let raw = (0..32)
        .map(|id| (deep.clone(), permission(id)))
        .collect::<Vec<_>>();
    let deep_rows = Rows::from_vec(raw.clone(), &mut b).unwrap();
    let deep_old = oracle::Rows::from_vec(raw, &mut b).unwrap();
    deep_rows.copy_work(&mut b).unwrap();
    let copy = deep_rows.clone();
    assert_eq!(
        copy.position(&deep, &mut b).unwrap(),
        deep_old.position(&deep, &mut b).unwrap()
    );
    assert_eq!(
        copy.index.as_ref().unwrap().nodes.len(),
        deep.len() + 1,
        "flat arena supports deep paths without recursive node clone/drop"
    );
}
