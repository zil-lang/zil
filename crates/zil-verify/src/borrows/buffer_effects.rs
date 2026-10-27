//! Byte-storage effects and necessary parameter separation, reconstructed only
//! from checked CFGs. Finite prefixes are included even without a return edge.
use super::*;

pub(super) type Effects = BTreeMap<u32, bool>;
pub(super) type Apart = BTreeSet<(u32, u32)>;

pub(super) fn access(kind: &InstructionKind, head_windows: bool) -> Option<(ValueId, bool)> {
    match kind {
        InstructionKind::ReadDesc { desc, .. } if head_windows => Some((*desc, false)),
        InstructionKind::WriteDesc { desc, .. } if head_windows => Some((*desc, true)),
        _ => byte_buffers::access(kind),
    }
}

/// A borrowed descriptor environment can write any captured root. Reconstruct
/// the same concrete roots both for a call's local loan check and for exporting
/// effects from a head-free helper to its caller.
pub(super) fn callback_roots<'a>(
    module: &TypedModule,
    function: &TypedFunction,
    kind: &InstructionKind,
    values: &'a [Roots],
    locals: &'a State,
    budget: &mut Budget,
) -> Check<Option<&'a Roots>> {
    Ok(match kind {
        InstructionKind::CallClosure { callee, .. }
        | InstructionKind::CallClosureOnce { callee, .. }
        | InstructionKind::CallClosureBorrowed { callee, .. }
            if captures_descriptors(module, &function.values[callee.0 as usize], budget)? =>
        {
            Some(&values[callee.0 as usize])
        }
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. }
            if captures_descriptors(module, &function.locals[callee.0 as usize].ty, budget)? =>
        {
            lookup(budget, locals.len())?;
            locals.get(callee)
        }
        _ => None,
    })
}

fn add_effect(row: &mut Effects, parameter: u32, write: bool, budget: &mut Budget) -> Check<bool> {
    budget.spend(row.len().saturating_add(1))?;
    match row.get_mut(&parameter) {
        Some(old) if write && !*old => {
            *old = true;
            Ok(true)
        }
        Some(_) => Ok(false),
        None => {
            row.insert(parameter, write);
            Ok(true)
        }
    }
}

pub(super) fn infer(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    facts: &[Facts],
    head_windows: bool,
    budget: &mut Budget,
) -> Check<Vec<Effects>> {
    budget.spend(module.functions.len())?;
    let mut effects = vec![Effects::new(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, function) in module.functions.iter().enumerate() {
            budget.spend(1)?;
            if head_windows {
                // A synchronous helper can invoke its concrete borrowed-Desc
                // environment. Until a precise capture-access contract is
                // available here, publish a conservative write to its roots.
                for (parameter, local) in function.parameters.iter().enumerate() {
                    budget.spend(1)?;
                    if captures_descriptors(module, &function.locals[local.0 as usize].ty, budget)?
                    {
                        changed |= add_effect(&mut effects[index], parameter as u32, true, budget)?;
                    }
                }
            }
            for (block_index, (block, reachable)) in function
                .blocks
                .iter()
                .zip(&graphs[index].reachable)
                .enumerate()
            {
                budget.spend(1)?;
                if !reachable {
                    continue;
                }
                let mut descriptor_locals = if head_windows {
                    if let Some(input) = facts[index].inputs.get(block_index) {
                        state_cost(input, budget)?;
                        input.clone()
                    } else {
                        State::new()
                    }
                } else {
                    State::new()
                };
                for instruction in &block.instructions {
                    budget.spend(1)?;
                    if let Some((buffer, write)) = access(&instruction.kind, head_windows) {
                        for root in &facts[index].values[buffer.0 as usize] {
                            budget.spend(1)?;
                            if let Root::Parameter(parameter) = root {
                                changed |=
                                    add_effect(&mut effects[index], *parameter, write, budget)?;
                            }
                        }
                    }
                    if head_windows
                        && let Some(roots) = callback_roots(
                            module,
                            function,
                            &instruction.kind,
                            &facts[index].values,
                            &descriptor_locals,
                            budget,
                        )?
                    {
                        for root in roots {
                            budget.spend(1)?;
                            if let Root::Parameter(parameter) = root {
                                changed |=
                                    add_effect(&mut effects[index], *parameter, true, budget)?;
                            }
                        }
                    }
                    if let InstructionKind::Call {
                        function: callee,
                        arguments,
                    } = &instruction.kind
                    {
                        budget.spend(effects[callee.0 as usize].len())?;
                        let imported = effects[callee.0 as usize].clone();
                        for (parameter, write) in imported {
                            let argument = arguments[parameter as usize];
                            for root in &facts[index].values[argument.0 as usize] {
                                budget.spend(1)?;
                                if let Root::Parameter(parameter) = root {
                                    changed |=
                                        add_effect(&mut effects[index], *parameter, write, budget)?;
                                }
                            }
                        }
                    }
                    if head_windows {
                        local_transfer(
                            &mut descriptor_locals,
                            &instruction.kind,
                            &facts[index].values,
                            budget,
                        )?;
                    }
                }
            }
        }
        if !changed {
            return Ok(effects);
        }
    }
}

/// Different input names carry no separation evidence. Keep the obligation
/// symbolic until a caller can substitute actual owned storage roots.
pub(super) fn unknown_pairs(
    left: &[Root],
    right: &[Root],
    apart: &mut Apart,
    budget: &mut Budget,
) -> Check<()> {
    for a in left {
        for b in right {
            budget.spend(1)?;
            if let (Root::Parameter(a), Root::Parameter(b)) = (*a, *b)
                && a != b
            {
                budget.spend(apart.len().saturating_add(1))?;
                apart.insert((a.min(b), a.max(b)));
            }
        }
    }
    Ok(())
}

/// Instantiate a callee's required separation. Equal possible roots cannot
/// satisfy it; unequal input roots become the caller's own explicit obligation.
fn instantiate(
    left: &[Root],
    right: &[Root],
    apart: &mut Apart,
    span: Span,
    budget: &mut Budget,
) -> Check<()> {
    if left.is_empty() || right.is_empty() {
        return Err(reject(
            "E-BORROW-ORIGIN",
            Some(span),
            "byte-storage separation has an ungrounded actual argument",
        ));
    }
    for a in left {
        for b in right {
            budget.spend(1)?;
            if a.intersects(*b) {
                return Err(reject(
                    "E-BORROW-CONFLICT",
                    Some(span),
                    "call arguments alias where its independently reconstructed window contract requires separation",
                ));
            }
        }
    }
    unknown_pairs(left, right, apart, budget)
}

pub(super) fn close_apart(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    facts: &[Facts],
    mut apart: Vec<Apart>,
    budget: &mut Budget,
) -> Check<()> {
    loop {
        let mut changed = false;
        for (index, function) in module.functions.iter().enumerate() {
            budget.spend(1)?;
            for (block, reachable) in function.blocks.iter().zip(&graphs[index].reachable) {
                budget.spend(1)?;
                if !reachable {
                    continue;
                }
                for instruction in &block.instructions {
                    budget.spend(1)?;
                    let InstructionKind::Call {
                        function: callee,
                        arguments,
                    } = &instruction.kind
                    else {
                        continue;
                    };
                    budget.spend(apart[callee.0 as usize].len())?;
                    let imported = apart[callee.0 as usize].clone();
                    let before = apart[index].len();
                    for (a, b) in imported {
                        instantiate(
                            &facts[index].values[arguments[a as usize].0 as usize],
                            &facts[index].values[arguments[b as usize].0 as usize],
                            &mut apart[index],
                            instruction.span,
                            budget,
                        )?;
                    }
                    changed |= apart[index].len() != before;
                }
            }
        }
        if !changed {
            return Ok(());
        }
    }
}
