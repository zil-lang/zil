//! Sealed Option address-actual proof. Metadata never supplies a target root.
use super::*;

pub(super) fn declaration(d: &LifeNominalDeclaration) -> bool {
    d.id.compiler
        && d.id.module.0 == u32::MAX
        && d.id.declaration.0 == 0
        && d.identity.package == "@zil.builtin"
        && d.identity.module_path == ["option"]
        && d.identity.declaration_path == ["enum", "Option"]
        && d.identity.generation_path.is_empty()
        && d.kind == LifeNominalKind::Enum
        && d.parameters == [LifeParameterKind::Type]
}

pub(super) fn shape(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    let Some((storage, life)) = qualified(ty) else {
        return Ok(false);
    };
    let Type::Enum { name, variants } = storage else {
        return Ok(false);
    };
    if life.slots.is_empty() {
        return Ok(false);
    }
    b.spend(8)?;
    if name.is_empty() || variants.len() != 2 {
        return Ok(false);
    }
    for (i, (name, payload)) in variants.iter().enumerate() {
        if (i == 0 && (name != "None" || !payload.is_empty()))
            || (i == 1 && (name != "Some" || payload.len() != 1))
        {
            return Ok(false);
        }
    }
    match &variants[1].1[0] {
        Type::Address {
            mutable: false,
            inner,
        } => {
            type_work(inner, limits, b)?;
            let pointee = match inner.as_ref() {
                Type::Slice { element } => element.as_ref(),
                other => other,
            };
            if !closed_pointee(pointee, limits, b)? {
                return Ok(false);
            }
        }
        _ => return Ok(false),
    }
    if life.binder_count != 1
        || !life.nominal_arguments.is_empty()
        || life.slots.len() != 1
        || life.retained.len() != 1
    {
        return Ok(false);
    }
    let slot = &life.slots[0];
    let map = &life.retained[0];
    Ok(
        matches!(slot.path.as_slice(), [PlaceProjection::Variant(p)] if p.variant == 1 && p.index == 0)
            && slot.mode == StoredBorrowMode::SharedAddress
            && slot.binder == 0
            && matches!(map.path.as_slice(), [LifeTypeProjection::TypeArgument(0)])
            && map.class == 0
            && map.binder == 0,
    )
}

/// Each operation transports actual permissions; the class row alone never
/// creates a source. ReadVariant retains its existing runtime tag check.
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Option<Payload>> {
    let ty = match &i.kind {
        InstructionKind::MakeVariant { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("Option constructor lacks result"))?,
        )?,
        InstructionKind::ReadVariant { value, .. } | InstructionKind::VariantTag { value } => {
            vt(f, *value)?
        }
        _ => return Ok(None),
    };
    let limits = b.limits.clone();
    if !shape(ty, &limits, b)? {
        return Ok(None);
    }
    qualification(module, ty, &limits, b)?;
    b.spend(4)?;
    let result = vt(
        f,
        i.result
            .ok_or_else(|| err("Option operation lacks result"))?,
    )?;
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| err("Option input outside actual values"))
    };
    let mut out = Payload::default();
    let Type::BorrowedStorage { storage, life } = ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    let path = &life.slots[0].path;
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            let (_, payload) = variants
                .get(*variant as usize)
                .ok_or_else(|| err("Option variant is absent"))?;
            if payload.len() != fields.len() {
                return Err(err("Option constructor payload arity differs"));
            }
            for (id, expected) in fields.iter().zip(payload) {
                type_work(expected, &limits, b)?;
                type_work(vt(f, *id)?, &limits, b)?;
                if vt(f, *id)? != expected {
                    return Err(err("Option payload complete type differs"));
                }
                add_leaf(&mut out, path, &get(*id)?.address, b)?;
            }
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            if *variant != 1 || *index != 0 || result != &variants[1].1[0] {
                return Err(err("Option projection differs from actual Some payload"));
            }
            if let Some(permission) = leaf(get(*value)?, path, b)? {
                join_permission(&mut out.address, permission, b)?;
            }
        }
        InstructionKind::VariantTag { .. } => {
            if result != &Type::I32 {
                return Err(err("Option tag result is not i32"));
            }
        }
        _ => unreachable!(),
    }
    Ok(Some(out))
}

// Iteratively reconstruct exactly the ordinary data domain. Code, dyn, owning,
// nested address and qualified classes are not borrowed through this profile.
fn closed_pointee(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    let mut todo = vec![(ty, 0usize)];
    while let Some((ty, depth)) = todo.pop() {
        b.spend(1)?;
        if depth > limits.max_call_depth {
            return Err(err("Option pointee exceeds actual depth limit"));
        }
        match ty {
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64 => {}
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                todo.extend(fields.iter().map(|t| (t, depth + 1)));
            }
            Type::Record { name, fields } => {
                b.spend(name.len().saturating_add(fields.len()))?;
                for (name, child) in fields {
                    b.spend(name.len())?;
                    todo.push((child, depth + 1));
                }
            }
            Type::Enum { name, variants } => {
                b.spend(name.len())?;
                for (name, fields) in variants {
                    b.spend(name.len().saturating_add(fields.len()))?;
                    todo.extend(fields.iter().map(|t| (t, depth + 1)));
                }
            }
            Type::Array { element, .. } => {
                b.spend(1)?;
                todo.push((element, depth + 1));
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                todo.push((error, depth + 1));
                todo.push((result, depth + 1));
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}
