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
            | InstructionKind::ReadErrorUnion { .. }
            | InstructionKind::ErrorTag { .. }
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
    if !b.masked_stored_tasks() {
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
        InstructionKind::ReadErrorUnion { value, .. } if b.task_failure_carriers => vt(f, *value)?,
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
    if matches!(i.kind, InstructionKind::ReadErrorUnion { .. }) && !union.is_copy_value() {
        return Ok(false);
    }
    if !ordinary_union_shape(union, b)? {
        return Ok(false);
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
    if let InstructionKind::TakeErrorUnion { is_error, .. }
    | InstructionKind::ReadErrorUnion { is_error, .. } = i.kind
    {
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

// A Copy observation still borrows its real owning parent. This classifier
// admits only complete address-free ordinary trees, never a Copy flag alone.
pub(super) fn ordinary_copy_field_borrow(
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
    let field = &fields
        .get(index as usize)
        .ok_or_else(|| err("ordinary Copy field borrow selects an absent field"))?
        .1;
    // Owning/qualified Package children retain their existing protocols.
    if !matches!(
        field,
        Type::Unit
            | Type::Bool
            | Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::Record { .. }
            | Type::Tuple(_)
            | Type::Array { .. }
            | Type::Enum { .. }
            | Type::ErrorUnion { .. }
    ) {
        return Ok(false);
    }
    super::type_work(field, &b.limits.clone(), b)?;
    let mut pending = vec![(field, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth {
            return Err(limit("ordinary Copy field borrow type depth exceeded"));
        }
        match ty {
            Type::Unit
            | Type::Bool
            | Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64 => {}
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Array { element, .. } => pending.push((element, depth + 1)),
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.push((error, depth + 1));
                pending.push((result, depth + 1));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
                }
            }
            _ => {
                return Err(unsupported(
                    "ordinary Copy field borrow hides an address, capability, qualified carrier or owner",
                    Some(i.span),
                ));
            }
        }
    }
    let output = vt(
        f,
        i.result
            .ok_or_else(|| err("ordinary Copy field borrow lacks result"))?,
    )?;
    if !matches!(output, Type::Address { mutable: actual, inner } if *actual == mutable && inner.as_ref() == field)
    {
        return Err(err(
            "ordinary Copy field borrow differs from the complete actual field",
        ));
    }
    Ok(true)
}

pub(super) fn ordinary_package_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if qualified_package_borrow(f, i, b)? {
        return Ok(true);
    }
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

// The independent raw checker authenticates the actual ordinary owner and
// child before the stored ledger leaves this scalar/Copy observation empty.
pub(super) fn ordinary_package_read(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let InstructionKind::ReadPackageField { local, index } = i.kind else {
        return Ok(false);
    };
    let parent = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| err("ordinary Package read local is absent"))?
        .ty;
    let Type::Package { fields, .. } = parent else {
        return Ok(false);
    };
    let field = &fields
        .get(index as usize)
        .ok_or_else(|| err("ordinary Package read field is absent"))?
        .1;
    resources::charge_payload(field, b)?;
    if !field.is_copy_value() {
        return Err(err(
            "ordinary Package read requires a complete pure Copy field",
        ));
    }
    ordinary(field, b, 0)?;
    let output = vt(
        f,
        i.result
            .ok_or_else(|| err("ordinary Package read lacks result"))?,
    )?;
    if output != field {
        return Err(err(
            "ordinary Package read result differs from its actual field",
        ));
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
        | Type::I8
        | Type::I16
        | Type::Isize
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::U8
        | Type::U16
        | Type::U32
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
        InstructionKind::ErrorTag { value } | InstructionKind::ReadErrorUnion { value, .. } => {
            let union = vt(f, *value)?;
            // Inspect both arms before observing the tag or selecting one.
            // A scalar selected arm cannot erase a hidden borrowed sibling.
            ordinary(union, b, 0)?;
            let Type::ErrorUnion { error, result } = union else {
                return Err(err("ordinary union observation has no actual ErrorUnion"));
            };
            match i.kind {
                InstructionKind::ReadErrorUnion { is_error: true, .. } => error.as_ref(),
                InstructionKind::ReadErrorUnion {
                    is_error: false, ..
                } => result.as_ref(),
                _ => &Type::Bool,
            }
        }
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

fn ordinary_union_shape(union: &Type, b: &mut Budget) -> Check<bool> {
    let Type::ErrorUnion { error, result } = union else {
        return Ok(false);
    };
    super::type_work(union, &b.limits.clone(), b)?;
    if union.is_copy_value() && !b.task_failure_carriers {
        return Ok(false);
    }
    b.spend(2)?;
    ordinary_owned_fields(vec![(error.as_ref(), 0), (result.as_ref(), 0)], b)
}

fn ordinary_owned_fields(mut pending: Vec<(&Type, usize)>, b: &mut Budget) -> Check<bool> {
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
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::F32
            | Type::F64
            | Type::ByteBuffer => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

// Only shared Copy endpoints of a complete address-free ordinary Package
// can enter this no-stored-target lane. Current lifetime and moved-field
// facts remain checked against the real local by the independent flow.
fn ordinary_package_field_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
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
        .ok_or_else(|| err("ordinary Package borrow local absent"))?
        .ty;
    if !matches!(root, Type::Package { .. }) {
        return Ok(false);
    }
    super::type_work(root, &b.limits.clone(), b)?;
    if !ordinary_owned_fields(vec![(root, 0)], b)? {
        return Ok(false);
    }
    let mut endpoint = root;
    b.spend(path.len())?;
    for step in path {
        let (PlaceProjection::Field(index), Type::Package { fields, .. }) = (step, endpoint) else {
            return Ok(false);
        };
        endpoint = &fields
            .get(*index as usize)
            .ok_or_else(|| err("ordinary Package Copy path absent"))?
            .1;
    }
    if endpoint.is_copy_value() {
        ordinary(endpoint, b, 0)?;
    } else if !matches!(endpoint, Type::Managed(_)) {
        return Ok(false);
    }
    if !matches!(vt(f,i.result.ok_or_else(||err("ordinary Package Copy borrow lacks result"))?)?,Type::Address{mutable:false,inner} if inner.as_ref()==endpoint)
    {
        return Err(err("ordinary Package Copy borrow result differs"));
    }
    b.ordinary_error_places = true;
    Ok(true)
}

// An actual selected Copy arm of an ordinary owned union is a new producer.
// Its current tag and initialized/complete subtree are checked by Sum flow.
pub(super) fn ordinary_error_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if ordinary_package_field_borrow(f, i, b)? {
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
    let selected_error = match path.as_slice() {
        [PlaceProjection::ErrorArm(error)] => *error,
        _ => return Ok(false),
    };
    let parent = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| err("ordinary error borrow local is absent"))?
        .ty;
    let Type::ErrorUnion { error, result } = parent else {
        return Ok(false);
    };
    let selected = if selected_error {
        error.as_ref()
    } else {
        result.as_ref()
    };
    let other = if selected_error {
        result.as_ref()
    } else {
        error.as_ref()
    };
    resources::charge_payload(parent, b)?;
    if parent.is_copy_value() {
        // Both arms must be target-free before an ordinary selected-arm view
        // can omit stored permissions. An inactive sibling is not evidence.
        ordinary(parent, b, 0)?;
    } else if !matches!(other, Type::Package { .. }) || !ordinary_union_shape(parent, b)? {
        return Ok(false);
    }
    resources::charge_payload(selected, b)?;
    if !selected.is_copy_value() {
        return Ok(false);
    }
    ordinary(selected, b, 0)?;
    if !matches!(vt(f, i.result.ok_or_else(|| err("ordinary error borrow lacks result"))?)?, Type::Address { mutable: false, inner } if inner.as_ref() == selected)
    {
        return Err(err(
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
            assert!(ordinary(&ty, &mut Budget::new(&limits), 0).is_err());
        }
    }
}

/// A package projection retains the actual owner root; it never creates an
/// external borrowed slot or supplies ownership from the qualified shell.
pub(super) fn qualified_package_description(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let InstructionKind::DescribePackageField { local, index } = i.kind else {
        return Ok(false);
    };
    let parent = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| err("description owner local absent"))?
        .ty;
    let Type::BorrowedStorage { storage, .. } = parent else {
        return Ok(false);
    };
    let Type::Package { fields, .. } = storage.as_ref() else {
        return Ok(false);
    };
    let (_, field) = fields
        .get(index as usize)
        .ok_or_else(|| err("description owning field absent"))?;
    let Type::Managed(payload) = field else {
        return Err(err("description field is not an actual managed owner"));
    };
    // Reuse the complete pure payload check, including every inactive arm.
    ordinary(payload, b, 0)?;
    let output = vt(
        f,
        i.result
            .ok_or_else(|| err("description lacks a typed result"))?,
    )?;
    resources::charge_payload(output, b)?;
    if !matches!(output, Type::Desc(actual) if actual == payload) {
        return Err(err(
            "description differs from its exact managed field payload",
        ));
    }
    Ok(true)
}

pub(super) fn qualified_package_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    let InstructionKind::BorrowPackageField {
        local,
        index,
        mutable: false,
    } = i.kind
    else {
        return Ok(false);
    };
    let parent = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| err("owning field view local absent"))?
        .ty;
    let Type::BorrowedStorage { storage, .. } = parent else {
        return Ok(false);
    };
    let Type::Package { fields, .. } = storage.as_ref() else {
        return Ok(false);
    };
    let (_, field) = fields
        .get(index as usize)
        .ok_or_else(|| err("owning field view index absent"))?;
    let Type::Managed(payload) = field else {
        return Ok(false);
    };
    ordinary(payload, b, 0)?;
    let out = vt(
        f,
        i.result
            .ok_or_else(|| err("owning field view result absent"))?,
    )?;
    resources::charge_payload(out, b)?;
    if !matches!(out, Type::Address { mutable: false, inner } if inner.as_ref() == field) {
        return Err(err("owning field view changes its exact managed payload"));
    }
    Ok(true)
}

#[cfg(test)]
mod copy_field_borrow_tests {
    use super::*;
    mod actual_input {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/nested-store/raw-managers.rs"
        ));
    }
    fn request(field: Type, output: Type) -> (TypedFunction, Instruction) {
        let (mut module, _) = actual_input::request(actual_input::Scenario::default());
        let f = &mut module.functions[0];
        let Type::Package { fields, .. } = &mut f.locals[1].ty else {
            panic!()
        };
        fields.push(("key".into(), field));
        let id = ValueId(f.values.len() as u32);
        f.values.push(output);
        let instruction = Instruction {
            result: Some(id),
            kind: InstructionKind::BorrowPackageField {
                local: LocalId(1),
                index: 1,
                mutable: false,
            },
            span: f.span,
        };
        (module.functions.remove(0), instruction)
    }
    fn key(child: Type) -> Type {
        Type::Record {
            name: "FieldKey".into(),
            fields: vec![("id".into(), child)],
        }
    }
    fn address(inner: Type) -> Type {
        Type::Address {
            mutable: false,
            inner: Box::new(inner),
        }
    }
    #[test]
    fn actual_field_identity_and_complete_address_free_copy_tree_are_required() {
        let complete = key(Type::Tuple(vec![
            Type::U32,
            Type::Array {
                element: Box::new(Type::I64),
                length: 2,
            },
            Type::Enum {
                name: "ClosedTag".into(),
                variants: vec![("Empty".into(), vec![]), ("Value".into(), vec![Type::F64])],
            },
        ]));
        let (f, i) = request(complete.clone(), address(complete));
        let mut budget = Budget::new(&Limits::default());
        budget.store_option_results = true;
        assert!(ordinary_copy_field_borrow(&f, &i, &mut budget).unwrap());
        for hidden in [
            address(Type::I32),
            Type::Desc(Box::new(Type::ByteBuffer)),
            Type::Managed(Box::new(Type::ByteBuffer)),
            Type::FunctionPointer {
                parameters: vec![],
                result: Box::new(Type::I32),
            },
            Type::ForeignCallback {
                parameters: vec![],
                result: Box::new(Type::I32),
            },
        ] {
            let field = key(Type::Tuple(vec![Type::Array {
                element: Box::new(hidden),
                length: 3,
            }]));
            let (f, i) = request(field.clone(), address(field));
            assert!(ordinary_copy_field_borrow(&f, &i, &mut budget).is_err());
        }
        let field = key(Type::U32);
        let same_layout = Type::Record {
            name: "OtherKey".into(),
            fields: vec![("id".into(), Type::U32)],
        };
        let (f, i) = request(field.clone(), address(same_layout));
        assert!(ordinary_copy_field_borrow(&f, &i, &mut budget).is_err());
        let (f, i) = request(
            field.clone(),
            Type::Address {
                mutable: true,
                inner: Box::new(field),
            },
        );
        assert!(ordinary_copy_field_borrow(&f, &i, &mut budget).is_err());
    }
}
