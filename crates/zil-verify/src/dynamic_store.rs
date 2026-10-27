//! Independent sealed prefix admission. No frontend facts confer authority.
use super::*;

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

fn target(target: &TargetProfile, span: Option<Span>) -> Check<()> {
    if !target.is_supported_x64_llvm() || target.triple != "x86_64-unknown-linux-gnu" {
        return Err(unsupported(
            "owned Store currently requires checked Linux x64 LLVM",
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
    resources::charge_payload(element, budget)?;
    let Type::Package { fields, .. } = element else {
        return Err(unsupported(
            "Store elements require a complete direct-Managed Package",
            span,
        ));
    };
    if !fields.iter().all(|(_, ty)| {
        ty.is_copy_value()
            || matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer
            || (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()))
    }) || !has_storage(element)
    {
        return Err(unsupported(
            "Store elements require nonzero complete direct-Managed Packages",
            span,
        ));
    }
    packages::check_type_for(module, element, span, budget)?;
    // Array lengths are metadata, not expanded slots. Their actual byte
    // product must still fit the target even when the type tree is shallow.
    layout(element, budget)?;
    Ok(())
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
    budget: &mut Budget,
) -> Check<(u64, u64)> {
    let (mut bytes, mut maximum) = (0u64, 1u64);
    for field in fields {
        let (size, alignment) = layout(field, budget)?;
        bytes = bounded(align_to(bytes, alignment)?.checked_add(size))?;
        maximum = maximum.max(alignment);
    }
    Ok((align_to(bytes, maximum)?, maximum))
}

/// Independent x64 layout admission. LLVM emission computes its own actual
/// stride; this walk prevents zero/overflow types from reaching that code.
fn layout(ty: &Type, budget: &mut Budget) -> Check<(u64, u64)> {
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
        Type::ByteBuffer => (24, 8),
        Type::Managed(payload) => layout(payload, budget)?,
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields_layout(fields.iter().map(|(_, ty)| ty), budget)?
        }
        Type::Tuple(fields) => fields_layout(fields, budget)?,
        Type::Array { element, length } => {
            let (size, alignment) = layout(element, budget)?;
            (bounded(size.checked_mul(u64::from(*length)))?, alignment)
        }
        Type::ErrorUnion { error, result } => {
            fields_layout([&Type::Bool, error.as_ref(), result.as_ref()], budget)?
        }
        Type::Enum { variants, .. } => {
            let (mut bytes, mut alignment) = (0u64, 1u64);
            for (_, fields) in variants {
                let (size, align) = fields_layout(fields, budget)?;
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
    check_element(module, element, None, budget)?;
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
        | InstructionKind::OwnedStorePop { store } => Some((store, true)),
        InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store } => Some((store, false)),
        _ => None,
    }
}
