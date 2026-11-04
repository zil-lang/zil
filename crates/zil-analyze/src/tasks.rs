//! Producer checks reconstruct the scalar task profile and actual suspension.
use super::*;
fn fail(span: zil_base::Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-TASK-ABI",
        Some(span),
        message,
    )]
}
fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64
    )
}
fn worker(
    module: &TypedModule,
    id: FunctionId,
    budget: &mut Budget,
    span: zil_base::Span,
) -> CompileResult<()> {
    let f = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| fail(span, "task worker is absent"))?;
    budget.spend(
        f.locals
            .len()
            .saturating_add(f.values.len())
            .saturating_add(module.resources.len())
            .saturating_add(1),
        "task worker real scalar profile",
    )?;
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.result_mode != PassingMode::Infer
        || f.parameters.len() != 1
        || f.locals
            .get(f.parameters[0].0 as usize)
            .is_none_or(|l| l.ty != Type::I32)
        || f.return_type != Type::I32
        || f.locals.iter().any(|l| !scalar(&l.ty))
        || f.values.iter().any(|t| !scalar(t))
        || module.resources.iter().any(|r| r.teardown == Some(id))
    {
        return Err(fail(
            span,
            "task worker requires a closed scalar fn(i32)->i32 without external users or cleanup",
        ));
    }
    for b in &f.blocks {
        budget.spend(1, "task worker block")?;
        for i in &b.instructions {
            budget.spend(1, "task worker instruction")?;
            if !matches!(
                i.kind,
                InstructionKind::Constant(
                    Constant::Unit
                        | Constant::Bool(_)
                        | Constant::U8(_)
                        | Constant::U16(_)
                        | Constant::I32(_)
                        | Constant::I64(_)
                ) | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::Unary {
                        op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                        ..
                    }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::TaskYield
            ) {
                return Err(fail(
                    i.span,
                    "scalar task worker contains an external, owning or unsupported operation",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if !budget.tasks {
        return Ok(());
    }
    budget.spend(
        module.functions.len(),
        "task suspension inventory allocation",
    )?;
    let mut suspend = vec![false; module.functions.len()];
    for (index, f) in module.functions.iter().enumerate() {
        for block in &f.blocks {
            for i in &block.instructions {
                budget.spend(1, "task suspension inventory")?;
                let ty = |v: ValueId| {
                    f.values
                        .get(v.0 as usize)
                        .ok_or_else(|| fail(i.span, "task operand is absent"))
                };
                let expected = match i.kind {
                    InstructionKind::TaskStart {
                        worker: id,
                        argument,
                    } => {
                        if ty(argument)? != &Type::I32 {
                            return Err(fail(i.span, "task payload must be i32"));
                        }
                        worker(module, id, budget, i.span)?;
                        Some(Type::Managed(Box::new(Type::TaskI32)))
                    }
                    InstructionKind::TaskYield => {
                        suspend[index] = true;
                        worker(module, f.id, budget, i.span)?;
                        Some(Type::Unit)
                    }
                    InstructionKind::TaskValid { task } => {
                        if !matches!(ty(task)?, Type::Desc(p) if **p == Type::TaskI32) {
                            return Err(fail(
                                i.span,
                                "task validity requires its actual descriptor",
                            ));
                        }
                        Some(Type::Bool)
                    }
                    InstructionKind::TaskPoll { task }
                    | InstructionKind::TaskRequestCancel { task }
                    | InstructionKind::TaskPollClose { task } => {
                        if !ty(task)?.is_exclusive_task_head_address() {
                            return Err(fail(
                                i.span,
                                "task activation requires exclusive actual owning head",
                            ));
                        }
                        Some(match i.kind {
                            InstructionKind::TaskPoll { .. } => {
                                Type::Tuple(vec![Type::I32, Type::I32])
                            }
                            InstructionKind::TaskPollClose { .. } => Type::Bool,
                            _ => Type::Unit,
                        })
                    }
                    InstructionKind::TaskFinishDestroy { task } => {
                        if !matches!(ty(task)?, Type::Managed(p) if **p == Type::TaskI32) {
                            return Err(fail(i.span, "task destruction consumes its actual owner"));
                        }
                        Some(Type::Unit)
                    }
                    _ => None,
                };
                if let Some(expected) = expected {
                    if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&expected) {
                        return Err(fail(
                            i.span,
                            "task instruction result differs from sealed ABI",
                        ));
                    }
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for f in &module.functions {
            for block in &f.blocks {
                for i in &block.instructions {
                    budget.spend(1, "derive actual Suspend call closure")?;
                    if let InstructionKind::Call { function, .. } = i.kind {
                        let callee = suspend
                            .get(function.0 as usize)
                            .ok_or_else(|| fail(i.span, "call target is absent"))?;
                        if *callee && !suspend[f.id.0 as usize] {
                            suspend[f.id.0 as usize] = true;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for f in &module.functions {
        for b in &f.blocks {
            for i in &b.instructions {
                budget.spend(1, "Suspend Direct boundary exclusion")?;
                let direct = match i.kind {
                    InstructionKind::Call { function, .. }
                    | InstructionKind::FunctionRef { function } => Some(function),
                    InstructionKind::WithScopedThread { worker, .. } => Some(worker),
                    _ => None,
                };
                if direct.is_some_and(|id| suspend.get(id.0 as usize).copied().unwrap_or(false)) {
                    return Err(fail(
                        i.span,
                        "Suspend function cannot be called, referenced or started through a Direct ABI",
                    ));
                }
            }
        }
    }
    Ok(())
}

// Task head windows use actual address parent identities. Kind tags, function
// names and copied pointer bytes are not exclusive-poll authority.
pub(super) fn loans(
    module: &TypedModule,
    plan: &CandidatePlan,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<()> {
    use std::collections::BTreeMap;
    type Set = BTreeSet<usize>;
    type State = BTreeMap<LocalId, Set>;
    fn add(to: &mut Set, from: &Set, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(
            to.len().saturating_add(from.len()).saturating_add(1),
            "task loan relation union",
        )?;
        let n = to.len();
        to.extend(from);
        Ok(n != to.len())
    }
    fn copy(s: &State, budget: &mut Budget) -> CompileResult<State> {
        budget.spend(s.len(), "task head state allocation")?;
        for row in s.values() {
            budget.spend(row.len(), "task head state relations")?;
        }
        Ok(s.clone())
    }
    if !budget.tasks {
        return Ok(());
    }
    for (fi, f) in module.functions.iter().enumerate() {
        budget.spend(
            f.values.len().saturating_add(f.locals.len()),
            "task head profile inventory",
        )?;
        if !f.values.iter().any(Type::is_exclusive_task_head_address)
            && !f
                .locals
                .iter()
                .any(|l| l.ty.is_exclusive_task_head_address())
        {
            continue;
        }
        let n = f.values.len();
        let count = n.saturating_add(f.locals.len());
        budget.spend(
            count.saturating_mul(3).saturating_add(f.blocks.len()),
            "task head loan tables",
        )?;
        let mut aliases = vec![Set::new(); n];
        let mut parents = vec![Set::new(); count];
        let mut roots: Vec<Vec<HoldRoot>> = vec![vec![]; count];
        for h in &plan.functions[fi].holds {
            budget.spend(h.roots.len(), "task loan actual root snapshot")?;
            roots[h.value.0 as usize] = h.roots.clone();
        }
        let mut entry = State::new();
        for (index, local) in f.parameters.iter().enumerate() {
            if f.locals[local.0 as usize]
                .ty
                .is_exclusive_task_head_address()
            {
                budget.spend(3, "task head input loan")?;
                entry.insert(*local, Set::from([n + local.0 as usize]));
                roots[n + local.0 as usize] = vec![HoldRoot::Parameter(index as u32)];
            }
        }
        let (pred, reachable) = &graphs[fi];
        let mut outputs = vec![State::new(); f.blocks.len()];
        loop {
            let mut changed = false;
            for (b, block) in f.blocks.iter().enumerate() {
                if !reachable[b] {
                    continue;
                }
                let mut state = if b == f.entry.0 as usize {
                    copy(&entry, budget)?
                } else {
                    State::new()
                };
                for &p in &pred[b] {
                    budget.spend(outputs[p].len(), "task head predecessor state")?;
                    for (local, row) in &outputs[p] {
                        add(state.entry(*local).or_default(), row, budget)?;
                    }
                }
                for i in &block.instructions {
                    budget.spend(1, "task head alias instruction")?;
                    if let Some(output) = i.result
                        && f.values[output.0 as usize].is_exclusive_task_head_address()
                    {
                        let id = output.0 as usize;
                        let row = match i.kind {
                            InstructionKind::BorrowLocalMut { local } if matches!(&f.locals[local.0 as usize].ty, Type::Managed(p) if **p == Type::TaskI32) => {
                                Set::from([id])
                            }
                            InstructionKind::ReborrowAddress {
                                address,
                                mutable: true,
                            } => {
                                let source = &aliases[address.0 as usize];
                                let mut ancestry = Set::new();
                                add(&mut ancestry, source, budget)?;
                                for p in source {
                                    add(&mut ancestry, &parents[*p], budget)?;
                                }
                                changed |= add(&mut parents[id], &ancestry, budget)?;
                                Set::from([id])
                            }
                            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                                let source = state.get(&local);
                                budget.spend(
                                    source.map_or(0, Set::len),
                                    "task head local alias snapshot",
                                )?;
                                source.cloned().unwrap_or_default()
                            }
                            _ => {
                                return Err(fail(
                                    i.span,
                                    "exclusive task head has no actual checked producer",
                                ));
                            }
                        };
                        changed |= add(&mut aliases[id], &row, budget)?;
                    }
                    match i.kind {
                        InstructionKind::Store { local, value }
                            if f.locals[local.0 as usize]
                                .ty
                                .is_exclusive_task_head_address() =>
                        {
                            budget.spend(
                                aliases[value.0 as usize].len().saturating_add(1),
                                "task address local assignment",
                            )?;
                            state.insert(local, aliases[value.0 as usize].clone());
                        }
                        InstructionKind::TakeLocal { local }
                        | InstructionKind::StorageLive(local)
                        | InstructionKind::StorageDead(local) => {
                            state.remove(&local);
                        }
                        _ => {}
                    }
                }
                budget.spend(
                    state.len().saturating_add(outputs[b].len()),
                    "task head state convergence",
                )?;
                if state != outputs[b] {
                    outputs[b] = state;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let afters = resources::address_afters(f, reachable, budget)?;
        for (b, block) in f.blocks.iter().enumerate() {
            if !reachable[b] {
                continue;
            }
            let mut state = if b == f.entry.0 as usize {
                copy(&entry, budget)?
            } else {
                State::new()
            };
            for &p in &pred[b] {
                budget.spend(outputs[p].len(), "task checking predecessor state")?;
                for (local, row) in &outputs[p] {
                    add(state.entry(*local).or_default(), row, budget)?;
                }
            }
            for (position, i) in block.instructions.iter().enumerate() {
                budget.spend(1, "task exclusive activation check")?;
                let (accessed, operand) = match i.kind {
                    InstructionKind::TaskPoll { task }
                    | InstructionKind::TaskRequestCancel { task }
                    | InstructionKind::TaskPollClose { task } => {
                        (Some(roots[task.0 as usize].as_slice()), Some(task))
                    }
                    InstructionKind::TaskValid { task } => {
                        (Some(roots[task.0 as usize].as_slice()), None)
                    }
                    InstructionKind::ReborrowAddress { address, .. }
                        if f.values[address.0 as usize].is_exclusive_task_head_address() =>
                    {
                        (Some(roots[address.0 as usize].as_slice()), Some(address))
                    }
                    _ => (None, None),
                };
                let local = match i.kind {
                    InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::Store { local, .. }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                        if matches!(&f.locals[local.0 as usize].ty, Type::Managed(p) if **p == Type::TaskI32) =>
                    {
                        Some(local)
                    }
                    _ => None,
                };
                let owner_roots = local.map(|l| [HoldRoot::Local(l)]);
                let accessed = accessed.or_else(|| owner_roots.as_ref().map(|r| r.as_slice()));
                if let Some(accessed) = accessed {
                    if accessed.is_empty() {
                        return Err(fail(i.span, "task activation has no actual head root"));
                    }
                    let mut allowed = Set::new();
                    if let Some(operand) = operand {
                        add(&mut allowed, &aliases[operand.0 as usize], budget)?;
                        for loan in &aliases[operand.0 as usize] {
                            add(&mut allowed, &parents[*loan], budget)?;
                        }
                    }
                    for active in &afters[b][position] {
                        budget.spend(1, "task live exclusive head")?;
                        let held = match active {
                            resources::Place::Value(v)
                                if i.result != Some(*v)
                                    && f.values[v.0 as usize].is_exclusive_task_head_address() =>
                            {
                                Some(&aliases[v.0 as usize])
                            }
                            resources::Place::Local(l)
                                if f.locals[l.0 as usize].ty.is_exclusive_task_head_address() =>
                            {
                                state.get(l)
                            }
                            _ => None,
                        };
                        for loan in held.into_iter().flatten() {
                            budget.spend(1, "task actual loan identity")?;
                            if allowed.contains(loan) {
                                continue;
                            }
                            budget.product(
                                accessed.len(),
                                roots[*loan].len(),
                                1,
                                "task head overlap",
                            )?;
                            if accessed.iter().any(|r| roots[*loan].contains(r)) {
                                return Err(fail(
                                    i.span,
                                    "task head access overlaps another live exclusive activation",
                                ));
                            }
                        }
                    }
                }
                match i.kind {
                    InstructionKind::Store { local, value }
                        if f.locals[local.0 as usize]
                            .ty
                            .is_exclusive_task_head_address() =>
                    {
                        budget.spend(
                            aliases[value.0 as usize].len().saturating_add(1),
                            "task checking local assignment",
                        )?;
                        state.insert(local, aliases[value.0 as usize].clone());
                    }
                    InstructionKind::TakeLocal { local }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local) => {
                        state.remove(&local);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
