//! Independent sealed prefix admission. No frontend facts confer authority.
use super::*;

#[cfg(test)]
mod tests;

pub(super) const PREFIX: &str = "@builtin:OwnedStore<";

pub(super) fn element(ty: &Type) -> Option<&Type> {
    let (Type::Managed(payload) | Type::Desc(payload)) = ty else {
        return None;
    };
    let Type::OwnedStore { element } = payload.as_ref() else {
        return None;
    };
    Some(element)
}

pub(super) fn is_carrier(ty: &Type) -> bool {
    element(ty).is_some()
}

/// The inner Store of an owning capture deliberately uses the old leaf
/// element profile, not the broader callback Store predicate. This check is
/// also used while qualifying resource-only outer Store metadata, before the
/// ordinary module closure pass has run.
fn check_captured_store(
    module: &TypedModule,
    ty: &Type,
    limits: Option<&Limits>,
    budget: &mut Budget,
) -> Check<()> {
    let Type::Managed(payload) = ty else {
        return Err(unsupported(
            "Store capture requires the actual owning carrier, not a description",
            None,
        ));
    };
    let Type::OwnedStore { element } = payload.as_ref() else {
        return Err(unsupported("expected an owning Store capture", None));
    };
    // The root queue and the complete tree are charged before the recursive
    // Copy predicates or canonical identity allocation below.
    budget.spend(1)?;
    resources::charge_payload(element, budget)?;
    let Type::Package { name, fields } = element.as_ref() else {
        return Err(unsupported(
            "Store capture requires a finite leaf Package element",
            None,
        ));
    };
    if !fields.iter().any(|(_, ty)| matches!(ty, Type::Managed(_)))
        || !fields.iter().all(|(_, ty)| {
            ty.is_copy_value()
                || matches!(ty, Type::Managed(inner) if **inner == Type::ByteBuffer
                    || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value()))
        })
    {
        return Err(unsupported(
            "Store callback captures retain the finite owning environment profile: inner Store elements must be leaf Copy/direct-Managed Packages",
            None,
        ));
    }
    target(&module.target, None)?;
    check_element_with_limits(module, element, None, limits, budget)?;
    budget.spend(PREFIX.len().saturating_add(name.len()).saturating_add(1))?;
    let canonical = definition_name(element);
    budget.spend(
        module
            .resources
            .len()
            .saturating_mul(canonical.len().saturating_add(1)),
    )?;
    let resource = module
        .resources
        .iter()
        .find(|r| r.name == canonical)
        .ok_or_else(|| {
            reject(
                "E-VERIFY-OWNED-STORE",
                None,
                "captured Store lacks its canonical sealed definition",
            )
        })?;
    // A matching resource may occur later in the resource table. Bound it
    // independently before equality when called by that early admission path.
    if let Some(limits) = limits {
        check_type_shape(&resource.payload, &module.target, limits, budget)?;
    }
    budget.spend(1)?;
    resources::charge_payload(&resource.payload, budget)?;
    if resource.teardown.is_some() || resource.payload != **payload {
        return Err(reject(
            "E-VERIFY-OWNED-STORE",
            None,
            "captured Store requires its complete exact payload and no user teardown",
        ));
    }
    Ok(())
}

/// Only the new owning capture shapes enter here. Neither this private
/// qualification nor a fixed-size Store header grants executable authority.
pub(super) fn check_owned_capture(
    module: &TypedModule,
    ty: &Type,
    limits: Option<&Limits>,
    budget: &mut Budget,
) -> Check<()> {
    if is_carrier(ty) {
        return check_captured_store(module, ty, limits, budget);
    }
    let Type::Package { fields, .. } = ty else {
        return Err(unsupported(
            "expected a complete Store-owning Package capture",
            None,
        ));
    };
    budget.spend(1)?;
    resources::charge_payload(ty, budget)?;
    for (_, field) in fields {
        if is_carrier(field) {
            check_captured_store(module, field, limits, budget)?;
        } else if !(field.is_copy_value()
            || matches!(field, Type::Managed(inner) if **inner == Type::ByteBuffer
                || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value())))
        {
            return Err(unsupported(
                "Store-owning Package capture requires direct qualified Store, Managed and finite Copy fields",
                None,
            ));
        }
    }
    packages::check_type_for(module, ty, None, budget)
}

pub(super) fn check_capture_result(
    module: &TypedModule,
    result: &Type,
    limits: Option<&Limits>,
    budget: &mut Budget,
) -> Check<()> {
    if let Some(limits) = limits {
        check_type_shape(result, &module.target, limits, budget)?;
    }
    budget.spend(1)?;
    resources::charge_payload(result, budget)?;
    if !result.is_copy_value() {
        return Err(unsupported(
            "an environment owning a captured Store requires a finite Copy or Unit result",
            None,
        ));
    }
    Ok(())
}

fn target(target: &TargetProfile, span: Option<Span>) -> Check<()> {
    if !target.is_supported_x64_llvm() {
        return Err(unsupported(
            "owned Store requires checked Linux x64 or Win64 LLVM",
            span,
        ));
    }
    Ok(())
}

fn has_storage(ty: &Type) -> bool {
    match ty {
        Type::Unit => false,
        Type::Managed(payload) => has_storage(payload),
        Type::Tuple(fields) => fields.iter().any(has_storage),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields.iter().any(|(_, t)| has_storage(t))
        }
        Type::Array { element, length } => *length > 0 && has_storage(element),
        _ => true,
    }
}

pub(super) fn check_element(
    module: &TypedModule,
    element: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    check_element_with_limits(module, element, span, None, budget)
}

fn check_element_with_limits(
    module: &TypedModule,
    element: &Type,
    span: Option<Span>,
    limits: Option<&Limits>,
    budget: &mut Budget,
) -> Check<()> {
    resources::charge_payload(element, budget)?;
    let Type::Package { fields, .. } = element else {
        return Err(unsupported(
            "Store elements require a complete direct-Managed Package",
            span,
        ));
    };
    let mut callback = None;
    let mut second = None;
    let mut callback_fields = 0usize;
    if !fields.iter().all(|(_, ty)| {
        if let Type::Closure(id) = ty {
            callback_fields += 1;
            match callback {
                None => callback = Some(*id),
                Some(first) if first != *id => second = Some(*id),
                _ => {}
            }
            callback_fields <= 2
        } else {
            ty.is_copy_value()
                || matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer
                || (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()))
        }
    }) || !has_storage(element)
    {
        return Err(unsupported(
            "Store elements require nonzero complete direct-Managed Packages",
            span,
        ));
    }
    // Resource definitions are admitted before the module-wide closure pass.
    // Only the new callback route walks reachable capture metadata here, with
    // the caller's depth limits before any recursive Copy/layout predicate.
    // Ordinary Store follows its original work and admission path unchanged.
    let environments = match (callback, second) {
        (Some(first), Some(second)) => {
            // Definitions precede the module-wide closure shape check. Bound
            // both new result trees before recursive Copy predicates as well.
            for id in [first, second] {
                budget.spend(1)?;
                let definition = closures::definition(module, id, span)?;
                if let Some(limits) = limits {
                    check_type_shape(&definition.result, &module.target, limits, budget)?;
                }
                resources::charge_payload(&definition.result, budget)?;
                if !definition.result.is_copy_value() {
                    return Err(unsupported(
                        "heterogeneous callback Store requires Copy results for both actual environments",
                        span,
                    ));
                }
            }
            Some(closure_layouts_pair(
                module,
                first,
                Some(second),
                limits,
                budget,
            )?)
        }
        (Some(id), None) => Some(closure_layouts(module, id, limits, budget)?),
        (None, None) => None,
        (None, Some(_)) => unreachable!("second callback follows the first field"),
    };
    packages::check_type_for(module, element, span, budget)?;
    // Array lengths are metadata, not expanded slots. Their actual byte
    // product must still fit the target even when the type tree is shallow.
    let (bytes, _) = layout(element, environments.as_deref(), budget)?;
    if bytes == 0 {
        return Err(unsupported(
            "Store element requires a nonzero actual inline layout",
            span,
        ));
    }
    Ok(())
}

/// Independently size the reachable concrete environment DAG. These are not
/// LLVM sizes or producer layout claims. Capture data precedes its live flags;
/// a Unit capture occupies one byte, unlike a Unit aggregate field.
fn closure_layouts(
    module: &TypedModule,
    root: ClosureId,
    limits: Option<&Limits>,
    budget: &mut Budget,
) -> Check<Vec<Option<(u64, u64)>>> {
    closure_layouts_pair(module, root, None, limits, budget)
}

// This memoizes checked type layouts only. Each real field and Store element
// still has its own value, live flags, move and cleanup responsibilities.
fn closure_layouts_pair(
    module: &TypedModule,
    root: ClosureId,
    second: Option<ClosureId>,
    limits: Option<&Limits>,
    budget: &mut Budget,
) -> Check<Vec<Option<(u64, u64)>>> {
    if limits.is_some_and(|limits| module.closures.len() > limits.max_functions) {
        return Err(limit("Store callback count exceeds the selected bound"));
    }
    budget.spend(module.closures.len().saturating_mul(3).saturating_add(1))?;
    if second.is_some() {
        budget.spend(2)?; // second root and the bounded two-entry queue copy
    }
    let mut state = vec![0u8; module.closures.len()];
    let mut heights = vec![0usize; module.closures.len()];
    let mut layouts = vec![None; module.closures.len()];
    // No extra table or traversal on the historical no-Store-capture path.
    let mut store_profiles: Option<Vec<bool>> = None;
    let mut pending = vec![(root, false)];
    if let Some(other) = second {
        pending.insert(0, (other, false));
    }
    while let Some((id, exit)) = pending.pop() {
        budget.spend(1)?;
        let definition = closures::definition(module, id, None)?;
        let index = id.0 as usize;
        if definition.id != id {
            return Err(reject(
                "E-VERIFY-CLOSURE",
                None,
                "Store callback identity differs from its actual definition",
            ));
        }
        if exit {
            budget.spend(definition.captures.len())?;
            let (mut bytes, mut maximum, mut flags, mut height) = (0u64, 1u64, 0u64, 0usize);
            let mut owns_store = false;
            for capture in &definition.captures {
                let (size, align) = if *capture == Type::Unit {
                    budget.spend(1)?;
                    (1, 1)
                } else {
                    layout(capture, Some(&layouts), budget)?
                };
                bytes = bounded(align_to(bytes, align)?.checked_add(size))?;
                maximum = maximum.max(align);
                let added = match capture {
                    Type::Managed(payload) => {
                        owns_store |= matches!(payload.as_ref(), Type::OwnedStore { .. });
                        1
                    }
                    Type::Closure(child) => {
                        owns_store |= store_profiles.as_ref().is_some_and(|p| p[child.0 as usize]);
                        1
                    }
                    Type::Package { fields, .. } => {
                        budget.spend(fields.len())?;
                        1 + fields.iter().fold(0u64, |count, (_, field)| {
                            owns_store |= is_carrier(field);
                            count + u64::from(matches!(field, Type::Managed(_)))
                        })
                    }
                    _ => 0,
                };
                flags = bounded(flags.checked_add(added))?;
                if let Type::Closure(child) = capture {
                    height = height.max(heights[child.0 as usize].saturating_add(1));
                }
            }
            if limits.is_some_and(|limits| height > limits.max_call_depth) {
                return Err(limit("Store callback layout depth exceeds selected bound"));
            }
            if owns_store {
                check_capture_result(module, &definition.result, limits, budget)?;
                if store_profiles.is_none() {
                    budget.spend(module.closures.len())?;
                    store_profiles = Some(vec![false; module.closures.len()]);
                }
                store_profiles.as_mut().expect("allocated Store profiles")[index] = true;
            }
            bytes = bounded(bytes.checked_add(flags))?;
            layouts[index] = Some((align_to(bytes, maximum)?, maximum));
            heights[index] = height;
            state[index] = 2;
            continue;
        }
        match state[index] {
            2 => continue,
            1 => {
                return Err(reject(
                    "E-VERIFY-CLOSURE-LAYOUT",
                    None,
                    "recursive inline Store callback environment layout",
                ));
            }
            _ => {}
        }
        if limits.is_some_and(|limits| definition.captures.len() > limits.max_locals) {
            return Err(limit("Store callback capture count exceeds selected bound"));
        }
        budget.spend(definition.captures.len().saturating_add(1))?;
        state[index] = 1;
        pending.push((id, true));
        for capture in definition.captures.iter().rev() {
            if let Some(limits) = limits {
                check_type_shape(capture, &module.target, limits, budget)?;
            }
            if is_carrier(capture) || packages::has_store_fields(capture) {
                check_owned_capture(module, capture, limits, budget)?;
            }
            if packages::has_array_fields(capture) || packages::has_callable_fields(capture) {
                return Err(unsupported(
                    "Store callback captures retain the existing finite owning environment profile",
                    None,
                ));
            }
            match capture {
                Type::Closure(child) => pending.push((*child, false)),
                Type::Managed(_) | Type::Package { .. } => {}
                ty if ty.is_copy_value() => {}
                _ => {
                    return Err(unsupported(
                        "Store callback requires a self-contained environment",
                        None,
                    ));
                }
            }
        }
    }
    Ok(layouts)
}

fn bounded(value: Option<u64>) -> Check<u64> {
    value
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(|| limit("owned Store element layout exceeds target address range"))
}

fn align_to(size: u64, align: u64) -> Check<u64> {
    bounded(size.checked_add(align - 1)).map(|n| n / align * align)
}

fn fields_layout<'a>(
    fields: impl IntoIterator<Item = &'a Type>,
    environments: Option<&[Option<(u64, u64)>]>,
    budget: &mut Budget,
) -> Check<(u64, u64)> {
    let (mut bytes, mut maximum) = (0u64, 1u64);
    for field in fields {
        let (size, alignment) = layout(field, environments, budget)?;
        bytes = bounded(align_to(bytes, alignment)?.checked_add(size))?;
        maximum = maximum.max(alignment);
    }
    Ok((align_to(bytes, maximum)?, maximum))
}

/// Independent x64 layout admission. LLVM emission computes its own actual
/// stride; this walk prevents zero/overflow types from reaching that code.
fn layout(
    ty: &Type,
    environments: Option<&[Option<(u64, u64)>]>,
    budget: &mut Budget,
) -> Check<(u64, u64)> {
    budget.spend(1)?;
    Ok(match ty {
        Type::Unit => (0, 1),
        Type::Bool | Type::U8 => (1, 1),
        Type::U16 => (2, 2),
        Type::I32 => (4, 4),
        Type::I64
        | Type::ForeignHandle
        | Type::CWideString
        | Type::FunctionPointer { .. }
        | Type::ForeignCallback { .. } => (8, 8),
        // All capture Store elements and canonical definitions are checked
        // before layout; a fixed header alone is never qualification.
        Type::ByteBuffer | Type::OwnedStore { .. } => (24, 8),
        Type::Managed(payload) => layout(payload, environments, budget)?,
        Type::Closure(id) => environments
            .and_then(|layouts| layouts.get(id.0 as usize))
            .copied()
            .flatten()
            .ok_or_else(|| {
                reject(
                    "E-VERIFY-CLOSURE-LAYOUT",
                    None,
                    "Store callback layout lacks a checked concrete environment",
                )
            })?,
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields_layout(fields.iter().map(|(_, ty)| ty), environments, budget)?
        }
        Type::Tuple(fields) => fields_layout(fields, environments, budget)?,
        Type::Array { element, length } => {
            let (size, alignment) = layout(element, environments, budget)?;
            (bounded(size.checked_mul(u64::from(*length)))?, alignment)
        }
        Type::ErrorUnion { error, result } => fields_layout(
            [&Type::Bool, error.as_ref(), result.as_ref()],
            environments,
            budget,
        )?,
        Type::Enum { variants, .. } => {
            let (mut bytes, mut alignment) = (0u64, 1u64);
            for (_, fields) in variants {
                let (size, align) = fields_layout(fields, environments, budget)?;
                bytes = bytes.max(size);
                alignment = alignment.max(align);
            }
            let size = bounded(align_to(4, alignment)?.checked_add(align_to(bytes, alignment)?))?;
            (align_to(size, alignment.max(4))?, alignment.max(4))
        }
        _ => {
            return Err(unsupported(
                "Store element has no admitted inline layout",
                None,
            ));
        }
    })
}

pub(super) fn definition_name(element: &Type) -> String {
    let Type::Package { name, .. } = element else {
        unreachable!("checked Store element")
    };
    format!("{PREFIX}{name}>")
}

pub(super) fn check_definition(
    module: &TypedModule,
    definition: &ResourceDefinition,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<bool> {
    if !matches!(definition.payload, Type::OwnedStore { .. })
        && !definition.name.starts_with(PREFIX)
    {
        return Ok(false);
    }
    target(&module.target, None)?;
    let Type::OwnedStore { element } = &definition.payload else {
        return Err(reject(
            "E-VERIFY-OWNED-STORE",
            None,
            "Store resource identity requires its exact sealed payload",
        ));
    };
    check_element_with_limits(module, element, None, Some(limits), budget)?;
    if definition.name != definition_name(element) || definition.teardown.is_some() {
        return Err(reject(
            "E-VERIFY-OWNED-STORE",
            None,
            "Store definition must have its canonical element identity and no user teardown",
        ));
    }
    Ok(true)
}

pub(super) fn check_carrier(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    target(&module.target, span)?;
    let Some(element) = element(ty) else {
        return Err(reject(
            "E-VERIFY-OWNED-STORE",
            span,
            "expected a Store carrier",
        ));
    };
    check_element(module, element, span, budget)?;
    let name = definition_name(element);
    budget.spend(
        module
            .resources
            .len()
            .saturating_mul(name.len().saturating_add(1)),
    )?;
    if !module.resources.iter().any(|r| r.name == name && r.teardown.is_none()
        && matches!(&r.payload, Type::OwnedStore { element: actual } if actual.as_ref() == element)) {
        return Err(reject("E-VERIFY-OWNED-STORE", span, "Store carrier lacks its canonical sealed definition"));
    }
    Ok(())
}

pub(super) fn check_instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    target(&module.target, span)?;
    let bad = |message| reject("E-VERIFY-OWNED-STORE", span, message);
    if let InstructionKind::OwnedStoreNew { element } = &instruction.kind {
        check_element(module, element, span, budget)?;
        let ty = Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(element.clone()),
        }));
        check_carrier(module, &ty, span, budget)?;
        return Ok(Some(ty));
    }
    let (store, extra) = match instruction.kind {
        InstructionKind::OwnedStoreReserve { store, capacity } => (store, Some(capacity)),
        InstructionKind::OwnedStoreBorrow { store, index }
        | InstructionKind::OwnedStoreBorrowMut { store, index } => (store, Some(index)),
        InstructionKind::OwnedStorePush { store, value } => (store, Some(value)),
        InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store } => (store, None),
        _ => unreachable!("Store instruction dispatch"),
    };
    let carrier = value_type(function, store, span)?;
    if !matches!(carrier, Type::Desc(_)) {
        return Err(bad("Store operation requires a real Store description"));
    }
    check_carrier(module, carrier, span, budget)?;
    let element = element(carrier).expect("checked Store carrier");
    let result = match instruction.kind {
        InstructionKind::OwnedStoreReserve { .. } => {
            if value_type(function, extra.expect("reserve capacity"), span)? != &Type::I64 {
                return Err(bad("Store capacity must be i64"));
            }
            Type::Bool
        }
        InstructionKind::OwnedStoreBorrow { .. } | InstructionKind::OwnedStoreBorrowMut { .. } => {
            if value_type(function, extra.expect("element index"), span)? != &Type::I64 {
                return Err(bad("Store element index must be i64"));
            }
            Type::Address {
                mutable: matches!(
                    instruction.kind,
                    InstructionKind::OwnedStoreBorrowMut { .. }
                ),
                inner: Box::new(element.clone()),
            }
        }
        InstructionKind::OwnedStorePush { .. } => {
            if value_type(function, extra.expect("push element"), span)? != element {
                return Err(bad(
                    "Store push requires exactly its complete nominal Package element",
                ));
            }
            Type::ErrorUnion {
                error: Box::new(element.clone()),
                result: Box::new(Type::Unit),
            }
        }
        InstructionKind::OwnedStorePop { .. } => Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(element.clone()),
        },
        InstructionKind::OwnedStoreLen { .. } | InstructionKind::OwnedStoreCapacity { .. } => {
            Type::I64
        }
        _ => unreachable!("checked Store operation"),
    };
    Ok(Some(result))
}

pub(super) fn access(kind: &InstructionKind) -> Option<(ValueId, bool)> {
    match *kind {
        InstructionKind::OwnedStoreReserve { store, .. }
        | InstructionKind::OwnedStorePush { store, .. }
        | InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreBorrowMut { store, .. } => Some((store, true)),
        InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store }
        | InstructionKind::OwnedStoreBorrow { store, .. } => Some((store, false)),
        _ => None,
    }
}
