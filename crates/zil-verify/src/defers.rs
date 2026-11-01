//! Independent must-invoke reconstruction over checked executable CFG.
//! Affine environment consumption alone cannot settle a registered action.
use super::*;

const UNSET: u8 = 0;
const STORED: u8 = 1;
const REGISTERED: u8 = 2;
const CALLED: u8 = 3;

#[derive(Clone, PartialEq, Eq)]
struct Activation {
    stack: Vec<ScopeId>,
    owner: Vec<Option<ScopeId>>,
    action: Vec<u8>,
    resource: Vec<bool>,
    newer: BTreeSet<(usize, usize)>,
}

impl Activation {
    fn charge(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(
            self.stack
                .len()
                .saturating_add(self.owner.len().saturating_mul(3))
                .saturating_add(self.newer.len())
                .saturating_add(1),
        )
    }

    fn retire(&mut self, local: usize, budget: &mut Budget) -> Check<()> {
        budget.spend(self.newer.len().saturating_add(1))?;
        self.resource[local] = false;
        self.newer
            .retain(|&(older, newer)| older != local && newer != local);
        Ok(())
    }

    fn born(&mut self, local: usize, budget: &mut Budget) -> Check<()> {
        self.retire(local, budget)?;
        budget.spend(self.resource.len())?;
        for (older, &live) in self.resource.iter().enumerate() {
            if live {
                budget.spend(
                    self.newer
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2),
                )?;
                self.newer.insert((older, local));
            }
        }
        self.resource[local] = true;
        Ok(())
    }

    fn check_last(&self, local: usize, span: Span, budget: &mut Budget) -> Check<()> {
        budget.spend(self.newer.len())?;
        if self
            .newer
            .iter()
            .any(|&(older, newer)| older == local && self.resource[newer])
        {
            return Err(fail(
                span,
                "cleanup violates reverse successful construction/registration order",
            ));
        }
        Ok(())
    }
}

fn fail(span: Span, message: &str) -> Diagnostic {
    reject("E-VERIFY-DEFER", Some(span), message)
}

fn is_owner(module: &TypedModule, ty: &Type, budget: &mut Budget) -> Check<bool> {
    budget.spend(1)?;
    Ok(match ty {
        Type::Managed(_) | Type::Package { .. } => true,
        Type::ErrorUnion { .. } => errors::owned(ty),
        Type::Enum { .. } => sums::owned_enum(ty),
        Type::Array { element, length } => {
            *length != 0 && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
        }
        Type::Closure(id) => {
            let c = &module.closures[id.0 as usize];
            budget.spend(c.captures.len())?;
            c.kind == CallableKind::FnOnce
                || c.captures.iter().any(|t| {
                    matches!(
                        t,
                        Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
                    )
                })
        }
        _ => false,
    })
}

// Check the entire remaining local cleanup segment before admitting any action
// call. This rules out an early ordinary-body call even if debt later balances.
fn suffix(
    block: &BasicBlock,
    start: usize,
    scope: ScopeId,
    actions: &[Option<ScopeId>],
    bindings: &[Option<u32>],
    budget: &mut Budget,
) -> Check<()> {
    let mut cursor = start;
    while let Some(instruction) = block.instructions.get(cursor) {
        budget.spend(1)?;
        match &instruction.kind {
            InstructionKind::ScopeExit { scope: owner, .. } if *owner == scope => return Ok(()),
            InstructionKind::TakeLocal { local } if actions[local.0 as usize] == Some(scope) => {
                budget.spend(1)?;
                if !block.instructions.get(cursor + 1).is_some_and(|next| {
                    next.result.is_none()
                        && matches!(&next.kind,
                        InstructionKind::CallClosureOnce { callee, arguments }
                        if Some(*callee) == instruction.result && arguments.is_empty())
                }) {
                    return Err(fail(
                        instruction.span,
                        "action TakeLocal requires its adjacent actual CallClosureOnce",
                    ));
                }
                cursor += 2;
            }
            InstructionKind::Load(local)
                if bindings[local.0 as usize].is_some() && actions[local.0 as usize].is_none() =>
            {
                budget.spend(1)?;
                if !block.instructions.get(cursor + 1).is_some_and(|next|
                    matches!(next.kind, InstructionKind::WriteCapture { index, value }
                        if Some(index) == bindings[local.0 as usize] && Some(value) == instruction.result))
                {
                    return Err(fail(instruction.span, "cleanup capture load must immediately write its bound capture"));
                }
                cursor += 2;
            }
            InstructionKind::DropManaged { .. }
            | InstructionKind::DropEnum { .. }
            | InstructionKind::DropClosure { .. }
            | InstructionKind::DropPackage { .. }
            | InstructionKind::DropErrorUnion { .. }
            | InstructionKind::DropOwnedArray { .. }
            | InstructionKind::StorageDead(_)
            | InstructionKind::EndPackageCapture { .. } => cursor += 1,
            _ => {
                return Err(fail(
                    instruction.span,
                    "defer call must occur in a cleanup suffix directly reaching its own ScopeExit",
                ));
            }
        }
    }
    Err(fail(
        block.instructions[start].span,
        "defer cleanup cannot cross a block edge before its ScopeExit",
    ))
}

pub(super) fn check(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<()> {
    let count = f.locals.len();
    budget.spend(count.saturating_mul(3).saturating_add(f.values.len()))?;
    let mut actions = vec![None; count];
    let mut owning = Vec::with_capacity(count);
    let mut bindings = vec![None; count];
    let mut capture_read = vec![None; f.values.len()];
    for local in &f.locals {
        owning.push(is_owner(module, &local.ty, budget)?);
    }
    budget.spend(f.blocks.len())?;
    for block in &f.blocks {
        for instruction in &block.instructions {
            budget.spend(1)?;
            if let InstructionKind::ReadCapture { index } = instruction.kind {
                capture_read[instruction.result.expect("checked capture read").0 as usize] =
                    Some(index);
            }
        }
    }
    budget.spend(f.blocks.len())?;
    for (block_index, block) in f.blocks.iter().enumerate() {
        for (position, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            if let InstructionKind::Store { local, value } = instruction.kind
                && let Some(capture) = capture_read[value.0 as usize]
            {
                bindings[local.0 as usize] = Some(capture);
            }
            let InstructionKind::RegisterDefer { scope, local } = instruction.kind else {
                continue;
            };
            let index = local.0 as usize;
            let Type::Closure(id) = f.locals[index].ty else {
                return Err(fail(
                    instruction.span,
                    "registered local is not a concrete closure",
                ));
            };
            let c = &module.closures[id.0 as usize];
            budget.spend(module.functions[c.function.0 as usize].blocks.len())?;
            for body_block in &module.functions[c.function.0 as usize].blocks {
                for body_instruction in &body_block.instructions {
                    budget.spend(1)?;
                    if matches!(body_instruction.kind, InstructionKind::RegisterDefer { .. }) {
                        return Err(fail(
                            body_instruction.span,
                            "nested defer registration is outside the admitted cleanup body",
                        ));
                    }
                }
            }
            budget.spend(f.parameters.len().saturating_add(1))?;
            if instruction.result.is_some()
                || c.kind != CallableKind::FnOnce
                || !c.parameters.is_empty()
                || c.result != Type::Unit
                || f.parameters.contains(&local)
                || actions[index].is_some()
                || !graph.reachable[block_index]
            {
                return Err(fail(
                    instruction.span,
                    "registration requires one reachable hidden zero-argument Unit FnOnce local",
                ));
            }
            if !position.checked_sub(1).and_then(|p| block.instructions.get(p)).is_some_and(|previous|
                previous.result.is_none() && matches!(previous.kind, InstructionKind::Store { local: target, .. } if target == local))
            {
                return Err(fail(instruction.span, "RegisterDefer must immediately follow its action's initial Store"));
            }
            actions[index] = Some(scope);
        }
    }
    budget.spend(
        count
            .saturating_mul(3)
            .saturating_add(f.blocks.len())
            .saturating_add(1),
    )?;
    let mut entry = Activation {
        stack: vec![],
        owner: vec![None; count],
        action: vec![UNSET; count],
        resource: vec![false; count],
        newer: BTreeSet::new(),
    };
    budget.spend(f.parameters.len())?;
    for &parameter in &f.parameters {
        if owning[parameter.0 as usize] {
            entry.born(parameter.0 as usize, budget)?;
        }
    }
    let mut incoming = vec![None; f.blocks.len()];
    incoming[f.entry.0 as usize] = Some(entry);
    let mut work = vec![f.entry.0 as usize];
    while let Some(block_index) = work.pop() {
        budget.spend(1)?;
        let input = incoming[block_index].as_ref().expect("defer input exists");
        input.charge(budget)?;
        let mut state = input.clone();
        let block = &f.blocks[block_index];
        let mut taken = None;
        for (position, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            if let Some((local, value)) = taken.take() {
                if !matches!(&instruction.kind, InstructionKind::CallClosureOnce { callee, arguments }
                    if *callee == value && arguments.is_empty())
                    || instruction.result.is_some()
                {
                    return Err(fail(
                        instruction.span,
                        "only an adjacent matching zero-argument CallClosureOnce fulfills defer",
                    ));
                }
                state.check_last(local, instruction.span, budget)?;
                state.action[local] = CALLED;
                state.retire(local, budget)?;
                continue;
            }
            match instruction.kind {
                InstructionKind::ScopeEnter { scope } => {
                    budget.spend(1)?;
                    if state.stack.is_empty() {
                        budget.spend(f.parameters.len())?;
                        for &parameter in &f.parameters {
                            state.owner[parameter.0 as usize] = Some(scope);
                        }
                    }
                    state.stack.push(scope);
                }
                InstructionKind::StorageLive(local) => {
                    let index = local.0 as usize;
                    state.owner[index] = state.stack.last().copied();
                    state.action[index] = UNSET;
                }
                InstructionKind::Store { local, .. } => {
                    let index = local.0 as usize;
                    if actions[index].is_some() {
                        if state.action[index] != UNSET
                            || !block.instructions.get(position + 1).is_some_and(|next|
                                matches!(next.kind, InstructionKind::RegisterDefer { local: target, .. } if target == local))
                        {
                            return Err(fail(instruction.span, "registered storage permits only its unique initial Store"));
                        }
                        state.action[index] = STORED;
                    } else if owning[index] {
                        state.born(index, budget)?;
                    }
                }
                InstructionKind::UnpackVariant {
                    local, ref fields, ..
                } => {
                    state.retire(local.0 as usize, budget)?;
                    budget.spend(fields.len())?;
                    for target in fields.iter().rev() {
                        if owning[target.0 as usize] {
                            state.born(target.0 as usize, budget)?;
                        }
                    }
                }
                InstructionKind::RegisterDefer { scope, local } => {
                    let index = local.0 as usize;
                    if state.stack.last() != Some(&scope)
                        || state.owner[index] != Some(scope)
                        || state.action[index] != STORED
                    {
                        return Err(fail(
                            instruction.span,
                            "defer registration has the wrong scope, storage activation or initialization",
                        ));
                    }
                    state.action[index] = REGISTERED;
                    state.born(index, budget)?;
                }
                InstructionKind::TakeLocal { local } => {
                    let index = local.0 as usize;
                    if let Some(scope) = actions[index] {
                        if state.action[index] != REGISTERED || state.stack.last() != Some(&scope) {
                            return Err(fail(
                                instruction.span,
                                "action take occurs before registration, after invocation or in a different scope",
                            ));
                        }
                        suffix(block, position, scope, &actions, &bindings, budget)?;
                        taken =
                            Some((index, instruction.result.expect("checked TakeLocal result")));
                        // Do not forget age or the pending action on TakeLocal.
                    } else {
                        state.retire(index, budget)?;
                    }
                }
                InstructionKind::DropManaged { local }
                | InstructionKind::DropEnum { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropOwnedArray { local } => {
                    let index = local.0 as usize;
                    if state.action[index] == REGISTERED {
                        return Err(fail(
                            instruction.span,
                            "dropping an environment does not invoke its registered cleanup body",
                        ));
                    }
                    state.check_last(index, instruction.span, budget)?;
                    state.retire(index, budget)?;
                }
                InstructionKind::Load(local)
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::DescribeLocal { local }
                | InstructionKind::CallClosureShared { callee: local, .. }
                | InstructionKind::CallClosureMut { callee: local, .. } => {
                    if actions[local.0 as usize].is_some() {
                        return Err(fail(
                            instruction.span,
                            "registered action storage cannot be loaded, borrowed, described or called ordinarily",
                        ));
                    }
                }
                InstructionKind::StorageDead(local) => {
                    let index = local.0 as usize;
                    if state.action[index] == REGISTERED {
                        return Err(fail(
                            instruction.span,
                            "StorageDead retains a must-invoke cleanup obligation",
                        ));
                    }
                    state.owner[index] = None;
                    state.action[index] = UNSET;
                    state.retire(index, budget)?;
                }
                InstructionKind::ScopeExit { scope, .. } => {
                    budget.spend(count)?;
                    if state.action.iter().enumerate().any(|(index, &status)| {
                        status == REGISTERED && state.owner[index] == Some(scope)
                    }) {
                        return Err(fail(
                            instruction.span,
                            "ScopeExit retains a must-invoke cleanup obligation",
                        ));
                    }
                    if state.stack.pop() != Some(scope) {
                        return Err(fail(
                            instruction.span,
                            "defer cleanup has a mismatched ScopeExit",
                        ));
                    }
                }
                _ => {}
            }
        }
        if taken.is_some() {
            return Err(fail(
                f.span,
                "action TakeLocal lacks an adjacent invocation",
            ));
        }
        if matches!(
            block.terminator,
            Terminator::Return(_) | Terminator::Unreachable
        ) {
            budget.spend(count)?;
            if state.action.contains(&REGISTERED) {
                return Err(fail(
                    f.span,
                    "normal exit retains a registered cleanup obligation",
                ));
            }
        }
        for &next in &graph.successors[block_index] {
            state.charge(budget)?;
            match &mut incoming[next] {
                Some(prior) => {
                    prior.charge(budget)?;
                    if prior.stack != state.stack
                        || prior.owner != state.owner
                        || prior.action != state.action
                    {
                        return Err(fail(
                            f.span,
                            "CFG join/backedge disagrees on registered action activation",
                        ));
                    }
                    let mut changed = false;
                    budget.spend(count)?;
                    for (target, &source) in prior.resource.iter_mut().zip(&state.resource) {
                        changed |= source && !*target;
                        *target |= source;
                    }
                    let before = prior.newer.len();
                    prior.newer.extend(&state.newer);
                    if changed || before != prior.newer.len() {
                        budget.spend(1)?;
                        work.push(next);
                    }
                }
                slot @ None => {
                    *slot = Some(state.clone());
                    budget.spend(1)?;
                    work.push(next);
                }
            }
        }
    }
    Ok(())
}
