//! Complete flat State oracle, not a mirror of page operations.
use super::*;
include!("reference.rs");
fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn source(seed: usize) -> Sources {
    let target = |id: usize| actual::Target {
        base: if id.is_multiple_of(2) {
            actual::Base::Input(id as u32)
        } else {
            actual::Base::Local(LocalId(id as u32))
        },
        steps: (0..id % 7)
            .map(|i| PlaceProjection::Field(i as u32))
            .collect(),
        mutable: id.is_multiple_of(3),
    };
    let mut value = Sources::default();
    if !seed.is_multiple_of(3) {
        value.direct.insert(target(seed));
    }
    if seed.is_multiple_of(4) {
        value.captures.insert(0, actual::Targets::new());
    }
    if seed.is_multiple_of(5) {
        value.captures.insert(2, BTreeSet::from([target(seed + 1)]));
    }
    value
}
fn old(count: usize, seed: usize) -> OldState {
    let fields = field_fresh::Plan::new(vec![2, 0, 1, 0, 0], &mut budget(1_000_000))
        .unwrap()
        .empty(&mut budget(1_000_000))
        .unwrap();
    OldState {
        debt: vec![LIVE, EMPTY, MOVED],
        fresh: vec![FRESH, INPUT, 0],
        package_fresh: fields,
        product_fresh: vec![INPUT, FRESH],
        desc: (0..count)
            .map(|i| {
                if i == 0 {
                    BTreeSet::from([Root::Parameter(seed as u32)])
                } else {
                    BTreeSet::new()
                }
            })
            .collect(),
        retirement: (0..count).map(|i| source(i + seed)).collect(),
        newer: vec![BTreeSet::from([seed]), BTreeSet::new()],
    }
}
fn convert(value: &OldState) -> State {
    State {
        debt: value.debt.clone(),
        fresh: value.fresh.clone(),
        package_fresh: value.package_fresh.clone(),
        product_fresh: value.product_fresh.clone(),
        desc: value.desc.clone(),
        retirement: Rows::from_vec(value.retirement.clone(), &mut budget(10_000_000)).unwrap(),
        newer: value.newer.clone(),
    }
}
fn snapshot(value: &State) -> OldState {
    OldState {
        debt: value.debt.clone(),
        fresh: value.fresh.clone(),
        package_fresh: value.package_fresh.clone(),
        product_fresh: value.product_fresh.clone(),
        desc: value.desc.clone(),
        retirement: value.retirement.dense(),
        newer: value.newer.clone(),
    }
}
#[test]
fn retirement_pages_complete_state_matches_flat_clone_merge_and_aliases() {
    for left in [0, 1, 3, 4, 5, 15, 16, 17, 31, 32, 33, 72] {
        for right in [0, 1, 4, 5, 16, 17, 33, 72] {
            let mut expected = old(left, 0);
            let incoming = old(right, 2);
            let mut actual = convert(&expected);
            let from = convert(&incoming);
            let alias = actual.clone();
            let before = snapshot(&alias);
            for _ in 0..2 {
                assert_eq!(
                    old_merge(&mut expected, &incoming, &mut budget(10_000_000)),
                    merge(&mut actual, &from, &mut budget(10_000_000))
                );
                assert_eq!(
                    snapshot(&actual),
                    expected,
                    "complete logical state {left}/{right}"
                );
            }
            assert_eq!(snapshot(&alias), before);
            assert_eq!(snapshot(&from), incoming);
            assert_eq!(
                snapshot(&clone_state(&actual, &mut budget(10_000_000)).unwrap()),
                old_clone_state(&expected, &mut budget(10_000_000)).unwrap()
            );
        }
    }
}
#[test]
fn retirement_pages_reads_assignments_and_captures_never_alias_or_fill_missing_rows() {
    for count in [0, 1, 4, 5, 15, 16, 17, 72] {
        let mut rows = Rows::new(count, &mut budget(10_000_000)).unwrap();
        let mut flat = vec![Sources::default(); count];
        for index in 0..count {
            let value = source(index + 11);
            flat[index] = value.clone();
            rows.replace(index, value, &mut budget(10_000_000)).unwrap();
        }
        let alias = rows.clone();
        let before = alias.dense();
        for index in (0..count).rev() {
            let value = source(index + 23);
            rows.direct(index, value.direct.clone(), &mut budget(10_000_000))
                .unwrap();
            flat[index].direct = value.direct;
            rows.captures(index, value.captures.clone(), &mut budget(10_000_000))
                .unwrap();
            flat[index].captures = value.captures;
            assert_eq!(rows.get(index, &mut budget(100)).unwrap(), &flat[index]);
        }
        assert_eq!(rows.dense(), flat);
        assert_eq!(alias.dense(), before);
        let before = rows.dense();
        assert!(rows.get(count, &mut budget(100)).is_err());
        assert!(
            rows.replace(count, Sources::default(), &mut budget(10_000_000))
                .is_err()
        );
        assert_eq!(rows.dense(), before);
        assert_eq!(matches!(rows.storage, Storage::Dense(_)), count <= SMALL);
    }
}
#[test]
fn retirement_pages_first_write_prepays_deep_copy_and_cleanup_before_publication() {
    let seed = Rows::from_vec((0..33).map(source).collect(), &mut budget(10_000_000)).unwrap();
    let run = |n| {
        let mut value = seed.clone();
        let mut b = budget(n);
        let result = value.replace(17, source(99), &mut b);
        (result, value, n - b.remaining)
    };
    let (result, expected, n) = run(10_000_000);
    result.unwrap();
    let (failure, value, _) = run(n - 1);
    assert_eq!(
        failure.unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(
        value.dense(),
        seed.dense(),
        "no detached page or changed Source on failed prepayment"
    );
    let (exact, value, _) = run(n);
    exact.unwrap();
    assert_eq!(value.dense(), expected.dense());
    let (fresh, value, _) = run(n);
    fresh.unwrap();
    assert_eq!(value.dense(), expected.dense());
    let mut unique = seed.clone();
    unique.replace(17, source(99), &mut budget(n)).unwrap();
    let mut b = budget(10_000_000);
    unique.replace(17, source(100), &mut b).unwrap();
    let repeat = 10_000_000 - b.remaining;
    assert!(repeat < n, "unique page does not repeat its deep copy");
    assert_eq!(seed.get(17, &mut budget(100)).unwrap(), &source(17));
    eprintln!(
        "retirement page first shared write N={n}, unique page write={repeat}, N-1=ResourceLimit freshN=Pass"
    );
}
#[test]
fn retirement_pages_complete_requests_keep_tiny_cost_and_reduce_real_large_copy_work() {
    for count in [0, 1, 3, 4, 5, 16, 17, 72] {
        let seed = old(count, 1);
        let incoming = old(count, 3);
        let actual = convert(&seed);
        let from = convert(&incoming);
        let old_request = |n| {
            let mut b = budget(n);
            let mut state = seed.clone();
            let result: Check<()> = (|| {
                for _ in 0..16 {
                    state = old_clone_state(&state, &mut b)?;
                    old_merge(&mut state, &incoming, &mut b)?;
                }
                Ok(())
            })();
            (result, state, n - b.remaining)
        };
        let request = |n| {
            let mut b = budget(n);
            let mut state = actual.clone();
            let result: Check<()> = (|| {
                for _ in 0..16 {
                    state = clone_state(&state, &mut b)?;
                    merge(&mut state, &from, &mut b)?;
                }
                Ok(())
            })();
            (result, state, n - b.remaining)
        };
        let (old_result, old_state, old_n) = old_request(10_000_000);
        old_result.unwrap();
        let (result, state, n) = request(10_000_000);
        result.unwrap();
        assert_eq!(snapshot(&state), old_state);
        if count <= SMALL {
            assert_eq!(n, old_n, "tiny original representation/cost");
        }
        // This deliberately includes all new merge traversal, COW and cleanup
        // costs. It is not asserted to be cheaper when every page is written.
        let (fail, _, _) = request(n - 1);
        assert_eq!(
            fail.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        request(n).0.unwrap();
        request(n).0.unwrap();
        eprintln!("complete retirement State clone+merge x16 rows={count} N={n} old={old_n}");
    }
    // Separate actual immutable State snapshots: original full flat copying
    // versus paid page references, including the other State fact lanes.
    let seed = old(72, 1);
    let actual = convert(&seed);
    let mut old_b = budget(10_000_000);
    let mut new_b = budget(10_000_000);
    for _ in 0..16 {
        old_clone_state(&seed, &mut old_b).unwrap();
        clone_state(&actual, &mut new_b).unwrap();
    }
    let old_n = 10_000_000 - old_b.remaining;
    let n = 10_000_000 - new_b.remaining;
    assert!(n < old_n, "real unchanged snapshots {n} >= {old_n}");
    eprintln!("complete retirement State immutable snapshots x16 N={n} old={old_n}");
}
#[test]
fn retirement_pages_construction_and_shared_state_merge_have_fresh_boundaries() {
    for count in [0, 1, 4, 5, 16, 17, 72] {
        let mut b = budget(10_000_000);
        let expected = Rows::new(count, &mut b).unwrap();
        let n = 10_000_000 - b.remaining;
        if n > 0 {
            assert_eq!(
                Rows::new(count, &mut budget(n - 1)).unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
        assert_eq!(Rows::new(count, &mut budget(n)).unwrap(), expected);
        assert_eq!(Rows::new(count, &mut budget(n)).unwrap(), expected);
    }
    let seed = convert(&old(33, 0));
    let incoming = convert(&old(17, 2));
    let run = |n| {
        let mut state = seed.clone();
        let mut b = budget(n);
        let result = merge(&mut state, &incoming, &mut b);
        (result, state, n - b.remaining)
    };
    let (success, expected, n) = run(10_000_000);
    assert!(success.unwrap());
    assert_eq!(
        run(n - 1).0.unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    let (exact, state, _) = run(n);
    assert!(exact.unwrap());
    assert_eq!(state, expected);
    let (fresh, state, _) = run(n);
    assert!(fresh.unwrap());
    assert_eq!(state, expected);
    let mut old_state = old(33, 0);
    old_merge(&mut old_state, &old(17, 2), &mut budget(10_000_000)).unwrap();
    assert_eq!(
        snapshot(&expected),
        old_state,
        "min row count plus untouched target suffix"
    );
}
