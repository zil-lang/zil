//! Frozen pre-word verifier liveness; no packed implementation is used here.
use super::*;
pub(super) fn reserve(budget: &mut Budget, sizes: &[usize]) -> Check<()> {
    let count = sizes
        .iter()
        .try_fold(1usize, |n, x| n.checked_mul(*x))
        .ok_or_else(|| limit("borrow state size overflow"))?;
    budget.spend(count)
}
pub(super) fn bits_join(target: &mut [bool], source: &[bool], budget: &mut Budget) -> Check<bool> {
    budget.spend(target.len())?;
    let mut changed = false;
    for (a, b) in target.iter_mut().zip(source) {
        if *b && !*a {
            *a = true;
            changed = true;
        }
    }
    Ok(changed)
}
pub(super) fn reverse(live: &mut [bool], instruction: &Instruction, value_count: usize) {
    if let Some(value) = instruction.result {
        live[value.0 as usize] = false;
    }
    match instruction.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => live[value_count + local.0 as usize] = false,
        InstructionKind::Load(local)
        | InstructionKind::TakeLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::BorrowDynOwned { local, .. }
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::CallClosureMut { callee: local, .. }
        | InstructionKind::CallClosureShared { callee: local, .. } => {
            live[value_count + local.0 as usize] = true
        }
        _ => {}
    }
    for operand in instruction_operands(&instruction.kind) {
        live[operand.0 as usize] = true;
    }
}
pub(super) fn retain_replacement_values(
    live: &mut [bool],
    schedule: Option<&crate::place_replacements::Schedule>,
    block: usize,
    position: usize,
    budget: &mut Budget,
) -> Check<()> {
    if let Some(schedule) = schedule {
        let row = &schedule.retained[block][position];
        budget.spend(row.len())?;
        for value in row {
            live[value.0 as usize] = true;
        }
    }
    Ok(())
}
pub(super) fn term_use(live: &mut [bool], terminator: &Terminator) {
    match terminator {
        Terminator::Return(Some(value))
        | Terminator::Branch {
            condition: value, ..
        } => live[value.0 as usize] = true,
        _ => {}
    }
}
pub(super) fn parent(
    f: &TypedFunction,
    graph: &FunctionGraph,
    schedule: Option<&crate::place_replacements::Schedule>,
    budget: &mut Budget,
) -> Check<Trace> {
    let n = f.blocks.len();
    let v = f.values.len();
    let m = f.locals.len();
    let reachable = &graph.reachable;
    let successors = &graph.successors;
    let width = v
        .checked_add(m)
        .ok_or_else(|| limit("borrow liveness width overflow"))?;
    reserve(budget, &[n, width, 2])?;
    let mut live_in = vec![vec![false; width]; n];
    let mut live_out = live_in.clone();
    let mut rounds = Vec::new();
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            budget.spend(1)?;
            if !reachable[b] {
                continue;
            }
            budget.spend(width)?;
            let mut live = vec![false; width];
            for &next in &successors[b] {
                bits_join(&mut live, &live_in[next], budget)?;
            }
            live_out[b].clone_from(&live);
            term_use(&mut live, &f.blocks[b].terminator);
            for (position, instruction) in f.blocks[b].instructions.iter().enumerate().rev() {
                budget.spend(1 + instruction_operands(&instruction.kind).count())?;
                reverse(&mut live, instruction, v);
                retain_replacement_values(&mut live, schedule, b, position, budget)?;
            }
            budget.spend(width)?;
            if live != live_in[b] {
                live_in[b] = live;
                changed = true;
            }
        }
        rounds.push((live_in.clone(), live_out.clone(), changed));
        if !changed {
            break;
        }
    }

    let mut afters = Vec::new();
    for (b, block) in f.blocks.iter().enumerate() {
        if !reachable[b] {
            afters.push(None);
            continue;
        }
        reserve(budget, &[block.instructions.len(), width])?;
        let mut after = vec![vec![]; block.instructions.len()];
        let mut live = live_out[b].clone();
        term_use(&mut live, &block.terminator);
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            after[i] = live.clone();
            reverse(&mut live, instruction, v);
            retain_replacement_values(&mut live, schedule, b, i, budget)?;
        }
        afters.push(Some(after));
    }
    Ok(Trace {
        input: live_in,
        output: live_out,
        rounds,
        afters,
    })
}
pub(super) fn loans(
    f: &TypedFunction,
    graph: &FunctionGraph,
    schedule: Option<&crate::place_replacements::Schedule>,
    budget: &mut Budget,
) -> Check<Trace> {
    let n = f.blocks.len();
    let v = f.values.len();
    let width = v
        .checked_add(f.locals.len())
        .ok_or_else(|| limit("loan liveness overflow"))?;
    reserve(budget, &[n, width, 2])?;
    let mut live_in = vec![vec![false; width]; n];
    let mut live_out = live_in.clone();
    let mut rounds = Vec::new();
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            if !graph.reachable[b] {
                continue;
            }
            budget.spend(width)?;
            let mut live = vec![false; width];
            for &next in &graph.successors[b] {
                bits_join(&mut live, &live_in[next], budget)?;
            }
            live_out[b].clone_from(&live);
            term_use(&mut live, &f.blocks[b].terminator);
            for (position, instruction) in f.blocks[b].instructions.iter().enumerate().rev() {
                budget.spend(1 + instruction_operands(&instruction.kind).count())?;
                reverse(&mut live, instruction, v);
                retain_replacement_values(&mut live, schedule, b, position, budget)?;
            }
            if live != live_in[b] {
                live_in[b] = live;
                changed = true;
            }
        }
        rounds.push((live_in.clone(), live_out.clone(), changed));
        if !changed {
            break;
        }
    }
    let mut afters = Vec::new();
    for (b, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[b] {
            afters.push(None);
            continue;
        }
        reserve(budget, &[block.instructions.len(), width])?;
        let mut after = vec![vec![]; block.instructions.len()];
        let mut live = live_out[b].clone();
        term_use(&mut live, &block.terminator);
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            after[i] = live.clone();
            reverse(&mut live, instruction, v);
            retain_replacement_values(&mut live, schedule, b, i, budget)?;
        }
        afters.push(Some(after));
    }
    Ok(Trace {
        input: live_in,
        output: live_out,
        rounds,
        afters,
    })
}
