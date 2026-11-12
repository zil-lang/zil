//! Independent finite shared-address verifier. No analyzer implementation is
//! imported. Provenance uses sorted sparse root vectors and only nonempty local
//! rows; liveness is a separate bounded bit set without a provenance dimension.
use super::*;
mod buffer_effects;
mod captures;
mod liveness;
mod loans;
use liveness::Bits;
mod port_facts;
mod returned;
mod roots;
use roots::{Root, contains_overlap, field_root, overlaps};
use std::collections::BTreeMap;

type Roots = Vec<Root>;
type State = BTreeMap<LocalId, Roots>;

fn tracked_desc(ty: &Type, descriptor_origins: bool) -> bool {
    matches!(ty, Type::Desc(inner)
        if descriptor_origins || matches!(inner.as_ref(), Type::ByteBuffer | Type::Task { .. } | Type::TaskI32 | Type::OwnedStore { .. } | Type::Mutex { .. }))
}
fn retained_task_value(ty: &Type) -> bool {
    if let Some(owner) = tasks::transfer::outcome_owner(ty) {
        return retained_task_value(owner);
    }
    match ty {
        Type::MutexGuard { .. } => true,
        Type::Task { retained_life, .. } => !retained_life.ports.is_empty(),
        Type::Managed(inner) | Type::Desc(inner) => {
            matches!(inner.as_ref(), Type::Task { retained_life, .. } if !retained_life.ports.is_empty())
        }
        Type::ErrorUnion { result, .. } => {
            matches!(result.as_ref(), Type::Managed(inner) if matches!(inner.as_ref(), Type::Task { retained_life, .. } if !retained_life.ports.is_empty()))
        }
        _ => false,
    }
}

fn mixed_task_strong_value(ty: &Type, budget: &mut Budget) -> Check<bool> {
    crate::stored_flow::type_work(ty, &budget.limits.clone(), budget)?;
    Ok(!matches!(ty, Type::Desc(_))
        && (retained_task_value(ty)
            || matches!(ty, Type::Address { .. } | Type::BorrowedStorage { .. })
            || crate::stored_flow::enum_address::signature(ty)))
}

fn has_sources(
    module: &TypedModule,
    ty: &Type,
    descriptor_origins: bool,
    budget: &mut Budget,
) -> Check<bool> {
    if !descriptor_origins {
        return Ok(closures::holds_addresses(module, ty)
            || tracked_desc(ty, false)
            || retained_task_value(ty));
    }
    match ty {
        Type::Address { .. } | Type::Desc(_) | Type::MutexGuard { .. } => Ok(true),
        Type::Managed(_) | Type::ErrorUnion { .. } if retained_task_value(ty) => Ok(true),
        Type::Closure(id) => {
            let captures = &module.closures[id.0 as usize].captures;
            budget.spend(captures.len())?;
            Ok(captures
                .iter()
                .any(|ty| matches!(ty, Type::Address { .. } | Type::Desc(_))))
        }
        _ => Ok(false),
    }
}

fn function_has_sources(
    module: &TypedModule,
    function: &TypedFunction,
    descriptor_origins: bool,
    budget: &mut Budget,
) -> Check<bool> {
    if budget.masked_stored_tasks() && super::stored_flow::qualified_function(function, budget)? {
        return Ok(true);
    }
    if descriptor_origins {
        budget.spend(
            function
                .values
                .len()
                .saturating_add(function.parameters.len()),
        )?;
    }
    for ty in function.values.iter().chain(
        function
            .parameters
            .iter()
            .map(|id| &function.locals[id.0 as usize].ty),
    ) {
        if has_sources(module, ty, descriptor_origins, budget)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn descriptor_origins(module: &TypedModule, budget: &mut Budget) -> Check<bool> {
    // A description's actual permissions do not depend on unrelated head
    // instructions elsewhere in the module.
    budget.spend(1)?;
    Ok(!module.resources.is_empty())
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
fn local_transfer(
    f: &TypedFunction,
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
        InstructionKind::UnpackVariant {
            local,
            variant,
            ref fields,
        } if tasks::transfer::outcome_owner(&f.locals[local.0 as usize].ty).is_some() => {
            budget.spend(
                fields
                    .len()
                    .saturating_add(state.get(&local).map_or(0, |s| s.len())),
            )?;
            let source = state.remove(&local).unwrap_or_default();
            for target in fields {
                state.remove(target);
            }
            if variant == 0 {
                let [target] = fields.as_slice() else {
                    return Err(reject(
                        "E-VERIFY-TASK-PORT",
                        Some(instruction.span),
                        "Task failure transports one owning field",
                    ));
                };
                if !source.is_empty() {
                    state.insert(*target, source);
                }
            }
        }
        InstructionKind::TakeLocal { local }
        | InstructionKind::TakeVariantField { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropClosure { local } => {
            lookup(budget, state.len())?;
            state.remove(&local);
        }
        InstructionKind::DropManaged { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropEnum { local }
            if budget.borrowed_task_ports =>
        {
            lookup(budget, state.len())?;
            state.remove(&local);
        }
        _ => {}
    }
    Ok(())
}
fn replacement_schedule(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<
    Option<(
        crate::place_replacements::Inventory,
        crate::place_replacements::Schedule,
    )>,
> {
    if !budget.replacements || f.foreign.is_some() {
        return Ok(None);
    }
    let inventory = crate::place_replacements::Inventory::reconstruct(module, f, budget)?;
    let schedule = inventory.schedule(f, graph, budget)?;
    Ok(Some((inventory, schedule)))
}

fn check(
    module: &TypedModule,
    f: &TypedFunction,
    plan: &FunctionPlan,
    graph: &FunctionGraph,
    facts: &Facts,
    descriptor_origins: bool,
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
    let replacements = replacement_schedule(module, f, graph, budget)?;
    let schedule = replacements.as_ref().map(|(_, s)| s);
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
        .and_then(|n| n.checked_add(1))
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
    if !function_has_sources(module, f, descriptor_origins, budget)? {
        if plan.holds.iter().any(|claim| !claim.roots.is_empty()) {
            return Err(reject(
                "E-VERIFY-HOLD",
                Some(f.span),
                "scalar-only function cannot claim address Hold roots",
            ));
        }
        return Ok(());
    }
    if budget.returned_environment_sources {
        for (index, block) in f.blocks.iter().enumerate() {
            if !graph.reachable[index] {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::Call { function, .. } = instruction.kind
                    && let Some(result) = instruction.result
                    && let Some(contract) =
                        &module.functions[function.0 as usize].callable_return_contract
                    && (!contract.hold.is_empty() || !contract.call.is_empty())
                    && values[result.0 as usize].is_empty()
                {
                    return Err(reject(
                        "E-VERIFY-CALLABLE-SOURCE",
                        Some(instruction.span),
                        "returned borrowed environment has no grounded actual input roots",
                    ));
                }
            }
        }
    }
    for (claim, actual) in plan.holds.iter().zip(values) {
        budget.spend(actual.len().saturating_add(1))?;
        let mut coarse: Vec<_> = actual.iter().map(|root| root.coarse()).collect();
        budget.spend(coarse.len())?;
        coarse.dedup();
        if budget.task_sources() {
            let ty = &f.values[claim.value.0 as usize];
            let mixed_strong = budget.masked_stored_tasks() && mixed_task_strong_value(ty, budget)?;
            let function_proof = budget
                .task_stored_sources
                .as_ref()
                .and_then(|rows| rows.get(f.id.0 as usize))
                .ok_or_else(|| {
                    reject(
                        "E-VERIFY-TASK-SOURCES",
                        Some(f.span),
                        "canonical Hold lacks an actual stored SSA proof",
                    )
                })?;
            let checked = function_proof
                .values
                .get(claim.value.0 as usize)
                .ok_or_else(|| {
                    reject(
                        "E-VERIFY-TASK-SOURCES",
                        Some(f.span),
                        "canonical Hold lacks an actual stored SSA proof",
                    )
                })?;
            let overlay = !budget.masked_stored_tasks()
                || function_proof.covered
                    && mixed_strong
                    && (!matches!(ty, Type::Address { .. }) || !checked.pointer.is_empty());
            let count = if overlay { checked.public.len() } else { 0 };
            budget.spend(count.saturating_add(usize::from(budget.masked_stored_tasks())))?;
            if overlay {
                coarse = budget.task_stored_sources.as_ref().unwrap()[f.id.0 as usize].values
                    [claim.value.0 as usize]
                    .public
                    .clone();
            } else if budget.masked_stored_tasks() {
                budget.spend(
                    coarse
                        .len()
                        .saturating_mul(coarse.len().checked_ilog2().map_or(1, |n| n as usize + 1)),
                )?;
                coarse.sort_unstable();
                coarse.dedup();
            }
        }
        if claim.roots != coarse {
            return Err(reject(
                "E-VERIFY-HOLD",
                Some(f.span),
                format!(
                    "Hold candidate differs from independently reconstructed provenance at value {}: claimed {:?}, actual {:?}",
                    claim.value.0, claim.roots, coarse
                ),
            ));
        }
    }

    let width = v
        .checked_add(m)
        .ok_or_else(|| limit("borrow liveness width overflow"))?;
    let units = Bits::word_count(width);
    budget.spend(2)?; // Both outer row containers, including logical width zero.
    reserve(budget, &[n, units + 1, 2])?;
    let mut live_in: Vec<_> = (0..n).map(|_| Bits::new(width)).collect();
    let mut live_out = live_in.clone();
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            budget.spend(1)?;
            if !reachable[b] {
                continue;
            }
            budget.spend(units + 1)?;
            let mut live = Bits::new(width);
            for &next in &successors[b] {
                live.join(&live_in[next], budget)?;
            }
            budget.spend(live.work())?;
            live_out[b].clone_from(&live);
            live.term_use(&f.blocks[b].terminator)?;
            for (position, instruction) in f.blocks[b].instructions.iter().enumerate().rev() {
                budget.spend(1 + instruction_operands(&instruction.kind).count())?;
                live.reverse(instruction, v)?;
                live.retain(schedule, b, position, budget)?;
            }
            budget.spend(live.work())?;
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
        // Placeholder records and full snapshots are both prepaid.
        budget.spend(1)?;
        reserve(budget, &[block.instructions.len(), units + 2])?;
        let mut after = vec![Bits::default(); block.instructions.len()];
        budget.spend(live_out[b].work())?;
        let mut live = live_out[b].clone();
        live.term_use(&block.terminator)?;
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            budget.spend(1 + instruction_operands(&instruction.kind).count())?;
            after[i] = live.clone();
            live.reverse(instruction, v)?;
            live.retain(schedule, b, i, budget)?;
        }
        state_cost(&inputs[b], budget)?;
        let mut locals = inputs[b].clone();
        let mut alive = alive_in[b].clone();
        for (i, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            let span = Some(instruction.span);
            let hidden = schedule.map_or(&[][..], |s| s.retained[b][i].as_slice());
            budget.spend(hidden.len())?;
            for operand in instruction_operands(&instruction.kind).chain(hidden.iter().copied()) {
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
            | InstructionKind::BorrowDynOwned { local, .. }
            | InstructionKind::BorrowOwnedArrayElement { local, .. }
            | InstructionKind::BorrowPackageField { local, .. }
            | InstructionKind::BorrowVariantLocal { local, .. }
            | InstructionKind::BorrowFieldLocal { local, .. }
            | InstructionKind::BorrowPlaceLocal { local, .. }
            | InstructionKind::PlaceTagLocal { local, .. }
            | InstructionKind::TakeTupleField { local, .. }
            | InstructionKind::TakeVariantField { local, .. }
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
                | InstructionKind::DropEnum { local }
                | InstructionKind::UnpackVariant { local, .. }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local)
                | InstructionKind::EndPackageCapture { local, .. }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropDynOwned { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::TakeOwnedArrayElement { local, .. }
                | InstructionKind::DropOwnedArrayElement { local, .. }
                | InstructionKind::StoreOwnedArrayElement { local, .. } => Some(Root::Local(local)),
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index }
                | InstructionKind::StorePackageField { local, index, .. } => {
                    Some(field_root(f, local, index, budget)?)
                }
                InstructionKind::DropManaged { local }
                    if matches!(f.locals[local.0 as usize].ty, Type::Managed(_)) =>
                {
                    Some(Root::Local(local))
                }
                InstructionKind::TakeLocal { local }
                    if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                        || matches!(f.locals[local.0 as usize].ty, Type::OwnedDyn { .. })
                        || matches!(f.locals[local.0 as usize].ty, Type::Package { .. })
                        || errors::owned(&f.locals[local.0 as usize].ty, budget.sums)
                        || resources::arrays::is_owned_type(&f.locals[local.0 as usize].ty)
                        || matches!(f.locals[local.0 as usize].ty, Type::Managed(_)) =>
                {
                    Some(Root::Local(local))
                }
                InstructionKind::TakeCapture { index }
                    if instruction.result.is_some_and(|value| {
                        byte_buffers::is_owned(&f.values[value.0 as usize])
                            || matches!(&f.values[value.0 as usize], Type::Managed(payload)
                                if matches!(payload.as_ref(), Type::OwnedStore { .. }))
                            || matches!(
                                f.values[value.0 as usize],
                                Type::Closure(_) | Type::OwnedDyn { .. }
                            )
                            || (budget.pod_stores
                                && matches!(
                                    f.values[value.0 as usize],
                                    Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                                ))
                    }) =>
                {
                    Some(Root::Capture(index))
                }
                _ => None,
            };
            if let Some(root) = invalidated {
                let mut live = after[i].paid_ones(budget)?;
                while let Some(carrier) = live.next(budget)? {
                    let ty = if carrier < v {
                        &f.values[carrier]
                    } else {
                        &f.locals[carrier - v].ty
                    };
                    // A storage Desc may carry derivation metadata, but it is
                    // not a strong address qualification. Resource checking
                    // separately validates its actual field instance and use.
                    if matches!(ty, Type::Desc(_)) {
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
            local_transfer(f, &mut locals, instruction, values, budget)?;
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct Facts {
    values: Vec<Roots>,
    inputs: Vec<State>,
    product_captures: bool,
}
fn empty_facts(f: &TypedFunction, budget: &mut Budget) -> Check<Facts> {
    budget.spend(f.values.len())?;
    Ok(Facts {
        values: vec![vec![]; f.values.len()],
        inputs: Vec::new(),
        product_captures: false,
    })
}

fn reconstruct(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    summaries: &[Roots],
    descriptor_origins: bool,
    budget: &mut Budget,
) -> Check<Facts> {
    let n = f.blocks.len();
    let v = f.values.len();
    let predecessors = &graph.predecessors;
    let reachable = &graph.reachable;
    budget.spend(v)?;
    if !budget.task_sources() && !function_has_sources(module, f, descriptor_origins, budget)? {
        return Ok(Facts {
            values: vec![vec![]; v],
            inputs: vec![],
            product_captures: false,
        });
    }
    reserve(budget, &[n, 2])?;
    let mut values = vec![Roots::new(); v];
    let mut inputs = vec![State::new(); n];
    let mut outputs = vec![State::new(); n];
    let mut product_captures = false;
    let capture_directory = if budget.owning_address_captures || budget.returned_environment_sources
    {
        budget.spend(f.values.len().saturating_add(f.locals.len()))?;
        f.values
            .iter()
            .chain(f.locals.iter().map(|local| &local.ty))
            .any(|ty| helpers::parameter_type(ty).is_some())
    } else {
        false
    };
    loop {
        let mut changed = false;
        let capture_facts = if capture_directory {
            Some(captures::reconstruct_roots(
                module, f, graph, &values, budget,
            )?)
        } else {
            None
        };
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1)?;
            if !reachable[b] {
                continue;
            }
            let mut state = State::new();
            if b == f.entry.0 as usize {
                budget.spend(f.parameters.len())?;
                for (index, local) in f.parameters.iter().enumerate() {
                    if has_sources(
                        module,
                        &f.locals[local.0 as usize].ty,
                        descriptor_origins,
                        budget,
                    )? {
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
            let mut capture_state = if let Some(facts) = &capture_facts {
                captures::input_state(facts, b, budget)?
            } else {
                captures::State::new()
            };
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let Some(result) = instruction.result {
                    let mut row = Roots::new();
                    if let Some((definition, arguments)) =
                        borrowed_fp::invocation_call(module, f, &instruction.kind)
                    {
                        for root in &summaries[definition.function.0 as usize] {
                            budget.spend(1)?;
                            if *root == Root::Static {
                                roots_join(&mut row, &[Root::Static], budget)?;
                                continue;
                            }
                            let Root::Parameter(index) = root else {
                                return Err(reject(
                                    "E-VERIFY-HOLD",
                                    Some(instruction.span),
                                    "invocation callback summary has non-input source",
                                ));
                            };
                            let actual = *arguments.get(*index as usize).ok_or_else(|| {
                                reject(
                                    "E-VERIFY-HOLD",
                                    Some(instruction.span),
                                    "invocation callback input absent",
                                )
                            })?;
                            roots_join(&mut row, &values[actual.0 as usize], budget)?;
                        }
                    } else {
                        match &instruction.kind {
                            InstructionKind::StaticUtf8 { .. } => {
                                budget.spend(1)?;
                                row.push(Root::Static);
                            }
                            InstructionKind::WithScopedThread { argument, .. }
                                if retained_task_value(&f.values[result.0 as usize]) =>
                            {
                                roots_join(&mut row, &values[argument.0 as usize], budget)?;
                            }
                            InstructionKind::TaskCreate {
                                arguments,
                                port_bindings,
                                ..
                            }
                            | InstructionKind::TaskCreateOwned {
                                arguments,
                                port_bindings,
                                ..
                            } if retained_task_value(&f.values[result.0 as usize]) => {
                                for binding in port_bindings {
                                    let zil_ir::StorageBase::Parameter(index) = binding.source.base
                                    else {
                                        return Err(reject(
                                            "E-VERIFY-TASK",
                                            Some(instruction.span),
                                            "task retained port has no input",
                                        ));
                                    };
                                    let source =
                                        arguments.get(index as usize).ok_or_else(|| {
                                            reject(
                                                "E-VERIFY-TASK",
                                                Some(instruction.span),
                                                "task retained port input is absent",
                                            )
                                        })?;
                                    roots_join(&mut row, &values[source.0 as usize], budget)?;
                                }
                            }
                            InstructionKind::MutexLock { mutex } => {
                                roots_join(&mut row, &values[mutex.0 as usize], budget)?;
                            }
                            InstructionKind::TakeErrorUnion {
                                value,
                                is_error: false,
                            } if retained_task_value(&f.values[result.0 as usize]) => {
                                roots_join(&mut row, &values[value.0 as usize], budget)?;
                            }
                            InstructionKind::TakeVariantField { local, path }
                                if retained_task_value(&f.locals[local.0 as usize].ty)
                                    && path.as_slice()
                                        == [zil_ir::PlaceProjection::ErrorArm(false)]
                                    && retained_task_value(&f.values[result.0 as usize]) =>
                            {
                                lookup(budget, state.len())?;
                                if let Some(source) = state.get(local) {
                                    roots_join(&mut row, source, budget)?;
                                }
                            }
                            InstructionKind::MakeErrorUnion { value, .. }
                                if retained_task_value(&f.values[result.0 as usize]) =>
                            {
                                roots_join(&mut row, &values[value.0 as usize], budget)?;
                            }
                            InstructionKind::DescribeLocal { local }
                                if matches!(&f.locals[local.0 as usize].ty, Type::Managed(inner)
                                if descriptor_origins || matches!(inner.as_ref(), Type::ByteBuffer | Type::Task { .. } | Type::TaskI32 | Type::OwnedStore { .. })) =>
                            {
                                row.push(Root::Local(*local));
                            }
                            InstructionKind::DescribePackageField { local, index }
                                if tracked_desc(
                                    &f.values[result.0 as usize],
                                    descriptor_origins,
                                ) =>
                            {
                                // Only checked independent owning byte fields receive
                                // a projection; unknown resources keep the whole root.
                                row.push(field_root(f, *local, *index, budget)?);
                            }
                            InstructionKind::ProjectOwnedArrayField { local, .. }
                                if tracked_desc(
                                    &f.values[result.0 as usize],
                                    descriptor_origins,
                                ) =>
                            {
                                // Reconstruct from the actual local, independently
                                // of the candidate Hold rows. This deliberately
                                // does not assert distinct element loans.
                                row.push(Root::Local(*local));
                            }
                            InstructionKind::DescribeCapture { index }
                                if tracked_desc(
                                    &f.values[result.0 as usize],
                                    descriptor_origins,
                                ) =>
                            {
                                row.push(Root::Capture(*index));
                            }
                            InstructionKind::ByteBufferBorrow { buffer, .. }
                            | InstructionKind::MutexValue { guard: buffer } => {
                                roots_join(&mut row, &values[buffer.0 as usize], budget)?;
                            }
                            InstructionKind::OwnedStoreBorrow { store, .. }
                            | InstructionKind::OwnedStoreBorrowMut { store, .. } => {
                                let source = &values[store.0 as usize];
                                if budget.store_descriptions()
                                    || source.iter().any(|root| matches!(root, Root::Window(..)))
                                {
                                    let precise =
                                        roots::store_element_roots(f, result, source, budget)?;
                                    roots_join(&mut row, &precise, budget)?;
                                } else {
                                    roots_join(&mut row, source, budget)?;
                                }
                            }
                            InstructionKind::BorrowOwnedArrayElement { local, .. } => {
                                row.push(Root::Local(*local));
                            }
                            InstructionKind::BorrowPlaceLocal { local, path, .. } => {
                                let root = if let (
                                    Type::Package { .. },
                                    [zil_ir::PlaceProjection::Field(index)],
                                ) = (&f.locals[local.0 as usize].ty, path.as_slice())
                                {
                                    field_root(f, *local, *index, budget)?
                                } else {
                                    roots::mixed_root(f, *local, path, false, budget)?
                                };
                                row.push(root);
                            }
                            InstructionKind::BorrowFieldLocal { local, path, .. } => {
                                row.push(roots::inline_field_root(f, *local, path, budget)?);
                            }
                            InstructionKind::BorrowPlaceAddress { address, path, .. } => {
                                row = roots::mixed_address_roots(
                                    f,
                                    *address,
                                    path,
                                    false,
                                    &values[address.0 as usize],
                                    budget,
                                )?;
                            }
                            InstructionKind::BorrowVariantAddress { address, path, .. } => {
                                row = roots::address_roots(
                                    f,
                                    *address,
                                    path,
                                    &values[address.0 as usize],
                                    budget,
                                )?;
                            }
                            InstructionKind::BorrowVariantLocal { local, path, .. } => {
                                budget.spend(path.len().saturating_add(1))?;
                                row.push(roots::variant_root(f, *local, path, budget)?);
                            }
                            InstructionKind::BorrowPackageField { local, index, .. } => {
                                row.push(field_root(f, *local, *index, budget)?);
                            }
                            InstructionKind::BorrowDynOwned { local, .. }
                                if matches!(
                                    f.locals[local.0 as usize].ty,
                                    Type::Address { .. }
                                ) =>
                            {
                                if let Some(actual) = state.get(local) {
                                    roots_join(&mut row, actual, budget)?;
                                }
                            }
                            InstructionKind::BorrowLocal { local }
                            | InstructionKind::BorrowLocalMut { local }
                            | InstructionKind::BorrowDynOwned { local, .. } => {
                                row.push(Root::Local(*local));
                                if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                                    && let Some(captures) = state.get(local)
                                {
                                    roots_join(&mut row, captures, budget)?;
                                }
                            }
                            InstructionKind::AddressField { address, index } => {
                                let precise = roots::inline_address_field(
                                    f,
                                    *address,
                                    *index,
                                    &values[address.0 as usize],
                                    budget,
                                )?;
                                roots_join(&mut row, &precise, budget)?;
                            }
                            InstructionKind::CallDynSlot {
                                callee: address, ..
                            } if matches!(f.values[result.0 as usize], Type::Address { .. }) => {
                                roots_join(&mut row, &values[address.0 as usize], budget)?;
                            }
                            InstructionKind::DescribeAddress { address }
                            | InstructionKind::ReborrowAddress { address, .. }
                            | InstructionKind::MakeDynBorrow {
                                source: address, ..
                            }
                            | InstructionKind::AddressSlice { address, .. }
                            | InstructionKind::AddressIndex { address, .. } => {
                                roots_join(&mut row, &values[address.0 as usize], budget)?;
                            }
                            InstructionKind::BorrowCapture { index, .. }
                            | InstructionKind::ReadCapture { index }
                                if matches!(f.values[result.0 as usize], Type::Address { .. })
                                    || tracked_desc(
                                        &f.values[result.0 as usize],
                                        descriptor_origins,
                                    ) =>
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
                            InstructionKind::CallClosureShared { callee, .. }
                            | InstructionKind::CallClosureMut { callee, .. }
                                if matches!(f.values[result.0 as usize], Type::Address { .. }) =>
                            {
                                if capture_facts.is_some() {
                                    let d = captures::lending_entry(module, f, &instruction.kind)
                                        .ok_or_else(|| {
                                        reject(
                                            "E-VERIFY-LENDING",
                                            Some(instruction.span),
                                            "lending closure missing",
                                        )
                                    })?;
                                    lookup(budget, capture_state.len())?;
                                    row = captures::lending_result(
                                        d,
                                        &summaries[d.function.0 as usize],
                                        capture_state.get(callee),
                                        &[Root::Local(*callee)],
                                        budget,
                                    )?;
                                } else {
                                    row.push(Root::Local(*callee));
                                }
                            }
                            InstructionKind::CallClosureBorrowed { callee, .. }
                                if matches!(f.values[result.0 as usize], Type::Address { .. }) =>
                            {
                                if let Some(capture_facts) = &capture_facts {
                                    let d = captures::lending_entry(module, f, &instruction.kind)
                                        .ok_or_else(|| {
                                        reject(
                                            "E-VERIFY-LENDING",
                                            Some(instruction.span),
                                            "borrowed lending closure missing",
                                        )
                                    })?;
                                    row = captures::lending_result(
                                        d,
                                        &summaries[d.function.0 as usize],
                                        Some(&capture_facts.values[callee.0 as usize]),
                                        &values[callee.0 as usize],
                                        budget,
                                    )?;
                                } else {
                                    roots_join(&mut row, &values[callee.0 as usize], budget)?;
                                }
                            }
                            InstructionKind::CallIndirect { callee, arguments } => {
                                for index in
                                    borrowed_fp::inputs(&f.values[callee.0 as usize], budget)?
                                {
                                    let argument = arguments[index];
                                    roots_join(&mut row, &values[argument.0 as usize], budget)?;
                                }
                            }
                            InstructionKind::Call {
                                function,
                                arguments,
                            } => {
                                if matches!(f.values[result.0 as usize], Type::Closure(_)) {
                                    row = returned::roots(
                                        module,
                                        f,
                                        *function,
                                        arguments,
                                        &values,
                                        capture_facts.as_ref().map(|facts| facts.values.as_slice()),
                                        budget,
                                    )?;
                                }
                                for root in &summaries[function.0 as usize] {
                                    budget.spend(1)?;
                                    if *root == Root::Static {
                                        roots_join(&mut row, &[Root::Static], budget)?;
                                        continue;
                                    }
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
                                    budget.spend(1)?;
                                    product_captures |=
                                        f.values[capture.0 as usize].is_package_address();
                                    if !descriptor_origins
                                        && let Type::Closure(id) = f.values[capture.0 as usize]
                                    {
                                        budget
                                            .spend(module.closures[id.0 as usize].captures.len())?;
                                    }
                                    if descriptor_origins
                                        || closures::holds_addresses(
                                            module,
                                            &f.values[capture.0 as usize],
                                        )
                                    {
                                        roots_join(&mut row, &values[capture.0 as usize], budget)?;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    changed |= roots_join(&mut values[result.0 as usize], &row, budget)?;
                }
                local_transfer(f, &mut state, instruction, &values, budget)?;
                if let Some(capture_facts) = &capture_facts {
                    captures::local_transfer(
                        &mut capture_state,
                        instruction,
                        &capture_facts.values,
                        budget,
                    )?;
                }
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
    Ok(Facts {
        values,
        inputs,
        product_captures,
    })
}

// The verifier reconstructs its own dependency graph and uses a separate
// scheduling strategy. No candidate or analyzer summary controls this graph.
fn reconstruct_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    descriptor_origins: bool,
    mask: Option<&[bool]>,
    budget: &mut Budget,
) -> Check<Vec<Facts>> {
    let count = module.functions.len();
    budget.spend(count)?;
    let mut summaries = vec![Roots::new(); count];
    if !module.functions.iter().any(|f| {
        matches!(f.return_type, Type::Address { .. }) || retained_task_value(&f.return_type)
    }) {
        return module
            .functions
            .iter()
            .zip(graphs)
            .enumerate()
            .map(|(index, (f, graph))| {
                if mask.is_some_and(|mask| mask[index]) {
                    empty_facts(f, budget)
                } else {
                    reconstruct(module, f, graph, &summaries, descriptor_origins, budget)
                }
            })
            .collect();
    }
    reserve(budget, &[count, 5])?;
    let mut dependents = vec![Vec::<usize>::new(); count];
    for (user, f) in module.functions.iter().enumerate() {
        if mask.is_some_and(|mask| mask[user]) {
            continue;
        }
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1)?;
            if !graphs[user].reachable[b] {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1)?;
                let callee = match instruction.kind {
                    InstructionKind::Call { function, .. } => Some(function),
                    _ => borrowed_fp::invocation_call(module, f, &instruction.kind)
                        .map(|(d, _)| d.function)
                        .or_else(|| {
                            budget
                                .owning_address_captures
                                .then(|| captures::lending_entry(module, f, &instruction.kind))
                                .flatten()
                                .map(|d| d.function)
                        }),
                };
                let Some(callee) = callee else {
                    continue;
                };
                if !instruction.result.is_some_and(|v| {
                    matches!(f.values.get(v.0 as usize), Some(Type::Address { .. }))
                        || f.values.get(v.0 as usize).is_some_and(retained_task_value)
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
    let mut dirty: Vec<usize> = (0..count)
        .rev()
        .filter(|index| !mask.is_some_and(|mask| mask[*index]))
        .collect();
    let mut next = Vec::new();
    let mut scheduled: Vec<bool> = (0..count)
        .map(|index| !mask.is_some_and(|mask| mask[index]))
        .collect();
    while !dirty.is_empty() {
        while let Some(index) = dirty.pop() {
            budget.spend(1)?;
            scheduled[index] = false;
            #[cfg(test)]
            TEST_SOLVES.with(|n| n.set(n.get() + 1));
            let function = &module.functions[index];
            let facts = reconstruct(
                module,
                function,
                &graphs[index],
                &summaries,
                descriptor_origins,
                budget,
            )?;
            let mut changed = false;
            if matches!(function.return_type, Type::Address { .. })
                || retained_task_value(&function.return_type)
            {
                for (block, reachable) in function.blocks.iter().zip(&graphs[index].reachable) {
                    budget.spend(1)?;
                    if *reachable && let Terminator::Return(Some(value)) = block.terminator {
                        let roots = &facts.values[value.0 as usize];
                        if matches!(function.return_type, Type::MutexGuard { .. })
                            && (roots.is_empty() || roots.iter().any(|root| {
                                let HoldRoot::Parameter(index) = root.coarse() else { return true };
                                let Some(local) = function.parameters.get(index as usize) else { return true };
                                !matches!(&function.locals[local.0 as usize].ty,
                                    Type::Desc(inner) if matches!(inner.as_ref(), Type::Mutex { .. }))
                                    && !matches!(&function.locals[local.0 as usize].ty, Type::MutexGuard { .. })
                            }))
                        {
                            return Err(reject("E-VERIFY-MUTEX", Some(function.span), "guard return has no actual mutex head input"));
                        }
                        budget.spend(roots.len())?;
                        let incoming: Vec<_> = roots
                            .iter()
                            .filter_map(|root| match root.coarse() {
                                HoldRoot::Static => Some(Root::Static),
                                HoldRoot::Parameter(index) => Some(Root::Parameter(index)),
                                HoldRoot::Capture(_) if budget.owning_address_captures => {
                                    Some(*root)
                                }
                                _ => None,
                            })
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
        .enumerate()
        .map(|(index, facts)| {
            if mask.is_some_and(|mask| mask[index]) {
                return empty_facts(&module.functions[index], budget);
            }
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
) -> Check<Vec<BTreeSet<u32>>> {
    check_module_mask(module, plan, graphs, None, budget)
}
pub(super) fn check_mixed(
    module: &TypedModule,
    plan: &CandidatePlan,
    graphs: &[FunctionGraph],
    mask: &[bool],
    budget: &mut Budget,
) -> Check<Vec<BTreeSet<u32>>> {
    check_module_mask(module, plan, graphs, Some(mask), budget)
}
fn overlay_store_option_sources(
    module: &TypedModule,
    ordinary: &mut [Facts],
    checked: &[crate::stored_flow::StoreOptionFunctionSources],
    budget: &mut Budget,
) -> Check<()> {
    if checked.len() != module.functions.len() || ordinary.len() != module.functions.len() {
        return Err(reject(
            "E-VERIFY-STORE-WINDOW",
            None,
            "Option source proof omitted an actual function",
        ));
    }
    budget.spend(checked.len())?;
    for ((function, ordinary), stored) in module.functions.iter().zip(ordinary).zip(checked) {
        if stored.values.len() != function.values.len()
            || ordinary.values.len() != function.values.len()
        {
            return Err(reject(
                "E-VERIFY-STORE-WINDOW",
                Some(function.span),
                "Option source proof omitted an actual SSA value",
            ));
        }
        for (index, (ty, sources)) in function.values.iter().zip(&stored.values).enumerate() {
            let qualified =
                crate::stored_flow::option_address::shape(ty, &budget.limits.clone(), budget)?;
            budget.spend(sources.len())?;
            let mut window = false;
            for source in sources {
                window |= source.heap_element.is_some();
                if let (HoldRoot::Parameter(position), [zil_ir::PlaceProjection::Variant(selector)]) =
                    (source.base, source.leaf.as_slice())
                    && selector.variant == 1
                    && selector.index == 0
                {
                    let input = function
                        .parameters
                        .get(position as usize)
                        .and_then(|local| function.locals.get(local.0 as usize))
                        .ok_or_else(|| {
                            reject(
                                "E-VERIFY-STORE-WINDOW",
                                Some(function.span),
                                "Option leaf source input is absent",
                            )
                        })?;
                    let input = match &input.ty {
                        Type::Address { inner, .. } => inner.as_ref(),
                        ty => ty,
                    };
                    window |= crate::stored_flow::option_address::shape(
                        input,
                        &budget.limits.clone(),
                        budget,
                    )?;
                }
            }
            if !qualified && !window {
                continue;
            }
            if !qualified
                && !matches!(ty, Type::Address { mutable: false, .. })
                && !(budget.shared_byte_borrow
                    && matches!(ty, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer))
                && !(budget.store_descriptions()
                    && matches!(ty, Type::Address { .. } | Type::Desc(_)))
            {
                return Err(reject(
                    "E-VERIFY-STORE-WINDOW",
                    Some(function.span),
                    "Store window authority escaped into a non-address value",
                ));
            }
            let mut roots = Vec::new();
            budget.spend(sources.len())?;
            for source in sources {
                let byte_payload = if stored.byte_payloads.is_empty() {
                    false
                } else {
                    budget.spend(
                        stored
                            .byte_payloads
                            .len()
                            .checked_ilog2()
                            .map_or(1, |n| n as usize + 2),
                    )?;
                    stored
                        .byte_payloads
                        .binary_search(&ValueId(index as u32))
                        .is_ok()
                };
                let root = roots::store_option_root(function, ty, source, byte_payload, budget)?;
                roots_join(&mut roots, &[root], budget)?;
            }
            ordinary.values[index] = roots;
        }
    }
    Ok(())
}

fn check_module_mask(
    module: &TypedModule,
    plan: &CandidatePlan,
    graphs: &[FunctionGraph],
    mask: Option<&[bool]>,
    budget: &mut Budget,
) -> Check<Vec<BTreeSet<u32>>> {
    // The complete raw-type preflight already found every retained Task port.
    // Closed/legacy modules keep their original fee path and need no new table.
    let port_facts = if budget.borrowed_task_ports {
        Some(port_facts::solve(module, graphs, budget)?)
    } else {
        None
    };
    let empty_port_facts = port_facts::FunctionFacts::default();
    let descriptor_origins = descriptor_origins(module, budget)?;
    let mut facts = reconstruct_module(module, graphs, descriptor_origins, mask, budget)?;
    if budget.masked_stored_tasks() && !budget.task_sources() {
        let checked = budget.store_option_sources.take().ok_or_else(|| {
            reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Option result lacks its complete independent stored proof",
            )
        })?;
        overlay_store_option_sources(module, &mut facts, &checked, budget)?;
        budget.store_option_sources = Some(checked);
    }
    if budget.task_sources() {
        let checked = budget.task_stored_sources.take().ok_or_else(|| {
            reject(
                "E-VERIFY-TASK-SOURCES",
                None,
                "Task result overlap lacks its complete actual stored proof",
            )
        })?;
        if checked.len() != module.functions.len() {
            return Err(reject(
                "E-VERIFY-TASK-SOURCES",
                None,
                "Task stored proof omitted an actual function",
            ));
        }
        budget.spend(checked.len())?;
        for ((function, facts), actual) in module.functions.iter().zip(&mut facts).zip(&checked) {
            if actual.values.len() != function.values.len()
                || facts.values.len() != function.values.len()
            {
                return Err(reject(
                    "E-VERIFY-TASK-SOURCES",
                    None,
                    "Task source proof omitted an actual SSA value",
                ));
            }
            if budget.masked_stored_tasks() {
                budget.spend(1)?;
                if !actual.covered {
                    continue;
                }
            }
            for ((ty, row), sources) in function
                .values
                .iter()
                .zip(&mut facts.values)
                .zip(&actual.values)
            {
                if budget.masked_stored_tasks() {
                    budget.spend(1)?;
                    if !mixed_task_strong_value(ty, budget)?
                        || matches!(ty, Type::Address { .. }) && sources.pointer.is_empty()
                    {
                        continue;
                    }
                }
                let roots: Vec<Root> = if matches!(ty, Type::Address { .. }) {
                    budget.spend(sources.pointer.len())?;
                    sources
                        .pointer
                        .iter()
                        .map(|source| match (source.base, source.port) {
                            (HoldRoot::Parameter(p), Some(port)) => Root::ParameterPort(p, port),
                            (HoldRoot::Static, _) => Root::Static,
                            (HoldRoot::Local(local), _) => Root::Local(local),
                            (HoldRoot::Parameter(p), _) => Root::Parameter(p),
                            (HoldRoot::Capture(c), _) => Root::Capture(c),
                        })
                        .collect()
                } else {
                    budget.spend(sources.public.len())?;
                    sources
                        .public
                        .iter()
                        .map(|root| match root {
                            HoldRoot::Static => Root::Static,
                            HoldRoot::Local(local) => Root::Local(*local),
                            HoldRoot::Parameter(p) => Root::Parameter(*p),
                            HoldRoot::Capture(c) => Root::Capture(*c),
                        })
                        .collect()
                };
                if budget.masked_stored_tasks() && matches!(ty, Type::Address { .. }) {
                    // The complete stored proof may refine a Task-exported
                    // pointer away from its frame. Equal coarse origins retain
                    // ordinary field/window detail and its existing loans.
                    budget.spend(sources.pointer.len())?;
                    if sources.pointer.iter().all(|source| source.port.is_none()) {
                        let count = row.len().saturating_add(roots.len());
                        budget.spend(
                            count.saturating_mul(
                                count.checked_ilog2().map_or(1, |n| n as usize + 2),
                            ),
                        )?;
                        let mut ordinary: Vec<_> = row.iter().map(|root| root.coarse()).collect();
                        ordinary.sort_unstable();
                        ordinary.dedup();
                        let mut stored: Vec<_> = roots.iter().map(|root| root.coarse()).collect();
                        stored.sort_unstable();
                        stored.dedup();
                        if ordinary == stored {
                            continue;
                        }
                    }
                }
                *row = roots;
            }
        }
        budget.task_stored_sources = Some(checked);
    }
    if budget.masked_stored_tasks() && budget.task_sources() {
        let checked = budget.store_option_sources.take().ok_or_else(|| {
            reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Option result lacks its complete independent stored proof",
            )
        })?;
        overlay_store_option_sources(module, &mut facts, &checked, budget)?;
        budget.store_option_sources = Some(checked);
    }
    if budget.invocation_callbacks {
        check_callback_returns(module, graphs, &facts, budget)?;
    }

    if budget.borrowed_function_pointers {
        budget.spend(module.functions.len())?;
        for function in &module.functions {
            budget.spend(function.blocks.len())?;
            for block in &function.blocks {
                budget.spend(block.instructions.len())?;
                for instruction in &block.instructions {
                    let InstructionKind::FunctionRef { function: target } = instruction.kind else {
                        continue;
                    };
                    let Some(pointer @ Type::BorrowedFunctionPointer { .. }) = instruction
                        .result
                        .and_then(|id| function.values.get(id.0 as usize))
                    else {
                        continue;
                    };
                    let allowed = borrowed_fp::inputs(pointer, budget)?;
                    let target_function = &module.functions[target.0 as usize];
                    if !matches!(target_function.return_type, Type::Address { .. }) {
                        continue;
                    }
                    budget.spend(target_function.blocks.len())?;
                    for (block, reachable) in target_function
                        .blocks
                        .iter()
                        .zip(&graphs[target.0 as usize].reachable)
                    {
                        if *reachable && let Terminator::Return(Some(value)) = block.terminator {
                            let roots = &facts[target.0 as usize].values[value.0 as usize];
                            budget.spend(
                                roots.len().saturating_mul(allowed.len().saturating_add(1)),
                            )?;
                            if roots.is_empty() || roots.iter().any(|root| !matches!(root, Root::Parameter(index) if allowed.contains(&(*index as usize)))) {
                                return Err(reject("E-VERIFY-FUNCTION-LIFE", Some(instruction.span), "actual pointer target return exceeds the declared input relation"));
                            }
                        }
                    }
                }
            }
        }
    }
    // Public Hold remains an address dependency claim. Weak descriptions are
    // independently reconstructed above for effects and responsibility, and
    // must not be promoted to strong holds when publishing or checking it.
    let mut holds = if descriptor_origins {
        Some(reconstruct_module(module, graphs, false, mask, budget)?)
    } else {
        None
    };
    if let Some(holds) = &mut holds
        && budget.masked_stored_tasks()
    {
        let checked = budget.store_option_sources.take().ok_or_else(|| {
            reject(
                "E-VERIFY-STORE-WINDOW",
                None,
                "Option Hold lacks checked physical sources",
            )
        })?;
        overlay_store_option_sources(module, holds, &checked, budget)?;
        budget.store_option_sources = Some(checked);
    }
    budget.spend(module.resources.len())?;
    let has_buffers = descriptor_origins
        || budget.returned_environment_sources
        || budget.task_sources()
        || budget.masked_stored_tasks()
        || module
            .resources
            .iter()
            .any(|r| r.payload == Type::ByteBuffer);
    let capture_facts = if has_buffers {
        module
            .functions
            .iter()
            .zip(graphs)
            .zip(&facts)
            .map(|((function, graph), ordinary)| {
                captures::reconstruct(module, function, graph, ordinary, budget)
            })
            .collect::<Check<Vec<_>>>()?
    } else {
        vec![]
    };
    let effects = if has_buffers {
        captures::infer(module, graphs, &facts, &capture_facts, budget)?
    } else {
        vec![]
    };
    let mut apart = if has_buffers {
        budget.spend(module.functions.len())?;
        Vec::with_capacity(module.functions.len())
    } else {
        Vec::new()
    };
    for (function_index, (((function, plan), graph), facts)) in module
        .functions
        .iter()
        .zip(&plan.functions)
        .zip(graphs)
        .zip(&facts)
        .enumerate()
    {
        if mask.is_some_and(|mask| mask[function_index]) {
            if has_buffers {
                apart.push(Default::default());
            }
            continue;
        }
        // Only the new parameter-capture lane adds a whole-invocation
        // obligation. Ordinary local captures retain their existing cost.
        let mut captured_parameters = std::collections::BTreeSet::new();
        if facts.product_captures {
            // Origins are the kernel's final private facts, reconstructed after
            // the whole-module return frontier. Candidate Hold rows are not read.
            budget.spend(function.blocks.len())?;
            for (block, live) in function.blocks.iter().zip(&graph.reachable) {
                if !live {
                    continue;
                }
                budget.spend(block.instructions.len())?;
                for instruction in &block.instructions {
                    let InstructionKind::MakeClosure { captures, .. } = &instruction.kind else {
                        continue;
                    };
                    budget.spend(captures.len())?;
                    for capture in captures {
                        if !function.values[capture.0 as usize].is_package_address() {
                            continue;
                        }
                        let roots = &facts.values[capture.0 as usize];
                        budget.spend(roots.len())?;
                        if roots.is_empty() {
                            return Err(unsupported(
                                "shared Product capture requires proven ordinary local Product origins",
                                Some(instruction.span),
                            ));
                        }
                        for root in roots {
                            let actual = match root {
                                Root::Local(local)
                                | Root::Field(local, _)
                                | Root::Projection(local, ..) => {
                                    // A precise inline child still belongs to
                                    // its actual ordinary local carrier. Check
                                    // that entire carrier below; Store windows
                                    // have separate roots and gain no authority.
                                    &function.locals[local.0 as usize].ty
                                }
                                Root::Parameter(index) => {
                                    let local = function.parameters[*index as usize];
                                    let Type::Address { mutable, inner } =
                                        &function.locals[local.0 as usize].ty
                                    else {
                                        return Err(unsupported(
                                            "shared Product capture requires a checked shared Product parameter",
                                            Some(instruction.span),
                                        ));
                                    };
                                    if !*mutable
                                        && matches!(
                                            function.values[capture.0 as usize],
                                            Type::Address { mutable: true, .. }
                                        )
                                    {
                                        return Err(reject(
                                            "E-BORROW-PERMISSION",
                                            Some(instruction.span),
                                            "exclusive Product capture cannot promote shared parameter authority",
                                        ));
                                    }
                                    budget.spend(captured_parameters.len().saturating_add(1))?;
                                    captured_parameters.insert(*index);
                                    inner.as_ref()
                                }
                                _ => {
                                    return Err(unsupported(
                                        "shared Product capture requires proven ordinary local Product origins",
                                        Some(instruction.span),
                                    ));
                                }
                            };
                            if !matches!(actual, Type::Package { .. }) {
                                return Err(unsupported(
                                    "shared Product capture requires proven ordinary local Product origins",
                                    Some(instruction.span),
                                ));
                            }
                            packages::check_captured_product_type(
                                module,
                                actual,
                                Some(instruction.span),
                                budget,
                            )?;
                        }
                    }
                }
            }
        }
        if budget.static_utf8 {
            for (ty, roots) in function.values.iter().zip(&facts.values) {
                budget.spend(roots.len().saturating_add(1))?;
                if matches!(ty, Type::Address { mutable: true, .. })
                    && roots.iter().any(|root| root.coarse() == HoldRoot::Static)
                {
                    return Err(reject(
                        "E-BORROW-PERMISSION",
                        Some(function.span),
                        "program-static text cannot become a mutable address",
                    ));
                }
            }
        }
        let mut lending_sources = std::collections::BTreeSet::new();
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
                if let Some(bound) = &function.reference_return_bound {
                    budget.spend(roots.len().saturating_mul(bound.len().saturating_add(1)))?;
                    if roots.iter().any(|root| root.coarse() != HoldRoot::Static && !matches!(root.coarse(),HoldRoot::Parameter(index) if bound.binary_search(&index).is_ok())) {
                        return Err(reject("E-VERIFY-LIFE",Some(function.span),"actual return origins exceed the declared life input set"));
                    }
                }
                let invocation = function.environment.is_some_and(|id| {
                    let d = &module.closures[id.0 as usize];
                    d.lending_captures.is_empty() && matches!(d.result, Type::Address { .. })
                });
                if function.environment.is_some() && !invocation {
                    budget.spend(roots.len().saturating_add(1))?;
                    for root in roots {
                        let HoldRoot::Capture(index) = root.coarse() else {
                            return Err(reject(
                                "E-BORROW-ESCAPE",
                                Some(function.span),
                                "lending result must borrow actual environment storage, not an activation local or explicit parameter",
                            ));
                        };
                        lending_sources.insert(index);
                    }
                } else if roots
                    .iter()
                    .any(|root| !matches!(root.coarse(), HoldRoot::Parameter(_) | HoldRoot::Static))
                {
                    return Err(reject(
                        "E-BORROW-ESCAPE",
                        Some(function.span),
                        "returned reference would outlive its local or closure storage",
                    ));
                }
            }
            for instruction in &block.instructions {
                if matches!(
                    instruction.kind,
                    InstructionKind::CallClosure { .. } | InstructionKind::CallClosureOnce { .. }
                ) && borrowed_fp::invocation_call(module, function, &instruction.kind).is_none()
                    && instruction.result.is_some_and(|value| {
                        matches!(function.values[value.0 as usize], Type::Address { .. })
                    })
                {
                    return Err(reject(
                        "E-VERIFY-LENDING",
                        Some(instruction.span),
                        "lending result cannot escape a by-value temporary or consuming closure call",
                    ));
                }
                if let Some(value) = instruction.result
                    && (matches!(function.values[value.0 as usize], Type::Address { .. })
                        || tracked_desc(&function.values[value.0 as usize], descriptor_origins))
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
        if let Some(environment) = function.environment
            && matches!(function.return_type, Type::Address { .. })
        {
            let definition = closures::definition(module, environment, Some(function.span))?;
            budget.spend(
                definition
                    .lending_captures
                    .len()
                    .saturating_add(lending_sources.len()),
            )?;
            if !definition
                .lending_captures
                .iter()
                .copied()
                .eq(lending_sources)
            {
                return Err(reject(
                    "E-VERIFY-LENDING",
                    Some(function.span),
                    "declared lending capture sources differ from independently reconstructed returns",
                ));
            }
        }
        check(
            module,
            function,
            plan,
            graph,
            holds
                .as_ref()
                .map_or(facts, |rows| &rows[function.id.0 as usize]),
            false,
            budget,
        )?;
        let mut required = loans::check(
            module,
            function,
            graph,
            facts,
            port_facts
                .as_ref()
                .map_or(&empty_port_facts, |facts| &facts[function_index]),
            has_buffers.then(|| captures::Context {
                module,
                function,
                ordinary: facts,
                facts: &capture_facts[function.id.0 as usize],
                effects: &effects,
            }),
            descriptor_origins,
            budget,
        )?;
        if !captured_parameters.is_empty() {
            // Captured Product parameters may denote Store elements. Their
            // parent remains symbolic here, so every possible write during
            // this call must be disjoint, even after the closure's final read.
            // The existing independently reconstructed effects include finite
            // prefixes and transitive static/callback invocations. No public
            // Hold row or candidate summary supplies this requirement.
            budget.spend(captured_parameters.len())?;
            for parameter in captured_parameters {
                budget.spend(effects[function.id.0 as usize].len())?;
                for (source, write) in &effects[function.id.0 as usize] {
                    if *write {
                        buffer_effects::instantiate(
                            module,
                            function,
                            &[Root::Parameter(parameter)],
                            &[source.position()],
                            &mut required,
                            function.span,
                            budget,
                        )?;
                    }
                }
            }
        }
        if has_buffers {
            apart.push(required);
        }
    }
    if has_buffers {
        buffer_effects::close_apart(
            buffer_effects::ApartContext {
                module,
                graphs,
                facts: &facts,
                capture_facts: &capture_facts,
                effects: &effects,
                mask,
            },
            apart,
            budget,
        )?;
    }
    if !budget.generic_mutex {
        return Ok(Vec::new());
    }
    let mut guard_sources = vec![BTreeSet::new(); module.functions.len()];
    for (index, function) in module.functions.iter().enumerate() {
        if !matches!(function.return_type, Type::MutexGuard { .. }) {
            continue;
        }
        budget.spend(function.blocks.len())?;
        for (block, live) in function.blocks.iter().zip(&graphs[index].reachable) {
            if !live {
                continue;
            }
            if let Terminator::Return(Some(value)) = block.terminator {
                let roots = &facts[index].values[value.0 as usize];
                budget.spend(roots.len())?;
                for root in roots {
                    if let HoldRoot::Parameter(parameter) = root.coarse() {
                        guard_sources[index].insert(parameter);
                    }
                }
            }
        }
    }
    Ok(guard_sources)
}

fn check_callback_return(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    facts: &[Facts],
    d: &ClosureDefinition,
    life: Option<&StorageLifeContract>,
    budget: &mut Budget,
) -> Check<()> {
    borrowed_fp::signature_work(&d.parameters, &d.result, budget)?;
    if let Some(life) = life {
        crate::signature_life::charge(life, budget)?;
        borrowed_fp::relation(&d.parameters, &d.result, life, None)?;
    }
    if !matches!(d.result, Type::Address { .. }) {
        return Ok(());
    }
    let f = &module.functions[d.function.0 as usize];
    budget.spend(f.blocks.len())?;
    for (block, live) in f
        .blocks
        .iter()
        .zip(&graphs[d.function.0 as usize].reachable)
    {
        if *live && let Terminator::Return(Some(v)) = block.terminator {
            let origins = &facts[d.function.0 as usize].values[v.0 as usize];
            budget.spend(origins.len().saturating_mul(
                life.map_or(1, |c| c.bindings.len().saturating_mul(2).saturating_add(1)),
            ))?;
            let mutable = matches!(d.result, Type::Address { mutable: true, .. });
            if origins.is_empty()
                || origins.iter().any(|root| {
                    // Only an actual parameter projection qualifies. A captured
                    // target carried by a parameter is not invocation storage.
                    let index = match root {
                        Root::Parameter(index) | Root::Window(HoldRoot::Parameter(index), ..) => {
                            index
                        }
                        _ => return true,
                    };
                    !d.parameters
                        .get(*index as usize)
                        .is_some_and(|ty| matches!(ty,Type::Address{mutable:m,..} if !mutable||*m))
                        || life.is_some_and(|life| {
                            super::signature_life::outer(life, StorageBase::Parameter(*index))
                                != super::signature_life::outer(life, StorageBase::Result)
                        })
                })
            {
                return Err(reject(
                    "E-VERIFY-CALLABLE-LIFE",
                    Some(f.span),
                    "actual callback return is outside the current invocation input relation",
                ));
            }
        }
    }
    Ok(())
}
fn check_callback_returns(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    facts: &[Facts],
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(module.closures.len().saturating_add(module.functions.len()))?;
    for d in &module.closures {
        budget.spend(d.parameters.len())?;
        if d.lending_captures.is_empty() && borrowed_fp::invocation(&d.parameters, &d.result) {
            check_callback_return(module, graphs, facts, d, None, budget)?;
        }
    }
    for f in &module.functions {
        if let Some(c) = &f.callable_contract {
            budget.spend(c.roles.len())?;
            for role in &c.roles {
                if let Some(life) = &role.life {
                    let d = closures::definition(module, role.closure, Some(f.span))?;
                    check_callback_return(module, graphs, facts, d, Some(life), budget)?;
                }
            }
        }
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
                    retained_return_sources: None,
                    storage_life: None,
                    call_kind: zil_ir::CallKind::Direct,
                    resume_bound: zil_ir::TaskResumeBound::Local,
                    reference_return_bound: None,
                    callable_return_contract: None,
                    callable_contract: None,
                    protocol_contracts: Vec::new(),
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
            dyn_declarations: Vec::new(),
            dyn_interfaces: Vec::new(),
            dyn_vtables: Vec::new(),
            life_declarations: Vec::new(),
            life_instances: Vec::new(),
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
        let descriptor_origins = descriptor_origins(module, budget)?;
        budget.spend(module.functions.len())?;
        let mut summaries = vec![Roots::new(); module.functions.len()];
        let facts = loop {
            let mut changed = false;
            let mut all = Vec::with_capacity(module.functions.len());
            for (index, function) in module.functions.iter().enumerate() {
                TEST_SOLVES.with(|n| n.set(n.get() + 1));
                let facts = reconstruct(
                    module,
                    function,
                    &graphs[index],
                    &summaries,
                    descriptor_origins,
                    budget,
                )?;
                if matches!(function.return_type, Type::Address { .. }) {
                    for (block, reachable) in function.blocks.iter().zip(&graphs[index].reachable) {
                        budget.spend(1)?;
                        if *reachable && let Terminator::Return(Some(value)) = block.terminator {
                            let roots = &facts.values[value.0 as usize];
                            budget.spend(roots.len())?;
                            let incoming: Vec<_> = roots
                                .iter()
                                .filter_map(|root| match root.coarse() {
                                    HoldRoot::Static => Some(Root::Static),
                                    HoldRoot::Parameter(index) => Some(Root::Parameter(index)),
                                    _ => None,
                                })
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
                        role_data_uses: Vec::new(),
                        predecessors: vec![vec![]],
                        successors: vec![vec![]],
                        reachable: vec![true],
                        package_views: packages::CaptureViews::default(),
                    }
                } else {
                    FunctionGraph {
                        role_data_uses: Vec::new(),
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
        let fast = reconstruct_module(
            module,
            &graphs,
            false,
            None,
            &mut Budget::new(&Limits::default()),
        )
        .unwrap();
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
            false,
            None,
            &mut Budget::new(&Limits::default()),
        )
        .unwrap_err();
        assert_eq!(error.code, "E-VERIFY-CALL");
        let module = fixture(&[None], &[false], false);
        let limits = Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        };
        let error = reconstruct_module(
            &module,
            &graphs(&module),
            false,
            None,
            &mut Budget::new(&limits),
        )
        .unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    }
}
