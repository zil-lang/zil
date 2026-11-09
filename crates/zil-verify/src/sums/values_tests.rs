//! Frozen flat SSA representation and full State operations from 6ad.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
struct FlatState {
    initialized: Vec<u8>,
    locals: Vec<Arc<Content>>,
    values: Vec<Arc<Content>>,
    captures: Vec<Option<Arc<Content>>>,
    facts: BTreeSet<Fact>,
    observations: BTreeSet<ValueId>,
    partial: Option<partial_places::State>,
}
impl FlatState {
    fn cost(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(
            self.initialized
                .len()
                .saturating_add(self.locals.len())
                .saturating_add(self.values.len())
                .saturating_add(self.captures.len())
                .saturating_add(self.observations.len())
                .saturating_add(self.facts.len()),
        )?;
        if let Some(p) = &self.partial {
            p.cost(budget)?;
        }
        for (_, path, _) in &self.facts {
            budget.spend(path.len())?;
        }
        Ok(())
    }
    fn duplicate(&self, budget: &mut Budget) -> Check<Self> {
        self.cost(budget)?;
        Ok(self.clone())
    }
    fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        other.cost(budget)?;
        let mut changed = false;
        if let (Some(a), Some(b)) = (&mut self.partial, &other.partial) {
            changed |= a.join(b, budget)?;
        }
        for (a, b) in self.initialized.iter_mut().zip(&other.initialized) {
            let old = *a;
            *a |= *b;
            changed |= old != *a;
        }
        for (a, b) in self
            .locals
            .iter_mut()
            .zip(&other.locals)
            .chain(self.values.iter_mut().zip(&other.values))
        {
            if !Arc::ptr_eq(a, b) {
                if Arc::strong_count(a) > 1 {
                    a.cost(budget)?;
                }
                changed |= Arc::make_mut(a).merge(b, budget)?;
            }
        }
        for (a, b) in self.captures.iter_mut().zip(&other.captures) {
            if let (Some(a), Some(b)) = (a, b)
                && !Arc::ptr_eq(a, b)
            {
                if Arc::strong_count(a) > 1 {
                    a.cost(budget)?;
                }
                changed |= Arc::make_mut(a).merge(b, budget)?;
            }
        }
        let old = self.facts.len();
        self.facts.retain(|f| other.facts.contains(f));
        changed |= old != self.facts.len();
        let old = self.observations.len();
        self.observations.retain(|v| other.observations.contains(v));
        changed |= old != self.observations.len();
        Ok(changed)
    }
}

fn budget(n: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    })
}
fn payload(index: usize, seed: usize) -> Content {
    let source = if seed.is_multiple_of(2) {
        Source::Input(0, vec![0, index as u32])
    } else {
        Source::Captured(1, vec![1, index as u32, 0])
    };
    let leaf = Content::Leaf(BTreeSet::from([source]));
    let value = match index % 4 {
        0 => Content::Copy,
        1 => leaf.clone(),
        2 => Content::Product(vec![
            leaf.clone(),
            Content::Missing(Box::new(Content::Copy)),
        ]),
        _ => Content::Sum(
            BTreeMap::from([(0, vec![leaf]), (1, vec![Content::Copy])]),
            Some((seed as u32, vec![1, 0, 0])),
        ),
    };
    if !seed.is_multiple_of(2) && index.is_multiple_of(3) {
        Content::Missing(Box::new(value))
    } else {
        value
    }
}
fn partial(seed: usize) -> partial_places::State {
    use crate::place_replacements::fixture::{instruction as i, module};
    let mut module = module();
    let f = &mut module.functions[0];
    f.values = vec![Type::I32, Type::I32, Type::Bool];
    f.blocks[0].instructions = vec![
        i(
            Some(ValueId(0)),
            InstructionKind::VariantTagLocal {
                local: LocalId(0),
                path: vec![],
            },
        ),
        i(
            Some(ValueId(1)),
            InstructionKind::Constant(Constant::I32(seed as i32)),
        ),
        i(
            Some(ValueId(2)),
            InstructionKind::Binary {
                op: BinaryOp::Equal,
                left: ValueId(0),
                right: ValueId(1),
            },
        ),
    ];
    f.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(2),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    let mut b = budget(1_000_000);
    let meta = partial_places::Meta::new(f, &mut b).unwrap();
    let mut state = partial_places::State::default();
    for instruction in &f.blocks[0].instructions {
        meta.step(&mut state, instruction, &mut b).unwrap();
    }
    meta.edge(&mut state, &f.blocks[0].terminator, 1, &mut b)
        .unwrap();
    state
}
fn flat(len: usize, seed: usize) -> FlatState {
    FlatState {
        initialized: vec![if seed == 0 { 1 } else { 2 }, 2],
        locals: vec![Arc::new(payload(2, seed)), Arc::new(payload(1, seed))],
        values: (0..len).map(|i| Arc::new(payload(i, seed))).collect(),
        captures: vec![Some(Arc::new(payload(3, seed))), None],
        facts: if seed == 0 {
            BTreeSet::from([(LocalId(0), vec![(0, 0)], 0), (LocalId(1), vec![(1, 0)], 1)])
        } else {
            BTreeSet::from([(LocalId(0), vec![(0, 0)], 0)])
        },
        observations: (0..len.min(2))
            .filter(|i| seed == 0 || *i == 0)
            .map(|i| ValueId(i as u32))
            .collect(),
        partial: Some(partial(seed)),
    }
}
fn paged(old: &FlatState) -> State {
    State {
        initialized: old.initialized.clone(),
        locals: old.locals.clone(),
        values: values::Values::build(old.values.len(), &mut budget(1_000_000), |i, _| {
            Ok(old.values[i].clone())
        })
        .unwrap(),
        captures: old.captures.clone(),
        facts: old.facts.clone(),
        observations: old.observations.clone(),
        partial: old.partial.clone(),
    }
}
fn snapshot(state: &State) -> FlatState {
    FlatState {
        initialized: state.initialized.clone(),
        locals: state.locals.clone(),
        values: state.values.flat(),
        captures: state.captures.clone(),
        facts: state.facts.clone(),
        observations: state.observations.clone(),
        partial: state.partial.clone(),
    }
}
fn assert_complete(old: &FlatState, new: &State) {
    assert!(old == &snapshot(new), "complete independent State differs");
}

#[test]
fn paged_ssa_full_state_matches_flat_across_boundaries_and_repeated_joins() {
    for len in [0, 1, 31, 32, 33, 63, 64, 65] {
        let seed = flat(len, 0);
        let incoming = flat(len, 1);
        let mut old = seed.clone();
        let mut new = paged(&seed);
        let other = paged(&incoming);
        let original = seed.clone();
        if len > 0 {
            let index = len - 1;
            let root = Arc::new(payload(index, 3));
            old.values[index] = root.clone();
            new.values.set(index, root, &mut budget(1_000_000)).unwrap();
        }
        assert_complete(&old, &new);
        let mut a = budget(1_000_000);
        let mut b = budget(1_000_000);
        assert_eq!(old.merge(&incoming, &mut a), new.merge(&other, &mut b));
        assert_complete(&old, &new);
        assert_eq!(old.merge(&incoming, &mut a), new.merge(&other, &mut b));
        assert_complete(&old, &new);
        assert!(seed == original, "external flat snapshot changed");
        let old_clone = old.duplicate(&mut a).unwrap();
        let new_clone = new.duplicate(&mut b).unwrap();
        assert_complete(&old_clone, &new_clone);
    }
}

#[test]
fn paged_ssa_preserves_semantic_error_prefix_and_original_product_zip() {
    for len in [2, 33, 65] {
        for shared in [false, true] {
            let mut left = flat(len, 0);
            let mut right = flat(len, 1);
            left.values[0] = Arc::new(Content::Leaf(BTreeSet::from([Source::Fresh])));
            right.values[0] =
                Arc::new(Content::Missing(Box::new(Content::Leaf(BTreeSet::from([
                    Source::Unknown,
                ])))));
            left.values[len - 1] = Arc::new(Content::Copy);
            right.values[len - 1] = Arc::new(Content::Product(vec![Content::Copy]));
            let mut old = left.clone();
            let mut new = paged(&left);
            let incoming = paged(&right);
            let alias = if shared { Some(new.clone()) } else { None };
            let expected_alias = alias.as_ref().map(snapshot);
            let a = old.merge(&right, &mut budget(1_000_000));
            let b = new.merge(&incoming, &mut budget(1_000_000));
            assert!(a.is_err());
            assert_eq!(a, b);
            assert_complete(&old, &new);
            assert_eq!(new.initialized[0], 3);
            assert!(matches!(
                new.values.get(0, &mut budget(100)).unwrap().as_ref(),
                Content::Missing(_)
            ));
            if let (Some(alias), Some(expected)) = (alias, expected_alias) {
                assert_complete(&expected, &alias);
            }
        }
    }
    let mut left = flat(1, 0);
    let mut right = flat(1, 1);
    left.values[0] = Arc::new(Content::Product(vec![Content::Copy, Content::Copy]));
    right.values[0] = Arc::new(Content::Product(vec![Content::Copy]));
    let mut new = paged(&left);
    let other = paged(&right);
    assert_eq!(
        left.merge(&right, &mut budget(1_000_000)),
        new.merge(&other, &mut budget(1_000_000))
    );
    assert_complete(&left, &new);
}

#[test]
fn paged_ssa_content_cow_accounts_for_external_aliases_independently_of_page_sharing() {
    let old = Content::Leaf(BTreeSet::from([Source::Input(0, vec![1; 20])]));
    let incoming = Content::Leaf(BTreeSet::from([Source::Fresh]));
    let run = |external: bool, shared_page: bool| {
        let mut setup = budget(1_000_000);
        let mut left =
            values::Values::build(1, &mut setup, |_, _| Ok(Arc::new(old.clone()))).unwrap();
        let right =
            values::Values::build(1, &mut setup, |_, _| Ok(Arc::new(incoming.clone()))).unwrap();
        let external = external.then(|| left.get(0, &mut setup).unwrap().clone());
        let shared = shared_page.then(|| left.clone());
        let mut b = budget(1_000_000);
        left.merge(&right, &mut b).unwrap();
        if let Some(alias) = external {
            assert_eq!(alias.as_ref(), &old);
        }
        if let Some(alias) = shared {
            assert_eq!(alias.get(0, &mut setup).unwrap().as_ref(), &old);
        }
        1_000_000 - b.remaining
    };
    let mut b = budget(1_000_000);
    old.cost(&mut b).unwrap();
    let content_cost = 1_000_000 - b.remaining;
    assert_eq!(run(true, false) - run(false, false), content_cost);
    // Shared-only rejected probe: two root walks, root identity, Content node,
    // two source iterator entries, then Input's owner/path and Fresh comparison.
    // The original Content COW and page header/root copy still follow in full.
    let rejected_probe_cost = 2 + 1 + 1 + 2 + (20 + 1) + 1;
    assert_eq!(
        run(false, true) - run(false, false),
        content_cost + 2 + rejected_probe_cost // header + one root ref + probe
    );
}

#[test]
fn paged_ssa_rejects_broken_inventory_before_identity_and_keeps_failed_set_private() {
    for len in [0, 1, 31, 32, 33, 63, 64, 65] {
        let keyed: BTreeMap<_, _> = (0..len)
            .map(|i| (ValueId(i as u32), Arc::new(Content::Copy)))
            .collect();
        let mut b = budget(1_000_000);
        let value = values::Values::from_keyed(len, keyed.clone(), &mut b).unwrap();
        assert_eq!(value.flat().len(), len);
        assert!(value.get(len, &mut b).is_err());
        let mut invalid = value.clone();
        let before = invalid.flat();
        assert!(invalid.set(len, Arc::new(Content::Copy), &mut b).is_err());
        assert_eq!(before, invalid.flat());
        let mut extra = value.clone();
        extra.corrupt_shape(0);
        assert!(extra.merge(&extra.clone(), &mut b).is_err());
        if len > 0 {
            let mut wrong = keyed.clone();
            let root = wrong.remove(&ValueId(0)).unwrap();
            wrong.insert(ValueId(len as u32), root);
            assert!(values::Values::from_keyed(len, wrong, &mut b).is_err());
            let mut missing = keyed;
            missing.remove(&ValueId(0));
            assert!(values::Values::from_keyed(len, missing, &mut b).is_err());
            for kind in 0..3 {
                let mut broken = value.clone();
                broken.corrupt_shape(kind);
                assert!(broken.merge(&broken.clone(), &mut b).is_err());
                assert!(value.clone().merge(&broken, &mut b).is_err());
            }
        }
    }
    let base = values::Values::build(33, &mut budget(1_000_000), |_, _| {
        Ok(Arc::new(Content::Copy))
    })
    .unwrap();
    for n in 0..36 {
        let mut left = base.clone();
        let snapshot = left.flat();
        let error = left
            .set(
                0,
                Arc::new(Content::Leaf(BTreeSet::from([Source::Fresh]))),
                &mut budget(n),
            )
            .unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(snapshot, left.flat());
        assert_eq!(base.flat(), left.flat());
    }
    let mut left = base.clone();
    left.set(0, Arc::new(Content::Copy), &mut budget(36))
        .unwrap();
    let mut fresh = base.clone();
    fresh
        .set(0, Arc::new(Content::Copy), &mut budget(36))
        .unwrap();
    eprintln!("V shared page set N=36; N-1 unchanged/fresh N checked");
}

#[test]
fn paged_ssa_has_exact_fresh_request_boundaries_against_full_flat_state() {
    let seed = flat(1024, 0);
    let run = |old: bool, n| {
        let mut b = budget(n);
        if old {
            let mut state = seed.clone();
            for _ in 0..16 {
                let other = state.duplicate(&mut b)?;
                assert!(!state.merge(&other, &mut b)?);
            }
            Ok::<_, Diagnostic>(state)
        } else {
            let mut state = paged(&seed);
            for _ in 0..16 {
                let other = state.duplicate(&mut b)?;
                assert!(!state.merge(&other, &mut b)?);
            }
            Ok(snapshot(&state))
        }
    };
    let boundary = |old| {
        let mut lo = 0;
        let mut hi = 1_000_000;
        assert!(run(old, hi).is_ok());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if run(old, mid).is_ok() {
                hi = mid
            } else {
                lo = mid + 1
            }
        }
        lo
    };
    let n = boundary(false);
    let old_n = boundary(true);
    assert!(n < old_n, "{n} >= {old_n}");
    assert!(run(false, n).unwrap() == run(true, old_n).unwrap());
    assert!(run(false, n).unwrap() == seed);
    assert_eq!(
        run(false, n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    eprintln!(
        "V paged full State 16 clone+identity-merge N={n}, flat N={old_n}; N-1/fresh checked"
    );
}

#[test]
fn paged_ssa_full_state_retains_local_capture_and_external_aliases() {
    for shared in [false, true] {
        let mut left = flat(33, 0);
        let mut right = flat(33, 1);
        left.locals[0] = left.values[1].clone();
        left.captures[0] = Some(left.values[1].clone());
        right.locals[0] = right.values[1].clone();
        right.captures[0] = Some(right.values[1].clone());
        let external = left.values[1].clone();
        let expected = external.as_ref().clone();
        let mut old = left.clone();
        let mut new = paged(&left);
        let incoming = paged(&right);
        let alias = shared.then(|| new.clone());
        let snapshot_alias = alias.as_ref().map(snapshot);
        assert_eq!(
            old.merge(&right, &mut budget(1_000_000)),
            new.merge(&incoming, &mut budget(1_000_000))
        );
        assert_complete(&old, &new);
        assert_eq!(external.as_ref(), &expected);
        if let (Some(alias), Some(expected)) = (alias, snapshot_alias) {
            assert_complete(&expected, &alias);
        }
    }
}
