//! Scalar FFI has no pointer permissions. Its lexical unsafe evidence still
//! comes from the real CFG, independently of Verify's unsafe-region checker.
use super::*;
use std::collections::{BTreeMap, VecDeque};

pub(crate) fn regions(
    f: &TypedFunction,
    preds: &[Vec<usize>],
    live: &[bool],
    b: &mut Budget,
) -> CompileResult<()> {
    let bad_region = || bad("scalar foreign operation lacks its actual lexical unsafe region");
    b.spend(
        f.blocks.len().saturating_mul(3),
        "scalar foreign region CFG storage",
    )?;
    let mut declarations = BTreeMap::new();
    for (index, block) in f.blocks.iter().enumerate() {
        for instruction in &block.instructions {
            b.spend(1, "scalar foreign lexical inventory")?;
            if matches!(
                instruction.kind,
                InstructionKind::UnsafeEnter { .. }
                    | InstructionKind::UnsafeExit { .. }
                    | InstructionKind::ForeignCall { .. }
            ) && !live[index]
            {
                return Err(bad_region());
            }
            if let InstructionKind::UnsafeEnter { region } = instruction.kind {
                b.spend(
                    declarations.len().saturating_add(1),
                    "scalar foreign lexical declaration",
                )?;
                if instruction.span.start > instruction.span.end
                    || declarations.insert(region, instruction.span).is_some()
                {
                    return Err(bad_region());
                }
            }
        }
    }
    let mut entries = vec![None::<Vec<u32>>; f.blocks.len()];
    let mut pending = VecDeque::with_capacity(f.blocks.len());
    let entry = f.entry.0 as usize;
    entries[entry] = Some(Vec::new());
    pending.push_back(entry);
    while let Some(index) = pending.pop_front() {
        let input = entries[index].as_ref().ok_or_else(bad_region)?;
        b.spend(
            input.len().saturating_add(1),
            "scalar foreign actual stack snapshot",
        )?;
        let mut stack = input.clone();
        for instruction in &f.blocks[index].instructions {
            b.spend(1, "scalar foreign actual region step")?;
            match instruction.kind {
                InstructionKind::UnsafeEnter { region } => {
                    b.spend(
                        stack
                            .len()
                            .saturating_add(declarations.len())
                            .saturating_add(1),
                        "scalar foreign region nesting",
                    )?;
                    if stack.contains(&region) {
                        return Err(bad_region());
                    }
                    if let Some(parent) = stack.last() {
                        let outer = declarations.get(parent).ok_or_else(bad_region)?;
                        if outer.file != instruction.span.file
                            || outer.start > instruction.span.start
                            || outer.end < instruction.span.end
                        {
                            return Err(bad_region());
                        }
                    }
                    stack.push(region);
                }
                InstructionKind::UnsafeExit { region } => {
                    if stack.pop() != Some(region) {
                        return Err(bad_region());
                    }
                }
                InstructionKind::ForeignCall { region, .. } => {
                    b.spend(
                        declarations.len().saturating_add(1),
                        "scalar foreign actual region anchor",
                    )?;
                    let span = declarations.get(&region).ok_or_else(bad_region)?;
                    if stack.last() != Some(&region)
                        || span.file != instruction.span.file
                        || span.start > instruction.span.start
                        || span.end < instruction.span.end
                    {
                        return Err(bad_region());
                    }
                }
                _ => {}
            }
        }
        if matches!(f.blocks[index].terminator, Terminator::Return(_)) && !stack.is_empty() {
            return Err(bad_region());
        }
        // Enumerating predecessor rows gives an independent transfer schedule;
        // a lexical stack has to agree exactly on every discovered CFG edge.
        for (next, parents) in preds.iter().enumerate() {
            b.spend(
                parents.len().saturating_add(1),
                "scalar foreign outgoing edge inverse",
            )?;
            if !parents.contains(&index) {
                continue;
            }
            b.spend(stack.len().saturating_add(1), "scalar foreign join stack")?;
            if let Some(old) = &entries[next] {
                b.spend(old.len(), "scalar foreign join comparison")?;
                if old != &stack {
                    return Err(bad_region());
                }
            } else {
                entries[next] = Some(stack.clone());
                pending.push_back(next);
            }
        }
    }
    Ok(())
}
