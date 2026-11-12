//! Reconstruct Context's port numbering independently from its physical slots.
use super::*;

pub(super) fn validate(module: &TypedModule, item: &DynInterface, b: &mut Budget) -> Check<()> {
    let fail = || {
        reject(
            "E-VERIFY-DYN-CONTEXT",
            None,
            "Context port directory differs from its complete qualified storage type",
        )
    };
    let Some(context) = &item.context else {
        if item.retained_life.binder_count != 0
            || !item.retained_life.ports.is_empty()
            || !item.retained_life.retained.is_empty()
            || !item.retained_life.nominal_arguments.is_empty()
        {
            return Err(fail());
        }
        return Ok(());
    };
    let limits = b.limits.clone();
    crate::stored_flow::qualification(module, context, &limits, b)?;
    let Type::BorrowedStorage { storage, life } = context else {
        return Err(fail());
    };
    if item.retained_life.binder_count != life.binder_count
        || item.retained_life.retained != life.retained
        || item.retained_life.nominal_arguments != life.nominal_arguments
    {
        return Err(fail());
    }
    b.spend(life.slots.len())?;
    let mut numbering: Vec<(&Type, StoredBorrowMode, u32)> = Vec::new();
    for row in &life.slots {
        let mut node = storage.as_ref();
        let mut remaining = row.path.as_slice();
        while let Some((selector, tail)) = remaining.split_first() {
            b.spend(1)?;
            node = match node {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                other => other,
            };
            let selected = match selector {
                PlaceProjection::Field(index) => match node {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        fields.get(*index as usize).map(|(_, t)| t)
                    }
                    Type::Tuple(fields) => fields.get(*index as usize),
                    Type::Array { element, length } if *index < *length => Some(element.as_ref()),
                    _ => None,
                },
                PlaceProjection::Variant(p) => match node {
                    Type::Enum { variants, .. } => variants
                        .get(p.variant as usize)
                        .and_then(|(_, fields)| fields.get(p.index as usize)),
                    _ => None,
                },
                PlaceProjection::ErrorArm(error_arm) => match node {
                    Type::ErrorUnion { error, result } => Some(if *error_arm {
                        error.as_ref()
                    } else {
                        result.as_ref()
                    }),
                    _ => None,
                },
            }
            .ok_or_else(fail)?;
            node = selected;
            remaining = tail;
        }
        let mut first = true;
        for (previous, mode, binder) in &numbering {
            crate::dynamic_metadata::ty(previous, b)?;
            crate::dynamic_metadata::ty(node, b)?;
            if *previous == node && *mode == row.mode && *binder == row.binder {
                first = false;
                break;
            }
        }
        if first {
            let claim = item
                .retained_life
                .ports
                .get(numbering.len())
                .ok_or_else(fail)?;
            crate::dynamic_metadata::ty(&claim.leaf, b)?;
            crate::dynamic_metadata::ty(node, b)?;
            if claim.leaf != *node || claim.mode != row.mode || claim.binder != row.binder {
                return Err(fail());
            }
            numbering.push((node, row.mode, row.binder));
        }
    }
    if numbering.len() != item.retained_life.ports.len() {
        return Err(fail());
    }
    Ok(())
}
