//! Complete Sources and State oracles for the shared, different-page proof.
use super::*;
include!("no_change_reference.rs");

const ROOM: usize = 10_000_000;

fn first(rows: &Rows) -> Rows {
    let Storage::Pages { pages, .. } = &rows.storage else {
        unreachable!()
    };
    Rows {
        storage: Storage::Pages {
            len: pages[0].len(),
            pages: vec![pages[0].clone()],
        },
    }
}

#[test]
fn retirement_no_change_equal_and_subset_pages_preserve_complete_state_and_aliases() {
    for count in [5, 15, 16, 17, 33, 72] {
        for subset in [false, true] {
            let incoming = old(count, 1);
            let mut original = incoming.clone();
            if subset {
                for row in original.retirement.iter_mut().step_by(3) {
                    row.direct.extend(source(1001).direct);
                    row.captures.insert(91, actual::Targets::new());
                }
            }
            let seed = convert(&original);
            let from = convert(&incoming);
            let mut actual = seed.clone();
            let mut previous = seed.clone();
            let mut flat = original.clone();
            assert_eq!(same_pages(&seed.retirement, &from.retirement), 0);
            let expected = old_merge(&mut flat, &incoming, &mut budget(ROOM));
            assert!(!expected.as_ref().unwrap());
            assert_eq!(
                identity_state_merge(&mut previous, &from, &mut budget(ROOM)),
                expected
            );
            assert_eq!(merge(&mut actual, &from, &mut budget(ROOM)), expected);
            assert_eq!(snapshot(&actual), flat);
            assert_eq!(snapshot(&previous), flat);
            assert_eq!(
                snapshot(&clone_state(&actual, &mut budget(ROOM)).unwrap()),
                old_clone_state(&flat, &mut budget(ROOM)).unwrap()
            );
            assert_eq!(flat, original);
            assert_eq!(snapshot(&seed), original);
            assert_eq!(snapshot(&from), incoming);
            assert_eq!(
                same_pages(&actual.retirement, &seed.retirement),
                page_pairs(&actual.retirement, &seed.retirement),
                "a fully unchanged page is not detached"
            );
            actual
                .retirement
                .replace(count / 2, source(2001), &mut budget(ROOM))
                .unwrap();
            assert_eq!(snapshot(&seed), original);
            assert_eq!(snapshot(&from), incoming);
        }
    }
}

#[test]
fn retirement_no_change_requires_capture_key_even_when_merge_reports_false() {
    let mut original = old(33, 1);
    original.retirement[17] = Sources::default();
    let mut incoming = original.clone();
    incoming.retirement[17]
        .captures
        .insert(97, actual::Targets::new());
    let seed = convert(&original);
    let from = convert(&incoming);
    let mut actual = seed.clone();
    let mut previous = seed.clone();
    let mut flat = original.clone();
    assert!(!old_merge(&mut flat, &incoming, &mut budget(ROOM)).unwrap());
    assert!(!identity_state_merge(&mut previous, &from, &mut budget(ROOM)).unwrap());
    assert!(!merge(&mut actual, &from, &mut budget(ROOM)).unwrap());
    assert_ne!(
        flat, original,
        "the empty capture key is still complete data"
    );
    assert_eq!(snapshot(&actual), flat);
    assert_eq!(snapshot(&previous), flat);
    assert!(actual.retirement.dense()[17].captures.contains_key(&97));
    assert_eq!(same_pages(&actual.retirement, &seed.retirement), 2);
    assert_eq!(snapshot(&seed), original);
    assert_eq!(snapshot(&from), incoming);
}

#[test]
fn retirement_no_change_keeps_full_recipe_identity_and_long_path_fallback() {
    let target = actual::Target {
        base: actual::Base::InputCapture(3, 7),
        steps: vec![PlaceProjection::Field(11); 129],
        mutable: false,
    };
    for lane in [false, true] {
        for kind in 0..5 {
            let mut original = old(33, 1);
            original.retirement[17] = if lane {
                Sources {
                    direct: actual::Targets::new(),
                    captures: BTreeMap::from([(4, BTreeSet::from([target.clone()]))]),
                }
            } else {
                Sources {
                    direct: BTreeSet::from([target.clone()]),
                    captures: actual::Captures::new(),
                }
            };
            let mut next = target.clone();
            match kind {
                0 => next.base = actual::Base::InputCapture(3, 8),
                1 => next.base = actual::Base::Environment(3),
                2 => next.steps[128] = PlaceProjection::Field(12),
                3 => next.mutable = true,
                _ => next.steps.push(PlaceProjection::Field(11)),
            }
            let mut incoming = original.clone();
            if lane {
                incoming.retirement[17]
                    .captures
                    .insert(4, BTreeSet::from([next]));
            } else {
                incoming.retirement[17].direct = BTreeSet::from([next]);
            }
            let seed = convert(&original);
            let from = convert(&incoming);
            let mut actual = seed.clone();
            let mut previous = seed.clone();
            let mut flat = original.clone();
            assert!(old_merge(&mut flat, &incoming, &mut budget(ROOM)).unwrap());
            assert!(identity_state_merge(&mut previous, &from, &mut budget(ROOM)).unwrap());
            assert!(merge(&mut actual, &from, &mut budget(ROOM)).unwrap());
            assert_eq!(snapshot(&actual), flat, "lane={lane} kind={kind}");
            assert_eq!(snapshot(&previous), flat);
            assert_eq!(snapshot(&seed), original);
            assert_eq!(snapshot(&from), incoming);
            assert_eq!(same_pages(&actual.retirement, &seed.retirement), 2);
        }
    }
}

#[test]
fn retirement_no_change_unique_mixed_and_unequal_shapes_retain_original_fallback() {
    for (left, right, shared) in [
        (3, 4, false),
        (3, 5, false),
        (5, 3, false),
        (33, 33, false),
        (5, 6, true),
        (6, 5, true),
        (15, 16, true),
        (16, 15, true),
    ] {
        let original = old(left, 1);
        let incoming = old(right, 2);
        let from = convert(&incoming);
        let mut actual = convert(&original);
        let mut previous = convert(&original);
        let aliases = shared.then(|| (actual.clone(), previous.clone()));
        let mut a = budget(ROOM);
        let mut p = budget(ROOM);
        let expected = identity_state_merge(&mut previous, &from, &mut p);
        assert_eq!(merge(&mut actual, &from, &mut a), expected);
        assert_eq!(snapshot(&actual), snapshot(&previous));
        let pairs = page_pairs(&actual.retirement, &from.retirement);
        assert_eq!(
            p.remaining - a.remaining,
            pairs * if shared { 2 } else { 1 },
            "only the real strong-count and unequal-length probes precede the complete old path"
        );
        if let Some((a, p)) = aliases {
            assert_eq!(snapshot(&a), original);
            assert_eq!(snapshot(&p), original);
        }
        assert_eq!(snapshot(&from), incoming);
    }
}

#[test]
fn retirement_no_change_exact_state_budget_and_failed_next_proof_keep_complete_facts() {
    let original = old(33, 1);
    let seed = convert(&original);
    let from = convert(&original);
    let run = |limit| {
        let mut actual = seed.clone();
        let mut b = budget(limit);
        let result = merge(&mut actual, &from, &mut b);
        (result, actual, limit - b.remaining)
    };
    let (result, actual, n) = run(ROOM);
    assert!(!result.unwrap());
    assert_eq!(snapshot(&actual), original);
    for limit in 0..n {
        let (result, actual, _) = run(limit);
        assert_eq!(
            result.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(snapshot(&actual), original);
        assert_eq!(same_pages(&actual.retirement, &seed.retirement), 3);
    }
    for _ in 0..2 {
        let (result, actual, spent) = run(n);
        assert!(!result.unwrap());
        assert_eq!(snapshot(&actual), original);
        assert_eq!(spent, n);
    }
    let mut changed = convert(&original).retirement;
    changed.replace(0, source(1001), &mut budget(ROOM)).unwrap();
    let mut prefix = first(&seed.retirement);
    let input = first(&changed);
    let mut b = budget(ROOM);
    assert!(prefix.merge(&input, &mut b).unwrap());
    let first_n = ROOM - b.remaining;
    let mut actual = seed.retirement.clone();
    let error = actual
        .merge(&changed, &mut budget(first_n + 4))
        .unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    let mut expected = seed.retirement.dense();
    for (to, from) in expected[..PAGE].iter_mut().zip(&changed.dense()[..PAGE]) {
        to.merge(from, &mut budget(ROOM)).unwrap();
    }
    assert_eq!(actual.dense(), expected);
    assert_eq!(same_pages(&actual, &seed.retirement), 2);
    assert_eq!(snapshot(&seed), original);
    eprintln!(
        "different-page no-change complete State N={n}; all lower budgets reject without mutation, fresh N passes, failed next proof keeps the changed first page"
    );
}

#[test]
fn retirement_no_change_complete_repeated_state_requests_measure_real_copy_work() {
    let original = old(72, 1);
    let seed = convert(&original);
    let from = convert(&original);
    let run = |prior, limit| {
        let mut actual = seed.clone();
        let mut b = budget(limit);
        let result: Check<()> = (|| {
            for _ in 0..16 {
                actual = clone_state(&actual, &mut b)?;
                let changed = if prior {
                    identity_state_merge(&mut actual, &from, &mut b)?
                } else {
                    merge(&mut actual, &from, &mut b)?
                };
                assert!(!changed);
            }
            Ok(())
        })();
        (result, actual, limit - b.remaining)
    };
    let (result, actual, n) = run(false, ROOM);
    result.unwrap();
    let (previous, old, old_n) = run(true, ROOM);
    previous.unwrap();
    assert_eq!(snapshot(&actual), original);
    assert_eq!(snapshot(&old), original);
    assert!(
        n < old_n,
        "complete State request {n} >= identity-only {old_n}"
    );
    assert_eq!(
        run(false, n - 1).0.unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    run(false, n).0.unwrap();
    run(false, n).0.unwrap();
    assert_eq!(snapshot(&seed), original);
    assert_eq!(snapshot(&from), original);
    eprintln!(
        "complete shared different-page clone+merge x16 N={n}, identity-only={old_n}; includes actual new proof and fallback fees"
    );
}
