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

pub(super) fn shape(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    fee(b, 8)?;
    let Some((storage, life)) = carrier(ty) else {
        return Ok(false);
    };
    let Type::Enum { name, variants } = storage else {
        return Ok(false);
    };
    let [(none, empty), (some, fields)] = variants.as_slice() else {
        return Ok(false);
    };
    let [payload] = fields.as_slice() else {
        return Ok(false);
    };
    if name.is_empty() || none != "None" || !empty.is_empty() || some != "Some" {
        return Ok(false);
    }
    let Type::Address {
        mutable: false,
        inner,
    } = payload
    else {
        return Ok(false);
    };
    b.type_work(inner, true, "Option actual shared pointee")?;
    if !inner.is_address_pointee() {
        return Ok(false);
    }
    let [slot] = life.slots.as_slice() else {
        return Ok(false);
    };
    let [map] = life.retained.as_slice() else {
        return Ok(false);
    };
    Ok(life.binder_count == 1
        && life.nominal_arguments.is_empty()
        && slot.path
            == [PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: 1,
                index: 0,
            })]
        && slot.binder == 0
        && slot.mode == StoredBorrowMode::SharedAddress
        && map.path == [LifeTypeProjection::TypeArgument(0)]
        && map.class == 0
        && map.binder == slot.binder)
}

/// Each operation transports actual permissions; the class row alone never
/// creates a source. ReadVariant retains its existing runtime tag check.
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    let ty = match &i.kind {
        InstructionKind::MakeVariant { .. } => value_type(
            f,
            i.result
                .ok_or_else(|| failure("Option constructor lacks result"))?,
        )?,
        InstructionKind::ReadVariant { value, .. } | InstructionKind::VariantTag { value } => {
            value_type(f, *value)?
        }
        _ => return Ok(None),
    };
    if !shape(ty, b)? {
        return Ok(None);
    }
    fee(b, module.life_instances.len())?;
    let mut matches = 0usize;
    for instance in &module.life_instances {
        b.type_work(&instance.ty, true, "Option complete instance identity")?;
        b.type_work(ty, true, "Option operation complete identity")?;
        if instance.ty == *ty {
            matches += 1;
        }
    }
    if matches != 1 {
        return Err(failure("Option operation lacks a unique complete instance"));
    }
    fee(b, 4)?;
    let result = value_type(
        f,
        i.result
            .ok_or_else(|| failure("Option operation lacks result"))?,
    )?;
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("Option input outside actual values"))
    };
    let mut out = Value::default();
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
                .ok_or_else(|| failure("Option variant is absent"))?;
            if payload.len() != fields.len() {
                return Err(failure("Option constructor payload arity differs"));
            }
            for (id, expected) in fields.iter().zip(payload) {
                b.type_work(expected, true, "Option payload equality")?;
                b.type_work(value_type(f, *id)?, true, "Option actual payload equality")?;
                if value_type(f, *id)? != expected {
                    return Err(failure("Option payload complete type differs"));
                }
                let mut permission = Permission::default();
                union(&mut permission, &get(*id)?.pointer, b)?;
                fee(b, path.len().saturating_add(1))?;
                out.slots.insert(path.clone(), permission);
            }
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            if *variant != 1 || *index != 0 || result != &variants[1].1[0] {
                return Err(failure(
                    "Option projection differs from actual Some payload",
                ));
            }
            if let Some(permission) = get(*value)?.slots.get(path) {
                union(&mut out.pointer, permission, b)?;
            }
        }
        InstructionKind::VariantTag { .. } => {
            if result != &Type::I32 {
                return Err(failure("Option tag result is not i32"));
            }
        }
        _ => unreachable!(),
    }
    Ok(Some(out))
}
