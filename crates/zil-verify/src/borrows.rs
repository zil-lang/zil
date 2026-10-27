//! Independent finite shared-address verifier. No analyzer implementation is
//! imported. Provenance uses sorted sparse root vectors and only nonempty local
//! rows; liveness is a separate bounded bit set without a provenance dimension.
use super::*;
mod buffer_effects;
mod loans;
mod roots;
use roots::{Root, contains_overlap, field_root, overlaps};
use std::collections::BTreeMap;

type Roots = Vec<Root>;
type State = BTreeMap<LocalId, Roots>;

fn has_sources(module: &TypedModule, ty: &Type) -> bool {
    closures::holds_addresses(module, ty) || byte_buffers::is_desc(ty)
}

fn reserve(budget: &mut Budget, sizes: &[usize]) -> Check<()> {
    let count = sizes
        .iter()
        .try_fold(1usize, |n, x| n.checked_mul(*x))
        .ok_or_else(|| limit("borrow state size overflow"))?;
    budget.spend(count)
}
fn lookup(budget: &mut Budget, size: usize) -> Check<()> {
    budget.spend(size.checked_ilog2().map_or(1, |n| n as usize + 2))
}
fn state_cost(state: &State, budget: &mut Budget) -> Check<()> {
    budget.spend(state.len())?;
    for roots in state.values() {
        budget.spend(roots.len())?;
    }
    Ok(())
}
fn roots_join(target: &mut Roots, source: &[Root], budget: &mut Budget) -> Check<bool> {
    let capacity = target
        .len()
        .checked_add(source.len())
        .ok_or_else(|| limit("Hold union overflow"))?;
    budget.spend(capacity.saturating_add(1))?;
    if source.is_empty() {
        return Ok(false);
    }
    let mut merged = Vec::with_capacity(capacity);
    let (mut left, mut right) = (0, 0);
    while left < target.len() && right < source.len() {
        match target[left].cmp(&source[right]) {
            std::cmp::Ordering::Less => {
                merged.push(target[left]);
                left += 1;
            }
            std::cmp::Ordering::Greater => {
                merged.push(source[right]);
                right += 1;
            }
            std::cmp::Ordering::Equal => {
                merged.push(target[left]);
                left += 1;
                right += 1;
            }
        }
    }
    merged.extend_from_slice(&target[left..]);
    merged.extend_from_slice(&source[right..]);
    let changed = merged.len() != target.len();
    if changed {
        *target = merged;
    }
    Ok(changed)
}
fn bits_join(target: &mut [bool], source: &[bool], budget: &mut Budget) -> Check<bool> {
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
fn local_transfer(
    state: &mut State,
    instruction: &Instruction,
    values: &[Roots],
    budget: &mut Budget,
) -> Check<()> {
    match instruction.kind {
        InstructionKind::Store { local, value } => {
            let source = &values[value.0 as usize];
            lookup(budget, state.len())?;
            budget.spend(source.len())?;
            if source.is_empty() {
                state.remove(&local);
            } else {
                state.insert(local, source.clone());
            }
        }
        InstructionKind::TakeLocal { local }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropClosure { local } => {
            lookup(budget, state.len())?;
            state.remove(&local);
        }
        _ => {}
    }
    Ok(())
}
fn reverse(live: &mut [bool], instruction: &Instruction, value_count: usize) {
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
fn term_use(live: &mut [bool], terminator: &Terminator) {
    match terminator {
        Terminator::Return(Some(value))
        | Terminator::Branch {
            condition: value, ..
        } => live[value.0 as usize] = true,
        _ => {}
    }
}

fn check(
    module: &TypedModule,
    f: &TypedFunction,
    plan: &FunctionPlan,
    graph: &FunctionGraph,
    facts: &Facts,
    budget: &mut Budget,
) -> Check<()> {
    let FunctionGraph {
        successors,
        predecessors,
        reachable,
        ..
    } = graph;
    let values = &facts.values;
    let inputs = &facts.inputs;
    let m = f.locals.len();
    let v = f.values.len();
    let n = f.blocks.len();
    if plan.holds.len() != v {
        return Err(reject(
            "E-VERIFY-HOLD",
            Some(f.span),
            "Hold candidate must describe each ValueId exactly once",
        ));
    }
    budget.spend(v)?;
    // This is only a bound on untrusted claims, never a state-space dimension.
    let captures = f
        .environment
        .map_or(0, |id| module.closures[id.0 as usize].captures.len());
    let max_roots = m
        .checked_add(captures)
        .and_then(|n| n.checked_add(f.parameters.len()))
        .ok_or_else(|| limit("Hold root count overflow"))?;
    for (i, claim) in plan.holds.iter().enumerate() {
        if claim.value.0 as usize != i || claim.roots.len() > max_roots {
            return Err(reject(
                "E-VERIFY-HOLD",
                Some(f.span),
                "noncanonical Hold candidate identity or root count",
            ));
        }
        budget.spend(claim.roots.len())?;
    }
    if !f.values.iter().any(|ty| has_sources(module, ty))
        && !f
            .parameters
            .iter()
            .any(|id| has_sources(module, &f.locals[id.0 as usize].ty))
    {
        if plan.holds.iter().any(|claim| !claim.roots.is_empty()) {
            return Err(reject(
                "E-VERIFY-HOLD",
                Some(f.span),
                "scalar-only function cannot claim address Hold roots",
            ));
        }
        return Ok(());
    }
    for (claim, actual) in plan.holds.iter().zip(values) {
        budget.spend(actual.len().saturating_add(1))?;
        let mut coarse: Vec<_> = actual.iter().map(|root| root.coarse()).collect();
        budget.spend(coarse.len())?;
        coarse.dedup();
        if claim.roots != coarse {
            return Err(reject(
                "E-VERIFY-HOLD",
                Some(f.span),
                "Hold candidate differs from independently reconstructed provenance",
            ));
        }
    }

    let width = v
        .checked_add(m)
        .ok_or_else(|| limit("borrow liveness width overflow"))?;
    reserve(budget, &[n, width, 2])?;
    let mut live_in = vec![vec![false; width]; n];
    let mut live_out = live_in.clone();
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
            for instruction in f.blocks[b].instructions.iter().rev() {
                budget.spend(1 + instruction_operands(&instruction.kind).count())?;
                reverse(&mut live, instruction, v);
            }
            budget.spend(width)?;
            if live != live_in[b] {
                live_in[b] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Explicitly activated roots start dead; legacy raw scalar locals with no
    // StorageLive have whole-function storage. Parameters are live on entry.
    reserve(budget, &[n, m, 2])?;
    let mut initial = vec![2u8; m];
    for block in &f.blocks {
        for instruction in &block.instructions {
            budget.spend(1)?;
            if let InstructionKind::StorageLive(local)
            | InstructionKind::BindPackageCapture { local, .. } = instruction.kind
            {
                initial[local.0 as usize] = 1;
            }
        }
    }
    for local in &f.parameters {
        initial[local.0 as usize] = 2;
    }
    let mut alive_in = vec![vec![0u8; m]; n];
    let mut alive_out = alive_in.clone();
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(m.saturating_add(1))?;
            if !reachable[b] {
                continue;
            }
            let mut state = if b == f.entry.0 as usize {
                initial.clone()
            } else {
                vec![0u8; m]
            };
            if b != f.entry.0 as usize {
                for &p in &predecessors[b] {
                    if reachable[p] {
                        budget.spend(m)?;
                        for (current, incoming) in state.iter_mut().zip(&alive_out[p]) {
                            *current |= *incoming;
                        }
                    }
                }
            }
            alive_in[b].clone_from(&state);
            for instruction in &block.instructions {
                budget.spend(1)?;
                match instruction.kind {
                    InstructionKind::StorageLive(local)
                    | InstructionKind::BindPackageCapture { local, .. } => {
                        state[local.0 as usize] = 2
                    }
                    InstructionKind::StorageDead(local)
                    | InstructionKind::EndPackageCapture { local, .. } => {
                        state[local.0 as usize] = 1
                    }
                    _ => {}
                }
            }
            if state != alive_out[b] {
                alive_out[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (b, block) in f.blocks.iter().enumerate() {
        if !reachable[b] {
            continue;
        }
        reserve(budget, &[block.instructions.len(), width])?;
        let mut after = vec![vec![]; block.instructions.len()];
        let mut live = live_out[b].clone();
        term_use(&mut live, &block.terminator);
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            after[i] = live.clone();
            reverse(&mut live, instruction, v);
        }
        state_cost(&inputs[b], budget)?;
        let mut locals = inputs[b].clone();
        let mut alive = alive_in[b].clone();
        for (i, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            let span = Some(instruction.span);
            for operand in instruction_operands(&instruction.kind) {
                let held = &values[operand.0 as usize];
                budget.spend(held.len())?;
                if held.iter().any(|root| {
                    root.local()
                        .is_some_and(|local| alive[local.0 as usize] != 2)
                }) {
                    return Err(reject(
                        "E-BORROW-DEAD",
                        span,
                        "address or closure uses storage after its lexical lifetime",
                    ));
                }
            }
            if let InstructionKind::Load(local)
            | InstructionKind::TakeLocal { local }
            | InstructionKind::BorrowLocalMut { local }
            | InstructionKind::Store { local, .. }
            | InstructionKind::BorrowLocal { local }
            | InstructionKind::CallClosureMut { callee: local, .. }
            | InstructionKind::CallClosureShared { callee: local, .. } = instruction.kind
                && alive[local.0 as usize] != 2
            {
                return Err(reject(
                    "E-BORROW-DEAD",
                    span,
                    "local storage is not live on every incoming path",
                ));
            }
            if let InstructionKind::CallClosureMut { callee, .. }
            | InstructionKind::CallClosureShared { callee, .. } = instruction.kind
                && let Some(held) = locals.get(&callee)
            {
                budget.spend(held.len())?;
                if held.iter().any(|root| {
                    root.local()
                        .is_some_and(|local| alive[local.0 as usize] != 2)
                }) {
                    return Err(reject(
                        "E-BORROW-DEAD",
                        span,
                        "mutable closure uses captured storage after its lexical lifetime",
                    ));
                }
            }
            let invalidated = match instruction.kind {
                InstructionKind::Store { local, .. }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local)
                | InstructionKind::EndPackageCapture { local, .. }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropPackage { local } => Some(Root::Local(local)),
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index }
                | InstructionKind::StorePackageField { local, index, .. } => {
                    Some(field_root(f, local, index))
                }
                InstructionKind::DropManaged { local }
                    if byte_buffers::is_owned(&f.locals[local.0 as usize].ty) =>
                {
                    Some(Root::Local(local))
                }
                InstructionKind::TakeLocal { local }
                    if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                        || matches!(f.locals[local.0 as usize].ty, Type::Package { .. })
                        || errors::owned(&f.locals[local.0 as usize].ty)
                        || byte_buffers::is_owned(&f.locals[local.0 as usize].ty) =>
                {
                    Some(Root::Local(local))
                }
                InstructionKind::TakeCapture { index }
                    if instruction.result.is_some_and(|value| {
                        byte_buffers::is_owned(&f.values[value.0 as usize])
                    }) =>
                {
                    Some(Root::Capture(index))
                }
                _ => None,
            };
            if let Some(root) = invalidated {
                budget.spend(width)?;
                for (carrier, live) in after[i].iter().enumerate() {
                    if !*live {
                        continue;
                    }
                    let held = if carrier < v {
                        Some(&values[carrier])
                    } else {
                        lookup(budget, locals.len())?;
                        locals.get(&LocalId((carrier - v) as u32))
                    };
                    if let Some(held) = held
                        && contains_overlap(held, root, budget)?
                    {
                        return Err(reject(
                            "E-BORROW-CONFLICT",
                            span,
                            "write or storage change overlaps a shared address still required by a later use",
                        ));
                    }
                }
            }
            match instruction.kind {
                InstructionKind::StorageLive(local)
                | InstructionKind::BindPackageCapture { local, .. } => {
                    if alive[local.0 as usize] != 1 {
                        return Err(reject(
                            "E-BORROW-STORAGE",
                            span,
                            "StorageLive cannot reactivate storage that is already live",
                        ));
                    }
                    alive[local.0 as usize] = 2;
                }
                InstructionKind::StorageDead(local)
                | InstructionKind::EndPackageCapture { local, .. } => alive[local.0 as usize] = 1,
                _ => {}
            }
            local_transfer(&mut locals, instruction, values, budget)?;
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct Facts {
    values: Vec<Roots>,
    inputs: Vec<State>,
}

fn reconstruct(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    summaries: &[Roots],
    budget: &mut Budget,
) -> Check<Facts> {
    let n = f.blocks.len();
    let v = f.values.len();
    let predecessors = &graph.predecessors;
    let reachable = &graph.reachable;
    budget.spend(v)?;
    if !f.values.iter().any(|ty| has_sources(module, ty))
        && !f
            .parameters
            .iter()
            .any(|id| has_sources(module, &f.locals[id.0 as usize].ty))
    {
        return Ok(Facts {
            values: vec![vec![]; v],
            inputs: vec![],
        });
    }
    reserve(budget, &[n, 2])?;
    let mut values = vec![Roots::new(); v];
    let mut inputs = vec![State::new(); n];
    let mut outputs = vec![State::new(); n];
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1)?;
            if !reachable[b] {
                continue;
            }
            let mut state = State::new();
            if b == f.entry.0 as usize {
                budget.spend(f.parameters.len())?;
                for (index, local) in f.parameters.iter().enumerate() {
                    if has_sources(module, &f.locals[local.0 as usize].ty) {
                        state.insert(*local, vec![Root::Parameter(index as u32)]);
                    }
                }
            } else {
                for &p in &predecessors[b] {
                    budget.spend(1)?;
                    if reachable[p] {
                        for (local, incoming) in &outputs[p] {
                            lookup(budget, state.len())?;
                            roots_join(state.entry(*local).or_default(), incoming, budget)?;
                        }
                    }
                }
            }
            state_cost(&state, budget)?;
            inputs[b].clone_from(&state);
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let Some(result) = instruction.result {
                    let mut row = Roots::new();
                    match &instruction.kind {
                        InstructionKind::DescribeLocal { local }
                            if byte_buffers::is_owned(&f.locals[local.0 as usize].ty) =>
                        {
                            row.push(Root::Local(*local));
                        }
                        InstructionKind::DescribePackageField { local, index }
                            if byte_buffers::is_desc(&f.values[result.0 as usize]) =>
                        {
                            // Only checked independent owning byte fields receive
                            // a projection; unknown resources keep the whole root.
                            row.push(field_root(f, *local, *index));
                        }
                        InstructionKind::DescribeCapture { index }
                            if byte_buffers::is_desc(&f.values[result.0 as usize]) =>
                        {
                            row.push(Root::Capture(*index));
                        }
                        InstructionKind::ByteBufferBorrow { buffer, .. } => {
                            roots_join(&mut row, &values[buffer.0 as usize], budget)?;
                        }
                        InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local } => {
                            row.push(Root::Local(*local));
                            if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                                && let Some(captures) = state.get(local)
                            {
                                roots_join(&mut row, captures, budget)?;
                            }
                        }
                        InstructionKind::ReborrowAddress { address, .. }
                        | InstructionKind::AddressField { address, .. }
                        | InstructionKind::AddressSlice { address, .. }
                        | InstructionKind::AddressIndex { address, .. } => {
                            roots_join(&mut row, &values[address.0 as usize], budget)?;
                        }
                        InstructionKind::ReadCapture { index }
                            if matches!(f.values[result.0 as usize], Type::Address { .. }) =>
                        {
                            row.push(Root::Capture(*index))
                        }
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                            lookup(budget, state.len())?;
                            if let Some(source) = state.get(local) {
                                budget.spend(source.len())?;
                                row.clone_from(source);
                            }
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            for root in &summaries[function.0 as usize] {
                                budget.spend(1)?;
                                let Root::Parameter(index) = root else {
                                    return Err(reject(
                                        "E-VERIFY-HOLD",
                                        Some(instruction.span),
                                        "non-parameter call result origin",
                                    ));
                                };
                                let argument = arguments[*index as usize];
                                roots_join(&mut row, &values[argument.0 as usize], budget)?;
                            }
                        }
                        InstructionKind::MakeClosure { captures, .. } => {
                            for capture in captures {
                                roots_join(&mut row, &values[capture.0 as usize], budget)?;
                            }
                        }
                        _ => {}
                    }
                    changed |= roots_join(&mut values[result.0 as usize], &row, budget)?;
                }
                local_transfer(&mut state, instruction, &values, budget)?;
            }
            state_cost(&state, budget)?;
            state_cost(&outputs[b], budget)?;
            if outputs[b] != state {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(Facts { values, inputs })
}

// The verifier reconstructs its own dependency graph and uses a separate
// scheduling strategy. No candidate or analyzer summary controls this graph.
fn reconstruct_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<Facts>> {
    let count = module.functions.len();
    budget.spend(count)?;
    let mut summaries = vec![Roots::new(); count];
    if !module
        .functions
        .iter()
        .any(|f| matches!(f.return_type, Type::Address { .. }))
    {
        return module
            .functions
            .iter()
            .zip(graphs)
            .map(|(f, graph)| reconstruct(module, f, graph, &summaries, budget))
            .collect();
    }
    reserve(budget, &[count, 5])?;
    let mut dependents = vec![Vec::<usize>::new(); count];
    for (user, f) in module.functions.iter().enumerate() {
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1)?;
            if !graphs[user].reachable[b] {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1)?;
                let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                else {
                    continue;
                };
                if !instruction.result.is_some_and(|v| {
                    matches!(f.values.get(v.0 as usize), Some(Type::Address { .. }))
                }) {
                    continue;
                }
                let Some(users) = dependents.get_mut(callee.0 as usize) else {
                    return Err(reject(
                        "E-VERIFY-CALL",
                        Some(instruction.span),
                        "call target is outside the function table",
                    ));
                };
                if users.last() != Some(&user) {
                    budget.spend(1)?;
                    users.push(user);
                }
            }
        }
    }
    let mut all: Vec<Option<Facts>> = (0..count).map(|_| None).collect();
    // Keep this solver independent from the producer's FIFO. Complete a batch
    // before consuming its notifications, so many changed callees cause only
    // one revisit of an already-checked caller in the following batch.
    let mut dirty: Vec<usize> = (0..count).rev().collect();
    let mut next = Vec::new();
    let mut scheduled = vec![true; count];
    while !dirty.is_empty() {
        while let Some(index) = dirty.pop() {
            budget.spend(1)?;
            scheduled[index] = false;
            #[cfg(test)]
            TEST_SOLVES.with(|n| n.set(n.get() + 1));
            let function = &module.functions[index];
            let facts = reconstruct(module, function, &graphs[index], &summaries, budget)?;
            let mut changed = false;
            if matches!(function.return_type, Type::Address { .. }) {
                for (block, reachable) in function.blocks.iter().zip(&graphs[index].reachable) {
                    budget.spend(1)?;
                    if *reachable && let Terminator::Return(Some(value)) = block.terminator {
                        let roots = &facts.values[value.0 as usize];
                        budget.spend(roots.len())?;
                        let incoming: Vec<_> = roots
                            .iter()
                            .copied()
                            .filter(|root| matches!(root, Root::Parameter(_)))
                            .collect();
                        changed |= roots_join(&mut summaries[index], &incoming, budget)?;
                    }
                }
            }
            all[index] = Some(facts);
            if changed {
                budget.spend(dependents[index].len())?;
                for &user in &dependents[index] {
                    if !scheduled[user] {
                        budget.spend(1)?;
                        scheduled[user] = true;
                        next.push(user);
                    }
                }
            }
        }
        std::mem::swap(&mut dirty, &mut next);
    }
    all.into_iter()
        .map(|facts| {
            facts.ok_or_else(|| reject("E-VERIFY-HOLD", None, "return frontier omitted a function"))
        })
        .collect()
}

#[cfg(test)]
thread_local! {
    static TEST_SOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// First check every function structurally, then reconstruct all return origins
/// independently. Summaries contain source parameter positions, never callee
/// stack slots, so substitution cannot extend a local lifetime across a call.
pub(super) fn check_module(
    module: &TypedModule,
    plan: &CandidatePlan,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<()> {
    let facts = reconstruct_module(module, graphs, budget)?;
    budget.spend(module.resources.len())?;
    let has_buffers = module
        .resources
        .iter()
        .any(|r| r.payload == Type::ByteBuffer);
    let effects = if has_buffers {
        buffer_effects::infer(module, graphs, &facts, budget)?
    } else {
        vec![]
    };
    let mut apart = if has_buffers {
        budget.spend(module.functions.len())?;
        Vec::with_capacity(module.functions.len())
    } else {
        Vec::new()
    };
    for (((function, plan), graph), facts) in module
        .functions
        .iter()
        .zip(&plan.functions)
        .zip(graphs)
        .zip(&facts)
    {
        for (block, reachable) in function.blocks.iter().zip(&graph.reachable) {
            if !reachable {
                continue;
            }
            if matches!(function.return_type, Type::Address { .. })
                && let Terminator::Return(Some(value)) = block.terminator
            {
                let roots = &facts.values[value.0 as usize];
                if roots.is_empty() {
                    return Err(reject(
                        "E-BORROW-ORIGIN",
                        Some(function.span),
                        "reference return has no grounded input origin; a recursive cycle is not an empty contract",
                    ));
                }
                if roots.iter().any(|root| !matches!(root, Root::Parameter(_))) {
                    return Err(reject(
                        "E-BORROW-ESCAPE",
                        Some(function.span),
                        "returned reference would outlive its local or closure storage",
                    ));
                }
            }
            for instruction in &block.instructions {
                if let Some(value) = instruction.result
                    && (matches!(function.values[value.0 as usize], Type::Address { .. })
                        || byte_buffers::is_desc(&function.values[value.0 as usize]))
                    && facts.values[value.0 as usize].is_empty()
                {
                    return Err(reject(
                        "E-BORROW-ORIGIN",
                        Some(instruction.span),
                        "address value has no independently grounded origin",
                    ));
                }
            }
        }
        check(module, function, plan, graph, facts, budget)?;
        let required = loans::check(module, function, graph, facts, &effects, budget)?;
        if has_buffers {
            apart.push(required);
        }
    }
    if has_buffers {
        buffer_effects::close_apart(module, graphs, &facts, apart, budget)?;
    }
    Ok(())
}

#[cfg(test)]
mod frontier_tests {
    use super::*;

    fn fixture(edges: &[Option<usize>], grounded: &[bool], swap: bool) -> TypedModule {
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let span = zil_base::Span::new(zil_base::FileId(0), 0, 1);
        let instruction = |result, kind| Instruction {
            result: Some(ValueId(result)),
            kind,
            span,
        };
        let functions = edges
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let mut values = vec![address.clone(), address.clone(), Type::Bool];
                let instructions = vec![
                    instruction(0, InstructionKind::Load(LocalId(0))),
                    instruction(1, InstructionKind::Load(LocalId(1))),
                    instruction(2, InstructionKind::Load(LocalId(2))),
                ];
                let mut blocks = vec![BasicBlock {
                    id: BlockId(0),
                    instructions,
                    terminator: Terminator::Return(Some(ValueId(0))),
                }];
                if let Some(target) = target {
                    values.push(address.clone());
                    let arguments = if swap {
                        vec![ValueId(1), ValueId(0), ValueId(2)]
                    } else {
                        vec![ValueId(0), ValueId(1), ValueId(2)]
                    };
                    let call = instruction(
                        3,
                        InstructionKind::Call {
                            function: FunctionId(*target as u32),
                            arguments,
                        },
                    );
                    if grounded[i] {
                        blocks[0].terminator = Terminator::Branch {
                            condition: ValueId(2),
                            then_block: BlockId(1),
                            else_block: BlockId(2),
                        };
                        blocks.push(BasicBlock {
                            id: BlockId(1),
                            instructions: vec![],
                            terminator: Terminator::Return(Some(ValueId(0))),
                        });
                        blocks.push(BasicBlock {
                            id: BlockId(2),
                            instructions: vec![call],
                            terminator: Terminator::Return(Some(ValueId(3))),
                        });
                    } else {
                        blocks[0].instructions.push(call);
                        blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
                    }
                }
                TypedFunction {
                    callable_return_contract: None,
                    callable_contract: None,
                    result_mode: zil_ir::PassingMode::Infer,
                    id: FunctionId(i as u32),
                    name: format!("f{i}"),
                    foreign: None,
                    environment: None,
                    parameters: vec![LocalId(0), LocalId(1), LocalId(2)],
                    return_type: address.clone(),
                    locals: vec![address.clone(), address.clone(), Type::Bool]
                        .into_iter()
                        .enumerate()
                        .map(|(n, ty)| LocalDecl {
                            id: LocalId(n as u32),
                            name: format!("p{n}"),
                            ty,
                            mutable: false,
                            span,
                        })
                        .collect(),
                    values,
                    blocks,
                    entry: BlockId(0),
                    span,
                }
            })
            .collect();
        TypedModule {
            resources: Vec::new(),
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: zil_base::ModuleId(0),
            source_fingerprint: zil_base::Fingerprint::ZERO,
            target: zil_base::TargetProfile::default(),
            functions,
            closures: vec![],
            unsupported: vec![],
        }
    }
    fn slow(
        module: &TypedModule,
        graphs: &[FunctionGraph],
        budget: &mut Budget,
    ) -> Check<Vec<Facts>> {
        budget.spend(module.functions.len())?;
        let mut summaries = vec![Roots::new(); module.functions.len()];
        let facts = loop {
            let mut changed = false;
            let mut all = Vec::with_capacity(module.functions.len());
            for (index, function) in module.functions.iter().enumerate() {
                TEST_SOLVES.with(|n| n.set(n.get() + 1));
                let facts = reconstruct(module, function, &graphs[index], &summaries, budget)?;
                if matches!(function.return_type, Type::Address { .. }) {
                    for (block, reachable) in function.blocks.iter().zip(&graphs[index].reachable) {
                        budget.spend(1)?;
                        if *reachable && let Terminator::Return(Some(value)) = block.terminator {
                            let roots = &facts.values[value.0 as usize];
                            budget.spend(roots.len())?;
                            let incoming: Vec<_> = roots
                                .iter()
                                .copied()
                                .filter(|root| matches!(root, Root::Parameter(_)))
                                .collect();
                            changed |= roots_join(&mut summaries[index], &incoming, budget)?;
                        }
                    }
                }
                all.push(facts);
            }
            if !changed {
                break all;
            }
        };
        Ok(facts)
    }

    fn graphs(module: &TypedModule) -> Vec<FunctionGraph> {
        module
            .functions
            .iter()
            .map(|f| {
                if f.blocks.len() == 1 {
                    FunctionGraph {
                        predecessors: vec![vec![]],
                        successors: vec![vec![]],
                        reachable: vec![true],
                        package_views: packages::CaptureViews::default(),
                    }
                } else {
                    FunctionGraph {
                        predecessors: vec![vec![], vec![0], vec![0]],
                        successors: vec![vec![1, 2], vec![], vec![]],
                        reachable: vec![true; 3],
                        package_views: packages::CaptureViews::default(),
                    }
                }
            })
            .collect()
    }
    fn compare(module: &TypedModule) -> (usize, usize) {
        let graphs = graphs(module);
        TEST_SOLVES.with(|n| n.set(0));
        let fast =
            reconstruct_module(module, &graphs, &mut Budget::new(&Limits::default())).unwrap();
        let fast_visits = TEST_SOLVES.with(|n| n.get());
        TEST_SOLVES.with(|n| n.set(0));
        let reference = slow(module, &graphs, &mut Budget::new(&Limits::default())).unwrap();
        let slow_visits = TEST_SOLVES.with(|n| n.get());
        assert_eq!(fast, reference);
        (fast_visits, slow_visits)
    }
    #[test]
    fn frontier_facts_equal_full_scan_for_recursion_and_disconnected_components() {
        for seed in 0..24 {
            let count = 13;
            let edges: Vec<_> = (0..count)
                .map(|i| {
                    if (i + seed) % 7 == 0 {
                        None
                    } else {
                        Some((i * 7 + seed) % count)
                    }
                })
                .collect();
            let grounded: Vec<_> = (0..count).map(|i| (i + seed) % 3 == 0).collect();
            compare(&fixture(&edges, &grounded, seed % 2 == 0));
        }
        compare(&fixture(&[Some(1), Some(0)], &[false, false], true));
    }
    #[test]
    fn frontier_solves_reverse_chain_with_linear_revisits() {
        let n = 96;
        let edges: Vec<_> = (0..n).map(|i| (i + 1 < n).then_some(i + 1)).collect();
        let (fast, slow) = compare(&fixture(&edges, &vec![false; n], false));
        assert_eq!(fast, 2 * n - 1);
        assert_eq!(slow, n * (n + 1));
    }
    #[test]
    fn frontier_coalesces_many_leaf_updates_before_revisiting_the_caller() {
        let leaves = 64;
        let mut module = fixture(&vec![None; leaves + 1], &vec![false; leaves + 1], false);
        let caller = &mut module.functions[0];
        for target in 1..=leaves {
            let value = ValueId(caller.values.len() as u32);
            caller.values.push(caller.return_type.clone());
            caller.blocks[0].instructions.push(Instruction {
                result: Some(value),
                kind: InstructionKind::Call {
                    function: FunctionId(target as u32),
                    arguments: vec![ValueId(0), ValueId(1), ValueId(2)],
                },
                span: caller.span,
            });
            caller.blocks[0].terminator = Terminator::Return(Some(value));
        }
        let (fast, slow) = compare(&module);
        assert_eq!(fast, leaves + 2);
        assert_eq!(slow, 3 * (leaves + 1));
    }
    #[test]
    fn frontier_checks_target_and_budget_before_index_use() {
        let module = fixture(&[Some(999)], &[false], false);
        let error = reconstruct_module(
            &module,
            &graphs(&module),
            &mut Budget::new(&Limits::default()),
        )
        .unwrap_err();
        assert_eq!(error.code, "E-VERIFY-CALL");
        let module = fixture(&[None], &[false], false);
        let limits = Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        };
        let error =
            reconstruct_module(&module, &graphs(&module), &mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    }
}
