//! Independent DAG traversal and predecessor provenance reconstruction.
use super::*;
const CONFLICT: usize = usize::MAX;
#[derive(Clone)]
struct State {
    slots: Vec<(usize, u8)>,
    values: Vec<usize>,
    settled: u8,
}
fn failure() -> Diagnostic {
    bad("acyclic owning callback has no unique live actual parameter origin")
}
fn successors(block: &BasicBlock) -> Check<[Option<usize>; 2]> {
    match block.terminator {
        Terminator::Jump(id) => Ok([Some(id.0 as usize), None]),
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => Ok([Some(then_block.0 as usize), Some(else_block.0 as usize)]),
        Terminator::Return(_) | Terminator::Trap => Ok([None, None]),
        _ => Err(failure()),
    }
}
fn owner_value(f: &TypedFunction, value: ValueId) -> bool {
    matches!(f.values.get(value.0 as usize), Some(Type::Closure(_)))
}
fn owner_slot(f: &TypedFunction, local: LocalId) -> bool {
    matches!(
        f.locals.get(local.0 as usize).map(|d| &d.ty),
        Some(Type::Closure(_))
    )
}
fn copy(state: &State, b: &mut Budget) -> Check<State> {
    b.spend(
        state
            .slots
            .len()
            .saturating_mul(2)
            .saturating_add(state.values.len())
            .saturating_add(1),
    )?;
    Ok(state.clone())
}
fn join(target: &mut State, source: &State, b: &mut Budget) -> Check<()> {
    b.spend(
        target
            .slots
            .len()
            .saturating_mul(2)
            .saturating_add(target.values.len())
            .saturating_add(1),
    )?;
    for (a, c) in target.slots.iter_mut().zip(&source.slots) {
        if a.0 != c.0 {
            a.0 = CONFLICT;
        }
        a.1 |= c.1;
    }
    for (a, c) in target.values.iter_mut().zip(&source.values) {
        if a != c {
            *a = CONFLICT;
        }
    }
    if target.settled != source.settled {
        target.settled = 2;
    }
    Ok(())
}

pub(super) fn parameter(
    f: &TypedFunction,
    call: &InstructionKind,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> Check<usize> {
    let count = f.blocks.len();
    b.spend(
        count
            .saturating_mul(7)
            .saturating_add(f.values.len())
            .saturating_add(f.parameters.len())
            .saturating_add(1),
    )?;
    if count == 0 || f.entry.0 as usize >= count {
        return Err(failure());
    }
    let contract = f.callable_contract.as_ref().ok_or_else(failure)?;
    let [role] = contract.roles.as_slice() else {
        return Err(failure());
    };
    let position = role.parameter as usize;
    if role.closure != d.id
        || role.kind != CallableKind::FnOnce
        || role.cleanup != CallableCleanup::Owned
    {
        return Err(failure());
    }
    let parameter = *f.parameters.get(position).ok_or_else(failure)?;
    let mut owners = 0usize;
    for (index, local) in f.parameters.iter().enumerate() {
        if owner_slot(f, *local) {
            owners += 1;
            if index != position
                || f.locals.get(local.0 as usize).map(|l| &l.ty) != Some(&Type::Closure(d.id))
            {
                return Err(failure());
            }
        }
    }
    if owners != 1 {
        return Err(failure());
    }
    let mut definitions = vec![false; f.values.len()];
    for (index, block) in f.blocks.iter().enumerate() {
        if block.id.0 as usize != index {
            return Err(failure());
        }
        for target in successors(block)?.into_iter().flatten() {
            if target >= count || target == f.entry.0 as usize {
                return Err(failure());
            }
        }
        b.spend(block.instructions.len())?;
        for i in &block.instructions {
            if !super::staged::finite_instruction(&i.kind) {
                return Err(failure());
            }
            if let Some(value) = i.result {
                let seen = definitions.get_mut(value.0 as usize).ok_or_else(failure)?;
                if *seen {
                    return Err(failure());
                }
                *seen = true;
            }
        }
    }
    // Iterative DFS distinguishes an actual backedge from a diamond join.
    let mut color = vec![0u8; count];
    let mut stack = Vec::with_capacity(count);
    let mut order = Vec::with_capacity(count);
    stack.push((f.entry.0 as usize, 0usize));
    while let Some((node, edge)) = stack.last().copied() {
        b.spend(1)?;
        color[node] = 1;
        if edge == 2 {
            color[node] = 2;
            order.push(node);
            stack.pop();
            continue;
        }
        stack.last_mut().unwrap().1 += 1;
        if let Some(next) = successors(&f.blocks[node])?[edge] {
            match color[next] {
                1 => return Err(failure()),
                0 => stack.push((next, 0)),
                _ => {}
            }
        }
    }
    if order.len() != count {
        return Err(failure());
    }
    b.spend(count)?;
    order.reverse();
    let mut outputs: Vec<Option<State>> = vec![None; count];
    let mut found = false;
    for node in order {
        let mut state = if node == f.entry.0 as usize {
            b.spend(
                f.locals
                    .len()
                    .saturating_mul(2)
                    .saturating_add(f.values.len()),
            )?;
            let mut state = State {
                slots: vec![(0, 0); f.locals.len()],
                values: vec![0; f.values.len()],
                settled: 0,
            };
            let input = state
                .slots
                .get_mut(parameter.0 as usize)
                .ok_or_else(failure)?;
            *input = (position + 1, 1);
            state
        } else {
            let mut incoming = None;
            b.spend(count.saturating_mul(3))?;
            for (index, block) in f.blocks.iter().enumerate() {
                if successors(block)?
                    .into_iter()
                    .flatten()
                    .any(|next| next == node)
                {
                    let previous = outputs[index].as_ref().ok_or_else(failure)?;
                    if let Some(target) = &mut incoming {
                        join(target, previous, b)?;
                    } else {
                        incoming = Some(copy(previous, b)?);
                    }
                }
            }
            incoming.ok_or_else(failure)?
        };
        let block = &f.blocks[node];
        b.spend(block.instructions.len())?;
        for instruction in &block.instructions {
            if instruction.result.is_some_and(|v| owner_value(f, v))
                && !matches!(instruction.kind, InstructionKind::TakeLocal { .. })
            {
                return Err(failure());
            }
            match &instruction.kind {
                InstructionKind::TakeLocal { local } if owner_slot(f, *local) => {
                    let source = state.slots.get_mut(local.0 as usize).ok_or_else(failure)?;
                    if source.0 != position + 1 || source.1 & 2 != 0 || state.settled != 0 {
                        return Err(failure());
                    }
                    let value = instruction
                        .result
                        .filter(|v| f.values.get(v.0 as usize) == Some(&Type::Closure(d.id)))
                        .ok_or_else(failure)?;
                    state.values[value.0 as usize] = source.0;
                    source.0 = 0;
                }
                InstructionKind::Store { local, value } if owner_slot(f, *local) => {
                    let source = state.values.get_mut(value.0 as usize).ok_or_else(failure)?;
                    let dest = state.slots.get_mut(local.0 as usize).ok_or_else(failure)?;
                    if *local == parameter
                        || dest.0 != 0
                        || dest.1 & 2 != 0
                        || *source != position + 1
                        || state.settled != 0
                        || f.values.get(value.0 as usize) != Some(&Type::Closure(d.id))
                    {
                        return Err(failure());
                    }
                    dest.0 = *source;
                    *source = 0;
                }
                InstructionKind::CallClosureOnce { callee, arguments } => {
                    b.spend(arguments.len())?;
                    if arguments.iter().any(|v| owner_value(f, *v)) || state.settled != 0 {
                        return Err(failure());
                    }
                    let source = state
                        .values
                        .get_mut(callee.0 as usize)
                        .ok_or_else(failure)?;
                    if *source != position + 1 {
                        return Err(failure());
                    }
                    *source = 0;
                    state.settled = 1;
                    found |= std::ptr::eq(&instruction.kind, call);
                }
                InstructionKind::DropClosure { local } if owner_slot(f, *local) => {
                    let slot = &mut state.slots[local.0 as usize];
                    if slot.0 != 0 {
                        if slot.0 != position + 1 || state.settled != 0 {
                            return Err(failure());
                        }
                        slot.0 = 0;
                        state.settled = 1;
                    }
                    slot.1 |= 2;
                }
                InstructionKind::StorageLive(local) if owner_slot(f, *local) => {
                    let slot = &mut state.slots[local.0 as usize];
                    if slot.1 != 0 {
                        return Err(failure());
                    }
                    slot.1 = 1;
                }
                InstructionKind::StorageDead(local) if owner_slot(f, *local) => {
                    let slot = &mut state.slots[local.0 as usize];
                    if slot.0 != 0 {
                        return Err(failure());
                    }
                    slot.1 |= 2;
                }
                InstructionKind::Load(local) if owner_slot(f, *local) => return Err(failure()),
                _ => {
                    for operand in instruction_operands(&instruction.kind) {
                        b.spend(1)?;
                        if owner_value(f, operand) {
                            return Err(failure());
                        }
                    }
                }
            }
        }
        b.spend(1)?;
        match block.terminator {
            Terminator::Return(value) => {
                b.spend(state.slots.len().saturating_add(state.values.len()))?;
                if value.is_some_and(|v| owner_value(f, v))
                    || state.settled != 1
                    || state.slots.iter().any(|s| s.0 != 0)
                    || state.values.iter().any(|v| *v != 0)
                {
                    return Err(failure());
                }
            }
            Terminator::Branch { condition, .. } if owner_value(f, condition) => {
                return Err(failure());
            }
            _ => {}
        }
        outputs[node] = Some(state);
    }
    if !found {
        return Err(failure());
    }
    Ok(position)
}
