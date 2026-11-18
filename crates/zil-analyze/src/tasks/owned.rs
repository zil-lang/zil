//! Producer reconstruction of closed inline-resource continuation profile. This checks actual
//! input storage AND every ordinary teardown's reachable code before allowing
//! synchronous close. A result-only Task type never proves either fact.
use super::*;
fn local_type(
    f: &TypedFunction,
    local: LocalId,
    span: Option<zil_base::Span>,
) -> CompileResult<&Type> {
    f.locals
        .get(local.0 as usize)
        .map(|l| &l.ty)
        .ok_or_else(|| fail(span.unwrap_or(f.span), "owned task local absent"))
}
fn value_type(
    f: &TypedFunction,
    value: ValueId,
    span: Option<zil_base::Span>,
) -> CompileResult<&Type> {
    f.values
        .get(value.0 as usize)
        .ok_or_else(|| fail(span.unwrap_or(f.span), "owned task value absent"))
}
fn reject(_: &str, span: Option<zil_base::Span>, message: &str) -> Vec<Diagnostic> {
    fail(span.unwrap_or_default(), message)
}
fn scalar(t: &Type) -> bool {
    matches!(
        t,
        Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
    )
}
pub(super) fn retained_pointee(t: &Type) -> bool {
    match t {
        t if scalar(t) => true,
        Type::Record { fields, .. } => fields.iter().all(|(_, field)| retained_pointee(field)),
        Type::Tuple(fields) => fields.iter().all(retained_pointee),
        Type::Array { element, .. } | Type::Slice { element } => retained_pointee(element),
        _ => false,
    }
}
pub(super) fn retained_result(t: &Type) -> bool {
    match t {
        t if scalar(t) => true,
        Type::Managed(inner) => retained_result(inner),
        Type::Record { fields, .. } => fields.iter().all(|(_, field)| retained_result(field)),
        Type::Tuple(fields) => fields.iter().all(retained_result),
        Type::Enum { variants, .. } => variants
            .iter()
            .all(|(_, fields)| fields.iter().all(retained_result)),
        Type::ErrorUnion { error, result } => retained_result(error) && retained_result(result),
        _ => false,
    }
}
pub(super) fn borrowed_shape(
    f: &TypedFunction,
    budget: &mut Budget,
) -> CompileResult<zil_ir::RetainedLifeShape> {
    let mut ports = Vec::new();
    let mut classes = std::collections::BTreeMap::new();
    let mut next_binder = 0u32;
    let mut retained = Vec::new();
    for (position, parameter) in f.parameters.iter().enumerate() {
        budget.spend(1, "actual Task retained input inventory")?;
        let ty = local_type(f, *parameter, Some(f.span))?;
        budget.type_work(ty, false, "actual Task retained input type")?;
        if let Type::Address { mutable, inner } = ty
            && retained_pointee(inner)
        {
            let source = f.storage_life.as_ref().and_then(|life| {
                super::super::signature_life::outer(
                    life,
                    zil_ir::StorageBase::Parameter(position as u32),
                )
            });
            let binder = if let Some(source) = source {
                if let Some(existing) = classes.get(&source) {
                    *existing
                } else {
                    let next = next_binder;
                    classes.insert(source, next);
                    next_binder += 1;
                    next
                }
            } else {
                let next = next_binder;
                next_binder += 1;
                next
            };
            budget.spend(2, "Task complete input child occurrence")?;
            retained.push(zil_ir::StorageRetainedBinding {
                path: vec![zil_ir::LifeTypeProjection::Parameter(
                    u32::try_from(position).map_err(|_| fail(f.span, "Task input ordinal overflows"))?)],
                class: 0,
                binder,
            });
            ports.push(zil_ir::RetainedBorrowPort {
                leaf: ty.clone(),
                mode: if *mutable {
                    zil_ir::StoredBorrowMode::ExclusiveAddress
                } else {
                    zil_ir::StoredBorrowMode::SharedAddress
                },
                binder,
            });
        }
    }
    Ok(zil_ir::RetainedLifeShape {
        binder_count: next_binder,
        ports,
        retained: {
            if matches!(f.return_type, Type::Address { mutable: false, .. }) {
                if let Some(binder) = f.storage_life.as_ref()
                    .and_then(|life| super::super::signature_life::outer(life, zil_ir::StorageBase::Result))
                    .and_then(|source| classes.get(&source)) {
                    budget.spend(2, "Task complete result child occurrence")?;
                    retained.push(zil_ir::StorageRetainedBinding {
                        path: vec![zil_ir::LifeTypeProjection::Result], class: 0, binder: *binder,
                    });
                }
            }
            retained
        },
        nominal_arguments: vec![],
    })
}
fn frame_store_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let element = match &instruction.kind {
        InstructionKind::OwnedStoreNew { element } => element,
        InstructionKind::OwnedStoreReserve { store, .. }
        | InstructionKind::OwnedStorePush { store, .. }
        | InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store }
        | InstructionKind::OwnedStoreBorrow { store, .. }
        | InstructionKind::OwnedStoreBorrowMut { store, .. } => {
            let Type::Desc(payload) = value_type(f, *store, Some(instruction.span))? else {
                return Err(fail(
                    instruction.span,
                    "frame Store operation has no actual description head",
                ));
            };
            let Type::OwnedStore { element } = payload.as_ref() else {
                return Err(fail(
                    instruction.span,
                    "frame Store operation changes its complete payload family",
                ));
            };
            crate::ownership_store::payload_definition(module, payload, b)?;
            element
        }
        _ => return Ok(false),
    };
    crate::pod_store::instruction_types(f, instruction, element, b)?;
    Ok(true)
}

fn frame_tag_local_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::PlaceTagLocal { local, path } = &instruction.kind else { return Ok(false); };
    let root = local_type(f, *local, Some(instruction.span))?;
    let complete = match (root, path.as_slice()) {
        (Type::ErrorUnion { error, result }, [])
            if matches!(result.as_ref(), Type::Managed(task) if matches!(task.as_ref(), Type::Task { .. }))
                && crate::stored_flow::failure_shape(error, budget)? =>
        {
            pollable_storage(module, root, budget)?
        }
        (Type::Enum { .. } | Type::ErrorUnion { .. }, []) => complete_aggregate(module, root, budget)?,
        _ => false,
    };
    if !complete {
        return Err(vec![Diagnostic::unsupported(Phase::Analyze,
            "task PlaceTagLocal requires a complete Sum local root and empty path", Some(instruction.span))]);
    }
    // The ordinary Sum/initialization kernel proves the current tag and
    // initialized occupant. Eligibility neither selects nor cleans an arm.
    Ok(true)
}

fn frame_package_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> CompileResult<bool> {
    let (local, index) = match instruction.kind {
        InstructionKind::ReadPackageField { local, index }
        | InstructionKind::DescribePackageField { local, index } => (local, index),
        _ => return Ok(false),
    };
    let root = local_type(f, local, Some(instruction.span))?;
    budget.type_work(root, false, "Task complete package projection root")?;
    let Type::Package { fields, .. } = crate::resources::package_storage(root) else {
        return Err(fail(
            instruction.span,
            "Task field operation requires its actual package local",
        ));
    };
    if !complete_aggregate(module, root, budget)? {
        return Err(fail(
            instruction.span,
            "Task field operation has no complete physical package",
        ));
    }
    let field = fields
        .get(index as usize)
        .map(|(_, ty)| ty)
        .ok_or_else(|| {
            fail(
                instruction.span,
                "Task field index is outside its actual package",
            )
        })?;
    let output = instruction.result.ok_or_else(|| {
        fail(
            instruction.span,
            "Task field operation has no actual result",
        )
    })?;
    let result = value_type(f, output, Some(instruction.span))?;
    budget.type_work(field, false, "Task actual package field")?;
    budget.type_work(result, false, "Task actual package field result")?;
    let valid = match (&instruction.kind, field, result) {
        (InstructionKind::ReadPackageField { .. }, field, result) => {
            field.is_copy_value() && field == result
        }
        (
            InstructionKind::DescribePackageField { .. },
            Type::Managed(payload),
            Type::Desc(actual),
        ) => payload == actual,
        _ => false,
    };
    if !valid {
        return Err(fail(
            instruction.span,
            "Task package projection changes the actual field mode or complete type",
        ));
    }
    // This selects the existing typed GEP/load or field description only.
    // Local-shell liveness, source/root/epoch, permissions and every fixed
    // class remain obligations of the ordinary resource and stored kernels.
    Ok(true)
}

fn frame_take_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> CompileResult<bool> {
    let (local, path) = match &instruction.kind {
        InstructionKind::TakeTupleField { local, path }
        | InstructionKind::TakeVariantField { local, path } => (*local, path),
        _ => return Ok(false),
    };
    let root = local_type(f, local, Some(instruction.span))?;
    if path.is_empty()
        || !pollable_storage(module, root, budget)? && !complete_aggregate(module, root, budget)?
    {
        return Err(fail(
            instruction.span,
            "Task selective take needs its complete actual local and nonempty path",
        ));
    }
    budget.spend(
        path.len().saturating_add(1),
        "Task actual selective field path",
    )?;
    let mut selected = root;
    for step in path {
        selected = match (step, selected) {
            (zil_ir::PlaceProjection::Field(index), ty) => {
                let physical = match ty {
                    Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                    other => other,
                };
                match physical {
                    Type::Tuple(fields) => fields.get(*index as usize),
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        fields.get(*index as usize).map(|(_, ty)| ty)
                    }
                    _ => None,
                }
            }
            (zil_ir::PlaceProjection::Variant(selector), Type::Enum { variants, .. }) => variants
                .get(selector.variant as usize)
                .and_then(|(_, fields)| fields.get(selector.index as usize)),
            (
                zil_ir::PlaceProjection::ErrorArm(error),
                Type::ErrorUnion {
                    error: failure,
                    result,
                },
            ) => Some(if *error {
                failure.as_ref()
            } else {
                result.as_ref()
            }),
            _ => None,
        }
        .ok_or_else(|| {
            fail(
                instruction.span,
                "Task selective take path differs from its actual local layout",
            )
        })?;
    }
    let output = instruction.result.ok_or_else(|| {
        fail(
            instruction.span,
            "Task selective take lacks its actual result",
        )
    })?;
    let result = value_type(f, output, Some(instruction.span))?;
    budget.type_work(selected, false, "Task complete selected field")?;
    budget.type_work(result, false, "Task complete selective take result")?;
    if selected != result {
        return Err(fail(
            instruction.span,
            "Task selective take changes its complete actual field type or mode",
        ));
    }
    // This is the same path followed by the native typed GEP/load and partial
    // live flags. The ordinary Sum/resource kernels independently prove its
    // current tag, initialization, unique move, source and no-later-use facts.
    Ok(true)
}

fn frame_storage_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> CompileResult<bool> {
    // Both helper graphs and persistent workers pay and consume this exact
    // physical instruction inventory. Call/creation/protocol/root-specific
    // qualifications remain in their callers; this grants no live owner,
    // active Sum tag, access permission, source or synchronous Drop proof.
    budget.spend(1, "actual Task frame operation")?;
    if frame_store_instruction(module, f, instruction, budget)?
        || frame_closure_instruction(module, f, instruction, budget)?
        || frame_tag_local_instruction(module, f, instruction, budget)?
        || frame_package_instruction(module, f, instruction, budget)?
        || frame_take_instruction(module, f, instruction, budget)?
        || frame_static_read_instruction(module, f, instruction, budget)?
    {
        return Ok(true);
    }
    Ok(matches!(
        &instruction.kind,
        InstructionKind::Constant(_)
            | InstructionKind::Load(_)
            | InstructionKind::Store { .. }
            | InstructionKind::StorageLive(_)
            | InstructionKind::StorageDead(_)
            | InstructionKind::ScopeEnter { .. }
            | InstructionKind::ScopeExit { .. }
            | InstructionKind::ReadDesc { .. }
            | InstructionKind::WriteDesc { .. }
            | InstructionKind::UnsafeEnter { .. }
            | InstructionKind::UnsafeExit { .. }
            | InstructionKind::MakeManaged { .. }
            | InstructionKind::DropManaged { .. }
            | InstructionKind::TakeLocal { .. }
            | InstructionKind::DescribeLocal { .. }
            | InstructionKind::Unary { .. }
            | InstructionKind::Binary { .. }
            | InstructionKind::ByteBufferNew
            | InstructionKind::ByteBufferReserve { .. }
            | InstructionKind::ByteBufferPush { .. }
            | InstructionKind::ByteBufferTruncate { .. }
            | InstructionKind::ByteBufferLen { .. }
            | InstructionKind::ByteBufferCapacity { .. }
            | InstructionKind::ByteBufferBorrow { .. }
            | InstructionKind::ConvertInteger { .. }
            | InstructionKind::MakeAggregate { .. }
            | InstructionKind::MakeVariant { .. }
            | InstructionKind::VariantTag { .. }
            | InstructionKind::ReadVariant { .. }
            | InstructionKind::MakeErrorUnion { .. }
            | InstructionKind::ErrorTag { .. }
            | InstructionKind::TakeErrorUnion { .. }
            | InstructionKind::UnpackTuple { .. }
            | InstructionKind::DropErrorUnion { .. }
            | InstructionKind::ConvertNumber { .. }
            | InstructionKind::MakeOwnedTuple { .. }
            | InstructionKind::DropTuple { .. }
            // The ordinary resource CFG checks the actual element ordinal,
            // live shell, moves, replacement and cleanup at every operation.
            // Frame eligibility does not assign Closed to any element.
            | InstructionKind::MakeOwnedArray { .. }
            | InstructionKind::TakeOwnedArrayElement { .. }
            | InstructionKind::BorrowOwnedArrayElement { .. }
            | InstructionKind::ProjectOwnedArrayField { .. }
            | InstructionKind::StoreOwnedArrayElement { .. }
            | InstructionKind::DropOwnedArrayElement { .. }
            | InstructionKind::DropOwnedArray { .. }
            | InstructionKind::MakePackage { .. }
            | InstructionKind::DropPackage { .. }
            | InstructionKind::TakePackageField { .. }
            | InstructionKind::StorePackageField { .. }
            | InstructionKind::DropPackageField { .. }
            | InstructionKind::VariantTagLocal { .. }
            | InstructionKind::ReadVariantLocal { .. }
            | InstructionKind::UnpackVariant { .. }
            | InstructionKind::DropEnum { .. }
            | InstructionKind::BorrowPlaceLocal { .. }
            | InstructionKind::ReadErrorUnion { .. }
    ))
}

fn static_byte_view(t: &Type) -> bool {
    matches!(t, Type::Address { mutable: false, inner }
        if inner.as_ref() == &Type::U8
            || matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8))
}

fn frame_static_read_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> CompileResult<bool> {
    let (address, result) = match instruction.kind {
        InstructionKind::StaticUtf8 { .. } => {
            let value = instruction.result.ok_or_else(|| fail(instruction.span, "Task literal has no actual result"))?;
            (value, value)
        }
        InstructionKind::SliceLength { address }
        | InstructionKind::AddressIndex { address, .. }
        | InstructionKind::AddressSlice { address, .. }
        | InstructionKind::ReadAddress { address } => {
            let result = instruction.result.ok_or_else(|| fail(instruction.span, "Task static read has no actual result"))?;
            (address, result)
        }
        _ => return Ok(false),
    };
    if !static_byte_view(value_type(f, address, Some(instruction.span))?) {
        return Ok(false);
    }
    let source = value_type(f, address, Some(instruction.span))?;
    let slice = matches!(source, Type::Address { mutable: false, inner }
        if matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8));
    let result = value_type(f, result, Some(instruction.span))?;
    let valid = match instruction.kind {
        InstructionKind::SliceLength { .. } => slice && *result == Type::I64,
        InstructionKind::ReadAddress { .. } => !slice && *result == Type::U8,
        InstructionKind::AddressIndex { .. } => slice && matches!(result,
            Type::Address { mutable: false, inner } if inner.as_ref() == &Type::U8),
        _ => slice && matches!(result, Type::Address { mutable: false, inner }
            if matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8)),
    };
    if !valid { return Err(fail(instruction.span, "Task static read changes its complete actual result")); }
    match crate::retained::check_dyn_static_return(module, f, address, &[], budget) {
        Ok(()) => Ok(true),
        Err(errors) if errors.iter().any(|error| error.category == zil_base::ResultCategory::ResourceLimit) => Err(errors),
        // A nonliteral address may still use an independently admitted
        // borrowed-driver operation. The Static category supplies no access.
        Err(_) => Ok(false),
    }
}

fn record(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(t, false, "actual owned task type")?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1, "actual owning task profile")?;
        match t {
            t if scalar(t) => {}
            Type::Record { fields, .. } => {
                budget.spend(fields.len(), "actual owning task profile")?;
                pending.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Tuple(fields) => pending.extend(fields),
            Type::Array { element, .. } => pending.push(element),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}
// Complete ordinary products/sums retain their existing canonical debt. This
// qualification grants neither thread migration nor borrowed environment escape.
fn complete_aggregate(module: &TypedModule, t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(t, false, "complete task aggregate")?;
    let mut work = vec![t];
    while let Some(t) = work.pop() {
        budget.spend(1, "complete task aggregate")?;
        match t {
            t if scalar(t) => {}
            Type::BorrowedStorage { storage, .. } => work.push(storage),
            Type::Managed(payload) => {
                if matches!(payload.as_ref(), Type::OwnedStore { .. } | Type::Task { .. } | Type::AsyncRegistration { .. }) {
                    if !pollable_storage(module, t, budget)? {
                        return Ok(false);
                    }
                } else if payload.as_ref() != &Type::ByteBuffer && !record(payload, budget)? {
                    return Ok(false);
                }
            }
            Type::Closure(_) => {
                if !pollable_storage(module, t, budget)? {
                    return Ok(false);
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                budget.spend(fields.len(), "complete task aggregate fields")?;
                work.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len(), "complete task aggregate fields")?;
                work.extend(fields);
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len(), "complete task aggregate variants")?;
                for (_, fields) in variants {
                    budget.spend(fields.len(), "complete task aggregate fields")?;
                    work.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2, "complete task aggregate arms")?;
                work.extend([error.as_ref(), result.as_ref()]);
            }
            Type::Array { element, .. } => {
                budget.spend(1, "complete task array element recipe")?;
                work.push(element);
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn aggregate_resources<'a>(
    module: &'a TypedModule,
    t: &'a Type,
    budget: &mut Budget,
) -> CompileResult<Vec<&'a Type>> {
    let mut work = vec![t];
    let mut leaves = Vec::new();
    let mut closures = BTreeSet::new();
    while let Some(t) = work.pop() {
        budget.spend(1, "complete task aggregate")?;
        match t {
            Type::Managed(_) => leaves.push(t),
            Type::BorrowedStorage { storage, .. } => work.push(storage),
            Type::Closure(id) => {
                budget.lookup(closures.len(), "frame concrete cleanup graph membership")?;
                if !closures.insert(*id) {
                    continue;
                }
                let definition = module
                    .closures
                    .get(id.0 as usize)
                    .filter(|definition| definition.id == *id)
                    .ok_or_else(|| {
                        fail(
                            zil_base::Span::default(),
                            "frame cleanup has no actual closure definition",
                        )
                    })?;
                if !frame_closure_domain(module, definition, budget)? {
                    return Err(fail(
                        zil_base::Span::default(),
                        "frame cleanup closure lacks its complete actual captured class directory",
                    ));
                }
                budget.spend(
                    definition.captures.len(),
                    "frame complete captured cleanup fields",
                )?;
                work.extend(&definition.captures);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                budget.spend(fields.len(), "complete task aggregate fields")?;
                work.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len(), "complete task aggregate fields")?;
                work.extend(fields);
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len(), "complete task aggregate variants")?;
                for (_, fields) in variants {
                    budget.spend(fields.len(), "complete task aggregate fields")?;
                    work.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2, "complete task aggregate arms")?;
                work.extend([error.as_ref(), result.as_ref()]);
            }
            Type::Array { element, .. } => {
                // This is a cleanup-definition inventory. Actual element
                // occupants and their distinct debts stay in the resource CFG.
                budget.spend(1, "complete task array cleanup recipe")?;
                work.push(element);
            }
            _ => {}
        }
    }
    Ok(leaves)
}
pub(super) fn result_type(
    module: &TypedModule,
    t: &Type,
    budget: &mut Budget,
) -> CompileResult<bool> {
    if *t == Type::I32 {
        return Ok(true);
    }
    if let Type::Address {
        mutable: false,
        inner,
    } = t
    {
        return Ok(retained_pointee(inner));
    }
    if let Type::Managed(payload) = t {
        if matches!(payload.as_ref(), Type::OwnedStore { .. } | Type::Task { .. }) {
            return pollable_storage(module, t, budget);
        }
        return if payload.as_ref() == &Type::ByteBuffer {
            Ok(true)
        } else {
            record(payload, budget)
        };
    }
    if record(t, budget)? {
        return Ok(true);
    }
    complete_aggregate(module, t, budget)
}
fn package(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let Type::Package { fields, .. } = t else {
        return Ok(false);
    };
    budget.spend(fields.len(), "actual single-owner package")?;
    let mut owners = 0;
    for (_, field) in fields {
        if let Type::Managed(payload) = field {
            owners += 1;
            if !record(payload, budget)? {
                return Ok(false);
            }
        } else if !record(field, budget)? {
            return Ok(false);
        }
    }
    Ok(owners == 1)
}
fn tuple(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let Type::Tuple(fields) = t else {
        return Ok(false);
    };
    for field in fields {
        if let Type::Managed(payload) = field {
            if !record(payload, budget)? {
                return Ok(false);
            }
        } else if !record(field, budget)? {
            return Ok(false);
        }
    }
    Ok(true)
}
// Whole Enum roots retain one canonical obligation. Their alternatives are
// not simultaneous owners; ordinary Sum transfer checks the active tag.
// Keep the existing Copy classification first so old frame costs stay exact.
fn enum_resource<'a>(
    module: &'a TypedModule,
    payload: &Type,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<&'a zil_ir::ResourceDefinition> {
    if *payload == Type::ByteBuffer {
        budget.spend(
            module.resources.len().saturating_add(1),
            "Task sealed buffer resource",
        )?;
        let mut found = None;
        for resource in &module.resources {
            if resource.payload == Type::ByteBuffer
                || resource.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
            {
                if found.is_some()
                    || resource.payload != Type::ByteBuffer
                    || resource.name != zil_ir::BYTE_BUFFER_RESOURCE_NAME
                    || resource.teardown.is_some()
                {
                    return Err(reject(
                        "E-VERIFY-TASK",
                        span,
                        "Task ByteBuffer cleanup is not uniquely sealed",
                    ));
                }
                found = Some(resource);
            }
        }
        return found
            .ok_or_else(|| reject("E-VERIFY-TASK", span, "Task ByteBuffer resource is absent"));
    }
    let Type::Record { name, .. } = payload else {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "complete Enum leaf requires an actual inline resource Record",
        ));
    };
    budget.spend(
        module.resources.len().saturating_add(1),
        "Enum resource inventory",
    )?;
    let mut selected = None;
    for resource in &module.resources {
        budget.spend(
            name.len().min(resource.name.len()).saturating_add(1),
            "Enum resource name identity",
        )?;
        let same_name = resource.name == *name;
        // Record equality includes its nominal name. An unrelated constructor
        // or Record name cannot equal this payload; avoid traversing its whole
        // fields merely to discover that mismatch. A matching resource label
        // still takes the full comparison, including malformed declarations.
        let comparable_payload = if let Type::Record {
            name: candidate, ..
        } = &resource.payload
        {
            budget.spend(
                name.len().min(candidate.len()).saturating_add(1),
                "Enum resource payload name identity",
            )?;
            candidate == name
        } else {
            false
        };
        if !same_name && !comparable_payload {
            continue;
        }
        budget.type_work(payload, false, "Enum resource query identity")?;
        budget.type_work(&resource.payload, false, "Enum resource candidate identity")?;
        let same_payload = resource.payload == *payload;
        if same_name || same_payload {
            if name.is_empty() || !same_name || !same_payload || selected.is_some() {
                return Err(reject(
                    "E-VERIFY-TASK",
                    span,
                    "complete Enum leaf resource identity is not unique and canonical",
                ));
            }
            selected = Some(resource);
        }
    }
    let resource = selected.ok_or_else(|| {
        reject(
            "E-VERIFY-TASK",
            span,
            "complete Enum leaf has no actual resource",
        )
    })?;
    if let Some(id) = resource.teardown {
        budget.spend(8, "Enum resource teardown entry")?;
        let f = module.functions.get(id.0 as usize).ok_or_else(|| {
            reject(
                "E-VERIFY-TASK",
                span,
                "complete Enum teardown entry is absent",
            )
        })?;
        if f.id != id
            || f.foreign.is_some()
            || f.environment.is_some()
            || f.call_kind != CallKind::Direct
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || f.reference_return_bound.is_some()
            || f.return_type != Type::Unit
            || f.result_mode != PassingMode::Infer
            || f.parameters.len() != 1
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "complete Enum resource teardown has an invalid actual entry",
            ));
        }
        let receiver = local_type(f, f.parameters[0], span)?;
        budget.type_work(receiver, false, "Enum teardown actual receiver")?;
        budget.type_work(payload, false, "Enum teardown expected receiver")?;
        if !matches!(receiver, Type::Desc(actual) if actual.as_ref()==payload) {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "complete Enum teardown receiver differs from its actual resource",
            ));
        }
        // Desc contributes a genuine invocation head independently of the
        // closed Record's intrinsic data domain. Validate the complete actual
        // relation, rather than confusing that outer occurrence with data.
        if f.storage_life.is_some()
            && (!record(payload, budget)?
                || !crate::signature_life::ordinary_heads::relation(module, f, budget)?)
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "complete Enum teardown changes its actual descriptor head relation",
            ));
        }
    }
    Ok(resource)
}
fn storage(module: &TypedModule, t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(1, "actual frame storage constructor")?;
    match t {
        Type::Closure(_) => pollable_storage(module, t, budget),
        Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(), Type::OwnedStore { .. }) => {
            crate::ownership_store::payload_definition(module, p, budget)?;
            super::input_scope::known_zero(module, p, budget)
        }
        Type::Managed(p) | Type::Desc(p) => {
            if p.as_ref() == &Type::ByteBuffer {
                Ok(true)
            } else {
                record(p, budget)
            }
        }
        Type::Record { .. } | Type::Enum { .. } | Type::ErrorUnion { .. } => record(t, budget),
        Type::Package { .. } => package(t, budget),
        Type::Tuple(_) => tuple(t, budget),
        Type::Address { inner, .. } if retained_pointee(inner) => Ok(true),
        Type::Address { inner, .. } => storage(module, inner, budget),
        t => Ok(scalar(t)),
    }
}
fn frame_closure_domain(module: &TypedModule, c: &zil_ir::ClosureDefinition, budget: &mut Budget) -> CompileResult<bool> {
    let domain = super::input_scope::domain(&Type::Closure(c.id), budget)?;
    let super::input_scope::Domain::Known(count) = domain else { return Ok(false); };
    if count == 0 { return Ok(true); }
    let Some(life) = &c.storage_life else { return Ok(false); };
    let Some(entry) = module.functions.get(c.function.0 as usize).filter(|entry|
        entry.id == c.function && entry.environment == Some(c.id) && entry.foreign.is_none()
            && entry.call_kind == CallKind::Direct && entry.storage_life.as_ref() == Some(life)) else {
        return Ok(false);
    };
    budget.spend(c.parameters.len().saturating_add(c.captures.len()).saturating_add(1),
        "complete Task frame closure header")?;
    if count != life.fixed_binder_count || entry.return_type != c.result
        || entry.parameters.len() != c.parameters.len() { return Ok(false); }
    for (parameter, expected) in entry.parameters.iter().zip(&c.parameters) {
        let actual = local_type(entry, *parameter, Some(entry.span))?;
        budget.type_work(actual, false, "Task frame closure actual parameter")?;
        budget.type_work(expected, false, "Task frame closure declared parameter")?;
        if actual != expected { return Ok(false); }
    }
    for ty in c.parameters.iter().chain(&c.captures).chain(std::iter::once(&c.result)) {
        budget.type_work(ty, false, "Task frame full closure class carrier")?;
    }
    crate::signature_life::charge(life, budget)?;
    crate::signature_life::closure_relation_resource_scopes(module, c.kind, &c.parameters,
        &c.captures, &c.result, life, budget)?;
    Ok(true)
}
// A private physical continuation profile. Complete retained classes stay in
// their real header domain; frame layout grants no Closed or SyncDrop proof.
fn pollable_storage(module: &TypedModule, t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    pollable_storage_at(module, t, budget, 0, &mut BTreeSet::new())
}
fn pollable_storage_at(module: &TypedModule, t: &Type, budget: &mut Budget,
    depth: usize, active: &mut BTreeSet<ClosureId>) -> CompileResult<bool> {
    if depth > budget.partial_depth {
        return Err(vec![Diagnostic::limit(Phase::Analyze, "Task frame capture storage exceeds its finite depth", None)]);
    }
    budget.type_work(t, false, "pollable frame actual type")?;
    budget.spend(1, "pollable frame actual storage")?;
    match t {
        Type::BorrowedStorage { storage, .. } => {
            pollable_storage_at(module, storage, budget, depth + 1, active)
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            budget.spend(fields.len(), "pollable frame complete product")?;
            for (_, field) in fields {
                if !pollable_storage_at(module, field, budget, depth + 1, active)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Tuple(fields) => {
            budget.spend(fields.len(), "pollable frame complete tuple")?;
            for field in fields {
                if !pollable_storage_at(module, field, budget, depth + 1, active)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Enum { variants, .. } => {
            budget.spend(variants.len(), "pollable frame complete alternatives")?;
            for (_, fields) in variants {
                budget.spend(fields.len(), "pollable frame complete alternative fields")?;
                for field in fields {
                    if !pollable_storage_at(module, field, budget, depth + 1, active)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        Type::ErrorUnion { error, result } => {
            Ok(pollable_storage_at(module, error, budget, depth + 1, active)?
                && pollable_storage_at(module, result, budget, depth + 1, active)?)
        }
        Type::Array { element, .. } => {
            // A homogeneous physical recipe is checked once. No occupant's
            // origin, Closed phase, or replacement generation follows from it.
            pollable_storage_at(module, element, budget, depth + 1, active)
        }
        Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(), Type::OwnedStore { .. }) => {
            crate::ownership_store::payload_definition(module, p, budget)?;
            Ok(super::input_scope::known_zero(module, p, budget)?)
        }
        // An owned child head is a real asynchronous cleanup obligation. Its
        // constructor is separately traced by Task origin/migration checks.
        Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(), Type::Task { .. }) => {
            // This is the actual opaque pointer/head slot, never an inline
            // expansion of its result, ports or phantom class domain.
            frame_resource(module, p, budget, None)?;
            Ok(true)
        }
        Type::Managed(p) | Type::Desc(p)
            if matches!(p.as_ref(), Type::AsyncRegistration { .. }) =>
        {
            let Type::AsyncRegistration { environment } = p.as_ref() else {
                unreachable!()
            };
            pollable_storage_at(module, environment, budget, depth + 1, active)
        }
        Type::Address { inner, .. } if retained_pointee(inner) => Ok(true),
        Type::Address { inner, .. } => pollable_storage_at(module, inner, budget, depth + 1, active),
        Type::Closure(id) => {
            let Some(c) = module.closures.get(id.0 as usize).filter(|c| c.id == *id) else {
                return Ok(false);
            };
            if !c.lending_captures.is_empty() {
                return Ok(false);
            }
            if !frame_closure_domain(module, c, budget)? { return Ok(false); }
            budget.lookup(active.len(), "Task frame active capture graph")?;
            if !active.insert(*id) {
                return Err(fail(Default::default(), "Task frame closure storage is recursively inline"));
            }
            let result = (|| {
            for (index, capture) in c.captures.iter().enumerate() {
                if matches!(capture,Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..}|Type::AsyncRegistration{..}))
                {
                    if !pollable_storage_at(module, capture, budget, depth + 1, active)? {
                        return Ok(false);
                    }
                    // A real owner leaf is drained by the enclosing root's
                    // flagged cleanup group. It is not an inline Record and
                    // has not acquired synchronous Drop or Closed here.
                    continue;
                }
                if let Type::Address {
                    mutable: false,
                    inner,
                } = capture
                {
                    let Some(life) = &c.storage_life else {
                        return Ok(false);
                    };
                    budget.spend(
                        life.static_bindings.len(),
                        "actual callback constant field selector",
                    )?;
                    if !life.static_bindings.iter().any(|position| {
                        position.base == StorageBase::Capture(index as u32)
                            && position.path.is_empty()
                    }) || !retained_pointee(inner)
                        || !super::input_scope::known_zero(module, inner, budget)?
                    {
                        return Ok(false);
                    }
                    // Only frame layout eligibility. The separate actual
                    // all-constructor/write source proof is still required.
                    continue;
                }
                let complete = pollable_storage_at(module, capture, budget, depth + 1, active)?;
                if !complete {
                    return Ok(false);
                }
                for leaf in aggregate_resources(module, capture, budget)? {
                    let Type::Managed(payload) = leaf else {
                        unreachable!()
                    };
                    let resource = frame_resource(module, payload, budget, None)?;
                    if let Some(drop) = resource.teardown {
                        cleanup(module, drop, budget, None)?;
                    }
                }
            }
            Ok(true)
            })();
            active.remove(id);
            result
        }
        _ => storage(module, t, budget),
    }
}

// One resource inventory is used after frame qualification for both direct
// owners and leaves of products/sums. An asynchronous head carries its real
// cleanup debt; admitting its sealed storage never proves Closed or SyncDrop.
fn frame_resource<'a>(
    module: &'a TypedModule,
    payload: &Type,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<&'a zil_ir::ResourceDefinition> {
    if let Type::OwnedStore { element } = payload {
        crate::ownership_store::payload_definition(module, payload, budget)?;
        if !super::input_scope::known_zero(module, payload, budget)? {
            return Err(fail(
                span.unwrap_or_default(),
                "frame Store omits its complete element class domain",
            ));
        }
        let mut selected = None;
        for resource in &module.resources {
            budget.type_work(
                &resource.payload,
                false,
                "frame Store complete resource identity",
            )?;
            budget.type_work(payload, false, "frame Store complete requested payload")?;
            if resource.payload == *payload && selected.replace(resource).is_some() {
                return Err(fail(
                    span.unwrap_or_default(),
                    "frame Store resource identity is ambiguous",
                ));
            }
        }
        let resource = selected.ok_or_else(|| {
            fail(
                span.unwrap_or_default(),
                "frame Store has no canonical resource",
            )
        })?;
        for leaf in aggregate_resources(module, element, budget)? {
            let Type::Managed(child) = leaf else {
                unreachable!()
            };
            let nested = frame_resource(module, child, budget, span)?;
            if let Some(drop) = nested.teardown {
                cleanup(module, drop, budget, span)?;
            }
        }
        return Ok(resource);
    }
    let expected = match payload {
        Type::Task { .. } => {
            super::require_closed_context(payload)?;
            budget.type_work(payload, false, "Task frame sealed resource identity")?;
            Some(format!(
                "{TASK_RESOURCE_NAME_PREFIX}{}",
                payload.fingerprint()
            ))
        }
        Type::AsyncRegistration { environment } | Type::Registration { environment } => {
            let Type::Closure(id) = environment.as_ref() else {
                return Err(fail(
                    span.unwrap_or_default(),
                    "frame registration has no real concrete environment",
                ));
            };
            crate::registration::semantic_resource_scope(module, payload, budget)?;
            let prefix = if matches!(payload, Type::AsyncRegistration { .. }) {
                crate::registration::ASYNC_PREFIX
            } else {
                "@builtin:Registration:"
            };
            budget.spend(
                prefix.len().saturating_add(32),
                "frame registration sealed identity",
            )?;
            Some(format!("{prefix}{}", id.0))
        }
        _ => None,
    };
    let Some(expected) = expected else {
        return enum_resource(module, payload, budget, span);
    };
    let mut selected = None;
    budget.spend(
        module.resources.len(),
        "frame asynchronous resource inventory",
    )?;
    for resource in &module.resources {
        budget.type_work(
            &resource.payload,
            false,
            "frame asynchronous resource identity",
        )?;
        budget.spend(
            resource.name.len().saturating_add(expected.len()),
            "frame asynchronous allocator identity",
        )?;
        if resource.payload != *payload {
            continue;
        }
        if selected.replace(resource).is_some()
            || resource.name != expected
            || resource.teardown.is_some()
        {
            return Err(fail(
                span.unwrap_or_default(),
                "frame asynchronous head has no unique canonical allocator and cleanup identity",
            ));
        }
    }
    selected.ok_or_else(|| {
        fail(
            span.unwrap_or_default(),
            "frame asynchronous head lacks its sealed resource definition",
        )
    })
}
fn borrowed_driver_signature(f: &TypedFunction, budget: &mut Budget) -> CompileResult<bool> {
    if f.protocol_contracts.is_empty() {
        return Ok(false);
    }
    // Existing owned-retirement and ordinary helpers keep their prior
    // admission/fees. Only a real borrowed registration role enters this path.
    let first = f.protocol_contracts[0].parameter as usize;
    if !f.parameters.get(first).and_then(|p|f.locals.get(p.0 as usize)).is_some_and(|local|matches!(&local.ty,Type::Address{mutable:true,inner} if matches!(inner.as_ref(),Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})))) {return Ok(false);}
    budget.type_work(&f.return_type, false, "actual driver helper return")?;
    budget.spend(
        f.parameters
            .len()
            .saturating_add(f.protocol_contracts.len()),
        "actual borrowed driver signature",
    )?;
    for local in &f.parameters {
        budget.type_work(
            &f.locals[local.0 as usize].ty,
            false,
            "actual driver helper input",
        )?;
    }
    let registration = |t: &Type| matches!(t,Type::Address{mutable:true,inner} if matches!(inner.as_ref(),Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})));
    Ok(f.foreign.is_none()
        && f.environment.is_none()
        && f.call_kind == CallKind::Direct
        && f.return_type.is_copy_value()
        && f.parameters.iter().all(|p| {
            let t = &f.locals[p.0 as usize].ty;
            t.is_copy_value() || registration(t)
        })
        && f.protocol_contracts.iter().all(|c| {
            f.parameters
                .get(c.parameter as usize)
                .is_some_and(|p| registration(&f.locals[p.0 as usize].ty))
        }))
}
fn cleanup(
    module: &TypedModule,
    first: FunctionId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    helper_graph(module, first, budget, span, false)
}
// Shared physical frame qualification for actual closure operations in a
// Suspend root and its Direct helpers. This does not prove that a slot is live,
// movable or synchronously droppable: resources::check_module still checks
// those obligations at every actual instruction, including guarded cleanup.
fn frame_closure_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &zil_ir::Instruction,
    budget: &mut Budget,
) -> CompileResult<bool> {
    let id = match &instruction.kind {
        InstructionKind::MakeClosure { closure, .. } => *closure,
        InstructionKind::DropClosure { local } => {
            let Type::Closure(id) = local_type(f, *local, Some(instruction.span))? else {
                return Err(fail(
                    instruction.span,
                    "frame closure cleanup has another actual local type",
                ));
            };
            *id
        }
        InstructionKind::CallClosure { .. }
        | InstructionKind::CallClosureShared { .. }
        | InstructionKind::CallClosureMut { .. }
        | InstructionKind::CallClosureBorrowed { .. }
        | InstructionKind::CallClosureOnce { .. } => {
            let (definition, _) = crate::stored_callables::called(module, f, &instruction.kind)
                .ok_or_else(|| {
                    fail(
                        instruction.span,
                        "frame callback has no exact actual mode and entry",
                    )
                })?;
            definition.id
        }
        _ => return Ok(false),
    };
    budget.spend(2, "actual frame closure definition and entry")?;
    let definition = module
        .closures
        .get(id.0 as usize)
        .filter(|definition| definition.id == id)
        .ok_or_else(|| fail(instruction.span, "frame closure identity is absent"))?;
    let entry = module
        .functions
        .get(definition.function.0 as usize)
        .filter(|entry| entry.id == definition.function)
        .ok_or_else(|| fail(instruction.span, "frame closure actual entry is absent"))?;
    if entry.environment != Some(id)
        || entry.foreign.is_some()
        || entry.call_kind != CallKind::Direct
        || entry.parameters.len() != definition.parameters.len()
    {
        return Err(fail(
            instruction.span,
            "frame closure has another actual entry or call ABI",
        ));
    }
    for (local, expected) in entry.parameters.iter().zip(&definition.parameters) {
        let actual = local_type(entry, *local, Some(instruction.span))?;
        budget.type_work(actual, false, "actual frame closure parameter")?;
        budget.type_work(expected, false, "declared frame closure parameter")?;
        if actual != expected {
            return Err(fail(
                instruction.span,
                "frame closure entry changes a complete parameter",
            ));
        }
    }
    budget.type_work(&entry.return_type, false, "actual frame closure result")?;
    budget.type_work(&definition.result, false, "declared frame closure result")?;
    for contract in entry.storage_life.iter().chain(&definition.storage_life) {
        crate::signature_life::charge(contract, budget)?;
    }
    if entry.return_type != definition.result
        || entry.storage_life != definition.storage_life
        || !pollable_storage(module, &Type::Closure(id), budget)?
    {
        return Err(fail(
            instruction.span,
            "frame closure needs its complete actual capture storage",
        ));
    }
    // Exact capture operands/results, receiver mode and parameter types. The
    // body/role kernels subsequently prove all actual call traces and loans;
    // automatic Task/registration cleanup retains its separate origin/Closed
    // proof in the live resource CFG, rather than a type-only permission here.
    crate::stored_callables::types(module, f, instruction, budget)?;
    Ok(true)
}

fn task_driver_signature(f: &TypedFunction) -> CompileResult<bool> {
    let mut found = false;
    // helper_graph already prepays this complete parameter inventory.
    for parameter in &f.parameters {
        let ty = local_type(f, *parameter, Some(f.span))?;
        let head = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        if let Type::Managed(payload) = head
            && matches!(payload.as_ref(), Type::Task { .. })
        {
            super::require_closed_context(payload)?;
            found = true;
        } else if !scalar(ty) {
            return Ok(false);
        }
    }
    Ok(found)
}
fn driver_call(
    module: &TypedModule,
    first: FunctionId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    helper_graph(module, first, budget, span, true)
}
fn helper_graph(
    module: &TypedModule,
    first: FunctionId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
    driver_root: bool,
) -> CompileResult<()> {
    let owner = module as *const TypedModule as usize;
    let key = (
        [
            budget.partial_depth,
            budget.helper_nodes,
            budget.helper_calls,
            budget.helper_edges,
            budget.helper_depth,
        ],
        first,
        driver_root,
    );
    let scoped = budget
        .initialization
        .as_ref()
        .is_some_and(|cache| cache.module == owner);
    if scoped {
        helper_scope(budget);
        let width = budget
            .initialization
            .as_ref()
            .unwrap()
            .task_helper_graphs
            .len();
        budget.lookup(width, "immutable helper frame graph query")?;
        if let Some([nested, options, heads]) = budget
            .initialization
            .as_ref()
            .unwrap()
            .task_helper_graphs
            .get(&key)
            .copied()
        {
            budget.nested_store_windows |= nested;
            budget.store_option_results |= options;
            budget.head_descriptions |= heads;
            return Ok(());
        }
    }
    helper_graph_uncached(module, first, budget, span, driver_root)?;
    // Publish only after the entire transitive graph succeeds. A request
    // without a bound immutable module always executes the complete cold walk.
    if scoped {
        helper_scope(budget);
        let flags = [
            budget.nested_store_windows,
            budget.store_option_results,
            budget.head_descriptions,
        ];
        let width = budget
            .initialization
            .as_ref()
            .unwrap()
            .task_helper_graphs
            .len();
        budget.lookup(width, "immutable helper frame graph publication")?;
        budget
            .initialization
            .as_mut()
            .unwrap()
            .task_helper_graphs
            .insert(key, flags);
    }
    Ok(())
}
fn helper_scope(budget: &mut Budget) {
    let scope = budget
        .task_input_scopes
        .as_ref()
        .map(|table| table.identity());
    let cache = budget.initialization.as_mut().unwrap();
    let same = match (&cache.task_helper_scope, &scope) {
        (Some(old), Some(current)) => std::sync::Arc::ptr_eq(old, current),
        (None, None) => true,
        _ => false,
    };
    if !same {
        cache.task_helper_graphs.clear();
        cache.task_helper_scope = scope;
    }
}
fn helper_graph_uncached(
    module: &TypedModule,
    first: FunctionId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
    driver_root: bool,
) -> CompileResult<()> {
    let mut pending = vec![(first, driver_root)];
    let mut seen = BTreeSet::new();
    while let Some((id, driving)) = pending.pop() {
        budget.spend(seen.len().saturating_add(1), "actual owning task profile")?;
        if !seen.insert((id, driving)) {
            continue;
        }
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| reject("E-VERIFY-TASK", span, "task cleanup target absent"))?;
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(f.parameters.len()),
            "actual owning task profile",
        )?;
        let handoff = driving
            && match (&f.return_type, &f.callable_return_contract) {
                (Type::Closure(id), Some(contract)) if *id == contract.closure => {
                    // Forming/returning an owning environment does not invoke it.
                    // Retained-return and resource kernels prove the actual moved
                    // capture sources; this is frame eligibility only.
                    pollable_storage(module, &f.return_type, budget)?
                }
                _ => false,
            };
        // A driving call may hand an actual retained Task or Poll result back
        // to the enclosing continuation. Its layout and its use as a live
        // owner are independent of the stricter synchronous destructor graph.
        let result_storage = if driving {
            pollable_storage(module, &f.return_type, budget)?
        } else {
            record(&f.return_type, budget)? || complete_aggregate(module, &f.return_type, budget)?
        };
        if f.call_kind != CallKind::Direct
            || f.environment.is_some()
            // A callable role is a checked input relation, not a different
            // physical frame ABI. helpers::check validates its complete real
            // occurrences after this layout pass; no role grants cleanup here.
            || f.callable_return_contract.is_some() && !handoff
            || !result_storage
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task cleanup must have an actual closed synchronous Direct graph",
            ));
        }
        if f.foreign.is_some() {
            if !scalar(&f.return_type)
                || f.parameters
                    .iter()
                    .any(|p| !scalar(&f.locals[p.0 as usize].ty))
                || f.foreign
                    .as_ref()
                    .is_some_and(|x| x.scoped_callback.is_some())
            {
                return Err(reject(
                    "E-VERIFY-TASK",
                    span,
                    "task cleanup cannot publish frame storage to a foreign entry",
                ));
            }
            continue;
        }
        let task_driver = driving && task_driver_signature(f)?;
        // A Direct factory may build a real Task and explicitly drive its
        // failed prepared inputs to Closed before returning. Its result only
        // selects this physical walk: every local, SSA, leaf teardown and
        // instruction below is still checked on the same transitive graph.
        // TaskYield/SuspendCall remain absent from this Direct operation set.
        let task_factory = driving
            && matches!(
                f.result_mode,
                PassingMode::Infer | PassingMode::Give | PassingMode::Create
            )
            && match &f.return_type {
                Type::Managed(payload) => matches!(payload.as_ref(), Type::Task { .. }),
                Type::ErrorUnion { result, .. } => matches!(result.as_ref(), Type::Managed(payload)
                    if matches!(payload.as_ref(), Type::Task { .. })),
                _ => false,
            };
        let borrowed_driver = task_driver
            || task_factory
            || handoff
            || driving && borrowed_driver_signature(f, budget)?;
        // This contract is only a premise selector. The resource and protocol
        // kernels independently require the original owner to be consumed and
        // prove checked finish observes Closed; the annotation cannot retire it.
        let closed_retirement = !f.protocol_contracts.is_empty() && f.protocol_contracts.iter().all(|c| {
            c.entry.as_slice() == [zil_ir::ProtocolState::Closed]
                && !c.exits.is_empty()
                && c.exits.iter().all(|row| row.input == zil_ir::ProtocolState::Closed && row.output.is_empty())
                && f.parameters.get(c.parameter as usize).is_some_and(|local| matches!(&f.locals[local.0 as usize].ty, Type::Managed(t) if matches!(t.as_ref(), Type::AsyncRegistration { .. })))
        });
        for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            let closed_input = closed_retirement
                && matches!(t, Type::Managed(inner) if matches!(inner.as_ref(), Type::AsyncRegistration { .. }));
            if closed_input {
                continue;
            }
            let supported = if borrowed_driver {
                pollable_storage(module, t, budget)?
            } else {
                storage(module, t, budget)?
            };
            if !supported && !complete_aggregate(module, t, budget)? {
                return Err(reject(
                    "E-VERIFY-TASK",
                    span,
                    "task cleanup retains unsupported owned or borrowed storage",
                ));
            }
        }
        for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            for leaf in aggregate_resources(module, t, budget)? {
                budget.spend(1, "complete helper cleanup leaves")?;
                let Type::Managed(payload) = leaf else {
                    unreachable!()
                };
                let resource = frame_resource(module, payload, budget, span)?;
                if let Some(drop) = resource.teardown {
                    pending.push((drop, false));
                }
            }
        }
        for b in &f.blocks {
            for i in &b.instructions {
                if frame_storage_instruction(module, f, i, budget)? {
                    continue;
                }
                match &i.kind {
                    InstructionKind::TaskPoll { .. }
                    | InstructionKind::TaskValid { .. }
                    | InstructionKind::TaskRequestCancel { .. }
                    | InstructionKind::TaskPollClose { .. }
                    | InstructionKind::TaskFinishDestroy { .. }
                        if task_driver || task_factory => {}
                    InstructionKind::TaskCreate { .. }
                    | InstructionKind::TaskCreateOwned { .. }
                    | InstructionKind::AsyncRegistrationStart { .. }
                        if task_factory => {}
                    InstructionKind::Call { function, .. }
                    | InstructionKind::ForeignCall { function, .. } => {
                        pending.push((*function, borrowed_driver))
                    }
                    InstructionKind::AsyncRegistrationRequestClose { .. }
                    | InstructionKind::AsyncRegistrationPollClose { .. }
                    | InstructionKind::AsyncRegistrationValid { .. }
                    | InstructionKind::BorrowLocal { .. }
                    | InstructionKind::BorrowLocalMut { .. }
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::ReadAddress { .. }
                    | InstructionKind::WriteAddress { .. }
                        if borrowed_driver => {}
                    InstructionKind::AsyncRegistrationFinishDestroyChecked { .. }
                    | InstructionKind::AsyncRegistrationFinishDestroy { .. }
                        if closed_retirement || task_factory => {}
                    _ => {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            Some(i.span),
                            "task cleanup contains an asynchronous or external-user operation",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
pub(super) fn profile(
    module: &TypedModule,
    f: &TypedFunction,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<Vec<FunctionId>> {
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || !(f.result_mode == PassingMode::Infer
            && (if matches!(f.return_type, Type::Address { mutable: false, .. }) {
                result_type(module, &f.return_type, budget)?
            } else {
                record(&f.return_type, budget)? || result_type(module, &f.return_type, budget)?
            })
            || matches!(f.result_mode, PassingMode::Give | PassingMode::Create)
                && (matches!(f.return_type, Type::Managed(_))
                    || matches!(&f.return_type, Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::Managed(_))))
                && result_type(module, &f.return_type, budget)?)
        || module.resources.iter().any(|r| r.teardown == Some(f.id))
    {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "owned task worker requires a closed actual frame and complete supported result",
        ));
    }

    if f.resume_bound == zil_ir::TaskResumeBound::Transfer {
        for parameter in &f.parameters {
            let ty = local_type(f, *parameter, span)?;
            if matches!(ty, Type::Closure(_))
                && (super::needs_synchronous_proof(module, ty, budget)?
                    || super::drop_sync::contains_kind(module, ty, budget, true)?)
            {
                return Err(fail(
                    span.unwrap_or(f.span),
                    "hidden asynchronous callable inputs need actual per-instance frame handoff proof",
                ));
            }
        }
    }
    for parameter in &f.parameters {
        if !matches!(local_type(f, *parameter, span)?, Type::Managed(_))
            && !record(local_type(f, *parameter, span)?, budget)?
            && !complete_aggregate(module, local_type(f, *parameter, span)?, budget)?
            && !(matches!(local_type(f, *parameter, span)?, Type::Closure(_))
                && pollable_storage(module, local_type(f, *parameter, span)?, budget)?)
            && !matches!(local_type(f, *parameter, span)?, Type::Address { inner, .. } if retained_pointee(inner))
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task initial frame cannot retain borrowed input storage",
            ));
        }
    }
    let mut checked = BTreeSet::new();
    for (is_value, index, t) in f.locals.iter().map(|l| (false, l.id.0 as usize, &l.ty))
        .chain(f.values.iter().enumerate().map(|(index, ty)| (true, index, ty))) {
        let old_storage = pollable_storage(module, t, budget)?;
        let enum_storage = !old_storage && complete_aggregate(module, t, budget)?;
        if !old_storage && !enum_storage {
            budget.type_work(t, false, "Task rejected complete frame slot")?;
            budget.spend(192, "Task rejected canonical slot diagnostic")?;
            let mut errors = reject("E-VERIFY-TASK", span,
                "task owner needs a complete canonical inline resource frame slot");
            errors[0].notes.push(format!("actual function {} ({}) {} {index}, complete Type {t:?}",
                f.id.0, f.name, if is_value { "SSA" } else { "local" }));
            return Err(errors);
        }
        let leaves = aggregate_resources(module, t, budget)?;
        for leaf in leaves {
            if let Type::Managed(payload) = leaf {
                let resource = frame_resource(module, payload, budget, span)?;
                if let Some(drop) = resource.teardown
                    && checked.insert(drop)
                {
                    cleanup(module, drop, budget, span)?;
                }
            }
        }
    }
    if !f.return_type.is_copy_value() && !matches!(f.return_type, Type::Managed(_)) {
        for leaf in aggregate_resources(module, &f.return_type, budget)? {
            let Type::Managed(payload) = leaf else {
                unreachable!()
            };
            let resource = frame_resource(module, payload, budget, span)?;
            if let Some(drop) = resource.teardown {
                cleanup(module, drop, budget, span)?;
            }
        }
    }
    let mut children = vec![];
    for b in &f.blocks {
        for i in &b.instructions {
            if frame_storage_instruction(module, f, i, budget)? {
                continue;
            }
            match &i.kind {
                InstructionKind::Call { function, .. }
                | InstructionKind::ForeignCall { function, .. } => {
                    driver_call(module, *function, budget, Some(i.span))?
                }
                InstructionKind::SuspendCall {
                    function,
                    arguments,
                } => {
                    budget.spend(arguments.len().saturating_add(1), "child actual signature")?;
                    let target = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| fail(i.span, "child target absent"))?;
                    if arguments.len() != target.parameters.len() {
                        return Err(fail(i.span, "child actual arity differs"));
                    }
                    for (arg, param) in arguments.iter().zip(&target.parameters) {
                        let actual = value_type(f, *arg, Some(i.span))?;
                        let wanted = local_type(target, *param, Some(i.span))?;
                        budget.type_work(actual, false, "child actual input")?;
                        budget.type_work(wanted, false, "child signature input")?;
                        if actual != wanted {
                            return Err(fail(i.span, "child actual input type differs"));
                        }
                    }
                    children.push(*function);
                }
                InstructionKind::TaskCreate { .. }
                | InstructionKind::TaskCreateOwned { .. }
                | InstructionKind::TaskPoll { .. }
                | InstructionKind::TaskValid { .. }
                | InstructionKind::TaskRequestCancel { .. }
                | InstructionKind::TaskPollClose { .. }
                | InstructionKind::TaskFinishDestroy { .. }
                | InstructionKind::AsyncRegistrationStart { .. }
                | InstructionKind::AsyncRegistrationValid { .. }
                | InstructionKind::AsyncRegistrationRequestClose { .. }
                | InstructionKind::AsyncRegistrationPollClose { .. }
                | InstructionKind::AsyncRegistrationFinishDestroy { .. }
                | InstructionKind::AsyncRegistrationFinishDestroyChecked { .. }
                | InstructionKind::TaskYield
                | InstructionKind::ExtractField { .. }
                | InstructionKind::BorrowFieldLocal { .. }
                | InstructionKind::ReadAddress { .. }
                | InstructionKind::WriteAddress { .. }
                | InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::AddressField { .. }
                | InstructionKind::AddressIndex { .. }
                | InstructionKind::AddressSlice { .. }
                | InstructionKind::ReborrowAddress { .. } => {}
                _ => {
                    return Err(reject(
                        "E-VERIFY-TASK",
                        Some(i.span),
                        "task owned frame operation requires complete saved responsibility lowering",
                    ));
                }
            }
        }
    }
    Ok(children)
}

/// Layout eligibility for a consuming invocation's remaining-environment
/// continuation. Resource transfer and actual per-frame debts are independent.
pub(crate) fn once_cleanup_resumable(
    module: &TypedModule,
    f: &TypedFunction,
    callee: ValueId,
    budget: &mut Budget,
) -> CompileResult<bool> {
    if f.call_kind != CallKind::Suspend || f.foreign.is_some() || f.environment.is_some() {
        return Ok(false);
    }
    let Some(ty @ Type::Closure(_)) = f.values.get(callee.0 as usize) else {
        return Ok(false);
    };
    if !super::needs_synchronous_proof(module, ty, budget)?
        && !(budget.async_registrations
            && super::drop_sync::contains_kind(module, ty, budget, true)?)
    {
        return Ok(false);
    }
    pollable_storage(module, ty, budget)
}

#[cfg(test)]
mod frame_consumer_tests {
    use super::*;
    #[allow(dead_code)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
        ));
    }

    #[test]
    fn array_frame_keeps_complete_callable_resources_without_proving_closed() {
        let mut module = crate::static_utf8_tests::module();
        let task = Type::Task {
            result: Box::new(Type::I32),
            retained_life: super::super::empty_context(),
        };
        let owner = Type::Managed(Box::new(task.clone()));
        let life = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            static_bindings: vec![],
            classes: vec![],
        };
        module.functions[0].environment = Some(ClosureId(0));
        module.functions[0].storage_life = Some(life.clone());
        module.functions[0].return_type = Type::I32;
        module.closures = vec![zil_ir::ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![owner.clone(), owner.clone()],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::FnMut,
            lending_captures: vec![],
            storage_life: Some(life),
        }];
        module.resources = vec![zil_ir::ResourceDefinition {
            name: format!("{TASK_RESOURCE_NAME_PREFIX}{}", task.fingerprint()),
            payload: task,
            teardown: None,
        }];
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        budget.task_input_scopes =
            Some(super::super::input_scope::build(&module, &limits, &mut budget).unwrap());
        let mut prior_work = None;
        for length in [1, 2, u32::MAX] {
            let array = Type::Array {
                element: Box::new(Type::Closure(ClosureId(0))),
                length,
            };
            let before = budget.remaining();
            assert!(pollable_storage(&module, &array, &mut budget).unwrap());
            assert!(complete_aggregate(&module, &array, &mut budget).unwrap());
            assert_eq!(
                aggregate_resources(&module, &array, &mut budget).unwrap(),
                vec![&owner, &owner],
                "both same-typed captured resource leaves remain in the recipe"
            );
            assert!(
                super::super::drop_sync::contains_kind(&module, &array, &mut budget, false)
                    .unwrap(),
                "frame storage must retain the separate actual Task cleanup obligation"
            );
            let work = before - budget.remaining();
            if let Some(previous) = prior_work {
                assert_eq!(
                    work, previous,
                    "a type recipe does not expand an array into occupants"
                );
            }
            prior_work = Some(work);
        }
        let array = Type::Array {
            element: Box::new(Type::Closure(ClosureId(0))),
            length: 1,
        };
        for invalid in 0..3 {
            let mut changed = module.clone();
            match invalid {
                0 => changed.resources.clear(),
                1 => changed.resources[0].name = "not-the-task-allocator".into(),
                _ => changed.resources.push(changed.resources[0].clone()),
            }
            assert!(
                pollable_storage(&changed, &array, &mut budget).is_err(),
                "an array must not hide a missing, substituted or duplicate resource identity"
            );
        }
        module.closures.clear();
        assert!(
            !pollable_storage(&module, &array, &mut budget).unwrap(),
            "the element still needs its actual closure definition"
        );
    }

    #[test]
    fn actual_main_profile_consumes_selected_task_success_arm() {
        // Exercise the production entry that rejected the real adapter. This
        // is frame eligibility only; ordinary branch/move proofs remain separate.
        let mut module = fixture::module();
        module.functions.truncate(1);
        module.closures.clear();
        let task = Type::Task {
            result: Box::new(Type::I32),
            retained_life: zil_ir::RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let owner = Type::Managed(Box::new(task.clone()));
        module.resources = vec![zil_ir::ResourceDefinition {
            name: format!("{TASK_RESOURCE_NAME_PREFIX}{}", task.fingerprint()),
            payload: task.clone(),
            teardown: None,
        }];
        let f = &mut module.functions[0];
        f.call_kind = CallKind::Suspend;
        f.storage_life = None;
        f.parameters.clear();
        f.return_type = Type::I32;
        f.locals.truncate(1);
        f.locals[0].id = LocalId(0);
        f.locals[0].ty = Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(owner.clone()),
        };
        f.values = vec![owner];
        f.blocks.truncate(1);
        f.blocks[0].instructions = vec![fixture::instruction(
            Some(0),
            InstructionKind::TakeVariantField {
                local: LocalId(0),
                path: vec![zil_ir::PlaceProjection::ErrorArm(false)],
            },
        )];
        assert!(
            profile(
                &module,
                &module.functions[0],
                &mut Budget::new(&Limits::default()),
                None
            )
            .unwrap()
            .is_empty()
        );
        module.functions[0].values[0] = Type::Desc(Box::new(task));
        let errors = profile(
            &module,
            &module.functions[0],
            &mut Budget::new(&Limits::default()),
            None,
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("complete actual field type or mode"))
        );
    }

    #[test]
    fn package_frame_description_preserves_actual_field_mode_and_qualified_owner() {
        let payload = Type::Record {
            name: "ResultToken".into(),
            fields: vec![("code".into(), Type::I32)],
        };
        let owner = Type::Managed(Box::new(payload.clone()));
        let packet = Type::BorrowedStorage {
            storage: Box::new(Type::Package {
                name: "Packet".into(),
                fields: vec![("token".into(), owner.clone()), ("code".into(), Type::I32)],
            }),
            life: zil_ir::StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let mut module = fixture::module();
        module.functions.truncate(1);
        let f = &mut module.functions[0];
        f.locals.truncate(1);
        f.locals[0].id = LocalId(0);
        f.locals[0].ty = packet.clone();
        f.values = vec![Type::Desc(Box::new(payload.clone()))];
        let describe = fixture::instruction(
            Some(0),
            InstructionKind::DescribePackageField {
                local: LocalId(0),
                index: 0,
            },
        );
        let mut budget = Budget::new(&Limits::default());
        assert!(
            frame_package_instruction(&module, &module.functions[0], &describe, &mut budget)
                .unwrap()
        );
        assert_eq!(
            aggregate_resources(&module, &packet, &mut budget).unwrap(),
            vec![&owner]
        );
        // The mode and complete nominal payload matter even when both
        // alternatives would be represented by machine pointers.
        for output in [
            owner.clone(),
            Type::I32,
            Type::Desc(Box::new(Type::Record {
                name: "DifferentToken".into(),
                fields: vec![("code".into(), Type::I32)],
            })),
        ] {
            module.functions[0].values[0] = output;
            assert!(
                frame_package_instruction(
                    &module,
                    &module.functions[0],
                    &describe,
                    &mut Budget::new(&Limits::default())
                )
                .is_err()
            );
        }
        let read = fixture::instruction(
            Some(0),
            InstructionKind::ReadPackageField {
                local: LocalId(0),
                index: 0,
            },
        );
        module.functions[0].values[0] = owner;
        assert!(
            frame_package_instruction(
                &module,
                &module.functions[0],
                &read,
                &mut Budget::new(&Limits::default())
            )
            .is_err(),
            "an owning field cannot be copied"
        );
        module.functions[0].values[0] = Type::I32;
        let scalar = fixture::instruction(
            Some(0),
            InstructionKind::ReadPackageField {
                local: LocalId(0),
                index: 1,
            },
        );
        assert!(
            frame_package_instruction(
                &module,
                &module.functions[0],
                &scalar,
                &mut Budget::new(&Limits::default())
            )
            .unwrap()
        );
    }

    #[test]
    fn frame_closure_missing_actual_domain_is_never_an_empty_scope_proof() {
        let mut module = fixture::module();
        module.functions.truncate(1);
        module.functions[0].environment = Some(ClosureId(0));
        module.functions[0].storage_life = None;
        module.functions[0].parameters.clear();
        module.functions[0].return_type = Type::I32;
        let closure = zil_ir::ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::FnOnce,
            lending_captures: vec![],
            storage_life: None,
        };
        module.closures = vec![closure.clone()];
        let mut budget = Budget::new(&Limits::default());
        assert!(!frame_closure_domain(&module, &closure, &mut budget).unwrap());
        assert!(!pollable_storage(&module, &Type::Closure(ClosureId(0)), &mut budget).unwrap());
    }

    #[test]
    fn direct_task_factory_has_no_saved_yield_or_suspend_call_activation() {
        let mut module = fixture::module();
        module.functions.truncate(1);
        module.closures.clear();
        let task = Type::Task {
            result: Box::new(Type::I32),
            retained_life: zil_ir::RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        module.resources = vec![zil_ir::ResourceDefinition {
            name: format!("{TASK_RESOURCE_NAME_PREFIX}{}", task.fingerprint()),
            payload: task.clone(),
            teardown: None,
        }];
        let f = &mut module.functions[0];
        f.parameters.clear();
        f.locals.clear();
        f.values.clear();
        f.storage_life = None;
        f.callable_return_contract = None;
        f.reference_return_bound = None;
        f.return_type = Type::Managed(Box::new(task));
        f.blocks.truncate(1);
        for kind in [
            InstructionKind::TaskYield,
            InstructionKind::SuspendCall {
                function: FunctionId(0),
                arguments: vec![],
            },
        ] {
            module.functions[0].blocks[0].instructions = vec![fixture::instruction(None, kind)];
            let errors = helper_graph(
                &module,
                FunctionId(0),
                &mut Budget::new(&Limits::default()),
                None,
                true,
            )
            .unwrap_err();
            assert!(errors.iter().any(|error| {
                error
                    .message
                    .contains("asynchronous or external-user operation")
            }));
        }
    }
}
