//! Bounded packages. Raw declarations and operands are rechecked here;
//! producer type/ownership claims are never a proof. Responsibility and field
//! description provenance are independently reconstructed in `resources`.
use super::*;

/// Reconstructed from executable entry instructions, never a producer claim.
/// Empty vectors preserve the ordinary function path without a view table.
#[derive(Default)]
pub(super) struct CaptureViews {
    pub(super) by_local: Vec<Option<u32>>,
    pub(super) by_capture: Vec<Option<LocalId>>,
}

impl CaptureViews {
    pub(super) fn capture(&self, local: LocalId) -> Option<u32> {
        self.by_local.get(local.0 as usize).copied().flatten()
    }
}

pub(super) fn capture_views(
    module: &TypedModule,
    function: &TypedFunction,
    predecessors: &[Vec<usize>],
    budget: &mut Budget,
) -> Check<CaptureViews> {
    let Some(environment) = function.environment else {
        return Ok(CaptureViews::default());
    };
    let captures = &closures::definition(module, environment, Some(function.span))?.captures;
    budget.spend(captures.len())?;
    if !captures.iter().any(|ty| matches!(ty, Type::Package { .. })) {
        return Ok(CaptureViews::default());
    }
    let bad = |message| reject("E-VERIFY-CAPTURE", Some(function.span), message);
    let entry = function.entry.0 as usize;
    if !predecessors[entry].is_empty() {
        return Err(bad(
            "a Package capture entry cannot have incoming CFG edges",
        ));
    }
    let instructions = &function.blocks[entry].instructions;
    if !instructions
        .first()
        .is_some_and(|i| matches!(i.kind, InstructionKind::ScopeEnter { .. }))
    {
        return Err(bad(
            "Package capture views require an entry root ScopeEnter",
        ));
    }
    budget.spend(function.locals.len().saturating_add(captures.len()))?;
    let mut views = CaptureViews {
        by_local: vec![None; function.locals.len()],
        by_capture: vec![None; captures.len()],
    };
    let mut prefix_end = 1;
    for instruction in instructions.iter().skip(1) {
        budget.spend(1)?;
        let InstructionKind::BindPackageCapture { local, index } = instruction.kind else {
            break;
        };
        // Operand/type validation ran before this CFG contract.
        if views.by_local[local.0 as usize].replace(index).is_some()
            || views.by_capture[index as usize].replace(local).is_some()
        {
            return Err(bad(
                "Package capture bindings must be one-to-one and unique",
            ));
        }
        prefix_end += 1;
    }
    budget.spend(captures.len())?;
    if captures
        .iter()
        .enumerate()
        .any(|(index, ty)| matches!(ty, Type::Package { .. }) && views.by_capture[index].is_none())
    {
        return Err(bad(
            "every Package capture requires exactly one entry-prefix Bind",
        ));
    }
    for (block, body) in function.blocks.iter().enumerate() {
        for (position, instruction) in body.instructions.iter().enumerate() {
            budget.spend(1)?;
            match instruction.kind {
                InstructionKind::BindPackageCapture { .. }
                    if block != entry || position == 0 || position >= prefix_end =>
                {
                    return Err(bad(
                        "Package Bind must belong to the consecutive entry prefix",
                    ));
                }
                InstructionKind::EndPackageCapture { local, index }
                    if views.capture(local) != Some(index) =>
                {
                    return Err(bad("Package End must match the unique capture binding"));
                }
                InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local)
                    if views.capture(local).is_some() =>
                {
                    return Err(bad(
                        "Package capture views cannot use ordinary StorageLive or StorageDead",
                    ));
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                    if views.capture(local).is_some() =>
                {
                    return Err(bad("Package capture views cannot expose a local address"));
                }
                _ => {}
            }
        }
    }
    Ok(views)
}

pub(super) fn owned_field(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Managed(_) | Type::Closure(_) | Type::Package { .. }
    ) || matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. }))
}

/// A shape classification only. Admission remains independently checked at
/// every ordinary boundary; the old container gates never use this extension.
pub(super) fn has_children(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, field)| matches!(field, Type::Package { .. })))
}

/// Capture admission is explicit: only the complete ordinary Product profile
/// extends the old flat gate. Bind/End, per-child debts, call mode and source
/// provenance are still reconstructed from the actual closure body.
pub(super) fn check_capture_type(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    check_ordinary_type(module, ty, span, budget)
}

/// Qualification of an actual complete ordinary Product head. This does not
/// establish a loan, lifetime, initialization or permission to consume fields.
/// The complete raw type tree is bounded before this shape classification.
pub(super) fn shared_product_head(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, field)| matches!(field, Type::Managed(_) | Type::Package { .. }))
            && fields.iter().all(|(_, field)| field.is_copy_value()
                || matches!(field, Type::Managed(payload)
                    if **payload == Type::ByteBuffer
                        || (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()))
                || shared_product_head(field)))
}

pub(super) fn check_shared_product_type(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    check_ordinary_type(module, ty, span, budget)?;
    if !has_children(ty) {
        let Type::Package { fields, .. } = ty else {
            unreachable!("checked Package")
        };
        budget.spend(fields.len())?;
        if !fields.iter().all(|(_, field)| field.is_copy_value()
            || matches!(field, Type::Managed(payload) if matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. })))
        {
            return Err(unsupported("shared ordinary Product heads require Copy and finite Managed leaves", span));
        }
    }
    Ok(())
}

pub(super) fn check_ordinary_type(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    if !has_children(ty) {
        return check_type_for(module, ty, span, budget);
    }
    // Every raw type has already passed the bounded shape traversal. Charge
    // this independent nominal/resource admission and all cleanup path growth
    // before allocating its work list. No recursive Rust stack is needed.
    resources::charge_payload(ty, budget)?;
    budget.spend(2)?;
    let mut pending = vec![(ty, 1usize)];
    while let Some((ty, depth)) = pending.pop() {
        let Type::Package { name, fields } = ty else {
            unreachable!("only complete child Packages are queued");
        };
        budget.spend(depth.saturating_add(fields.len().saturating_mul(depth + 1)))?;
        if name.is_empty() {
            return Err(reject("E-VERIFY-PACKAGE", span, "empty package identity"));
        }
        let mut names = BTreeSet::new();
        let mut owns = false;
        for (name, field) in fields {
            budget.spend(
                name.len()
                    .saturating_mul(fields.len().saturating_add(1).ilog2() as usize + 2),
            )?;
            if name.is_empty() || !names.insert(name) {
                return Err(reject(
                    "E-VERIFY-PACKAGE",
                    span,
                    "package fields must have unique nonempty names",
                ));
            }
            match field {
                Type::Package { .. } => {
                    owns = true;
                    pending.push((field, depth + 1));
                }
                Type::Managed(inner)
                    if matches!(inner.as_ref(), Type::ByteBuffer | Type::Record { .. }) =>
                {
                    resources::check_type_for(module, field, span, budget)?;
                    owns = true;
                }
                _ => {
                    // Keeps existing finite Copy companions, while rejecting
                    // owning Sum/array/Store, Closure, borrowed or opaque fields.
                    check_type(field, span)?;
                }
            }
        }
        if !owns {
            return Err(unsupported(
                "each Product Package requires a transitive explicit manages leaf",
                span,
            ));
        }
    }
    Ok(())
}

pub(super) fn has_callable_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. } if fields.iter().any(|(_, ty)| matches!(ty, Type::Closure(_))))
}

pub(super) fn has_array_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, ty)| matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. }))))
}

pub(super) fn has_store_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, ty)| dynamic_store::is_carrier(ty)))
}

/// Count concrete cleanup/layout occurrences, including each nested owning
/// array repetition. Copy arrays need no generated per-element cleanup.
pub(super) fn layout_work(ty: &Type, budget: &mut Budget) -> Check<usize> {
    let mut pending = vec![(ty, 1usize)];
    let mut total = 0usize;
    while let Some((ty, count)) = pending.pop() {
        budget.spend(1)?;
        total = total
            .checked_add(count)
            .ok_or_else(|| limit("package layout work overflow"))?;
        match ty {
            Type::Array { element, length } => {
                let count = if matches!(element.as_ref(), Type::Package { .. }) {
                    count
                        .checked_mul(*length as usize)
                        .ok_or_else(|| limit("package array expansion overflow"))?
                } else {
                    count
                };
                pending.push((element, count));
            }
            Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Address { inner, .. }
            | Type::Opaque { hidden: inner, .. } => pending.push((inner, count)),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, count)));
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|ty| (ty, count)));
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    pending.extend(fields.iter().map(|ty| (ty, count)));
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.extend([(error.as_ref(), count), (result.as_ref(), count)])
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                budget.spend(parameters.len())?;
                pending.extend(parameters.iter().map(|ty| (ty, count)));
                pending.push((result, count));
            }
            Type::Slice { element } => pending.push((element, count)),
            _ => {}
        }
    }
    Ok(total)
}

pub(super) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    check_package_type(module, ty, span, budget, true)
}

fn check_package_type(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
    array_fields: bool,
) -> Check<()> {
    let Type::Package { name, fields } = ty else {
        return Err(reject(
            "E-VERIFY-PACKAGE",
            span,
            "package operation requires an actual nominal package",
        ));
    };
    resources::charge_payload(ty, budget)?;
    if name.is_empty() {
        return Err(reject("E-VERIFY-PACKAGE", span, "empty package identity"));
    }
    if has_array_fields(ty) {
        if !array_fields {
            return Err(unsupported(
                "owning array fields require direct-Managed and Copy Package elements",
                span,
            ));
        }
        let work = layout_work(ty, budget)?;
        budget.spend(work)?;
    }
    let mut names = BTreeSet::new();
    let mut owns = false;
    for (name, field) in fields {
        if name.is_empty() || !names.insert(name) {
            return Err(reject(
                "E-VERIFY-PACKAGE",
                span,
                "package fields must have unique nonempty names",
            ));
        }
        if dynamic_store::is_carrier(field) && (!matches!(field, Type::Managed(_)) || !array_fields)
        {
            return Err(unsupported(
                "Store fields require direct manages ownership outside owning array fields",
                span,
            ));
        }
        if byte_buffers::is_desc(field) {
            return Err(unsupported(
                "borrowed byte storage package fields are outside the finite profile",
                span,
            ));
        }
        if matches!(field, Type::Managed(_)) {
            resources::check_type_for(module, field, span, budget)?;
            owns = true;
        } else if let Type::Closure(id) = field {
            if !array_fields {
                return Err(unsupported(
                    "callback State cannot nest inside an owning array field",
                    span,
                ));
            }
            closures::definition(module, *id, span)?;
            closures::self_contained(module, *id, budget)?;
        } else if let Type::Array { element, length } = field
            && matches!(element.as_ref(), Type::Package { .. })
        {
            if *length == 0 {
                return Err(unsupported(
                    "owning Package array fields require a nonzero fixed length",
                    span,
                ));
            }
            check_package_type(module, element, span, budget, false)?;
        } else {
            // This independent finite-Copy admission also rejects nested
            // packages, descriptions, addresses and owning environments.
            check_type(field, span)?;
        }
    }
    if !owns {
        return Err(unsupported(
            "this package slice requires at least one manages field",
            span,
        ));
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
    let bad = |message| reject("E-VERIFY-PACKAGE", span, message);
    match &instruction.kind {
        InstructionKind::BindPackageCapture { local, index }
        | InstructionKind::EndPackageCapture { local, index } => {
            let environment = function.environment.ok_or_else(|| {
                bad("Package capture binding requires an actual closure environment")
            })?;
            let definition = closures::definition(module, environment, span)?;
            let ty = definition
                .captures
                .get(*index as usize)
                .ok_or_else(|| bad("Package capture index is outside the environment"))?;
            // End can occur on many edges. Charge each deep raw type equality,
            // rather than relying on an earlier declaration admission.
            resources::charge_payload(ty, budget)?;
            if !matches!(ty, Type::Package { .. }) || local_type(function, *local, span)? != ty {
                return Err(bad(
                    "Package capture view must exactly match its Package field type",
                ));
            }
            budget.spend(function.parameters.len())?;
            if function.parameters.contains(local) {
                return Err(bad("a Package capture view cannot also be a parameter"));
            }
            Ok(None)
        }
        InstructionKind::MakePackage { fields } => {
            let result = instruction
                .result
                .ok_or_else(|| bad("package construction requires a typed result"))?;
            let ty = value_type(function, result, span)?;
            check_ordinary_type(module, ty, span, budget)?;
            let Type::Package {
                fields: expected, ..
            } = ty
            else {
                unreachable!()
            };
            if fields.len() != expected.len() {
                return Err(bad(
                    "package construction requires every declared field exactly once",
                ));
            }
            budget.spend(fields.len())?;
            for (value, (_, expected)) in fields.iter().zip(expected) {
                if value_type(function, *value, span)? != expected {
                    return Err(bad("package input differs from its declared field type"));
                }
            }
            Ok(Some(ty.clone()))
        }
        InstructionKind::ReadPackageField { local, index }
        | InstructionKind::DescribePackageField { local, index }
        | InstructionKind::TakePackageField { local, index }
        | InstructionKind::DropPackageField { local, index }
        | InstructionKind::StorePackageField { local, index, .. }
        | InstructionKind::BorrowPackageField { local, index, .. } => {
            let Type::Package { fields, .. } = local_type(function, *local, span)? else {
                return Err(bad("package projection requires a package local"));
            };
            let (_, ty) = fields
                .get(*index as usize)
                .ok_or_else(|| bad("package field index is outside its declaration"))?;
            resources::charge_payload(ty, budget)?;
            match &instruction.kind {
                InstructionKind::BorrowPackageField { mutable, .. } => {
                    if let Type::Closure(id) = ty {
                        closures::self_contained(module, *id, budget)?;
                    } else if !*mutable && matches!(ty, Type::Package { .. }) {
                        check_shared_product_type(module, ty, span, budget)?;
                    } else if !*mutable
                        && matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Record { .. }))
                    {
                        resources::check_type_for(module, ty, span, budget)?;
                    } else if !ty.is_copy_value() {
                        return Err(bad(
                            "field borrow requires a finite Copy or exact concrete Closure field",
                        ));
                    }
                    if *mutable && !function.locals[local.0 as usize].mutable {
                        return Err(reject(
                            "E-VERIFY-MUTABILITY",
                            span,
                            "exclusive field borrow requires mutable Package storage",
                        ));
                    }
                    Ok(Some(Type::Address {
                        mutable: *mutable,
                        inner: Box::new(ty.clone()),
                    }))
                }
                InstructionKind::DescribePackageField { .. } => {
                    let Type::Managed(inner) = ty else {
                        return Err(bad("package description requires a manages field"));
                    };
                    Ok(Some(Type::Desc(inner.clone())))
                }
                InstructionKind::TakePackageField { .. }
                | InstructionKind::DropPackageField { .. } => {
                    if !owned_field(ty) {
                        return Err(bad(
                            "package field transfer or cleanup requires an owning field",
                        ));
                    }
                    Ok(
                        matches!(instruction.kind, InstructionKind::TakePackageField { .. })
                            .then(|| ty.clone()),
                    )
                }
                InstructionKind::StorePackageField { value, .. } => {
                    if !function.locals[local.0 as usize].mutable {
                        return Err(reject(
                            "E-VERIFY-MUTABILITY",
                            span,
                            "package field store requires mutable storage",
                        ));
                    }
                    if value_type(function, *value, span)? != ty {
                        return Err(bad(
                            "package field store differs from its declared field type",
                        ));
                    }
                    Ok(None)
                }
                InstructionKind::ReadPackageField { .. } => {
                    if owned_field(ty) {
                        return Err(bad("package Copy projection cannot copy an owning field"));
                    }
                    check_type(ty, span)?;
                    Ok(Some(ty.clone()))
                }
                _ => unreachable!("only field operations enter this match"),
            }
        }
        InstructionKind::DropPackage { local } => {
            if !matches!(local_type(function, *local, span)?, Type::Package { .. }) {
                return Err(bad("package cleanup requires a package local"));
            }
            Ok(None)
        }
        _ => unreachable!("only package operations enter their checker"),
    }
}
