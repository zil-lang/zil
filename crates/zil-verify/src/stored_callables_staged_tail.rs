//! Independent finite tail-fork inventory; this authenticates no ownership fact.
use super::*;

pub(super) fn entry<'a>(f: &'a TypedFunction, b: &mut Budget) -> Check<&'a BasicBlock> {
    b.spend(f.blocks.len().saturating_mul(4).saturating_add(1))?;
    if f.blocks.len() != 3 {
        return Err(bad("staged tail requires entry and two terminal leaves"));
    }
    // Every block has exactly one real identity, including unreachable input.
    for (index, block) in f.blocks.iter().enumerate() {
        if f.blocks[..index].iter().any(|other| other.id == block.id) {
            return Err(bad("staged tail has duplicate block identity"));
        }
    }
    let entry = f
        .blocks
        .iter()
        .find(|block| block.id == f.entry)
        .ok_or_else(|| bad("staged tail has no actual entry"))?;
    let Terminator::Branch {
        then_block,
        else_block,
        ..
    } = entry.terminator
    else {
        return Err(bad("staged tail entry must branch after the owning call"));
    };
    if then_block == else_block || then_block == entry.id || else_block == entry.id {
        return Err(bad("staged tail aliases an entry or exit"));
    }
    let mut calls = 0usize;
    for block in &f.blocks {
        if block.id != entry.id
            && (block.id != then_block && block.id != else_block
                || !matches!(block.terminator, Terminator::Return(_) | Terminator::Trap))
        {
            return Err(bad(
                "staged tail has an unreachable block, merge or backedge",
            ));
        }
        b.spend(block.instructions.len())?;
        for instruction in &block.instructions {
            if !super::staged::finite_instruction(&instruction.kind) {
                return Err(bad("staged tail contains unsupported storage effects"));
            }
            if matches!(instruction.kind, InstructionKind::CallClosureOnce { .. }) {
                calls += 1;
                if block.id != entry.id {
                    return Err(bad("staged tail has a call outside its entry"));
                }
            }
            if block.id == entry.id {
                continue;
            }
            if instruction
                .result
                .is_some_and(|v| matches!(f.values.get(v.0 as usize), Some(Type::Closure(_))))
            {
                return Err(bad("staged tail defines another owning SSA"));
            }
            // Drop/Dead are allowed only on these terminal successors. No leaf
            // can precede or return to the unique consuming entry transfer.
            let local = match instruction.kind {
                InstructionKind::StorageLive(local)
                | InstructionKind::Load(local)
                | InstructionKind::TakeLocal { local }
                | InstructionKind::Store { local, .. } => Some(local),
                _ => None,
            };
            if local.is_some_and(|l| {
                matches!(
                    f.locals.get(l.0 as usize).map(|l| &l.ty),
                    Some(Type::Closure(_))
                )
            }) {
                return Err(bad("staged tail reactivates or accesses an owning local"));
            }
        }
    }
    if calls != 1 {
        return Err(bad("staged tail lacks one unique actual consuming call"));
    }
    Ok(entry)
}

pub(super) fn value(
    f: &TypedFunction,
    current: ValueId,
    before: usize,
    b: &mut Budget,
) -> Check<()> {
    b.spend(f.blocks.len())?;
    let mut definitions = 0usize;
    let mut uses = 0usize;
    for block in &f.blocks {
        b.spend(block.instructions.len())?;
        for (index, instruction) in block.instructions.iter().enumerate() {
            if instruction.result == Some(current) {
                definitions += 1;
            }
            for operand in instruction_operands(&instruction.kind) {
                b.spend(1)?;
                if operand != current {
                    continue;
                }
                uses += 1;
                if block.id != f.entry || index != before {
                    return Err(bad("staged tail SSA escapes its unique entry transfer"));
                }
            }
        }
        b.spend(1)?;
        let escape = match block.terminator {
            Terminator::Return(Some(value)) => value == current,
            Terminator::Branch { condition, .. } => condition == current,
            _ => false,
        };
        if escape {
            return Err(bad("staged tail SSA escapes in a terminator"));
        }
    }
    if definitions != 1 || uses != 1 {
        return Err(bad(
            "staged tail SSA needs one definition and consuming use",
        ));
    }
    Ok(())
}
