//! Actual receiver occupancy between a close result and its use. Saved values
//! retain their reaching Store, so a repeated SSA call site is not an epoch.
use super::*;

fn replaces(i: &InstructionKind, local: LocalId) -> bool {
    match i {
        InstructionKind::Store { local: changed, .. }
        | InstructionKind::BorrowLocalMut { local: changed }
        | InstructionKind::TakeLocal { local: changed }
        | InstructionKind::DropClosure { local: changed }
        | InstructionKind::StorageDead(changed)
        | InstructionKind::StorageLive(changed) => *changed == local,
        // exits() proved this entry cannot replace any owned task capture.
        InstructionKind::CallClosureMut { callee, .. } if *callee == local => false,
        InstructionKind::Call { .. }
        | InstructionKind::CallIndirect { .. }
        | InstructionKind::ForeignCall { .. }
        | InstructionKind::WriteAddress { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallClosureShared { .. }
        | InstructionKind::CallClosureMut { .. }
        | InstructionKind::CallClosureOnce { .. }
        | InstructionKind::CallClosureBorrowed { .. }
        | InstructionKind::WithForeignCallback { .. }
        | InstructionKind::WithScopedThread { .. }
        | InstructionKind::WithSyncCell { .. } => true,
        _ => false,
    }
}

fn unchanged(
    code: &Code<'_>,
    from: Point,
    to: Point,
    local: LocalId,
    b: &mut Budget,
) -> Check<bool> {
    let mut pending = vec![to];
    let mut visited = BTreeSet::new();
    while let Some((block, end)) = pending.pop() {
        b.spend(visited.len().saturating_add(1))?;
        if !visited.insert((block, end)) {
            continue;
        }
        let Some(row) = code.f.blocks.get(block) else {
            return Ok(false);
        };
        if end > row.instructions.len() {
            return Ok(false);
        }
        let lower = if block == from.0 && from.1 <= end {
            from.1
        } else {
            0
        };
        for instruction in &row.instructions[lower..end] {
            b.spend(1)?;
            if replaces(&instruction.kind, local) {
                return Ok(false);
            }
        }
        if block == from.0 && lower == from.1 {
            continue;
        }
        if block == code.f.entry.0 as usize {
            return Ok(false);
        }
        for &parent in &code.predecessors[block] {
            if code.reachable[parent] {
                pending.push((parent, code.f.blocks[parent].instructions.len()));
            }
        }
    }
    Ok(true)
}

fn query(
    code: &Code<'_>,
    value: ValueId,
    use_at: Point,
    local: LocalId,
    b: &mut Budget,
    active: &mut BTreeSet<ValueId>,
) -> Check<bool> {
    b.spend(active.len().saturating_add(1))?;
    if !active.insert(value) {
        return Ok(false);
    }
    let Some((at, instruction)) = code.definition(value) else {
        active.remove(&value);
        return Ok(false);
    };
    let answer = match instruction.kind {
        InstructionKind::CallClosureMut { callee, .. } if callee == local => {
            code.f.values.get(value.0 as usize)
                == Some(&super::super::super::task_poll_type(&Type::Unit))
                && unchanged(code, (at.0, at.1 + 1), use_at, local, b)?
        }
        InstructionKind::Load(saved) => {
            if !unchanged(code, (at.0, at.1 + 1), use_at, local, b)? {
                active.remove(&value);
                return Ok(false);
            }
            let mut pending = vec![at];
            let mut visited = BTreeSet::new();
            let mut found = false;
            while let Some((block, end)) = pending.pop() {
                b.spend(visited.len().saturating_add(1))?;
                if !visited.insert((block, end)) {
                    continue;
                }
                let row = &code.f.blocks[block];
                let mut stored = false;
                for position in (0..end).rev() {
                    let instruction = &row.instructions[position];
                    b.spend(1)?;
                    if let InstructionKind::Store {
                        local: target,
                        value: source,
                    } = instruction.kind
                        && target == saved
                    {
                        if !query(code, source, (block, position), local, b, active)? {
                            active.remove(&value);
                            return Ok(false);
                        }
                        stored = true;
                        found = true;
                        break;
                    }
                    if replaces(&instruction.kind, local)
                        || matches!(instruction.kind, InstructionKind::StorageLive(target) | InstructionKind::StorageDead(target) if target == saved)
                    {
                        active.remove(&value);
                        return Ok(false);
                    }
                }
                if stored {
                    continue;
                }
                if block == code.f.entry.0 as usize {
                    active.remove(&value);
                    return Ok(false);
                }
                for &parent in &code.predecessors[block] {
                    if code.reachable[parent] {
                        pending.push((parent, code.f.blocks[parent].instructions.len()));
                    }
                }
            }
            found
        }
        _ => false,
    };
    active.remove(&value);
    Ok(answer)
}

pub(super) fn allow_at(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    at: Point,
    b: &mut Budget,
) -> Check<bool> {
    let Some(Type::Closure(id)) = f.locals.get(local.0 as usize).map(|l| &l.ty) else {
        return Ok(false);
    };
    let Some(d) = module.closures.get(id.0 as usize) else {
        return Ok(false);
    };
    let Some(tasks) = inventory(module, d, b)? else {
        return Ok(false);
    };
    if tasks.is_empty() {
        return Ok(true);
    }
    let Some(proof) = exits(module, *id, b)? else {
        return Ok(false);
    };
    let code = Code::new(f, b)?;
    for capture in tasks {
        if !proof.always.contains(&capture) && !proof.ready.contains(&capture) {
            return Ok(false);
        }
        let mut pending = vec![at];
        let mut seen = BTreeSet::new();
        while let Some((block, end)) = pending.pop() {
            b.spend(seen.len().saturating_add(1))?;
            if !seen.insert((block, end)) {
                continue;
            }
            let Some(row) = f.blocks.get(block) else {
                return Ok(false);
            };
            if end > row.instructions.len() {
                return Ok(false);
            }
            let mut anchored = false;
            for instruction in row.instructions[..end].iter().rev() {
                b.spend(1)?;
                if matches!(instruction.kind, InstructionKind::CallClosureMut { callee, .. }
                    if callee == local && proof.always.contains(&capture))
                {
                    anchored = true;
                    break;
                }
                if replaces(&instruction.kind, local) {
                    return Ok(false);
                }
            }
            if anchored {
                continue;
            }
            if block == f.entry.0 as usize {
                return Ok(false);
            }
            for &predecessor in &code.predecessors[block] {
                if !code.reachable[predecessor] {
                    continue;
                }
                let row = &f.blocks[predecessor];
                if proof.ready.contains(&capture)
                    && let Terminator::Branch {
                        condition,
                        then_block,
                        else_block,
                    } = row.terminator
                    && then_block != else_block
                    && let Some((value, yes)) = ready_test(&code, condition, b)?
                    && (if yes { then_block } else { else_block }).0 as usize == block
                    && query(
                        &code,
                        value,
                        (predecessor, row.instructions.len()),
                        local,
                        b,
                        &mut BTreeSet::new(),
                    )?
                {
                    continue;
                }
                pending.push((predecessor, row.instructions.len()));
            }
        }
    }
    Ok(true)
}
