//! Frozen 0099 complete shared-root transfer, independent of the cache.
use super::*;
fn old_shared(value: &mut Arc<Content>, ty: &Type, budget: &mut Budget) -> Check<()> {
    budget.spend(1)?;
    if Arc::strong_count(value) > 1
        && matches!(value.as_ref(), Content::Sum(..))
        && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
    {
        // Keep the original entry and qualification order. In particular,
        // charge_payload rejects invalid Task contexts before any shape proof.
        budget.spend(1)?;
        let actual = shared_enum_physical(ty, budget)?;
        let copy_sum = if indexed_invalidation(budget) {
            budget.addressed_copy_sum(actual)?
        } else {
            resources::charge_payload(actual, budget)?;
            actual.is_copy_value()
        };
        if copy_sum && value.tags_unknown(actual, budget)? {
            return Ok(());
        }
    }
    value.cost(budget)?;
    Arc::make_mut(value).forget_copy_tags(ty, budget)
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
fn tag_type(arms: usize) -> Type {
    Type::Enum {
        name: "CacheShape".into(),
        variants: (0..arms)
            .map(|i| (format!("Arm{i}"), vec![Type::I32]))
            .collect(),
    }
}
fn function(types: Vec<Type>) -> TypedFunction {
    let mut f = crate::place_replacements::fixture::module()
        .functions
        .remove(0);
    let base = f.locals[0].clone();
    f.locals = types
        .into_iter()
        .enumerate()
        .map(|(index, ty)| {
            let mut local = base.clone();
            local.id = LocalId(index as u32);
            local.ty = ty;
            local
        })
        .collect();
    f
}
fn unknown(ty: &Type) -> Arc<Content> {
    Arc::new(content(ty, None, &mut Vec::new(), false, &mut budget(1, 1_000_000)).unwrap())
}
fn entries(cache: &Cache<'_>) -> usize {
    cache.slots.as_ref().map_or(0, |slots| {
        slots
            .iter()
            .filter(|slot| slot.as_ref().is_some_and(|entry| entry.unknown.is_some()))
            .count()
    })
}
#[test]
fn local_tag_cache_uses_strong_identity_and_fixed_actual_local_type() {
    let f = function(vec![tag_type(2), tag_type(3)]);
    let ty = &f.locals[0].ty;
    let value = unknown(ty);
    let mut cache = Cache::new(&f);
    let mut cold = budget(1, 1_000_000);
    assert!(cache.known(0, &value, ty, &mut cold).unwrap());
    assert_eq!(entries(&cache), 1);
    let mut hot = budget(1, 1_000_000);
    assert!(cache.known(0, &value, ty, &mut hot).unwrap());
    assert!(hot.remaining > cold.remaining);
    // The same root in another actual local has its own different Type.
    assert!(
        !cache
            .known(1, &value, &f.locals[1].ty, &mut budget(1, 1_000_000))
            .unwrap()
    );
    assert_eq!(entries(&cache), 1);
    let equal_but_distinct = unknown(ty);
    assert!(!Arc::ptr_eq(&value, &equal_but_distinct));
    let mut miss = budget(1, 1_000_000);
    assert!(cache.known(0, &equal_but_distinct, ty, &mut miss).unwrap());
    assert!(miss.remaining < hot.remaining);
    assert!(Arc::ptr_eq(
        cache.slots.as_ref().unwrap()[0]
            .as_ref()
            .unwrap()
            .unknown
            .as_ref()
            .unwrap(),
        &equal_but_distinct
    ));
    let temporary_type = ty.clone();
    let previous = cache.slots.as_ref().unwrap()[0]
        .as_ref()
        .unwrap()
        .unknown
        .as_ref()
        .unwrap()
        .clone();
    assert!(
        cache
            .known(0, &value, &temporary_type, &mut budget(1, 1_000_000))
            .unwrap()
    );
    assert!(Arc::ptr_eq(
        cache.slots.as_ref().unwrap()[0]
            .as_ref()
            .unwrap()
            .unknown
            .as_ref()
            .unwrap(),
        &previous
    ));
    // An identical function owns different actual Type nodes and a fresh cache.
    let other = function(vec![ty.clone()]);
    let mut separate = Cache::new(&other);
    assert!(
        separate
            .known(0, &value, &other.locals[0].ty, &mut budget(1, 1_000_000))
            .unwrap()
    );
    assert_eq!(entries(&separate), 1);
}
#[test]
fn local_tag_cache_preserves_snapshot_when_its_reference_forces_content_cow() {
    let f = function(vec![tag_type(2)]);
    let ty = &f.locals[0].ty;
    let cache = std::cell::RefCell::new(Cache::new(&f));
    let mut value = unknown(ty);
    let alias = value.clone();
    forget_shared_copy_tags(&mut value, ty, &mut budget(1, 1_000_000), Some((&cache, 0))).unwrap();
    assert_eq!(entries(&cache.borrow()), 1);
    let snapshot = (*value).clone();
    drop(alias);
    assert_eq!(Arc::strong_count(&value), 2); // current owner and cached immutable root
    let mut b = budget(1, 1_000_000);
    value.cost(&mut b).unwrap();
    let Content::Sum(arms, route) = Arc::make_mut(&mut value) else {
        panic!()
    };
    arms.remove(&1);
    *route = Some((7, vec![1, 0, 0]));
    assert_eq!(
        cache.borrow().slots.as_ref().unwrap()[0]
            .as_ref()
            .unwrap()
            .unknown
            .as_ref()
            .unwrap()
            .as_ref(),
        &snapshot
    );
    let mut old = value.clone();
    let keep = value.clone();
    assert_eq!(
        old_shared(&mut old, ty, &mut budget(1, 1_000_000)),
        forget_shared_copy_tags(&mut value, ty, &mut budget(1, 1_000_000), Some((&cache, 0)))
    );
    assert_eq!(old, value);
    assert!(matches!(keep.as_ref(),Content::Sum(arms,Some(_)) if arms.len()==1));
    assert_eq!(
        cache.borrow().slots.as_ref().unwrap()[0]
            .as_ref()
            .unwrap()
            .unknown
            .as_ref()
            .unwrap()
            .as_ref(),
        &snapshot
    );
}
#[test]
fn local_tag_cache_cannot_bypass_copy_context_failures_or_change_fallback() {
    let mut types = vec![
        tag_type(2),
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(Type::I32),
        },
        Type::Enum {
            name: "Owned".into(),
            variants: vec![("A".into(), vec![Type::Managed(Box::new(Type::I32))])],
        },
    ];
    let invalid = Type::Task {
        result: Box::new(Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        }),
        retained_life: zil_ir::RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
    };
    types.push(Type::Enum {
        name: "BadContext".into(),
        variants: vec![("A".into(), vec![invalid])],
    });
    let f = function(types);
    for profile in 0..3 {
        for (local, declaration) in f.locals.iter().enumerate() {
            let cache = std::cell::RefCell::new(Cache::new(&f));
            let root = if local == 3 {
                Content::Sum(BTreeMap::from([(0, vec![Content::Copy])]), None)
            } else {
                (*unknown(&declaration.ty)).clone()
            };
            for value in [
                root.clone(),
                Content::Missing(Box::new(root.clone())),
                Content::Product(vec![root.clone()]),
                Content::Copy,
            ] {
                let mut old = Arc::new(value.clone());
                let old_alias = old.clone();
                let mut new = Arc::new(value);
                let new_alias = new.clone();
                let a = old_shared(&mut old, &declaration.ty, &mut budget(profile, 1_000_000));
                let b = forget_shared_copy_tags(
                    &mut new,
                    &declaration.ty,
                    &mut budget(profile, 1_000_000),
                    Some((&cache, local)),
                );
                assert_eq!(a, b);
                assert_eq!(old, new);
                assert_eq!(old_alias, new_alias);
                if let Err(error) = a {
                    assert_ne!(error.category, zil_base::ResultCategory::ResourceLimit);
                    assert_eq!(entries(&cache.borrow()), 0);
                }
            }
        }
    }
}
#[test]
fn local_tag_cache_reserves_table_and_deep_release_before_atomic_publication() {
    let f = function(vec![tag_type(2)]);
    let ty = &f.locals[0].ty;
    let run = |n| {
        let cache = std::cell::RefCell::new(Cache::new(&f));
        let mut value = unknown(ty);
        let before = value.clone();
        let result = forget_shared_copy_tags(&mut value, ty, &mut budget(1, n), Some((&cache, 0)));
        assert!(Arc::ptr_eq(&value, &before));
        let count = entries(&cache.borrow());
        let allocated = cache.borrow().slots.is_some();
        (result, count, allocated)
    };
    let (mut low, mut high) = (0, 1_000_000);
    assert!(run(high).0.is_ok());
    while low < high {
        let middle = (low + high) / 2;
        if run(middle).0.is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let n = low;
    assert_eq!(run(n).1, 1);
    assert!(run(n).0.is_ok());
    let (failure, count, allocated) = run(n - 1);
    assert_eq!(
        failure.unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(count, 0);
    assert!(!allocated);
    assert_eq!(run(n).1, 1);
    eprintln!("V local tag cache first publication N={n}; N-1/fresh checked");
}
#[test]
fn local_tag_cache_repeated_transfer_matches_old_and_pays_its_extra_cow() {
    let f = function(vec![tag_type(12)]);
    let ty = &f.locals[0].ty;
    let run = |cached: bool, n| {
        let cache = std::cell::RefCell::new(Cache::new(&f));
        let mut value = unknown(ty);
        let alias = value.clone();
        let mut b = budget(1, n);
        for _ in 0..64 {
            if cached {
                forget_shared_copy_tags(&mut value, ty, &mut b, Some((&cache, 0)))?;
            } else {
                old_shared(&mut value, ty, &mut b)?;
            }
        }
        assert_eq!(value, alias);
        drop(alias);
        if cached {
            assert_eq!(Arc::strong_count(&value), 2);
        }
        let before = b.remaining;
        if Arc::strong_count(&value) > 1 {
            value.cost(&mut b)?;
        }
        if let Content::Sum(_, route) = Arc::make_mut(&mut value) {
            *route = Some((9, vec![1, 0, 0]));
        }
        let extra_cow = before - b.remaining;
        if cached {
            assert!(extra_cow > 0);
        }
        Ok::<_, Diagnostic>(((*value).clone(), extra_cow))
    };
    let boundary = |cached| {
        let (mut low, mut high) = (0, 1_000_000);
        assert!(run(cached, high).is_ok());
        while low < high {
            let middle = (low + high) / 2;
            if run(cached, middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        low
    };
    let new_n = boundary(true);
    let old_n = boundary(false);
    assert!(new_n < old_n, "cache N={new_n}, old N={old_n}");
    assert_eq!(run(true, new_n).unwrap().0, run(false, old_n).unwrap().0);
    assert_eq!(run(true, new_n).unwrap(), run(true, new_n).unwrap());
    assert_eq!(
        run(true, new_n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(
        run(false, old_n - 1).unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    eprintln!(
        "V local tag cache 64 transfers + actual COW N={new_n}, uncached N={old_n}; N-1/fresh checked"
    );
}

fn cache_logical(cache: &Cache<'_>) -> Vec<Option<Content>> {
    cache.slots.as_ref().map_or_else(
        || vec![None; cache.function.locals.len()],
        |slots| {
            slots
                .iter()
                .map(|slot| {
                    slot.as_ref()
                        .and_then(|entry| entry.unknown.as_ref().map(|root| root.as_ref().clone()))
                })
                .collect()
        },
    )
}
#[test]
fn local_copy_index_matches_complete_c23d_transfer_cache_and_metadata() {
    use super::qualification_reference as old;
    let owned = Type::Enum {
        name: "NotCopy".into(),
        variants: vec![("A".into(), vec![Type::Managed(Box::new(Type::I32))])],
    };
    let f = function(vec![
        tag_type(3),
        owned,
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(Type::I32),
        },
    ]);
    for profile in 0..3 {
        let previous = std::cell::RefCell::new(old::OldCache::new(&f));
        let current = std::cell::RefCell::new(Cache::new(&f));
        let mut a = budget(profile, 1_000_000);
        let mut b = budget(profile, 1_000_000);
        for round in 0..4 {
            for (local, declaration) in f.locals.iter().enumerate() {
                let base = (*unknown(&declaration.ty)).clone();
                let mut variants = vec![
                    base.clone(),
                    Content::Missing(Box::new(base.clone())),
                    Content::Product(vec![base.clone()]),
                ];
                if let Content::Sum(arms, _) = &base {
                    let mut selected = arms.clone();
                    selected.pop_last();
                    variants.push(Content::Sum(selected, Some((0, vec![1, 0, 0]))));
                }
                for value in variants {
                    let mut left = Arc::new(value.clone());
                    let left_alias = left.clone();
                    let mut right = Arc::new(value);
                    let right_alias = right.clone();
                    let old_cache = indexed_invalidation(&a).then_some((&previous, local));
                    let new_cache = indexed_invalidation(&b).then_some((&current, local));
                    assert_eq!(
                        old::shared(&mut left, &declaration.ty, &mut a, old_cache),
                        forget_shared_copy_tags(&mut right, &declaration.ty, &mut b, new_cache)
                    );
                    assert_eq!(left, right);
                    assert_eq!(left_alias, right_alias);
                    assert_eq!(
                        previous.borrow().logical(),
                        cache_logical(&current.borrow())
                    );
                    assert_eq!(
                        a.addressed_copy_sums, b.addressed_copy_sums,
                        "round {round}, local {local}"
                    );
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
fn local_copy_index_keeps_false_distinct_from_shape_and_rejects_context_failures() {
    let owned = Type::Enum {
        name: "OwnedQualifier".into(),
        variants: vec![("A".into(), vec![Type::Managed(Box::new(Type::I32))])],
    };
    let f = function(vec![owned, tag_type(2)]);
    let mut cache = Cache::new(&f);
    let ty = &f.locals[0].ty;
    let value = unknown(ty);
    let mut b = budget(1, 1_000_000);
    assert!(!cache.known(0, &value, ty, &mut b).unwrap());
    let entry = cache.slots.as_ref().unwrap()[0].as_ref().unwrap();
    assert!(!entry.copy);
    assert!(entry.unknown.is_none());
    assert!(!cache.known(0, &value, ty, &mut b).unwrap());
    assert_eq!(entries(&cache), 0);
    // A Copy qualification alone cannot make a refined/missing route unknown.
    let copy_ty = &f.locals[1].ty;
    let value = Arc::new(Content::Sum(
        BTreeMap::from([(0, vec![Content::Copy])]),
        Some((0, vec![1, 0, 0])),
    ));
    assert!(!cache.known(1, &value, copy_ty, &mut b).unwrap());
    let entry = cache.slots.as_ref().unwrap()[1].as_ref().unwrap();
    assert!(entry.copy);
    assert!(entry.unknown.is_none());
    let equal_temporary = copy_ty.clone();
    let previous = cache_logical(&cache);
    assert!(!cache.known(1, &value, &equal_temporary, &mut b).unwrap());
    assert_eq!(cache_logical(&cache), previous);
    assert!(!cache.known(usize::MAX, &value, copy_ty, &mut b).unwrap());
    assert_eq!(cache_logical(&cache), previous);
    let invalid = Type::Enum {
        name: "BadTask".into(),
        variants: vec![(
            "A".into(),
            vec![Type::Task {
                result: Box::new(Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                }),
                retained_life: zil_ir::RetainedLifeShape {
                    binder_count: 0,
                    ports: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
            }],
        )],
    };
    let invalid_f = function(vec![invalid]);
    let invalid_ty = &invalid_f.locals[0].ty;
    let mut invalid_cache = Cache::new(&invalid_f);
    let root = Arc::new(Content::Sum(
        BTreeMap::from([(0, vec![Content::Copy])]),
        None,
    ));
    let mut old_budget = budget(1, 1_000_000);
    let mut new_budget = budget(1, 1_000_000);
    let expected = old_budget.addressed_copy_sum(invalid_ty).unwrap_err();
    let actual = invalid_cache
        .known(0, &root, invalid_ty, &mut new_budget)
        .unwrap_err();
    assert_eq!(actual, expected);
    assert_ne!(actual.category, zil_base::ResultCategory::ResourceLimit);
    assert!(invalid_cache.slots.is_none());
    assert!(new_budget.addressed_copy_sums.is_empty());
}
#[test]
fn local_copy_index_full_requests_have_independent_n_minus_one_and_fresh_boundaries() {
    use super::qualification_reference as old;
    let f = function(
        (0..16)
            .map(|index| Type::Enum {
                name: format!("Actual{index}"),
                variants: vec![("A".into(), vec![Type::I32]), ("B".into(), vec![Type::I32])],
            })
            .collect(),
    );
    let run = |original: bool, n| {
        let previous = std::cell::RefCell::new(old::OldCache::new(&f));
        let current = std::cell::RefCell::new(Cache::new(&f));
        let mut values: Vec<_> = f.locals.iter().map(|local| unknown(&local.ty)).collect();
        let aliases = values.clone();
        let mut b = budget(1, n);
        for _ in 0..32 {
            for (local, declaration) in f.locals.iter().enumerate() {
                if original {
                    old::shared(
                        &mut values[local],
                        &declaration.ty,
                        &mut b,
                        Some((&previous, local)),
                    )?;
                } else {
                    forget_shared_copy_tags(
                        &mut values[local],
                        &declaration.ty,
                        &mut b,
                        Some((&current, local)),
                    )?;
                }
            }
        }
        assert_eq!(values, aliases);
        Ok::<_, Diagnostic>((
            values
                .iter()
                .map(|root| root.as_ref().clone())
                .collect::<Vec<_>>(),
            b.addressed_copy_sums,
        ))
    };
    let boundary = |original| {
        let (mut low, mut high) = (0, 1_000_000);
        assert!(run(original, high).is_ok());
        while low < high {
            let middle = (low + high) / 2;
            if run(original, middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        low
    };
    let new_n = boundary(false);
    let old_n = boundary(true);
    assert!(new_n < old_n, "local N={new_n}, c23d N={old_n}");
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
    eprintln!(
        "V local Copy direct-index 16 actual locals x32 transfers N={new_n}, c23d N={old_n}; N-1/fresh checked"
    );
}
