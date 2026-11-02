//! Independent path-sensitive instance lineage for one by-value F return.
//! ClosureId is a type; it never establishes which invocation supplied a value.
use super::*;
use std::collections::VecDeque;

const EMPTY: u8 = 1;
const PARAMETER: u8 = 2;
const OTHER: u8 = 4;

// Existing mutable owning locals lower to one internal array cell. This
// carrier is tracked as its sole element, not admitted as a public F shape.
fn singleton(f: &TypedFunction, ty: &Type) -> bool {
    matches!(ty, Type::Array { element, length: 1 } if element.as_ref() == &f.return_type)
}

fn tracked(f: &TypedFunction, ty: &Type) -> bool {
    ty == &f.return_type || singleton(f, ty)
}

fn live(mask: u8, f: &TypedFunction) -> Check<u8> {
    if mask == 0 || mask & EMPTY != 0 {
        return Err(bad(f, "callable instance is absent on a reachable path"));
    }
    Ok(mask)
}

fn value_slot(f: &TypedFunction, value: ValueId) -> usize {
    f.locals.len() + value.0 as usize
}

fn source_value(f: &TypedFunction, state: &[u8], value: ValueId) -> Check<u8> {
    live(state[value_slot(f, value)], f)
}

fn take_value(f: &TypedFunction, state: &mut [u8], value: ValueId) -> Check<u8> {
    let slot = value_slot(f, value);
    let origin = live(state[slot], f)?;
    state[slot] = EMPTY;
    Ok(origin)
}

fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    block: &BasicBlock,
    state: &mut [u8],
    budget: &mut Budget,
) -> Check<()> {
    let width = f.locals.len();
    for instruction in &block.instructions {
        budget.spend(1)?;
        let result = instruction
            .result
            .filter(|value| tracked(f, &f.values[value.0 as usize]));
        let mut output = None;
        match &instruction.kind {
            InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local)
            | InstructionKind::DropOwnedArray { local } => {
                state[local.0 as usize] = EMPTY;
            }
            InstructionKind::DropClosure { local } => {
                state[local.0 as usize] = EMPTY;
            }
            InstructionKind::TakeLocal { local } if result.is_some() => {
                let slot = local.0 as usize;
                output = Some(live(state[slot], f)?);
                state[slot] = EMPTY;
            }
            InstructionKind::Load(local) if result.is_some() => {
                let origin = live(state[local.0 as usize], f)?;
                if origin & PARAMETER != 0 {
                    return Err(bad(
                        f,
                        "symbolic F bound does not grant Copy of its actual parameter instance",
                    ));
                }
                output = Some(origin);
            }
            InstructionKind::Store { local, value }
                if tracked(f, &f.locals[local.0 as usize].ty) =>
            {
                state[local.0 as usize] = take_value(f, state, *value)?;
            }
            InstructionKind::MakeOwnedArray { elements } if result.is_some() => {
                let [source] = elements.as_slice() else {
                    return Err(bad(f, "forwarded F requires one internal array element"));
                };
                output = Some(take_value(f, state, *source)?);
            }
            InstructionKind::TakeOwnedArrayElement { local, index } if result.is_some() => {
                if *index != 0 || !singleton(f, &f.locals[local.0 as usize].ty) {
                    return Err(bad(
                        f,
                        "forwarded F take must name its actual array element",
                    ));
                }
                output = Some(live(state[local.0 as usize], f)?);
                state[local.0 as usize] = EMPTY;
            }
            InstructionKind::StoreOwnedArrayElement {
                local,
                index,
                value,
            } if singleton(f, &f.locals[local.0 as usize].ty) => {
                if *index != 0 {
                    return Err(bad(
                        f,
                        "forwarded F refill must name its actual array element",
                    ));
                }
                state[local.0 as usize] = take_value(f, state, *value)?;
            }
            InstructionKind::DropOwnedArrayElement { local, index }
                if singleton(f, &f.locals[local.0 as usize].ty) =>
            {
                if *index != 0 {
                    return Err(bad(
                        f,
                        "forwarded F drop must name its actual array element",
                    ));
                }
                state[local.0 as usize] = EMPTY;
            }
            InstructionKind::BorrowOwnedArrayElement { local, index, .. }
                if singleton(f, &f.locals[local.0 as usize].ty) =>
            {
                if *index != 0 {
                    return Err(bad(
                        f,
                        "forwarded F borrow must name its actual array element",
                    ));
                }
                live(state[local.0 as usize], f)?;
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                if let Some(output_value) = result {
                    let target = &module.functions[function.0 as usize];
                    output =
                        Some(
                            match target.callable_return_contract.as_ref().map(|c| c.source) {
                                Some(CallableReturnSource::Parameter(position)) => {
                                    let input = arguments.get(position as usize).ok_or_else(|| {
                                bad(f, "nested forwarding source names an absent actual argument")
                            })?;
                                    source_value(f, state, *input)?
                                }
                                Some(CallableReturnSource::Fresh) => OTHER,
                                None => {
                                    return Err(bad(
                                        f,
                                        "closure-producing call has no checked result relation",
                                    ));
                                }
                            },
                        );
                    if f.values[output_value.0 as usize] != target.return_type {
                        return Err(bad(
                            f,
                            "nested callable result differs from its actual return type",
                        ));
                    }
                }
                budget.spend(arguments.len())?;
                for value in arguments {
                    if singleton(f, &f.values[value.0 as usize]) {
                        return Err(bad(f, "internal F array cannot cross a call boundary"));
                    }
                    if f.values[value.0 as usize] == f.return_type {
                        take_value(f, state, *value)?;
                    }
                }
            }
            InstructionKind::CallClosureOnce { callee, arguments } => {
                if f.values[callee.0 as usize] == f.return_type {
                    take_value(f, state, *callee)?;
                }
                budget.spend(arguments.len())?;
                for value in arguments {
                    if tracked(f, &f.values[value.0 as usize]) {
                        take_value(f, state, *value)?;
                    }
                }
            }
            InstructionKind::CallClosure { callee, .. }
                if f.values[callee.0 as usize] == f.return_type
                    && state[value_slot(f, *callee)] & PARAMETER != 0 =>
            {
                return Err(bad(
                    f,
                    "symbolic F cannot be copied into a by-value Fn call",
                ));
            }
            InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. }
                if f.locals[callee.0 as usize].ty == f.return_type =>
            {
                live(state[callee.0 as usize], f)?;
            }
            InstructionKind::MakeClosure { captures, .. } => {
                budget.spend(captures.len())?;
                for value in captures {
                    if singleton(f, &f.values[value.0 as usize]) {
                        return Err(bad(f, "internal F array cannot be captured"));
                    }
                    if f.values[value.0 as usize] == f.return_type {
                        take_value(f, state, *value)?;
                    }
                }
                if result.is_some() {
                    output = Some(OTHER);
                }
            }
            _ if result.is_some() => output = Some(OTHER),
            _ => {}
        }
        if let Some(value) = result {
            let slot = width + value.0 as usize;
            if state[slot] & PARAMETER != 0 {
                return Err(bad(f, "loop redefines a still-live callable instance"));
            }
            state[slot] = output.unwrap_or(OTHER);
        }
        // The finite forwarding profile does not store F in arbitrary
        // aggregates or let an unclassified operation copy its role source.
        let known = matches!(
            instruction.kind,
            InstructionKind::Store { .. }
                | InstructionKind::MakeOwnedArray { .. }
                | InstructionKind::StoreOwnedArrayElement { .. }
                | InstructionKind::Call { .. }
                | InstructionKind::CallClosureOnce { .. }
                | InstructionKind::CallClosure { .. }
                | InstructionKind::MakeClosure { .. }
                | InstructionKind::CallClosureBorrowed { .. }
        );
        if !known {
            for operand in instruction_operands(&instruction.kind) {
                if tracked(f, &f.values[operand.0 as usize])
                    && state[value_slot(f, operand)] & PARAMETER != 0
                {
                    return Err(bad(f, "forwarded F enters an unsupported value operation"));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn check(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    parameter: u32,
    budget: &mut Budget,
) -> Check<()> {
    let blocks = f.blocks.len();
    let width = f
        .locals
        .len()
        .checked_add(f.values.len())
        .ok_or_else(|| limit("forwarded instance state width overflow"))?;
    let allowance = width
        .checked_mul(blocks)
        .and_then(|n| n.checked_mul(2))
        .ok_or_else(|| limit("forwarded instance state size overflow"))?;
    budget.spend(allowance.saturating_add(blocks.saturating_mul(2)))?;
    let mut inputs = vec![None::<Vec<u8>>; blocks];
    let mut queued = vec![false; blocks];
    let mut queue = VecDeque::new();
    let mut initial = vec![EMPTY; width];
    initial[f.parameters[parameter as usize].0 as usize] = PARAMETER;
    inputs[f.entry.0 as usize] = Some(initial);
    queued[f.entry.0 as usize] = true;
    queue.push_back(f.entry.0 as usize);
    let mut returns = 0usize;
    while let Some(block) = queue.pop_front() {
        queued[block] = false;
        budget.spend(width.saturating_add(1))?;
        let mut state = inputs[block].as_ref().unwrap().clone();
        transfer(module, f, &f.blocks[block], &mut state, budget)?;
        if let Terminator::Return(Some(value)) = f.blocks[block].terminator {
            budget.spend(width)?;
            if source_value(f, &state, value)? != PARAMETER
                || state
                    .iter()
                    .enumerate()
                    .any(|(index, origin)| index != value_slot(f, value) && origin & PARAMETER != 0)
            {
                return Err(bad(
                    f,
                    "return is not this invocation's one live callable parameter instance",
                ));
            }
            returns += 1;
        }
        for &next in &graph.successors[block] {
            budget.spend(width.saturating_add(1))?;
            let changed = if let Some(input) = &mut inputs[next] {
                let mut changed = false;
                for (dst, src) in input.iter_mut().zip(&state) {
                    let joined = *dst | *src;
                    changed |= *dst != joined;
                    *dst = joined;
                }
                changed
            } else {
                inputs[next] = Some(state.clone());
                true
            };
            if changed && !queued[next] {
                queued[next] = true;
                queue.push_back(next);
            }
        }
    }
    if returns == 0 {
        return Err(bad(f, "forwarding has no reachable normal callable return"));
    }
    Ok(())
}
