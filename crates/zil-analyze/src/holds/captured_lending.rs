//! Actual capture slots for ordinary external owning-address lending.
//! This producer map is never consumed by the independent verifier.
use super::*;
type Parents = BTreeSet<ValueId>;
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Slot {
    roots: Roots,
    parents: Parents,
}
type Slots = BTreeMap<u32, Slot>;
pub(super) type Locals = BTreeMap<LocalId, Slots>;
pub(super) struct Facts {
    pub(super) values: Vec<Slots>,
    pub(super) inputs: Vec<Locals>,
    pub(super) parents: Vec<Parents>,
}
fn cost(slots: &Slots, budget: &mut Budget) -> CompileResult<()> {
    budget.spend(slots.len(), "external lending slot scan")?;
    for slot in slots.values() {
        budget.spend(
            slot.roots.len().saturating_add(slot.parents.len()),
            "external lending slot roots",
        )?;
    }
    Ok(())
}
fn join(to: &mut Slots, from: &Slots, budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(from.len(), "external lending slots merge")?;
    let mut changed = false;
    for (slot, roots) in from {
        budget.lookup(to.len(), "external lending slot lookup")?;
        let target = to.entry(*slot).or_default();
        changed |= extend(&mut target.roots, &roots.roots, budget)?;
        changed |= parent_join(&mut target.parents, &roots.parents, budget)?;
    }
    Ok(changed)
}
fn copy_state(from: &Locals, budget: &mut Budget) -> CompileResult<Locals> {
    budget.spend(from.len(), "external lending local snapshot")?;
    for slots in from.values() {
        cost(slots, budget)?;
    }
    Ok(from.clone())
}
pub(super) fn local_step(
    state: &mut Locals,
    i: &Instruction,
    values: &[Slots],
    budget: &mut Budget,
) -> CompileResult<()> {
    match i.kind {
        InstructionKind::Store { local, value } => {
            budget.lookup(state.len(), "external lending local write")?;
            let slots = &values[value.0 as usize];
            cost(slots, budget)?;
            if slots.is_empty() {
                state.remove(&local);
            } else {
                state.insert(local, slots.clone());
            }
        }
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::TakeLocal { local }
        | InstructionKind::DropClosure { local } => {
            budget.lookup(state.len(), "external lending local end")?;
            state.remove(&local);
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn entry<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    kind: &InstructionKind,
) -> Option<&'a ClosureDefinition> {
    let ty = match kind {
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => &f.locals[callee.0 as usize].ty,
        InstructionKind::CallClosureBorrowed { callee, .. } => &f.values[callee.0 as usize],
        _ => return None,
    };
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else {
        return None;
    };
    module.closures.get(id.0 as usize)
}
pub(super) fn reconstruct(
    module: &TypedModule,
    f: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    ordinary: &[Roots],
    summaries: &[Roots],
    budget: &mut Budget,
) -> CompileResult<Facts> {
    budget.spend(
        f.values
            .len()
            .saturating_add(f.blocks.len().saturating_mul(2)),
        "external lending slot tables",
    )?;
    let mut values = vec![Slots::new(); f.values.len()];
    let mut inputs = vec![Locals::new(); f.blocks.len()];
    let mut outputs = vec![Locals::new(); f.blocks.len()];
    let initial = Locals::new();
    budget.spend(f.parameters.len(), "external lending parameter slots")?;
    for local in &f.parameters {
        let ty = &f.locals[local.0 as usize].ty;
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        if let Type::Closure(id) = ty {
            let d = &module.closures[id.0 as usize];
            budget.spend(d.captures.len(), "external lending symbolic slots")?;
            for ty in &d.captures {
                if matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }))
                    && !d.lending_captures.is_empty()
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "external owning-address lending through a callable parameter needs actual capture-slot substitution",
                        Some(f.span),
                    )]);
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1, "external lending slot iteration")?;
            if !reachable[b] {
                continue;
            }
            let mut state = if b == f.entry.0 as usize {
                copy_state(&initial, budget)?
            } else {
                Locals::new()
            };
            for p in &predecessors[b] {
                budget.spend(1, "external lending slot predecessor")?;
                if !reachable[*p] {
                    continue;
                }
                budget.spend(outputs[*p].len(), "external lending predecessor slots")?;
                for (local, slots) in &outputs[*p] {
                    budget.lookup(state.len(), "external lending predecessor local")?;
                    join(state.entry(*local).or_default(), slots, budget)?;
                }
            }
            inputs[b] = copy_state(&state, budget)?;
            for i in &block.instructions {
                budget.spend(1, "external lending slot instruction")?;
                if let Some(result) = i.result {
                    let mut row = Slots::new();
                    match &i.kind {
                        InstructionKind::MakeClosure { closure, captures } => {
                            let d = &module.closures[closure.0 as usize];
                            budget.spend(captures.len(), "external lending actual slots")?;
                            for (index, (value, ty)) in captures.iter().zip(&d.captures).enumerate()
                            {
                                if matches!(ty, Type::Address { .. }) {
                                    budget.spend(
                                        ordinary[value.0 as usize].len(),
                                        "external lending actual slot copy",
                                    )?;
                                    row.insert(
                                        index as u32,
                                        Slot {
                                            roots: ordinary[value.0 as usize].clone(),
                                            parents: Parents::from([*value]),
                                        },
                                    );
                                }
                            }
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            if let Some(contract) =
                                &module.functions[function.0 as usize].callable_return_contract
                                && let Some(bindings) = &contract.capture_sources
                            {
                                for binding in bindings {
                                    let zil_ir::StorageBase::Capture(index) = binding.position.base
                                    else {
                                        return Err(bad(
                                            "returned environment destination is not an actual capture",
                                        ));
                                    };
                                    let mut slot = Slot::default();
                                    for atom in &binding.sources {
                                        if matches!(atom, zil_ir::StorageSource::Static { .. }) {
                                            budget.spend(1, "proved static environment leaf")?;
                                            slot.roots.insert(Root::Static);
                                            continue;
                                        }
                                        let source = crate::exact_storage_source(atom)?;
                                        extend(
                                            &mut slot.roots,
                                            &factory_source_roots(
                                                f, source, arguments, ordinary, budget,
                                            )?,
                                            budget,
                                        )?;
                                        let zil_ir::StorageBase::Parameter(parameter) = source.base
                                        else {
                                            unreachable!("source checked above")
                                        };
                                        slot.parents.insert(arguments[parameter as usize]);
                                    }
                                    // This ordinary envelope conservatively unions all
                                    // leaves of one capture. The independent stored
                                    // solver retains the exact nested permissions.
                                    let actual = row.entry(index).or_default();
                                    extend(&mut actual.roots, &slot.roots, budget)?;
                                    budget.spend(
                                        slot.parents.len(),
                                        "returned capture parent envelope",
                                    )?;
                                    actual.parents.extend(slot.parents);
                                }
                            }
                        }
                        InstructionKind::Load(local)
                        | InstructionKind::TakeLocal { local }
                        | InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local } => {
                            budget.lookup(state.len(), "external lending slot read")?;
                            if let Some(source) = state.get(local) {
                                cost(source, budget)?;
                                row.clone_from(source);
                            }
                        }
                        InstructionKind::ReborrowAddress { address, .. } => {
                            cost(&values[address.0 as usize], budget)?;
                            row.clone_from(&values[address.0 as usize]);
                        }
                        _ => {}
                    }
                    changed |= join(&mut values[result.0 as usize], &row, budget)?;
                }
                local_step(&mut state, i, &values, budget)?;
            }
            let previous = copy_state(&outputs[b], budget)?;
            let comparison = copy_state(&state, budget)?;
            if comparison != previous {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut facts = Facts {
        values,
        inputs,
        parents: Vec::new(),
    };
    facts.parents = ancestry(
        module,
        f,
        predecessors,
        reachable,
        summaries,
        &facts,
        budget,
    )?;
    Ok(facts)
}
pub(super) fn input(facts: &Facts, b: usize, budget: &mut Budget) -> CompileResult<Locals> {
    copy_state(&facts.inputs[b], budget)
}
fn rebase(relative: Root, actual: Root) -> CompileResult<Root> {
    let Root::Window(HoldRoot::Capture(_), start, end) = relative else {
        return Ok(actual);
    };
    Ok(match actual.position() {
        Root::Window(origin, base, bound) => {
            let lo = base
                .checked_add(start)
                .ok_or_else(|| bad("external lending window overflow"))?;
            let hi = base
                .checked_add(end)
                .filter(|hi| *hi <= bound)
                .ok_or_else(|| bad("external lending window exceeds actual parent"))?;
            Root::Window(origin, lo, hi)
        }
        Root::Projection(local, base, bound) => {
            let lo = base
                .checked_add(start)
                .ok_or_else(|| bad("external lending projection overflow"))?;
            let hi = base
                .checked_add(end)
                .filter(|hi| *hi <= bound)
                .ok_or_else(|| bad("external lending projection exceeds actual parent"))?;
            Root::Projection(local, lo, hi)
        }
        Root::Local(local) => Root::Window(HoldRoot::Local(local), start, end),
        Root::Parameter(p) => Root::Window(HoldRoot::Parameter(p), start, end),
        Root::Capture(c) => Root::Window(HoldRoot::Capture(c), start, end),
        other => other,
    })
}
pub(super) fn result(
    d: &ClosureDefinition,
    returned: &Roots,
    slots: Option<&Slots>,
    environment: &Roots,
    budget: &mut Budget,
) -> CompileResult<Roots> {
    let mut found = Roots::new();
    budget.spend(returned.len(), "external lending actual returns")?;
    for root in returned {
        let index = match root.position() {
            Root::Capture(i) | Root::Window(HoldRoot::Capture(i), ..) => i,
            _ => continue,
        };
        let ty = d
            .captures
            .get(index as usize)
            .ok_or_else(|| bad("lending return capture is absent"))?;
        if !matches!(ty, Type::Address { .. }) {
            extend(&mut found, environment, budget)?;
            continue;
        }
        if let Some(slots) = slots {
            budget.lookup(slots.len(), "external lending selected slot")?;
            if let Some(actual) = slots.get(&index) {
                budget.spend(actual.roots.len(), "external lending selected actual roots")?;
                for actual in &actual.roots {
                    let projected = rebase(*root, *actual)?;
                    budget.lookup(found.len(), "external lending projected insertion")?;
                    found.insert(projected);
                }
            }
        }
    }
    Ok(found)
}

fn parent_join(to: &mut Parents, from: &Parents, b: &mut Budget) -> CompileResult<bool> {
    b.spend(from.len(), "external lending parent merge scan")?;
    let mut changed = false;
    for parent in from {
        b.lookup(to.len(), "external lending parent insertion")?;
        changed |= to.insert(*parent);
    }
    Ok(changed)
}
type AddressLocals = BTreeMap<LocalId, Parents>;
fn copy_addresses(from: &AddressLocals, b: &mut Budget) -> CompileResult<AddressLocals> {
    b.spend(from.len(), "external lending address-local snapshot")?;
    for parents in from.values() {
        b.spend(parents.len(), "external lending local ancestry copy")?;
    }
    Ok(from.clone())
}
fn ancestry(
    module: &TypedModule,
    f: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    summaries: &[Roots],
    captures: &Facts,
    b: &mut Budget,
) -> CompileResult<Vec<Parents>> {
    b.spend(
        f.values.len().saturating_add(f.blocks.len()),
        "external lending ancestry tables",
    )?;
    let mut parents = vec![Parents::new(); f.values.len()];
    let mut outputs = vec![AddressLocals::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for (block_index, block) in f.blocks.iter().enumerate() {
            b.spend(1, "external lending ancestry iteration")?;
            if !reachable[block_index] {
                continue;
            }
            let mut state = AddressLocals::new();
            for predecessor in &predecessors[block_index] {
                b.spend(1, "external lending ancestry predecessor")?;
                if !reachable[*predecessor] {
                    continue;
                }
                b.spend(
                    outputs[*predecessor].len(),
                    "external lending ancestry local merge",
                )?;
                for (local, row) in &outputs[*predecessor] {
                    b.lookup(state.len(), "external lending ancestry local lookup")?;
                    parent_join(state.entry(*local).or_default(), row, b)?;
                }
            }
            let mut capture_state = input(captures, block_index, b)?;
            for instruction in &block.instructions {
                b.spend(1, "external lending ancestry instruction")?;
                if let Some(result) = instruction.result
                    && matches!(f.values[result.0 as usize], Type::Address { .. })
                {
                    let mut row = Parents::new();
                    match &instruction.kind {
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                            b.lookup(state.len(), "external lending ancestry load")?;
                            if let Some(source) = state.get(local) {
                                parent_join(&mut row, source, b)?;
                            }
                        }
                        InstructionKind::ReborrowAddress { address, .. }
                        | InstructionKind::BorrowPlaceAddress { address, .. }
                        | InstructionKind::BorrowVariantAddress { address, .. }
                        | InstructionKind::AddressField { address, .. }
                        | InstructionKind::AddressIndex { address, .. }
                        | InstructionKind::AddressSlice { address, .. } => {
                            row.insert(*address);
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            let returned = &summaries[function.0 as usize];
                            b.spend(returned.len(), "external lending actual return positions")?;
                            for source in returned {
                                let Root::Parameter(index) = source else {
                                    continue;
                                };
                                let argument = *arguments
                                    .get(*index as usize)
                                    .ok_or_else(|| bad("lending return argument missing"))?;
                                b.lookup(
                                    row.len(),
                                    "external lending delegated selected argument",
                                )?;
                                row.insert(argument);
                            }
                        }
                        InstructionKind::CallClosureShared { callee, .. }
                        | InstructionKind::CallClosureMut { callee, .. } => {
                            let d = entry(module, f, &instruction.kind)
                                .ok_or_else(|| bad("lending ancestry closure missing"))?;
                            b.lookup(
                                capture_state.len(),
                                "external lending ancestry selected local",
                            )?;
                            if let Some(slots) = capture_state.get(callee) {
                                selected_parents(d, slots, &mut row, b)?;
                            }
                        }
                        InstructionKind::CallClosureBorrowed { callee, .. } => {
                            let d = entry(module, f, &instruction.kind)
                                .ok_or_else(|| bad("borrowed lending ancestry closure missing"))?;
                            selected_parents(d, &captures.values[callee.0 as usize], &mut row, b)?;
                        }
                        _ => {}
                    }
                    changed |= parent_join(&mut parents[result.0 as usize], &row, b)?;
                }
                match instruction.kind {
                    InstructionKind::Store { local, value }
                        if matches!(f.values[value.0 as usize], Type::Address { .. }) =>
                    {
                        b.lookup(state.len(), "external lending ancestry store")?;
                        state.insert(local, Parents::from([value]));
                    }
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::Store { local, .. } => {
                        b.lookup(state.len(), "external lending ancestry end")?;
                        state.remove(&local);
                    }
                    _ => {}
                }
                local_step(&mut capture_state, instruction, &captures.values, b)?;
            }
            let before = copy_addresses(&outputs[block_index], b)?;
            let after = copy_addresses(&state, b)?;
            if before != after {
                outputs[block_index] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(parents)
}
fn selected_parents(
    d: &ClosureDefinition,
    slots: &Slots,
    to: &mut Parents,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(
        d.lending_captures.len(),
        "external lending parent source slots",
    )?;
    for index in &d.lending_captures {
        if matches!(d.captures[*index as usize], Type::Address { .. }) {
            b.lookup(slots.len(), "external lending parent selected slot")?;
            if let Some(slot) = slots.get(index) {
                parent_join(to, &slot.parents, b)?;
            }
        }
    }
    Ok(())
}
pub(super) fn delegated(
    f: &TypedFunction,
    child: ValueId,
    active: resources::Place,
    facts: &Facts,
    b: &mut Budget,
) -> CompileResult<bool> {
    let mut pending = vec![child];
    let mut seen = Parents::new();
    while let Some(value) = pending.pop() {
        b.lookup(seen.len(), "external lending delegation visited")?;
        if !seen.insert(value) {
            continue;
        }
        if super::delegated(f, value, active, b)? {
            return Ok(true);
        }
        let parents = &facts.parents[value.0 as usize];
        b.spend(parents.len(), "external lending delegation expansion")?;
        pending.extend(parents);
    }
    Ok(false)
}
