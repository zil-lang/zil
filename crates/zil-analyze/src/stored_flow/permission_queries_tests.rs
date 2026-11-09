use super::*;
fn permission_tail_by_ignore_first(
    permission: &Permission,
    write: bool,
    active: &Loans,
    loans: &[Loan],
    ignore: &Loans,
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    for id in active {
        ordered(b, ignore.len(), 0)?;
        if ignore.contains(id) {
            continue;
        }
        let loan = loans.get(*id).ok_or_else(|| failure("live loan absent"))?;
        if write || loan.mutable {
            for target in &permission.targets {
                for held in &loan.targets {
                    separate(target, held, apart, b)?;
                }
            }
        }
    }
    Ok(())
}
fn target(base: HoldRoot, field: u32) -> Target {
    Target {
        retained_port: None,
        base,
        stored: vec![],
        place: vec![PlaceProjection::Field(field)],
        heap_element: None,
    }
}
#[test]
fn read_loan_shortcut_keeps_all_apart_effects_diagnostics_and_current_flags() {
    let pointer = target(HoldRoot::Parameter(0), 7);
    let permission = Permission {
        targets: BTreeSet::from([pointer.clone()]),
        ..Permission::default()
    };
    let seed = (
        target(HoldRoot::Parameter(2), 0),
        target(HoldRoot::Parameter(3), 1),
    );
    for mutable in 0..8 {
        let loans = (0..3)
            .map(|i| Loan {
                mutable: mutable & (1 << i) != 0,
                targets: BTreeSet::from([if i == 1 {
                    target(HoldRoot::Parameter(1), 2)
                } else {
                    pointer.clone()
                }]),
                ..Loan::default()
            })
            .collect::<Vec<_>>();
        for active in [
            vec![],
            vec![0],
            vec![1],
            vec![2],
            vec![usize::MAX],
            vec![0, 1],
            vec![0, 1, 2],
            vec![0, 1, usize::MAX],
        ] {
            let active = active.into_iter().collect::<Loans>();
            for mask in 0..16 {
                let ignore = [0, 1, 2, usize::MAX]
                    .into_iter()
                    .enumerate()
                    .filter(|(bit, _)| mask & (1 << bit) != 0)
                    .map(|(_, id)| id)
                    .collect::<Loans>();
                for write in [false, true] {
                    let mut old = BTreeSet::from([seed.clone()]);
                    let mut new = old.clone();
                    let expected = permission_tail_by_ignore_first(
                        &permission,
                        write,
                        &active,
                        &loans,
                        &ignore,
                        &mut old,
                        &mut Budget::new(&Limits::default()),
                    );
                    let actual = permission_tail(
                        &permission,
                        write,
                        &active,
                        &loans,
                        &ignore,
                        &mut new,
                        &mut Budget::new(&Limits::default()),
                    );
                    assert_eq!(
                        (actual, new),
                        (expected, old),
                        "complete result and apart error prefix: mutable={mutable} active={active:?} ignore={ignore:?} write={write}"
                    );
                }
            }
        }
    }
}
#[test]
fn read_shortcut_preserves_invalid_arena_order_and_separate_head_guards() {
    let permission = Permission {
        targets: BTreeSet::from([target(HoldRoot::Parameter(0), 0)]),
        ..Permission::default()
    };
    let active = BTreeSet::from([usize::MAX]);
    for write in [false, true] {
        permission_tail(
            &permission,
            write,
            &active,
            &[],
            &active,
            &mut BTreeSet::new(),
            &mut Budget::new(&Limits::default()),
        )
        .unwrap();
        let errors = permission_tail(
            &permission,
            write,
            &active,
            &[],
            &Loans::new(),
            &mut BTreeSet::new(),
            &mut Budget::new(&Limits::default()),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live loan absent"))
        );
    }
    let mut loans = vec![Loan {
        targets: permission.targets.clone(),
        ..Loan::default()
    }];
    let mut apart = BTreeSet::new();
    permission_tail(
        &permission,
        false,
        &BTreeSet::from([0]),
        &loans,
        &Loans::new(),
        &mut apart,
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    assert!(apart.is_empty());
    loans[0].mutable = true;
    assert!(
        permission_tail(
            &permission,
            false,
            &BTreeSet::from([0]),
            &loans,
            &Loans::new(),
            &mut apart,
            &mut Budget::new(&Limits::default())
        )
        .is_err()
    );
    loans[0].mutable = false;
    assert!(
        permission_tail(
            &permission,
            true,
            &BTreeSet::from([0]),
            &loans,
            &Loans::new(),
            &mut apart,
            &mut Budget::new(&Limits::default())
        )
        .is_err()
    );
    assert!(
        use_permission(
            &Permission::default(),
            false,
            &BTreeSet::from([0]),
            &loans,
            &mut apart,
            &mut Budget::new(&Limits::default())
        )
        .unwrap_err()
        .iter()
        .any(|d| d.message.contains("no actual target permission"))
    );
}
#[test]
fn shared_read_removes_real_ignore_searches_with_exact_fresh_work() {
    let active = (0..512).collect::<Loans>();
    let ignore = active.clone();
    let loans = vec![Loan::default(); 512];
    let permission = Permission {
        targets: BTreeSet::from([target(HoldRoot::Parameter(0), 0)]),
        ..Permission::default()
    };
    let mut old = Budget::new(&Limits::default());
    let mut new = Budget::new(&Limits::default());
    permission_tail_by_ignore_first(
        &permission,
        false,
        &active,
        &loans,
        &ignore,
        &mut BTreeSet::new(),
        &mut old,
    )
    .unwrap();
    permission_tail(
        &permission,
        false,
        &active,
        &loans,
        &ignore,
        &mut BTreeSet::new(),
        &mut new,
    )
    .unwrap();
    assert!(new.remaining() > old.remaining());
    let n = Limits::default().max_analysis_steps - new.remaining();
    println!(
        "512 shared read ignore checks old={} new={n}",
        Limits::default().max_analysis_steps - old.remaining()
    );
    for steps in [n, n - 1, n] {
        let result = permission_tail(
            &permission,
            false,
            &active,
            &loans,
            &ignore,
            &mut BTreeSet::new(),
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == n {
            result.unwrap();
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }
    let mut old = Budget::new(&Limits::default());
    let mut new = Budget::new(&Limits::default());
    permission_tail_by_ignore_first(
        &permission,
        true,
        &active,
        &loans,
        &ignore,
        &mut BTreeSet::new(),
        &mut old,
    )
    .unwrap();
    permission_tail(
        &permission,
        true,
        &active,
        &loans,
        &ignore,
        &mut BTreeSet::new(),
        &mut new,
    )
    .unwrap();
    assert_eq!(
        new.remaining(),
        old.remaining(),
        "writes retain the original traversal and all fees"
    );
}
