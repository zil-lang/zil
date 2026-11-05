//! Narrow owner carriers alongside capture-grounded Fixed. These checks only
//! classify and type actual operations. The independent resource ledger still
//! proves every owner/description epoch, transfer, Once use and cleanup.
use super::*;

pub(super) fn payload(ty: &Type) -> bool {
    matches!(ty, Type::Record { fields, .. } if fields.iter().all(|(_, ty)| stored_callables::scalar(ty)))
}
pub(super) fn owner(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) if payload(inner))
}
pub(super) fn description(ty: &Type) -> bool {
    matches!(ty, Type::Desc(inner) if payload(inner))
}
fn bad(span: Span, message: &str) -> Diagnostic {
    reject("E-VERIFY-STORED-OWNER", Some(span), message)
}
fn shape(ty: &Type, span: Span, b: &mut Budget) -> Check<()> {
    stored_flow::type_work(ty, &b.limits.clone(), b)?;
    if owner(ty) || description(ty) {
        Ok(())
    } else {
        Err(unsupported(
            "Fixed/owner integration requires a class-free scalar Record carrier",
            Some(span),
        ))
    }
}
fn actual(f: &TypedFunction, value: ValueId, span: Span) -> Check<&Type> {
    f.values
        .get(value.0 as usize)
        .ok_or_else(|| bad(span, "owner operand has no actual type"))
}
fn local(f: &TypedFunction, id: LocalId, span: Span) -> Check<&Type> {
    f.locals
        .get(id.0 as usize)
        .map(|l| &l.ty)
        .ok_or_else(|| bad(span, "owner local is absent"))
}
fn result<'a>(f: &'a TypedFunction, i: &Instruction) -> Check<&'a Type> {
    actual(
        f,
        i.result
            .ok_or_else(|| bad(i.span, "owner operation has no result"))?,
        i.span,
    )
}
fn same_result(f: &TypedFunction, i: &Instruction, expected: &Type, b: &mut Budget) -> Check<()> {
    let ty = result(f, i)?;
    stored_flow::type_work(expected, &b.limits.clone(), b)?;
    stored_flow::type_work(ty, &b.limits.clone(), b)?;
    if ty == expected {
        Ok(())
    } else {
        Err(bad(
            i.span,
            "owner operation result differs from its actual source",
        ))
    }
}
fn capture<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    index: u32,
    span: Span,
) -> Check<&'a Type> {
    let d = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .ok_or_else(|| bad(span, "owner operation has no actual environment"))?;
    if d.kind != CallableKind::FnOnce || d.storage_life.is_none() {
        return Err(unsupported(
            "owning Fixed capture operation requires a genuine FnOnce definition",
            Some(span),
        ));
    }
    d.captures
        .get(index as usize)
        .ok_or_else(|| bad(span, "owner capture index absent"))
}
pub(super) fn instruction(f: &TypedFunction, i: &InstructionKind) -> bool {
    if let InstructionKind::TakeLocal { local } = i {
        return f
            .locals
            .get(local.0 as usize)
            .is_some_and(|local| matches!(local.ty, Type::Managed(_)));
    }
    matches!(
        i,
        InstructionKind::MakeManaged { .. }
            | InstructionKind::TakeCapture { .. }
            | InstructionKind::DescribeLocal { .. }
            | InstructionKind::DescribeCapture { .. }
            | InstructionKind::ReadDesc { .. }
            | InstructionKind::DropManaged { .. }
    )
}
pub(super) fn types(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<()> {
    match &i.kind {
        InstructionKind::MakeManaged { fields } => {
            let out = result(f, i)?;
            shape(out, i.span, b)?;
            let Type::Managed(inner) = out else {
                return Err(bad(i.span, "MakeManaged must produce an actual owner"));
            };
            let Type::Record {
                fields: expected, ..
            } = inner.as_ref()
            else {
                unreachable!()
            };
            b.spend(fields.len().saturating_add(expected.len()))?;
            if fields.len() != expected.len() {
                return Err(bad(i.span, "MakeManaged omits an actual scalar field"));
            }
            for (value, (_, ty)) in fields.iter().zip(expected) {
                if actual(f, *value, i.span)? != ty {
                    return Err(bad(i.span, "MakeManaged scalar operand type differs"));
                }
            }
        }
        InstructionKind::TakeLocal { local: id } => {
            let ty = local(f, *id, i.span)?;
            shape(ty, i.span, b)?;
            if !owner(ty) {
                return Err(bad(i.span, "TakeLocal requires an actual finite owner"));
            }
            same_result(f, i, ty, b)?;
        }
        InstructionKind::TakeCapture { index } => {
            let ty = capture(module, f, *index, i.span)?;
            shape(ty, i.span, b)?;
            if !owner(ty) {
                return Err(bad(i.span, "TakeCapture requires an actual owned capture"));
            }
            same_result(f, i, ty, b)?;
        }
        InstructionKind::DescribeLocal { .. } | InstructionKind::DescribeCapture { .. } => {
            // The two operands are distinct domains; resolve each actual source.
            let ty = match &i.kind {
                InstructionKind::DescribeLocal { local: id } => local(f, *id, i.span)?,
                InstructionKind::DescribeCapture { index } => capture(module, f, *index, i.span)?,
                _ => unreachable!(),
            };
            shape(ty, i.span, b)?;
            let Type::Managed(inner) = ty else {
                return Err(bad(i.span, "description source is not an actual owner"));
            };
            let out = result(f, i)?;
            shape(out, i.span, b)?;
            let Type::Desc(actual) = out else {
                return Err(bad(i.span, "description must produce Desc"));
            };
            stored_flow::type_work(ty, &b.limits.clone(), b)?;
            stored_flow::type_work(out, &b.limits.clone(), b)?;
            if inner != actual {
                return Err(bad(
                    i.span,
                    "description payload differs from actual source",
                ));
            }
        }
        InstructionKind::ReadDesc { desc, index } => {
            let ty = actual(f, *desc, i.span)?;
            shape(ty, i.span, b)?;
            let Type::Desc(inner) = ty else {
                return Err(bad(i.span, "field read requires actual Desc"));
            };
            let Type::Record { fields, .. } = inner.as_ref() else {
                unreachable!()
            };
            let (_, field) = fields
                .get(*index as usize)
                .ok_or_else(|| bad(i.span, "description field index absent"))?;
            same_result(f, i, field, b)?;
        }
        InstructionKind::DropManaged { local: id } => {
            let ty = local(f, *id, i.span)?;
            shape(ty, i.span, b)?;
            if !owner(ty) || i.result.is_some() {
                return Err(bad(
                    i.span,
                    "DropManaged requires actual owner and no result",
                ));
            }
        }
        _ => return Err(bad(i.span, "unknown finite owner operation")),
    }
    Ok(())
}
/// A module cannot carry arbitrary resource profiles just because one closure
/// has an allowed owner. Each declaration must be the exact scalar payload of
/// an actual owning capture in a qualified genuine FnOnce definition.
pub(super) fn module(module: &TypedModule, b: &mut Budget) -> Check<()> {
    if module.resources.is_empty() {
        return Ok(());
    }
    b.spend(module.closures.len())?;
    let mut payloads = Vec::new();
    for d in &module.closures {
        if d.kind == CallableKind::FnOnce && d.storage_life.is_some() {
            b.spend(d.captures.len())?;
            for ty in &d.captures {
                stored_flow::type_work(ty, &b.limits.clone(), b)?;
                if let Type::Managed(inner) = ty
                    && payload(inner)
                {
                    payloads.push(inner.as_ref());
                }
            }
        }
    }
    b.spend(module.resources.len())?;
    for resource in &module.resources {
        stored_flow::type_work(&resource.payload, &b.limits.clone(), b)?;
        if !payload(&resource.payload) {
            return Err(unsupported(
                "unrelated resource shape is outside owning Fixed integration",
                None,
            ));
        }
        b.spend(payloads.len())?;
        let mut found = false;
        for actual in &payloads {
            stored_flow::type_work(actual, &b.limits.clone(), b)?;
            stored_flow::type_work(&resource.payload, &b.limits.clone(), b)?;
            if *actual == &resource.payload {
                found = true;
            }
        }
        if !found {
            return Err(unsupported(
                "resource declaration is not an actual finite owning Fixed capture",
                None,
            ));
        }
    }
    Ok(())
}

/// New modules do not acquire arbitrary nested owners or owning-head addresses
/// from the old Copy-value fallback. Concrete environment capture types are
/// checked separately; only top-level finite owner/Desc carriers are added.
pub(super) fn value_type(ty: &Type, b: &mut Budget) -> Check<()> {
    if matches!(ty, Type::Managed(_) | Type::Desc(_)) {
        shape(ty, Span::default(), b)?;
        return Ok(());
    }
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            Type::Managed(_)
            | Type::Desc(_)
            | Type::Package { .. }
            | Type::OwnedStore { .. }
            | Type::Task { .. }
            | Type::TaskI32
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. }
            | Type::ByteBuffer => {
                return Err(unsupported(
                    "nested owner or owner-head address is outside finite Fixed integration",
                    None,
                ));
            }
            Type::Address { inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::BorrowedStorage { storage: inner, .. } => {
                b.spend(1)?;
                pending.push(inner);
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                b.spend(parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(())
}
