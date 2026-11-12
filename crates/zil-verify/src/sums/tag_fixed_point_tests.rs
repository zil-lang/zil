//! Independent frozen reconstruction from f82. Never calls the new predicate.
use super::*;
impl Content {
    fn retain_missing_by_reconstruction(&mut self, old: &Content, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        if let Self::Missing(child) = old {
            if !matches!(self, Self::Missing(_)) {
                b.spend(1)?;
                let previous = std::mem::replace(self, Self::Copy);
                *self = Self::Missing(Box::new(previous));
            }
            let Self::Missing(at) = self else {
                unreachable!()
            };
            return at.retain_missing_by_reconstruction(child, b);
        }
        match (self, old) {
            (Self::Product(a), Self::Product(c)) => {
                for (a, c) in a.iter_mut().zip(c) {
                    a.retain_missing_by_reconstruction(c, b)?;
                }
            }
            (Self::Sum(a, _), Self::Sum(c, _)) => {
                for (tag, fs) in c {
                    b.spend(a.len().saturating_add(1))?;
                    if let Some(at) = a.get_mut(tag) {
                        for (a, c) in at.iter_mut().zip(fs) {
                            a.retain_missing_by_reconstruction(c, b)?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn forget_copy_tags_by_reconstruction(&mut self, ty: &Type, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        let ty = shared_enum_physical(ty, b)?;
        if let Self::Missing(child) = self {
            return child.forget_copy_tags_by_reconstruction(ty, b);
        }
        let copy_sum = match self {
            Self::Sum(_, _)
                if indexed_invalidation(b)
                    && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) =>
            {
                b.addressed_copy_sum(ty)?
            }
            Self::Product(_) if indexed_invalidation(b) => false,
            Self::Copy | Self::Leaf(_) if indexed_invalidation(b) => {
                return Ok(());
            }
            _ => {
                resources::charge_payload(ty, b)?;
                matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) && ty.is_copy_value()
            }
        };
        if copy_sum {
            let old = self.duplicate(b)?;
            let mut unknown = content_by_reconstruction(ty, None, &mut Vec::new(), false, b)?;
            unknown.retain_missing_by_reconstruction(&old, b)?;
            *self = unknown;
            return Ok(());
        }
        match (self, ty) {
            (Self::Product(fs), Type::Tuple(ts)) => {
                for (f, t) in fs.iter_mut().zip(ts) {
                    f.forget_copy_tags_by_reconstruction(t, b)?;
                }
            }
            (Self::Product(fs), Type::Package { fields, .. }) => {
                for (f, (_, t)) in fs.iter_mut().zip(fields) {
                    f.forget_copy_tags_by_reconstruction(t, b)?;
                }
            }
            (Self::Sum(vs, route), Type::Enum { variants, .. }) => {
                *route = None;
                for (v, fs) in vs {
                    for (f, t) in fs.iter_mut().zip(&variants[*v as usize].1) {
                        f.forget_copy_tags_by_reconstruction(t, b)?;
                    }
                }
            }
            (Self::Sum(vs, route), Type::ErrorUnion { error, result }) => {
                *route = None;
                for (v, fs) in vs {
                    if let Some(f) = fs.first_mut() {
                        f.forget_copy_tags_by_reconstruction(
                            if *v == 0 { error } else { result },
                            b,
                        )?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
fn content_by_reconstruction(
    ty: &Type,
    parameter: Option<u32>,
    path: &mut Vec<u32>,
    bottom: bool,
    budget: &mut Budget,
) -> Check<Content> {
    budget.spend(path.len().saturating_add(2))?;
    if let Type::BorrowedStorage { .. } = ty {
        let physical = shared_enum_physical(ty, budget)?;
        if !std::ptr::eq(physical, ty) {
            return content_by_reconstruction(physical, parameter, path, bottom, budget);
        }
    }
    Ok(match ty {
        Type::Managed(_) => Content::Leaf(if bottom {
            BTreeSet::new()
        } else {
            BTreeSet::from([parameter.map_or(Source::Unknown, |p| Source::Input(p, path.clone()))])
        }),
        Type::Tuple(fields) => {
            budget.spend(fields.len())?;
            let mut children = Vec::with_capacity(fields.len());
            for (index, ty) in fields.iter().enumerate() {
                path.extend([0, index as u32]);
                children.push(content_by_reconstruction(
                    ty, parameter, path, bottom, budget,
                )?);
                path.truncate(path.len() - 2);
            }
            Content::Product(children)
        }
        Type::Package { fields, .. } => {
            budget.spend(fields.len())?;
            let mut children = Vec::with_capacity(fields.len());
            for (index, (_, ty)) in fields.iter().enumerate() {
                path.extend([0, index as u32]);
                children.push(content_by_reconstruction(
                    ty, parameter, path, bottom, budget,
                )?);
                path.truncate(path.len() - 2);
            }
            Content::Product(children)
        }
        Type::Enum { variants, .. } => {
            if bottom {
                return Ok(Content::Sum(BTreeMap::new(), None));
            }
            budget.spend(variants.len())?;
            let mut alternatives = BTreeMap::new();
            for (variant, (_, fields)) in variants.iter().enumerate() {
                budget.spend(fields.len())?;
                let mut children = Vec::with_capacity(fields.len());
                for (index, ty) in fields.iter().enumerate() {
                    path.extend([1, variant as u32, index as u32]);
                    children.push(content_by_reconstruction(
                        ty, parameter, path, bottom, budget,
                    )?);
                    path.truncate(path.len() - 3);
                }
                if !bottom {
                    alternatives.insert(variant as u32, children);
                }
            }
            Content::Sum(alternatives, parameter.map(|p| (p, path.clone())))
        }
        Type::ErrorUnion { error, result } => {
            if bottom {
                return Ok(Content::Sum(BTreeMap::new(), None));
            }
            budget.spend(2)?;
            let mut alternatives = BTreeMap::new();
            for (variant, ty) in [(0, error.as_ref()), (1, result.as_ref())] {
                path.extend([1, variant, 0]);
                let child = content_by_reconstruction(ty, parameter, path, bottom, budget)?;
                path.truncate(path.len() - 3);
                if !bottom {
                    alternatives.insert(variant, vec![child]);
                }
            }
            Content::Sum(alternatives, parameter.map(|p| (p, path.clone())))
        }
        _ => Content::Copy,
    })
}

fn budget(profile: usize, n: usize) -> Budget {
    let mut b = Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    });
    b.pod_stores = profile == 1;
    b.store_option_results = profile == 2;
    b.ordinary_error_places = profile == 2;
    b
}
fn scalar(name: &str) -> Type {
    Type::Enum {
        name: name.into(),
        variants: vec![("A".into(), vec![Type::I32]), ("B".into(), vec![Type::I32])],
    }
}
fn vary(at: &mut Content, seed: usize, cursor: &mut usize) {
    *cursor += 1;
    let index = *cursor;
    match at {
        Content::Missing(child) => vary(child, seed, cursor),
        Content::Product(fields) => {
            for child in fields {
                vary(child, seed, cursor);
            }
        }
        Content::Sum(arms, route) => {
            if seed & 1 != 0 {
                arms.retain(|tag, _| *tag == 0);
            }
            if seed & 2 != 0 {
                *route = Some((7, vec![1, index as u32, 0]));
            }
            for child in arms.values_mut().flatten() {
                vary(child, seed, cursor);
            }
        }
        _ => {}
    }
    if seed & 4 != 0 && index.is_multiple_of(2) {
        let old = std::mem::replace(at, Content::Copy);
        *at = Content::Missing(Box::new(Content::Missing(Box::new(old))));
    }
}
#[test]
fn copy_tag_fixed_point_matches_independent_reconstruction_for_all_profiles() {
    let inner = scalar("Inner");
    let outer = Type::Enum {
        name: "Outer".into(),
        variants: vec![
            ("A".into(), vec![inner.clone(), Type::I32]),
            ("B".into(), vec![inner.clone()]),
        ],
    };
    let types = [
        inner.clone(),
        outer.clone(),
        Type::ErrorUnion {
            error: Box::new(inner.clone()),
            result: Box::new(outer.clone()),
        },
        Type::Tuple(vec![inner.clone(), outer.clone()]),
        Type::Package {
            name: "Owned".into(),
            fields: vec![
                ("tag".into(), inner),
                ("owner".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ],
        },
    ];
    for ty in types {
        for seed in 0..8 {
            for parameter in [None, Some(3)] {
                let mut original = content_by_reconstruction(
                    &ty,
                    parameter,
                    &mut Vec::new(),
                    false,
                    &mut budget(1, 1_000_000),
                )
                .unwrap();
                vary(&mut original, seed, &mut 0);
                for profile in 0..3 {
                    let mut old = original.clone();
                    let mut new = original.clone();
                    let mut a = budget(profile, 1_000_000);
                    let mut b = budget(profile, 1_000_000);
                    assert_eq!(
                        old.forget_copy_tags_by_reconstruction(&ty, &mut a),
                        new.forget_copy_tags(&ty, &mut b)
                    );
                    assert_eq!(old, new, "type={ty:?} seed={seed} profile={profile}");
                    assert_eq!(
                        old.forget_copy_tags_by_reconstruction(&ty, &mut a),
                        new.forget_copy_tags(&ty, &mut b)
                    );
                    assert_eq!(old, new, "repeated type={ty:?}");
                    assert_eq!(
                        (a.tasks, a.registrations, a.generic_mutex),
                        (b.tasks, b.registrations, b.generic_mutex)
                    );
                }
            }
        }
    }
}
#[test]
fn unknown_copy_tag_shape_preserves_nested_missing_and_requires_every_route_absent() {
    let ty = Type::ErrorUnion {
        error: Box::new(scalar("Error")),
        result: Box::new(Type::Tuple(vec![Type::I32, scalar("Result")])),
    };
    let mut unknown =
        content_by_reconstruction(&ty, None, &mut Vec::new(), false, &mut budget(1, 1_000_000))
            .unwrap();
    vary(&mut unknown, 4, &mut 0);
    assert!(
        unknown
            .tags_unknown(&ty, &mut budget(1, 1_000_000))
            .unwrap()
    );
    let mut routed = unknown.clone();
    fn set_nested_route(at: &mut Content) -> bool {
        match at {
            Content::Missing(child) => set_nested_route(child),
            Content::Product(fields) => fields.iter_mut().any(set_nested_route),
            Content::Sum(arms, route) => {
                if arms.values_mut().flatten().any(set_nested_route) {
                    true
                } else {
                    *route = Some((2, vec![0, 1]));
                    true
                }
            }
            _ => false,
        }
    }
    assert!(set_nested_route(&mut routed));
    assert!(!routed.tags_unknown(&ty, &mut budget(1, 1_000_000)).unwrap());
    let mut old = routed.clone();
    let mut new = routed;
    old.forget_copy_tags_by_reconstruction(&ty, &mut budget(1, 1_000_000))
        .unwrap();
    new.forget_copy_tags(&ty, &mut budget(1, 1_000_000))
        .unwrap();
    assert_eq!(old, new);
}
#[test]
fn unknown_shape_checks_exact_terminals_arm_keys_and_error_first_layout() {
    let enum_ty = scalar("Tag");
    let union = Type::ErrorUnion {
        error: Box::new(enum_ty.clone()),
        result: Box::new(Type::I32),
    };
    let unknown = content_by_reconstruction(
        &union,
        None,
        &mut Vec::new(),
        false,
        &mut budget(1, 1_000_000),
    )
    .unwrap();
    assert!(
        unknown
            .tags_unknown(&union, &mut budget(1, 1_000_000))
            .unwrap()
    );
    let Content::Sum(arms, _) = &unknown else {
        panic!("union content")
    };
    assert!(matches!(arms[&0][0], Content::Sum(..)));
    assert_eq!(arms[&1][0], Content::Copy);
    let mut swapped = unknown.clone();
    let Content::Sum(arms, _) = &mut swapped else {
        unreachable!()
    };
    let error = arms.remove(&0).unwrap();
    let result = arms.remove(&1).unwrap();
    arms.insert(0, result);
    arms.insert(1, error);
    let mut invalid = vec![
        swapped,
        Content::Sum(BTreeMap::new(), None),
        Content::Sum(
            BTreeMap::from([(0, vec![Content::Copy]), (7, vec![Content::Copy])]),
            None,
        ),
        Content::Sum(
            BTreeMap::from([(0, vec![]), (1, vec![Content::Copy])]),
            None,
        ),
    ];
    let mut extra = unknown.clone();
    let Content::Sum(arms, _) = &mut extra else {
        unreachable!()
    };
    arms.get_mut(&1).unwrap().push(Content::Copy);
    invalid.push(extra);
    for original in invalid {
        assert!(
            !original
                .tags_unknown(&union, &mut budget(1, 1_000_000))
                .unwrap()
        );
        let mut old = original.clone();
        let mut new = original;
        assert_eq!(
            old.forget_copy_tags_by_reconstruction(&union, &mut budget(1, 1_000_000)),
            new.forget_copy_tags(&union, &mut budget(1, 1_000_000))
        );
        assert_eq!(old, new);
    }
    let copy = Content::Copy;
    for ty in [
        Type::Managed(Box::new(Type::ByteBuffer)),
        Type::Tuple(vec![]),
        Type::Package {
            name: "P".into(),
            fields: vec![],
        },
        enum_ty,
        union,
        Type::BorrowedStorage {
            storage: Box::new(Type::I32),
            life: StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        },
    ] {
        assert!(!copy.tags_unknown(&ty, &mut budget(1, 1_000_000)).unwrap());
    }
    for ty in [
        Type::I32,
        Type::Record {
            name: "R".into(),
            fields: vec![("x".into(), scalar("Hidden"))],
        },
        Type::Array {
            element: Box::new(Type::I32),
            length: 3,
        },
    ] {
        assert!(copy.tags_unknown(&ty, &mut budget(1, 1_000_000)).unwrap());
    }
    assert!(
        !Content::Leaf(BTreeSet::from([Source::Unknown]))
            .tags_unknown(&Type::I32, &mut budget(1, 1_000_000))
            .unwrap()
    );
}
#[test]
fn owning_sources_and_shared_arc_snapshots_keep_their_independent_content() {
    let ty = Type::Enum {
        name: "Owner".into(),
        variants: vec![
            ("A".into(), vec![Type::Managed(Box::new(Type::ByteBuffer))]),
            ("B".into(), vec![]),
        ],
    };
    let sources = BTreeSet::from([
        Source::Fresh,
        Source::Input(2, vec![0, 1]),
        Source::Captured(3, vec![1, 0, 0]),
        Source::ReturnedInput(1, vec![]),
        Source::Unknown,
    ]);
    let original = Content::Sum(
        BTreeMap::from([(0, vec![Content::Missing(Box::new(Content::Leaf(sources)))])]),
        Some((5, vec![1, 0, 0])),
    );
    let expected_snapshot = original.clone();
    let alias = Arc::new(original.clone());
    let mut edited = alias.clone();
    Arc::make_mut(&mut edited)
        .forget_copy_tags(&ty, &mut budget(1, 1_000_000))
        .unwrap();
    assert_eq!(*alias, expected_snapshot);
    let mut old = original;
    old.forget_copy_tags_by_reconstruction(&ty, &mut budget(1, 1_000_000))
        .unwrap();
    assert_eq!(old, *edited);
}
#[test]
fn exact_unknown_fixed_point_has_a_smaller_complete_request_boundary() {
    let ty = Type::ErrorUnion {
        error: Box::new(scalar("E")),
        result: Box::new(scalar("R")),
    };
    let original =
        content_by_reconstruction(&ty, None, &mut Vec::new(), false, &mut budget(1, 1_000_000))
            .unwrap();
    let run = |old: bool, steps| {
        let mut b = budget(1, steps);
        let mut value = original.clone();
        for _ in 0..64 {
            if old {
                value.forget_copy_tags_by_reconstruction(&ty, &mut b)?;
            } else {
                value.forget_copy_tags(&ty, &mut b)?;
            }
        }
        Ok::<_, Diagnostic>(value)
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
    assert_eq!(run(false, n).unwrap(), run(true, old_n).unwrap());
    assert_eq!(
        run(false, n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(run(false, n).unwrap(), original);
    eprintln!("V fixed unknown 64x N={n}, frozen reconstruction N={old_n}; N-1/fresh verified");
}

#[test]
fn large_tag_layout_preserves_owning_routes_missing_fields_and_shared_aliases() {
    let owner = Type::Package {
        name: "PlainOwner".into(),
        fields: vec![
            ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ("n".into(), Type::I64),
        ],
    };
    let types = [
        owner.clone(),
        Type::ErrorUnion {
            error: Box::new(owner.clone()),
            result: Box::new(Type::Bool),
        },
        Type::Package {
            name: "NestedOwner".into(),
            fields: vec![
                ("child".into(), owner.clone()),
                ("tag".into(), scalar("ActualCopyTag")),
            ],
        },
        Type::Enum {
            name: "OwnChoice".into(),
            variants: vec![("Owner".into(), vec![owner]), ("Empty".into(), vec![])],
        },
    ];
    for ty in types {
        for seed in 0..8 {
            for parameter in [None, Some(3)] {
                let mut original = content_by_reconstruction(
                    &ty,
                    parameter,
                    &mut Vec::new(),
                    false,
                    &mut budget(1, 1_000_000),
                )
                .unwrap();
                vary(&mut original, seed, &mut 0);
                let mut expected = original.clone();
                expected
                    .forget_copy_tags_by_reconstruction(&ty, &mut budget(1, 1_000_000))
                    .unwrap();
                let alias = Arc::new(original.clone());
                let mut actual = alias.clone();
                let mut work = budget(1, 1_000_000);
                work.large_tag_layout = true;
                forget_shared_copy_tags(&mut actual, &ty, &mut work, None).unwrap();
                assert_eq!(
                    actual.as_ref(),
                    &expected,
                    "type={ty:?} seed={seed} parameter={parameter:?}"
                );
                assert_eq!(alias.as_ref(), &original, "shared predecessor changed");
                forget_shared_copy_tags(&mut actual, &ty, &mut work, None).unwrap();
                assert_eq!(actual.as_ref(), &expected);
            }
        }
    }
}
