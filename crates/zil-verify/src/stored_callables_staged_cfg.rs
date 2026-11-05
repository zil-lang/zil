//! Independent graph traversal and predecessor provenance reconstruction.
//! Cycles are checked by paid whole-graph fixed-point sweeps.
use super::*;
const CONFLICT: usize = usize::MAX;
#[derive(Clone, PartialEq, Eq)]
struct State {
    slots: Vec<(usize, u8)>,
    values: Vec<usize>,
    settled: (u8, Vec<u8>),
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
            .saturating_add(state.settled.1.len())
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
            .saturating_add(target.settled.1.len())
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
    target.settled.0 |= source.settled.0;
    for (left, right) in target.settled.1.iter_mut().zip(&source.settled.1) {
        *left |= *right;
    }
    Ok(())
}

fn consumption(roles: &[CallableRole], origin: usize, b: &mut Budget) -> Check<usize> {
    if roles.len() > 1 {
        b.spend(roles.len())?;
    }
    let index = roles
        .iter()
        .position(|role| role.parameter as usize + 1 == origin)
        .ok_or_else(failure)?;
    Ok(index)
}

pub(super) fn parameter(
    f: &TypedFunction,
    call: &InstructionKind,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> Check<usize> {
    match solve(f, call, d, b, false) {
        Err(error) if error.category != zil_base::ResultCategory::ResourceLimit => {
            solve(f, call, d, b, true)
        }
        result => result,
    }
}
fn solve(
    f: &TypedFunction,
    call: &InstructionKind,
    d: &ClosureDefinition,
    b: &mut Budget,
    cyclic: bool,
) -> Check<usize> {
    let count = f.blocks.len();
    b.spend(
        count
            .saturating_mul(9)
            .saturating_add(f.values.len())
            .saturating_add(f.parameters.len())
            .saturating_add(1),
    )?;
    if count == 0 || f.entry.0 as usize >= count {
        return Err(failure());
    }
    let contract = f.callable_contract.as_ref().ok_or_else(failure)?;
    let roles = &contract.roles;
    if roles.is_empty() {
        return Err(failure());
    }
    if roles.len() > 1 {
        b.spend(
            roles
                .len()
                .saturating_add(f.parameters.len().saturating_mul(roles.len())),
        )?;
    }
    for role in roles {
        let parameter = f
            .parameters
            .get(role.parameter as usize)
            .ok_or_else(failure)?;
        let actual = &f.locals.get(parameter.0 as usize).ok_or_else(failure)?.ty;
        let valid = match role.cleanup {
            CallableCleanup::Owned => {
                role.kind == CallableKind::FnOnce && actual == &Type::Closure(role.closure)
            }
            CallableCleanup::Borrowed => matches!(actual,Type::Address {mutable,inner}
                if role.kind!=CallableKind::FnOnce && *mutable==(role.kind==CallableKind::FnMut)
                    && inner.as_ref()==&Type::Closure(role.closure)),
            _ => false,
        };
        if !valid {
            return Err(failure());
        }
    }
    if roles.len() == 1 && roles[0].closure != d.id {
        return Err(failure());
    }
    let mut owners = 0usize;
    for (index, local) in f.parameters.iter().enumerate() {
        if owner_slot(f, *local) {
            owners += 1;
            if !roles.iter().any(|role| role.parameter as usize == index) {
                return Err(failure());
            }
        }
    }
    if owners
        != roles
            .iter()
            .filter(|role| role.cleanup == CallableCleanup::Owned)
            .count()
    {
        return Err(failure());
    }
    let mut definitions = vec![false; f.values.len()];
    for (index, block) in f.blocks.iter().enumerate() {
        if block.id.0 as usize != index {
            return Err(failure());
        }
        for target in successors(block)?.into_iter().flatten() {
            if target >= count || (!cyclic && target == f.entry.0 as usize) {
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
    let mut backedge = false;
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
                1 if !cyclic => return Err(failure()),
                1 => backedge = true,
                0 => stack.push((next, 0)),
                _ => {}
            }
        }
    }
    if order.len() != count || (cyclic && !backedge) {
        return Err(failure());
    }
    b.spend(count)?;
    order.reverse();
    let mut outputs: Vec<Option<State>> = vec![None; count];
    let mut found = false;
    let mut answer = None;
    loop {
        let mut changed = false;
        for &node in &order {
            if roles.len() > 1 {
                b.spend(roles.len())?;
            }
            let mut state = if node == f.entry.0 as usize {
                b.spend(
                    f.locals
                        .len()
                        .saturating_mul(2)
                        .saturating_add(f.values.len()),
                )?;
                let mut state = State {
                    slots: vec![(0, 1); f.locals.len()],
                    values: vec![0; f.values.len()],
                    settled: (1, vec![1; roles.len() - 1]),
                };
                if roles.len() > 1 {
                    b.spend(roles.len())?;
                }
                for (index, role) in roles.iter().enumerate() {
                    if role.cleanup == CallableCleanup::Borrowed {
                        if index == 0 {
                            state.settled.0 = 2;
                        } else {
                            state.settled.1[index - 1] = 2;
                        }
                    } else {
                        let position = role.parameter as usize;
                        let parameter = f.parameters[position];
                        *state
                            .slots
                            .get_mut(parameter.0 as usize)
                            .ok_or_else(failure)? = (position + 1, 2);
                    }
                }
                if cyclic {
                    b.spend(count.saturating_mul(3))?;
                    for (index, block) in f.blocks.iter().enumerate() {
                        if successors(block)?
                            .into_iter()
                            .flatten()
                            .any(|next| next == node)
                            && let Some(previous) = &outputs[index]
                        {
                            join(&mut state, previous, b)?;
                        }
                    }
                }
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
                        let previous = match outputs[index].as_ref() {
                            Some(previous) => previous,
                            None if cyclic => continue,
                            None => return Err(failure()),
                        };
                        if let Some(target) = &mut incoming {
                            join(target, previous, b)?;
                        } else {
                            incoming = Some(copy(previous, b)?);
                        }
                    }
                }
                match incoming {
                    Some(incoming) => incoming,
                    None if cyclic => continue,
                    None => return Err(failure()),
                }
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
                        let mask = consumption(roles, source.0, b)?;
                        if source.1 != 2
                            || (if mask == 0 {
                                state.settled.0
                            } else {
                                state.settled.1[mask - 1]
                            }) != 1
                        {
                            return Err(failure());
                        }
                        let value = instruction
                            .result
                            .filter(|v| {
                                f.values.get(v.0 as usize) == Some(&f.locals[local.0 as usize].ty)
                            })
                            .ok_or_else(failure)?;
                        state.values[value.0 as usize] = source.0;
                        source.0 = 0;
                    }
                    InstructionKind::Store { local, value } if owner_slot(f, *local) => {
                        let source = state.values.get_mut(value.0 as usize).ok_or_else(failure)?;
                        let dest = state.slots.get_mut(local.0 as usize).ok_or_else(failure)?;
                        let mask = consumption(roles, *source, b)?;
                        if roles
                            .iter()
                            .any(|role| f.parameters[role.parameter as usize] == *local)
                            || dest.0 != 0
                            || dest.1 != 2
                            || (if mask == 0 {
                                state.settled.0
                            } else {
                                state.settled.1[mask - 1]
                            }) != 1
                            || f.values.get(value.0 as usize)
                                != Some(&f.locals[local.0 as usize].ty)
                        {
                            return Err(failure());
                        }
                        dest.0 = *source;
                        *source = 0;
                    }
                    InstructionKind::CallClosureOnce { callee, arguments } => {
                        b.spend(arguments.len())?;
                        if arguments.iter().any(|v| owner_value(f, *v)) {
                            return Err(failure());
                        }
                        let source = state
                            .values
                            .get_mut(callee.0 as usize)
                            .ok_or_else(failure)?;
                        let mask = consumption(roles, *source, b)?;
                        if (if mask == 0 {
                            state.settled.0
                        } else {
                            state.settled.1[mask - 1]
                        }) != 1
                        {
                            return Err(failure());
                        }
                        if std::ptr::eq(&instruction.kind, call) {
                            if f.values.get(callee.0 as usize) != Some(&Type::Closure(d.id)) {
                                return Err(failure());
                            }
                            found = true;
                            answer = Some(*source - 1);
                        }
                        *source = 0;
                        if mask == 0 {
                            state.settled.0 = 2;
                        } else {
                            state.settled.1[mask - 1] = 2;
                        }
                    }
                    InstructionKind::DropClosure { local } if owner_slot(f, *local) => {
                        let slot = &mut state.slots[local.0 as usize];
                        if slot.0 != 0 {
                            let mask = consumption(roles, slot.0, b)?;
                            if (if mask == 0 {
                                state.settled.0
                            } else {
                                state.settled.1[mask - 1]
                            }) != 1
                            {
                                return Err(failure());
                            }
                            slot.0 = 0;
                            if mask == 0 {
                                state.settled.0 = 2;
                            } else {
                                state.settled.1[mask - 1] = 2;
                            }
                        }
                        slot.1 = if cyclic { 1 } else { slot.1 | 4 };
                    }
                    InstructionKind::StorageLive(local) if owner_slot(f, *local) => {
                        let slot = &mut state.slots[local.0 as usize];
                        if slot.1 != 1
                            || (cyclic
                                && (slot.0 != 0
                                    || roles.iter().any(|role| {
                                        f.parameters[role.parameter as usize] == *local
                                    })))
                        {
                            return Err(failure());
                        }
                        slot.1 = 2;
                    }
                    InstructionKind::StorageDead(local) if owner_slot(f, *local) => {
                        let slot = &mut state.slots[local.0 as usize];
                        if slot.0 != 0 {
                            return Err(failure());
                        }
                        slot.1 = if cyclic { 1 } else { slot.1 | 4 };
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
                    b.spend(
                        state
                            .slots
                            .len()
                            .saturating_add(state.values.len())
                            .saturating_add(roles.len().saturating_sub(1)),
                    )?;
                    if value.is_some_and(|v| owner_value(f, v))
                        || state.settled.0 != 2
                        || state.settled.1.iter().any(|status| *status != 2)
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
            if cyclic {
                b.spend(
                    state
                        .slots
                        .len()
                        .saturating_mul(2)
                        .saturating_add(state.values.len())
                        .saturating_add(1),
                )?;
                changed |= outputs[node].as_ref() != Some(&state);
            }
            outputs[node] = Some(state);
        }
        if !cyclic || !changed {
            break;
        }
    }
    if cyclic {
        b.spend(count)?;
        if outputs.iter().any(Option::is_none) {
            return Err(failure());
        }
    }
    if !found {
        return Err(failure());
    }
    answer.ok_or_else(failure)
}
