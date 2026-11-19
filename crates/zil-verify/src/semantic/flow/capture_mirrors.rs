//! A mutable inline capture is copied into a local on entry. Each projected
//! assignment publishes the same rebuilt value both locally and to the entry's
//! environment slot. Its original root is the unique initial ReadCapture.
use super::*;

pub(super) fn initial(
    producers: &sites::Producers,
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    b: &mut Budget,
) -> Check<Option<ValueId>> {
    let Some(declaration) = f.locals.get(local.0 as usize) else {
        return Err(bad("capture mirror local is absent"));
    };
    if !declaration.mutable || !matches!(declaration.ty, Type::Tuple(_) | Type::Record { .. }) {
        return Ok(None);
    }
    let entry = f
        .blocks
        .get(f.entry.0 as usize)
        .filter(|block| block.id == f.entry)
        .ok_or_else(|| bad("capture mirror has no actual entry block"))?;
    b.spend(entry.instructions.len())?;
    let Some(start) = entry
        .instructions
        .iter()
        .position(|op| op.kind == InstructionKind::StorageLive(local))
    else {
        return Ok(None);
    };
    let Some([read, store]) = entry.instructions.get(start + 1..start + 3) else {
        return Ok(None);
    };
    let (InstructionKind::ReadCapture { index: capture }, Some(initial)) =
        (&read.kind, read.result)
    else {
        return Ok(None);
    };
    if store.kind
        != (InstructionKind::Store {
            local,
            value: initial,
        })
    {
        return Ok(None);
    }
    let mut publishes = false;
    for block in &f.blocks {
        b.spend(block.instructions.len())?;
        publishes |= block.instructions.windows(2).any(|pair| matches!(
            (&pair[0].kind, &pair[1].kind),
            (InstructionKind::Store { local: id, value }, InstructionKind::WriteCapture { index, value: written })
                if id == &local && index == capture && value == written
        ));
    }
    if !publishes {
        return Ok(None);
    }
    if !crate::stored_callables::closed_inline_product(module, &declaration.ty, b)? {
        return Ok(None);
    }

    let mut assignments = BTreeSet::new();
    for block in &f.blocks {
        for (at, op) in block.instructions.iter().enumerate() {
            b.spend(1)?;
            match op.kind {
                InstructionKind::StorageLive(id) if id == local => {
                    if (block.id, at) != (entry.id, start) {
                        return Err(bad("capture mirror has another storage activation"));
                    }
                }
                InstructionKind::Store { local: id, value } if id == local => {
                    if (block.id, at) == (entry.id, start + 2) {
                        continue;
                    }
                    let (update_block, update_at, update) = producers.get(f, value, b)?;
                    let InstructionKind::UpdateField { aggregate, .. } = update.kind else {
                        return Err(bad("capture mirror replacement has no projected update"));
                    };
                    let (read_block, read_at, read) = producers.get(f, aggregate, b)?;
                    if update_block != block.id
                        || update_at as usize + 1 != at
                        || read_block != block.id
                        || read_at >= update_at
                        || read.kind != InstructionKind::Load(local)
                        || update.span != op.span
                    {
                        return Err(bad("capture mirror rebuild changes its source snapshot"));
                    }
                    let middle = block
                        .instructions
                        .get(read_at as usize + 1..at)
                        .ok_or_else(|| {
                            bad("capture mirror rebuild has an invalid actual instruction range")
                        })?;
                    b.spend(middle.len())?;
                    if middle.iter().any(|op| matches!(op.kind, InstructionKind::Store { local: id, .. } if id == local)) {
                        return Err(bad("capture mirror rebuild uses a superseded local snapshot"));
                    }
                    if block.instructions.get(at + 1).is_none_or(|write| {
                        write.kind
                            != (InstructionKind::WriteCapture {
                                index: *capture,
                                value,
                            })
                            || write.span != op.span
                    }) || !assignments.insert(op.span)
                    {
                        return Err(bad(
                            "capture mirror update has no unique matching environment publication",
                        ));
                    }
                }
                InstructionKind::WriteCapture { index, value } if index == *capture => {
                    if at == 0
                        || block.instructions[at - 1].kind
                            != (InstructionKind::Store { local, value })
                        || block.id == entry.id && at == start + 3
                    {
                        return Err(bad(
                            "capture environment is written outside its mirror update",
                        ));
                    }
                }
                InstructionKind::TakeCapture { index }
                | InstructionKind::BorrowCapture { index, .. }
                | InstructionKind::DescribeCapture { index }
                | InstructionKind::BindPackageCapture { index, .. }
                    if index == *capture =>
                {
                    return Err(bad("capture mirror has another environment access path"));
                }
                InstructionKind::Load(id) | InstructionKind::StorageDead(id) if id == local => {}
                _ => {
                    for id in op.kind.local_operands() {
                        b.spend(1)?;
                        if id == local {
                            return Err(bad("capture mirror has another local access path"));
                        }
                    }
                }
            }
        }
    }
    Ok((!assignments.is_empty()).then_some(initial))
}

/// A spill belongs to one entry activation. Scalar writes must publish to
/// the same capture. A stored address can only be loaded unchanged; borrowing
/// its slot would select another indirection. The CFG proves each actual loan.
pub(super) fn activation(
    producers: &sites::Producers, f: &TypedFunction, local: LocalId, b: &mut Budget,
) -> Check<Option<u32>> {
    let declaration = f.locals.get(local.0 as usize).ok_or_else(|| bad("forwarded capture local is absent"))?;
    let address = matches!(declaration.ty, Type::Address { .. });
    if !address && !declaration.ty.is_b0_scalar() { return Ok(None); }
    let entry = f.blocks.get(f.entry.0 as usize).filter(|block| block.id == f.entry)
        .ok_or_else(|| bad("capture spill has no actual entry block"))?;
    b.spend(entry.instructions.len())?;
    let Some(start) = entry.instructions.iter().position(|op| op.kind == InstructionKind::StorageLive(local))
    else { return Ok(None); };
    let Some([read, initial]) = entry.instructions.get(start + 1..start + 3) else { return Ok(None); };
    let (InstructionKind::ReadCapture { index }, Some(value)) = (&read.kind, read.result) else { return Ok(None); };
    if initial.kind != (InstructionKind::Store { local, value }) { return Ok(None); }
    for block in &f.blocks {
        b.spend(block.instructions.len())?;
        for (at, op) in block.instructions.iter().enumerate() {
            match op.kind {
                InstructionKind::StorageLive(id) if id == local && !(block.id == entry.id && at == start) => return Ok(None),
                InstructionKind::BorrowLocal { local: id } | InstructionKind::BorrowLocalMut { local: id }
                    if address && id == local => return Ok(None),
                InstructionKind::Store { local: id, value: rhs } if id == local && !(block.id == entry.id && at == start + 2) => {
                    if address || block.instructions.get(at + 1).is_none_or(|next| next.kind !=
                        (InstructionKind::WriteCapture { index: *index, value: rhs })) { return Ok(None); }
                }
                InstructionKind::WriteCapture { index: actual, value: rhs } if actual == *index => {
                    if address { return Ok(None); }
                    let prior = at.checked_sub(1).and_then(|at| block.instructions.get(at));
                    if prior.is_some_and(|previous| previous.kind == (InstructionKind::Store { local, value: rhs })) { continue; }
                    let (_, _, load) = producers.get(f, rhs, b)?;
                    if prior.is_none_or(|previous| previous.result != Some(rhs)) || load.kind != InstructionKind::Load(local) {
                        return Ok(None);
                    }
                }
                InstructionKind::StorageLive(_) | InstructionKind::Store { .. }
                | InstructionKind::BorrowLocal { .. } | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::Load(_) | InstructionKind::StorageDead(_) => {}
                _ => {
                    for id in op.kind.local_operands() {
                        b.spend(1)?;
                        if id == local { return Ok(None); }
                    }
                }
            }
        }
    }
    Ok(Some(*index))
}
