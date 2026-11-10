// Complete frozen 241 constructors and resource run; include inside a test module
// with the normal resources imports. State, Transfer and unchanged Plan extension
// remain shared; constructor adapter names below are the only body changes.
impl Slots {
    fn old_layout_new(
        module: &TypedModule,
        f: &TypedFunction,
        views: &packages::CaptureViews,
        budget: &mut Budget,
    ) -> Check<Self> {
        if budget.sum_function(f.id) {
            for ty in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
                charge_payload(ty, budget)?;
            }
        }
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_mul(2),
        )?;
        budget.spend(module.closures.len().saturating_mul(2))?;
        let mut owned_closures = Vec::with_capacity(module.closures.len());
        let mut borrowed_closures = Vec::with_capacity(module.closures.len());
        for closure in &module.closures {
            budget.spend(closure.captures.len().saturating_mul(2))?;
            let mut inline_sum_owner = false;
            for ty in &closure.captures {
                if budget.inline_store_root(ty)? {
                    inline_sum_owner |= dynamic_store::inline_sum_capture(ty, budget)?;
                }
            }
            owned_closures.push(
                closure.kind == CallableKind::FnOnce
                    || inline_sum_owner
                    || closure.captures.iter().any(|ty| {
                        matches!(
                            ty,
                            Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
                        )
                    }),
            );
            borrowed_closures.push(
                closure
                    .captures
                    .iter()
                    .any(|ty| matches!(ty, Type::Desc(_))),
            );
        }
        let mut debt_count = 0usize;
        let mut desc_count = 0usize;
        let sum_extension = budget.sums;
        let head_descriptions = budget.head_descriptions;
        let inline_store_addresses = budget.inline_store_addresses;
        // Only actual Sum addresses containing an inline Store need this
        // additional origin row. Leave old-only functions' slot inventory and
        // traversal unchanged.
        let (local_sum_address, value_sum_address) = if inline_store_addresses {
            budget.spend(f.locals.len().saturating_add(f.values.len()))?;
            let mut local = Vec::with_capacity(f.locals.len());
            let mut value = Vec::with_capacity(f.values.len());
            for declaration in &f.locals {
                local.push(match &declaration.ty {
                    Type::Address { inner, .. } if budget.inline_store_root(&declaration.ty)? => {
                        dynamic_store::inline_sum_capture(inner, budget)?
                    }
                    _ => false,
                });
            }
            for ty in &f.values {
                value.push(match ty {
                    Type::Address { inner, .. } if budget.inline_store_root(ty)? => {
                        dynamic_store::inline_sum_capture(inner, budget)?
                    }
                    _ => false,
                });
            }
            (local, value)
        } else {
            (Vec::new(), Vec::new())
        };
        let mut slot = |ty: &Type, managed: bool, sum_address: bool| {
            let count = if managed {
                &mut debt_count
            } else {
                &mut desc_count
            };
            if (managed && package_storage(ty).is_some())
                || matches!(
                    (ty, managed),
                    (Type::Managed(_) | Type::MutexGuard { .. } | Type::OwnedDyn { .. }, true)
                        | (Type::Desc(_) | Type::MutexGuard { .. }, false)
                )
                || (!managed && matches!(ty, Type::Address { inner, .. }
                    if matches!(inner.as_ref(), Type::MutexGuard { .. })))
                || (!managed && matches!(ty, Type::Address { inner, .. }
                    if matches!(inner.as_ref(), Type::Dyn { .. })))
                || (managed
                && (errors::owned(ty, sum_extension) || (sum_extension && sums::owned(ty))))
                || (managed
                    && matches!(ty, Type::Array { element, length }
                    if *length > 0 && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))))
                || matches!(ty, Type::Closure(id) if if managed {
                    owned_closures[id.0 as usize]
                } else { borrowed_closures[id.0 as usize] })
                || (!managed
                    && (head_descriptions || matches!(ty, Type::Address { inner, .. }
                        if matches!(inner.as_ref(), Type::BorrowedStorage { storage, .. }
                            if matches!(storage.as_ref(), Type::Package { .. }))))
                    && (head_address(ty)
                        || ty.is_package_address()
                        || matches!(ty, Type::Address { inner, .. }
                            if package_storage(inner).is_some())))
                // A projected Store head can be reached through an ordinary
                // Sum address. Keep its actual address source through local
                // copies until BorrowPlaceAddress selects the checked field.
                || (!managed && sum_address)
                || (!managed
                    && matches!(ty,
                    Type::Address { inner, .. }
                    if matches!(inner.as_ref(), Type::Closure(id) if borrowed_closures[id.0 as usize])))
            {
                let id = *count;
                *count += 1;
                Some(id)
            } else {
                None
            }
        };
        // Collect only actual Package declarations during the existing local
        // slot pass. A function without a Package keeps the former traversal
        // and allocation cost; it does not build or scan an empty field table.
        let mut package_declarations = Vec::new();
        let mut array_declarations = Vec::new();
        let mut managed_local = Vec::with_capacity(f.locals.len());
        for (local, declaration) in f.locals.iter().enumerate() {
            managed_local.push(slot(&declaration.ty, true, false));
            if let Some(fields) = package_storage(&declaration.ty) {
                budget.spend(2)?;
                package_declarations.push((local, fields));
            }
            if let Type::Array { element, length } = &declaration.ty
                && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
            {
                let width = match element.as_ref() {
                    Type::Package { fields, .. } => fields.len().saturating_add(1),
                    _ => 1,
                };
                budget.spend((*length as usize).saturating_mul(width))?;
                array_declarations.push((local, *length));
            }
        }
        let local_count = managed_local.iter().flatten().count();
        let managed_value = f.values.iter().map(|t| slot(t, true, false)).collect();
        let desc_local = f
            .locals
            .iter()
            .enumerate()
            .map(|(index, local)| {
                slot(
                    &local.ty,
                    false,
                    local_sum_address.get(index).copied().unwrap_or(false),
                )
            })
            .collect();
        let desc_value = f
            .values
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                slot(
                    ty,
                    false,
                    value_sum_address.get(index).copied().unwrap_or(false),
                )
            })
            .collect();
        budget.spend(f.values.len())?;
        let mut package_widths = Vec::new();
        for (value, ty) in f.values.iter().enumerate() {
            if let Some(fields) = package_storage(ty) {
                if package_widths.is_empty() {
                    budget.spend(f.values.len())?;
                    package_widths = vec![0; f.values.len()];
                }
                // Field byte allocation is charged in empty, before it occurs.
                budget.spend(1)?;
                package_widths[value] = fields.len();
            }
        }
        let mut package_fields = if package_declarations.is_empty() && array_declarations.is_empty()
        {
            Vec::new()
        } else {
            budget.spend(f.locals.len())?;
            vec![Vec::new(); f.locals.len()]
        };
        for (local, fields) in package_declarations {
            budget.spend(fields.len())?;
            package_fields[local] = fields
                .iter()
                .map(|(_, ty)| {
                    if packages::owned_field(ty, sum_extension) {
                        let next = debt_count;
                        debt_count += 1;
                        Some(next)
                    } else {
                        None
                    }
                })
                .collect();
        }
        for (local, length) in array_declarations {
            package_fields[local] = (0..length)
                .map(|_| {
                    let next = debt_count;
                    debt_count += 1;
                    Some(next)
                })
                .collect();
        }
        let capture_start = debt_count;
        let captures = f
            .environment
            .map_or(&[][..], |id| &module.closures[id.0 as usize].captures);
        budget.spend(captures.len())?;
        let capture_debt = captures
            .iter()
            .enumerate()
            .map(|(index, ty)| -> Check<Option<usize>> {
                if matches!(ty, Type::Managed(_) | Type::Closure(_))
                    || (budget.inline_store_root(ty)?
                        && dynamic_store::inline_sum_capture(ty, budget)?)
                {
                    let next = debt_count;
                    debt_count += 1;
                    Ok(Some(next))
                } else if matches!(ty, Type::Package { .. }) {
                    let local =
                        views.by_capture[index].expect("independently checked Package binding");
                    Ok(managed_local[local.0 as usize])
                } else {
                    Ok(None)
                }
            })
            .collect::<Check<Vec<_>>>()?;
        let mut retained_debt = Vec::new();
        if !views.by_local.is_empty() {
            budget.spend(
                capture_start
                    .saturating_add(views.by_local.len())
                    .saturating_add(views.by_capture.len().saturating_mul(2)),
            )?;
            retained_debt = vec![false; capture_start];
            for local in views.by_capture.iter().flatten() {
                retained_debt[managed_local[local.0 as usize].expect("Package shell debt")] = true;
                budget.spend(package_fields[local.0 as usize].len())?;
                for &field in package_fields[local.0 as usize].iter().flatten() {
                    retained_debt[field] = true;
                }
            }
        }
        let product = product::Plan::new(f, budget)?;
        let package_fresh_plan = field_fresh::Plan::new(package_widths, budget)?;
        Ok(Self {
            managed_local,
            managed_value,
            package_fields,
            package_fresh_plan,
            product,
            desc_local,
            desc_value,
            capture_debt,
            package_view: views.by_local.clone(),
            package_capture_local: views.by_capture.clone(),
            retained_debt,
            capture_start,
            local_count,
            debt_count,
            desc_count,
            retirement: false,
        })
    }
}

fn old_run(
    module: &TypedModule,
    binding: &mut desc_liveness::Binding<'_>,
    effects: &[BTreeSet<Root>],
    validate: bool,
    sum_rows: Option<&[Vec<sums::Bridge>]>,
    budget: &mut Budget,
    replacements: Option<&replacements::Plan>,
) -> Check<BTreeSet<Root>> {
    let f = binding.function();
    let graph = binding.graph();
    if f.foreign.is_some() {
        return Ok(BTreeSet::new());
    }
    let mut slots = Slots::old_layout_new(module, f, &graph.package_views, budget)?;
    if let Some(plan) = replacements {
        plan.extend_slots(f, &mut slots, budget)?;
    }
    if slots.debt_count == 0 && slots.desc_count == 0 && !slots.retirement {
        return Ok(BTreeSet::new());
    }
    let mut written = if let Some(plan) = replacements {
        plan.writes(f.id, budget)?
    } else {
        BTreeSet::new()
    };
    let after = binding.after(&slots, budget)?;
    let mut initial = slots.empty(budget)?;
    // Charge actual initialization work, including the old Managed-only scan.
    budget.spend(slots.capture_debt.len())?;
    for (index, debt) in slots.capture_debt.iter().enumerate() {
        if let Some(debt) = debt {
            initial.debt[*debt] = LIVE;
            initial.fresh[*debt] = INPUT;
        }
        if let Some(local) = slots.package_capture_local.get(index).copied().flatten() {
            budget.spend(slots.fields(local).len())?;
            for &field in slots.fields(local).iter().flatten() {
                initial.debt[field] = LIVE;
                initial.fresh[field] = INPUT;
            }
        }
    }
    for (p, local) in f.parameters.iter().enumerate() {
        if let Some(index) = slots.managed_local[local.0 as usize] {
            for old in 0..slots.local_count {
                budget.spend(1)?;
                if !slots.retained(old) && initial.debt[old] & LIVE != 0 {
                    initial.newer[old].insert(index);
                }
            }
            initial.debt[index] = LIVE;
            initial.fresh[index] = INPUT;
            let fields = slots.fields(*local);
            budget.spend(fields.len())?;
            for &field in fields.iter().flatten() {
                initial.debt[field] = LIVE;
                initial.fresh[field] = INPUT;
            }
        }
        if let Some(index) = slots.desc_local[local.0 as usize] {
            let root = Root::Parameter(p as u32);
            initial.desc[index].insert(if matches!(&f.locals[local.0 as usize].ty, Type::Desc(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. })) { root.store_parent() } else { root });
        }
    }
    if let Some(plan) = replacements {
        plan.seed_sources(module, f, &slots, &mut initial, budget)?;
    }
    if validate {
        task_frames::start(f, &slots, &initial, budget)?;
    }
    budget.spend(f.blocks.len().saturating_mul(2))?;
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(initial);
    let mut pending = schedule::Worklist::new(
        graph,
        f.entry.0 as usize,
        budget.pod_stores || (budget.store_option_results && budget.ordinary_error_places),
        budget,
    )?;
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    let transfer = Transfer {
        module,
        f,
        slots: &slots,
        after: after.history(),
        effects,
        sum_rows,
        replacements,
    };
    while let Some(b) = pending.pop(budget)? {
        queued[b] = false;
        let mut output = clone_state(inputs[b].as_ref().expect("queued resource state"), budget)?;
        transfer.block(b, &mut output, false, &mut written, budget)?;
        for &next in &graph.successors[b] {
            budget.spend(1)?;
            let changed = if let Some(input) = &mut inputs[next] {
                merge(input, &output, budget)?
            } else {
                inputs[next] = Some(clone_state(&output, budget)?);
                true
            };
            if changed && !queued[next] {
                queued[next] = true;
                pending.push(next, budget)?;
            }
        }
    }
    #[cfg(test)]
    TEST_RESOURCE_INPUTS.with(|capture| {
        let mut capture = capture.borrow_mut();
        if capture.is_some() {
            *capture = Some(inputs.clone());
        }
    });
    for (b, input) in inputs.into_iter().enumerate() {
        if let Some(mut state) = input {
            transfer.block(b, &mut state, validate, &mut written, budget)?;
        }
    }
    Ok(written)
}

fn old_bindings<'a>(
    module: &'a TypedModule,
    graphs: &'a [FunctionGraph],
    refined: Option<&'a BTreeMap<usize, FunctionGraph>>,
    b: &mut Budget,
) -> Check<Vec<desc_liveness::Binding<'a>>> {
    let words =
        std::mem::size_of::<desc_liveness::Binding<'_>>().div_ceil(std::mem::size_of::<usize>());
    let outer = std::mem::size_of::<Vec<desc_liveness::Binding<'_>>>()
        .div_ceil(std::mem::size_of::<usize>());
    // Actual binding slots plus their eventual entry dispatch, and the full
    // outer Vec record with allocation/release. No Arc owners are introduced.
    b.spend(
        module
            .functions
            .len()
            .saturating_mul(words.saturating_add(1))
            .saturating_add(outer)
            .saturating_add(2),
    )?;
    let mut bindings = Vec::with_capacity(module.functions.len());
    for (index, f) in module.functions.iter().enumerate() {
        b.spend(3)?; // actual function/graph ordinal reads and binding write
        let graph = if let Some(rows) = refined {
            b.spend(rows.len().saturating_add(1))?;
            rows.get(&index).unwrap_or(&graphs[index])
        } else {
            &graphs[index]
        };
        bindings.push(desc_liveness::Binding::new(f, graph));
    }
    Ok(bindings)
}

fn old_infer(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
    replacements: Option<&replacements::Plan>,
    bindings: &mut [desc_liveness::Binding<'_>],
) -> Check<Vec<BTreeSet<Root>>> {
    budget.spend(module.functions.len())?;
    let mut effects = vec![BTreeSet::new(); module.functions.len()];
    let dependents = effect_dependents(module, graphs, budget)?;
    // Independent verifier scheduling: finish a batch before processing its
    // notifications. Every function runs initially; growing rows wake consumers.
    let mut dirty: Vec<usize> = (0..module.functions.len()).rev().collect();
    let mut next = Vec::new();
    let mut scheduled = vec![true; module.functions.len()];
    while !dirty.is_empty() {
        while let Some(index) = dirty.pop() {
            budget.spend(1)?;
            scheduled[index] = false;
            #[cfg(test)]
            TEST_EFFECT_SOLVES.with(|value| value.set(value.get() + 1));
            let found = old_run(
                module,
                &mut bindings[index],
                &effects,
                false,
                None,
                budget,
                replacements,
            )?;
            budget.spend(found.len().saturating_add(effects[index].len()))?;
            let before = effects[index].len();
            effects[index].extend(found);
            if effects[index].len() != before {
                budget.spend(dependents[index].len())?;
                for &user in &dependents[index] {
                    if !scheduled[user] {
                        budget.spend(1)?;
                        scheduled[user] = true;
                        next.push(user);
                    }
                }
            }
        }
        std::mem::swap(&mut dirty, &mut next);
    }
    Ok(effects)
}
