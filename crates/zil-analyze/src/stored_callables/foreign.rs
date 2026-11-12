//! Scalar FFI has no pointer permissions. Its lexical unsafe evidence still
//! comes from the real CFG, independently of Verify's unsafe-region checker.
use super::*;
use std::collections::{BTreeMap, VecDeque};

fn number(ty: &Type) -> bool {
    ty.is_integer() || ty.is_float() || *ty == Type::ForeignHandle
}

// Closed synchronous C data only. Pointer arguments are external noescape
// promises, not sources; their actual access is checked at each caller.
fn payload(ty: &Type, mutable: bool, depth: usize, b: &mut Budget) -> CompileResult<bool> {
    b.spend(1, "actual synchronous foreign payload node")?;
    if depth > b.partial_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "foreign payload depth",
            None,
        )]);
    }
    if number(ty) || !mutable && *ty == Type::CWideString {
        return Ok(true);
    }
    match ty {
        Type::Array { element, length } if *length != 0 => payload(element, mutable, depth + 1, b),
        Type::Record { name, fields } if !name.is_empty() && !fields.is_empty() => {
            b.spend(
                name.len().saturating_add(fields.len()),
                "actual foreign record inventory",
            )?;
            let mut names = std::collections::BTreeSet::new();
            for (name, ty) in fields {
                b.product(
                    name.len().saturating_add(1),
                    names.len().saturating_add(1),
                    1,
                    "actual foreign record field identities",
                )?;
                if name.is_empty() || !names.insert(name) || !payload(ty, mutable, depth + 1, b)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub(super) fn declaration(f: &TypedFunction, b: &mut Budget) -> CompileResult<bool> {
    let Some(declaration) = &f.foreign else {
        return Ok(false);
    };
    b.spend(
        f.parameters
            .len()
            .saturating_add(declaration.abi.len())
            .saturating_add(declaration.symbol.len())
            .saturating_add(1),
        "actual synchronous foreign declaration",
    )?;
    let bytes = declaration.symbol.as_bytes();
    if declaration.abi != "C"
        || declaration.symbol == "main"
        || !bytes
            .first()
            .is_some_and(|v| v.is_ascii_alphabetic() || *v == b'_')
        || !bytes
            .iter()
            .all(|v| v.is_ascii_alphanumeric() || *v == b'_')
        || declaration.static_callbacks
        || declaration.scoped_callback.is_some()
        || declaration.thread_driver_transfer
        || f.environment.is_some()
        || f.call_kind != CallKind::Direct
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.result_mode != zil_ir::PassingMode::Infer
        || !(f.return_type == Type::Unit || number(&f.return_type))
        || !f.blocks.is_empty()
        || !f.values.is_empty()
        || f.entry != zil_ir::BlockId(0)
        || f.parameters.len() != f.locals.len()
    {
        return Ok(false);
    }
    let mut borrowed = 0usize;
    for (index, (id, local)) in f.parameters.iter().zip(&f.locals).enumerate() {
        b.spend(1, "actual foreign parameter correspondence")?;
        if id.0 as usize != index || local.id != *id || local.mutable {
            return Ok(false);
        }
        if let Type::Address { mutable, inner } = &local.ty {
            b.spend(1, "actual foreign noescape position")?;
            if declaration.noescape_parameters.get(borrowed).copied() != Some(index as u32)
                || !payload(inner, *mutable, 0, b)?
            {
                return Ok(false);
            }
            borrowed += 1;
        } else if !(number(&local.ty) || local.ty == Type::CWideString) {
            return Ok(false);
        }
    }
    Ok(borrowed == declaration.noescape_parameters.len())
}

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
