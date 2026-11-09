use super::*;

fn use_permission_unfiltered(
    permission: &Permission,
    write: bool,
    active: &Loans,
    loans: &[Loan],
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    permission_head(permission, write, b)?;
    let ignore = ancestry(&permission.loans, loans, b)?;
    permission_tail(permission, write, active, loans, &ignore, apart, b)
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
fn permission(base: HoldRoot, field: u32, ids: &[usize]) -> Permission {
    Permission {
        targets: BTreeSet::from([target(base, field)]),
        loans: ids.iter().copied().collect(),
    }
}
fn budget(steps: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: steps,
        ..Limits::default()
    })
}
#[test]
fn step_access_matches_old_full_checks_for_interleaved_permissions_and_errors() {
    for flags in 0..16 {
        let mut loans = (0..4)
            .map(|id| Loan {
                targets: BTreeSet::from([target(
                    HoldRoot::Parameter(if id == 1 { 0 } else { id as u32 + 1 }),
                    0,
                )]),
                mutable: flags & (1 << id) != 0,
                ..Loan::default()
            })
            .collect::<Vec<_>>();
        loans[2].parents.insert(3);
        loans[3].parents.insert(2);
        for active in [vec![], vec![0, 1], vec![2, 3], vec![0, 1, usize::MAX]] {
            let active = active.into_iter().collect::<Loans>();
            for static_utf8 in [false, true] {
                let mut old_budget = Budget::new(&Limits::default());
                let mut new_budget = Budget::new(&Limits::default());
                old_budget.static_utf8 = static_utf8;
                new_budget.static_utf8 = static_utf8;
                let seed = (
                    target(HoldRoot::Parameter(8), 0),
                    target(HoldRoot::Parameter(9), 0),
                );
                let mut old_apart = BTreeSet::from([seed.clone()]);
                let mut new_apart = old_apart.clone();
                let mut access = Access::new(&active, &loans, &mut new_budget).unwrap();
                for (pointer, write) in [
                    (permission(HoldRoot::Parameter(0), 0, &[]), false),
                    (permission(HoldRoot::Parameter(0), 0, &[0]), true),
                    (permission(HoldRoot::Parameter(0), 0, &[2]), false),
                    (permission(HoldRoot::Static, 0, &[]), true),
                    (Permission::default(), false),
                    (permission(HoldRoot::Parameter(0), 0, &[usize::MAX]), false),
                    (permission(HoldRoot::Parameter(7), 0, &[]), true),
                ] {
                    let old = use_permission_unfiltered(
                        &pointer,
                        write,
                        &active,
                        &loans,
                        &mut old_apart,
                        &mut old_budget,
                    );
                    let new = access.check(&pointer, write, &mut new_apart, &mut new_budget);
                    assert_eq!(
                        (new, &new_apart),
                        (old, &old_apart),
                        "flags={flags} active={active:?} static={static_utf8} write={write} pointer={pointer:?}"
                    );
                }
            }
        }
    }
}
#[test]
fn fresh_step_reads_current_flags_and_preserves_head_parent_and_invalid_order() {
    let active = Loans::from([0, 1, usize::MAX]);
    let mut loans = vec![Loan::default(); 2];
    loans[1].mutable = true;
    let pointer = permission(HoldRoot::Parameter(0), 0, &[]);
    let mut b = Budget::new(&Limits::default());
    {
        let mut access = Access::new(&active, &loans, &mut b).unwrap();
        assert_eq!(
            access.read_candidates(&mut b).unwrap(),
            &Loans::from([1, usize::MAX])
        );
        for ignore in [Loans::new(), Loans::from([usize::MAX])] {
            let mut old_apart = BTreeSet::new();
            let mut new_apart = BTreeSet::new();
            let old = permission_tail(
                &pointer,
                false,
                &active,
                &loans,
                &ignore,
                &mut old_apart,
                &mut b,
            );
            let new = permission_tail(
                &pointer,
                false,
                access.read_candidates(&mut b).unwrap(),
                &loans,
                &ignore,
                &mut new_apart,
                &mut b,
            );
            assert_eq!((new, new_apart), (old, old_apart));
        }
    }
    loans[0].mutable = true;
    loans[1].mutable = false;
    let mut access = Access::new(&active, &loans, &mut b).unwrap();
    assert_eq!(
        access.read_candidates(&mut b).unwrap(),
        &Loans::from([0, usize::MAX])
    );

    let mut invalid_parent = loans.clone();
    invalid_parent[0].parents.insert(usize::MAX);
    for (pointer, write, table, static_utf8, message) in [
        (
            Permission::default(),
            false,
            &loans,
            false,
            "no actual target permission",
        ),
        (
            permission(HoldRoot::Parameter(0), 0, &[usize::MAX]),
            false,
            &loans,
            false,
            "loan ancestry",
        ),
        (
            permission(HoldRoot::Parameter(0), 0, &[0]),
            false,
            &invalid_parent,
            false,
            "loan ancestry",
        ),
        (
            permission(HoldRoot::Static, 0, &[usize::MAX]),
            true,
            &loans,
            true,
            "no mutable authority",
        ),
    ] {
        let mut b = Budget::new(&Limits::default());
        b.static_utf8 = static_utf8;
        let mut access = Access::new(&active, table, &mut b).unwrap();
        let error = access
            .check(&pointer, write, &mut BTreeSet::new(), &mut b)
            .unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains(message)),
            "{error:?}"
        );
        assert!(
            access.reads.is_none(),
            "head/ancestry errors precede filter construction"
        );
    }
}
fn many_reads(steps: usize) -> CompileResult<usize> {
    let active = (0..512).collect::<Loans>();
    let loans = vec![Loan::default(); 512];
    let pointer = permission(HoldRoot::Parameter(0), 0, &[]);
    let mut b = budget(steps);
    let mut access = Access::new(&active, &loans, &mut b)?;
    for _ in 0..512 {
        access.check(&pointer, false, &mut BTreeSet::new(), &mut b)?;
    }
    Ok(b.remaining())
}
#[test]
fn one_step_pays_one_real_filter_for_many_leaves_with_fresh_exact_limits() {
    let active = (0..512).collect::<Loans>();
    let loans = vec![Loan::default(); 512];
    let pointer = permission(HoldRoot::Parameter(0), 0, &[]);
    let limit = Limits::default().max_analysis_steps;
    let mut old = budget(limit);
    for _ in 0..512 {
        use_permission_unfiltered(
            &pointer,
            false,
            &active,
            &loans,
            &mut BTreeSet::new(),
            &mut old,
        )
        .unwrap();
    }
    let remaining = many_reads(limit).unwrap();
    let n = limit - remaining;
    assert!(remaining > old.remaining());
    println!(
        "512 leaves x 512 shared loans old={} new={n}",
        limit - old.remaining()
    );
    for steps in [n, n - 1, n] {
        let result = many_reads(steps);
        if steps == n {
            assert_eq!(result.unwrap(), 0);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }
    let mutable = vec![
        Loan {
            mutable: true,
            ..Loan::default()
        };
        512
    ];
    for steps in [1, 1026, 1050] {
        let mut b = budget(steps);
        let mut access = Access::new(&active, &mutable, &mut b).unwrap();
        assert!(
            access
                .check(&pointer, false, &mut BTreeSet::new(), &mut b)
                .is_err()
        );
        assert!(
            access.reads.is_none(),
            "failed filter must never publish a partial set"
        );
    }
    let mut old = budget(limit);
    let mut new = budget(limit);
    let mut access = Access::new(&active, &loans, &mut new).unwrap();
    let after_constructor = new.remaining();
    use_permission_unfiltered(
        &pointer,
        true,
        &active,
        &loans,
        &mut BTreeSet::new(),
        &mut old,
    )
    .unwrap();
    access
        .check(&pointer, true, &mut BTreeSet::new(), &mut new)
        .unwrap();
    assert_eq!(after_constructor - new.remaining(), limit - old.remaining());
    assert!(
        access.reads.is_none(),
        "write retains the full old active path"
    );
}
