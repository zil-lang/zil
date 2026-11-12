use super::*;
#[allow(dead_code)]
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
    ));
}
fn result_row_by_scan(
    summary: &mut Summary,
    path: &[PlaceProjection],
    sources: &[Source],
    b: &mut Budget,
) -> Check<()> {
    b.spend(amount(
        summary.result.len().saturating_add(1),
        path.len().saturating_add(1),
    )?)?;
    if let Some((_, row)) = summary.result.iter_mut().find(|(p, _)| p == path) {
        add_sources(row, sources, b)?;
    } else {
        b.spend(path.len())?;
        source_work(sources, b)?;
        summary.result.push((path.to_vec(), sources.to_vec()));
    }
    Ok(())
}
fn summarize_by_scan(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    solution: &Solution,
    inactive: Option<&[ValueId]>,
    b: &mut Budget,
) -> Check<Summary> {
    let mut summary = Summary {
        complete: true,
        ..Summary::default()
    };
    for (bi, block) in f.blocks.iter().enumerate() {
        if g.reachable[bi] && matches!(block.terminator, Terminator::Return(_)) {
            b.spend(solution.outputs[bi].vacant.len())?;
            if solution.outputs[bi]
                .vacant
                .iter()
                .any(|slot| matches!(slot.base, HoldRoot::Parameter(_) | HoldRoot::Capture(_)))
            {
                summary.complete = false;
            }
        }
    }
    for (id, (ty, p)) in f.values.iter().zip(&solution.values).enumerate() {
        b.spend(1)?;
        if p.enum_variants.present() {
            p.enum_variants.work(b)?;
            summary
                .enum_values
                .push((ValueId(id as u32), p.enum_variants.clone()));
        }
        if p.option_variants.mask() == 1 {
            b.spend(2)?;
            summary.none_values.push(ValueId(id as u32));
        }
        if let Some(dead) = inactive {
            b.spend(dead.len().saturating_add(1))?;
            if dead.contains(&ValueId(id as u32)) {
                continue;
            }
        }
        if p.creation_complete == Some(false) {
            summary.complete = false;
        }
        if matches!(ty, Type::Address { .. }) && p.address.sources.is_empty() {
            summary.complete = false;
        }
        if b.stored_enum_addresses
            && matches!(ty, Type::ErrorUnion { .. })
            && enum_address::ordinary(module, ty, b)?
        {
            if !p.enum_variants.grounded(ty, b)? {
                summary.complete = false;
            }
            for (path, _) in enum_address::slots(ty, b)? {
                if p.enum_variants.slot_active(&path)
                    && leaf(p, &path, b)?.is_none_or(|p| p.sources.is_empty())
                {
                    summary.complete = false;
                }
            }
        }
        if shared_array_shape(ty, b)? {
            let Type::Array { length, .. } = ty else {
                unreachable!()
            };
            b.spend(*length as usize)?;
            for index in 0..*length {
                if leaf(p, &[PlaceProjection::Field(index)], b)?
                    .is_none_or(|p| p.sources.is_empty())
                {
                    summary.complete = false;
                }
            }
        }
        if let Some((_, shape)) = qualified(ty) {
            if enum_address::ordinary(module, ty, b)? {
                if !p.enum_variants.grounded(ty, b)? {
                    summary.complete = false;
                }
            } else if option_address::candidate(ty) {
                b.spend(96)?;
                if !option_address::signature(ty) {
                    return Err(err("Option activity has no exact shape"));
                }
                let mask = p.option_variants.mask();
                if mask == 0 {
                    summary.complete = false;
                }
                if mask == 1 {
                    continue;
                }
            }
            for row in &shape.slots {
                if p.enum_variants.present() && !p.enum_variants.slot_active(&row.path) {
                    continue;
                }
                if leaf(p, &row.path, b)?.is_none_or(|p| p.sources.is_empty()) {
                    summary.complete = false;
                }
            }
        }
        if fixed_environment(module, ty, b) {
            let carrier = if let Type::Address { inner, .. } = ty {
                inner.as_ref()
            } else {
                ty
            };
            let Type::Closure(id) = carrier else {
                return Err(err("actual environment type is absent"));
            };
            let d = &module.closures[id.0 as usize];
            b.spend(d.captures.len())?;
            for (index, ty) in d.captures.iter().enumerate() {
                let prefix = vec![PlaceProjection::Field(index as u32)];
                if matches!(ty, Type::Address { .. })
                    && leaf(p, &prefix, b)?.is_none_or(|p| p.sources.is_empty())
                {
                    summary.complete = false;
                }
                if let Some((_, shape)) = addressed(ty) {
                    for row in &shape.slots {
                        b.spend(row.path.len().saturating_add(1))?;
                        let mut path = prefix.clone();
                        path.extend(&row.path);
                        if leaf(p, &path, b)?.is_none_or(|p| p.sources.is_empty()) {
                            summary.complete = false;
                        }
                    }
                }
            }
        }
    }
    for (bi, (block, reachable)) in f.blocks.iter().zip(&g.reachable).enumerate() {
        b.spend(1)?;
        if b.stored_slot_updates && *reachable && matches!(block.terminator, Terminator::Return(_))
        {
            for (position, payload) in solution.outputs[bi].external.iter().enumerate() {
                let local = *f
                    .parameters
                    .get(position)
                    .ok_or_else(|| err("external output exceeds actual parameter table"))?;
                if !matches!(&f.locals[local.0 as usize].ty,
                    Type::Address { mutable: true, inner } if qualified(inner).is_some())
                {
                    continue;
                }
                for (path, permission) in &payload.leaves {
                    b.spend(
                        path.len()
                            .saturating_add(permission.sources.len())
                            .saturating_add(1),
                    )?;
                    for source in &permission.sources {
                        if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Static) {
                            return Err(err("external slot update retains a callee-local target"));
                        }
                        if let HoldRoot::Parameter(origin) = source.base
                            && relation(f, StorageBase::Parameter(position as u32), path, b)?
                                != relation(f, StorageBase::Parameter(origin), &source.leaf, b)?
                        {
                            return Err(err(
                                "external slot update exceeds its declared lifetime relation",
                            ));
                        }
                    }
                    let key = (position as u32, path.clone());
                    if let Some((_, _, row)) = summary
                        .external_after
                        .iter_mut()
                        .find(|(p, existing, _)| *p == key.0 && *existing == key.1)
                    {
                        add_sources(row, &permission.sources, b)?;
                    } else {
                        b.spend(
                            summary
                                .external_after
                                .len()
                                .saturating_mul(path.len().saturating_add(1)),
                        )?;
                        summary
                            .external_after
                            .push((key.0, key.1, permission.sources.clone()));
                    }
                }
            }
        }
        if *reachable && let Terminator::Return(Some(id)) = block.terminator {
            let payload = &solution.values[id.0 as usize];
            summary
                .returned_enum_variants
                .join(&payload.enum_variants, b)?;
            summary
                .returned_variants
                .join(&payload.option_variants, b)?;
            if matches!(f.return_type, Type::Address { .. }) {
                result_row_by_scan(&mut summary, &[], &payload.address.sources, b)?;
            }
            for (path, p) in &payload.leaves {
                result_row_by_scan(&mut summary, path, &p.sources, b)?;
            }
            task_address::summarize(f, payload, &mut summary, b)?;
        }
    }
    for loan in &solution.loans {
        let _ = ancestry(&loan.parents, &solution.loans, b)?;
    }
    for (id, loan) in solution.loans.iter().enumerate() {
        let parents = ancestry(&loan.parents, &solution.loans, b)?;
        b.spend(parents.len())?;
        if parents.contains(&id) {
            return Err(err("cyclic actual loan ancestry"));
        }
    }
    Ok(summary)
}

fn origin(id: u32) -> Source {
    Source {
        retained_port: (id.is_multiple_of(2)).then_some(PortOrigin::Task { carrier: Vec::new(), port: id }),
        base: HoldRoot::Parameter(id),
        leaf: vec![PlaceProjection::Field(id)],
        inline: vec![PlaceProjection::ErrorArm(id.is_multiple_of(2))],
        heap_parents: Vec::new(),
        heap_element: (id.is_multiple_of(3)).then(|| vec![PlaceProjection::Field(id + 1)]),
    }
}
fn fixture_for(payloads: Vec<Payload>) -> (TypedModule, TypedFunction, FunctionGraph, Solution) {
    let module = fixture::module();
    let mut f = module.functions[0].clone();
    f.storage_life = None;
    f.locals.clear();
    f.parameters.clear();
    f.return_type = fixture::address(Type::I32, false);
    f.values = vec![f.return_type.clone(); payloads.len()];
    let mut block = f.blocks[0].clone();
    block.instructions.clear();
    f.blocks = (0..payloads.len())
        .map(|id| {
            let mut block = block.clone();
            block.id = BlockId(id as u32);
            block.terminator = Terminator::Return(Some(ValueId(id as u32)));
            block
        })
        .collect();
    let graph = FunctionGraph {
        successors: vec![vec![]; payloads.len()],
        predecessors: vec![vec![]; payloads.len()],
        reachable: vec![true; payloads.len()],
        package_views: Default::default(),
        role_data_uses: vec![],
    };
    let solution = Solution {
        outputs: vec![Frame::default(); payloads.len()],
        values: payloads,
        entry: Frame::default(),
        loans: vec![],
        sites: vec![],
    };
    (module, f, graph, solution)
}
fn payload(rows: Vec<(Path, Vec<Source>)>, sources: Vec<Source>) -> Payload {
    Payload {
        address: Permission {
            sources,
            loans: vec![],
        },
        leaves: leaf_rows::Rows::from_vec(
            rows.into_iter()
                .map(|(path, sources)| {
                    (
                        path,
                        Permission {
                            sources,
                            loans: vec![],
                        },
                    )
                })
                .collect(),
            &mut Budget::new(&Limits::default()),
        )
        .unwrap(),
        ..Payload::default()
    }
}
fn compare(
    fixture: &(TypedModule, TypedFunction, FunctionGraph, Solution),
    task: bool,
) -> Check<Summary> {
    let (module, f, graph, solution) = fixture;
    let budget = || {
        let mut b = Budget::new(&Limits::default());
        b.task_borrowed_results = task;
        b
    };
    let old = summarize_by_scan(module, f, graph, solution, None, &mut budget());
    let new = summarize(module, f, graph, solution, None, &mut budget());
    assert_eq!(new, old, "complete Summary and original diagnostics");
    new
}
#[test]
fn indexed_result_rows_preserve_first_order_full_paths_and_source_normalization() {
    let mut paths = vec![
        vec![],
        vec![PlaceProjection::Field(0)],
        vec![PlaceProjection::Field(0), PlaceProjection::Field(0)],
        vec![PlaceProjection::ErrorArm(false)],
        vec![PlaceProjection::ErrorArm(true)],
    ];
    for variant in [0, 1] {
        for index in [0, 1] {
            paths.push(vec![PlaceProjection::Variant(zil_ir::VariantProjection {
                variant,
                index,
            })]);
        }
    }
    for ordinal in 0..48 {
        let mut path = vec![PlaceProjection::Field(7); 17];
        path.push(PlaceProjection::Field(ordinal));
        paths.push(path);
    }
    let first = payload(
        paths
            .iter()
            .rev()
            .map(|path| (path.clone(), vec![origin(5), origin(2), origin(5)]))
            .collect(),
        vec![origin(3), origin(1), origin(3)],
    );
    let one = fixture_for(vec![first.clone()]);
    let expected = compare(&one, false).unwrap();
    assert_eq!(expected.result[0].0, vec![]);
    // This first hit comes from an empty leaf path and sorts the address row;
    // every other newly copied row preserves its original unsorted duplicates.
    assert_eq!(expected.result[1].1, vec![origin(5), origin(2), origin(5)]);
    let second = payload(
        paths
            .iter()
            .map(|path| (path.clone(), Vec::new()))
            .collect(),
        vec![],
    );
    let third = payload(
        paths
            .iter()
            .map(|path| (path.clone(), vec![origin(4), origin(0), origin(4)]))
            .collect(),
        vec![],
    );
    let all = fixture_for(vec![first, second, third]);
    let summary = compare(&all, false).unwrap();
    assert_eq!(
        summary
            .result
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        expected
            .result
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>()
    );
    assert!(summary.result[1].1.windows(2).all(|p| p[0] <= p[1]));
    assert_eq!(
        summary.result[1]
            .1
            .iter()
            .filter(|s| **s == origin(5))
            .count(),
        2
    );
    compare(&all, true).unwrap();
    // A separate invocation with different discovery order creates a new index.
    compare(
        &fixture_for(vec![payload(
            paths
                .iter()
                .map(|path| (path.clone(), vec![origin(1)]))
                .collect(),
            vec![],
        )]),
        false,
    )
    .unwrap();
}
#[test]
fn indexed_results_keep_task_failures_and_final_actual_ancestry_checks() {
    let mut fixture = fixture_for(vec![payload(
        vec![(vec![PlaceProjection::Field(0)], vec![origin(0)])],
        vec![origin(1)],
    )]);
    fixture.3.values[0].task_ports.push((
        vec![],
        Permission {
            sources: vec![origin(1)],
            loans: vec![],
        },
    ));
    let error = compare(&fixture, true).unwrap_err();
    assert!(
        error
            .message
            .contains("returned Task lacks complete retained relation")
    );
    fixture.3.values[0].task_ports.clear();
    fixture.3.loans = vec![Loan {
        parents: vec![0],
        ..Loan::default()
    }];
    assert!(
        compare(&fixture, false)
            .unwrap_err()
            .message
            .contains("cyclic actual loan ancestry")
    );
    fixture.3.loans[0].parents = vec![usize::MAX];
    assert!(
        compare(&fixture, false)
            .unwrap_err()
            .message
            .contains("loan parent outside actual arena")
    );
    fixture.3.loans[0].parents.clear();
    compare(&fixture, false).unwrap();
}
#[test]
fn indexed_results_remove_actual_scans_with_exact_fresh_default_work() {
    let fixture = fixture_for(vec![payload(
        (0..512)
            .rev()
            .map(|id| (vec![PlaceProjection::Field(id)], vec![origin(1)]))
            .collect(),
        vec![],
    )]);
    let (module, f, graph, solution) = &fixture;
    let mut old_b = Budget::new(&Limits::default());
    let mut new_b = Budget::new(&Limits::default());
    let expected = summarize_by_scan(module, f, graph, solution, None, &mut old_b).unwrap();
    assert_eq!(
        summarize(module, f, graph, solution, None, &mut new_b).unwrap(),
        expected
    );
    assert!(
        new_b.remaining > old_b.remaining,
        "actual indexed path lookup replaces full previous-row scans, including reverse insertion shifts"
    );
    let n = Limits::default().max_analysis_steps - new_b.remaining;
    for steps in [n, n - 1, n] {
        let result = summarize(
            module,
            f,
            graph,
            solution,
            None,
            &mut Budget::new(&Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            }),
        );
        if steps == n {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert_eq!(
                result.unwrap_err().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
}
