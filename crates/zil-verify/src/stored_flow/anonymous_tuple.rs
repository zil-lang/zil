//! Ordinary anonymous Tuple carriers are authenticated from their complete
//! physical type, independently of any Task declaration or producer flag.
use super::*;

pub(super) struct Layout<'a> {
    pub(super) fields: &'a [Type],
    pub(super) life: &'a StorageLifeShape,
    pub(super) owned: bool,
    pub(super) affine: bool,
}

pub(super) fn layout<'a>(ty: &'a Type, b: &mut Budget) -> Check<Option<Layout<'a>>> {
    let Type::BorrowedStorage { storage, life } = ty else {
        return Ok(None);
    };
    let Type::Tuple(fields) = storage.as_ref() else {
        return Ok(None);
    };
    type_work(ty, &b.limits.clone(), b)?;
    if !life.nominal_arguments.is_empty() {
        return Err(err(
            "anonymous Tuple cannot fabricate nominal lifetime arguments",
        ));
    }
    let mut expected = Vec::new();
    let mut retained = Vec::new();
    let mut owned = false;
    let mut nested_affine = false;
    for (index, child) in fields.iter().enumerate() {
        b.spend(1)?;
        let prefix = vec![PlaceProjection::Field(index as u32)];
        match child {
            Type::Address { mutable, inner } => {
                if !inner.is_address_pointee() {
                    return Err(err("anonymous Tuple has invalid actual pointee"));
                }
                expected.push((
                    prefix,
                    if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    None,
                ));
            }
            Type::Managed(payload)
                if payload.is_copy_value()
                    || matches!(payload.as_ref(), Type::ByteBuffer | Type::OwnedStore { .. }) =>
            {
                owned = true
            }
            Type::BorrowedStorage {
                life: child_life, ..
            } => {
                if let Some(child_layout) = layout(child, b)? {
                    owned |= child_layout.owned;
                    nested_affine |= child_layout.affine;
                } else if matches!(child, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))
                    || shared_record_child(child, b)?
                    || retained::pure(child, b)?
                {
                    // Pure nominal children can still have a nonempty phantom
                    // domain. Real owning Packages keep their actual debt.
                    physical(child, &b.limits.clone(), b)?;
                    let child_owned = crate::sums::owned(child);
                    if matches!(child, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))
                        && !child_owned
                    {
                        return Err(err("anonymous Tuple Package child has no actual owner"));
                    }
                    owned |= child_owned;
                    nested_affine |= child_owned
                        || child_life
                            .slots
                            .iter()
                            .any(|s| s.mode == StoredBorrowMode::ExclusiveAddress);
                } else {
                    return Ok(None);
                }
                let boundary = vec![LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                ))];
                b.spend(child_life.binder_count as usize)?;
                for class in 0..child_life.binder_count {
                    retained.push((boundary.clone(), class));
                }
                for slot in &child_life.slots {
                    b.spend(slot.path.len().saturating_add(3))?;
                    let mut complete = prefix.clone();
                    complete.extend(&slot.path);
                    expected.push((complete, slot.mode, Some((boundary.clone(), slot.binder))));
                }
            }
            child if child.is_copy_value() => {}
            _ => return Ok(None),
        }
    }
    if expected.is_empty() && retained.is_empty() {
        return Ok(None);
    }
    b.spend(
        expected
            .len()
            .saturating_add(life.slots.len())
            .saturating_add(retained.len())
            .saturating_add(life.retained.len()),
    )?;
    if expected.len() != life.slots.len() || retained.len() != life.retained.len() {
        return Err(err(
            "anonymous Tuple omits actual address or child-domain slots",
        ));
    }
    for ((path, class), actual) in retained.iter().zip(&life.retained) {
        b.spend(
            path.len()
                .saturating_add(actual.path.len())
                .saturating_add(1),
        )?;
        if actual.path != *path || actual.class != *class || actual.binder >= life.binder_count {
            return Err(err(
                "anonymous Tuple child domain differs from complete actual retained boundary",
            ));
        }
    }
    let mut classes = BTreeSet::new();
    let mut exclusive = false;
    for ((path, mode, child_class), slot) in expected.iter().zip(&life.slots) {
        b.spend(
            slot.path
                .len()
                .saturating_add(path.len())
                .saturating_add(classes.len())
                .saturating_add(2),
        )?;
        if slot.path != *path || slot.mode != *mode {
            return Err(err(
                "anonymous Tuple physical field or mode differs from actual child",
            ));
        }
        if let Some((boundary, class)) = child_class {
            b.spend(
                life.retained
                    .len()
                    .saturating_mul(boundary.len().saturating_add(2)),
            )?;
            let mapped = life
                .retained
                .iter()
                .find(|r| r.path == *boundary && r.class == *class)
                .ok_or_else(|| {
                    err("anonymous Tuple child slot lacks its complete retained class map")
                })?;
            if mapped.binder != slot.binder {
                return Err(err(
                    "anonymous Tuple child slot changes its invariant semantic class",
                ));
            }
        }
        if slot.binder >= life.binder_count
            || (!classes.contains(&slot.binder) && slot.binder as usize != classes.len())
        {
            return Err(err(
                "anonymous Tuple class partition is not complete and canonical",
            ));
        }
        classes.insert(slot.binder);
        exclusive |= *mode == StoredBorrowMode::ExclusiveAddress;
    }
    for row in &life.retained {
        b.spend(classes.len().saturating_add(1))?;
        if !classes.contains(&row.binder) && row.binder as usize != classes.len() {
            return Err(err(
                "anonymous Tuple retained class partition is not canonical",
            ));
        }
        classes.insert(row.binder);
    }
    if classes.len() != life.binder_count as usize {
        return Err(err("anonymous Tuple has unused classes"));
    }
    b.stored_exclusive_shapes |= exclusive;
    Ok(Some(Layout {
        fields,
        life,
        owned,
        affine: owned || nested_affine || exclusive,
    }))
}

pub(super) fn operation(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<bool> {
    let ty = match &i.kind {
        InstructionKind::MakeAggregate { .. } | InstructionKind::MakeOwnedTuple { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("anonymous Tuple construction has no result"))?,
        )?,
        InstructionKind::UnpackTuple { local, .. } | InstructionKind::DropTuple { local } => {
            &f.locals[local.0 as usize].ty
        }
        InstructionKind::ExtractField { aggregate, .. } => vt(f, *aggregate)?,
        _ => return Ok(false),
    };
    let Some(actual) = layout(ty, b)? else {
        return Ok(false);
    };
    match &i.kind {
        InstructionKind::MakeAggregate { fields } | InstructionKind::MakeOwnedTuple { fields } => {
            if matches!(i.kind, InstructionKind::MakeOwnedTuple { .. }) != actual.affine {
                return Err(err(
                    "anonymous Tuple construction changes its actual affine mode",
                ));
            }
            b.spend(fields.len().saturating_add(actual.fields.len()))?;
            if fields.len() != actual.fields.len() {
                return Err(err("anonymous Tuple constructor omits actual children"));
            }
            for (value, expected) in fields.iter().zip(actual.fields) {
                type_work(expected, &b.limits.clone(), b)?;
                type_work(vt(f, *value)?, &b.limits.clone(), b)?;
                if vt(f, *value)? != expected {
                    return Err(err(
                        "anonymous Tuple constructor changes complete child Type",
                    ));
                }
            }
        }
        InstructionKind::UnpackTuple { local, fields } => {
            b.spend(fields.len().saturating_mul(fields.len().saturating_add(2)))?;
            if !actual.affine || fields.len() != actual.fields.len() {
                return Err(err(
                    "anonymous Tuple unpack requires every actual affine child",
                ));
            }
            let mut destinations = BTreeSet::new();
            for (target, expected) in fields.iter().zip(actual.fields) {
                let target_type = &f
                    .locals
                    .get(target.0 as usize)
                    .ok_or_else(|| err("anonymous Tuple target is absent"))?
                    .ty;
                type_work(target_type, &b.limits.clone(), b)?;
                type_work(expected, &b.limits.clone(), b)?;
                if target == local || !destinations.insert(*target) || target_type != expected {
                    return Err(err(
                        "anonymous Tuple unpack aliases or retags an actual child",
                    ));
                }
            }
        }
        InstructionKind::ExtractField { index, .. } => {
            let child = actual
                .fields
                .get(*index as usize)
                .ok_or_else(|| err("anonymous Tuple snapshot field is absent"))?;
            type_work(child, &b.limits.clone(), b)?;
            let output = vt(
                f,
                i.result
                    .ok_or_else(|| err("anonymous Tuple snapshot has no result"))?,
            )?;
            type_work(output, &b.limits.clone(), b)?;
            if actual.affine || output != child {
                return Err(err(
                    "anonymous Tuple snapshot cannot copy or retag an affine child",
                ));
            }
        }
        InstructionKind::DropTuple { .. } if !actual.affine => {
            return Err(err("shared anonymous Tuple needs no affine Drop"));
        }
        _ => {}
    }
    Ok(true)
}

pub(super) fn unpack(
    f: &TypedFunction,
    local: LocalId,
    fields: &[LocalId],
    state: &mut Frame,
    b: &mut Budget,
) -> Check<()> {
    let payload = copy_payload(&state.locals[local.0 as usize], b)?;
    for (index, target) in fields.iter().enumerate() {
        let mut child = Payload::default();
        for (path, permission) in &payload.leaves {
            b.spend(path.len().saturating_add(1))?;
            if let Some((PlaceProjection::Field(field), tail)) = path.split_first()
                && *field as usize == index
            {
                if tail.is_empty() && matches!(f.locals[target.0 as usize].ty, Type::Address { .. })
                {
                    join_permission(&mut child.address, permission, b)?;
                } else {
                    add_leaf(&mut child, tail, permission, b)?;
                }
            }
        }
        clear_local_holes(state, *target, b)?;
        state.locals[target.0 as usize] = child;
    }
    state.locals[local.0 as usize] = Payload::default();
    clear_local_holes(state, local, b)
}
