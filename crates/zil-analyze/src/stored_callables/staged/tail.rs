//! Bounded tail fork. Transfer recovery remains wholly within the entry block.
use super::*;
use zil_ir::{BasicBlock, ValueId};

pub(super) fn entry<'a>(f: &'a TypedFunction, b: &mut Budget) -> CompileResult<&'a BasicBlock> {
    b.spend(f.blocks.len(), "staged tail topology inventory")?;
    if f.blocks.len() != 3 {
        return Err(closed());
    }
    let entry = f
        .blocks
        .iter()
        .find(|bb| bb.id == f.entry)
        .ok_or_else(closed)?;
    let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = entry.terminator
    else {
        return Err(closed());
    };
    if then_block == else_block || then_block == f.entry || else_block == f.entry {
        return Err(closed());
    }
    let mut ids = [0usize; 3];
    let mut calls = 0usize;
    for block in &f.blocks {
        let slot = if block.id == f.entry {
            0
        } else if block.id == then_block {
            1
        } else if block.id == else_block {
            2
        } else {
            return Err(closed());
        };
        ids[slot] += 1;
        if slot != 0 && !matches!(block.terminator, Terminator::Return(_) | Terminator::Trap) {
            return Err(closed());
        }
        b.spend(
            block.instructions.len(),
            "staged tail whole-body instructions",
        )?;
        for i in &block.instructions {
            // This closed inventory excludes indirect writes and hidden calls.
            match &i.kind {
                InstructionKind::CallClosureOnce { arguments, .. } => {
                    b.spend(arguments.len(), "staged tail actual call operands")?;
                    calls += 1;
                    if slot != 0 {
                        return Err(closed());
                    }
                }
                InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::DropClosure { .. }
                | InstructionKind::Constant(_)
                | InstructionKind::Load(_)
                | InstructionKind::TakeLocal { .. }
                | InstructionKind::Store { .. }
                | InstructionKind::ReborrowAddress { mutable: false, .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::ConvertNumber { .. } => {}
                _ => return Err(closed()),
            }
            if slot == 0 {
                continue;
            }
            if i.result
                .is_some_and(|v| matches!(f.values.get(v.0 as usize), Some(Type::Closure(_))))
            {
                return Err(closed());
            }
            let target = match i.kind {
                InstructionKind::StorageLive(local)
                | InstructionKind::Load(local)
                | InstructionKind::TakeLocal { local }
                | InstructionKind::Store { local, .. } => Some(local),
                _ => None,
            };
            if target.is_some_and(|local| {
                matches!(
                    f.locals.get(local.0 as usize).map(|l| &l.ty),
                    Some(Type::Closure(_))
                )
            }) {
                return Err(closed());
            }
        }
    }
    if ids != [1, 1, 1] || calls != 1 {
        return Err(closed());
    }
    Ok(entry)
}

// Called only after entry's closed inventory. No operand vector is allocated.
fn uses(kind: &InstructionKind, value: ValueId) -> usize {
    match kind {
        InstructionKind::Store { value: v, .. }
        | InstructionKind::ConvertInteger { value: v, .. }
        | InstructionKind::ConvertNumber { value: v, .. }
        | InstructionKind::Unary { operand: v, .. }
        | InstructionKind::ReborrowAddress { address: v, .. } => usize::from(*v == value),
        InstructionKind::Binary { left, right, .. } => {
            usize::from(*left == value) + usize::from(*right == value)
        }
        InstructionKind::CallClosureOnce { callee, arguments } => {
            usize::from(*callee == value) + arguments.iter().filter(|v| **v == value).count()
        }
        _ => 0,
    }
}

pub(super) fn unique_use(
    f: &TypedFunction,
    value: ValueId,
    before: usize,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(f.blocks.len(), "staged tail SSA block inventory")?;
    let mut count = 0usize;
    let mut definitions = 0usize;
    for block in &f.blocks {
        b.spend(block.instructions.len(), "staged tail SSA whole-body scan")?;
        for (index, i) in block.instructions.iter().enumerate() {
            if let InstructionKind::CallClosureOnce { arguments, .. } = &i.kind {
                b.spend(arguments.len(), "staged tail SSA call operands")?;
            }
            definitions += usize::from(i.result == Some(value));
            let n = uses(&i.kind, value);
            if n != 0 && (block.id != f.entry || index != before) {
                return Err(closed());
            }
            count += n;
        }
        if matches!(block.terminator, Terminator::Return(Some(v)) if v == value)
            || matches!(block.terminator, Terminator::Branch { condition, .. } if condition == value)
        {
            return Err(closed());
        }
    }
    if count != 1 || definitions != 1 {
        return Err(closed());
    }
    Ok(())
}
