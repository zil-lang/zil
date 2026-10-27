//! Exclusive authority is reconstructed from executable CFG operations. The
//! public Hold candidate describes origins only and cannot manufacture a loan.
use super::buffer_effects::{Apart, Effects};
use super::*;
use std::cell::RefCell;
use std::collections::BTreeSet;
type Loans = BTreeSet<usize>;
type Locals = BTreeMap<LocalId, Loans>;

fn join(target: &mut Loans, source: &Loans, budget: &mut Budget) -> Check<bool> {
    budget.spend(
        source
            .len()
            .saturating_mul(target.len().checked_ilog2().map_or(1, |n| n as usize + 2))
            .saturating_add(1),
    )?;
    let old = target.len();
    target.extend(source);
    Ok(target.len() != old)
}
fn copy_state(state: &Locals, budget: &mut Budget) -> Check<Locals> {
    budget.spend(state.len())?;
    for row in state.values() {
        budget.spend(row.len())?;
    }
    Ok(state.clone())
}
struct Flow {
    values: Vec<Loans>,
    inputs: Vec<Locals>,
    parents: Vec<Loans>,
    mutable: Vec<bool>,
    roots: Vec<Roots>,
}

/// Descriptor writes do not mutate the closure's environment position. Only
/// that position's loan is delegated here; shared addresses captured alongside
/// a Desc keep their own read-only qualification and cannot authorize a write.
fn environment_loans(function: &TypedFunction, loans: &Loans, budget: &mut Budget) -> Check<Loans> {
    budget.spend(loans.len())?;
    let mut result = Loans::new();
    for &loan in loans {
        let ty = if loan < function.values.len() {
            &function.values[loan]
        } else {
            let local = function.parameters[loan - function.values.len()];
            &function.locals[local.0 as usize].ty
        };
        if matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_))) {
            budget.spend(result.len().saturating_add(1))?;
            result.insert(loan);
        }
    }
    Ok(result)
}

fn local_transfer(
    state: &mut Locals,
    instruction: &Instruction,
    values: &[Loans],
    budget: &mut Budget,
) -> Check<()> {
    match instruction.kind {
        InstructionKind::Store { local, value } => {
            budget.spend(values[value.0 as usize].len().saturating_add(1))?;
            if values[value.0 as usize].is_empty() {
                state.remove(&local);
            } else {
                state.insert(local, values[value.0 as usize].clone());
            }
        }
        InstructionKind::TakeLocal { local }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropClosure { local } => {
            state.remove(&local);
        }
        _ => {}
    }
    Ok(())
}
fn reconstruct(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    facts: &Facts,
    budget: &mut Budget,
) -> Check<Flow> {
    let v = f.values.len();
    let count = v
        .checked_add(f.parameters.len())
        .ok_or_else(|| limit("loan identity count overflow"))?;
    budget.spend(
        count
            .saturating_mul(4)
            .saturating_add(f.blocks.len().saturating_mul(2)),
    )?;
    let mut flow = Flow {
        values: vec![Loans::new(); v],
        inputs: vec![Locals::new(); f.blocks.len()],
        parents: vec![Loans::new(); count],
        mutable: vec![false; count],
        roots: vec![vec![]; count],
    };
    for (i, ty) in f.values.iter().enumerate() {
        flow.mutable[i] = matches!(ty, Type::Address { mutable: true, .. });
        budget.spend(facts.values[i].len())?;
        flow.roots[i] = facts.values[i].clone();
    }
    for (i, local) in f.parameters.iter().enumerate() {
        flow.mutable[v + i] = affine_type(module, &f.locals[local.0 as usize].ty);
        flow.roots[v + i] = vec![Root::Parameter(i as u32)];
    }
    let mut outputs = vec![Locals::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1)?;
            if !graph.reachable[b] {
                continue;
            }
            let mut state = Locals::new();
            if b == f.entry.0 as usize {
                budget.spend(f.parameters.len())?;
                for (i, local) in f.parameters.iter().enumerate() {
                    if closures::holds_addresses(module, &f.locals[local.0 as usize].ty) {
                        state.insert(*local, Loans::from([v + i]));
                    }
                }
            } else {
                for &p in &graph.predecessors[b] {
                    if !graph.reachable[p] {
                        continue;
                    }
                    for (local, row) in &outputs[p] {
                        join(state.entry(*local).or_default(), row, budget)?;
                    }
                }
            }
            flow.inputs[b] = copy_state(&state, budget)?;
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let Some(result) = instruction.result {
                    let id = result.0 as usize;
                    let mut row = Loans::new();
                    let mut parents = Loans::new();
                    match &instruction.kind {
                        InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local } => {
                            row.insert(id);
                            if matches!(f.locals[local.0 as usize].ty, Type::Closure(_)) {
                                // The new loan qualifies the environment position.
                                // Its captured loans retain their own permissions.
                                flow.roots[id] = vec![Root::Local(*local)];
                                if let Some(held) = state.get(local) {
                                    join(&mut row, held, budget)?;
                                }
                            }
                        }
                        InstructionKind::ByteBufferBorrow { .. }
                        | InstructionKind::OwnedStoreBorrow { .. }
                        | InstructionKind::BorrowOwnedArrayElement { .. } => {
                            row.insert(id);
                        }
                        InstructionKind::BorrowCapture { .. }
                        | InstructionKind::ReadCapture { .. }
                            if matches!(f.values[id], Type::Address { .. }) =>
                        {
                            row.insert(id);
                        }
                        InstructionKind::ReborrowAddress { address, .. } => {
                            row.insert(id);
                            join(&mut parents, &flow.values[address.0 as usize], budget)?;
                        }
                        InstructionKind::AddressField { address, .. }
                        | InstructionKind::AddressSlice { address, .. }
                        | InstructionKind::AddressIndex { address, .. } => {
                            join(&mut row, &flow.values[address.0 as usize], budget)?;
                        }
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                            if let Some(source) = state.get(local) {
                                join(&mut row, source, budget)?;
                            }
                        }
                        InstructionKind::Call { arguments, .. }
                            if matches!(f.values[id], Type::Address { .. }) =>
                        {
                            row.insert(id);
                            for argument in arguments {
                                if overlaps(
                                    &facts.values[id],
                                    &facts.values[argument.0 as usize],
                                    budget,
                                )? {
                                    join(&mut parents, &flow.values[argument.0 as usize], budget)?;
                                }
                            }
                        }
                        InstructionKind::MakeClosure { captures, .. } => {
                            for capture in captures {
                                join(&mut row, &flow.values[capture.0 as usize], budget)?;
                            }
                        }
                        _ => {}
                    }
                    changed |= join(&mut flow.values[id], &row, budget)?;
                    changed |= join(&mut flow.parents[id], &parents, budget)?;
                }
                local_transfer(&mut state, instruction, &flow.values, budget)?;
            }
            budget.spend(state.len().saturating_add(outputs[b].len()))?;
            if state != outputs[b] {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Transitive ancestry is finite and budgeted. Cyclic static reborrows lose
    // the unique dynamic-parent proof and are rejected conservatively.
    loop {
        let mut changed = false;
        for i in 0..count {
            budget.spend(flow.parents[i].len().saturating_add(1))?;
            let parents = flow.parents[i].clone();
            for parent in parents {
                budget.spend(flow.parents[parent].len())?;
                let ancestors = flow.parents[parent].clone();
                changed |= join(&mut flow.parents[i], &ancestors, budget)?;
            }
            if flow.parents[i].contains(&i) {
                return Err(reject(
                    "E-BORROW-CYCLE",
                    Some(f.span),
                    "cyclic reborrow has no independently bounded parent authority",
                ));
            }
        }
        if !changed {
            break;
        }
    }
    Ok(flow)
}
fn active(
    flow: &Flow,
    locals: &Locals,
    live: &[bool],
    v: usize,
    budget: &mut Budget,
) -> Check<Loans> {
    budget.spend(live.len())?;
    let mut active = Loans::new();
    for (i, yes) in live.iter().enumerate() {
        if !yes {
            continue;
        }
        if i < v {
            join(&mut active, &flow.values[i], budget)?;
        } else if let Some(row) = locals.get(&LocalId((i - v) as u32)) {
            join(&mut active, row, budget)?;
        }
    }
    budget.spend(active.len())?;
    let descendants = active.clone();
    for loan in descendants {
        join(&mut active, &flow.parents[loan], budget)?;
    }
    Ok(active)
}
struct Access<'a> {
    flow: &'a Flow,
    active: &'a Loans,
    ignore: &'a Loans,
    span: Span,
    apart: Option<&'a RefCell<Apart>>,
}
impl Access<'_> {
    fn check_descriptor_callback(
        &self,
        function: &TypedFunction,
        held: &Loans,
        roots: &[Root],
        write: bool,
        budget: &mut Budget,
    ) -> Check<()> {
        // A callback's captured address is in use during this invocation even
        // when this is the closure's final use. Future liveness alone would
        // miss its conflict with a Desc write in the same environment.
        budget.spend(self.active.len())?;
        let mut active = self.active.clone();
        join(&mut active, held, budget)?;
        let own = environment_loans(function, held, budget)?;
        Access {
            flow: self.flow,
            active: &active,
            ignore: self.ignore,
            span: self.span,
            apart: self.apart,
        }
        .check(&own, roots, write, budget)
    }

    fn check(&self, own: &Loans, roots: &[Root], write: bool, budget: &mut Budget) -> Check<()> {
        for &other in self.active {
            budget.spend(own.len().saturating_add(1))?;
            if self.ignore.contains(&other) || own.contains(&other) {
                continue;
            }
            if !(write || self.flow.mutable[other]) {
                continue;
            }
            if !overlaps(roots, &self.flow.roots[other], budget)? {
                if let Some(apart) = self.apart {
                    buffer_effects::unknown_pairs(
                        roots,
                        &self.flow.roots[other],
                        &mut apart.borrow_mut(),
                        budget,
                    )?;
                }
                continue;
            }
            // A closure may hold unrelated shared and exclusive captures. Only
            // the loans intersecting this other window must descend from it;
            // an unrelated capture cannot invalidate the delegated authority.
            let mut related = false;
            let mut delegated = true;
            for id in own {
                if overlaps(&self.flow.roots[*id], &self.flow.roots[other], budget)? {
                    related = true;
                    if !self.flow.parents[*id].contains(&other) {
                        delegated = false;
                        break;
                    }
                }
            }
            if related && delegated {
                continue;
            }
            return Err(reject(
                "E-BORROW-CONFLICT",
                Some(self.span),
                "access overlaps an active address window without its delegated authority",
            ));
        }
        Ok(())
    }
}
fn check_pairs(
    values: &[ValueId],
    _f: &TypedFunction,
    flow: &Flow,
    span: Span,
    apart: Option<&RefCell<Apart>>,
    budget: &mut Budget,
) -> Check<()> {
    for (i, a) in values.iter().enumerate() {
        for b in &values[i + 1..] {
            budget.spend(1)?;
            // A concrete environment argument carries separate windows for
            // its position and its captures. Looking only at the argument's
            // position would hide a mutable captured target from another arg.
            for left in &flow.values[a.0 as usize] {
                for right in &flow.values[b.0 as usize] {
                    budget.spend(1)?;
                    if (flow.mutable[*left] || flow.mutable[*right])
                        && let Some(apart) = apart
                    {
                        buffer_effects::unknown_pairs(
                            &flow.roots[*left],
                            &flow.roots[*right],
                            &mut apart.borrow_mut(),
                            budget,
                        )?;
                    }
                    if (flow.mutable[*left] || flow.mutable[*right])
                        && overlaps(&flow.roots[*left], &flow.roots[*right], budget)?
                    {
                        return Err(reject(
                            "E-BORROW-CONFLICT",
                            Some(span),
                            "simultaneous arguments or capture fields overlap an exclusive qualification",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
fn consumers(kind: &InstructionKind) -> impl Iterator<Item = &ValueId> {
    let (callee, values): (Option<&ValueId>, &[ValueId]) = match kind {
        InstructionKind::Store { value, .. }
        | InstructionKind::StoreOwnedArrayElement { value, .. } => {
            (None, std::slice::from_ref(value))
        }
        InstructionKind::MakeOwnedArray { elements } => (None, elements),
        InstructionKind::CallClosureOnce { callee, arguments } => (Some(callee), arguments),
        InstructionKind::Call { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallClosure { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. }
        | InstructionKind::CallClosureBorrowed { arguments, .. } => (None, arguments),
        InstructionKind::MakeClosure { captures, .. } => (None, captures),
        _ => (None, &[]),
    };
    callee.into_iter().chain(values)
}

fn ownership(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<Vec<bool>> {
    let n = f.blocks.len();
    let v = f.values.len();
    reserve(budget, &[n, v, 2])?;
    budget.spend(v.saturating_mul(2))?;
    let mut affine: Vec<_> = f.values.iter().map(|t| affine_type(module, t)).collect();
    // Owning-array publication consumes even a resource-free concrete callback.
    // This is a move obligation, not a fabricated resource cleanup obligation.
    // Preserve the established ordinary closure Copy profile outside this lane.
    for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
        budget.spend(1)?;
        let elements: &[ValueId] = match &instruction.kind {
            InstructionKind::MakeOwnedArray { elements } => elements,
            InstructionKind::StoreOwnedArrayElement { value, .. } => std::slice::from_ref(value),
            _ => &[],
        };
        budget.spend(elements.len())?;
        for value in elements {
            if matches!(f.values[value.0 as usize], Type::Closure(_)) {
                affine[value.0 as usize] = true;
            }
        }
        if matches!(
            instruction.kind,
            InstructionKind::TakeOwnedArrayElement { .. }
        ) && let Some(value) = instruction.result
            && matches!(f.values[value.0 as usize], Type::Closure(_))
        {
            affine[value.0 as usize] = true;
        }
    }
    let mut view = vec![false; v];
    for block in &f.blocks {
        for instruction in &block.instructions {
            if let Some(value) = instruction.result {
                view[value.0 as usize] = affine[value.0 as usize]
                    && matches!(
                        instruction.kind,
                        InstructionKind::Load(_)
                            | InstructionKind::AddressField { .. }
                            | InstructionKind::AddressSlice { .. }
                            | InstructionKind::AddressIndex { .. }
                    );
            }
        }
    }
    let mut inputs = vec![vec![false; v]; n];
    let mut outputs = inputs.clone();
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            if !graph.reachable[b] {
                continue;
            }
            budget.spend(v)?;
            let mut state = vec![false; v];
            if b != f.entry.0 as usize {
                for &p in &graph.predecessors[b] {
                    if graph.reachable[p] {
                        bits_join(&mut state, &outputs[p], budget)?;
                    }
                }
            }
            inputs[b].clone_from(&state);
            for instruction in &block.instructions {
                budget.spend(consumers(&instruction.kind).count().saturating_add(1))?;
                if let Some(value) = instruction.result {
                    state[value.0 as usize] = false;
                }
                for value in consumers(&instruction.kind) {
                    if affine[value.0 as usize] {
                        state[value.0 as usize] = true;
                    }
                }
            }
            if state != outputs[b] {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (b, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[b] {
            continue;
        }
        budget.spend(v)?;
        let mut state = inputs[b].clone();
        let check_use = |value: ValueId, state: &[bool], span: Span| -> Check<()> {
            if affine[value.0 as usize] && state[value.0 as usize] {
                return Err(reject(
                    "E-VERIFY-MOVE",
                    Some(span),
                    "non-Copy SSA carrier was already transferred",
                ));
            }
            Ok(())
        };
        for instruction in &block.instructions {
            budget.spend(
                instruction_operands(&instruction.kind)
                    .count()
                    .saturating_add(1),
            )?;
            for value in instruction_operands(&instruction.kind) {
                check_use(value, &state, instruction.span)?;
            }
            if let Some(value) = instruction.result {
                state[value.0 as usize] = false;
            }
            for value in consumers(&instruction.kind) {
                if !affine[value.0 as usize] {
                    continue;
                }
                check_use(*value, &state, instruction.span)?;
                if view[value.0 as usize] {
                    return Err(reject(
                        "E-VERIFY-MOVE",
                        Some(instruction.span),
                        "Load/projection is an access view; transfer requires TakeLocal or an explicit child reborrow",
                    ));
                }
                state[value.0 as usize] = true;
            }
        }
        if let Terminator::Return(Some(value)) = block.terminator {
            check_use(value, &state, f.span)?;
            if affine[value.0 as usize] && view[value.0 as usize] {
                return Err(reject(
                    "E-VERIFY-MOVE",
                    Some(f.span),
                    "return cannot copy an exclusive access view",
                ));
            }
        }
    }
    Ok(view)
}
pub(super) fn check(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    facts: &Facts,
    effects: &[Effects],
    head_windows: bool,
    budget: &mut Budget,
) -> Check<Apart> {
    if !head_windows
        && !f.locals.iter().any(|l| {
            affine_type(module, &l.ty)
                || byte_buffers::is_carrier(&l.ty)
                || resources::arrays::is_owned_type(&l.ty)
        })
        && !f.values.iter().any(|t| {
            affine_type(module, t)
                || byte_buffers::is_carrier(t)
                || resources::arrays::is_owned_type(t)
        })
    {
        return Ok(Apart::new());
    }
    let apart = RefCell::new(Apart::new());
    let apart_ref = (!effects.is_empty()).then_some(&apart);
    let view = ownership(module, f, graph, budget)?;
    let flow = reconstruct(module, f, graph, facts, budget)?;
    let n = f.blocks.len();
    let v = f.values.len();
    let width = v
        .checked_add(f.locals.len())
        .ok_or_else(|| limit("loan liveness overflow"))?;
    reserve(budget, &[n, width, 2])?;
    let mut live_in = vec![vec![false; width]; n];
    let mut live_out = live_in.clone();
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            if !graph.reachable[b] {
                continue;
            }
            budget.spend(width)?;
            let mut live = vec![false; width];
            for &next in &graph.successors[b] {
                bits_join(&mut live, &live_in[next], budget)?;
            }
            live_out[b].clone_from(&live);
            term_use(&mut live, &f.blocks[b].terminator);
            for instruction in f.blocks[b].instructions.iter().rev() {
                budget.spend(1 + instruction_operands(&instruction.kind).count())?;
                reverse(&mut live, instruction, v);
            }
            if live != live_in[b] {
                live_in[b] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (b, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[b] {
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
        let mut locals = copy_state(&flow.inputs[b], budget)?;
        let mut descriptor_locals = if head_windows {
            if let Some(input) = facts.inputs.get(b) {
                state_cost(input, budget)?;
                input.clone()
            } else {
                State::new()
            }
        } else {
            State::new()
        };
        for (i, instruction) in block.instructions.iter().enumerate() {
            let active = active(&flow, &locals, &after[i], v, budget)?;
            let ignore = instruction
                .result
                .map_or_else(Loans::new, |r| Loans::from([r.0 as usize]));
            let span = instruction.span;
            let empty = Loans::new();
            let access = Access {
                flow: &flow,
                active: &active,
                ignore: &ignore,
                span,
                apart: apart_ref,
            };
            if let Some((buffer, write)) = buffer_effects::access(&instruction.kind, head_windows) {
                access.check(&empty, &facts.values[buffer.0 as usize], write, budget)?;
            }
            if head_windows
                && let Some(callback) = buffer_effects::callback_roots(
                    module,
                    f,
                    &instruction.kind,
                    &facts.values,
                    &descriptor_locals,
                    budget,
                )?
            {
                // No unproved closure body is silently read-only. This
                // conservative access also exports unknown parameter
                // separation through the existing independent contract.
                let held = match &instruction.kind {
                    InstructionKind::CallClosure { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. }
                    | InstructionKind::CallClosureBorrowed { callee, .. } => {
                        &flow.values[callee.0 as usize]
                    }
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        locals.get(callee).unwrap_or(&empty)
                    }
                    _ => unreachable!(),
                };
                access.check_descriptor_callback(f, held, callback, true, budget)?;
            }
            if let InstructionKind::Call {
                function: callee,
                arguments,
            } = &instruction.kind
                && !effects.is_empty()
            {
                for (parameter, write) in &effects[callee.0 as usize] {
                    budget.spend(1)?;
                    let argument = arguments[*parameter as usize];
                    if head_windows
                        && captures_descriptors(module, &f.values[argument.0 as usize], budget)?
                    {
                        access.check_descriptor_callback(
                            f,
                            &flow.values[argument.0 as usize],
                            &facts.values[argument.0 as usize],
                            *write,
                            budget,
                        )?;
                    } else {
                        access.check(&empty, &facts.values[argument.0 as usize], *write, budget)?;
                    }
                }
            }
            match &instruction.kind {
                InstructionKind::Load(local)
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    access.check(
                        &empty,
                        &[Root::Local(*local)],
                        matches!(instruction.kind, InstructionKind::BorrowLocalMut { .. }),
                        budget,
                    )?;
                }
                InstructionKind::BorrowOwnedArrayElement { local, mutable, .. } => {
                    access.check(&empty, &[Root::Local(*local)], *mutable, budget)?;
                }
                InstructionKind::Store { local, .. }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local)
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::TakeOwnedArrayElement { local, .. }
                | InstructionKind::DropOwnedArrayElement { local, .. }
                | InstructionKind::StoreOwnedArrayElement { local, .. } => {
                    access.check(&empty, &[Root::Local(*local)], true, budget)?;
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index }
                | InstructionKind::StorePackageField { local, index, .. } => {
                    access.check(&empty, &[field_root(f, *local, *index)], true, budget)?;
                }
                InstructionKind::ReadAddress { address }
                | InstructionKind::SliceLength { address }
                | InstructionKind::WriteAddress { address, .. }
                | InstructionKind::ReborrowAddress { address, .. } => {
                    let write = matches!(
                        instruction.kind,
                        InstructionKind::WriteAddress { .. }
                            | InstructionKind::ReborrowAddress { mutable: true, .. }
                    );
                    access.check(
                        &flow.values[address.0 as usize],
                        &facts.values[address.0 as usize],
                        write,
                        budget,
                    )?;
                }
                InstructionKind::AddressSlice { address, .. }
                | InstructionKind::AddressIndex { address, .. }
                    if slices::is_window(&f.values[address.0 as usize]) =>
                {
                    // Both operations read the dynamic length even when their
                    // projected result is discarded. That metadata read has
                    // the same qualification rule as an explicit SliceLength.
                    // Array projections preserve their existing behavior.
                    access.check(
                        &flow.values[address.0 as usize],
                        &facts.values[address.0 as usize],
                        false,
                        budget,
                    )?;
                }
                InstructionKind::BorrowCapture { .. } | InstructionKind::ReadCapture { .. } => {
                    if let Some(result) = instruction.result {
                        access.check(
                            &empty,
                            &facts.values[result.0 as usize],
                            flow.mutable[result.0 as usize],
                            budget,
                        )?;
                    }
                }
                InstructionKind::DropManaged { local }
                    if matches!(f.locals[local.0 as usize].ty, Type::Managed(_)) =>
                {
                    access.check(&empty, &[Root::Local(*local)], true, budget)?;
                }
                InstructionKind::TakeLocal { local } => {
                    if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                        || matches!(f.locals[local.0 as usize].ty, Type::Package { .. })
                        || errors::owned(&f.locals[local.0 as usize].ty)
                        || resources::arrays::is_owned_type(&f.locals[local.0 as usize].ty)
                        || matches!(f.locals[local.0 as usize].ty, Type::Managed(_))
                    {
                        access.check(&empty, &[Root::Local(*local)], true, budget)?;
                    }
                    if let Some(held) = locals.get(local) {
                        for (value, live) in after[i][..v].iter().enumerate() {
                            budget.spend(1)?;
                            if *live && view[value] && !flow.values[value].is_disjoint(held) {
                                return Err(reject(
                                    "E-BORROW-CONFLICT",
                                    Some(span),
                                    "cannot transfer a carrier while an access view of it remains live",
                                ));
                            }
                        }
                    }
                }
                InstructionKind::TakeCapture { index }
                    if instruction.result.is_some_and(|value| {
                        byte_buffers::is_owned(&f.values[value.0 as usize])
                            || matches!(f.values[value.0 as usize], Type::Closure(_))
                    }) =>
                {
                    access.check(&empty, &[Root::Capture(*index)], true, budget)?;
                }
                InstructionKind::Call { arguments, .. }
                | InstructionKind::ForeignCall { arguments, .. }
                | InstructionKind::MakeClosure {
                    captures: arguments,
                    ..
                } => {
                    check_pairs(arguments, f, &flow, span, apart_ref, budget)?;
                    for value in arguments {
                        if let Type::Address { mutable, ref inner } = f.values[value.0 as usize] {
                            if matches!(inner.as_ref(), Type::Closure(_)) {
                                for &loan in &flow.values[value.0 as usize] {
                                    access.check(
                                        &flow.values[value.0 as usize],
                                        &flow.roots[loan],
                                        flow.mutable[loan],
                                        budget,
                                    )?;
                                }
                            } else {
                                access.check(
                                    &flow.values[value.0 as usize],
                                    &facts.values[value.0 as usize],
                                    mutable,
                                    budget,
                                )?;
                            }
                        }
                    }
                }
                InstructionKind::CallClosureMut { callee, arguments }
                | InstructionKind::CallClosureShared { callee, arguments } => {
                    check_pairs(arguments, f, &flow, span, apart_ref, budget)?;
                    if let Some(held) = locals.get(callee) {
                        for &loan in held {
                            access.check(held, &flow.roots[loan], flow.mutable[loan], budget)?;
                        }
                    }
                }
                InstructionKind::CallClosureBorrowed { callee, kind, .. } => {
                    for &loan in &flow.values[callee.0 as usize] {
                        access.check(
                            &flow.values[callee.0 as usize],
                            &flow.roots[loan],
                            *kind == CallableKind::FnMut && flow.mutable[loan],
                            budget,
                        )?;
                    }
                }
                InstructionKind::CallClosure { callee, .. }
                | InstructionKind::CallClosureOnce { callee, .. } => {
                    for &loan in &flow.values[callee.0 as usize] {
                        access.check(
                            &flow.values[callee.0 as usize],
                            &flow.roots[loan],
                            matches!(instruction.kind, InstructionKind::CallClosureOnce { .. })
                                && flow.mutable[loan],
                            budget,
                        )?;
                    }
                }
                _ => {}
            }
            local_transfer(&mut locals, instruction, &flow.values, budget)?;
            if head_windows {
                super::local_transfer(&mut descriptor_locals, instruction, &facts.values, budget)?;
            }
        }
    }
    Ok(apart.into_inner())
}
