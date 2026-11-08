//! Independent finite numeric aggregate qualification. No analyzer predicate,
//! stored summary or claimed Copy flag admits a target-bearing value.
use super::*;

pub(super) fn operation(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::ExtractField { .. }
            | InstructionKind::ExtractIndex { .. }
            | InstructionKind::UpdateField { .. }
            | InstructionKind::UpdateIndex { .. }
            | InstructionKind::MakeVariant { .. }
            | InstructionKind::MakeErrorUnion { .. }
            | InstructionKind::PlaceTagLocal { .. }
            | InstructionKind::VariantTag { .. }
            | InstructionKind::ReadVariant { .. }
    )
}

// Ordinary owning unions have a tag/debt in the independent resource kernel,
// but no stored-address permission. Admit only an actual complete union whose
// children contain no address, qualified carrier, callable or retained task.
pub(super) fn ordinary_owned_union(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.store_option_results {
        return Ok(false);
    }
    let union = match &i.kind {
        InstructionKind::PlaceTagLocal { local, path } if path.is_empty() => {
            if vt(f, i.result.ok_or_else(|| err("union tag lacks result"))?)? != &Type::I32 {
                return Err(err("ordinary owning union tag result differs"));
            }
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| err("ordinary union local absent"))?
                .ty
        }
        InstructionKind::TakeVariantField { local, path }
            if matches!(path.as_slice(), [PlaceProjection::ErrorArm(_)]) =>
        {
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| err("ordinary union local absent"))?
                .ty
        }
        InstructionKind::ErrorTag { value } => vt(f, *value)?,
        InstructionKind::TakeErrorUnion { value, .. } => vt(f, *value)?,
        InstructionKind::MakeErrorUnion { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("ordinary union constructor lacks result"))?,
        )?,
        _ => return Ok(false),
    };
    let Type::ErrorUnion { error, result } = union else {
        return Ok(false);
    };
    super::type_work(union, &b.limits.clone(), b)?;
    if union.is_copy_value() {
        return Ok(false);
    }
    b.spend(2)?;
    let mut pending = vec![(error.as_ref(), 0usize), (result.as_ref(), 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth {
            return Err(limit("ordinary union shape depth exceeded"));
        }
        match ty {
            Type::Address { .. }
            | Type::Desc(_)
            | Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::BorrowedFunctionPointer { .. }
            | Type::Task { .. } => return Ok(false),
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| (child, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|child| (child, depth + 1)));
                }
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|child| (child, depth + 1)));
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.push((error, depth + 1));
                pending.push((result, depth + 1));
            }
            Type::Managed(child)
            | Type::OwnedStore { element: child }
            | Type::Array { element: child, .. } => {
                b.spend(1)?;
                pending.push((child, depth + 1));
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64
            | Type::ByteBuffer => {}
            _ => return Ok(false),
        }
    }
    if let InstructionKind::MakeErrorUnion { is_error, value } = i.kind {
        let arm = if is_error {
            error.as_ref()
        } else {
            result.as_ref()
        };
        super::type_work(arm, &b.limits.clone(), b)?;
        if vt(f, value)? != arm {
            return Err(err("ordinary owning union actual arm differs"));
        }
    }
    if let InstructionKind::TakeVariantField { path, .. } = &i.kind {
        let arm = if matches!(path.as_slice(), [PlaceProjection::ErrorArm(true)]) {
            error.as_ref()
        } else {
            result.as_ref()
        };
        super::type_work(arm, &b.limits.clone(), b)?;
        if vt(
            f,
            i.result
                .ok_or_else(|| err("ordinary union take lacks result"))?,
        )? != arm
        {
            return Err(err("ordinary owning union taken arm differs"));
        }
    }
    if let InstructionKind::ErrorTag { .. } = i.kind
        && vt(
            f,
            i.result
                .ok_or_else(|| err("ordinary union tag lacks result"))?,
        )? != &Type::Bool
    {
        return Err(err("ordinary owning union tag result differs"));
    }
    if let InstructionKind::TakeErrorUnion { is_error, .. } = i.kind {
        let arm = if is_error {
            error.as_ref()
        } else {
            result.as_ref()
        };
        super::type_work(arm, &b.limits.clone(), b)?;
        if vt(
            f,
            i.result
                .ok_or_else(|| err("ordinary union take lacks result"))?,
        )? != arm
        {
            return Err(err("ordinary owning union taken arm differs"));
        }
    }
    Ok(true)
}

pub(super) fn ordinary_package_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let InstructionKind::BorrowPackageField {
        local,
        index,
        mutable,
    } = i.kind
    else {
        return Ok(false);
    };
    if !b.store_option_results {
        return Ok(false);
    }
    let Some(Type::Package { fields, .. }) = f.locals.get(local.0 as usize).map(|l| &l.ty) else {
        return Ok(false);
    };
    let Some((_, field)) = fields.get(index as usize) else {
        return Ok(false);
    };
    let Type::Package {
        fields: children, ..
    } = field
    else {
        return Ok(false);
    };
    super::type_work(field, &b.limits.clone(), b)?;
    b.spend(children.len())?;
    if !children.iter().all(|(_, child)| {
        child.is_copy_value()
            || matches!(child, Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer)
    }) {
        return Ok(false);
    }
    if !matches!(vt(f, i.result.ok_or_else(|| err("Package field borrow lacks result"))?)?,
        Type::Address { mutable: actual, inner } if *actual == mutable && inner.as_ref() == field)
    {
        return Err(err("ordinary Package field borrow result differs"));
    }
    Ok(true)
}

fn ordinary(ty: &Type, b: &mut Budget, depth: usize) -> Check<()> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth {
        return Err(limit("ordinary pure Copy type depth exceeded"));
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
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                ordinary(field, b, depth + 1)?;
            }
        }
        Type::Tuple(fields) => {
            for field in fields {
                ordinary(field, b, depth + 1)?;
            }
        }
        Type::Array { element, .. } => ordinary(element, b, depth + 1)?,
        Type::ErrorUnion { error, result } => {
            ordinary(error, b, depth + 1)?;
            ordinary(result, b, depth + 1)?;
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                b.spend(1)?;
                for field in fields {
                    ordinary(field, b, depth + 1)?;
                }
            }
        }
        Type::BorrowedStorage { .. } => {
            physical(ty, &b.limits.clone(), b)?;
            if !retained::pure(ty, b)? {
                return Err(unsupported(
                    "pure Copy operation contains a real borrowing or ownership leaf",
                    None,
                ));
            }
        }
        _ => {
            return Err(unsupported(
                "pure Copy operation contains an Address, borrowed carrier, capability or nonordinary payload",
                None,
            ));
        }
    }
    Ok(())
}

pub(super) fn types(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<()> {
    let output = vt(
        f,
        i.result
            .ok_or_else(|| err("ordinary value operation omits its SSA result"))?,
    )?;
    ordinary(output, b, 0)?;
    let projected = match &i.kind {
        InstructionKind::PlaceTagLocal { local, path } => {
            if !path.is_empty() {
                return Err(err(
                    "ordinary ErrorUnion tag crosses an unsupported local path",
                ));
            }
            let ty = &f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| err("ordinary ErrorUnion tag local is absent"))?
                .ty;
            if !matches!(ty, Type::ErrorUnion { .. }) {
                return Err(err("ordinary tag has no actual ErrorUnion local"));
            }
            ordinary(ty, b, 0)?;
            &Type::I32
        }
        InstructionKind::MakeErrorUnion { is_error, value } => {
            let Type::ErrorUnion { error, result } = output else {
                return Err(err(
                    "ordinary ErrorUnion constructor lacks actual union shape",
                ));
            };
            let arm = if *is_error {
                error.as_ref()
            } else {
                result.as_ref()
            };
            type_work(arm, &b.limits.clone(), b)?;
            if vt(f, *value)? != arm {
                return Err(err("ordinary ErrorUnion input differs from selected arm"));
            }
            output
        }
        InstructionKind::MakeVariant { variant, fields } => {
            let Type::Enum { variants, .. } =
                super::qualified(output).map_or(output, |(storage, _)| storage)
            else {
                return Err(err("ordinary variant output has no enum layout"));
            };
            let payload = &variants
                .get(*variant as usize)
                .ok_or_else(|| err("ordinary variant tag is outside the actual nominal"))?
                .1;
            b.spend(payload.len().saturating_add(fields.len()))?;
            if payload.len() != fields.len() {
                return Err(err("ordinary variant omits actual payload fields"));
            }
            let limits = b.limits.clone();
            for (input, field) in fields.iter().zip(payload) {
                type_work(vt(f, *input)?, &limits, b)?;
                type_work(field, &limits, b)?;
                if vt(f, *input)? != field {
                    return Err(err(
                        "ordinary variant input changes the full actual payload type",
                    ));
                }
            }
            output
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let aggregate = vt(f, *value)?;
            ordinary(aggregate, b, 0)?;
            let Type::Enum { variants, .. } =
                super::qualified(aggregate).map_or(aggregate, |(storage, _)| storage)
            else {
                return Err(err("ordinary variant projection has no enum owner"));
            };
            variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize))
                .ok_or_else(|| err("ordinary variant projection selects no actual field"))?
        }
        InstructionKind::VariantTag { value } => {
            let aggregate = vt(f, *value)?;
            ordinary(aggregate, b, 0)?;
            if !matches!(
                super::qualified(aggregate).map_or(aggregate, |(storage, _)| storage),
                Type::Enum { .. }
            ) {
                return Err(err("ordinary tag source is not an enum"));
            }
            &Type::I32
        }
        InstructionKind::ExtractField { aggregate, index } => {
            let aggregate = vt(f, *aggregate)?;
            ordinary(aggregate, b, 0)?;
            match super::qualified(aggregate).map_or(aggregate, |(storage, _)| storage) {
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, field)| field),
                Type::Tuple(fields) => fields.get(*index as usize),
                _ => None,
            }
            .ok_or_else(|| err("ordinary field projection crosses its actual inline layout"))?
        }
        InstructionKind::ExtractIndex { aggregate, index } => {
            let aggregate = vt(f, *aggregate)?;
            ordinary(aggregate, b, 0)?;
            if !vt(f, *index)?.is_integer() {
                return Err(err("ordinary array index is not an integer"));
            }
            let Type::Array { element, .. } = aggregate else {
                return Err(err("ordinary index source is not a fixed array"));
            };
            element.as_ref()
        }
        InstructionKind::UpdateField {
            aggregate,
            index,
            value,
        } => {
            let source = vt(f, *aggregate)?;
            ordinary(source, b, 0)?;
            let physical = super::qualified(source).map_or(source, |(storage, _)| storage);
            let field = match physical {
                Type::Tuple(fields) => fields.get(*index as usize),
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, ty)| ty),
                _ => None,
            }
            .ok_or_else(|| err("pure field update crosses actual inline layout"))?;
            type_work(field, &b.limits.clone(), b)?;
            type_work(vt(f, *value)?, &b.limits.clone(), b)?;
            if vt(f, *value)? != field {
                return Err(err("pure field update changes complete child type"));
            }
            source
        }
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => {
            let source = vt(f, *aggregate)?;
            ordinary(source, b, 0)?;
            if !vt(f, *index)?.is_integer() {
                return Err(err("pure index update requires integer index"));
            }
            let Type::Array { element, .. } = source else {
                return Err(err("pure index update requires actual fixed Array"));
            };
            type_work(element, &b.limits.clone(), b)?;
            type_work(vt(f, *value)?, &b.limits.clone(), b)?;
            if vt(f, *value)? != element.as_ref() {
                return Err(err("pure index update changes complete element type"));
            }
            source
        }
        _ => {
            return Err(err(
                "operation is outside ordinary pure Copy reconstruction",
            ));
        }
    };
    let limits = b.limits.clone();
    type_work(projected, &limits, b)?;
    type_work(output, &limits, b)?;
    if projected != output {
        return Err(err(
            "ordinary Copy result differs from its complete actual projection",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_and_nested_payloads_never_hide_address_roots() {
        let limits = Limits::default();
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let inactive = Type::Enum {
            name: "InactiveAddress".into(),
            variants: vec![
                ("Empty".into(), vec![]),
                ("Hidden".into(), vec![address.clone()]),
            ],
        };
        let nested = Type::Record {
            name: "NestedAddress".into(),
            fields: vec![(
                "payload".into(),
                Type::Tuple(vec![Type::Array {
                    element: Box::new(address),
                    length: 2,
                }]),
            )],
        };
        for ty in [inactive, nested, Type::ForeignHandle] {
            assert!(ordinary(&ty, &mut Budget::new(&limits), 0).is_err());
        }
    }
}
