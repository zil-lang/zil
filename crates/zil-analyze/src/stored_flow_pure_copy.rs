//! Ordinary finite values carry no stored targets. Admission checks the full
//! input/output types, including inactive enum variants, before empty transfer.
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

// In a mixed Option/ordinary-owner function, the stored-address ledger has no
// permission to transfer an owning ErrorUnion. Its tag and complete debt are
// checked by the ordinary resource kernel. Authenticate this exact zero-target
// shape before leaving the stored ledger empty; an address or qualified child
// must continue through a real stored permission operation.
pub(super) fn ordinary_owned_union(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.store_option_results {
        return Ok(false);
    }
    let union = match &i.kind {
        InstructionKind::PlaceTagLocal { local, path } if path.is_empty() => {
            if value_type(
                f,
                i.result.ok_or_else(|| failure("union tag lacks result"))?,
            )? != &Type::I32
            {
                return Err(failure("ordinary owning union tag result differs"));
            }
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| failure("ordinary union local absent"))?
                .ty
        }
        InstructionKind::TakeVariantField { local, path }
            if matches!(path.as_slice(), [PlaceProjection::ErrorArm(_)]) =>
        {
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| failure("ordinary union local absent"))?
                .ty
        }
        InstructionKind::ErrorTag { value } => value_type(f, *value)?,
        InstructionKind::TakeErrorUnion { value, .. } => value_type(f, *value)?,
        InstructionKind::MakeErrorUnion { .. } => value_type(
            f,
            i.result
                .ok_or_else(|| failure("ordinary union constructor lacks result"))?,
        )?,
        _ => return Ok(false),
    };
    let Type::ErrorUnion { error, result } = union else {
        return Ok(false);
    };
    if !ordinary_union_shape(union, b)? {
        return Ok(false);
    }
    if let InstructionKind::MakeErrorUnion { is_error, value } = i.kind {
        let arm = if is_error {
            error.as_ref()
        } else {
            result.as_ref()
        };
        b.type_work(arm, false, "ordinary owning union actual arm")?;
        if value_type(f, value)? != arm {
            return Err(failure("ordinary owning union actual arm differs"));
        }
    }
    if let InstructionKind::TakeVariantField { path, .. } = &i.kind {
        let arm = if matches!(path.as_slice(), [PlaceProjection::ErrorArm(true)]) {
            error.as_ref()
        } else {
            result.as_ref()
        };
        b.type_work(arm, false, "ordinary owning union taken arm")?;
        if value_type(
            f,
            i.result
                .ok_or_else(|| failure("ordinary union take lacks result"))?,
        )? != arm
        {
            return Err(failure("ordinary owning union taken arm differs"));
        }
    }
    if let InstructionKind::ErrorTag { .. } = i.kind
        && value_type(
            f,
            i.result
                .ok_or_else(|| failure("ordinary union tag lacks result"))?,
        )? != &Type::Bool
    {
        return Err(failure("ordinary owning union tag result differs"));
    }
    if let InstructionKind::TakeErrorUnion { is_error, .. } = i.kind {
        let arm = if is_error {
            error.as_ref()
        } else {
            result.as_ref()
        };
        b.type_work(arm, false, "ordinary owning union taken arm")?;
        if value_type(
            f,
            i.result
                .ok_or_else(|| failure("ordinary union take lacks result"))?,
        )? != arm
        {
            return Err(failure("ordinary owning union taken arm differs"));
        }
    }
    Ok(true)
}

pub(super) fn ordinary_package_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
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
    b.type_work(field, false, "ordinary borrowed Package field")?;
    fee(b, children.len())?;
    if !children.iter().all(|(_, child)| {
        child.is_copy_value()
            || matches!(child, Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer)
    }) {
        return Ok(false);
    }
    if !matches!(value_type(f, i.result.ok_or_else(|| failure("Package field borrow lacks result"))?)?,
        Type::Address { mutable: actual, inner } if *actual == mutable && inner.as_ref() == field)
    {
        return Err(failure("ordinary Package field borrow result differs"));
    }
    Ok(true)
}

// An ordinary owner can be observed without carrying any stored target into
// its Copy result. Qualified parent shells keep their separate transfer path.
pub(super) fn ordinary_package_read(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::ReadPackageField { local, index } = i.kind else {
        return Ok(false);
    };
    let parent = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("ordinary Package read local is absent"))?
        .ty;
    let Type::Package { fields, .. } = parent else {
        return Ok(false);
    };
    let field = &fields
        .get(index as usize)
        .ok_or_else(|| failure("ordinary Package read field is absent"))?
        .1;
    b.type_work(field, true, "ordinary Package Copy observation")?;
    if !field.is_copy_value() || !pure(field, b, 0)? {
        return Err(failure(
            "ordinary Package read requires a complete pure Copy field",
        ));
    }
    let output = value_type(
        f,
        i.result
            .ok_or_else(|| failure("ordinary Package read lacks result"))?,
    )?;
    if output != field {
        return Err(failure(
            "ordinary Package read result differs from its actual field",
        ));
    }
    Ok(true)
}

fn pure(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    fee(b, 1)?;
    if depth > b.partial_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "pure Copy type depth exceeded",
            None,
        )]);
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
        | Type::F64 => Ok(true),
        Type::Tuple(fields) => {
            for field in fields {
                if !pure(field, b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Array { element, .. } => pure(element, b, depth + 1),
        Type::ErrorUnion { error, result } => {
            Ok(pure(error, b, depth + 1)? && pure(result, b, depth + 1)?)
        }
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                if !pure(field, b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                fee(b, 1)?;
                for field in fields {
                    if !pure(field, b, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        Type::BorrowedStorage { .. } => {
            physical(ty, b)?;
            retained::pure(ty, b)
        }
        _ => Ok(false),
    }
}

pub(super) fn types(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> CompileResult<()> {
    let result = value_type(
        f,
        i.result
            .ok_or_else(|| failure("pure Copy operation has no result"))?,
    )?;
    if !pure(result, b, 0)? {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "pure Copy operation result contains an address, capability or nonordinary payload",
            Some(i.span),
        )]);
    }
    let expected = match &i.kind {
        InstructionKind::PlaceTagLocal { local, path } => {
            if !path.is_empty() {
                return Err(failure("pure ErrorUnion tag requires exact local root"));
            }
            let ty = &f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| failure("pure ErrorUnion tag local is absent"))?
                .ty;
            if !matches!(ty, Type::ErrorUnion { .. }) || !pure(ty, b, 0)? {
                return Err(failure(
                    "pure ErrorUnion tag lacks actual complete Copy union",
                ));
            }
            &Type::I32
        }
        InstructionKind::MakeErrorUnion { is_error, value } => {
            let Type::ErrorUnion {
                error,
                result: success,
            } = result
            else {
                return Err(failure(
                    "pure ErrorUnion construction lacks actual union type",
                ));
            };
            let arm = if *is_error {
                error.as_ref()
            } else {
                success.as_ref()
            };
            b.type_work(arm, false, "pure ErrorUnion selected arm")?;
            if value_type(f, *value)? != arm {
                return Err(failure("pure ErrorUnion input differs from selected arm"));
            }
            result
        }
        InstructionKind::ExtractField { aggregate, index } => {
            let source = value_type(f, *aggregate)?;
            if !pure(source, b, 0)? {
                return Err(failure(
                    "pure Copy field source contains a target-bearing payload",
                ));
            }
            match super::carrier(source).map_or(source, |(storage, _)| storage) {
                Type::Tuple(fields) => fields.get(*index as usize),
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, ty)| ty),
                _ => None,
            }
            .ok_or_else(|| failure("pure Copy field has no actual inline layout"))?
        }
        InstructionKind::ExtractIndex { aggregate, index } => {
            let source = value_type(f, *aggregate)?;
            if !pure(source, b, 0)? || !value_type(f, *index)?.is_integer() {
                return Err(failure(
                    "pure Copy array source or integer index is invalid",
                ));
            }
            let Type::Array { element, .. } = source else {
                return Err(failure("pure Copy index requires a fixed array"));
            };
            element.as_ref()
        }
        InstructionKind::UpdateField {
            aggregate,
            index,
            value,
        } => {
            let source = value_type(f, *aggregate)?;
            if !pure(source, b, 0)? {
                return Err(failure("pure field update source retains a real target"));
            }
            let physical = super::carrier(source).map_or(source, |(storage, _)| storage);
            let field = match physical {
                Type::Tuple(fields) => fields.get(*index as usize),
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, ty)| ty),
                _ => None,
            }
            .ok_or_else(|| failure("pure field update crosses actual inline layout"))?;
            b.type_work(field, false, "pure field update actual child")?;
            b.type_work(
                value_type(f, *value)?,
                false,
                "pure field update actual input",
            )?;
            if value_type(f, *value)? != field {
                return Err(failure("pure field update changes complete child type"));
            }
            source
        }
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => {
            let source = value_type(f, *aggregate)?;
            if !pure(source, b, 0)? || !value_type(f, *index)?.is_integer() {
                return Err(failure("pure index update source or index is invalid"));
            }
            let Type::Array { element, .. } = source else {
                return Err(failure("pure index update requires actual fixed Array"));
            };
            b.type_work(element, false, "pure index update actual child")?;
            b.type_work(
                value_type(f, *value)?,
                false,
                "pure index update actual input",
            )?;
            if value_type(f, *value)? != element.as_ref() {
                return Err(failure("pure index update changes complete element type"));
            }
            source
        }
        InstructionKind::MakeVariant { variant, fields } => {
            let Type::Enum { variants, .. } =
                super::carrier(result).map_or(result, |(storage, _)| storage)
            else {
                return Err(failure("pure Copy variant result is not an enum"));
            };
            let (_, payload) = variants
                .get(*variant as usize)
                .ok_or_else(|| failure("pure Copy variant is outside actual enum"))?;
            fee(b, payload.len().saturating_add(fields.len()))?;
            if payload.len() != fields.len() {
                return Err(failure("pure Copy variant payload count differs"));
            }
            for (field, expected) in fields.iter().zip(payload) {
                b.type_work(expected, false, "pure Copy variant complete field equality")?;
                b.type_work(
                    value_type(f, *field)?,
                    false,
                    "pure Copy variant actual input",
                )?;
                if value_type(f, *field)? != expected {
                    return Err(failure("pure Copy variant changes a complete field type"));
                }
            }
            result
        }
        InstructionKind::VariantTag { value } => {
            let source = value_type(f, *value)?;
            if !matches!(
                super::carrier(source).map_or(source, |(storage, _)| storage),
                Type::Enum { .. }
            ) || !pure(source, b, 0)?
            {
                return Err(failure(
                    "pure Copy tag source is not a complete ordinary enum",
                ));
            }
            &Type::I32
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let source = value_type(f, *value)?;
            if !pure(source, b, 0)? {
                return Err(failure("pure Copy variant source contains stored targets"));
            }
            let Type::Enum { variants, .. } =
                super::carrier(source).map_or(source, |(storage, _)| storage)
            else {
                return Err(failure("pure Copy projection source is not an enum"));
            };
            variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize))
                .ok_or_else(|| failure("pure Copy variant field is outside actual layout"))?
        }
        _ => return Err(failure("unknown pure Copy operation")),
    };
    b.type_work(expected, false, "pure Copy exact projected type")?;
    b.type_work(result, false, "pure Copy exact result type")?;
    if expected != result {
        return Err(failure(
            "pure Copy operation changes its complete actual type",
        ));
    }
    Ok(())
}

fn ordinary_union_shape(union: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::ErrorUnion { error, result } = union else {
        return Ok(false);
    };
    b.type_work(union, false, "ordinary owning union complete shape")?;
    if union.is_copy_value() {
        return Ok(false);
    }
    fee(b, 2)?;
    ordinary_owned_fields(vec![(error.as_ref(), 0), (result.as_ref(), 0)], b)
}

fn ordinary_owned_fields(mut pending: Vec<(&Type, usize)>, b: &mut Budget) -> CompileResult<bool> {
    while let Some((ty, depth)) = pending.pop() {
        fee(b, 1)?;
        if depth > b.partial_depth {
            return Err(failure("ordinary union shape depth exceeded"));
        }
        match ty {
            Type::Address { .. }
            | Type::Desc(_)
            | Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::BorrowedFunctionPointer { .. }
            | Type::Task { .. } => return Ok(false),
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| (child, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (_, fields) in variants {
                    fee(b, fields.len())?;
                    pending.extend(fields.iter().map(|child| (child, depth + 1)));
                }
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|child| (child, depth + 1)));
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.push((error, depth + 1));
                pending.push((result, depth + 1));
            }
            Type::Managed(child)
            | Type::OwnedStore { element: child }
            | Type::Array { element: child, .. } => {
                fee(b, 1)?;
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
    Ok(true)
}

// A shared scalar/Copy projection does not carry any stored leaf from its
// actual owner. All children, including inactive Sum arms, are examined first;
// the ordinary responsibility/loan kernel still proves initialization/liveness.
fn ordinary_package_copy_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::BorrowPlaceLocal {
        local,
        path,
        mutable: false,
    } = &i.kind
    else {
        return Ok(false);
    };
    if path.is_empty() {
        return Ok(false);
    }
    let root = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("ordinary Package borrow local absent"))?
        .ty;
    if !matches!(root, Type::Package { .. }) {
        return Ok(false);
    }
    b.type_work(root, true, "ordinary Package complete Copy projection")?;
    if !ordinary_owned_fields(vec![(root, 0)], b)? {
        return Ok(false);
    }
    let mut selected = root;
    fee(b, path.len())?;
    for step in path {
        selected = match (step, selected) {
            (PlaceProjection::Field(index), Type::Package { fields, .. }) => {
                &fields
                    .get(*index as usize)
                    .ok_or_else(|| failure("ordinary Package Copy path absent"))?
                    .1
            }
            _ => return Ok(false),
        };
    }
    if !selected.is_copy_value() || !pure(selected, b, 0)? {
        return Ok(false);
    }
    if !matches!(value_type(f,i.result.ok_or_else(||failure("ordinary Package Copy borrow lacks result"))?)?,Type::Address{mutable:false,inner} if inner.as_ref()==selected)
    {
        return Err(failure("ordinary Package Copy borrow result differs"));
    }
    b.ordinary_error_places = true;
    Ok(true)
}

// A selected Copy arm of an ordinary owned union is a new producer.
// Its current tag and initialized/complete subtree are checked by Sum flow.
pub(super) fn ordinary_error_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    if ordinary_package_copy_borrow(f, i, b)? {
        return Ok(true);
    }
    let InstructionKind::BorrowPlaceLocal {
        local,
        path,
        mutable: false,
    } = &i.kind
    else {
        return Ok(false);
    };
    let [PlaceProjection::ErrorArm(is_error)] = path.as_slice() else {
        return Ok(false);
    };
    let parent = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("ordinary error borrow local is absent"))?
        .ty;
    let Type::ErrorUnion { error, result } = parent else {
        return Ok(false);
    };
    let (selected, companion) = if *is_error {
        (error.as_ref(), result.as_ref())
    } else {
        (result.as_ref(), error.as_ref())
    };
    if !matches!(companion, Type::Package { .. }) || !ordinary_union_shape(parent, b)? {
        return Ok(false);
    }
    b.type_work(selected, true, "ordinary Copy selected arm borrow")?;
    if !selected.is_copy_value() {
        return Ok(false);
    }
    if !pure(selected, b, 0)? {
        return Ok(false);
    }
    if !matches!(value_type(f, i.result.ok_or_else(|| failure("ordinary error borrow lacks result"))?)?, Type::Address { mutable: false, inner } if inner.as_ref() == selected)
    {
        return Err(failure(
            "ordinary error borrow result differs from its complete actual arm",
        ));
    }
    b.ordinary_error_places = true;
    Ok(true)
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
            assert!(!pure(&ty, &mut Budget::new(&limits), 0).unwrap());
        }
    }
}
