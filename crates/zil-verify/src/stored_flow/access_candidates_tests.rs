use super::*;

fn access_unfiltered(
    p: &Permission,
    write: bool,
    active: &[usize],
    loans: &[Loan],
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    access_head(p, write, b)?;
    let delegated = ancestry(&p.loans, loans, b)?;
    access_tail(p, write, active, loans, &delegated, requirements, b)
}

#[allow(clippy::too_many_arguments)]
fn check_call_unfiltered(
    module: &TypedModule,
    f: &TypedFunction,
    function: FunctionId,
    arguments: &[ValueId],
    solution: &Solution,
    state: &Frame,
    active: &[usize],
    summaries: &[Summary],
    requirements: &mut Vec<(Source, Source)>,
    byte_flow: Option<&byte_queries::Flow>,
    byte_point: Option<&byte_queries::Point>,
    b: &mut Budget,
) -> Check<()> {
    let callee = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| err("call target absent"))?;
    if arguments.len() != callee.parameters.len() {
        return Err(err("call arity differs from actual body signature"));
    }
    for (position, argument) in arguments.iter().enumerate() {
        let value = &solution.values[argument.0 as usize];
        let ty = &callee.locals[callee.parameters[position].0 as usize].ty;
        if let Some((_, shape)) = addressed(ty) {
            for slot in &shape.slots {
                require_filled(state, &value.address, &slot.path, b)?;
            }
        }
        if let Type::Address { mutable, .. } = ty {
            access_unfiltered(
                &value.address,
                *mutable,
                active,
                &solution.loans,
                requirements,
                b,
            )?;
            if b.stored_slot_updates
                && *mutable
                && let Some((_, shape)) = addressed(ty)
            {
                for slot in &shape.slots {
                    let held = select(state, &value.address, &slot.path, b)?;
                    access_unfiltered(
                        &held,
                        slot.mode == StoredBorrowMode::ExclusiveAddress,
                        active,
                        &solution.loans,
                        requirements,
                        b,
                    )?;
                }
            }
        }
        for (_, p) in &value.leaves {
            access_unfiltered(p, false, active, &solution.loans, requirements, b)?;
        }
        for earlier in &arguments[..position] {
            let right = &solution.values[earlier.0 as usize];
            let left_mut = matches!(vt(f, *argument)?, Type::Address { mutable: true, .. });
            let right_mut = matches!(vt(f, *earlier)?, Type::Address { mutable: true, .. });
            if left_mut || right_mut {
                for a in &value.address.sources {
                    for z in &right.address.sources {
                        apart(a, z, requirements, b)?;
                    }
                }
            }
            if left_mut {
                for (_, p) in &right.leaves {
                    for a in &value.address.sources {
                        for z in &p.sources {
                            apart(a, z, requirements, b)?;
                        }
                    }
                }
            }
            if right_mut {
                for (_, p) in &value.leaves {
                    for a in &p.sources {
                        for z in &right.address.sources {
                            apart(a, z, requirements, b)?;
                        }
                    }
                }
            }
        }
    }
    for (a, z) in &summaries[function.0 as usize].apart {
        let weak =
            |source: &Source| match source.base {
                HoldRoot::Parameter(position) => callee
                    .parameters
                    .get(position as usize)
                    .is_some_and(|local| {
                        byte_queries::is_description(&callee.locals[local.0 as usize].ty)
                    }),
                _ => false,
            };
        if b.byte_queries && (weak(a) || weak(z)) {
            let flow =
                byte_flow.ok_or_else(|| err("Byte separation has no covered origin flow"))?;
            let left = flow.instantiate(
                a,
                callee,
                arguments,
                &solution.values,
                state,
                byte_point.ok_or_else(|| err("Byte separation has no current proof"))?,
                b,
            )?;
            let right = flow.instantiate(
                z,
                callee,
                arguments,
                &solution.values,
                state,
                byte_point.ok_or_else(|| err("Byte separation has no current proof"))?,
                b,
            )?;
            for a in &left {
                for z in &right {
                    for x in &a.sources {
                        for y in &z.sources {
                            apart(x, y, requirements, b)?;
                        }
                    }
                }
            }
            continue;
        }
        let a = instantiate(a, arguments, &solution.values, state, b)?;
        let z = instantiate(z, arguments, &solution.values, state, b)?;
        if a.sources.is_empty() || z.sources.is_empty() {
            return Err(err("call separation has incomplete actual target facts"));
        }
        for a in &a.sources {
            for z in &z.sources {
                apart(a, z, requirements, b)?;
            }
        }
    }
    Ok(())
}

fn source(base: HoldRoot, inline: Path) -> Source {
    Source {
        retained_port: None,
        base,
        leaf: vec![],
        inline,
        heap_element: None,
    }
}
fn permission(base: HoldRoot, loans: &[usize]) -> Permission {
    Permission {
        sources: vec![source(base, vec![])],
        loans: loans.to_vec(),
    }
}
fn budget(steps: usize) -> Budget {
    Budget::new(&Limits {
        max_analysis_steps: steps,
        ..Limits::default()
    })
}
#[test]
fn step_candidates_preserve_full_old_results_requirement_order_and_error_prefixes() {
    for flags in 0..16 {
        let mut loans = (0..4)
            .map(|id| Loan {
                sources: vec![source(
                    HoldRoot::Parameter(if id == 1 { 0 } else { id + 1 }),
                    vec![],
                )],
                exclusive: flags & (1 << id) != 0,
                ..Loan::default()
            })
            .collect::<Vec<_>>();
        loans[2].parents = vec![3, 3];
        loans[3].parents = vec![2];
        for active in [
            vec![],
            vec![0, 1],
            vec![2, 3],
            vec![3, 2, 1, 0, 1],
            vec![0, usize::MAX, 0],
        ] {
            for static_utf8 in [false, true] {
                let mut old_budget = Budget::new(&Limits::default());
                let mut new_budget = Budget::new(&Limits::default());
                old_budget.static_utf8 = static_utf8;
                new_budget.static_utf8 = static_utf8;
                let seed = (
                    source(HoldRoot::Parameter(8), vec![]),
                    source(HoldRoot::Parameter(9), vec![]),
                );
                let mut old_requirements = vec![seed.clone(), seed];
                let mut new_requirements = old_requirements.clone();
                let mut checks = Access::new(&active, &loans, &mut new_budget).unwrap();
                for (pointer, write) in [
                    (permission(HoldRoot::Parameter(0), &[]), false),
                    (permission(HoldRoot::Parameter(0), &[0, 0]), true),
                    (permission(HoldRoot::Parameter(0), &[2]), false),
                    (permission(HoldRoot::Static, &[]), true),
                    (Permission::default(), false),
                    (permission(HoldRoot::Parameter(0), &[usize::MAX]), false),
                    (permission(HoldRoot::Parameter(7), &[]), true),
                ] {
                    let old = access_unfiltered(
                        &pointer,
                        write,
                        &active,
                        &loans,
                        &mut old_requirements,
                        &mut old_budget,
                    );
                    let new = checks.check(&pointer, write, &mut new_requirements, &mut new_budget);
                    assert_eq!(
                        (new, &new_requirements),
                        (old, &old_requirements),
                        "flags={flags} active={active:?} static={static_utf8} write={write} pointer={pointer:?}"
                    );
                }
            }
        }
    }
}
#[allow(dead_code)]
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
    ));
}
#[test]
fn complete_call_oracle_preserves_held_leaf_argument_and_summary_checks() {
    for mutable in [false, true] {
        for mode in 0u32..8 {
            let mut module = fixture::module();
            let mut ty = module.functions[1].locals[0].ty.clone();
            let Type::Address {
                mutable: actual_mutable,
                inner,
            } = &mut ty
            else {
                unreachable!()
            };
            *actual_mutable = mutable;
            let Type::BorrowedStorage { storage, life } = inner.as_mut() else {
                unreachable!()
            };
            let Type::Record { fields, .. } = storage.as_mut() else {
                unreachable!()
            };
            let child = fields[0].1.clone();
            fields.push(("second".into(), child));
            let mut second_slot = life.slots[0].clone();
            second_slot.path = vec![PlaceProjection::Field(1)];
            life.slots.push(second_slot);
            let callee = &mut module.functions[1];
            callee.locals[0].ty = ty.clone();
            let mut second = callee.locals[0].clone();
            second.id = LocalId(1);
            second.name = "second".into();
            callee.locals.push(second);
            callee.parameters.push(LocalId(1));
            module.functions[2].values = vec![ty.clone(), ty].into();
            let caller = &module.functions[2];
            let mut b = Budget::new(&Limits::default());
            let mut values = Vec::new();
            let mut external = Vec::new();
            for position in 0..2 {
                let mut value = Payload {
                    address: permission(HoldRoot::Parameter(position), &[]),
                    ..Payload::default()
                };
                let mut holder = Payload::default();
                for field in 0..2 {
                    let path = vec![PlaceProjection::Field(field)];
                    let held = permission(HoldRoot::Parameter(4 + position * 2 + field), &[0]);
                    value
                        .leaves
                        .push((path.clone(), held.clone()), &mut b)
                        .unwrap();
                    holder.leaves.push((path, held), &mut b).unwrap();
                }
                values.push(value);
                external.push(holder);
            }
            let mut state = Frame {
                external,
                ..Frame::default()
            };
            let mut loans = vec![Loan {
                sources: vec![source(
                    HoldRoot::Parameter(if mode == 1 { 4 } else { 12 }),
                    vec![],
                )],
                exclusive: mode.is_multiple_of(2),
                ..Loan::default()
            }];
            let mut active = vec![0, 0];
            let mut args = vec![ValueId(0), ValueId(1)];
            let mut function = FunctionId(1);
            match mode {
                2 => active.push(usize::MAX),
                3 => values[1].address.loans.push(usize::MAX),
                4 => state.vacant.push(source(
                    HoldRoot::Parameter(0),
                    vec![PlaceProjection::Field(0)],
                )),
                5 => args.truncate(1),
                6 => function = FunctionId(u32::MAX),
                7 => loans[0].parents.push(usize::MAX),
                _ => {}
            }
            let solution = Solution {
                values,
                outputs: vec![],
                entry: Frame::default(),
                loans,
                sites: vec![],
            };
            let mut summaries = vec![Summary::default(); module.functions.len()];
            summaries[1].apart.push((
                source(HoldRoot::Parameter(0), vec![]),
                source(HoldRoot::Parameter(1), vec![]),
            ));
            let mut old_b = Budget::new(&Limits::default());
            let mut new_b = Budget::new(&Limits::default());
            old_b.stored_slot_updates = true;
            new_b.stored_slot_updates = true;
            let seed = (
                source(HoldRoot::Parameter(20), vec![]),
                source(HoldRoot::Parameter(21), vec![]),
            );
            let mut old_requirements = vec![seed.clone()];
            let mut new_requirements = old_requirements.clone();
            let old = check_call_unfiltered(
                &module,
                caller,
                function,
                &args,
                &solution,
                &state,
                &active,
                &summaries,
                &mut old_requirements,
                None,
                None,
                &mut old_b,
            );
            let mut checks = Access::new(&active, &solution.loans, &mut new_b).unwrap();
            let new = check_call(
                &module,
                caller,
                function,
                &args,
                &solution,
                &state,
                &mut checks,
                &summaries,
                &mut new_requirements,
                None,
                None,
                &mut new_b,
            );
            assert_eq!(
                (new, new_requirements),
                (old, old_requirements),
                "mutable={mutable} mode={mode}"
            );
            if matches!(mode, 4..=6) {
                assert!(checks.reads.is_none());
            }
        }
    }
}
fn many_reads(steps: usize) -> Check<usize> {
    let active = (0..512).collect::<Vec<_>>();
    let loans = vec![Loan::default(); 512];
    let pointer = permission(HoldRoot::Parameter(0), &[0, 1, 2, 3, 4]);
    let mut b = budget(steps);
    let mut checks = Access::new(&active, &loans, &mut b)?;
    for _ in 0..512 {
        checks.check(&pointer, false, &mut Vec::new(), &mut b)?;
    }
    Ok(b.remaining)
}
#[test]
fn fresh_tables_real_growth_head_order_and_exact_read_work_are_prepaid() {
    let active = vec![1, usize::MAX, 0, 1, usize::MAX];
    let mut loans = vec![Loan::default(); 2];
    loans[1].exclusive = true;
    let pointer = permission(HoldRoot::Parameter(0), &[]);
    let mut b = Budget::new(&Limits::default());
    {
        let mut checks = Access::new(&active, &loans, &mut b).unwrap();
        assert_eq!(
            checks.read_candidates(&mut b).unwrap(),
            &[1, usize::MAX, 1, usize::MAX]
        );
        for delegated in [vec![], vec![usize::MAX]] {
            let mut old_r = Vec::new();
            let mut new_r = Vec::new();
            let old = access_tail(
                &pointer, false, &active, &loans, &delegated, &mut old_r, &mut b,
            );
            let new = access_tail(
                &pointer,
                false,
                checks.read_candidates(&mut b).unwrap(),
                &loans,
                &delegated,
                &mut new_r,
                &mut b,
            );
            assert_eq!((new, new_r), (old, old_r));
        }
    }
    loans[0].exclusive = true;
    loans[1].exclusive = false;
    let mut checks = Access::new(&active, &loans, &mut b).unwrap();
    assert_eq!(
        checks.read_candidates(&mut b).unwrap(),
        &[usize::MAX, 0, usize::MAX]
    );
    let mut invalid_parent = loans.clone();
    invalid_parent[0].parents.push(usize::MAX);
    for (p, write, table, static_utf8, message) in [
        (
            Permission::default(),
            false,
            &loans,
            false,
            "no independently grounded target permission",
        ),
        (
            permission(HoldRoot::Parameter(0), &[usize::MAX]),
            false,
            &loans,
            false,
            "loan parent outside actual arena",
        ),
        (
            permission(HoldRoot::Parameter(0), &[0]),
            false,
            &invalid_parent,
            false,
            "loan parent outside actual arena",
        ),
        (
            permission(HoldRoot::Static, &[usize::MAX]),
            true,
            &loans,
            true,
            "cannot authorize mutation",
        ),
    ] {
        let mut b = Budget::new(&Limits::default());
        b.static_utf8 = static_utf8;
        let mut checks = Access::new(&active, table, &mut b).unwrap();
        let error = checks
            .check(&p, write, &mut Vec::new(), &mut b)
            .unwrap_err();
        assert!(error.message.contains(message), "{error:?}");
        assert!(checks.reads.is_none());
    }
    let active = (0..512).collect::<Vec<_>>();
    let loans = vec![Loan::default(); 512];
    let limit = Limits::default().max_analysis_steps;
    let pointer = permission(HoldRoot::Parameter(0), &[0, 1, 2, 3, 4]);
    let mut old = budget(limit);
    for _ in 0..512 {
        access_unfiltered(&pointer, false, &active, &loans, &mut Vec::new(), &mut old).unwrap();
    }
    let remaining = many_reads(limit).unwrap();
    let n = limit - remaining;
    assert!(remaining > old.remaining);
    println!(
        "512 leaves x 512 actual shared loans old={} new={n}",
        limit - old.remaining
    );
    for steps in [n, n - 1, n] {
        let result = many_reads(steps);
        if steps == n {
            assert_eq!(result.unwrap(), 0);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
    let exclusive = vec![
        Loan {
            exclusive: true,
            ..Loan::default()
        };
        512
    ];
    let empty_roots = permission(HoldRoot::Parameter(0), &[]);
    for steps in [1, 1026, 1050] {
        let mut b = budget(steps);
        let mut checks = Access::new(&active, &exclusive, &mut b).unwrap();
        assert!(
            checks
                .check(&empty_roots, false, &mut Vec::new(), &mut b)
                .is_err()
        );
        assert!(
            checks.reads.is_none(),
            "failed classification/growth publishes no partial candidates"
        );
    }
    let mut old = budget(limit);
    let mut new = budget(limit);
    let mut checks = Access::new(&active, &loans, &mut new).unwrap();
    let after_constructor = new.remaining;
    access_unfiltered(&pointer, true, &active, &loans, &mut Vec::new(), &mut old).unwrap();
    checks
        .check(&pointer, true, &mut Vec::new(), &mut new)
        .unwrap();
    assert_eq!(after_constructor - new.remaining, limit - old.remaining);
    assert!(checks.reads.is_none());
}
