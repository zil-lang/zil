//! Exact same-Arc guard, complete eager-page and independent flat-State oracles.
use super::*;
include!("reference.rs");
include!("identity_reference.rs");
#[path = "no_change_tests.rs"]
mod no_change;
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
fn page_pairs(a: &Rows, b: &Rows) -> usize {
    match (&a.storage, &b.storage) {
        (Storage::Pages { pages: a, .. }, Storage::Pages { pages: b, .. }) => a.len().min(b.len()),
        _ => 0,
    }
}
fn same_pages(a: &Rows, b: &Rows) -> usize {
    match (&a.storage, &b.storage) {
        (Storage::Pages { pages: a, .. }, Storage::Pages { pages: b, .. }) => {
            a.iter().zip(b).filter(|(a, b)| Arc::ptr_eq(a, b)).count()
        }
        _ => 0,
    }
}
#[test]
fn retirement_identity_same_pages_preserve_complete_state_and_later_cow_aliases() {
    for count in [0, 1, 4, 5, 15, 16, 17, 33, 72] {
        let flat = old(count, 1);
        let seed = convert(&flat);
        let from = seed.clone();
        let mut actual = seed.clone();
        let alias = seed.clone();
        let mut eager = seed.clone();
        let mut expected = flat.clone();
        let a = old_merge(&mut expected, &flat, &mut budget(10_000_000));
        let b = eager_state_merge(&mut eager, &from, &mut budget(10_000_000));
        let c = merge(&mut actual, &from, &mut budget(10_000_000));
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert!(!c.unwrap());
        assert_eq!(snapshot(&actual), expected);
        assert_eq!(snapshot(&eager), expected);
        assert_eq!(snapshot(&alias), flat);
        assert_eq!(snapshot(&from), flat);
        assert_eq!(
            same_pages(&actual.retirement, &from.retirement),
            page_pairs(&actual.retirement, &from.retirement)
        );
        if count > SMALL {
            assert_eq!(same_pages(&eager.retirement, &from.retirement), 0);
        }
        if count > 0 {
            actual
                .retirement
                .replace(count / 2, source(1001), &mut budget(10_000_000))
                .unwrap();
            assert_eq!(snapshot(&alias), flat);
            assert_eq!(snapshot(&from), flat);
            assert_ne!(snapshot(&actual), flat);
        }
    }
}
#[test]
fn retirement_identity_different_pages_and_mixed_lengths_keep_full_eager_fallback() {
    const ROOM: usize = 10_000_000;
    for left in [0, 1, 4, 5, 16, 17, 33, 72] {
        for right in [0, 1, 4, 5, 16, 17, 33, 72] {
            let original = old(left, 1);
            let incoming = old(right, 2);
            let seed = convert(&original);
            let from = convert(&incoming);
            let mut actual = seed.clone();
            let mut eager = seed.clone();
            let mut flat = original.clone();
            let mut old_b = budget(ROOM);
            let old_result = eager_state_merge(&mut eager, &from, &mut old_b);
            let mut new_b = budget(ROOM);
            let result = merge(&mut actual, &from, &mut new_b);
            assert_eq!(result, old_result);
            assert_eq!(result, old_merge(&mut flat, &incoming, &mut budget(ROOM)));
            assert_eq!(snapshot(&actual), flat);
            assert_eq!(snapshot(&eager), flat);
            assert_eq!(snapshot(&seed), original);
            assert_eq!(snapshot(&from), incoming);
            assert_eq!(same_pages(&seed.retirement, &from.retirement), 0);
            assert!(
                old_b.remaining - new_b.remaining >= page_pairs(&seed.retirement, &from.retirement),
                "identity and no-change checks are additional actual work; the complete false path stays paid"
            );
        }
    }
}
#[test]
fn retirement_identity_partial_aliases_and_long_paths_skip_only_identical_pages() {
    let original = old(33, 1);
    let seed = convert(&original);
    let mut from = seed.clone();
    let mut value = source(99);
    value.direct.insert(actual::Target {
        base: actual::Base::Input(77),
        steps: vec![PlaceProjection::Field(3); 129],
        mutable: true,
    });
    value.captures.insert(91, actual::Targets::new());
    from.retirement
        .replace(17, value, &mut budget(10_000_000))
        .unwrap();
    assert_eq!(same_pages(&seed.retirement, &from.retirement), 2);
    let incoming = snapshot(&from);
    let mut flat = original.clone();
    let mut eager = seed.clone();
    let mut actual = seed.clone();
    assert!(old_merge(&mut flat, &incoming, &mut budget(10_000_000)).unwrap());
    assert!(eager_state_merge(&mut eager, &from, &mut budget(10_000_000)).unwrap());
    assert!(merge(&mut actual, &from, &mut budget(10_000_000)).unwrap());
    assert_eq!(snapshot(&actual), flat);
    assert_eq!(snapshot(&eager), flat);
    assert_eq!(snapshot(&seed), original);
    assert_eq!(snapshot(&from), incoming);
    assert_eq!(same_pages(&actual.retirement, &from.retirement), 2);
}
#[test]
fn retirement_identity_exact_state_budget_and_changed_prefix_survive_failed_guard() {
    const ROOM: usize = 10_000_000;
    let original = old(33, 1);
    let seed = convert(&original);
    let from = seed.clone();
    let run = |limit| {
        let mut actual = seed.clone();
        let mut b = budget(limit);
        let result = merge(&mut actual, &from, &mut b);
        (result, actual, limit - b.remaining)
    };
    let (result, actual, n) = run(ROOM);
    assert!(!result.unwrap());
    assert_eq!(snapshot(&actual), original);
    let mut eager = seed.clone();
    let mut b = budget(ROOM);
    assert!(!eager_state_merge(&mut eager, &from, &mut b).unwrap());
    let old_n = ROOM - b.remaining;
    assert!(
        n < old_n,
        "actual same-page State request {n} >= eager {old_n}"
    );
    for limit in 0..n {
        let (result, actual, _) = run(limit);
        assert_eq!(
            result.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(snapshot(&actual), original);
        assert_eq!(same_pages(&actual.retirement, &from.retirement), 3);
    }
    run(n).0.unwrap();
    run(n).0.unwrap();
    // A genuine earlier page union is retained if the next page's new paid
    // guard fails. Do not claim equal partial States at equal old/new limits.
    let mut changed = seed.retirement.clone();
    changed.replace(0, source(1001), &mut budget(ROOM)).unwrap();
    let first = |rows: &Rows| match &rows.storage {
        Storage::Pages { pages, .. } => Rows {
            storage: Storage::Pages {
                len: PAGE,
                pages: vec![pages[0].clone()],
            },
        },
        _ => unreachable!(),
    };
    let mut prefix = first(&seed.retirement);
    let input = first(&changed);
    let mut b = budget(ROOM);
    assert!(prefix.merge(&input, &mut b).unwrap());
    let first_n = ROOM - b.remaining;
    let mut actual = seed.retirement.clone();
    let error = actual
        .merge(&changed, &mut budget(first_n + 2))
        .unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    let mut expected = seed.retirement.dense();
    let input = changed.dense();
    for (target, source) in expected[..PAGE].iter_mut().zip(&input[..PAGE]) {
        target.merge(source, &mut budget(ROOM)).unwrap();
    }
    assert_eq!(actual.dense(), expected);
    assert_eq!(seed.retirement.dense(), original.retirement);
    assert_eq!(same_pages(&actual, &seed.retirement), 2);
    eprintln!(
        "same-Arc complete State N={n}, eager N={old_n}; all lower budgets preserve facts, fresh N succeeds; next-page guard failure preserves changed prefix"
    );
}
