//! Qualified addresses point at current carrier memory. Their head permission
//! and every stored data pointer retain separate original loan ancestry.
use super::*;

pub(super) fn shape(ty: &Type) -> Option<&StorageLifeShape> {
    let Type::Address { inner, .. } = ty else {
        return None;
    };
    let Type::BorrowedStorage { storage, life } = inner.as_ref() else {
        return None;
    };
    (matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_)) && life.retained.is_empty())
        .then_some(life)
}

pub(super) fn rows(
    ty: &Type,
    payload: &Payload,
    frame: &Frame,
    b: &mut Budget,
) -> Check<Option<leaf_rows::Rows>> {
    let Some(shape) = shape(ty) else {
        return Ok(None);
    };
    b.spend(shape.slots.len().saturating_add(1))?;
    let mut current = Payload::default();
    for slot in &shape.slots {
        b.spend(slot.path.len())?;
        let permission = select(frame, &payload.address, &slot.path, b)?;
        add_leaf(&mut current, &slot.path, &permission, b)?;
    }
    Ok(Some(current.leaves))
}
