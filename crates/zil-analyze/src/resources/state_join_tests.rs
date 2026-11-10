use super::*;
use zil_base::ResultCategory;

// Keep the complete old state independent of the production Order encoding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OldState {
    cells: BTreeMap<Place, Fact>,
    later: BTreeSet<(LocalId, LocalId)>,
    // Complete SSA Packages carry the actual birth of each direct Managed
    // field. Local Packages already have separate lifecycle cells. Unknown
    // producers keep Incoming; packaging itself never creates a resource.
    package_values: PackageValues,
    product_births: product::Origins,
    sums: sums::Facts,
    // Enabled only for the bounded borrowed whole-Package helper proof.
    whole_commits: Option<replacements::Effects>,
}

// Representation setup is outside the measured join, as the original State
// fixture construction was. Packed controls use real paid insertions.
fn current(state: OldState) -> State {
    let mut later = order::Order::default();
    let mut setup = Budget::new(&Limits::default());
    for edge in state.later {
        later
            .insert(edge, &mut setup, false, "state join fixture order")
            .unwrap();
    }
    State {
        cells: state.cells,
        later,
        package_values: state.package_values,
        product_births: state.product_births,
        sums: state.sums,
        whole_commits: state.whole_commits,
    }
}

// Compare all logical fields after the operation. Decode actual words here;
// neither the old merge nor its expected edges call production Order helpers.
fn snapshot(state: &State) -> OldState {
    let later = match &state.later {
        order::Order::Small(edges) => edges.clone(),
        order::Order::Packed { count, words } => {
            let mut edges = BTreeSet::new();
            for (&(before, word), &bits) in words {
                assert_ne!(bits, 0);
                for bit in 0..64 {
                    if bits & (1u64 << bit) != 0 {
                        edges.insert((before, LocalId((word << 6) + bit)));
                    }
                }
            }
            assert_eq!(edges.len(), *count);
            edges
        }
    };
    OldState {
        cells: state.cells.clone(),
        later,
        package_values: state.package_values.clone(),
        product_births: state.product_births.clone(),
        sums: state.sums.clone(),
        whole_commits: state.whole_commits.clone(),
    }
}

// Frozen complete pre-zip State merge, including every unchanged sidecar.
fn lookup_join(to: &mut OldState, from: &OldState, budget: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (place, source) in &from.cells {
        budget.lookup(to.cells.len(), "resource join lookup")?;
        let target = to
            .cells
            .get_mut(place)
            .ok_or_else(|| error("incompatible resource states"))?;
        fact_cost(target, budget)?;
        fact_cost(source, budget)?;
        let before = (target.life.len(), target.sources.len(), target.birth.len());
        target.life.extend(&source.life);
        target.sources.extend(&source.sources);
        target.birth.extend(&source.birth);
        changed |= before != (target.life.len(), target.sources.len(), target.birth.len());
    }
    budget.spend(
        to.later.len().saturating_add(from.later.len()),
        "resource initialization order join",
    )?;
    let before = to.later.len();
    to.later.extend(&from.later);
    changed |= before != to.later.len();
    for (key, source) in &from.package_values {
        budget.lookup(to.package_values.len(), "Package SSA birth join lookup")?;
        let target = to
            .package_values
            .get_mut(key)
            .ok_or_else(|| error("incompatible Package SSA birth shapes"))?;
        budget.spend(
            target.len().saturating_add(source.len()),
            "Package SSA birth join",
        )?;
        let before = target.len();
        target.extend(source);
        changed |= before != target.len();
    }
    changed |= product::join(&mut to.product_births, &from.product_births, budget)?;
    changed |= to.sums.join(&from.sums, budget)?;
    changed |= replacements::must_join(&mut to.whole_commits, &from.whole_commits, budget)?;
    Ok(changed)
}

fn state(count: u32, incoming: bool) -> OldState {
    let birth = if incoming {
        Birth::Incoming
    } else {
        Birth::Created
    };
    let mut state = OldState::default();
    for index in 0..count {
        state.cells.insert(
            Place::Value(ValueId(index)),
            Fact {
                life: BTreeSet::from([if incoming {
                    Life::Vacant
                } else {
                    Life::Available
                }]),
                sources: BTreeSet::from([if incoming {
                    Source::BorrowedField(index)
                } else {
                    Source::Head(LocalId(index))
                }]),
                birth: BTreeSet::from([birth]),
            },
        );
    }
    state.later.insert(if incoming {
        (LocalId(0), LocalId(1))
    } else {
        (LocalId(1), LocalId(2))
    });
    state
        .package_values
        .insert((ValueId(0), 0), BTreeSet::from([birth]));
    state.product_births.insert(
        product::Carrier::Local(LocalId(0)),
        BTreeMap::from([(vec![0, 1], BTreeSet::from([birth]))]),
    );
    state.whole_commits = Some(BTreeMap::from([(
        crate::place_replacements::origins::Place {
            root: crate::place_replacements::origins::Root::Parameter(0),
            path: vec![zil_ir::PlaceProjection::Field(0)],
            mutable: true,
        },
        BTreeSet::from([birth]),
    )]));
    let module = crate::place_replacements::fixture::module();
    let mut budget = Budget::new(&Limits::default());
    budget.sum_functions = vec![true];
    let f = &module.functions[0];
    state.sums = sums::Plan::new(f, &mut budget)
        .unwrap()
        .unwrap()
        .initial(&module, f, &mut budget)
        .unwrap();
    state
}
#[test]
fn sorted_resource_cells_match_complete_state_lookup_merge_and_preserve_life_source_birth_gaps() {
    for count in [0, 1, 4, 64] {
        let left = state(count, true);
        let right = state(count, false);
        let mut sorted = current(left.clone());
        let mut old = left;
        let actual_right = current(right.clone());
        let a = join(
            &mut sorted,
            &actual_right,
            &mut Budget::new(&Limits::default()),
        )
        .unwrap();
        let b = lookup_join(&mut old, &right, &mut Budget::new(&Limits::default())).unwrap();
        assert_eq!(a, b);
        assert_eq!(snapshot(&sorted), old);
        assert_eq!(snapshot(&actual_right), right);
        assert_eq!(
            right,
            state(count, false),
            "incoming full state remains immutable"
        );
        for (place, fact) in &sorted.cells {
            assert_eq!(fact.life, BTreeSet::from([Life::Vacant, Life::Available]));
            assert_eq!(
                fact.birth,
                BTreeSet::from([Birth::Incoming, Birth::Created])
            );
            let Place::Value(ValueId(index)) = place else {
                unreachable!()
            };
            assert_eq!(
                fact.sources,
                BTreeSet::from([Source::BorrowedField(*index), Source::Head(LocalId(*index))])
            );
            assert!(
                !exact(fact, Life::Available),
                "a missing branch cannot regain a complete responsibility"
            );
        }
        assert!(
            !join(
                &mut sorted.clone(),
                &sorted,
                &mut Budget::new(&Limits::default())
            )
            .unwrap()
        );
    }
}
#[test]
fn sorted_resource_cells_reject_wrong_places_and_missing_branch_inventory() {
    let left = state(4, true);
    let mut wrong = state(4, false);
    let fact = wrong.cells.remove(&Place::Value(ValueId(1))).unwrap();
    wrong.cells.insert(Place::Local(LocalId(9)), fact);
    for sorted in [false, true] {
        let mut copy = left.clone();
        let mut b = Budget::new(&Limits::default());
        let errors = if sorted {
            join(&mut current(copy), &current(wrong.clone()), &mut b)
        } else {
            lookup_join(&mut copy, &wrong, &mut b)
        }
        .unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::SafeReject)
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("incompatible resource states"))
        );
    }
    let mut missing = left.clone();
    missing.cells.remove(&Place::Value(ValueId(2)));
    let errors = join(
        &mut current(left.clone()),
        &current(missing.clone()),
        &mut Budget::new(&Limits::default()),
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("cell inventory differs")),
        "{errors:?}"
    );
    // The old lookup loop accepted an incomplete incoming prefix. Production
    // same-function states cannot have this shape; reject it explicitly now.
    lookup_join(
        &mut left.clone(),
        &missing,
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    let mut empty = BTreeMap::new();
    assert!(
        !join_cells(
            &mut empty,
            &BTreeMap::new(),
            &mut Budget::new(&Limits::default())
        )
        .unwrap()
    );
}
#[test]
fn sorted_resource_cells_have_a_repeatable_smaller_full_state_request_boundary() {
    let left = state(256, true);
    let right = state(256, false);
    let actual_right = current(right.clone());
    let run = |sorted, work| {
        let mut state = left.clone();
        let mut b = Budget::new(&Limits {
            max_analysis_steps: work,
            ..Limits::default()
        });
        if sorted {
            join(&mut current(state), &actual_right, &mut b)
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
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        run(sorted, lo).unwrap();
        lo
    };
    let sorted = minimum(true);
    let old = minimum(false);
    assert!(sorted < old, "new={sorted} old={old}");
    println!(
        "Complete resource State join: sorted N={sorted}, lookup N={old}; N-1 reject and fresh N retry pass"
    );
}

#[test]
fn sorted_resource_cells_preserve_complete_state_with_actual_packed_order() {
    let mut left = state(64, true);
    let mut right = state(64, false);
    for before in 0..24 {
        for later in before + 1..24 {
            left.later.insert((LocalId(before), LocalId(later)));
            right
                .later
                .insert((LocalId(before + 64), LocalId(later + 64)));
        }
    }
    assert!(left.later.len() > 256 && right.later.len() > 256);
    let mut actual = current(left.clone());
    let incoming = current(right.clone());
    assert!(matches!(actual.later, order::Order::Packed { .. }));
    assert!(matches!(incoming.later, order::Order::Packed { .. }));
    let mut old = left;
    let changed = join(&mut actual, &incoming, &mut Budget::new(&Limits::default())).unwrap();
    let old_changed = lookup_join(&mut old, &right, &mut Budget::new(&Limits::default())).unwrap();
    assert_eq!(changed, old_changed);
    assert_eq!(snapshot(&actual), old);
    assert_eq!(snapshot(&incoming), right);
    assert!(
        !join(
            &mut actual.clone(),
            &actual,
            &mut Budget::new(&Limits::default())
        )
        .unwrap()
    );
}
