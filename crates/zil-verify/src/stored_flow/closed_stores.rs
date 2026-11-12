//! Closed Store instructions still use the independent sealed resource and
//! actual descriptor/loan kernels; this is structural admission only.
use super::*;

pub(super) fn operation(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.ordinary_closed_stores {
        return Ok(false);
    }
    let element = if let InstructionKind::OwnedStoreNew { element } = &i.kind {
        element
    } else if let Some((value, _)) = crate::dynamic_store::access(&i.kind) {
        let Some(element) = crate::dynamic_store::element(vt(f, value)?) else {
            return Err(err("ordinary Store operation has no actual Store carrier"));
        };
        element
    } else {
        return Ok(false);
    };
    if !pure_copy::ordinary_owned_fields(vec![(element, 0)], b)? {
        return Ok(false);
    }
    crate::dynamic_store::check_instruction(module, f, i, b)?;
    Ok(true)
}

// Authenticate the actual owning local, not a scalar Record surrogate.
pub(super) fn carrier_operation(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.ordinary_closed_stores {
        return Ok(false);
    }
    let local = match i.kind {
        InstructionKind::TakeLocal { local }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DescribeLocal { local } => local,
        _ => return Ok(false),
    };
    let ty = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| err("Store owner local is absent"))?
        .ty;
    if !matches!(ty, Type::Managed(_)) {
        return Ok(false);
    }
    let Some(element) = crate::dynamic_store::element(ty) else {
        return Ok(false);
    };
    if !pure_copy::ordinary_owned_fields(vec![(element, 0)], b)? {
        return Ok(false);
    }
    crate::dynamic_store::check_carrier(module, ty, Some(i.span), b)?;
    Ok(true)
}

pub(super) fn value(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if !b.ordinary_closed_stores {
        return Ok(false);
    }
    let root = match ty {
        Type::Address { inner, .. } | Type::Desc(inner) => inner.as_ref(),
        _ => ty,
    };
    if !pure_copy::ordinary_owned_fields(vec![(root, 0)], b)? {
        return Ok(false);
    }
    // Descriptions must actually describe one authenticated Store owner.
    if matches!(ty, Type::Desc(_)) {
        if crate::dynamic_store::element(ty).is_none() {
            return Ok(false);
        }
        crate::dynamic_store::check_carrier(module, ty, None, b)?;
    }
    Ok(true)
}
