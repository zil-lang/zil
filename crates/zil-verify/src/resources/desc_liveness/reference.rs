// Frozen complete 3145 bool solver/history and description consumer.
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
) -> Check<Vec<Vec<Vec<bool>>>> {
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
    let mut after = Vec::with_capacity(n);
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
    Ok(after)
}

fn old_no_live_description(
    state: &State,
    live: &[bool],
    root: Root,
    span: Span,
    budget: &mut Budget,
) -> Check<()> {
    for (active, roots) in live.iter().zip(&state.desc) {
        budget.spend(1)?;
        if *active {
            budget.spend(roots.len())?;
            if roots.iter().any(|candidate| {
                candidate.parent() == root.parent()
                    || matches!(
                        (candidate.parent(), root.parent()),
                        (Root::PackageField { local: a, .. } | Root::ArrayElement { local: a, .. }, Root::Local(b)) if a == b
                    )
            }) {
                return Err(bad(
                    span,
                    "moving, clearing or reconstructing a resource head would invalidate a live description",
                ));
            }
        }
    }
    Ok(())
}
