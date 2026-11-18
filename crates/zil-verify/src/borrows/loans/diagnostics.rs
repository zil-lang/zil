use super::*;
use std::collections::VecDeque;
use zil_base::{DiagnosticIrLocation, DiagnosticRole};

#[derive(Clone, Copy)]
pub(super) struct Point<'a> {
    pub(super) graph: &'a FunctionGraph,
    pub(super) locals: &'a Locals,
    pub(super) block: usize,
    pub(super) instruction: usize,
}

fn location(f: &TypedFunction, block: usize, at: Option<usize>) -> DiagnosticIrLocation {
    DiagnosticIrLocation {
        function: f.id.0,
        block: Some(f.blocks[block].id.0),
        instruction: at.map(|value| value as u32),
    }
}

fn checked_span(module: &TypedModule, span: Span) -> Option<Span> {
    (module.semantic.is_some() && span.start < span.end).then_some(span)
}

fn creation(
    f: &TypedFunction,
    flow: &Flow,
    loan: usize,
    budget: &mut Budget,
) -> Check<Option<(usize, usize)>> {
    let mut value = (loan < f.values.len()).then_some(ValueId(loan as u32));
    if value.is_none() {
        budget.spend(flow.created.len())?;
        value = flow
            .created
            .iter()
            .find_map(|((value, _), id)| (*id == loan).then_some(*value));
    }
    let Some(value) = value else {
        return Ok(None);
    };
    for (b, block) in f.blocks.iter().enumerate() {
        budget.spend(block.instructions.len().saturating_add(1))?;
        if let Some(at) = block
            .instructions
            .iter()
            .position(|i| i.result == Some(value))
        {
            return Ok(Some((b, at)));
        }
    }
    Ok(None)
}

fn contains(flow: &Flow, row: &Loans, loan: usize, budget: &mut Budget) -> Check<bool> {
    budget.spend(row.len().saturating_add(1))?;
    Ok(row.contains(&loan) || row.iter().any(|id| flow.parents[*id].contains(&loan)))
}

fn later_use(
    f: &TypedFunction,
    flow: &Flow,
    point: Point<'_>,
    loan: usize,
    created: Option<(usize, usize)>,
    budget: &mut Budget,
) -> Check<Option<(usize, Option<usize>)>> {
    budget.spend(f.blocks.len())?;
    let mut entries = vec![None::<Locals>; f.blocks.len()];
    let mut queue = VecDeque::from([(
        point.block,
        point.instruction + 1,
        copy_state(point.locals, budget)?,
    )]);
    while let Some((b, first, mut state)) = queue.pop_front() {
        let block = &f.blocks[b];
        let mut renewed = false;
        for (at, instruction) in block.instructions.iter().enumerate().skip(first) {
            budget.spend(1)?;
            if created == Some((b, at)) {
                // This path has reached the next creation of the static loan
                // site. It cannot witness a later use of the current loan.
                renewed = true;
                break;
            }
            let direct_local = match instruction.kind {
                InstructionKind::Load(local)
                | InstructionKind::TakeLocal { local }
                | InstructionKind::CallClosureShared { callee: local, .. }
                | InstructionKind::CallClosureMut { callee: local, .. } => Some(local),
                _ => None,
            };
            if let Some(row) = direct_local.and_then(|local| state.get(&local))
                && contains(flow, row, loan, budget)?
            {
                return Ok(Some((b, Some(at))));
            }
            for input in instruction_operands(&instruction.kind) {
                if contains(flow, &flow.values[input.0 as usize], loan, budget)? {
                    return Ok(Some((b, Some(at))));
                }
            }
            local_transfer(f, &mut state, instruction, &flow.values, budget)?;
        }
        if renewed {
            continue;
        }
        let terminal_value = match block.terminator {
            Terminator::Return(value) => value,
            Terminator::Branch { condition, .. } => Some(condition),
            _ => None,
        };
        if let Some(value) = terminal_value
            && contains(flow, &flow.values[value.0 as usize], loan, budget)?
        {
            return Ok(Some((b, None)));
        }
        for &next in &point.graph.successors[b] {
            budget.spend(1)?;
            if !point.graph.reachable[next] {
                continue;
            }
            let changed = if let Some(existing) = &mut entries[next] {
                budget.spend(state.len())?;
                let mut changed = false;
                for (local, loans) in &state {
                    changed |= join(existing.entry(*local).or_default(), loans, budget)?;
                }
                changed
            } else {
                entries[next] = Some(copy_state(&state, budget)?);
                true
            };
            if changed {
                queue.push_back((
                    next,
                    0,
                    copy_state(entries[next].as_ref().unwrap(), budget)?,
                ));
            }
        }
    }
    Ok(None)
}

pub(super) fn conflict(access: &Access<'_>, loan: usize, budget: &mut Budget) -> Diagnostic {
    let module = access.module;
    let f = access.function;
    let point = access.point;
    let span = checked_span(module, access.span);
    let mut diagnostic = reject(
        "E-BORROW-CONFLICT",
        span.or(Some(access.span)),
        "access overlaps an active address window without its delegated authority",
    );
    let born = creation(f, access.flow, loan, budget).ok().flatten();
    if let Some((b, at)) = born {
        diagnostic.relate(
            DiagnosticRole::BorrowCreated,
            checked_span(module, f.blocks[b].instructions[at].span),
            Some(location(f, b, Some(at))),
            "the active address window is created here",
        );
    }
    diagnostic.relate(
        DiagnosticRole::ConflictingAccess,
        span,
        Some(location(f, point.block, Some(point.instruction))),
        "this access overlaps the active address window",
    );
    if let Ok(Some((b, at))) = later_use(f, access.flow, point, loan, born, budget) {
        diagnostic.relate(
            DiagnosticRole::LaterUse,
            at.and_then(|at| checked_span(module, f.blocks[b].instructions[at].span)),
            Some(location(f, b, at)),
            "this later use carries the address window or its descendant",
        );
    }
    diagnostic
}
