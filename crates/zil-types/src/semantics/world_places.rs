//! Whole-class projection for actual inline storage operations. Tag reads do
//! not read payload occupants or manufacture a payload permission.
use super::*;

pub(super) fn place_diagnostic(
    world: &World<'_, '_>, message: &str, local: LocalId,
    selected: &[PlaceProjection], missing: Option<&[PlaceProjection]>,
) -> Diagnostic {
    let mut diagnostic = bad(message);
    if let Some((_, function, block, at, _)) = world.current_point
        && let Some(instruction) = world.module.functions.get(function.0 as usize)
            .filter(|entry| entry.id == function)
            .and_then(|entry| entry.blocks.get(block.0 as usize))
            .filter(|entry| entry.id == block)
            .and_then(|entry| entry.instructions.get(at))
    {
        diagnostic.span = Some(instruction.span);
    }
    diagnostic.notes.push(format!("actual local {} selects path {selected:?}", local.0));
    if let Some(missing) = missing {
        diagnostic.notes.push(format!("uninitialized path in this local {missing:?}"));
    }
    diagnostic
}

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
        let (definition, child) = world.child_at(environment, instance.id)?;
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
) -> R<SharedType> {
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
    locals: &[Option<SharedType>],
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

/// A partially initialized shell can supply a destination address only when
/// every use of that address is part of the exact path to its vacant slot.
pub(super) fn borrow_source(
    world: &mut World<'_, '_>, function: &TypedFunction, block: BlockId, at: usize,
    locals: &[Option<SharedType>], local: LocalId,
) -> R<SharedType> {
    world.pay(world.vacant.len())?;
    let Some((_, vacant)) = world.vacant.iter().find(|(owner, _)| *owner == local) else {
        return world.local_shell(locals, local);
    };
    let instruction = function.blocks.get(block.0 as usize).and_then(|bb| bb.instructions.get(at));
    let Some(instruction) = instruction else { return Err(bad("partial shell has no actual address preparation")); };
    match instruction.kind {
        InstructionKind::BorrowLocalMut { local: source } if source == local => {}
        InstructionKind::Load(source) if source == local
            && matches!(function.locals.get(local.0 as usize).map(|local| &local.ty), Some(Type::Address { mutable: true, .. })) => {
            world.pay(index::search(world.local_roots.len()))?;
            if !world.local_roots.get(&local).is_some_and(|roots| roots.len() == 1
                && roots.iter().all(|root| root.local == local && root.path.is_empty())) {
                return Err(bad("partial holder refill lacks its actual incoming address root"));
            }
        }
        _ => return Err(place_diagnostic(
            world, "whole local reads an uninitialized projected occupant", local, &[], Some(vacant),
        )),
    }
    let mut value = instruction.result.ok_or_else(|| bad("partial shell has no actual addressed result"))?;
    let mut after = at;
    let mut path = Vec::new();
    loop {
        let mut consumer = None;
        for bb in &function.blocks {
            world.pay(bb.instructions.len().saturating_add(1))?;
            if matches!(bb.terminator, Terminator::Return(Some(operand)) | Terminator::Branch { condition: operand, .. } if operand == value) {
                return Err(bad("vacant shell address escapes its refill"));
            }
            for (index, next) in bb.instructions.iter().enumerate() {
                for operand in next.kind.value_operands() {
                    world.pay(1)?;
                    if operand != value { continue; }
                    if bb.id != block || index <= after || consumer.replace((index,next)).is_some() {
                        return Err(bad("vacant shell address has another access besides its refill"));
                    }
                }
            }
        }
        match consumer {
            Some((index, Instruction {kind: InstructionKind::AddressField { address, index: field }, result: Some(child), ..})) if *address == value => {
                if path.len() >= world.limits.max_call_depth { return Err(Diagnostic::limit(Phase::Types, "stored refill path depth exceeded", None)); }
                path.push(PlaceProjection::Field(*field));
                value = *child;
                after = index;
            }
            Some((_, Instruction {kind: InstructionKind::StoreStoredBorrow { storage, path: tail, .. }, ..})) if *storage == value => {
                world.pay(path.len().saturating_add(tail.len()).saturating_add(1))?;
                path.extend_from_slice(tail);
                if !world.vacant.contains(&(local,path)) { return Err(bad("partial shell refill selects another stored slot")); }
                return world.local_shell(locals, local);
            }
            _ => return Err(bad("whole local with a vacant field may only prepare its actual refill")),
        }
    }
}
