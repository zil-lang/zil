//! Actual reaching pointer definitions. This records loan ancestry, never
//! targets or permission inferred from a common root, class, or local name.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Anchor {
    Parameter(LocalId),
    Value(ValueId),
}
type Heads = BTreeSet<Anchor>;
pub(super) type Locals = BTreeMap<LocalId, Heads>;
pub(super) struct Facts {
    heads: Vec<Heads>,
    parents: Vec<Heads>,
    inputs: Vec<Locals>,
}
fn union(to: &mut Heads, from: &Heads, b: &mut Budget) -> CompileResult<bool> {
    b.spend(from.len(), "local pointer reaching definition union")?;
    let mut changed = false;
    for node in from {
        b.lookup(to.len(), "local pointer definition insertion")?;
        changed |= to.insert(*node);
    }
    Ok(changed)
}
fn cost(state: &Locals, b: &mut Budget) -> CompileResult<()> {
    b.spend(state.len(), "local pointer state")?;
    for row in state.values() {
        b.spend(row.len(), "local pointer state definitions")?;
    }
    Ok(())
}
pub(super) fn input(facts: &Facts, block: usize, b: &mut Budget) -> CompileResult<Locals> {
    cost(&facts.inputs[block], b)?;
    Ok(facts.inputs[block].clone())
}
pub(super) fn step(
    state: &mut Locals,
    i: &Instruction,
    facts: &Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    match i.kind {
        InstructionKind::Store { local, value } => {
            b.lookup(state.len(), "local pointer definition replacement")?;
            let row = &facts.heads[value.0 as usize];
            b.spend(row.len(), "local pointer definition copy")?;
            if row.is_empty() {
                state.remove(&local);
            } else {
                state.insert(local, row.clone());
            }
        }
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::TakeLocal { local }
        | InstructionKind::BindPackageCapture { local, .. }
        | InstructionKind::EndPackageCapture { local, .. } => {
            b.lookup(state.len(), "local pointer definition kill")?;
            state.remove(&local);
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn reconstruct(
    f: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    summaries: &[Roots],
    captures: Option<&captured_lending::Facts>,
    b: &mut Budget,
) -> CompileResult<Option<Facts>> {
    let mut needed = false;
    b.spend(f.blocks.len(), "local pointer transport detection")?;
    for (block, live) in f.blocks.iter().zip(reachable) {
        if !live {
            continue;
        }
        b.spend(block.instructions.len(), "actual local pointer transport")?;
        needed |= block.instructions.iter().any(|i| matches!(i.kind,
            InstructionKind::Store { value, .. } if matches!(f.values[value.0 as usize], Type::Address { .. })));
    }
    if !needed {
        return Ok(None);
    }
    b.spend(
        f.values
            .len()
            .saturating_mul(2)
            .saturating_add(f.blocks.len().saturating_mul(2)),
        "local pointer ancestry tables",
    )?;
    let mut facts = Facts {
        heads: vec![Heads::new(); f.values.len()],
        parents: vec![Heads::new(); f.values.len()],
        inputs: vec![Locals::new(); f.blocks.len()],
    };
    let mut outputs = vec![Locals::new(); f.blocks.len()];
    let mut initial = Locals::new();
    b.spend(f.parameters.len(), "incoming pointer definition anchors")?;
    for local in &f.parameters {
        if matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) {
            b.lookup(initial.len(), "incoming pointer anchor insertion")?;
            initial.insert(*local, Heads::from([Anchor::Parameter(*local)]));
        }
    }
    // A real non-Load definition has its own identity. This supplies no target
    // or source: those still require the independent executable origin proof.
    for (block, live) in f.blocks.iter().zip(reachable) {
        if !live {
            continue;
        }
        b.spend(
            block.instructions.len(),
            "actual pointer definition identities",
        )?;
        for i in &block.instructions {
            if let Some(v) = i.result
                && matches!(f.values[v.0 as usize], Type::Address { .. })
                && !matches!(
                    i.kind,
                    InstructionKind::Load(_) | InstructionKind::TakeLocal { .. }
                )
            {
                b.spend(1, "pointer definition identity")?;
                facts.heads[v.0 as usize].insert(Anchor::Value(v));
            }
        }
    }
    loop {
        let mut changed = false;
        for (block_index, block) in f.blocks.iter().enumerate() {
            b.spend(1, "reaching pointer CFG iteration")?;
            if !reachable[block_index] {
                continue;
            }
            let mut state = if block_index == f.entry.0 as usize {
                cost(&initial, b)?;
                initial.clone()
            } else {
                Locals::new()
            };
            b.spend(
                predecessors[block_index].len(),
                "reaching pointer predecessor scan",
            )?;
            for pred in &predecessors[block_index] {
                if !reachable[*pred] {
                    continue;
                }
                b.spend(outputs[*pred].len(), "reaching pointer predecessor state")?;
                for (local, row) in &outputs[*pred] {
                    b.lookup(state.len(), "reaching pointer predecessor local")?;
                    union(state.entry(*local).or_default(), row, b)?;
                }
            }
            cost(&state, b)?;
            facts.inputs[block_index] = state.clone();
            for i in &block.instructions {
                b.spend(1, "reaching pointer instruction")?;
                if let Some(v) = i.result
                    && matches!(f.values[v.0 as usize], Type::Address { .. })
                {
                    let mut parents = Heads::new();
                    match &i.kind {
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                            b.lookup(state.len(), "actual pointer reaching load")?;
                            if let Some(row) = state.get(local) {
                                changed |= union(&mut facts.heads[v.0 as usize], row, b)?;
                            }
                        }
                        InstructionKind::ReborrowAddress { address, .. }
                        | InstructionKind::BorrowPlaceAddress { address, .. }
                        | InstructionKind::BorrowVariantAddress { address, .. }
                        | InstructionKind::AddressField { address, .. }
                        | InstructionKind::AddressIndex { address, .. }
                        | InstructionKind::AddressSlice { address, .. } => {
                            union(&mut parents, &facts.heads[address.0 as usize], b)?;
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            b.spend(
                                summaries[function.0 as usize].len(),
                                "actual pointer return source positions",
                            )?;
                            for source in &summaries[function.0 as usize] {
                                if let Root::Parameter(index) = source {
                                    let arg = *arguments
                                        .get(*index as usize)
                                        .ok_or_else(|| bad("pointer return argument missing"))?;
                                    union(&mut parents, &facts.heads[arg.0 as usize], b)?;
                                }
                            }
                        }
                        InstructionKind::CallClosureShared { .. }
                        | InstructionKind::CallClosureMut { .. }
                        | InstructionKind::CallClosureBorrowed { .. } => {
                            if let Some(captures) = captures {
                                b.spend(
                                    captures.parents[v.0 as usize].len(),
                                    "actual captured pointer parents",
                                )?;
                                for source in &captures.parents[v.0 as usize] {
                                    union(&mut parents, &facts.heads[source.0 as usize], b)?;
                                }
                            }
                        }
                        _ => {}
                    }
                    changed |= union(&mut facts.parents[v.0 as usize], &parents, b)?;
                }
                step(&mut state, i, &facts, b)?;
            }
            cost(&state, b)?;
            cost(&outputs[block_index], b)?;
            if state != outputs[block_index] {
                outputs[block_index] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(Some(facts))
}
pub(super) fn delegated(
    child: ValueId,
    active: resources::Place,
    state: &Locals,
    facts: &Facts,
    b: &mut Budget,
) -> CompileResult<bool> {
    let heads = match active {
        resources::Place::Value(v) => &facts.heads[v.0 as usize],
        resources::Place::Local(l) => {
            b.lookup(state.len(), "active pointer reaching definition")?;
            let Some(heads) = state.get(&l) else {
                return Ok(false);
            };
            heads
        }
        _ => return Ok(false),
    };
    if heads.is_empty() {
        return Ok(false);
    }
    b.spend(
        facts.heads[child.0 as usize].len(),
        "actual pointer ancestry frontier",
    )?;
    let mut pending: Vec<_> = facts.heads[child.0 as usize].iter().copied().collect();
    let mut ancestors = Heads::new();
    while let Some(node) = pending.pop() {
        b.lookup(ancestors.len(), "actual pointer ancestry visit")?;
        if !ancestors.insert(node) {
            continue;
        }
        if let Anchor::Value(v) = node {
            b.spend(
                facts.parents[v.0 as usize].len(),
                "actual pointer ancestry parents",
            )?;
            pending.extend(facts.parents[v.0 as usize].iter().copied());
        }
    }
    b.spend(heads.len(), "all reaching active pointer parents")?;
    for node in heads {
        b.lookup(ancestors.len(), "active pointer ancestor qualification")?;
        if !ancestors.contains(node) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn consumed_arguments(kind: &InstructionKind) -> impl Iterator<Item = ValueId> + '_ {
    let (callee, values): (Option<ValueId>, &[ValueId]) = match kind {
        InstructionKind::Store { value, .. }
        | InstructionKind::StoreOwnedArrayElement { value, .. }
        | InstructionKind::StorePackageField { value, .. } => (None, std::slice::from_ref(value)),
        InstructionKind::MakeOwnedArray { elements }
        | InstructionKind::MakePackage { fields: elements } => (None, elements),
        InstructionKind::CallClosureOnce { callee, arguments } => (Some(*callee), arguments),
        InstructionKind::WithForeignCallback { body, .. }
        | InstructionKind::WithScopedThread { body, .. }
        | InstructionKind::WithSyncCell { body, .. } => (Some(*body), &[]),
        InstructionKind::Call { arguments, .. }
        | InstructionKind::CallDynSlot { arguments, .. }
        | InstructionKind::CallIndirect { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallClosure { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. }
        | InstructionKind::CallClosureBorrowed { arguments, .. } => (None, arguments),
        InstructionKind::MakeClosure { captures, .. } => (None, captures),
        _ => (None, &[]),
    };
    callee.into_iter().chain(values.iter().copied())
}

/// Address SSA is affine even though its target and parent loan can outlive
/// transport. A consumed SSA definition cannot transfer that qualification twice.
pub(super) fn check_transfers(
    f: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(f.values.len(), "actual mutable pointer SSA classification")?;
    let mutable: BTreeSet<_> = f
        .values
        .iter()
        .enumerate()
        .filter_map(|(v, ty)| {
            matches!(ty, Type::Address { mutable: true, .. }).then_some(ValueId(v as u32))
        })
        .collect();
    if mutable.is_empty() {
        return Ok(());
    }
    b.spend(
        f.blocks.len().saturating_mul(2),
        "pointer transfer CFG tables",
    )?;
    let mut inputs = vec![BTreeSet::new(); f.blocks.len()];
    let mut outputs = inputs.clone();
    let mut views = BTreeSet::new();
    for (block, live) in f.blocks.iter().zip(reachable) {
        if !live {
            continue;
        }
        b.spend(block.instructions.len(), "pointer transfer producer kinds")?;
        for i in &block.instructions {
            if let Some(value) = i.result {
                b.lookup(mutable.len(), "pointer transfer producer type")?;
                if mutable.contains(&value)
                    && matches!(
                        i.kind,
                        InstructionKind::Load(_)
                            | InstructionKind::AddressField { .. }
                            | InstructionKind::AddressIndex { .. }
                            | InstructionKind::AddressSlice { .. }
                    )
                {
                    b.lookup(views.len(), "pointer access view insertion")?;
                    views.insert(value);
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for (bi, block) in f.blocks.iter().enumerate() {
            b.spend(1, "pointer transfer CFG iteration")?;
            if !reachable[bi] {
                continue;
            }
            let mut state = BTreeSet::new();
            b.spend(predecessors[bi].len(), "pointer transfer predecessors")?;
            for pred in &predecessors[bi] {
                if !reachable[*pred] {
                    continue;
                }
                b.spend(
                    outputs[*pred].len(),
                    "pointer transfer predecessor qualifications",
                )?;
                for value in &outputs[*pred] {
                    b.lookup(state.len(), "pointer transfer join")?;
                    state.insert(*value);
                }
            }
            b.spend(state.len(), "pointer transfer input snapshot")?;
            inputs[bi] = state.clone();
            for i in &block.instructions {
                b.spend(1, "pointer transfer instruction")?;
                if let Some(value) = i.result {
                    b.lookup(state.len(), "fresh pointer definition")?;
                    state.remove(&value);
                }
                for value in consumed_arguments(&i.kind) {
                    b.lookup(mutable.len(), "pointer consumption kind")?;
                    if mutable.contains(&value) {
                        b.lookup(state.len(), "pointer consumption state")?;
                        state.insert(value);
                    }
                }
            }
            b.spend(
                state.len().saturating_add(outputs[bi].len()),
                "pointer transfer frontier equality",
            )?;
            if state != outputs[bi] {
                outputs[bi] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let available = |value, state: &BTreeSet<ValueId>, b: &mut Budget| -> CompileResult<()> {
        b.lookup(state.len(), "pointer SSA availability")?;
        if state.contains(&value) {
            return Err(bad("mutable address SSA was already transferred"));
        }
        Ok(())
    };
    for (bi, block) in f.blocks.iter().enumerate() {
        if !reachable[bi] {
            continue;
        }
        b.spend(inputs[bi].len(), "pointer transfer checked input")?;
        let mut state = inputs[bi].clone();
        for i in &block.instructions {
            b.spend(1, "pointer transfer checked instruction")?;
            for value in resources::uses(&i.kind) {
                available(value, &state, b)?;
            }
            if let Some(value) = i.result {
                b.lookup(state.len(), "pointer SSA regeneration")?;
                state.remove(&value);
            }
            for value in consumed_arguments(&i.kind) {
                b.lookup(mutable.len(), "actual pointer consumption")?;
                if !mutable.contains(&value) {
                    continue;
                }
                available(value, &state, b)?;
                b.lookup(views.len(), "actual pointer transfer view")?;
                if views.contains(&value) {
                    return Err(bad(
                        "pointer access view requires an actual take or child reborrow before transfer",
                    ));
                }
                b.lookup(state.len(), "checked pointer consumption")?;
                state.insert(value);
            }
        }
        if let Terminator::Return(Some(value)) = block.terminator {
            available(value, &state, b)?;
            b.lookup(views.len(), "pointer return access view")?;
            if views.contains(&value) {
                return Err(bad("pointer access view cannot transfer as a result"));
            }
        }
    }
    Ok(())
}
