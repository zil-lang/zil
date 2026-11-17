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

pub(super) fn code_atom(ty:&Type,b:&mut Budget)->Check<bool>{
    match ty{
        Type::FunctionPointer{..}=>crate::class_flow::plain_pointer_type(ty,b),
        Type::ForeignCallback{parameters,result}=>{resources::charge_payload(ty,b)?;crate::native_values::callback_signature(parameters,result,None)?;Ok(true)},
        _=>Ok(false),
    }
}

// Ordinary owning sums retain their tag/debt in the independent resource
// kernel. Complete target-free children, including closed owning closures,
// have no stored-address permission; this never makes the whole sum Copy.
pub(super) fn ordinary_owned_union(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    // Flat owning ErrorUnion/Package values predate the sum-extension work
    // flag. Authenticate the complete actual shape here; a scheduling flag
    // neither supplies this proof nor excludes an ordinary existing carrier.
    if ordinary_owned_enum(module,f,i,b)?{return Ok(true);}
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
        InstructionKind::DropErrorUnion{local}=>&f.locals.get(local.0 as usize).ok_or_else(||err("ordinary owning sum cleanup local is absent"))?.ty,
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
    if !ordinary_union_shape(module,union, b)? {
        return Ok(false);
    }
    if matches!(i.kind,InstructionKind::DropErrorUnion{..})&&i.result.is_some(){return Err(err("ordinary owning sum cleanup has a value result"));}
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
        Type::FunctionPointer{..}|Type::ForeignCallback{..}=>{
            if !code_atom(ty,b)?{return Err(unsupported("pure Copy code atom lacks its complete finite signature/ABI domain",None));}
        },
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

pub(super) fn types(module:&TypedModule,f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<()> {
    types_inner(module,f,i,b).map_err(|mut failure|{if failure.span.is_none(){failure.span=Some(i.span);}failure})
}
fn types_inner(module:&TypedModule,f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<()> {
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
            // Reading a selected Copy child does not copy an owning sibling.
            // Rich pointer-bearing parents keep their separate permission
            // consumers; a complete target-free product may own other fields.
            if !complete_owned_fields(module,vec![(aggregate,0)],b)?{ordinary(aggregate, b, 0)?;}
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

fn ordinary_union_shape(module:&TypedModule,union: &Type, b: &mut Budget) -> Check<bool> {
    let Type::ErrorUnion { error, result } = union else {
        return Ok(false);
    };
    super::type_work(union, &b.limits.clone(), b)?;
    if union.is_copy_value() && !b.task_failure_carriers {
        return Ok(false);
    }
    b.spend(2)?;
    complete_owned_fields(module,vec![(error.as_ref(), 0), (result.as_ref(), 0)],b)
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
            Type::FunctionPointer{..}|Type::ForeignCallback{..}=>if !code_atom(ty,b)?{return Ok(false);},
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn complete_owned_fields(module:&TypedModule,pending:Vec<(&Type,usize)>,b:&mut Budget)->Check<bool>{
    for(ty,_)in pending{if !crate::stored_owned::target_free_value(module,ty,b)?{return Ok(false);}}
    Ok(true)
}

// A concrete callback keeps its own complete opaque boundary. This lane
// transports its real physical captured pointers; it does not classify that
// environment target-free or infer CE/SyncDrop from its number of classes.
pub(super) fn callable_tuple(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if !matches!(ty,Type::Tuple(_)){return Ok(false);}
    let mut pending = vec![(ty, 0usize)];
    let mut callable = false;
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth { return Err(limit("callable Tuple domain depth exceeded")); }
        match ty {
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Closure(id) => {
                let (count, _) = crate::dynamic_witness::intrinsic_domain(module, ty, b)?;
                if count != 0 { return Ok(false); }
                // Independently enumerate actual physical capture occurrences,
                // including real Static fields. No pointer is made from N0.
                returned_environment::leaves(module, *id, b)?;
                callable = true;
            }
            other => if !crate::stored_owned::target_free_value(module, other, b)? { return Ok(false); },
        }
    }
    if callable { crate::sums::check_type_for(module, ty, None, b)?; }
    Ok(callable)
}

// Physical callback capture rows have a real private Field path. Logical
// carrier namespaces must use their own complete transport, never disappear
// into that path or an empty target permission.
pub(super) fn physical_tuple_payload(p: &Payload, b: &mut Budget) -> Check<()> {
    payload_work(p, b)?;
    if !p.address.sources.is_empty() || !p.address.loans.is_empty()
        || !p.store_description.sources.is_empty() || !p.store_description.loans.is_empty()
        || !p.task_ports.is_empty() || !p.dyn_ports.0.is_empty() || !p.dyn_views.is_empty()
        || p.option_variants.mask() != 0
    {
        return Err(unsupported("ordinary callable Tuple requires its independent logical carrier transport", None));
    }
    Ok(())
}

// Ordinary affine products may have no stored targets while still owning
// resources. Complete actual type and Sum checks precede their transfers;
// neither this lane nor a zero domain discharges tag/init/debt/cleanup facts.
pub(super) fn ordinary_owned_tuple(module:&TypedModule,f:&TypedFunction,i:&Instruction,b:&mut Budget)->Check<bool>{
    let ty=match &i.kind{
        InstructionKind::MakeOwnedTuple{..}=>vt(f,i.result.ok_or_else(||err("ordinary owning Tuple constructor lacks result"))?)?,
        InstructionKind::UnpackTuple{local,..}|InstructionKind::DropTuple{local}|InstructionKind::TakeTupleField{local,..}=>&f.locals.get(local.0 as usize).ok_or_else(||err("ordinary owning Tuple has no actual local"))?.ty,
        _=>return Ok(false),
    };
    if !matches!(ty,Type::Tuple(_))||ty.is_copy_value()||!(complete_owned_value(module,ty,b)?||callable_tuple(module,ty,b)?){return Ok(false);}
    crate::sums::check_instruction(f,i,&b.limits.clone(),b)?;
    Ok(true)
}

pub(super) fn ordinary_tuple_transfer(f:&TypedFunction,i:&Instruction,state:&mut Frame,values:&[Payload],out:&mut Payload,b:&mut Budget)->Check<()>{
    match &i.kind{
        InstructionKind::MakeOwnedTuple{fields}=>{
            for(index,value)in fields.iter().enumerate(){
                let payload=values.get(value.0 as usize).ok_or_else(||err("ordinary owning Tuple input is absent"))?;
                physical_tuple_payload(payload,b)?;
                for(tail,permission)in &payload.leaves{b.spend(tail.len().saturating_add(1))?;let mut path=vec![PlaceProjection::Field(index as u32)];path.extend(tail);add_leaf(out,&path,permission,b)?;}
            }
        },
        InstructionKind::UnpackTuple{local,fields}=>{
            let payload=copy_payload(&state.locals[local.0 as usize],b)?;
            physical_tuple_payload(&payload,b)?;
            for(index,target)in fields.iter().enumerate(){
                let prefix=[PlaceProjection::Field(index as u32)];let mut child=Payload::default();
                child.enum_variants=payload.enum_variants.project(&prefix,b)?;
                for(path,permission)in &payload.leaves{b.spend(path.len().saturating_add(1))?;if let Some(tail)=path.strip_prefix(&prefix){add_leaf(&mut child,tail,permission,b)?;}}
                clear_local_holes(state,*target,b)?;state.locals[target.0 as usize]=child;
            }
            state.locals[local.0 as usize]=Payload::default();clear_local_holes(state,*local,b)?;
        },
        InstructionKind::TakeTupleField{local,path}=>{
            let holder=&mut state.locals[local.0 as usize];physical_tuple_payload(holder,b)?;out.enum_variants=holder.enum_variants.project(path,b)?;
            b.spend(holder.leaves.len())?;let mut remaining=Vec::with_capacity(holder.leaves.len());
            for(actual,permission)in std::mem::take(&mut holder.leaves).into_vec(b)?{
                b.spend(actual.len().saturating_add(path.len()))?;
                if let Some(tail)=actual.strip_prefix(path.as_slice()){add_leaf(out,tail,&permission,b)?;}else{b.spend(1)?;remaining.push((actual,permission));}
            }
            holder.leaves=leaf_rows::Rows::from_vec(remaining,b)?;
        },
        InstructionKind::DropTuple{local}=>{physical_tuple_payload(&state.locals[local.0 as usize],b)?;state.locals[local.0 as usize]=Payload::default();clear_local_holes(state,*local,b)?;},
        _=>return Err(err("ordinary owning Tuple transfer has another actual opcode")),
    }
    let _=f;Ok(())
}

// Only shared Copy endpoints of a complete address-free ordinary Package
// can enter this no-stored-target lane. Current lifetime and moved-field
// facts remain checked against the real local by the independent flow.
fn ordinary_package_field_borrow(
    module: &TypedModule,
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
    if !complete_owned_fields(module,vec![(root, 0)],b)? {
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
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if ordinary_package_field_borrow(module,f, i, b)? {
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
    } else if b.task_sources() && matches!(other,Type::Managed(inner)if matches!(inner.as_ref(),Type::Task{..})) {
        // This does not classify the union or its Task arm as target-free.
        // Only the selected pure Copy arm is borrowed below. The sibling's
        // complete Task domain, actual ports, parents and phase stay in the
        // existing Task and affine-sum consumers and in the same local state.
        crate::dynamic_witness::intrinsic_domain(module,other,b)?;
    } else if !matches!(other, Type::Package { .. }) || !ordinary_union_shape(module,parent, b)? {
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
    let storage = match parent {
        Type::BorrowedStorage { storage, .. } => storage.as_ref(),
        Type::Package { .. } => parent,
        _ => return Ok(false),
    };
    let Type::Package { fields, .. } = storage else {
        return Ok(false);
    };
    let (_, field) = fields
        .get(index as usize)
        .ok_or_else(|| err("description owning field absent"))?;
    let Type::Managed(payload) = field else {
        return Err(err("description field is not an actual managed owner"));
    };
    // The ordinary Package bridge selects the class-free Record description
    // protocol. Store/ByteBuffer fields retain their separate description,
    // allocation and owner-epoch consumers; an earlier classifier must not
    // reject them as failed pure values before those consumers can run.
    if matches!(parent, Type::Package { .. }) && !matches!(payload.as_ref(), Type::Record { .. }) {
        return Ok(false);
    }
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
        let Type::Managed(header) = &f.locals[1].ty else {
            panic!()
        };
        let Type::OwnedStore { element } = header.as_ref() else {
            panic!()
        };
        f.locals[1].ty = element.as_ref().clone();
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

// Ordinary owning enums enter the same no-stored-target lane as owning error
// unions. This verifies complete physical payloads, not Copy eligibility or
// the active tag, move state, local initialization or cleanup responsibility.
fn ordinary_owned_enum(module:&TypedModule,f:&TypedFunction,i:&Instruction,b:&mut Budget)->Check<bool>{
    let ty=match &i.kind{
        InstructionKind::MakeVariant{..}=>vt(f,i.result.ok_or_else(||err("ordinary owning enum constructor has no result"))?)?,
        InstructionKind::VariantTag{value}|InstructionKind::ReadVariant{value,..}=>vt(f,*value)?,
        InstructionKind::PlaceTagLocal{local,path}if path.is_empty()=>&f.locals.get(local.0 as usize).ok_or_else(||err("ordinary owning enum tag has no local"))?.ty,
        InstructionKind::DropEnum{local}=>&f.locals.get(local.0 as usize).ok_or_else(||err("ordinary owning enum cleanup local is absent"))?.ty,
        _=>return Ok(false),
    };
    let Type::Enum{variants,..}=ty else{return Ok(false);};
    type_work(ty,&b.limits.clone(),b)?;
    if ty.is_copy_value()||!complete_owned_fields(module,vec![(ty,0)],b)?{return Ok(false);}
    if matches!(i.kind,InstructionKind::DropEnum{..}){if i.result.is_some(){return Err(err("ordinary owning enum cleanup has a value result"));}return Ok(true);}
    let output=vt(f,i.result.ok_or_else(||err("ordinary owning enum operation has no result"))?)?;
    match &i.kind{
        InstructionKind::MakeVariant{variant,fields}=>{
            let payload=&variants.get(*variant as usize).ok_or_else(||err("ordinary owning enum selects an absent variant"))?.1;
            b.spend(payload.len().saturating_add(fields.len()))?;
            if payload.len()!=fields.len(){return Err(err("ordinary owning enum constructor omits a payload field"));}
            for(field,expected)in fields.iter().zip(payload){let actual=vt(f,*field)?;type_work(actual,&b.limits.clone(),b)?;if actual!=expected{return Err(err("ordinary owning enum payload changes a complete field type"));}}
            if output!=ty{return Err(err("ordinary owning enum constructor changes its nominal result"));}
        },
        InstructionKind::VariantTag{..}|InstructionKind::PlaceTagLocal{..}=>if output!=&Type::I32{return Err(err("ordinary owning enum discriminator has another result type"));},
        InstructionKind::ReadVariant{variant,index,..}=>{
            let expected=variants.get(*variant as usize).and_then(|(_,fields)|fields.get(*index as usize)).ok_or_else(||err("ordinary owning enum read selects an absent complete field"))?;
            if !expected.is_copy_value()||output!=expected{return Err(err("ordinary owning enum observation cannot move or change a non-Copy payload"));}
        },
        _=>unreachable!(),
    }
    Ok(true)
}

pub(super) fn ordinary_owned_value(ty:&Type,b:&mut Budget)->Check<bool>{
    if !matches!(ty,Type::Package{..}|Type::Enum{..}|Type::ErrorUnion{..}|Type::Tuple(_)|Type::Array{..}){return Ok(false);}
    type_work(ty,&b.limits.clone(),b)?;
    ordinary_owned_fields(vec![(ty,0)],b)
}

pub(super) fn complete_owned_value(module:&TypedModule,ty:&Type,b:&mut Budget)->Check<bool>{
    if !matches!(ty,Type::Record{..}|Type::Package{..}|Type::Enum{..}|Type::ErrorUnion{..}|Type::Tuple(_)|Type::Array{..}){return Ok(false);}
    complete_owned_fields(module,vec![(ty,0)],b)
}

#[cfg(test)]
mod owned_observation_tests{
    use super::*;
    #[test]
    fn package_record_description_does_not_intercept_other_owner_protocols_raw_wire() {
        let limits = Limits::default();
        let (mut module, plan) = crate::tests::baseline();
        let record = Type::Record { name: "Ticket".into(), fields: vec![("code".into(), Type::I32)] };
        let store = Type::OwnedStore { element: Box::new(Type::I32) };
        let hidden = Type::Record { name: "Borrowing".into(), fields: vec![("pointer".into(),
            Type::Address { mutable: false, inner: Box::new(Type::I32) })] };
        let payloads = [record, store, Type::ByteBuffer, hidden];
        module.functions[0].locals[0].ty = Type::Package {
            name: "Bag".into(),
            fields: payloads.iter().enumerate().map(|(index, payload)|
                (format!("field{index}"), Type::Managed(Box::new(payload.clone())))).collect(),
        };
        module.functions[0].values = payloads.iter().map(|payload|
            Type::Desc(Box::new(payload.clone()))).chain(std::iter::once(Type::I32)).collect();
        for wire in [false, true] {
            let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            let (raw, _) = if wire { zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap() }
                else { (module.clone(), plan.clone()) };
            let f = &raw.functions[0];
            let describe = |index: u32, result: u32| Instruction {
                result: Some(ValueId(result)),
                kind: InstructionKind::DescribePackageField { local: LocalId(0), index },
                span: f.span,
            };
            let mut b = Budget::new(&limits);
            assert!(qualified_package_description(f, &describe(0, 0), &mut b).unwrap());
            // False selects another consumer; it never authenticates a Store
            // allocation, owner epoch, description or actual access window.
            assert!(!qualified_package_description(f, &describe(1, 1), &mut b).unwrap());
            assert!(!qualified_package_description(f, &describe(2, 2), &mut b).unwrap());
            assert!(qualified_package_description(f, &describe(0, 4), &mut b).is_err());
            assert!(qualified_package_description(f, &describe(3, 3), &mut b).is_err(),
                "the complete Record payload must not hide an Address as Copy");
        }
    }
    #[test]
    fn copy_error_arm_keeps_the_actual_task_sibling_outside_the_target_free_lane_raw_wire(){
        let limits=Limits::default();
        for count in [0,1]{
            let(mut module,_)=crate::tests::baseline();
            let error=Type::Enum{name:"@std.task:v1:TaskCreateError".into(),variants:vec![("Allocation".into(),vec![])]};
            let leaf=Type::Address{mutable:false,inner:Box::new(Type::I32)};
            let task=Type::Managed(Box::new(Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{binder_count:count,ports:(0..count).map(|binder|RetainedBorrowPort{leaf:leaf.clone(),mode:StoredBorrowMode::SharedAddress,binder}).collect(),retained:(0..count).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class,binder:class}).collect(),nominal_arguments:vec![]}}));
            module.functions[0].locals[0].ty=Type::ErrorUnion{error:Box::new(error.clone()),result:Box::new(task)};
            module.functions[0].values=vec![Type::Address{mutable:false,inner:Box::new(error.clone())},Type::Address{mutable:true,inner:Box::new(error)},Type::Address{mutable:false,inner:Box::new(Type::Unit)}];
            let bytes=zil_ir::wire::encode_module_candidate(&module,&CandidatePlan::empty_for(&module),&limits).unwrap();let(wire,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
            for actual in [&module,&wire]{
                let f=&actual.functions[0];let mut b=Budget::new(&limits);b.task_borrowed_results=true;
                let borrow=|value,mutable,selected|Instruction{result:Some(ValueId(value)),kind:InstructionKind::BorrowPlaceLocal{local:LocalId(0),path:vec![PlaceProjection::ErrorArm(selected)],mutable},span:f.span};
                assert!(ordinary_error_borrow(actual,f,&borrow(0,false,true),&mut b).unwrap());
                assert!(!complete_owned_value(actual,&f.locals[0].ty,&mut b).unwrap(),"the Task-bearing union is not relabeled target-free");
                assert!(!ordinary_error_borrow(actual,f,&borrow(1,true,true),&mut b).unwrap());
                assert!(ordinary_error_borrow(actual,f,&borrow(2,false,true),&mut b).is_err());
                assert!(!ordinary_error_borrow(actual,f,&borrow(0,false,false),&mut b).unwrap());
            }
        }
    }
    #[test]
    fn ordinary_owned_tuple_uses_exact_distinct_destinations_and_transports_real_leaf_parents_raw_wire(){
        let limits=Limits::default();let(mut module,_)=crate::tests::baseline();
        let owner=Type::Managed(Box::new(Type::Record{name:"Quiet".into(),fields:vec![("value".into(),Type::I32)]}));
        let tuple=Type::Tuple(vec![owner.clone(),Type::I32]);
        module.functions[0].locals[0].ty=tuple.clone();
        for(id,ty)in [(1,owner.clone()),(2,Type::I32)]{let mut local=module.functions[0].locals[0].clone();local.id=LocalId(id);local.ty=ty;module.functions[0].locals.push(local);}
        module.functions[0].values=vec![owner,Type::I32,tuple];
        let bytes=zil_ir::wire::encode_module_candidate(&module,&CandidatePlan::empty_for(&module),&limits).unwrap();let(wire,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        for actual in [&module,&wire]{
            let f=&actual.functions[0];let mut b=Budget::new(&limits);b.function_count=actual.functions.len();
            let instruction=|result:Option<u32>,kind:InstructionKind|Instruction{result:result.map(ValueId),kind,span:f.span};
            let unpack=instruction(None,InstructionKind::UnpackTuple{local:LocalId(0),fields:vec![LocalId(1),LocalId(2)]});
            let make=instruction(Some(2),InstructionKind::MakeOwnedTuple{fields:vec![ValueId(0),ValueId(1)]});
            let take=instruction(Some(0),InstructionKind::TakeTupleField{local:LocalId(0),path:vec![PlaceProjection::Field(0)]});
            let drop=instruction(None,InstructionKind::DropTuple{local:LocalId(0)});
            for i in [&unpack,&make,&take,&drop]{assert!(ordinary_owned_tuple(actual,f,i,&mut b).unwrap());}
            let alias=instruction(None,InstructionKind::UnpackTuple{local:LocalId(0),fields:vec![LocalId(1),LocalId(1)]});assert!(ordinary_owned_tuple(actual,f,&alias,&mut b).is_err());
            let mismatch=instruction(None,InstructionKind::UnpackTuple{local:LocalId(0),fields:vec![LocalId(2),LocalId(1)]});assert!(ordinary_owned_tuple(actual,f,&mismatch,&mut b).is_err());
            // A private transfer control: existing field facts are supplied
            // explicitly, never inferred from this target-free metadata test.
            let permission=Permission{sources:vec![Source{base:HoldRoot::Parameter(7),leaf:vec![],inline:vec![],heap_parents:vec![],heap_element:None,retained_port:None,static_dependency:StaticProvenance::NotStatic}],loans:vec![11,12]};
            let mut state=Frame{locals:vec![Payload::default();3],..Frame::default()};add_leaf(&mut state.locals[0],&[PlaceProjection::Field(0),PlaceProjection::Field(3)],&permission,&mut b).unwrap();
            ordinary_tuple_transfer(f,&unpack,&mut state,&[],&mut Payload::default(),&mut b).unwrap();
            assert_eq!(leaf(&state.locals[1],&[PlaceProjection::Field(3)],&mut b).unwrap(),Some(&permission));assert!(state.locals[0].leaves.is_empty());
        }
    }
    // These test qualification and selected result identity, not an executed
    // Source body or a certificate for the synthetic module's resource state.
    #[test]
    fn owned_sum_tags_and_selected_copy_fields_do_not_copy_their_parent_raw_wire(){
        let limits=Limits::default();let(mut module,plan)=crate::tests::baseline();
        let relation=StorageLifeContract{fixed_binder_count:0,invocation_binder_count:0,bindings:vec![],classes:vec![],static_bindings:vec![]};
        module.functions[0].environment=Some(ClosureId(0));module.functions[0].storage_life=Some(relation.clone());
        module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:vec![Type::I32],parameters:vec![],result:Type::I32,kind:CallableKind::FnMut,storage_life:Some(relation),lending_captures:vec![]});
        let fields=vec![("callback".into(),Type::Closure(ClosureId(0))),("number".into(),Type::I64)];
        let owner=Type::Package{name:"State".into(),fields:fields.clone()};
        let union=Type::ErrorUnion{error:Box::new(Type::Unit),result:Box::new(owner.clone())};
        let enumeration=Type::Enum{name:"StateSum".into(),variants:vec![("Some".into(),vec![Type::Closure(ClosureId(0)),Type::I64]) ]};
        let record=Type::Record{name:"OwningFields".into(),fields};
        assert!(!owner.is_copy_value());assert!(!union.is_copy_value());assert!(!record.is_copy_value());
        module.functions[0].locals[0].ty=union.clone();
        let mut local=module.functions[0].locals[0].clone();local.id=LocalId(1);local.ty=owner.clone();module.functions[0].locals.push(local);
        module.functions[0].values=vec![union,Type::Bool,owner,Type::Unit,enumeration,Type::I32,Type::I64,record,Type::Closure(ClosureId(0))];
        for wire in [false,true]{
            let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
            let(candidate,_)=if wire{zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap()}else{(module.clone(),plan.clone())};
            let f=&candidate.functions[0];let mut b=Budget::new(&limits);
            let operation=|result:Option<u32>,kind:InstructionKind|Instruction{result:result.map(ValueId),kind,span:f.span};
            for i in [operation(Some(1),InstructionKind::ErrorTag{value:ValueId(0)}),operation(Some(2),InstructionKind::TakeErrorUnion{value:ValueId(0),is_error:false}),operation(Some(0),InstructionKind::MakeErrorUnion{value:ValueId(2),is_error:false}),operation(None,InstructionKind::DropErrorUnion{local:LocalId(0)}),operation(Some(5),InstructionKind::VariantTag{value:ValueId(4)}),operation(Some(6),InstructionKind::ReadVariant{value:ValueId(4),variant:0,index:1})]{assert!(ordinary_owned_union(&candidate,f,&i,&mut b).unwrap());}
            let read=operation(Some(6),InstructionKind::ReadPackageField{local:LocalId(1),index:1});assert!(ordinary_package_read(f,&read,&mut b).unwrap());
            let extract=operation(Some(6),InstructionKind::ExtractField{aggregate:ValueId(7),index:1});types(&candidate,f,&extract,&mut b).unwrap();
            let wrong=operation(Some(1),InstructionKind::ExtractField{aggregate:ValueId(7),index:1});assert!(types(&candidate,f,&wrong,&mut b).is_err());
            let copy_owner=operation(Some(8),InstructionKind::ExtractField{aggregate:ValueId(7),index:0});assert!(types(&candidate,f,&copy_owner,&mut b).is_err());
            let update=operation(Some(7),InstructionKind::UpdateField{aggregate:ValueId(7),index:1,value:ValueId(6)});assert!(types(&candidate,f,&update,&mut b).is_err());
            let read_owner=operation(Some(8),InstructionKind::ReadPackageField{local:LocalId(1),index:0});assert!(ordinary_package_read(f,&read_owner,&mut b).is_err());
            let enum_owner=operation(Some(8),InstructionKind::ReadVariant{value:ValueId(4),variant:0,index:0});assert!(ordinary_owned_union(&candidate,f,&enum_owner,&mut b).is_err());
        }
    }
}

#[cfg(test)]
#[path = "stored_flow_pure_copy/callable_tuple_tests.rs"]
mod callable_tuple_tests;
