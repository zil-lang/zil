//! Independently derive the typed opaque directory from authenticated Context.
use super::*;

pub(super) fn validate(
    module: &TypedModule,
    interface: &DynInterface,
    b: &mut Budget,
) -> CompileResult<()> {
    let fail = || bad("dyn Context does not match its actual complete storage envelope");
    let Some(context) = &interface.context else {
        if interface.retained_life.binder_count != 0
            || !interface.retained_life.ports.is_empty()
            || !interface.retained_life.retained.is_empty()
            || !interface.retained_life.nominal_arguments.is_empty()
        {
            return Err(fail());
        }
        return Ok(());
    };
    let limits = zil_base::Limits {
        max_nodes: b.helper_nodes,
        max_call_depth: b.partial_depth,
        ..zil_base::Limits::default()
    };
    crate::stored_flow::qualification(module, context, &limits, b)?;
    let Type::BorrowedStorage { storage, life } = context else {
        return Err(fail());
    };
    if interface.retained_life.binder_count != life.binder_count
        || interface.retained_life.retained != life.retained
        || interface.retained_life.nominal_arguments != life.nominal_arguments
    {
        return Err(fail());
    }
    b.spend(life.slots.len(), "dyn Context actual leaf table")?;
    let mut leaves: Vec<(&Type, StoredBorrowMode, u32)> = Vec::new();
    for slot in &life.slots {
        let mut current = storage.as_ref();
        for step in &slot.path {
            b.spend(1, "dyn Context actual field")?;
            if let Type::BorrowedStorage { storage, .. } = current {
                current = storage;
            }
            current = match (current, step) {
                (
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                    PlaceProjection::Field(i),
                ) => fields.get(*i as usize).map(|(_, t)| t),
                (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
                (Type::Array { element, length }, PlaceProjection::Field(i)) if *i < *length => {
                    Some(element.as_ref())
                }
                (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants
                    .get(p.variant as usize)
                    .and_then(|(_, f)| f.get(p.index as usize)),
                (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                    Some(if *e { error.as_ref() } else { result.as_ref() })
                }
                _ => None,
            }
            .ok_or_else(fail)?;
        }
        let mut seen = false;
        for (leaf, mode, binder) in &leaves {
            crate::dyn_metadata::ty(leaf, b)?;
            crate::dyn_metadata::ty(current, b)?;
            if *leaf == current && *mode == slot.mode && *binder == slot.binder {
                seen = true;
                break;
            }
        }
        if !seen {
            leaves.push((current, slot.mode, slot.binder));
        }
    }
    if leaves.len() != interface.retained_life.ports.len() {
        return Err(fail());
    }
    for ((leaf, mode, binder), port) in leaves.into_iter().zip(&interface.retained_life.ports) {
        crate::dyn_metadata::ty(&port.leaf, b)?;
        if leaf != &port.leaf || mode != port.mode || binder != port.binder {
            return Err(fail());
        }
    }
    Ok(())
}
