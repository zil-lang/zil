//! Complete independent 118b State, including every dynamic proof lane.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
struct OldState {
    debt: Vec<u8>,
    fresh: Vec<u8>,
    // Independent SSA Package field provenance. These bytes do not add debt:
    // a complete Package still moves as one value, with local partial fields
    // checked by the existing responsibility slots.
    package_fresh: Vec<Vec<u8>>,
    // Actual local/SSA Managed leaves of new Product functions, independently
    // rebuilt here. Empty on the historical flat path; never a public proof.
    product_fresh: Vec<u8>,
    desc: Vec<BTreeSet<Root>>,
    retirement: Vec<replacements::Sources>,
    // An edge a -> b means b may have been initialized later while a remained live.
    newer: Vec<BTreeSet<usize>>,
}
fn old_state_cost(s: &OldState, budget: &mut Budget) -> Check<()> {
    if !s.retirement.is_empty() {
        budget.spend(s.retirement.len())?;
        for sources in &s.retirement {
            sources.cost(budget)?;
        }
    }
    budget.spend(
        s.debt
            .len()
            .saturating_mul(2)
            .saturating_add(s.package_fresh.len())
            .saturating_add(s.product_fresh.len())
            .saturating_add(s.desc.len())
            .saturating_add(s.newer.len()),
    )?;
    for row in &s.package_fresh {
        budget.spend(row.len())?;
    }
    for row in &s.desc {
        budget.spend(row.len())?;
    }
    for row in &s.newer {
        budget.spend(row.len())?;
    }
    Ok(())
}
fn old_clone_state(s: &OldState, budget: &mut Budget) -> Check<OldState> {
    old_state_cost(s, budget)?;
    Ok(s.clone())
}
fn old_merge(target: &mut OldState, source: &OldState, budget: &mut Budget) -> Check<bool> {
    old_state_cost(source, budget)?;
    let mut changed = false;
    for (a, b) in target.retirement.iter_mut().zip(&source.retirement) {
        changed |= a.merge(b, budget)?;
    }
    for (a, b) in target
        .debt
        .iter_mut()
        .zip(&source.debt)
        .chain(target.fresh.iter_mut().zip(&source.fresh))
        .chain(target.product_fresh.iter_mut().zip(&source.product_fresh))
        .chain(
            target
                .package_fresh
                .iter_mut()
                .flatten()
                .zip(source.package_fresh.iter().flatten()),
        )
    {
        let next = *a | *b;
        changed |= next != *a;
        *a = next;
    }
    for (a, b) in target.desc.iter_mut().zip(&source.desc) {
        budget.spend(a.len().saturating_add(b.len()))?;
        let before = a.len();
        a.extend(b);
        changed |= a.len() != before;
    }
    for (a, b) in target.newer.iter_mut().zip(&source.newer) {
        budget.spend(a.len().saturating_add(b.len()))?;
        let before = a.len();
        a.extend(b);
        changed |= a.len() != before;
    }
    Ok(changed)
}
fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn old(rows: Vec<Vec<u8>>, seed: u32) -> OldState {
    OldState {
        debt: vec![EMPTY, LIVE, if seed == 0 { MOVED } else { DONE }],
        fresh: vec![FRESH, INPUT, seed as u8],
        package_fresh: rows,
        product_fresh: vec![seed as u8, INPUT, FRESH],
        desc: vec![BTreeSet::new(), BTreeSet::from([Root::Parameter(seed)])],
        retirement: vec![replacements::Sources::field_state_fixture(seed)],
        newer: vec![BTreeSet::from([seed as usize]), BTreeSet::new()],
    }
}
fn convert(old: &OldState) -> State {
    State {
        debt: old.debt.clone(),
        fresh: old.fresh.clone(),
        package_fresh: Fields::from_dense(&old.package_fresh, &mut budget(1_000_000)).unwrap(),
        product_fresh: old.product_fresh.clone(),
        desc: old.desc.clone(),
        retirement: replacements::RetirementRows::from_vec(
            old.retirement.clone(),
            &mut budget(1_000_000),
        )
        .unwrap(),
        newer: old.newer.clone(),
    }
}
fn snapshot(new: &State) -> OldState {
    OldState {
        debt: new.debt.clone(),
        fresh: new.fresh.clone(),
        package_fresh: new.package_fresh.dense(),
        product_fresh: new.product_fresh.clone(),
        desc: new.desc.clone(),
        retirement: new.retirement.dense(),
        newer: new.newer.clone(),
    }
}
fn rows(len: usize, seed: usize) -> Vec<Vec<u8>> {
    (0..len)
        .map(|i| {
            if i.is_multiple_of(17) {
                (0..3).map(|j| ((i + j + seed) % 256) as u8).collect()
            } else {
                vec![]
            }
        })
        .collect()
}
#[test]
fn field_fresh_full_state_matches_old_across_dense_flat_and_mixed_shapes() {
    for left_len in [0, 1, 3, 4, 5, 31, 32, 33, 638] {
        for right_len in [0, 1, 4, 5, 33, 638] {
            let mut expected = old(rows(left_len, 0), 0);
            let incoming = old(rows(right_len, 1), 1);
            let mut actual = convert(&expected);
            let source = convert(&incoming);
            let alias = actual.clone();
            let before = snapshot(&alias);
            for _ in 0..2 {
                assert_eq!(
                    old_merge(&mut expected, &incoming, &mut budget(1_000_000)).unwrap(),
                    merge(&mut actual, &source, &mut budget(1_000_000)).unwrap()
                );
                assert_eq!(expected, snapshot(&actual));
            }
            assert_eq!(snapshot(&alias), before);
            assert_eq!(snapshot(&source), incoming);
            assert_eq!(
                old_clone_state(&expected, &mut budget(1_000_000)).unwrap(),
                snapshot(&clone_state(&actual, &mut budget(1_000_000)).unwrap())
            );
        }
    }
    // Same row count, different widths: preserve the former FLATTENED zip,
    // rather than a new per-row union or a same-shape-only permission rule.
    let a = vec![vec![1, 2], vec![], vec![4], vec![], vec![]];
    let b = vec![vec![], vec![8], vec![16, 32, 64], vec![], vec![128]];
    let mut expected = old(a, 0);
    let incoming = old(b, 1);
    let mut actual = convert(&expected);
    let source = convert(&incoming);
    assert_eq!(
        old_merge(&mut expected, &incoming, &mut budget(1_000_000)).unwrap(),
        merge(&mut actual, &source, &mut budget(1_000_000)).unwrap()
    );
    assert_eq!(expected, snapshot(&actual));
}
#[test]
fn field_fresh_all_u8_and_optional_missing_coordinates_remain_exact() {
    for count in [0, 1, 4, 5, 9] {
        let widths = vec![2; count];
        let plan = layout(&widths, &mut budget(1_000_000)).unwrap();
        let mut value = Fields::new(&widths, plan.as_ref(), &mut budget(1_000_000)).unwrap();
        let mut expected = vec![vec![INPUT; 2]; count];
        for byte in 0..=u8::MAX {
            for (row, fields) in expected.iter_mut().enumerate() {
                for (field, at) in fields.iter_mut().enumerate() {
                    *at = byte;
                    value.set(row, field, byte, &mut budget(100)).unwrap();
                    assert_eq!(value.get(row, field, &mut budget(100)).unwrap(), Some(byte));
                }
            }
        }
        assert_eq!(value.dense(), expected);
        assert_eq!(value.get(count, 0, &mut budget(100)).unwrap(), None);
        if count > 0 {
            assert_eq!(value.get(0, 2, &mut budget(100)).unwrap(), None);
        }
        let alias = value.clone();
        let snapshot = alias.dense();
        assert!(value.set(count, 0, INPUT, &mut budget(100)).is_err());
        assert_eq!(value.dense(), expected);
        if count > 0 {
            assert!(value.set(0, 2, INPUT, &mut budget(100)).is_err());
            assert_eq!(value.dense(), expected);
            value.set(0, 0, 0, &mut budget(100)).unwrap();
        }
        assert_eq!(alias.dense(), snapshot);
        assert!(matches!(value.storage, Storage::Dense(_)) == (count <= DENSE_ROWS));
    }
    let zero_rows = Fields::from_dense(&vec![vec![]; 9], &mut budget(1000)).unwrap();
    assert_eq!(zero_rows.get(8, 0, &mut budget(100)).unwrap(), None);
    assert_eq!(zero_rows.dense().len(), 9);
}
#[test]
fn field_fresh_sparse_set_prepays_and_corrupt_inventory_never_publishes() {
    for count in [1, 4, 5, 638] {
        let widths = vec![1; count];
        let plan = layout(&widths, &mut budget(1_000_000)).unwrap();
        let seed = Fields::new(&widths, plan.as_ref(), &mut budget(1_000_000)).unwrap();
        let base = seed.clone();
        let mut expected = vec![vec![INPUT]; count];
        expected[count - 1][0] = FRESH;
        let run = |n| {
            let mut state = seed.clone();
            let result = state.set(count - 1, 0, FRESH, &mut budget(n));
            (result, state)
        };
        let mut n = 0;
        while run(n).0.is_err() {
            n += 1;
            assert!(n < 100);
        }
        let (result, first) = run(n);
        result.unwrap();
        assert_eq!(
            first.get(count - 1, 0, &mut budget(100)).unwrap(),
            Some(FRESH)
        );
        assert_eq!(first.dense(), expected);
        assert_eq!(seed, base);
        assert_eq!(
            run(n - 1).0.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(run(n - 1).1, seed);
        let (result, fresh) = run(n);
        result.unwrap();
        let (result, repeated) = run(n);
        result.unwrap();
        assert_eq!(fresh, repeated);
        assert_eq!(fresh.dense(), expected);
        assert_eq!(seed, base);
    }
    let mut corrupt = Fields::from_dense(&vec![vec![INPUT]; 9], &mut budget(1000)).unwrap();
    if let Storage::Flat { data, .. } = &mut corrupt.storage {
        data.pop();
    } else {
        panic!()
    }
    assert!(corrupt.get(0, 0, &mut budget(100)).is_err());
    let before = corrupt.clone();
    assert!(corrupt.set(0, 0, FRESH, &mut budget(100)).is_err());
    assert_eq!(corrupt, before);
    let mut good = Fields::from_dense(&vec![vec![INPUT]; 9], &mut budget(1000)).unwrap();
    let snapshot = good.clone();
    assert!(good.merge(&corrupt, &mut budget(1000)).is_err());
    assert_eq!(good, snapshot);
    assert_eq!(
        layout(&[usize::MAX, 1, 0, 0, 0], &mut budget(1000))
            .unwrap_err()
            .category,
        zil_base::ResultCategory::ResourceLimit
    );
}
#[test]
fn field_fresh_full_state_budget_failure_preserves_the_same_completed_prefix() {
    let left = old(rows(638, 0), 0);
    let right = old(rows(638, 1), 1);
    let source = convert(&right);
    // Measure actual prefix work independently for each representation; equal
    // numeric limits need not stop at the same state after algorithm changes.
    let old_cost = |s: &OldState| {
        let mut b = budget(1_000_000);
        old_state_cost(s, &mut b).unwrap();
        1_000_000 - b.remaining
    };
    let new_cost = |s: &State| {
        let mut b = budget(1_000_000);
        state_cost(s, &mut b).unwrap();
        1_000_000 - b.remaining
    };
    let mut l = left.clone();
    l.desc.clear();
    l.newer.clear();
    let mut r = right.clone();
    r.desc.clear();
    r.newer.clear();
    let mut b = budget(1_000_000);
    old_merge(&mut l, &r, &mut b).unwrap();
    let old_prefix = 1_000_000 - b.remaining - old_cost(&r);
    let mut l = convert(&left);
    l.desc.clear();
    l.newer.clear();
    let mut r = source.clone();
    r.desc.clear();
    r.newer.clear();
    let mut b = budget(1_000_000);
    merge(&mut l, &r, &mut b).unwrap();
    let new_prefix = 1_000_000 - b.remaining - new_cost(&r);
    let fail_fee = left.desc[1].len() + right.desc[1].len();
    let mut expected = left.clone();
    let mut actual = convert(&left);
    let a = old_merge(
        &mut expected,
        &right,
        &mut budget(old_cost(&right) + old_prefix + fail_fee - 1),
    );
    let b = merge(
        &mut actual,
        &source,
        &mut budget(new_cost(&source) + new_prefix + fail_fee - 1),
    );
    assert_eq!(
        a.as_ref().unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(a, b);
    assert_eq!(expected, snapshot(&actual));
    assert_ne!(expected.debt, left.debt);
    assert_ne!(expected.package_fresh, left.package_fresh);
    assert_ne!(expected.retirement, left.retirement);
    assert_eq!(expected.desc, left.desc);
    assert_eq!(expected.newer, left.newer);
}
#[test]
fn field_fresh_complete_state_clone_merge_keeps_exact_fresh_boundaries_and_small_rows() {
    let seed = old(rows(638, 0), 0);
    let incoming = old(rows(638, 1), 1);
    let packed = convert(&seed);
    let source = convert(&incoming);
    let run = |old: bool, n| {
        let mut b = budget(n);
        if old {
            let mut state = seed.clone();
            for _ in 0..16 {
                let mut copy = old_clone_state(&state, &mut b)?;
                old_merge(&mut copy, &incoming, &mut b)?;
                state = copy;
            }
            Ok::<_, Diagnostic>(state)
        } else {
            let mut state = packed.clone();
            for _ in 0..16 {
                let mut copy = clone_state(&state, &mut b)?;
                merge(&mut copy, &source, &mut b)?;
                state = copy;
            }
            Ok(snapshot(&state))
        }
    };
    let boundary = |old| {
        let (mut low, mut high) = (0, 1_000_000);
        assert!(run(old, high).is_ok());
        while low < high {
            let mid = (low + high) / 2;
            if run(old, mid).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        low
    };
    let n = boundary(false);
    let old_n = boundary(true);
    assert!(n < old_n, "flat N={n}, dense N={old_n}");
    assert_eq!(
        run(false, n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(
        run(true, old_n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(run(false, n).unwrap(), run(true, old_n).unwrap());
    assert_eq!(run(false, n).unwrap(), run(false, n).unwrap());
    for count in 0..=DENSE_ROWS {
        let seed = old(rows(count, 0), 0);
        let state = convert(&seed);
        let mut a = budget(1000);
        let mut b = budget(1000);
        old_state_cost(&seed, &mut a).unwrap();
        state_cost(&state, &mut b).unwrap();
        assert_eq!(a.remaining, b.remaining);
        assert_eq!(
            snapshot(&clone_state(&state, &mut budget(1000)).unwrap()),
            old_clone_state(&seed, &mut budget(1000)).unwrap()
        );
    }
    eprintln!(
        "V full resource State x16 clone/merge N={n}, Vec<Vec<u8>> N={old_n}; N-1/fresh checked; tiny row cost exact"
    );
}

#[test]
fn field_fresh_plan_binds_actual_partitions_and_charges_mixed_target_empty_rows() {
    let first = Plan::new(vec![1, 0, 2, 0, 0], &mut budget(1000)).unwrap();
    let other = Plan::new(vec![0, 1, 0, 0, 2], &mut budget(1000)).unwrap();
    let mut a = first.empty(&mut budget(1000)).unwrap();
    let alias = first.empty(&mut budget(1000)).unwrap();
    let b = other.empty(&mut budget(1000)).unwrap();
    assert_eq!(a.get(0, 0, &mut budget(100)).unwrap(), Some(INPUT));
    assert_eq!(b.get(0, 0, &mut budget(100)).unwrap(), None);
    assert_eq!(a.get(0, 1, &mut budget(100)).unwrap(), None);
    if let (
        Storage::Flat {
            layout: x,
            data: left,
        },
        Storage::Flat {
            layout: y,
            data: right,
        },
    ) = (&a.storage, &alias.storage)
    {
        assert!(Arc::ptr_eq(x, y));
        assert_ne!(left.as_ptr(), right.as_ptr()); // static layout shared, dynamic bytes independent
    } else {
        panic!()
    }
    a.set(0, 0, FRESH, &mut budget(100)).unwrap();
    assert_eq!(alias.get(0, 0, &mut budget(100)).unwrap(), Some(INPUT));
    assert_eq!(b.get(1, 0, &mut budget(100)).unwrap(), Some(INPUT));
    let source = Fields::from_dense(&vec![vec![]; 9], &mut budget(1000)).unwrap();
    for count in 0..=DENSE_ROWS {
        let mut target = Fields::from_dense(&vec![vec![]; count], &mut budget(1000)).unwrap();
        let mut b = budget(1000);
        assert!(!target.merge(&source, &mut b).unwrap());
        // One source payload/inventory check, then every actual target dense
        // row visited by flatten, even though neither side contains a byte.
        assert_eq!(1000 - b.remaining, 1 + count);
        assert_eq!(target.dense(), vec![vec![]; count]);
    }
}
