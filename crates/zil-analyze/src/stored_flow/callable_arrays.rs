//! Permission transport for owning arrays with actual inline callable fields.
//! Resource/Task kernels retain initialization, occupant epoch and cleanup debt.
use super::*;

fn shape<'a>(
    module: &TypedModule,
    ty: &'a Type,
    b: &mut Budget,
) -> CompileResult<Option<(&'a Type, u32)>> {
    let Type::Array { element, length } = ty else {
        return Ok(None);
    };
    let storage = carrier(element).map_or(element.as_ref(), |(storage, _)| storage);
    if !matches!(storage, Type::Package { .. } | Type::Closure(_))
        || !environment_value(module, element, b)?
    {
        return Ok(None);
    }
    Ok(Some((element, *length)))
}
pub(super) fn admitted(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    shape(module, ty, b).map(|shape| shape.is_some())
}
pub(super) fn local(i: &Instruction) -> Option<(LocalId, Option<u32>)> {
    match i.kind {
        InstructionKind::BorrowOwnedArrayElement { local, index, .. }
        | InstructionKind::TakeOwnedArrayElement { local, index }
        | InstructionKind::DropOwnedArrayElement { local, index }
        | InstructionKind::StoreOwnedArrayElement { local, index, .. }
        | InstructionKind::ProjectOwnedArrayField { local, index, .. } => {
            Some((local, Some(index)))
        }
        InstructionKind::DropOwnedArray { local } => Some((local, None)),
        _ => None,
    }
}
pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    if let InstructionKind::MakeOwnedArray { .. } = i.kind {
        return i
            .result
            .map(|id| admitted(module, value_type(f, id)?, b))
            .transpose()
            .map(|value| value.unwrap_or(false));
    }
    let Some((local, _)) = local(i) else {
        return Ok(false);
    };
    let ty = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("callable array local is absent"))?
        .ty;
    admitted(module, ty, b)
}
pub(super) fn validate(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<()> {
    let output = i.result.map(|id| value_type(f, id)).transpose()?;
    if let InstructionKind::MakeOwnedArray { elements } = &i.kind {
        let (element, length) = shape(
            module,
            output.ok_or_else(|| failure("callable array constructor lacks its real output"))?,
            b,
        )?
        .ok_or_else(|| {
            failure("callable array constructor has no complete inline environment shape")
        })?;
        if length == 0 || elements.len() != length as usize {
            return Err(failure("callable array constructor omits a real element"));
        }
        for value in elements {
            b.type_work(element, false, "callable array complete element Type")?;
            if value_type(f, *value)? != element {
                return Err(failure(
                    "callable array constructor changes complete element Type",
                ));
            }
        }
        return Ok(());
    }
    let (local, index) =
        local(i).ok_or_else(|| failure("callable array operation has no real local"))?;
    let decl = f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("callable array local is absent"))?;
    let (element, length) = shape(module, &decl.ty, b)?
        .ok_or_else(|| failure("callable array has no actual owning element Type"))?;
    if index.is_some_and(|index| index >= length) {
        return Err(failure(
            "callable array operation leaves its complete element domain",
        ));
    }
    b.type_work(
        element,
        false,
        "callable array operation complete element Type",
    )?;
    let valid = match &i.kind {
        InstructionKind::BorrowOwnedArrayElement { mutable, .. } => {
            matches!(element, Type::Closure(_))
                && (!*mutable || decl.mutable)
                && matches!(output,Some(Type::Address {mutable:actual,inner}) if actual==mutable && inner.as_ref()==element)
        }
        InstructionKind::TakeOwnedArrayElement { .. } => output == Some(element),
        InstructionKind::DropOwnedArrayElement { .. } | InstructionKind::DropOwnedArray { .. } => {
            output.is_none()
        }
        InstructionKind::StoreOwnedArrayElement { value, .. } => {
            decl.mutable && output.is_none() && value_type(f, *value)? == element
        }
        InstructionKind::ProjectOwnedArrayField { field, .. } => {
            let storage = carrier(element).map_or(element, |(storage, _)| storage);
            let Type::Package { fields, .. } = storage else {
                return Err(failure(
                    "callable array field projection has no real Package",
                ));
            };
            let ty = &fields
                .get(*field as usize)
                .ok_or_else(|| failure("callable array field is absent"))?
                .1;
            b.type_work(ty, false, "callable array projected complete field Type")?;
            if let Type::Managed(payload) = ty {
                matches!(output,Some(Type::Desc(actual)) if actual==payload)
            } else {
                ty.is_copy_value() && output == Some(ty)
            }
        }
        _ => false,
    };
    if !valid {
        return Err(failure(
            "callable array operation changes its actual mode, arity or complete Type",
        ));
    }
    Ok(())
}
fn position(
    state: &State,
    local: LocalId,
    index: Option<u32>,
    b: &mut Budget,
) -> CompileResult<Target> {
    fee(b, usize::from(index.is_some()).saturating_add(1))?;
    Ok(Target {
        base: local_capture_base(state, local, b)?,
        stored: Vec::new(),
        place: index
            .map(|index| vec![PlaceProjection::Field(index)])
            .unwrap_or_default(),
        retained_port: None,
        heap_element: None,
        heap_parents: Vec::new(),
    })
}
fn vacant(state: &State, place: &Target, b: &mut Budget) -> CompileResult<bool> {
    for hole in &state.vacant {
        fee(
            b,
            target_width(hole)
                .saturating_add(target_width(place))
                .saturating_add(1),
        )?;
        if hole.base == place.base
            && hole.stored == place.stored
            && (hole.place.starts_with(&place.place) || place.place.starts_with(&hole.place))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn set_vacant(state: &mut State, place: Target, empty: bool, b: &mut Budget) -> CompileResult<()> {
    for hole in &state.vacant {
        fee(
            b,
            target_width(hole)
                .saturating_add(target_width(&place))
                .saturating_add(1),
        )?;
    }
    state.vacant.retain(|hole| {
        !(hole.base == place.base
            && hole.stored == place.stored
            && hole.place.starts_with(&place.place))
    });
    if empty {
        ordered(b, state.vacant.len(), target_width(&place))?;
        state.vacant.insert(place);
    }
    Ok(())
}
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Value> {
    validate(module, f, i, b)?;
    let mut out = Value::default();
    if let InstructionKind::MakeOwnedArray { elements } = &i.kind {
        for (index, value) in elements.iter().enumerate() {
            let index = u32::try_from(index)
                .map_err(|_| failure("callable array element ordinal overflows"))?;
            let source = values
                .get(value.0 as usize)
                .ok_or_else(|| failure("callable array operand is absent"))?;
            store_package_field_value(&mut out, index, source, value_type(f, *value)?, b)?;
        }
        return Ok(out);
    }
    let (local, index) = local(i).unwrap();
    let place = position(state, local, index, b)?;
    match &i.kind {
        InstructionKind::BorrowOwnedArrayElement { .. } => {
            out.pointer.targets.insert(place);
            if let Some(marker) = state.receiver_loan
                && matches!(local_capture_base(state, local, b)?, HoldRoot::Capture(_))
            {
                out.pointer.loans.insert(marker);
            }
        }
        InstructionKind::TakeOwnedArrayElement { index, .. } => {
            let holder = state
                .locals
                .get_mut(&local)
                .ok_or_else(|| failure("callable array take has no current shell"))?;
            out = take_package_field_value(holder, *index, b)?;
            set_vacant(state, place, true, b)?;
        }
        InstructionKind::DropOwnedArrayElement { index, .. } => {
            if let Some(holder) = state.locals.get_mut(&local) {
                take_package_field_value(holder, *index, b)?;
            }
            set_vacant(state, place, true, b)?;
        }
        InstructionKind::StoreOwnedArrayElement { index, value, .. } => {
            let source = values
                .get(value.0 as usize)
                .ok_or_else(|| failure("callable array replacement operand is absent"))?;
            let holder = state
                .locals
                .get_mut(&local)
                .ok_or_else(|| failure("callable array replacement has no current shell"))?;
            store_package_field_value(holder, *index, source, value_type(f, *value)?, b)?;
            set_vacant(state, place, false, b)?;
        }
        InstructionKind::ProjectOwnedArrayField { index, field, .. } => {
            let holder = state
                .locals
                .get(&local)
                .ok_or_else(|| failure("callable array projection has no current shell"))?;
            let path = [
                PlaceProjection::Field(*index),
                PlaceProjection::Field(*field),
            ];
            let result = value_type(f, i.result.unwrap())?;
            out = pointee_memory::project(holder, &path, Some(result), b)?;
            if matches!(result, Type::Desc(_)) {
                let mut target = place;
                fee(b, target.place.len().saturating_add(2))?;
                target.place.push(PlaceProjection::Field(*field));
                out.store_description.targets.insert(target);
            }
        }
        InstructionKind::DropOwnedArray { .. } => {
            state.locals.remove(&local);
            clear_local_holes(state, local, b)?;
        }
        _ => {
            return Err(failure(
                "callable array transport lost its actual operation",
            ));
        }
    }
    Ok(out)
}
pub(super) fn access(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &State,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Option<(Permission, bool)>> {
    if !instruction(module, f, i, b)? {
        return Ok(None);
    }
    let Some((local, index)) = local(i) else {
        return Ok(None);
    };
    let target = position(state, local, index, b)?;
    ordered(b, state.locals.len(), 0)?;
    if !state.locals.contains_key(&local) {
        if matches!(
            i.kind,
            InstructionKind::DropOwnedArrayElement { .. } | InstructionKind::DropOwnedArray { .. }
        ) {
            return Ok(None);
        }
        return Err(failure(
            "callable array access has no initialized current shell",
        ));
    }
    let hole = vacant(state, &target, b)?;
    let guarded = matches!(
        i.kind,
        InstructionKind::DropOwnedArrayElement { .. } | InstructionKind::DropOwnedArray { .. }
    );
    if !guarded && !matches!(i.kind, InstructionKind::StoreOwnedArrayElement { .. }) && hole {
        return Err(failure(
            "callable array access crosses a vacant actual element",
        ));
    }
    if guarded && index.is_some() && hole {
        return Ok(None);
    }
    let write = !matches!(
        i.kind,
        InstructionKind::BorrowOwnedArrayElement { mutable: false, .. }
            | InstructionKind::ProjectOwnedArrayField { .. }
    );
    if matches!(i.kind, InstructionKind::BorrowOwnedArrayElement { .. }) {
        let result = i
            .result
            .ok_or_else(|| failure("callable array borrow has no actual result"))?;
        return Ok(Some((
            copy_permission(&values[result.0 as usize].pointer, b)?,
            write,
        )));
    }
    Ok(Some((
        Permission {
            targets: Targets::from([target]),
            loans: Loans::new(),
        },
        write,
    )))
}
