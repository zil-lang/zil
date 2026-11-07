//! Ordinary nominal shared-borrow sums. A shape row never creates a source.
use super::*;

pub(super) fn candidate(ty: &Type) -> bool {
    matches!(ty, Type::BorrowedStorage { storage, life }
        if matches!(storage.as_ref(), Type::Enum { .. }) && !life.slots.is_empty())
}

pub(super) fn sum_value(ty: &Type) -> bool {
    match ty {
        Type::ErrorUnion { error, result } => {
            (sum_value(error) || sum_value(result))
                && (error.is_copy_value() || sum_value(error))
                && (result.is_copy_value() || sum_value(result))
        }
        Type::BorrowedStorage { life, .. } if candidate(ty) => life
            .slots
            .iter()
            .all(|s| s.mode == StoredBorrowMode::SharedAddress),
        _ => false,
    }
}

pub(super) fn arguments(instance: &LifeNominalInstance, left: &mut usize, span: Span) -> R<()> {
    if !candidate(&instance.ty) {
        return Ok(());
    }
    let Type::BorrowedStorage { storage, life } = &instance.ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    let mut expected = 0usize;
    for (ordinal, argument) in instance.arguments.iter().enumerate() {
        fee(left, 1, span)?;
        let LifeNominalActual::Type(actual @ Type::Address { mutable: false, .. }) = argument
        else {
            continue;
        };
        let map = life.retained.get(expected).ok_or_else(|| {
            error(
                "E-STORED-IDENTITY",
                span,
                "Enum Address actual omits its source class map",
            )
        })?;
        fee(left, map.path.len().saturating_add(life.slots.len()), span)?;
        if map.path.as_slice() != [LifeTypeProjection::TypeArgument(ordinal as u32)]
            || map.class != 0
            || map.binder >= life.binder_count
        {
            return Err(error(
                "E-STORED-IDENTITY",
                span,
                "Enum Address actual map differs from its complete argument ordinal",
            ));
        }
        let mut occurs = false;
        for slot in &life.slots {
            let [PlaceProjection::Variant(p)] = slot.path.as_slice() else {
                continue;
            };
            if let Some(field) = variants
                .get(p.variant as usize)
                .and_then(|(_, fields)| fields.get(p.index as usize))
            {
                fee(left, 1, span)?;
                occurs |= slot.binder == map.binder && field == actual;
            }
        }
        if !occurs {
            return Err(unsupported(
                span,
                "Enum Address Type actual needs its complete physical occurrence",
            ));
        }
        expected += 1;
    }
    if expected != life.retained.len() {
        return Err(error(
            "E-STORED-IDENTITY",
            span,
            "Enum source class map has no complete Address actual",
        ));
    }
    Ok(())
}

pub(super) fn shape(ty: &Type, left: &mut usize, depth: usize, span: Span) -> R<bool> {
    if let Type::ErrorUnion {
        error: failure,
        result,
    } = ty
    {
        let a = shape(failure, left, depth, span)?;
        let z = shape(result, left, depth, span)?;
        if !a && !z {
            return Ok(false);
        }
        if (!a && !closed_copy(failure, left, 0, depth, span)?)
            || (!z && !closed_copy(result, left, 0, depth, span)?)
        {
            return Err(unsupported(
                span,
                "borrowed error wrapper requires complete finite ordinary arms",
            ));
        }
        return Ok(true);
    }
    if !candidate(ty) {
        return Ok(false);
    }
    let Type::BorrowedStorage { storage, life } = ty else {
        unreachable!()
    };
    let Type::Enum { name, variants } = storage.as_ref() else {
        unreachable!()
    };
    fee(
        left,
        name.len().saturating_add(variants.len()).saturating_add(1),
        span,
    )?;
    if name.is_empty() || variants.is_empty() || life.binder_count == 0 {
        return Err(error(
            "E-STORED-TYPE",
            span,
            "borrowed Enum has an empty nominal or class domain",
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    let mut actual = Vec::new();
    for (variant, (name, fields)) in variants.iter().enumerate() {
        fee(
            left,
            name.len()
                .saturating_mul(names.len().saturating_add(1))
                .saturating_add(fields.len()),
            span,
        )?;
        if name.is_empty() || !names.insert(name) {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "borrowed Enum variants require unique nonempty names",
            ));
        }
        for (index, field) in fields.iter().enumerate() {
            match field {
                Type::Address {
                    mutable: false,
                    inner,
                } => {
                    let pointee = match inner.as_ref() {
                        Type::Slice { element } => element.as_ref(),
                        other => other,
                    };
                    if !closed_copy(pointee, left, 0, depth, span)? {
                        return Err(unsupported(
                            span,
                            "borrowed Enum pointee requires its complete consumer",
                        ));
                    }
                    fee(left, 1, span)?;
                    actual.push(PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant: u32::try_from(variant).map_err(|_| {
                            error("E-STORED-TYPE", span, "Enum variant ordinal overflow")
                        })?,
                        index: u32::try_from(index).map_err(|_| {
                            error("E-STORED-TYPE", span, "Enum payload ordinal overflow")
                        })?,
                    }));
                }
                other if closed_copy(other, left, 0, depth, span)? => {}
                _ => {
                    return Err(unsupported(
                        span,
                        "borrowed Enum payload requires a shared address or complete finite Copy value",
                    ));
                }
            }
        }
    }
    fee(left, actual.len().saturating_add(life.slots.len()), span)?;
    if actual.len() != life.slots.len() {
        return Err(error(
            "E-STORED-TYPE",
            span,
            "borrowed Enum slots do not cover the physical payloads",
        ));
    }
    for (path, slot) in actual.iter().zip(&life.slots) {
        fee(left, slot.path.len().saturating_add(1), span)?;
        if slot.path.as_slice() != [*path]
            || slot.mode != StoredBorrowMode::SharedAddress
            || slot.binder >= life.binder_count
        {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "borrowed Enum slot path, mode or class differs from its physical leaf",
            ));
        }
    }
    let mut classes = std::collections::BTreeSet::new();
    for binder in life
        .nominal_arguments
        .iter()
        .map(|row| row.binder)
        .chain(life.retained.iter().map(|row| row.binder))
        .chain(life.slots.iter().map(|row| row.binder))
    {
        fee(left, classes.len().saturating_add(1), span)?;
        if binder >= life.binder_count
            || (!classes.contains(&binder) && binder as usize != classes.len())
        {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "borrowed Enum classes are not canonical",
            ));
        }
        classes.insert(binder);
    }
    if classes.len() != life.binder_count as usize {
        return Err(error(
            "E-STORED-TYPE",
            span,
            "borrowed Enum contains an unused trailing class",
        ));
    }
    for row in &life.retained {
        fee(left, row.path.len().saturating_add(1), span)?;
        if !matches!(row.path.as_slice(), [LifeTypeProjection::TypeArgument(_)])
            || row.class != 0
            || row.binder >= life.binder_count
        {
            return Err(error(
                "E-STORED-TYPE",
                span,
                "borrowed Enum Type actual map has an invalid class boundary",
            ));
        }
    }
    Ok(true)
}
