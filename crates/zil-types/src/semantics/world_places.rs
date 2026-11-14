//! Whole-class projection for actual inline storage operations. Tag reads do
//! not read payload occupants or manufacture a payload permission.
use super::*;

pub(super) fn borrow_mode(ty: Option<&Type>, mutable: bool) -> R<()> {
    if !matches!(ty,Some(Type::Address{mutable:actual,..}) if *actual==mutable) {
        return Err(bad("actual borrow changes the opcode's exact outer mode"));
    }
    Ok(())
}

pub(super) fn capture_exit(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    function: &TypedFunction,
) -> R<()> {
    if let Some(environment) = function.environment {
        let (definition, child) = world.child(environment)?;
        world.pay(world.vacant_captures.len())?;
        if child.id != instance.id
            || definition.kind == CallableKind::FnMut
                && world
                    .vacant_captures
                    .iter()
                    .any(|(owner, _)| *owner == instance.id)
        {
            return Err(bad(
                "actual FnMut exit does not refill every transferred environment field",
            ));
        }
    }
    Ok(())
}

pub(super) fn path(
    world: &mut World<'_, '_>,
    value: &SemanticTypeValue,
    path: &[PlaceProjection],
) -> R<SemanticTypeValue> {
    if path.len() > world.limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "actual place depth exceeded",
            None,
        ));
    }
    world.pay(path.len().saturating_add(1))?;
    let mut value = world.copy(value)?;
    for selector in path {
        value = projection::place(world, &value, *selector)?;
    }
    Ok(value)
}
pub(super) fn local_mutable(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    local: LocalId,
    mutable: bool,
) -> R<()> {
    world.pay(1)?;
    if mutable
        && !function
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| bad("place owner local is absent"))?
            .mutable
    {
        return Err(bad("exclusive inline borrow has an immutable actual owner"));
    }
    Ok(())
}
pub(super) fn address_mutable(
    world: &mut World<'_, '_>,
    value: &SemanticTypeValue,
    mutable: bool,
) -> R<()> {
    if !matches!(World::type_of(value)?,Type::Address{mutable:parent,..} if !mutable || *parent) {
        return Err(bad(
            "actual addressed place lacks its complete parent address authority",
        ));
    }
    world.pay(1)
}
pub(super) fn tag_type(
    world: &mut World<'_, '_>,
    ty: &Type,
    path: &[PlaceProjection],
    enum_only: bool,
) -> R<()> {
    if path.len() > world.limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "actual tag place depth exceeded",
            None,
        ));
    }
    world.pay(path.len().saturating_add(1))?;
    let mut ty = ty;
    for selector in path {
        ty = match (stored::physical(ty), selector) {
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(index),
            ) if !enum_only => fields.get(*index as usize).map(|(_, ty)| ty),
            (Type::Tuple(fields), PlaceProjection::Field(index)) if !enum_only => {
                fields.get(*index as usize)
            }
            (Type::Array { element, length }, PlaceProjection::Field(index))
                if !enum_only && *index < *length =>
            {
                Some(element.as_ref())
            }
            (
                Type::Enum { variants, .. },
                PlaceProjection::Variant(VariantProjection { variant, index }),
            ) => variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(is_error))
                if !enum_only =>
            {
                Some(if *is_error {
                    error.as_ref()
                } else {
                    result.as_ref()
                })
            }
            _ => None,
        }
        .ok_or_else(|| bad("actual tag path leaves its complete inline product/sum"))?;
    }
    if !matches!(stored::physical(ty), Type::Enum { .. })
        && (enum_only || !matches!(stored::physical(ty), Type::ErrorUnion { .. }))
    {
        return Err(bad("actual tag reads another terminal constructor"));
    }
    Ok(())
}
pub(super) fn local_tag(
    world: &mut World<'_, '_>,
    locals: &[Option<SemanticTypeValue>],
    local: LocalId,
    path: &[PlaceProjection],
    enum_only: bool,
) -> R<()> {
    world.pay(
        world
            .vacant
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    if world
        .vacant
        .iter()
        .any(|(owner, vacant)| *owner == local && path.starts_with(vacant))
    {
        return Err(bad("actual tag reads a vacant header or ancestor"));
    }
    // A live header may have moved-out descendants. No payload class value is
    // produced, so neither an old descendant image nor a new epoch is read.
    let value = locals
        .get(local.0 as usize)
        .and_then(Option::as_ref)
        .ok_or_else(|| bad("actual tag local has no initialized shell"))?;
    tag_type(world, World::type_of(value)?, path, enum_only)
}
