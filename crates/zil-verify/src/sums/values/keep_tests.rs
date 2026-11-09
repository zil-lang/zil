//! Frozen 22da eager paged merge; independent storage oracle, not the new probe.
use super::*;
impl Values {
    fn merge_eager(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        budget.spend(1)?;
        if self.len != other.len {
            return Err(bad(None, "Sum SSA logical inventories differ"));
        }
        // BOTH complete inventories precede every identity shortcut.
        self.validate(budget)?;
        other.validate(budget)?;
        let mut changed = false;
        for (left, right) in self.pages.iter_mut().zip(&other.pages) {
            budget.spend(1)?;
            if Arc::ptr_eq(left, right) {
                continue;
            }
            // State::cost now pays for page references. All actual different-
            // page slot walks must therefore be charged here independently.
            budget.spend(left.roots.len())?;
            budget.spend(right.roots.len())?;
            let page = Self::edit_page(left, budget)?;
            for (a, b) in page.roots.iter_mut().zip(&right.roots) {
                // Keep this kernel's ORIGINAL per-Content alias/COW/merge.
                // Page sharing cannot replace its external alias test.
                if !Arc::ptr_eq(a, b) {
                    if Arc::strong_count(a) > 1 {
                        a.cost(budget)?;
                    }
                    changed |= Arc::make_mut(a).merge(b, budget)?;
                }
            }
        }
        Ok(changed)
    }
}
fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn values(roots: Vec<Content>) -> Values {
    Values::build(roots.len(), &mut budget(1_000_000), |index, _| {
        Ok(Arc::new(roots[index].clone()))
    })
    .unwrap()
}
fn sample() -> Vec<Content> {
    let source = Source::Input(0, vec![0, 1, 2, 3]);
    let leaf = Content::Leaf(BTreeSet::from([Source::Fresh, source]));
    vec![
        Content::Copy,
        leaf.clone(),
        Content::Missing(Box::new(leaf.clone())),
        Content::Product(vec![Content::Copy, leaf.clone()]),
        Content::Sum(
            BTreeMap::from([(0, vec![leaf.clone()]), (2, vec![Content::Copy])]),
            None,
        ),
        Content::Sum(BTreeMap::from([(0, vec![leaf])]), Some((3, vec![1, 0, 0]))),
    ]
}
fn roots(n: usize, seed: usize) -> Vec<Content> {
    let sample = sample();
    (0..n)
        .map(|i| sample[(i + seed) % sample.len()].clone())
        .collect()
}
#[test]
fn shared_page_keep_matches_frozen_eager_complete_inventory_and_repeated_merge() {
    for len in [0, 1, 31, 32, 33, 63, 64, 65] {
        for shared in [false, true] {
            let mut old = values(roots(len, 0));
            let mut new = values(roots(len, 0));
            let old_alias = shared.then(|| old.clone());
            let new_alias = shared.then(|| new.clone());
            // Independent Content allocations, rather than root pointer identity.
            let incoming = values(roots(len, 0));
            for _ in 0..2 {
                assert_eq!(
                    old.merge_eager(&incoming, &mut budget(1_000_000)),
                    new.merge(&incoming, &mut budget(1_000_000))
                );
                assert_eq!(old, new);
                if let (Some(a), Some(b)) = (&old_alias, &new_alias) {
                    assert_eq!(a, b);
                }
            }
            if len > 0 {
                let old_external = old.get(0, &mut budget(100)).unwrap().clone();
                let new_external = new.get(0, &mut budget(100)).unwrap().clone();
                let snapshot = (*new_external).clone();
                let replacement = Arc::new(Content::Leaf(BTreeSet::from([Source::Unknown])));
                old.set(0, replacement.clone(), &mut budget(1_000_000))
                    .unwrap();
                new.set(0, replacement, &mut budget(1_000_000)).unwrap();
                assert_eq!(old, new);
                assert_eq!(old_external, new_external);
                assert_eq!(*new_external, snapshot);
                if let (Some(a), Some(b)) = (&old_alias, &new_alias) {
                    assert_eq!(a, b);
                }
            }
        }
    }
}
#[test]
fn shared_page_keep_handles_missing_routes_sources_and_zip_fallback() {
    let leaf = |sources| Content::Leaf(sources);
    let route = Some((3, vec![1, 0, 0]));
    let sum = |route, fields| Content::Sum(BTreeMap::from([(0, fields)]), route);
    let cases = vec![
        (
            Content::Missing(Box::new(Content::Copy)),
            Content::Copy,
            true,
        ),
        (
            Content::Copy,
            Content::Missing(Box::new(Content::Copy)),
            false,
        ),
        (
            Content::Missing(Box::new(Content::Copy)),
            Content::Missing(Box::new(Content::Missing(Box::new(Content::Copy)))),
            false,
        ),
        (
            leaf(BTreeSet::from([
                Source::Fresh,
                Source::Input(0, vec![1, 2]),
                Source::Captured(1, vec![3]),
                Source::ReturnedInput(2, vec![4]),
            ])),
            leaf(BTreeSet::from([
                Source::Input(0, vec![1, 2]),
                Source::ReturnedInput(2, vec![4]),
            ])),
            true,
        ),
        (
            leaf(BTreeSet::from([Source::Input(0, vec![1, 2])])),
            leaf(BTreeSet::from([Source::Input(0, vec![1, 3])])),
            false,
        ),
        (
            sum(None, vec![Content::Copy]),
            sum(route.clone(), vec![Content::Copy]),
            true,
        ),
        (
            sum(route.clone(), vec![Content::Copy]),
            sum(None, vec![Content::Copy]),
            false,
        ),
        (
            sum(route.clone(), vec![Content::Copy]),
            sum(route.clone(), vec![Content::Copy]),
            true,
        ),
        (
            sum(route.clone(), vec![Content::Copy]),
            sum(Some((3, vec![1, 0, 1])), vec![Content::Copy]),
            false,
        ),
        (
            Content::Sum(BTreeMap::new(), route.clone()),
            sum(route.clone(), vec![Content::Copy]),
            false,
        ),
        (
            sum(route.clone(), vec![Content::Copy]),
            Content::Sum(BTreeMap::new(), None),
            true,
        ),
        (
            Content::Product(vec![Content::Copy, Content::Copy]),
            Content::Product(vec![Content::Copy]),
            false,
        ),
        (
            sum(route.clone(), vec![Content::Copy]),
            sum(route.clone(), vec![Content::Copy, Content::Copy]),
            false,
        ),
        (Content::Copy, Content::Product(vec![]), false),
    ];
    for (left, right, keep) in cases {
        assert_eq!(
            keeps_content(&left, &right, &mut budget(1_000_000)).unwrap(),
            keep
        );
        let mut old = values(vec![left.clone()]);
        let mut new = values(vec![left]);
        let old_alias = old.clone();
        let new_alias = new.clone();
        let incoming = values(vec![right]);
        let a = old.merge_eager(&incoming, &mut budget(1_000_000));
        let b = new.merge(&incoming, &mut budget(1_000_000));
        assert_eq!(a, b);
        assert_eq!(old, new);
        assert_eq!(old_alias, new_alias);
        if keep {
            assert!(!a.unwrap());
        }
    }
}
#[test]
fn shared_page_keep_preserves_semantic_error_prefix_and_external_aliases() {
    for len in [2, 33, 65] {
        for shared in [false, true] {
            let mut left = vec![Content::Copy; len];
            let mut right = left.clone();
            left[0] = Content::Leaf(BTreeSet::from([Source::Fresh]));
            right[0] = Content::Missing(Box::new(Content::Leaf(BTreeSet::from([Source::Unknown]))));
            right[len - 1] = Content::Product(vec![Content::Copy]);
            let mut old = values(left.clone());
            let mut new = values(left);
            let old_alias = shared.then(|| old.clone());
            let new_alias = shared.then(|| new.clone());
            let external = new.get(0, &mut budget(100)).unwrap().clone();
            let snapshot = (*external).clone();
            let incoming = values(right);
            let a = old.merge_eager(&incoming, &mut budget(1_000_000));
            let b = new.merge(&incoming, &mut budget(1_000_000));
            assert_eq!(
                a.as_ref().unwrap_err().category,
                zil_base::ResultCategory::SafeReject
            );
            assert_eq!(a, b);
            assert_eq!(old, new);
            assert!(matches!(
                new.get(0, &mut budget(100)).unwrap().as_ref(),
                Content::Missing(_)
            ));
            assert_eq!(*external, snapshot);
            if let (Some(a), Some(b)) = (old_alias, new_alias) {
                assert_eq!(a, b);
            }
        }
    }
}
#[test]
fn shared_page_keep_validates_all_pages_before_identity_or_keep() {
    for corrupt_left in [false, true] {
        for kind in 0..3 {
            let mut left = values(vec![Content::Copy; 33]);
            let mut right = left.clone();
            if corrupt_left {
                left.corrupt_shape(kind);
            } else {
                right.corrupt_shape(kind);
            }
            let mut old = left.clone();
            let a = old.merge_eager(&right, &mut budget(1_000_000));
            let b = left.merge(&right, &mut budget(1_000_000));
            assert_eq!(
                a.as_ref().unwrap_err().category,
                zil_base::ResultCategory::SafeReject
            );
            assert_eq!(a, b);
            assert_eq!(old, left);
        }
    }
}
#[test]
fn shared_page_keep_has_exact_component_n_minus_one_and_fresh_boundaries() {
    let run = |eager: bool, n| {
        let seed = values(vec![Content::Copy; 65]);
        let mut left = seed.clone();
        let incoming = values(vec![Content::Copy; 65]);
        let mut b = budget(n);
        let changed = if eager {
            left.merge_eager(&incoming, &mut b)?
        } else {
            left.merge(&incoming, &mut b)?
        };
        assert!(!changed);
        assert_eq!(left, seed);
        Ok::<_, Diagnostic>(left)
    };
    let boundary = |eager| {
        let (mut low, mut high) = (0, 100_000);
        assert!(run(eager, high).is_ok());
        while low < high {
            let middle = (low + high) / 2;
            if run(eager, middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        low
    };
    let new_n = boundary(false);
    let old_n = boundary(true);
    assert!(new_n < old_n, "shared keep N={new_n}, eager N={old_n}");
    assert_eq!(run(false, new_n).unwrap(), run(true, old_n).unwrap());
    assert_eq!(run(false, new_n).unwrap(), run(false, new_n).unwrap());
    assert_eq!(
        run(false, new_n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(
        run(true, old_n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    eprintln!("V shared-page keep N={new_n}, eager N={old_n}; N-1/fresh checked");
}
