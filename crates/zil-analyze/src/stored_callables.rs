//! Qualified callbacks with inline scalar environments. Metadata is checked
//! against the actual definition and receiver; roles use parameter positions.
use super::*;
mod foreign;
mod staged;
pub(super) use foreign::regions as foreign_regions;

pub(super) fn scalar(t: &Type) -> bool {
    matches!(
        t,
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
    )
}
pub(super) fn scalar_foreign(f: &TypedFunction, b: &mut Budget) -> CompileResult<bool> {
    let Some(declaration) = &f.foreign else {
        return Ok(false);
    };
    b.spend(
        f.parameters
            .len()
            .saturating_add(declaration.abi.len())
            .saturating_add(declaration.symbol.len())
            .saturating_add(1),
        "stored scalar foreign actual declaration",
    )?;
    let c_number = |ty: &Type| {
        matches!(
            ty,
            Type::U8
                | Type::U16
                | Type::U32
                | Type::I32
                | Type::I8
                | Type::I16
                | Type::Isize
                | Type::I64
                | Type::F32
                | Type::F64
        )
    };
    let bytes = declaration.symbol.as_bytes();
    Ok(declaration.abi == "C"
        && declaration.symbol != "main"
        && bytes
            .first()
            .is_some_and(|v| v.is_ascii_alphabetic() || *v == b'_')
        && bytes
            .iter()
            .all(|v| v.is_ascii_alphanumeric() || *v == b'_')
        && declaration.noescape_parameters.is_empty()
        && !declaration.static_callbacks
        && declaration.scoped_callback.is_none()
        && f.environment.is_none()
        && f.call_kind == CallKind::Direct
        && f.storage_life.is_none()
        && f.callable_contract.is_none()
        && f.callable_return_contract.is_none()
        && f.reference_return_bound.is_none()
        && f.result_mode == zil_ir::PassingMode::Infer
        && (f.return_type == Type::Unit || c_number(&f.return_type))
        && f.blocks.is_empty()
        && f.values.is_empty()
        && f.entry == zil_ir::BlockId(0)
        && f.parameters.len() == f.locals.len()
        && f.parameters
            .iter()
            .zip(&f.locals)
            .enumerate()
            .all(|(index, (id, local))| {
                id.0 as usize == index && local.id == *id && !local.mutable && c_number(&local.ty)
            }))
}

pub(super) fn lending(d: &ClosureDefinition) -> bool {
    d.storage_life.is_some()
        && !d.lending_captures.is_empty()
        && d.kind != CallableKind::FnOnce
        && matches!(&d.result, Type::Address { mutable:false, inner } if scalar(inner))
}
pub(super) fn life_result(d: &ClosureDefinition) -> &Type {
    if lending(d) && !d.storage_life.as_ref().is_some_and(|life|life.bindings.iter().any(|row|row.position.base==StorageBase::Receiver)) { &Type::Unit } else { &d.result }
}

// A scheduling tag, never qualification. definitions() rechecks the complete
// actual inputs, result, captures and entry before this ledger uses the value.
fn borrowed_owned_candidate(d: &ClosureDefinition) -> bool {
    d.storage_life.is_none()
        && d.lending_captures.is_empty()
        && d.kind != CallableKind::FunctionPointer
        && matches!(
            d.result,
            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
        )
}

fn closed_callback_data(ty: &Type, b: &mut Budget) -> CompileResult<()> {
    b.type_work(ty, false, "stored callback closed target-free data")?;
    fn closed(ty: &Type) -> bool {
        match ty {
            Type::Address { .. }
            | Type::Desc(_)
            | Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Opaque { .. }
            | Type::TaskI32
            | Type::Task { .. }
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. } => false,
            Type::Managed(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner } => closed(inner),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fields.iter().all(|(_, child)| closed(child))
            }
            Type::Tuple(fields) => fields.iter().all(closed),
            Type::Enum { variants, .. } => {
                variants.iter().all(|(_, fields)| fields.iter().all(closed))
            }
            Type::ErrorUnion { error, result } => closed(error) && closed(result),
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                parameters.iter().all(closed) && closed(result)
            }
            _ => true,
        }
    }
    if !closed(ty) {
        return Err(bad(
            "stored owning callback data contains a target-bearing or callable environment dependency",
        ));
    }
    Ok(())
}

fn borrowed_owned_definition(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<()> {
    b.type_work(&d.result, true, "stored callback actual owned result")?;
    b.spend(
        d.parameters.len(),
        "stored callback actual borrowed input inventory",
    )?;
    if d.result.is_copy_value()
        || !d
            .parameters
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. }))
    {
        return Err(bad(
            "stored owning callback requires an actual borrowed input and closed owned result",
        ));
    }
    closed_callback_data(&d.result, b)?;
    super::owned_callbacks::data(module, &d.result, b)?;
    for ty in &d.parameters {
        let pointee = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        closed_callback_data(pointee, b)?;
        super::owned_callbacks::input(module, ty, b)?;
    }
    if module
        .functions
        .get(d.function.0 as usize)
        .is_some_and(|entry| entry.reference_return_bound.is_some())
    {
        return Err(bad(
            "stored owning callback cannot claim a borrowed result bound",
        ));
    }
    super::owned_callbacks::signature(module, d, b)
}

pub(super) fn finite(module: &TypedModule, id: zil_ir::ClosureId) -> bool {
    module.closures.get(id.0 as usize).is_some_and(|d| {
        (d.lending_captures.is_empty() && scalar(&d.result))
            || lending(d)
            || borrowed_owned_candidate(d)
    })
}
pub(super) fn fixed_value(module: &TypedModule, t: &Type) -> bool {
    let t = if let Type::Address { inner, .. } = t {
        inner.as_ref()
    } else {
        t
    };
    matches!(t, Type::Closure(id) if module.closures.get(id.0 as usize).is_some_and(|d| d.storage_life.is_some()))
}
pub(super) fn value(module: &TypedModule, t: &Type) -> bool {
    let t = if let Type::Address { inner, .. } = t {
        inner.as_ref()
    } else {
        t
    };
    matches!(t,Type::Closure(id)if finite(module,*id))
}
pub(super) fn capture(t: &Type) -> bool {
    scalar(t)
        || matches!(t, Type::Address { mutable: false, inner }
        if scalar(inner) || matches!(inner.as_ref(), Type::BorrowedStorage { storage, life }
            if matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_)) && life.retained.is_empty()))
}
/// Reborrow the head on every read, preserving its nominal pointee classes.
pub(super) fn mutable_context_capture(t: &Type) -> bool {
    matches!(t, Type::Address { mutable: true, inner }
        if matches!(inner.as_ref(), Type::BorrowedStorage { storage, life }
            if matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                && life.retained.is_empty()))
}
pub(super) fn exclusive_capture(t: &Type) -> bool {
    mutable_context_capture(t)
        || matches!(t, Type::Address { mutable: true, inner } if scalar(inner))
}
pub(super) fn resource_payload(t: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::Record { fields, .. } = t else {
        return Ok(false);
    };
    b.spend(
        fields.len().saturating_add(1),
        "class-free resource payload eligibility",
    )?;
    Ok(fields.iter().all(|(_, t)| scalar(t)))
}
pub(super) fn owned_capture(t: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::Managed(payload) = t else {
        return Ok(false);
    };
    resource_payload(payload, b)
}
pub(super) fn definitions(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    b.spend(module.closures.len(), "qualified callback inventory")?;
    for d in &module.closures {
        let borrowed_owned = borrowed_owned_candidate(d);
        if !borrowed_owned
            && !b.composition_stored.is_empty()
            && !b
                .composition_stored
                .get(d.function.0 as usize)
                .copied()
                .unwrap_or(false)
        {
            continue;
        }
        b.spend(d.captures.len(), "qualified scalar capture inventory")?;
        let mut admitted = finite(module, d.id);
        for t in &d.captures {
            if matches!(t, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))
                && b.returned_borrowed_environments.contains(&d.id)
            {
                super::resources::product::admit_owned(module, t, b)?;
                continue;
            }
            admitted &= if d.storage_life.is_some() {
                capture(t)
                    || exclusive_capture(t)
                    || owned_capture(t, b)?
                    || matches!(t,Type::Closure(child) if b.returned_borrowed_environments.contains(child) && finite(module,*child))
            } else {
                scalar(t)
                    || capture(t)
                    || exclusive_capture(t)
                    || b.returned_borrowed_environments.contains(&d.id)
                        && (owned_capture(t, b)?
                            || matches!(t, Type::Closure(child)
                                if b.returned_borrowed_environments.contains(child)
                                    && finite(module, *child)))
            };
        }
        if !admitted {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "qualified callbacks require finite scalar/shared capture environments and scalar results",
                None,
            )]);
        }
        if borrowed_owned {
            borrowed_owned_definition(module, d, b)?;
        }
        super::borrowed_fp::signature_work(&d.parameters, &d.result, b)?;
        let f = module
            .functions
            .get(d.function.0 as usize)
            .ok_or_else(|| bad("qualified callback entry is absent"))?;
        b.spend(
            f.parameters.len(),
            "qualified callback actual parameter correspondence",
        )?;
        if f.environment != Some(d.id)
            || f.foreign.is_some()
            || f.call_kind != CallKind::Direct
            || f.parameters.len() != d.parameters.len()
            || f.return_type != d.result
        {
            return Err(bad("qualified callback complete actual entry differs"));
        }
        if let Some(life) = &d.storage_life {
            b.spend(d.parameters.len(), "captured Fixed shared argument profile")?;
            if d.parameters
                .iter()
                .any(|t| matches!(t, Type::Address { mutable: true, .. }))
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "captured Fixed exclusive arguments need capture target summary substitution",
                    None,
                )]);
            }
            super::signature_life::charge(life, b)?;
            if f.storage_life.as_ref() != Some(life) {
                return Err(bad(
                    "actual captured closure entry relation differs from definition",
                ));
            }
            super::borrowed_fp::qualified_array_relation_work(&d.parameters, &d.result, b)?;
            super::borrowed_fp::qualified_array_relation_work(&d.captures, &Type::Unit, b)?;
            super::signature_life::closure_relation(
            &module.closures,
            d.kind,
