// Complete frozen 54b03 run, liveness and history; recursive helpers stay on the old side.
fn old_reverse(slots: &Slots, live: &mut [bool], instruction: &Instruction) {
    if let Some(value) = instruction.result
        && let Some(index) = slots.desc_value[value.0 as usize]
    {
        live[index] = false;
    }
    match instruction.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => {
            if let Some(index) = slots.desc_local[local.0 as usize] {
                live[index] = false;
            }
        }
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. } => {
            if let Some(index) = slots.desc_local[local.0 as usize] {
                live[index] = true;
            }
        }
        _ => {}
    }
    for operand in instruction_operands(&instruction.kind) {
        if let Some(index) = slots.desc_value[operand.0 as usize] {
            live[index] = true;
        }
    }
}

fn old_liveness(
    f: &TypedFunction,
    graph: &FunctionGraph,
    slots: &Slots,
    budget: &mut Budget,
) -> Check<desc_liveness::History> {
    let n = f.blocks.len();
    budget.spend(
        n.checked_mul(slots.desc_count)
            .and_then(|v| v.checked_mul(2))
            .ok_or_else(|| limit("resource liveness overflow"))?,
    )?;
    let mut input = vec![vec![false; slots.desc_count]; n];
    let mut output = input.clone();
    let mut pending: VecDeque<_> = (0..n).filter(|&b| graph.reachable[b]).collect();
    budget.spend(n)?;
    let mut queued = graph.reachable.clone();
    while let Some(b) = pending.pop_front() {
        queued[b] = false;
        budget.spend(slots.desc_count.saturating_add(1))?;
        let mut live = vec![false; slots.desc_count];
        for &next in &graph.successors[b] {
            budget.spend(slots.desc_count)?;
            for (a, b) in live.iter_mut().zip(&input[next]) {
                *a |= *b;
            }
        }
        output[b].clone_from(&live);
        for i in f.blocks[b].instructions.iter().rev() {
            budget.spend(instruction_operands(&i.kind).count().saturating_add(1))?;
            old_reverse(slots, &mut live, i);
        }
        if input[b] != live {
            input[b] = live;
            for &p in &graph.predecessors[b] {
                budget.spend(1)?;
                if graph.reachable[p] && !queued[p] {
                    queued[p] = true;
                    pending.push_back(p);
                }
            }
        }
    }
    old_history(f, slots, &output, budget)
}

fn old_run(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    effects: &[BTreeSet<Root>],
    validate: bool,
    sum_rows: Option<&[Vec<sums::Bridge>]>,
    budget: &mut Budget,
    replacements: Option<&replacements::Plan>,
) -> Check<BTreeSet<Root>> {
    if f.foreign.is_some() {
        return Ok(BTreeSet::new());
    }
    let mut slots = Slots::new(module, f, &graph.package_views, budget)?;
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
    let after = old_liveness(f, graph, &slots, budget)?;
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
        after: &after,
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

fn old_set(words: &mut [usize], len: usize, index: usize, value: bool, b: &mut Budget) -> Check<()> {
    // Logical bound, checked word coordinate, mask, read, bit operation, write;
    // all payment and bounds checks precede mutation of this private scratch.
    b.spend(6)?;
    if index >= len {
        return Err(reject(
            "E-VERIFY-RESOURCE",
            None,
            "description history slot absent",
        ));
    }
    let at = &mut words[index / WORD];
    let mask = 1usize << (index % WORD);
    if value {
        *at |= mask;
    } else {
        *at &= !mask;
    }
    Ok(())
}

fn old_reverse_words(
    slots: &Slots,
    live: &mut [usize],
    instruction: &Instruction,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?; // actual instruction dispatch, including no-operand instructions
    if let Some(value) = instruction.result {
        b.spend(1)?; // actual SSA -> description coordinate read
        if let Some(index) = slots.desc_value[value.0 as usize] {
            old_set(live, slots.desc_count, index, false, b)?;
        }
    }
    match instruction.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => {
            b.spend(1)?;
            if let Some(index) = slots.desc_local[local.0 as usize] {
                old_set(live, slots.desc_count, index, false, b)?;
            }
        }
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. } => {
            b.spend(1)?;
            if let Some(index) = slots.desc_local[local.0 as usize] {
                old_set(live, slots.desc_count, index, true, b)?;
            }
        }
        _ => {}
    }
    for operand in instruction_operands(&instruction.kind) {
        b.spend(1)?; // operand visit and actual coordinate read
        if let Some(index) = slots.desc_value[operand.0 as usize] {
            old_set(live, slots.desc_count, index, true, b)?;
        }
    }
    Ok(())
}

fn old_history(
    f: &TypedFunction,
    slots: &Slots,
    output: &[Vec<bool>],
    budget: &mut Budget,
) -> Check<History> {
    if slots.desc_count <= SMALL {
        return old_dense(f, slots, output, budget);
    }
    let len = slots.desc_count;
    let count = len.div_ceil(WORD);
    // Outer container capacity/writes and allocation/release headers.
    budget.spend(f.blocks.len().saturating_mul(2).saturating_add(2))?;
    let mut blocks = Vec::with_capacity(f.blocks.len());
    for (block_index, block) in f.blocks.iter().enumerate() {
        // One actual output-row index, zeroed scratch words and Vec headers.
        budget.spend(count.saturating_add(3))?;
        let out = &output[block_index];
        let mut live = vec![0usize; count];
        for (index, &active) in out.iter().enumerate() {
            budget.spend(1)?; // original bool exit row read, even when inactive
            if active {
                old_set(&mut live, len, index, true, budget)?;
            }
        }
        // History row capacity and allocation/release headers; each clone below
        // pays its word payload, own Vec headers and history-slot write.
        // No empty per-instruction Vecs.
        budget.spend(block.instructions.len().saturating_add(2))?;
        let mut rows = Vec::with_capacity(block.instructions.len());
        for instruction in block.instructions.iter().rev() {
            budget.spend(count.saturating_add(3))?;
            rows.push(live.clone());
            old_reverse_words(slots, &mut live, instruction, budget)?;
        }
        // Reverse the rows into original instruction order. One saved header read
        // and two header writes per pair, including all-zero payload rows.
        budget.spend((rows.len() / 2).saturating_mul(3))?;
        rows.reverse();
        blocks.push(rows);
    }
    Ok(History::Packed { len, blocks })
}

fn old_dense(
    f: &TypedFunction,
    slots: &Slots,
    output: &[Vec<bool>],
    budget: &mut Budget,
) -> Check<History> {
    let mut after = Vec::with_capacity(f.blocks.len());
    for (b, block) in f.blocks.iter().enumerate() {
        budget.spend(
            block
                .instructions
                .len()
                .checked_mul(slots.desc_count)
                .ok_or_else(|| limit("resource liveness history overflow"))?,
        )?;
        let mut rows = vec![Vec::new(); block.instructions.len()];
        let mut live = output[b].clone();
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            rows[i] = live.clone();
            old_reverse(slots, &mut live, instruction);
        }
        after.push(rows);
    }
    Ok(History::Dense(after))
}
