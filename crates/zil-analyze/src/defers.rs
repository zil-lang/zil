//! Producer diagnostics for mandatory cleanup calls. No certificate is exported.
//! Kept separate from the affine resource analysis: taking an action transfers
//! its environment but does not discharge the obligation to invoke its body.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Default, PartialEq, Eq)]
struct State {
    scopes: Vec<ScopeId>,
    root_entered: bool,
    storage: BTreeMap<LocalId, ScopeId>,
    // A registered slot has exactly one initial store in each activation.
    stored: BTreeSet<LocalId>,
    pending: BTreeMap<LocalId, ScopeId>,
    invoked: BTreeSet<LocalId>,
    owned: BTreeSet<LocalId>,
    later: BTreeSet<(LocalId, LocalId)>,
}

fn error(message: &str) -> Vec<Diagnostic> {
    bad(format!("defer: {message}"))
}

fn cost(state: &State, budget: &mut Budget) -> CompileResult<()> {
    budget.spend(
        state
            .scopes
            .len()
            .saturating_add(state.storage.len())
            .saturating_add(state.stored.len())
            .saturating_add(state.pending.len())
            .saturating_add(state.invoked.len())
            .saturating_add(state.owned.len())
            .saturating_add(state.later.len())
            .saturating_add(1),
        "defer state traversal/copy",
    )
}

fn forget(state: &mut State, local: LocalId, budget: &mut Budget) -> CompileResult<()> {
    budget.spend(state.later.len().saturating_add(1), "defer order removal")?;
    state.owned.remove(&local);
    state.later.retain(|(a, b)| *a != local && *b != local);
    Ok(())
}

fn construct(state: &mut State, local: LocalId, budget: &mut Budget) -> CompileResult<()> {
    forget(state, local, budget)?;
    budget.spend(
        state.owned.len().saturating_add(1),
        "defer construction inventory",
    )?;
    for &older in &state.owned {
        budget.lookup(state.later.len(), "defer mixed order insertion")?;
        state.later.insert((older, local));
    }
    state.owned.insert(local);
    Ok(())
}

fn reverse(state: &State, local: LocalId, budget: &mut Budget) -> CompileResult<()> {
    for &(older, newer) in &state.later {
        budget.spend(1, "defer mixed cleanup order")?;
        if older == local {
            budget.lookup(state.owned.len(), "defer newer live lookup")?;
            if state.owned.contains(&newer) {
                return Err(error(
                    "cleanup is not reverse successful construction/registration order",
                ));
            }
        }
    }
    Ok(())
}

fn owned(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(1, "defer owned classification")?;
    Ok(match ty {
        Type::Managed(_) | Type::Package { .. } => true,
        ty if resources::owned_error(ty) => true,
        Type::Array { element, length } => {
            *length > 0 && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
        }
        Type::Closure(id) => {
            let c = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| error("absent concrete environment"))?;
            budget.spend(c.captures.len(), "defer environment classification")?;
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

pub(super) fn check(
    module: &TypedModule,
    f: &TypedFunction,
    budget: &mut Budget,
) -> CompileResult<()> {
    let mut actions = BTreeMap::new();
    let mut capture_reads = BTreeMap::new();
    let mut capture_locals = BTreeMap::new();
    budget.spend(f.blocks.len(), "defer capture source block inventory")?;
    for block in &f.blocks {
        for instruction in &block.instructions {
            budget.spend(1, "defer capture writeback source inventory")?;
            if let InstructionKind::ReadCapture { index } = instruction.kind {
                if let Some(value) = instruction.result {
                    budget.lookup(capture_reads.len(), "defer capture read insertion")?;
                    capture_reads.insert(value, index);
                }
            }
        }
    }
    // This inventory is only run after the existing scope scan finds Register.
    // In particular historical modules pay no new logical work units.
    budget.spend(f.blocks.len(), "defer registration block inventory")?;
    for block in &f.blocks {
        for (position, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1, "defer registration inventory")?;
            if let InstructionKind::Store { local, value } = instruction.kind {
                budget.lookup(capture_reads.len(), "defer capture binding lookup")?;
                if let Some(&index) = capture_reads.get(&value) {
                    budget.lookup(capture_locals.len(), "defer capture binding insertion")?;
                    capture_locals.insert(local, index);
                }
            }
            if let InstructionKind::RegisterDefer { scope, local } = instruction.kind {
                let ty = f
                    .locals
                    .get(local.0 as usize)
                    .map(|l| &l.ty)
                    .ok_or_else(|| error("registered local is absent"))?;
                let Type::Closure(id) = ty else {
                    return Err(error("registered action must be a concrete FnOnce closure"));
                };
                let c = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| error("registered environment is absent"))?;
                if let Some(body) = module.functions.get(c.function.0 as usize) {
                    budget.spend(body.blocks.len(), "defer nested action block traversal")?;
                    for block in &body.blocks {
                        for instruction in &block.instructions {
                            budget.spend(1, "defer nested action exclusion")?;
                            if matches!(instruction.kind, InstructionKind::RegisterDefer { .. }) {
                                return Err(error("nested cleanup registration is not admitted"));
                            }
                        }
                    }
                }
                budget.spend(
                    f.parameters.len().saturating_add(1),
                    "defer registration shape",
                )?;
                if c.kind != CallableKind::FnOnce
                    || !c.parameters.is_empty()
                    || c.result != Type::Unit
                    || instruction.result.is_some()
                    || f.parameters.contains(&local)
                {
                    return Err(error(
                        "registration requires a hidden zero-argument Unit FnOnce local",
                    ));
                }
                let previous = position
                    .checked_sub(1)
                    .and_then(|p| block.instructions.get(p));
                let Some(Instruction {
                    result: None,
                    kind:
                        InstructionKind::Store {
                            local: target,
                            value,
                        },
                    ..
                }) = previous
                else {
                    return Err(error(
                        "registration must immediately follow its initial Store",
                    ));
                };
                if target != &local || f.values.get(value.0 as usize) != Some(ty) {
                    return Err(error(
                        "registration Store has a different local or environment type",
                    ));
                }
                budget.lookup(actions.len(), "defer action inventory insertion")?;
                if actions.insert(local, scope).is_some() {
                    return Err(error("an action slot has multiple static registrations"));
                }
            }
        }
    }
    budget.spend(f.locals.len(), "defer owned local inventory")?;
    let mut owning = Vec::with_capacity(f.locals.len());
    for local in &f.locals {
        owning.push(owned(module, &local.ty, budget)?);
    }
    budget.spend(f.blocks.len(), "defer entry table")?;
    let mut inputs = vec![None::<State>; f.blocks.len()];
    budget.spend(f.blocks.len(), "defer reachability inventory")?;
    let mut visited = vec![false; f.blocks.len()];
    let mut initial = State::default();
    budget.spend(f.parameters.len(), "defer initial parameter order")?;
    for &local in &f.parameters {
        if owning[local.0 as usize] {
            construct(&mut initial, local, budget)?;
        }
    }
    inputs[f.entry.0 as usize] = Some(initial);
    budget.spend(1, "defer initial work queue")?;
    let mut queue = VecDeque::from([f.entry.0 as usize]);
    while let Some(index) = queue.pop_front() {
        budget.spend(1, "defer work block")?;
        visited[index] = true;
        let input = inputs[index].as_ref().expect("defer queue input");
        cost(input, budget)?;
        let mut state = input.clone();
        let block = &f.blocks[index];
        let mut suffix = None;
        let mut call = None;
        let mut writeback = None;
        for (position, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1, "defer instruction transfer")?;
            if let Some((local, value)) = call.take() {
                if !matches!(&instruction.kind, InstructionKind::CallClosureOnce { callee, arguments }
                    if *callee == value && arguments.is_empty())
                    || instruction.result.is_some()
                {
                    return Err(error(
                        "TakeLocal must be immediately followed by its zero-argument CallClosureOnce",
                    ));
                }
                reverse(&state, local, budget)?;
                budget.lookup(state.pending.len(), "defer actual call discharge")?;
                state.pending.remove(&local);
                state.invoked.insert(local);
                forget(&mut state, local, budget)?;
                continue;
            }
            if let Some(value) = writeback.take() {
                if !matches!(instruction.kind, InstructionKind::WriteCapture { value: actual, .. } if actual == value)
                {
                    return Err(error("cleanup capture load requires adjacent writeback"));
                }
                continue;
            }
            if suffix.is_some() {
                match instruction.kind {
                    InstructionKind::TakeLocal { local } if state.pending.contains_key(&local) => {}
                    InstructionKind::DropManaged { .. }
                    | InstructionKind::DropClosure { .. }
                    | InstructionKind::DropPackage { .. }
                    | InstructionKind::DropErrorUnion { .. }
                    | InstructionKind::DropOwnedArray { .. }
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::EndPackageCapture { .. }
                    | InstructionKind::ScopeExit { .. } => {}
                    InstructionKind::Load(local) if !actions.contains_key(&local) => {
                        let value = instruction
                            .result
                            .ok_or_else(|| error("cleanup load has no result"))?;
                        budget.lookup(capture_locals.len(), "defer cleanup capture binding")?;
                        let capture = capture_locals.get(&local).copied();
                        if !block.instructions.get(position + 1).is_some_and(|next|
                            matches!(next.kind, InstructionKind::WriteCapture { index, value: actual }
                                if actual == value && Some(index) == capture))
                        {
                            return Err(error("cleanup suffix contains an ordinary load"));
                        }
                        writeback = Some(value);
                        continue;
                    }
                    _ => {
                        return Err(error(
                            "defer invocation must be in a cleanup suffix ending at its ScopeExit",
                        ));
                    }
                }
            }
            match instruction.kind {
                InstructionKind::ScopeEnter { scope } => {
                    budget.spend(
                        state.scopes.len().saturating_add(1),
                        "defer scope activation",
                    )?;
                    if state.scopes.is_empty() {
                        if state.root_entered {
                            return Err(error(
                                "a closed action-bearing function root cannot be reopened",
                            ));
                        }
                        state.root_entered = true;
                        budget.spend(f.parameters.len(), "defer parameter storage")?;
                        for &local in &f.parameters {
                            state.storage.insert(local, scope);
                        }
                    }
                    state.scopes.push(scope);
                }
                InstructionKind::StorageLive(local) => {
                    let scope = *state
                        .scopes
                        .last()
                        .ok_or_else(|| error("storage has no active scope"))?;
                    budget.lookup(state.storage.len(), "defer local activation")?;
                    state.storage.insert(local, scope);
                    state.stored.remove(&local);
                    state.invoked.remove(&local);
                }
                InstructionKind::RegisterDefer { scope, local } => {
                    budget.lookup(state.storage.len(), "defer registration owner")?;
                    budget.lookup(state.pending.len(), "defer pending insertion")?;
                    if state.scopes.last() != Some(&scope)
                        || state.storage.get(&local) != Some(&scope)
                        || !state.stored.contains(&local)
                        || state.pending.contains_key(&local)
                        || state.invoked.contains(&local)
                    {
                        return Err(error(
                            "registration is not an initialized action in its current storage activation",
                        ));
                    }
                    state.pending.insert(local, scope);
                    construct(&mut state, local, budget)?;
                }
                InstructionKind::Store { local, .. } => {
                    budget.lookup(actions.len(), "defer protected store")?;
                    if actions.contains_key(&local) {
                        budget.lookup(state.stored.len(), "defer unique initial store")?;
                        if state.stored.contains(&local) || !block.instructions.get(position + 1).is_some_and(|next|
                            matches!(next.kind, InstructionKind::RegisterDefer { local: next, .. } if next == local))
                        {
                            return Err(error("an action slot permits only its initial Store immediately before RegisterDefer"));
                        }
                        state.stored.insert(local);
                    } else if owning[local.0 as usize] {
                        construct(&mut state, local, budget)?;
                    }
                }
                InstructionKind::TakeLocal { local } => {
                    budget.lookup(actions.len(), "defer protected take")?;
                    if actions.contains_key(&local) {
                        let scope = *state.pending.get(&local).ok_or_else(|| {
                            error("action taken before registration or after invocation")
                        })?;
                        if state.scopes.last() != Some(&scope) {
                            return Err(error(
                                "action invocation is outside its innermost owner scope",
                            ));
                        }
                        let value = instruction
                            .result
                            .ok_or_else(|| error("action TakeLocal has no result"))?;
                        suffix = Some(scope);
                        call = Some((local, value));
                        // Deliberately keep both pending and mixed-order edges.
                    } else {
                        forget(&mut state, local, budget)?;
                    }
                }
                InstructionKind::DropManaged { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropOwnedArray { local } => {
                    budget.lookup(state.pending.len(), "defer protected cleanup")?;
                    if state.pending.contains_key(&local) {
                        return Err(error("Drop cannot cancel a registered cleanup action"));
                    }
                    reverse(&state, local, budget)?;
                    forget(&mut state, local, budget)?;
                }
                InstructionKind::Load(local)
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::DescribeLocal { local }
                | InstructionKind::CallClosureShared { callee: local, .. }
                | InstructionKind::CallClosureMut { callee: local, .. } => {
                    budget.lookup(actions.len(), "defer protected ordinary use")?;
                    if actions.contains_key(&local) {
                        return Err(error(
                            "a cleanup action cannot be loaded, borrowed, described or called ordinarily",
                        ));
                    }
                }
                InstructionKind::StorageDead(local) => {
                    budget.lookup(state.pending.len(), "defer storage obligation")?;
                    if state.pending.contains_key(&local) {
                        return Err(error("storage ends with an uninvoked cleanup action"));
                    }
                    state.storage.remove(&local);
                    state.stored.remove(&local);
                    state.invoked.remove(&local);
                    forget(&mut state, local, budget)?;
                }
                InstructionKind::ScopeExit { scope, .. } => {
                    budget.spend(state.pending.len(), "defer scope obligations")?;
                    if state.pending.values().any(|owner| *owner == scope) {
                        return Err(error("scope exits with an uninvoked cleanup action"));
                    }
                    if state.scopes.pop() != Some(scope) {
                        return Err(error("cleanup exits the wrong scope"));
                    }
                    if suffix == Some(scope) {
                        suffix = None;
                    }
                }
                _ => {}
            }
        }
        if call.is_some() || writeback.is_some() || suffix.is_some() {
            return Err(error(
                "defer invocation must reach its ScopeExit in the same block",
            ));
        }
        if matches!(
            block.terminator,
            Terminator::Return(_) | Terminator::Unreachable
        ) && !state.pending.is_empty()
        {
            return Err(error("normal exit retains an uninvoked cleanup action"));
        }
        let targets = match block.terminator {
            Terminator::Jump(target) => [Some(target), None],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => [Some(then_block), Some(else_block)],
            _ => [None, None],
        };
        for target in targets.into_iter().flatten() {
            cost(&state, budget)?;
            let slot = &mut inputs[target.0 as usize];
            if let Some(previous) = slot {
                cost(previous, budget)?;
                if previous.scopes != state.scopes
                    || previous.root_entered != state.root_entered
                    || previous.storage != state.storage
                    || previous.stored != state.stored
                    || previous.pending != state.pending
                    || previous.invoked != state.invoked
                {
                    return Err(error(
                        "CFG join or backedge has incompatible action activations",
                    ));
                }
                let before = (previous.owned.len(), previous.later.len());
                previous.owned.extend(&state.owned);
                previous.later.extend(&state.later);
                if before != (previous.owned.len(), previous.later.len()) {
                    budget.spend(1, "defer fixed-point queue growth")?;
                    queue.push_back(target.0 as usize);
                }
            } else {
                *slot = Some(state.clone());
                budget.spend(1, "defer work queue growth")?;
                queue.push_back(target.0 as usize);
            }
        }
    }
    for (index, block) in f.blocks.iter().enumerate() {
        budget.spend(1, "defer reachable registration block")?;
        if !visited[index] {
            for instruction in &block.instructions {
                budget.spend(1, "defer unreachable registration exclusion")?;
                if matches!(instruction.kind, InstructionKind::RegisterDefer { .. }) {
                    return Err(error(
                        "unreachable registration cannot bypass obligation checking",
                    ));
                }
            }
        }
    }
    Ok(())
}
