//! Producer-side frame dependency conservation. The input Hold facts were
//! reconstructed by Analyze from this module, never supplied by Verify.
//! A live owning carrier keeps its roots even when it has no later SSA use.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Value(ValueId),
    Local(LocalId),
}
type Roots = BTreeSet<HoldRoot>;
type State = BTreeMap<Owner, Roots>;

fn retained(ty: &Type) -> bool {
    match ty {
        Type::Managed(payload) => {
            matches!(payload.as_ref(), Type::Task { retained_life, .. } if !retained_life.ports.is_empty())
        }
        Type::ErrorUnion { result, .. } => retained(result),
        _ => false,
    }
}
fn fee(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "borrowed Task actual owner dependencies")
}
fn copy(state: &State, b: &mut Budget) -> CompileResult<State> {
    fee(b, state.len())?;
    for roots in state.values() {
        fee(b, roots.len())?;
    }
    Ok(state.clone())
}
fn merge(to: &mut State, from: &State, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (owner, roots) in from {
        fee(b, to.len().saturating_add(roots.len()).saturating_add(1))?;
        let entry = to.entry(*owner).or_default();
        let before = entry.len();
        entry.extend(roots);
        changed |= entry.len() != before;
    }
    Ok(changed)
}
fn take(state: &mut State, owner: Owner, b: &mut Budget) -> CompileResult<Roots> {
    fee(b, state.len().saturating_add(1))?;
    Ok(state.remove(&owner).unwrap_or_default())
}
fn put(state: &mut State, owner: Owner, roots: Roots, b: &mut Budget) -> CompileResult<()> {
    fee(b, state.len().saturating_add(roots.len()).saturating_add(1))?;
    if roots.is_empty() {
        state.remove(&owner);
    } else {
        state.insert(owner, roots);
    }
    Ok(())
}
fn actual(holds: &[ValueHold], value: ValueId, b: &mut Budget) -> CompileResult<Roots> {
    let row = holds
        .get(value.0 as usize)
        .ok_or_else(|| bad("Task dependency Hold value absent"))?;
    fee(b, row.roots.len().saturating_add(1))?;
    if row.value != value {
        return Err(bad("Task dependency Hold identity differs"));
    }
    Ok(row.roots.iter().copied().collect())
}

fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    holds: &[ValueHold],
    state: &mut State,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    let output = i.result.filter(|v| retained(&f.values[v.0 as usize]));
    let roots = match &i.kind {
        InstructionKind::TaskCreate { .. } if output.is_some() => {
            let roots = actual(holds, output.unwrap(), b)?;
            if roots.is_empty() {
                return Err(fail(
                    i.span,
                    "borrowed Task constructor lacks an actual input root",
                ));
            }
            Some(roots)
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            // Ownership arguments are discharged by the independently checked
            // callee. Its returned carrier starts with the actual return roots.
            for argument in arguments {
                fee(b, 1)?;
                if retained(&f.values[argument.0 as usize]) {
                    if module.functions[function.0 as usize].foreign.is_some() {
                        return Err(fail(
                            i.span,
                            "borrowed Task cannot escape through an unverified foreign owner transfer",
                        ));
                    }
                    take(state, Owner::Value(*argument), b)?;
                }
            }
            if output.is_some() && module.functions[function.0 as usize].foreign.is_some() {
                return Err(fail(
                    i.span,
                    "foreign Task result has no reconstructed frame dependency",
                ));
            }
            output.map(|v| actual(holds, v, b)).transpose()?
        }
        InstructionKind::Store { local, value } if retained(&f.locals[local.0 as usize].ty) => {
            let roots = take(state, Owner::Value(*value), b)?;
            put(state, Owner::Local(*local), roots, b)?;
            None
        }
        InstructionKind::TakeLocal { local } if retained(&f.locals[local.0 as usize].ty) => {
            Some(take(state, Owner::Local(*local), b)?)
        }
        InstructionKind::TakeVariantField { local, path }
            if retained(&f.locals[local.0 as usize].ty) =>
        {
            // The independent Sum pass has already checked the actual tag
            // guard and that this selective take consumes the owning shell.
            let roots = take(state, Owner::Local(*local), b)?;
            match path.as_slice() {
                [zil_ir::PlaceProjection::ErrorArm(false)] => Some(roots),
                [zil_ir::PlaceProjection::ErrorArm(true)] => None,
                _ => {
                    return Err(fail(
                        i.span,
                        "borrowed Task selective take has no complete error arm",
                    ));
                }
            }
        }
        InstructionKind::TakeErrorUnion { value, is_error }
            if retained(&f.values[value.0 as usize]) =>
        {
            let roots = take(state, Owner::Value(*value), b)?;
            if *is_error { None } else { Some(roots) }
        }
        InstructionKind::MakeErrorUnion { value, is_error } if output.is_some() => {
            Some(if *is_error {
                Roots::new()
            } else {
                take(state, Owner::Value(*value), b)?
            })
        }
        InstructionKind::TaskFinishDestroy { task } if retained(&f.values[task.0 as usize]) => {
            take(state, Owner::Value(*task), b)?;
            None
        }
        InstructionKind::DropManaged { local } | InstructionKind::DropErrorUnion { local }
            if retained(&f.locals[local.0 as usize].ty) =>
        {
            // Task admission separately proves the reachable teardown is
            // synchronous; only completion of this instruction releases it.
            take(state, Owner::Local(*local), b)?;
            None
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local)
            if retained(&f.locals[local.0 as usize].ty) =>
        {
            if state.contains_key(&Owner::Local(*local)) {
                return Err(fail(
                    i.span,
                    "Task storage lifetime ends before its actual owner is discharged",
                ));
            }
            None
        }
        InstructionKind::ErrorTag { .. } => None,
        _ if output.is_some() => {
            return Err(fail(
                i.span,
                "borrowed Task owner has no supported actual transfer",
            ));
        }
        _ => {
            for operand in resources::uses(&i.kind) {
                fee(b, 1)?;
                if retained(&f.values[operand.0 as usize]) {
                    return Err(fail(
                        i.span,
                        "borrowed Task transport requires a checked complete owner transfer",
                    ));
                }
            }
            None
        }
    };
    if let Some(value) = output {
        put(state, Owner::Value(value), roots.unwrap_or_default(), b)?;
    }
    Ok(())
}

// Distinct local storage is disjoint. Distinct incoming pointer spellings are
// not a separation proof; this first profile conservatively overlaps them.
fn overlaps(left: HoldRoot, right: HoldRoot) -> bool {
    left == right
        || matches!(
            (left, right),
            (HoldRoot::Parameter(_), HoldRoot::Parameter(_))
                | (HoldRoot::Parameter(_), HoldRoot::Capture(_))
                | (HoldRoot::Capture(_), HoldRoot::Parameter(_))
                | (HoldRoot::Capture(_), HoldRoot::Capture(_))
        )
}
fn exclusive(state: &State, roots: &Roots, i: &Instruction, b: &mut Budget) -> CompileResult<()> {
    fee(b, state.len())?;
    for held in state.values() {
        b.product(
            roots.len(),
            held.len(),
            1,
            "borrowed Task protected root overlap",
        )?;
        if roots
            .iter()
            .any(|root| held.iter().any(|held| overlaps(*root, *held)))
        {
            return Err(fail(
                i.span,
                "exclusive access or storage end conflicts with a live borrowed Task frame",
            ));
        }
    }
    Ok(())
}
fn access(
    f: &TypedFunction,
    i: &Instruction,
    holds: &[ValueHold],
    state: &State,
    b: &mut Budget,
) -> CompileResult<()> {
    if state.is_empty() {
        return Ok(());
    }
    fee(b, 1)?;
    let local = match i.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropTuple { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::BorrowFieldLocal {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowPlaceLocal {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowVariantLocal {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowPackageField {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowOwnedArrayElement {
            local,
            mutable: true,
            ..
        }
        | InstructionKind::StorePackageField { local, .. }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StoreOwnedArrayElement { local, .. }
        | InstructionKind::TakeOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArrayElement { local, .. } => Some(local),
        _ => None,
    };
    if let Some(local) = local {
        exclusive(state, &Roots::from([HoldRoot::Local(local)]), i, b)?;
    }
    match i.kind {
        InstructionKind::CallClosureShared { .. }
        | InstructionKind::CallClosureMut { .. }
        | InstructionKind::CallClosureOnce { .. } => {
            return Err(fail(
                i.span,
                "closure environment effects during a borrowed Task require the retained capture consumer",
            ));
        }
        InstructionKind::WriteCapture { index, .. }
        | InstructionKind::BorrowCapture {
            index,
            mutable: true,
        } => {
            exclusive(state, &Roots::from([HoldRoot::Capture(index)]), i, b)?;
        }
        InstructionKind::CommitPlaceReplace { reservation, .. } => {
            // A reservation may predate creation of the Task. Check its actual
            // destination at commit, rather than treating reservation as write.
            for block in &f.blocks {
                fee(b, block.instructions.len().saturating_add(1))?;
                for producer in &block.instructions {
                    match producer.kind {
                        InstructionKind::ReservePlaceLocal {
                            reservation: id,
                            local,
                            ..
                        } if id == reservation => {
                            exclusive(state, &Roots::from([HoldRoot::Local(local)]), i, b)?;
                        }
                        InstructionKind::ReservePlaceAddress {
                            reservation: id,
                            address,
                            ..
                        } if id == reservation => {
                            exclusive(state, &actual(holds, address, b)?, i, b)?;
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
    let address = match i.kind {
        InstructionKind::WriteAddress { address, .. }
        | InstructionKind::ReborrowAddress {
            address,
            mutable: true,
        }
        | InstructionKind::BorrowVariantAddress {
            address,
            mutable: true,
            ..
        }
        | InstructionKind::BorrowPlaceAddress {
            address,
            mutable: true,
            ..
        } => Some(address),
        _ => None,
    };
    if let Some(value) = address {
        exclusive(state, &actual(holds, value, b)?, i, b)?;
    }
    // Existing addresses can be used after Task construction. Checking only
    // BorrowLocalMut would miss writes through an earlier reference or helper.
    for value in resources::uses(&i.kind) {
        fee(b, 1)?;
        if matches!(
            &f.values[value.0 as usize],
            Type::Address { mutable: true, .. }
        ) {
            exclusive(state, &actual(holds, value, b)?, i, b)?;
        }
        if matches!(&f.values[value.0 as usize], Type::Closure(_)) {
            exclusive(state, &actual(holds, value, b)?, i, b)?;
        }
    }
    Ok(())
}

pub(super) fn check(
    module: &TypedModule,
    plan: &CandidatePlan,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<()> {
    for (fi, f) in module.functions.iter().enumerate() {
        fee(b, f.values.len().saturating_add(f.locals.len()))?;
        if !f.values.iter().any(retained) && !f.locals.iter().any(|l| retained(&l.ty)) {
            continue;
        }
        if f.foreign.is_some() {
            continue;
        }
        let holds = &plan.functions[fi].holds;
        let (predecessors, reachable) = &graphs[fi];
        fee(
            b,
            f.blocks
                .len()
                .saturating_mul(3)
                .saturating_add(f.values.len()),
        )?;
        let mut tags = vec![None; f.values.len()];
        for block in &f.blocks {
            for i in &block.instructions {
                fee(b, 1)?;
                if let (Some(result), InstructionKind::ErrorTag { value }) = (i.result, &i.kind) {
                    if retained(&f.values[value.0 as usize]) {
                        tags[result.0 as usize] = Some(*value);
                    }
                }
            }
        }
        let mut initial = State::new();
        for (index, local) in f.parameters.iter().enumerate() {
            fee(b, 1)?;
            if retained(&f.locals[local.0 as usize].ty) {
                put(
                    &mut initial,
                    Owner::Local(*local),
                    Roots::from([HoldRoot::Parameter(index as u32)]),
                    b,
                )?;
            }
        }
        let mut inputs = vec![State::new(); f.blocks.len()];
        let mut outputs = vec![State::new(); f.blocks.len()];
        loop {
            let mut changed = false;
            for (index, block) in f.blocks.iter().enumerate() {
                fee(b, 1)?;
                if !reachable[index] {
                    continue;
                }
                let mut state = if index == f.entry.0 as usize {
                    copy(&initial, b)?
                } else {
                    State::new()
                };
                for predecessor in &predecessors[index] {
                    fee(b, 1)?;
                    if !reachable[*predecessor] {
                        continue;
                    }
                    let mut edge = copy(&outputs[*predecessor], b)?;
                    if let Terminator::Branch {
                        condition,
                        then_block,
                        else_block,
                    } = f.blocks[*predecessor].terminator
                    {
                        // ErrorTag=true means no successful Task frame exists.
                        // An unknown condition never grants this refinement.
                        if then_block != else_block && then_block.0 as usize == index {
                            if let Some(value) = tags[condition.0 as usize] {
                                take(&mut edge, Owner::Value(value), b)?;
                            }
                        }
                    }
                    merge(&mut state, &edge, b)?;
                }
                inputs[index] = copy(&state, b)?;
                for i in &block.instructions {
                    transfer(module, f, i, holds, &mut state, b)?;
                }
                fee(b, state.len().saturating_add(outputs[index].len()))?;
                for roots in state.values().chain(outputs[index].values()) {
                    fee(b, roots.len())?;
                }
                if state != outputs[index] {
                    outputs[index] = state;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (index, block) in f.blocks.iter().enumerate() {
            if !reachable[index] {
                continue;
            }
            let mut state = copy(&inputs[index], b)?;
            for i in &block.instructions {
                access(f, i, holds, &state, b)?;
                transfer(module, f, i, holds, &mut state, b)?;
            }
        }
    }
    Ok(())
}
