//! Reconstruct inline Tuple permissions from actual child layouts. Local
//! class ordinals remain child-local; physical paths distinguish their loans.
use super::*;

pub(super) fn slots(ty: &Type, b: &mut Budget) -> Check<Option<Vec<(Path, StoredBorrowMode)>>> {
    if !matches!(ty, Type::Tuple(_)) {
        return Ok(None);
    }
    let mut pending = vec![(ty, Vec::new(), 0usize)];
    let mut out = Vec::new();
    while let Some((current, prefix, depth)) = pending.pop() {
        b.spend(prefix.len().saturating_add(1))?;
        if depth > b.limits.max_call_depth {
            return Err(limit("compound permission type depth exceeded"));
        }
        match current {
            Type::Tuple(children) => {
                b.spend(
                    children
                        .len()
                        .saturating_mul(prefix.len().saturating_add(2)),
                )?;
                for (index, child) in children.iter().enumerate().rev() {
                    let mut path = prefix.clone();
                    path.push(PlaceProjection::Field(index as u32));
                    pending.push((child, path, depth + 1));
                }
            }
            Type::BorrowedStorage { life, .. } => {
                // Declaration correspondence is independently checked during
                // module qualification; no producer projection table is read.
                physical(current, &b.limits.clone(), b)?;
                if !life.retained.is_empty()
                    || life
                        .slots
                        .iter()
                        .any(|s| s.mode != StoredBorrowMode::SharedAddress)
                {
                    return Ok(None);
                }
                for slot in &life.slots {
                    b.spend(
                        prefix
                            .len()
                            .saturating_add(slot.path.len())
                            .saturating_add(1),
                    )?;
                    let mut path = prefix.clone();
                    path.extend(&slot.path);
                    out.push((path, slot.mode));
                }
            }
            other if other.is_copy_value() => {}
            _ => return Ok(None),
        }
    }
    Ok((!out.is_empty()).then_some(out))
}

pub(super) fn projection(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<bool> {
    let InstructionKind::ExtractField { aggregate, index } = i.kind else {
        return Ok(false);
    };
    let parent = vt(f, aggregate)?;
    if slots(parent, b)?.is_none() {
        return Ok(false);
    }
    let Type::Tuple(children) = parent else {
        unreachable!()
    };
    let child = children
        .get(index as usize)
        .ok_or_else(|| err("compound field ordinal is absent"))?;
    type_work(child, &b.limits.clone(), b)?;
    let actual = vt(
        f,
        i.result
            .ok_or_else(|| err("compound projection lacks result"))?,
    )?;
    type_work(actual, &b.limits.clone(), b)?;
    if actual != child {
        return Err(err(
            "compound projection changes its complete actual child type",
        ));
    }
    Ok(true)
}

pub(super) fn construct(
    f: &TypedFunction,
    i: &Instruction,
    fields: &[ValueId],
    values: &[Payload],
    out: &mut Payload,
    b: &mut Budget,
) -> Check<()> {
    let result = vt(
        f,
        i.result
            .ok_or_else(|| err("compound construction lacks result"))?,
    )?;
    if slots(result, b)?.is_none() {
        return Ok(());
    }
    let Type::Tuple(children) = result else {
        unreachable!()
    };
    b.spend(fields.len().saturating_add(children.len()))?;
    if fields.len() != children.len() {
        return Err(err("compound construction arity differs"));
    }
    for (index, (value, child)) in fields.iter().zip(children).enumerate() {
        type_work(child, &b.limits.clone(), b)?;
        type_work(vt(f, *value)?, &b.limits.clone(), b)?;
        if vt(f, *value)? != child {
            return Err(err("compound construction changes actual child type"));
        }
        for (tail, permission) in &values[value.0 as usize].leaves {
            b.spend(tail.len().saturating_add(1))?;
            let mut path = vec![PlaceProjection::Field(index as u32)];
            path.extend(tail);
            add_leaf(out, &path, permission, b)?;
        }
    }
    Ok(())
}

pub(super) fn seed(
    ty: &Type,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    let Some(slots) = slots(ty, b)? else {
        return Ok(());
    };
    for (path, mode) in slots {
        b.spend(path.len().saturating_mul(3).saturating_add(4))?;
        let source = Source {
            base,
            retained_port: None,
            leaf: path.clone(),
            inline: Vec::new(),
            heap_parents: Vec::new(),
            heap_element: None,
        };
        let id = loans.len();
        loans.push(Loan {
            sources: vec![source.clone()],
            parents: Vec::new(),
            exclusive: mode == StoredBorrowMode::ExclusiveAddress,
        });
        add_leaf(
            out,
            &path,
            &Permission {
                sources: vec![source],
                loans: vec![id],
            },
            b,
        )?;
    }
    Ok(())
}
