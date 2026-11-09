use super::*;
use zil_base::ResultCategory;

fn budget() -> Budget {
    Budget::new(&Limits::default())
}
fn leaf(missing: bool, birth: Birth) -> Arc<Tree> {
    Arc::new(Tree {
        missing,
        node: Node::Leaf(BTreeSet::from([birth])),
    })
}
fn tree(index: usize, incoming: bool) -> Arc<Tree> {
    let birth = if incoming {
        Birth::Created
    } else {
        Birth::Incoming
    };
    if index.is_multiple_of(2) {
        leaf(index.is_multiple_of(3), birth)
    } else {
        Arc::new(Tree {
            missing: false,
            node: Node::Product(vec![
                leaf(false, birth),
                Arc::new(Tree {
                    missing: false,
                    node: Node::Sum(BTreeMap::from([(
                        u32::from(incoming),
                        vec![leaf(false, birth)],
                    )])),
                }),
            ]),
        })
    }
}
fn keyed(len: usize, incoming: bool) -> BTreeMap<ValueId, Arc<Tree>> {
    (0..len)
        .map(|i| (ValueId(i as u32), tree(i, incoming)))
        .collect()
}
fn values(len: usize, incoming: bool) -> Values {
    Values::from_keyed(len, keyed(len, incoming), &mut budget()).unwrap()
}
// Frozen BTreeMap representation: exactly the former lookup + Tree operation.
fn old_join(
    left: &mut BTreeMap<ValueId, Arc<Tree>>,
    right: &BTreeMap<ValueId, Arc<Tree>>,
    b: &mut Budget,
) -> CompileResult<bool> {
    let mut changed = false;
    for (id, t) in right {
        b.lookup(left.len(), "Sum value join")?;
        changed |= left
            .get_mut(id)
            .ok_or_else(|| error("Sum value missing"))?
            .merge(t, b)?;
    }
    Ok(changed)
}
#[test]
fn canonical_pages_match_old_values_across_boundaries_and_repeated_joins() {
    for len in [0, 1, 31, 32, 33, 63, 64, 65] {
        let mut paged = values(len, false);
        let mut old = keyed(len, false);
        let frozen = paged.clone();
        let frozen_keyed = old.clone();
        assert_eq!(paged.len(), len);
        assert_eq!(paged.page_count(), len.div_ceil(WIDTH));
        let mut b = budget();
        // Cross-page writes and Tree edits mutate only this state. The saved
        // state still owns shared pages and shared Trees throughout the test.
        for index in (0..len).filter(|i| i % 7 == 0 || *i == 31 || *i == 32 || *i == 64) {
            let id = ValueId(index as u32);
            let replacement = tree(index, true);
            paged.set(id, replacement.clone(), &mut b).unwrap();
            old.insert(id, replacement);
            let t = paged.get_mut(&id, &mut b).unwrap().unwrap();
            let ot = old.get_mut(&id).unwrap();
            edit(t, &mut b).unwrap().missing = true;
            edit(ot, &mut b).unwrap().missing = true;
            if let Node::Leaf(births) = &mut edit(t, &mut b).unwrap().node {
                births.insert(Birth::Argument(2));
            }
            if let Node::Leaf(births) = &mut edit(ot, &mut b).unwrap().node {
                births.insert(Birth::Argument(2));
            }
        }
        assert_eq!(paged.keyed(), old);
        assert_eq!(frozen.keyed(), frozen_keyed);
        for incoming in [true, false, true] {
            let right = values(len, incoming);
            let old_right = keyed(len, incoming);
            let old_changed = old_join(&mut old, &old_right, &mut budget()).unwrap();
            let changed = paged.join(&right, &mut budget()).unwrap();
            assert_eq!(changed, old_changed, "len={len} incoming={incoming}");
            assert_eq!(paged.keyed(), old);
            assert_eq!(right.keyed(), old_right);
            assert_eq!(frozen.keyed(), frozen_keyed);
        }
        let clone = paged.clone();
        assert!(!paged.join(&clone, &mut budget()).unwrap());
        assert!(!old_join(&mut old, &clone.keyed(), &mut budget()).unwrap());
        assert_eq!(paged.keyed(), old);
    }
}
#[test]
fn mutable_slot_clones_page_then_tree_without_changing_sibling_state() {
    let seed = values(65, false);
    let mut changed = seed.clone();
    let mut b = budget();
    let id = ValueId(32);
    let slot = changed.get_mut(&id, &mut b).unwrap().unwrap();
    assert!(Arc::ptr_eq(slot, seed.get(&id).unwrap()));
    let t = edit(slot, &mut b).unwrap();
    t.missing = !t.missing;
    let Node::Leaf(births) = &mut t.node else {
        panic!("leaf at slot32")
    };
    births.insert(Birth::Captured(9));
    assert!(!Arc::ptr_eq(&changed.pages[1], &seed.pages[1]));
    assert!(!Arc::ptr_eq(
        changed.get(&id).unwrap(),
        seed.get(&id).unwrap()
    ));
    for page in [0, 2] {
        assert!(Arc::ptr_eq(&changed.pages[page], &seed.pages[page]));
    }
    assert_eq!(seed.keyed(), keyed(65, false));
    assert_ne!(changed.keyed(), seed.keyed());
}
#[test]
fn keyed_construction_rejects_holes_wrong_keys_and_length_instead_of_renumbering() {
    for len in [1, 32, 33, 65] {
        let mut wrong = keyed(len, false);
        let tree = wrong.remove(&ValueId((len / 2) as u32)).unwrap();
        wrong.insert(ValueId((len + 10) as u32), tree);
        assert_eq!(wrong.len(), len);
        let errors = Values::from_keyed(len, wrong, &mut budget()).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::SafeReject)
        );
        let mut hole = keyed(len, false);
        hole.remove(&ValueId(0));
        assert!(Values::from_keyed(len, hole, &mut budget()).is_err());
    }
}
#[test]
fn join_checks_every_page_shape_before_any_shared_identity_shortcut() {
    let seed = values(65, false);
    let mut broken = Vec::new();
    let mut wrong_base = seed.clone();
    Arc::make_mut(&mut wrong_base.pages[2]).base = 32;
    broken.push(wrong_base);
    let mut wrong_width = seed.clone();
    Arc::make_mut(&mut wrong_width.pages[1]).trees.pop();
    broken.push(wrong_width);
    let mut extra = seed.clone();
    extra.pages.push(Arc::new(Page {
        base: 96,
        trees: vec![],
    }));
    broken.push(extra);
    let mut missing = seed.clone();
    missing.pages.pop();
    broken.push(missing);
    for malformed in broken {
        // Shared malformed pages must not become trusted merely because both
        // states point to the identical Arc, nor after a valid first page.
        for (mut left, right) in [
            (seed.clone(), malformed.clone()),
            (malformed.clone(), seed.clone()),
            (malformed.clone(), malformed),
        ] {
            assert!(
                left.join(&right, &mut budget())
                    .unwrap_err()
                    .iter()
                    .all(|e| e.category == ResultCategory::SafeReject)
            );
        }
    }
    assert!(
        seed.clone()
            .join(&values(64, false), &mut budget())
            .is_err()
    );
}
#[test]
fn out_of_inventory_access_rejects_without_creating_new_ssa_slots() {
    let mut state = values(33, false);
    let seed = state.clone();
    for id in [ValueId(33), ValueId(u32::MAX)] {
        assert!(state.get(&id).is_none());
        assert!(state.get_mut(&id, &mut budget()).unwrap().is_none());
        assert!(state.set(id, tree(0, true), &mut budget()).is_err());
        assert_eq!(state, seed);
    }
}
#[test]
fn different_tree_shapes_keep_the_old_safe_rejection() {
    let mut old = keyed(33, false);
    let mut old_right = old.clone();
    old_right.insert(
        ValueId(32),
        Arc::new(Tree {
            missing: false,
            node: Node::Product(vec![leaf(false, Birth::Created)]),
        }),
    );
    let mut state = Values::from_keyed(33, old.clone(), &mut budget()).unwrap();
    let right = Values::from_keyed(33, old_right.clone(), &mut budget()).unwrap();
    let old_error = old_join(&mut old, &old_right, &mut budget()).unwrap_err();
    let new_error = state.join(&right, &mut budget()).unwrap_err();
    assert_eq!(old_error, new_error);
}
#[test]
fn page_copy_has_exact_prepaid_boundary_and_retry_is_fresh() {
    let seed = values(65, false);
    let run = |n| {
        let mut state = seed.clone();
        let mut b = Budget::new(&Limits {
            max_analysis_steps: n,
            ..Limits::default()
        });
        state.set(ValueId(32), tree(32, true), &mut b)?;
        Ok::<_, Vec<zil_base::Diagnostic>>(state)
    };
    // One checked index, one copied page header, 32 real Tree references.
    assert!(
        run(33)
            .unwrap_err()
            .iter()
            .all(|e| e.category == ResultCategory::ResourceLimit)
    );
    let first = run(34).unwrap();
    assert_eq!(first, run(34).unwrap());
    assert_eq!(seed.keyed(), keyed(65, false));
}

#[test]
fn single_side_missing_growth_and_nested_births_match_frozen_lookup() {
    let mut left = values(33, false);
    let mut right = left.clone();
    let mut b = budget();
    let slot = right.get_mut(&ValueId(31), &mut b).unwrap().unwrap();
    let node = edit(slot, &mut b).unwrap();
    node.missing = true;
    let Node::Product(fields) = &mut node.node else {
        panic!("nested slot")
    };
    let Node::Leaf(births) = &mut edit(&mut fields[0], &mut b).unwrap().node else {
        panic!("leaf")
    };
    births.insert(Birth::Captured(7));
    let mut old = left.keyed();
    assert!(old_join(&mut old, &right.keyed(), &mut budget()).unwrap());
    assert!(left.join(&right, &mut budget()).unwrap());
    assert_eq!(left.keyed(), old);
    assert!(!left.join(&right, &mut budget()).unwrap());
}

// Frozen 147 eager page merge: complete original validation, page COW and
// every original Tree merge. It never calls the readonly keep predicate.
fn eager_join(left: &mut Values, right: &Values, budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(1, "Sum SSA logical inventory comparison")?;
    if left.len != right.len {
        return Err(error("Sum joined state inventory differs"));
    }
    // Validate ALL pages on both sides before even the identity fast path.
    left.validate(budget)?;
    right.validate(budget)?;
    let mut changed = false;
    for (left, right) in left.pages.iter_mut().zip(&right.pages) {
        budget.spend(1, "Sum SSA shared page identity")?;
        if Arc::ptr_eq(left, right) {
            continue;
        }
        let page = Values::edit_page(left, budget)?;
        budget.product(page.trees.len(), 2, 1, "Sum SSA page slot walk")?;
        for (target, incoming) in page.trees.iter_mut().zip(&right.trees) {
            changed |= target.merge(incoming, budget)?;
        }
    }
    Ok(changed)
}

fn deep_copy(tree: &Arc<Tree>) -> Arc<Tree> {
    let node = match &tree.node {
        Node::Copy => Node::Copy,
        Node::Leaf(origins) => Node::Leaf(origins.clone()),
        Node::Product(fields) => Node::Product(fields.iter().map(deep_copy).collect()),
        Node::Sum(choices) => Node::Sum(
            choices
                .iter()
                .map(|(tag, fields)| (*tag, fields.iter().map(deep_copy).collect()))
                .collect(),
        ),
    };
    Arc::new(Tree {
        missing: tree.missing,
        node,
    })
}
fn subset(tree: &Arc<Tree>) -> Arc<Tree> {
    let node = match &tree.node {
        Node::Copy => Node::Copy,
        Node::Leaf(origins) => Node::Leaf(origins.iter().take(1).copied().collect()),
        Node::Product(fields) => Node::Product(fields.iter().map(subset).collect()),
        Node::Sum(choices) => Node::Sum(
            choices
                .iter()
                .take(1)
                .map(|(tag, fields)| (*tag, fields.iter().map(subset).collect()))
                .collect(),
        ),
    };
    Arc::new(Tree {
        missing: false,
        node,
    })
}
fn mixed_values(len: usize) -> Values {
    Values::build(len, &mut budget(), |index, _| {
        Ok(match index % 4 {
            0 => Arc::new(Tree {
                missing: true,
                node: Node::Copy,
            }),
            1 => Arc::new(Tree {
                missing: true,
                node: Node::Leaf(BTreeSet::from([
                    Birth::Incoming,
                    Birth::Created,
                    Birth::Argument(3),
                ])),
            }),
            2 => Arc::new(Tree {
                missing: false,
                node: Node::Product(vec![tree(0, false), tree(1, false)]),
            }),
            _ => Arc::new(Tree {
                missing: true,
                node: Node::Sum(BTreeMap::from([
                    (0, vec![tree(0, false)]),
                    (1, vec![tree(1, false)]),
                ])),
            }),
        })
    })
    .unwrap()
}
#[test]
fn shared_readonly_keep_matches_complete_eager_pages_for_all_tree_shapes_and_subsets() {
    for len in [0, 1, 31, 32, 33, 63, 64, 65] {
        for narrowed in [false, true] {
            let seed = mixed_values(len);
            let frozen = format!("{seed:?}");
            let right = Values::build(len, &mut budget(), |index, _| {
                let old = seed.get(&ValueId(index as u32)).unwrap();
                Ok(if narrowed {
                    subset(old)
                } else {
                    deep_copy(old)
                })
            })
            .unwrap();
            let mut lazy = seed.clone();
            let mut eager = seed.clone();
            assert!(!lazy.join(&right, &mut budget()).unwrap());
            assert!(!eager_join(&mut eager, &right, &mut budget()).unwrap());
            assert_eq!(lazy, eager);
            for (a, b) in lazy.pages.iter().zip(&seed.pages) {
                assert!(Arc::ptr_eq(a, b));
            }
            assert_eq!(format!("{seed:?}"), frozen);
            assert!(!lazy.join(&right, &mut budget()).unwrap());
        }
    }
}
#[test]
fn unique_pages_keep_inplace_and_shared_new_information_matches_old_changes() {
    for len in [1, 31, 32, 33, 65] {
        for shared in [false, true] {
            let mut lazy = values(len, false);
            let mut eager = values(len, false);
            let snapshot = shared.then(|| lazy.clone());
            let old_snapshot = shared.then(|| eager.clone());
            let frozen = snapshot.as_ref().map(|x| format!("{x:?}"));
            let right = values(len, true);
            let a = lazy.join(&right, &mut budget()).unwrap();
            let b = eager_join(&mut eager, &right, &mut budget()).unwrap();
            assert_eq!(a, b);
            assert_eq!(lazy, eager);
            assert!(!lazy.join(&right, &mut budget()).unwrap());
            assert!(!eager_join(&mut eager, &right, &mut budget()).unwrap());
            assert_eq!(lazy, eager);
            if let Some(snapshot) = snapshot {
                assert_eq!(Some(format!("{snapshot:?}")), frozen);
            }
            drop(old_snapshot);
        }
    }
}
#[test]
fn semantic_shape_errors_keep_complete_old_prefix_mutations_and_alias_isolation() {
    for len in [33, 65] {
        for at in [1, len - 1] {
            for shared in [false, true] {
                let mut lazy = values(len, false);
                let mut eager = values(len, false);
                let snapshot = shared.then(|| lazy.clone());
                let old_snapshot = shared.then(|| eager.clone());
                let frozen = snapshot.as_ref().map(|x| format!("{x:?}"));
                let mut right = values(len, true);
                let malformed = if at % 2 == 0 {
                    Node::Product(vec![])
                } else {
                    Node::Leaf(BTreeSet::new())
                };
                right
                    .set(
                        ValueId(at as u32),
                        Arc::new(Tree {
                            missing: true,
                            node: malformed,
                        }),
                        &mut budget(),
                    )
                    .unwrap();
                let a = lazy.join(&right, &mut budget()).unwrap_err();
                let b = eager_join(&mut eager, &right, &mut budget()).unwrap_err();
                assert_eq!(a, b);
                assert_eq!(
                    lazy, eager,
                    "all earlier pages/slots and error-side missing updates must match"
                );
                assert!(lazy.get(&ValueId(at as u32)).unwrap().missing);
                if let Some(snapshot) = snapshot {
                    assert_eq!(Some(format!("{snapshot:?}")), frozen);
                }
                drop(old_snapshot);
            }
        }
    }
}
fn copies(len: usize, missing: bool) -> Values {
    Values::build(len, &mut budget(), |_, _| {
        Ok(Arc::new(Tree {
            missing,
            node: Node::Copy,
        }))
    })
    .unwrap()
}
#[test]
fn shared_keep_has_an_exact_smaller_boundary_and_unique_has_no_hidden_probe() {
    let seed = copies(65, true);
    let right = copies(65, false);
    let run = |lazy, shared, steps| {
        let mut left = if shared {
            seed.clone()
        } else {
            copies(65, true)
        };
        let mut b = Budget::new(&Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        });
        if lazy {
            left.join(&right, &mut b)
        } else {
            eager_join(&mut left, &right, &mut b)
        }
    };
    let minimum = |lazy, shared| {
        let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if run(lazy, shared, mid).is_ok() {
                hi = mid
            } else {
                lo = mid + 1
            }
        }
        assert!(!run(lazy, shared, lo).unwrap());
        assert!(
            run(lazy, shared, lo - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        assert!(!run(lazy, shared, lo).unwrap());
        lo
    };
    let new_shared = minimum(true, true);
    let old_shared = minimum(false, true);
    let new_unique = minimum(true, false);
    let old_unique = minimum(false, false);
    assert!(new_shared < old_shared);
    // Only the actually executed page-sharing decision is added for unique
    // pages; there is no readonly slot/subtree scan on that path.
    assert_eq!(new_unique, old_unique + seed.page_count());
    println!(
        "Shared Sum pages: readonly N={new_shared}, old eager N={old_shared}; unique new={new_unique}/old={old_unique}; N-1/fresh verified"
    );
}
#[test]
fn readonly_budget_failure_never_mutates_or_publishes_a_partially_checked_page() {
    let seed = copies(65, true);
    let right = copies(65, false);
    let frozen = format!("{seed:?}");
    for steps in 0..80 {
        let mut left = seed.clone();
        let mut b = Budget::new(&Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        });
        let result = left.join(&right, &mut b);
        if let Err(errors) = result {
            assert!(
                errors
                    .iter()
                    .all(|e| e.category == ResultCategory::ResourceLimit)
            );
            assert_eq!(left, seed);
            for (a, b) in left.pages.iter().zip(&seed.pages) {
                assert!(Arc::ptr_eq(a, b));
            }
        }
        assert_eq!(format!("{seed:?}"), frozen);
    }
}
