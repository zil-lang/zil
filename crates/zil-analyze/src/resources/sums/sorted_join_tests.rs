use super::*;
// Frozen lookup traversal; the Tree lattice and observation merge are unchanged.
fn lookup_join(left: &mut Facts, right: &Facts, b: &mut Budget) -> CompileResult<bool> {
    if left.locals.is_empty() && left.captures.is_empty() {
        return Ok(false);
    }
    let mut changed = false;
    if let (Some(a), Some(c)) = (&mut left.partial, &right.partial) {
        changed |= a.join(c, b)?;
    }
    for (key, t) in &right.locals {
        b.lookup(left.locals.len(), "Sum local join")?;
        changed |= left
            .locals
            .get_mut(key)
            .ok_or_else(|| error("Sum local missing"))?
            .merge(t, b)?;
    }
    for (key, t) in &right.values {
        b.lookup(left.values.len(), "Sum value join")?;
        changed |= left
            .values
            .get_mut(key)
            .ok_or_else(|| error("Sum value missing"))?
            .merge(t, b)?;
    }
    for (key, t) in &right.captures {
        b.lookup(left.captures.len(), "Sum capture join")?;
        changed |= left
            .captures
            .get_mut(key)
            .ok_or_else(|| error("Sum capture missing"))?
            .merge(t, b)?;
    }
    b.spend(
        left.valid.len() + right.valid.len(),
        "Sum observations must join",
    )?;
    let before = left.valid.len();
    left.valid.retain(|v| right.valid.contains(v));
    Ok(changed || before != left.valid.len())
}
fn leaf(missing: bool, birth: Birth) -> Arc<Tree> {
    Arc::new(Tree {
        missing,
        node: Node::Leaf(BTreeSet::from([birth])),
    })
}
fn nested(missing: bool, variant: u32, birth: Birth) -> Arc<Tree> {
    Arc::new(Tree {
        missing,
        node: Node::Product(vec![
            leaf(false, Birth::Incoming),
            Arc::new(Tree {
                missing: false,
                node: Node::Sum(BTreeMap::from([(variant, vec![leaf(missing, birth)])])),
            }),
        ]),
    })
}
fn facts(count: u32, variant: u32, birth: Birth) -> Facts {
    Facts {
        locals: BTreeMap::from([(LocalId(0), nested(false, variant, birth))]),
        values: (0..count)
            .map(|i| {
                (
                    ValueId(i),
                    if i % 2 == 0 {
                        leaf(i % 3 == 0, birth)
                    } else {
                        nested(false, variant, birth)
                    },
                )
            })
            .collect(),
        captures: BTreeMap::from([(0, nested(false, variant, birth))]),
        valid: BTreeSet::from([ValueId(1), ValueId(variant + 2)]),
        partial: None,
    }
}
#[test]
fn sorted_join_preserves_complete_nested_trees_origins_and_observations() {
    for count in [0, 1, 3, 64] {
        for birth in [Birth::Created, Birth::Argument(1), Birth::Captured(2)] {
            let left = facts(count, 0, Birth::Incoming);
            let right = facts(count, 1, birth);
            let mut sorted = left.clone();
            let mut lookup = left;
            let a = sorted
                .join(&right, &mut Budget::new(&Limits::default()))
                .unwrap();
            let c = lookup_join(&mut lookup, &right, &mut Budget::new(&Limits::default())).unwrap();
            assert_eq!(a, c);
            assert_eq!(sorted, lookup);
            assert_eq!(right, facts(count, 1, birth));
        }
    }
    let mut empty = Facts::default();
    assert!(
        !empty
            .join(&Facts::default(), &mut Budget::new(&Limits::default()))
            .unwrap()
    );
    let mut table: BTreeMap<u32, Arc<Tree>> = BTreeMap::new();
    assert!(
        !join_tables(
            &mut table,
            &BTreeMap::new(),
            &mut Budget::new(&Limits::default())
        )
        .unwrap()
    );
}
#[test]
fn sorted_join_rejects_wrong_keys_and_missing_branch_slots() {
    let left = facts(3, 0, Birth::Incoming);
    let mut wrong = facts(3, 1, Birth::Created);
    let tree = wrong.values.remove(&ValueId(1)).unwrap();
    wrong.values.insert(ValueId(9), tree);
    for sorted in [false, true] {
        let mut copy = left.clone();
        let mut b = Budget::new(&Limits::default());
        let errors = if sorted {
            copy.join(&wrong, &mut b)
        } else {
            lookup_join(&mut copy, &wrong, &mut b)
        }
        .unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.category == zil_base::ResultCategory::SafeReject)
        );
    }
    for table in [0, 1, 2] {
        let mut missing = left.clone();
        match table {
            0 => {
                missing.locals.remove(&LocalId(0));
            }
            1 => {
                missing.values.remove(&ValueId(1));
            }
            _ => {
                missing.captures.remove(&0);
            }
        }
        let errors = left
            .clone()
            .join(&missing, &mut Budget::new(&Limits::default()))
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("inventory differs")),
            "{errors:?}"
        );
        // The old traversal accepted a broken prefix. It is not a valid
        // same-function state; the complete new key check explicitly rejects it.
        lookup_join(
            &mut left.clone(),
            &missing,
            &mut Budget::new(&Limits::default()),
        )
        .unwrap();
    }
}
#[test]
fn sorted_join_has_an_exact_smaller_request_boundary() {
    let left = facts(256, 0, Birth::Incoming);
    let right = facts(256, 1, Birth::Created);
    let run = |sorted, work| {
        let mut state = left.clone();
        let mut b = Budget::new(&Limits {
            max_analysis_steps: work,
            ..Limits::default()
        });
        if sorted {
            state.join(&right, &mut b)
        } else {
            lookup_join(&mut state, &right, &mut b)
        }
    };
    let minimum = |sorted| {
        let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if run(sorted, mid).is_ok() {
                hi = mid
            } else {
                lo = mid + 1
            }
        }
        run(sorted, lo).unwrap();
        assert!(
            run(sorted, lo - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
        );
        run(sorted, lo).unwrap();
        lo
    };
    let sorted = minimum(true);
    let lookup = minimum(false);
    assert!(sorted < lookup, "sorted={sorted} lookup={lookup}");
    println!(
        "Sum full inventory join: sorted N={sorted}, lookup N={lookup}; N-1 rejects, retry N passes"
    );
}
