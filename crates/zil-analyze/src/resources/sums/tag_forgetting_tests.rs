//! The scheduled tag operation must preserve the complete old tree semantics.
use super::*;

// Frozen pre-idempotence transfer. It deliberately rebuilds every Copy Sum,
// including all alternatives, then restores all old missing markers. This
// oracle never consults the optimized unknown-tag shape predicate.
impl Tree {
    fn forget_by_reconstruction(
        self: &mut Arc<Self>,
        ty: &Type,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        // The ordinary owning-error/borrowed-Option composition is found by
        // independent paid raw-type and actual error-place walks. It only
        // schedules this equivalent tag operation; it supplies no tag fact.
        let mixed = budget.store_option_results
            && (budget.task_borrowed_results || budget.ordinary_error_places);
        if budget.pod_stores || mixed {
            budget.spend(1, "Sum terminal tag classification")?;
            // Actual private facts already represent ordinary Record payloads
            // as Copy and managed headers as Leaf. Neither stores tag facts.
            // Do not rescan their complete types after every unrelated write.
            if matches!(self.node, Node::Copy | Node::Leaf(_)) {
                return Ok(());
            }
        }
        // This method starts at an immutable local type and descends only
        // through that type's actual children. Product nodes have no tag to
        // classify; the recursive walk below pays for their real children.
        let copy_sum = match &self.node {
            Node::Sum(_)
                if (budget.has_owned_store_package() || mixed)
                    && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) =>
            {
                budget.addressed_copy_sum(ty)?
            }
            Node::Sum(_) if matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) => {
                budget.type_work(ty, true, "mutable local Enum tag knowledge")?;
                ty.is_copy_value()
            }
            Node::Product(_) if budget.has_owned_store_package() || mixed => {
                budget.spend(1, "POD addressed Product tag descent")?;
                false
            }
            _ => {
                // Preserve the established fee path outside the newer POD
                // Store profile; its terminal facts still have no tag.
                budget.type_work(ty, true, "mutable local Enum tag knowledge")?;
                false
            }
        };
        if copy_sum {
            let old = self.copied(budget)?;
            let mut unknown = Self::unknown(ty, budget)?;
            unknown.preserve_missing(&old, budget)?;
            *self = unknown;
            return Ok(());
        }
        match (&mut edit(self, budget)?.node, ty) {
            (Node::Sum(vs), Type::Enum { variants, .. }) => {
                for (tag, fields) in vs {
                    for (child, t) in fields.iter_mut().zip(&variants[*tag as usize].1) {
                        child.forget_by_reconstruction(t, budget)?;
                    }
                }
            }
            (Node::Sum(vs), Type::ErrorUnion { error, result }) => {
                for (tag, fields) in vs {
                    if let Some(child) = fields.first_mut() {
                        child.forget_by_reconstruction(
                            if *tag == 1 { error } else { result },
                            budget,
                        )?;
                    }
                }
            }
            (Node::Product(children), Type::Tuple(fields)) => {
                for (child, ty) in children.iter_mut().zip(fields) {
                    child.forget_by_reconstruction(ty, budget)?;
                }
            }
            (Node::Product(children), Type::Package { fields, .. }) => {
                for (child, (_, t)) in children.iter_mut().zip(fields) {
                    child.forget_by_reconstruction(t, budget)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn scalar_enum(name: &str) -> Type {
    Type::Enum {
        name: name.into(),
        variants: vec![("First".into(), vec![]), ("Second".into(), vec![])],
    }
}

fn vary(tree: &mut Arc<Tree>, tags: usize, missing: usize, at: &mut usize) {
    let node = Arc::make_mut(tree);
    node.missing = missing & (1 << (*at % 8)) != 0;
    *at += 1;
    match &mut node.node {
        Node::Product(fields) => {
            for field in fields {
                vary(field, tags, missing, at);
            }
        }
        Node::Sum(choices) => {
            // Every retained alternative keeps its original index and fields.
            // Zero is the ordinary unrefined state; the other two choices are
            // the actual states obtained by either outcome of a tag guard.
            let selected = (tags >> (*at % 4)) % 3;
            if selected != 0 {
                choices.retain(|tag, _| *tag == (selected - 1) as u32);
            }
            for fields in choices.values_mut() {
                for field in fields {
                    vary(field, tags, missing, at);
                }
            }
        }
        Node::Leaf(births) if missing & 4 != 0 => {
            births.insert(Birth::Created);
        }
        _ => {}
    }
}

#[test]
fn scheduled_forgetting_matches_full_reconstruction_with_nested_tags_and_moves() {
    let inner = scalar_enum("Inner");
    let outer = Type::Enum {
        name: "Outer".into(),
        variants: vec![
            ("Pair".into(), vec![inner.clone(), Type::I32]),
            ("Other".into(), vec![inner.clone()]),
        ],
    };
    let union = Type::ErrorUnion {
        error: Box::new(inner.clone()),
        result: Box::new(outer.clone()),
    };
    let owner = Type::Package {
        name: "Owner".into(),
        fields: vec![
            ("data".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ("state".into(), union.clone()),
        ],
    };
    for ty in [inner, outer, union, owner] {
        for tags in 0..16 {
            for missing in 0..32 {
                let mut budget = Budget::new(&Limits::default());
                let mut original = Tree::unknown(&ty, &mut budget).unwrap();
                vary(&mut original, tags, missing, &mut 0);
                let input = Arc::clone(&original);
                original
                    .forget_by_reconstruction(&ty, &mut Budget::new(&Limits::default()))
                    .unwrap();
                for profile in 0..3 {
                    let mut scheduled = Arc::clone(&input);
                    let mut budget = Budget::new(&Limits::default());
                    if profile == 1 {
                        budget.pod_stores = true;
                    }
                    if profile == 2 {
                        budget.store_option_results = true;
                        budget.ordinary_error_places = true;
                    }
                    scheduled
                        .forget_addressed_copy_tags(&ty, &mut budget)
                        .unwrap();
                    assert_eq!(
                        original, scheduled,
                        "complete tag/move/resource tree differs: {ty:?}, tags={tags}, missing={missing}, profile={profile}"
                    );
                    scheduled
                        .forget_addressed_copy_tags(&ty, &mut budget)
                        .unwrap();
                    assert_eq!(
                        original, scheduled,
                        "repeated forgetting must be idempotent"
                    );
                }
            }
        }
    }
}

#[test]
fn unknown_tag_shape_checks_exact_terminals_and_all_alternative_inventories() {
    let scalar = scalar_enum("Scalar");
    let sum = |entries| {
        Arc::new(Tree {
            missing: true,
            node: Node::Sum(entries),
        })
    };
    let copy = Arc::new(Tree {
        missing: true,
        node: Node::Copy,
    });
    let leaf = Arc::new(Tree {
        missing: true,
        node: Node::Leaf(BTreeSet::from([Birth::Incoming, Birth::Created])),
    });
    let union = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(scalar.clone()),
    };
    let scalar_tree = Tree::unknown(&scalar, &mut Budget::new(&Limits::default())).unwrap();
    let package = Type::Package {
        name: "Actual".into(),
        fields: vec![("f".into(), Type::I32)],
    };
    let tuple = Type::Tuple(vec![Type::I32]);
    let mut invalid = vec![
        (copy.clone(), Type::Managed(Box::new(Type::ByteBuffer))),
        (copy.clone(), tuple.clone()),
        (copy.clone(), package.clone()),
        (copy.clone(), scalar.clone()),
        (copy.clone(), union.clone()),
        (leaf.clone(), Type::I32),
        (leaf.clone(), Type::Managed(Box::new(Type::ByteBuffer))),
        (
            Arc::new(Tree {
                missing: false,
                node: Node::Product(vec![]),
            }),
            tuple,
        ),
        (
            Arc::new(Tree {
                missing: false,
                node: Node::Product(vec![]),
            }),
            package,
        ),
        // Same length, wrong key must not be mistaken for all alternatives.
        (
            sum(BTreeMap::from([(0, vec![]), (2, vec![])])),
            scalar.clone(),
        ),
        (
            sum(BTreeMap::from([(0, vec![copy.clone()]), (1, vec![])])),
            scalar.clone(),
        ),
        (sum(BTreeMap::from([(0, vec![])])), scalar.clone()),
        (
            sum(BTreeMap::from([
                (0, vec![scalar_tree.clone()]),
                (2, vec![copy.clone()]),
            ])),
            union.clone(),
        ),
        (
            sum(BTreeMap::from([
                (0, vec![scalar_tree.clone()]),
                (1, vec![]),
            ])),
            union.clone(),
        ),
        // The two arms have different actual shapes; swapping error/success
        // must fail even though both maps contain 0/1 and one child per arm.
        (
            sum(BTreeMap::from([
                (0, vec![copy.clone()]),
                (1, vec![scalar_tree.clone()]),
            ])),
            union.clone(),
        ),
    ];
    // A nested Copy in place of an actual child Enum must also be detected.
    invalid.push((
        sum(BTreeMap::from([
            (0, vec![copy.clone()]),
            (1, vec![copy.clone()]),
        ])),
        union,
    ));
    for (tree, ty) in invalid {
        assert!(
            !tree
                .tags_unknown(&ty, &mut Budget::new(&Limits::default()))
                .unwrap(),
            "{tree:?} {ty:?}"
        );
        // Retain the old fallback result on malformed private inputs as well;
        // this is not a claim that malformed raw IR passes the outer kernels.
        let mut old = tree.clone();
        old.forget_by_reconstruction(&ty, &mut Budget::new(&Limits::default()))
            .unwrap();
        let mut new = tree;
        new.forget_addressed_copy_tags(&ty, &mut Budget::new(&Limits::default()))
            .unwrap();
        assert_eq!(new, old);
    }
    // Record and Array are Copy terminals in the actual Tree model, even when
    // their inline Copy payload itself contains an Enum. Do not invent a new
    // representation or descend through them just for this optimization.
    for ty in [
        Type::I32,
        Type::Record {
            name: "R".into(),
            fields: vec![("f".into(), scalar.clone())],
        },
        Type::Array {
            element: Box::new(scalar),
            length: 3,
        },
    ] {
        assert!(
            copy.tags_unknown(&ty, &mut Budget::new(&Limits::default()))
                .unwrap()
        );
    }
}

#[test]
fn generic_unknown_fixed_point_preserves_shared_missing_trees_and_owned_births() {
    let ty = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(scalar_enum("State")),
    };
    let mut full = Tree::unknown(&ty, &mut Budget::new(&Limits::default())).unwrap();
    vary(&mut full, 0, 31, &mut 0);
    let frozen = full.clone();
    full.forget_addressed_copy_tags(&ty, &mut Budget::new(&Limits::default()))
        .unwrap();
    assert!(
        Arc::ptr_eq(&full, &frozen),
        "exact fixed point shares the actual immutable tree"
    );
    assert_eq!(full, frozen);
    let mut narrowed = full.clone();
    let Node::Sum(choices) = &mut edit(&mut narrowed, &mut Budget::new(&Limits::default()))
        .unwrap()
        .node
    else {
        panic!("Sum")
    };
    choices.remove(&1);
    let snapshot = narrowed.clone();
    let mut old = narrowed.clone();
    old.forget_by_reconstruction(&ty, &mut Budget::new(&Limits::default()))
        .unwrap();
    narrowed
        .forget_addressed_copy_tags(&ty, &mut Budget::new(&Limits::default()))
        .unwrap();
    assert_eq!(narrowed, old);
    assert_ne!(narrowed, snapshot);
    assert_eq!(full, frozen);
    let owner = Type::Package {
        name: "Owner".into(),
        fields: vec![
            ("owned".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ("status".into(), ty),
        ],
    };
    let mut t = Tree::unknown(&owner, &mut Budget::new(&Limits::default())).unwrap();
    let Node::Product(fields) = &mut edit(&mut t, &mut Budget::new(&Limits::default()))
        .unwrap()
        .node
    else {
        panic!("Product")
    };
    let born = BTreeSet::from([Birth::Argument(4), Birth::Captured(9), Birth::Created]);
    edit(&mut fields[0], &mut Budget::new(&Limits::default()))
        .unwrap()
        .node = Node::Leaf(born.clone());
    let mut old = t.clone();
    old.forget_by_reconstruction(&owner, &mut Budget::new(&Limits::default()))
        .unwrap();
    t.forget_addressed_copy_tags(&owner, &mut Budget::new(&Limits::default()))
        .unwrap();
    assert_eq!(t, old);
    let Node::Product(fields) = &t.node else {
        panic!("Product")
    };
    assert_eq!(fields[0].birth(), Some(&born));
}

#[test]
fn unknown_tag_fixed_point_has_an_exact_smaller_work_boundary() {
    let inner = scalar_enum("BudgetInner");
    let ty = Type::ErrorUnion {
        error: Box::new(inner.clone()),
        result: Box::new(Type::Enum {
            name: "BudgetOuter".into(),
            variants: vec![
                ("Pair".into(), vec![inner.clone(), Type::I32]),
                ("Other".into(), vec![inner]),
            ],
        }),
    };
    let mut input = Tree::unknown(&ty, &mut Budget::new(&Limits::default())).unwrap();
    vary(&mut input, 0, 31, &mut 0);
    let mut expected = input.clone();
    expected
        .forget_by_reconstruction(&ty, &mut Budget::new(&Limits::default()))
        .unwrap();
    let run = |fixed_point, steps| {
        let mut tree = input.clone();
        let mut budget = Budget::new(&Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        });
        if fixed_point {
            tree.forget_addressed_copy_tags(&ty, &mut budget)?;
        } else {
            tree.forget_by_reconstruction(&ty, &mut budget)?;
        }
        Ok::<_, Vec<zil_base::Diagnostic>>(tree)
    };
    let minimum = |fixed_point| {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        assert_eq!(run(fixed_point, high).unwrap(), expected);
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(fixed_point, middle) {
                Ok(tree) => {
                    assert_eq!(tree, expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(!errors.is_empty());
                    assert!(
                        errors.iter().all(|e| e.phase == zil_base::Phase::Analyze
                            && e.category == zil_base::ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
            assert_eq!(
                input, expected,
                "failed or successful transfer changed its shared input"
            );
        }
        let tree = run(fixed_point, high).unwrap();
        assert_eq!(tree, expected);
        if fixed_point {
            assert!(Arc::ptr_eq(&tree, &input));
        }
        let errors = run(fixed_point, high - 1).unwrap_err();
        assert!(!errors.is_empty());
        assert!(
            errors.iter().all(|e| e.phase == zil_base::Phase::Analyze
                && e.category == zil_base::ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        assert_eq!(run(fixed_point, high).unwrap(), expected);
        assert_eq!(input, expected, "N-1 or retry changed its shared input");
        high
    };
    let fixed_point = minimum(true);
    let reconstruction = minimum(false);
    assert!(
        fixed_point < reconstruction,
        "fixed_point={fixed_point}, reconstruction={reconstruction}"
    );
    println!(
        "complete unknown-tag forgetting: fixed point N={fixed_point}, reconstruction N={reconstruction}; N-1/fresh agree"
    );
}
