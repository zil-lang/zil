// Complete c6ff fresh solver and ordered check; no checkpoint helpers.
#[allow(clippy::too_many_arguments)]
fn old_run_with_replacements(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    summaries: &[Content],
    validate: bool,
    budget: &mut Budget,
    replacements: Option<&replacements::Context<'_>>,
) -> Check<Solved> {
    let mut locals = Vec::new();
    budget.spend(
        f.locals
            .len()
            .saturating_add(f.blocks.len().saturating_mul(3)),
    )?;
    for local in &f.locals {
        locals.push(Arc::new(content(
            &local.ty,
            None,
            &mut Vec::new(),
            true,
            budget,
        )?));
    }
    let values = values::Values::build(f.values.len(), budget, |index, budget| {
        Ok(Arc::new(content(
            &f.values[index],
            None,
            &mut Vec::new(),
            true,
            budget,
        )?))
    })?;
    let mut captures = Vec::new();
    if let Some(environment) = f.environment
        && budget.direct_sum_capture_closures.contains(&environment.0)
    {
        let definition = &module.closures[environment.0 as usize];
        budget.spend(definition.captures.len())?;
        let mut direct_sum = false;
        for ty in &definition.captures {
            if budget.inline_store_root(ty)? {
                direct_sum |= dynamic_store::inline_sum_capture(ty, budget)?;
            }
        }
        if direct_sum {
            budget.spend(definition.captures.len())?;
            captures = vec![None; definition.captures.len()];
            for (index, ty) in definition.captures.iter().enumerate() {
                if budget.inline_store_root(ty)? && dynamic_store::inline_sum_capture(ty, budget)? {
                    let mut actual = content(ty, None, &mut Vec::new(), false, budget)?;
                    actual.mark_captured(index as u32, &mut Vec::new(), budget)?;
                    captures[index] = Some(Arc::new(actual));
                }
            }
        }
    }
    let mut state = State {
        initialized: initialization::Initialized::new(f.locals.len(), 1, budget)?,
        locals,
        values,
        captures,
        facts: BTreeSet::new(),
        observations: BTreeSet::new(),
        partial: None,
        dirty_tags: None,
    };
    for (index, local) in f.parameters.iter().enumerate() {
        state.initialized.set(local.0 as usize, 2, budget)?;
        state.locals[local.0 as usize] = Arc::new(content(
            &f.locals[local.0 as usize].ty,
            Some(index as u32),
            &mut Vec::new(),
            false,
            budget,
        )?);
    }
    let mut partial = false;
    let mut address_tag = false;
    let mut address_replacement = false;
    let mut definitions = vec![None; f.values.len()];
    let mut observers = vec![None; f.values.len()];
    for (b, block) in f.blocks.iter().enumerate() {
        for (i, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            address_tag |= matches!(instruction.kind, InstructionKind::PlaceTagAddress { .. });
            address_replacement |= matches!(
                instruction.kind,
                InstructionKind::ReservePlaceAddress { .. }
            );
            partial |= matches!(
                instruction.kind,
                InstructionKind::BorrowPlaceLocal { .. }
                    | InstructionKind::PlaceTagLocal { .. }
                    | InstructionKind::TakeTupleField { .. }
                    | InstructionKind::TakeVariantField { .. }
            );
            if let Some(v) = instruction.result {
                definitions[v.0 as usize] = Some((b, i));
                if let InstructionKind::VariantTagLocal { local, path } = &instruction.kind {
                    budget.spend(path.len())?;
                    observers[v.0 as usize] =
                        Some((*local, path.iter().map(|p| (p.variant, p.index)).collect()));
                }
            }
        }
    }
    let address_replacement_tags = address_tag && address_replacement;
    partial |= address_replacement_tags;
    if partial {
        state.partial = Some(partial_places::State::default());
    }
    let tagged_locals = if indexed_invalidation(budget) && partial {
        // Scan and index slots are prepaid before allocation. Each queued
        // child is paid before growth, including nested owning Products.
        budget.spend(f.locals.len().saturating_mul(2).saturating_add(1))?;
        let mut tagged = Vec::with_capacity(f.locals.len());
        for (local, declaration) in f.locals.iter().enumerate() {
            budget.spend(1)?;
            let mut pending = vec![&declaration.ty];
            while let Some(ty) = pending.pop() {
                budget.spend(1)?;
                match ty {
                    Type::Enum { .. } | Type::ErrorUnion { .. } => {
                        tagged.push(local);
                        break;
                    }
                    Type::BorrowedStorage { .. }
                        if budget.store_option_results || budget.stored_enum_addresses =>
                    {
                        let actual = shared_enum_physical(ty, budget)?;
                        if !std::ptr::eq(actual, ty) {
                            pending.push(actual);
                        }
                    }
                    Type::Tuple(fields) => {
                        budget.spend(fields.len())?;
                        pending.extend(fields);
                    }
                    Type::Package { fields, .. } => {
                        budget.spend(fields.len())?;
                        pending.extend(fields.iter().map(|(_, field)| field));
                    }
                    // Exactly the terminal forms in this kernel's Content
                    // constructor: Copy or Leaf, with no tag/route state.
                    _ => {}
                }
            }
        }
        Some(tagged)
    } else {
        None
    };
    let flow = Flow {
        f,
        module,
        definitions,
        observers,
        summaries,
        partial,
        replacements,
        address_replacement_tags,
        owned_creation: false,
        tagged_locals,
        tag_cache: (partial && indexed_invalidation(budget))
            .then(|| std::cell::RefCell::new(tag_cache::Cache::new(f))),
        place_meta: if partial {
            Some(partial_places::Meta::new(f, budget)?)
        } else {
            None
        },
    };
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(state);
    let mut pending = resources::schedule::Worklist::new(
        graph,
        f.entry.0 as usize,
        budget.pod_stores || (budget.store_option_results && budget.ordinary_error_places),
        budget,
    )?;
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    let mut excluded_edges = BTreeSet::new();
    while let Some(block) = pending.pop(budget)? {
        queued[block] = false;
        let mut output = inputs[block].as_ref().unwrap().duplicate(budget)?;
        flow.block(block, &mut output, false, false, budget)?;
        for &next in &graph.successors[block] {
            let mut edge = output.duplicate(budget)?;
            if !flow.edge(&mut edge, block, next, budget)? {
                budget.spend(excluded_edges.len().saturating_add(1))?;
                excluded_edges.insert((block, next));
                continue;
            }
            if !excluded_edges.is_empty() {
                budget.spend(excluded_edges.len().saturating_add(1))?;
                // A later fixed-point input can widen a singleton tag. The
                // final proof must not retain an earlier provisional exclusion.
                excluded_edges.remove(&(block, next));
            }
            let changed = if let Some(input) = &mut inputs[next] {
                input.merge(&edge, budget)?
            } else {
                inputs[next] = Some(edge);
                true
            };
            if changed && !queued[next] {
                queued[next] = true;
                pending.push(next, budget)?;
            }
        }
    }
    #[cfg(test)]
    TEST_SUM_INPUTS.with(|capture| {
        let mut capture = capture.borrow_mut();
        if capture.is_some() {
            *capture = Some(inputs.clone());
        }
    });
    let mut result = content(&f.return_type, None, &mut Vec::new(), true, budget)?;
    let mut rows = vec![Vec::new(); f.blocks.len()];
    for (block, input) in inputs.into_iter().enumerate() {
        if let Some(mut state) = input {
            rows[block] = flow.block(block, &mut state, validate, validate, budget)?;
            if validate
                && matches!(f.blocks[block].terminator, Terminator::Return(_))
                && f.environment
                    .is_some_and(|id| module.closures[id.0 as usize].kind != CallableKind::FnOnce)
            {
                budget.spend(state.captures.len())?;
                for capture in state.captures.iter().flatten() {
                    if !capture.complete(budget)? {
                        return Err(bad(
                            Some(f.span),
                            "Fn/FnMut return leaves an unfilled direct Sum capture",
                        ));
                    }
                }
            }
            if let Terminator::Return(Some(value)) = f.blocks[block].terminator {
                result.merge(state.values.get(value.0 as usize, budget)?, budget)?;
            }
        }
    }
    Ok((result, rows, excluded_edges))
}

fn old_check(module: &TypedModule, graphs: &[FunctionGraph], b: &mut Budget) -> Check<Proofs> {
    #[cfg(test)]
    if trace::full() {
        return scheduler_tests::full_scan(module, graphs, b);
    }
    let actual = actual::Program::reconstruct(module, graphs, b)?;
    b.spend(module.functions.len().saturating_mul(3))?;
    let mut places = Vec::with_capacity(module.functions.len());
    let mut result = Vec::with_capacity(module.functions.len());
    let mut effects = vec![Rows::new(); module.functions.len()];
    let mut whole_must_rows = BTreeSet::new();
    let store_calls = RefCell::new(BTreeSet::new());
    for (f, graph) in module.functions.iter().zip(graphs) {
        let inventory = Inventory::reconstruct(module, f, b)?;
        if whole_must(f, &inventory, b)? {
            b.spend(whole_must_rows.len().saturating_add(1))?;
            whole_must_rows.insert(f.id.0 as usize);
        }
        if f.foreign.is_none() {
            inventory.activation_entries(f, graph, b)?;
        }
        places.push(inventory);
        result.push(content(&f.return_type, None, &mut Vec::new(), true, b)?);
    }
    let mut ordered = ordered::Ordered::new(module.functions.len(), b)?;
    loop {
        for (index, (f, graph)) in module.functions.iter().zip(graphs).enumerate() {
            b.spend(1)?;
            if !ordered.run(index, b)? || f.foreign.is_some() {
                continue;
            }
            b.spend(1)?;
            #[cfg(test)]
            trace::solve(index);
            let context = Context {
                actual: &actual,
                places: &places,
                summaries: &effects,
                whole_must: &whole_must_rows,
                written: RefCell::new(Rows::new()),
                store_calls: &store_calls,
                observed: Some(RefCell::new(BTreeSet::new())),
            };
            let (found, _, _) =
                old_run_with_replacements(module, f, graph, &result, false, b, Some(&context))?;
            let rows = context.written.into_inner();
            ordered.observe(
                index,
                context
                    .observed
                    .expect("inference observes its actual reads")
                    .into_inner(),
                b,
            )?;
            let mut changed = result[f.id.0 as usize].merge(&found, b)?;
            b.spend(rows.len())?;
            for (target, payload) in rows {
                changed |= merge_row(&mut effects[f.id.0 as usize], &target, &payload, b)?;
            }
            if changed {
                ordered.changed(index, b)?;
            }
        }
        #[cfg(test)]
        trace::round(&result, &effects, &store_calls, b);
        if !ordered.advance(b)? {
            break;
        }
    }
    let mut rows = Vec::with_capacity(module.functions.len());
    let mut excluded_edges = BTreeMap::new();
    for (f, graph) in module.functions.iter().zip(graphs) {
        if f.foreign.is_some() {
            rows.push(Vec::new());
            continue;
        }
        let context = Context {
            actual: &actual,
            places: &places,
            summaries: &effects,
            whole_must: &whole_must_rows,
            written: RefCell::new(Rows::new()),
            store_calls: &store_calls,
            observed: None,
        };
        let (_, bridges, excluded) =
            old_run_with_replacements(module, f, graph, &result, true, b, Some(&context))?;
        rows.push(bridges);
        if !excluded.is_empty() {
            b.spend(excluded_edges.len().saturating_add(1))?;
            excluded_edges.insert(f.id.0 as usize, excluded);
        }
    }
    #[cfg(test)]
    trace::final_state(&result, &effects, &store_calls, b);
    b.spend(effects.len())?;
    let mut replacement_writes = Vec::with_capacity(effects.len());
    let needed = store_calls.into_inner();
    let mut replacement_targets = BTreeMap::new();
    let mut retirement_targets = BTreeMap::new();
    for (entry, effect) in effects.into_iter().enumerate() {
        let mut writes = BTreeSet::new();
        let mut targets = Vec::new();
        let mut retired = Vec::new();
        b.spend(needed.len())?;
        let transfer = needed.contains(&entry);
        b.spend(effect.len())?;
        for (target, _) in effect {
            let root = match target.base {
                actual::Base::Input(p) | actual::Base::InputCapture(p, _) => {
                    Some(resources::Root::Parameter(p))
                }
                actual::Base::Environment(c) => Some(resources::Root::BorrowedCapture(c)),
                _ => None,
            };
            if let Some(root) = root {
                lookup(b, writes.len())?;
                writes.insert(root);
                if transfer {
                    b.spend(1)?;
                    targets.push(target);
                } else {
                    // The exact input/capture ordinal and full route survive
                    // the boundary. Ordinary write roots lose this identity.
                    b.spend(target.steps.len().saturating_add(1))?;
                    retired.push(target);
                }
            }
        }
        replacement_writes.push(writes);
        if !targets.is_empty() {
            b.spend(replacement_targets.len().saturating_add(1))?;
            replacement_targets.insert(entry, targets);
        }
        if !retired.is_empty() {
            b.spend(retirement_targets.len().saturating_add(1))?;
            retirement_targets.insert(entry, retired);
        }
    }
    Ok(Proofs {
        rows,
        excluded_edges,
        replacement_writes,
        replacement_targets,
        retirement_targets,
    })
}
