//! Independent closed inline-resource continuation profile. This checks actual
//! input storage AND every ordinary teardown's reachable code before allowing
//! synchronous close. A result-only Task type never proves either fact.
use super::*;
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
pub(super) fn retained_pointee(ty: &Type) -> bool {
    match ty {
        ty if scalar(ty) => true,
        Type::Record { fields, .. } => fields.iter().all(|(_, field)| retained_pointee(field)),
        Type::Tuple(fields) => fields.iter().all(retained_pointee),
        Type::Array { element, .. } | Type::Slice { element } => retained_pointee(element),
        _ => false,
    }
}
pub(super) fn retained_result(ty: &Type) -> bool {
    match ty {
        ty if scalar(ty) => true,
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
) -> Check<zil_ir::RetainedLifeShape> {
    let mut ports = Vec::new();
    let mut classes = std::collections::BTreeMap::new();
    let mut next_binder = 0u32;
    for (position, parameter) in f.parameters.iter().enumerate() {
        budget.spend(1)?;
        let ty = local_type(f, *parameter, Some(f.span))?;
        resources::charge_payload(ty, budget)?;
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
        retained: if matches!(f.return_type, Type::Address { mutable: false, .. }) {
            f.storage_life
                .as_ref()
                .and_then(|life| {
                    super::super::signature_life::outer(life, zil_ir::StorageBase::Result)
                })
                .and_then(|source| classes.get(&source))
                .map(|binder| {
                    vec![zil_ir::StorageRetainedBinding {
                        path: vec![zil_ir::LifeTypeProjection::Result],
                        class: 0,
                        binder: *binder,
                    }]
                })
                .unwrap_or_default()
        } else {
            vec![]
        },
        nominal_arguments: vec![],
    })
}
fn record(t: &Type, budget: &mut Budget) -> Check<bool> {
    resources::charge_payload(t, budget)?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1)?;
        match t {
            t if scalar(t) => {}
            Type::Record { fields, .. } => {
                budget.spend(fields.len())?;
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
fn complete_aggregate(t: &Type, budget: &mut Budget) -> Check<bool> {
    resources::charge_payload(t, budget)?;
    let mut work = vec![t];
    while let Some(t) = work.pop() {
        budget.spend(1)?;
        match t {
            t if scalar(t) => {}
            Type::Managed(payload) => {
                if payload.as_ref() != &Type::ByteBuffer && !record(payload, budget)? {
                    return Ok(false);
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                budget.spend(fields.len())?;
                work.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                work.extend(fields);
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    work.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                work.extend([error.as_ref(), result.as_ref()]);
            }
            Type::Array { element, .. } => {
                if !record(element, budget)? {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn aggregate_resources<'a>(t: &'a Type, budget: &mut Budget) -> Check<Vec<&'a Type>> {
    let mut work = vec![t];
    let mut leaves = Vec::new();
    while let Some(t) = work.pop() {
        budget.spend(1)?;
        match t {
            Type::Managed(_) => leaves.push(t),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                budget.spend(fields.len())?;
                work.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                work.extend(fields);
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    work.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                work.extend([error.as_ref(), result.as_ref()]);
            }
            _ => {}
        }
    }
    Ok(leaves)
}
pub(super) fn result_type(t: &Type, budget: &mut Budget) -> Check<bool> {
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
        return if payload.as_ref() == &Type::ByteBuffer {
            Ok(true)
        } else {
            record(payload, budget)
        };
    }
    if record(t, budget)? {
        return Ok(true);
    }
    complete_aggregate(t, budget)
}
fn package(t: &Type, budget: &mut Budget) -> Check<bool> {
    let Type::Package { fields, .. } = t else {
        return Ok(false);
    };
    budget.spend(fields.len())?;
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
fn tuple(t: &Type, budget: &mut Budget) -> Check<bool> {
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
fn storage(t: &Type, budget: &mut Budget) -> Check<bool> {
    budget.spend(1)?;
    match t {
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
        Type::Address { inner, .. } => storage(inner, budget),
        t => Ok(scalar(t)),
    }
}
// A private continuation profile, independently reconstructed from actual
// concrete captures. It grants no Closed/no-future state and admits no borrow.
fn pollable_storage(module: &TypedModule, t: &Type, budget: &mut Budget) -> Check<bool> {
    resources::charge_payload(t, budget)?;
    budget.spend(1)?;
    match t {
        // An owned child head is a real asynchronous cleanup obligation. Its
        // constructor is separately traced by Task origin/migration checks.
        Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(),Type::Task{retained_life,..} if retained_life.ports.is_empty()) => {
            Ok(true)
        }
        Type::Managed(p) | Type::Desc(p)
            if matches!(p.as_ref(), Type::AsyncRegistration { .. }) =>
        {
            let Type::AsyncRegistration { environment } = p.as_ref() else {
                unreachable!()
            };
            pollable_storage(module, environment, budget)
        }
        Type::Address { inner, .. } if retained_pointee(inner) => Ok(true),
        Type::Address { inner, .. } => pollable_storage(module, inner, budget),
        Type::Closure(id) => {
            let Some(c) = module.closures.get(id.0 as usize) else {
                return Ok(false);
            };
            if !c.lending_captures.is_empty() {
                return Ok(false);
            }
            for capture in &c.captures {
                let complete = if matches!(capture, Type::Closure(_)) {
                    pollable_storage(module, capture, budget)?
                } else {
                    complete_aggregate(capture, budget)?
                };
                if !complete {
                    return Ok(false);
                }
                for leaf in aggregate_resources(capture, budget)? {
                    let Type::Managed(payload) = leaf else {
                        unreachable!()
                    };
                    budget.spend(module.resources.len())?;
                    for _ in &module.resources {
                        resources::charge_payload(payload, budget)?;
                    }
                    let Some(resource) = module.resources.iter().find(|r| r.payload == **payload)
                    else {
                        return Ok(false);
                    };
                    if let Some(drop) = resource.teardown {
                        cleanup(module, drop, budget, None)?;
                    }
                }
            }
            Ok(true)
        }
        _ => storage(t, budget),
    }
}
// This is a whole root slot profile, not a recursive addition to storage():
// Address<Enum>, a Product containing an owning Enum and pollable captures
// must not inherit the new frame qualification.
fn borrowed_driver_signature(f: &TypedFunction, budget: &mut Budget) -> Check<bool> {
    if f.protocol_contracts.is_empty() {
        return Ok(false);
    }
    // Existing owned-retirement and ordinary helpers keep their prior
    // admission/fees. Only a real borrowed registration role enters this path.
    let first = f.protocol_contracts[0].parameter as usize;
    if !f.parameters.get(first).and_then(|p|f.locals.get(p.0 as usize)).is_some_and(|local|matches!(&local.ty,Type::Address{mutable:true,inner} if matches!(inner.as_ref(),Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})))) {return Ok(false);}
    resources::charge_payload(&f.return_type, budget)?;
    budget.spend(
        f.parameters
            .len()
            .saturating_add(f.protocol_contracts.len()),
    )?;
    for local in &f.parameters {
        resources::charge_payload(&f.locals[local.0 as usize].ty, budget)?;
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
    span: Option<Span>,
) -> Check<()> {
    helper_graph(module, first, budget, span, false)
}
fn driver_call(
    module: &TypedModule,
    first: FunctionId,
    budget: &mut Budget,
    span: Option<Span>,
) -> Check<()> {
    helper_graph(module, first, budget, span, true)
}
fn task_driver_signature(f: &TypedFunction) -> Check<bool> {
    // The enclosing helper graph prepaid its parameter directory.
    let mut actual_tasks = 0;
    for local in &f.parameters {
        let ty = local_type(f, *local, Some(f.span))?;
        let payload = match ty {
            Type::Address { inner, .. } => inner.as_ref(),
            other => other,
        };
        match payload {
            Type::Managed(task) if matches!(task.as_ref(), Type::Task { .. }) => {
                super::require_closed_context(task, Some(f.span))?;
                actual_tasks += 1;
            }
            _ if scalar(ty) => {}
            _ => return Ok(false),
        }
    }
    Ok(actual_tasks != 0 && scalar(&f.return_type))
}
fn helper_graph(
    module: &TypedModule,
    first: FunctionId,
    budget: &mut Budget,
    span: Option<Span>,
    driver_root: bool,
) -> Check<()> {
    let mut pending = vec![(first, driver_root)];
    let mut seen = BTreeSet::new();
    while let Some((id, driving)) = pending.pop() {
        budget.spend(seen.len().saturating_add(1))?;
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
        )?;
        if f.call_kind != CallKind::Direct
            || f.environment.is_some()
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || !(record(&f.return_type, budget)? || complete_aggregate(&f.return_type, budget)?)
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
        let borrowed_driver = task_driver || driving && borrowed_driver_signature(f, budget)?;
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
                storage(t, budget)?
            };
            if !supported && !complete_aggregate(t, budget)? {
                return Err(reject(
                    "E-VERIFY-TASK",
                    span,
                    "task cleanup retains unsupported owned or borrowed storage",
                ));
            }
        }
        for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            if let Type::Managed(payload) = t {
                for _ in &module.resources {
                    resources::charge_payload(payload, budget)?;
                }
                if let Some(drop) = module
                    .resources
                    .iter()
                    .find(|r| r.payload == **payload)
                    .and_then(|r| r.teardown)
                {
                    pending.push((drop, false));
                }
            }
        }
        for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            if matches!(
                t,
                Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) && !t.is_copy_value()
            {
                for leaf in aggregate_resources(t, budget)? {
                    budget.spend(1)?;
                    let Type::Managed(payload) = leaf else {
                        unreachable!()
                    };
                    budget.spend(module.resources.len())?;
                    for _ in &module.resources {
                        resources::charge_payload(payload, budget)?;
                    }
                    let resource = module
                        .resources
                        .iter()
                        .find(|r| r.payload == **payload)
                        .ok_or_else(|| {
                            reject(
                                "E-VERIFY-TASK",
                                span,
                                "helper cleanup leaf has no actual resource",
                            )
                        })?;
                    if let Some(drop) = resource.teardown {
                        pending.push((drop, false));
                    }
                }
            }
        }
        for b in &f.blocks {
            for i in &b.instructions {
                budget.spend(1)?;
                match &i.kind {
                    InstructionKind::TaskPoll { .. }
                    | InstructionKind::TaskValid { .. }
                    | InstructionKind::TaskRequestCancel { .. }
                    | InstructionKind::TaskPollClose { .. }
                    | InstructionKind::TaskFinishDestroy { .. }
                        if task_driver => {}
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
                        if closed_retirement => {}
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
                    | InstructionKind::TakeTupleField { .. }
                    | InstructionKind::UnpackTuple { .. }
                    | InstructionKind::DropErrorUnion { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::MakeOwnedTuple { .. }
                    | InstructionKind::DropTuple { .. }
                    | InstructionKind::MakePackage { .. }
                    | InstructionKind::DropPackage { .. }
                    | InstructionKind::TakePackageField { .. }
                    | InstructionKind::StorePackageField { .. }
                    | InstructionKind::DropPackageField { .. }
                    | InstructionKind::VariantTagLocal { .. }
                    | InstructionKind::ReadVariantLocal { .. }
                    | InstructionKind::UnpackVariant { .. }
                    | InstructionKind::DropEnum { .. }
                    | InstructionKind::PlaceTagLocal { .. }
                    | InstructionKind::BorrowPlaceLocal { .. }
                    | InstructionKind::ReadErrorUnion { .. } => {}
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
    span: Option<Span>,
) -> Check<Vec<FunctionId>> {
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || !(f.result_mode == PassingMode::Infer
            && (if matches!(f.return_type, Type::Address { mutable: false, .. }) {
                result_type(&f.return_type, budget)?
            } else {
                record(&f.return_type, budget)? || result_type(&f.return_type, budget)?
            })
            || matches!(f.result_mode, PassingMode::Give | PassingMode::Create)
                && (matches!(f.return_type, Type::Managed(_))
                    || matches!(&f.return_type, Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::Managed(_))))
                && result_type(&f.return_type, budget)?)
        || module.resources.iter().any(|r| r.teardown == Some(f.id))
    {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "owned task worker requires a closed actual frame and complete supported result",
        ));
    }

    for parameter in &f.parameters {
        if !matches!(local_type(f, *parameter, span)?, Type::Managed(_))
            && !record(local_type(f, *parameter, span)?, budget)?
            && !complete_aggregate(local_type(f, *parameter, span)?, budget)?
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
    let mut enum_frame = false;
    for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
        // Preserve the exact former path for every already supported type.
        let new_enum = !pollable_storage(module, t, budget)?;
        if new_enum && !complete_aggregate(t, budget)? {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task owner needs a complete canonical inline resource frame slot",
            ));
        }
        let leaves = if new_enum {
            enum_frame = true;
            aggregate_resources(t, budget)?
        } else {
            match t {
                Type::Package { fields, .. } => fields.iter().map(|(_, t)| t).collect::<Vec<_>>(),
                Type::Tuple(fields) => fields.iter().collect(),
                _ => vec![t],
            }
        };
        for leaf in leaves {
            if let Type::Managed(payload) = leaf {
                for _ in &module.resources {
                    resources::charge_payload(payload, budget)?;
                }
                let resource = module
                    .resources
                    .iter()
                    .find(|r| r.payload == **payload)
                    .ok_or_else(|| {
                        reject(
                            "E-VERIFY-TASK",
                            span,
                            "owned task slot has no actual resource",
                        )
                    })?;
                if let Some(drop) = resource.teardown
                    && checked.insert(drop)
                {
                    cleanup(module, drop, budget, span)?;
                }
            }
        }
    }
    if !f.return_type.is_copy_value() && !matches!(f.return_type, Type::Managed(_)) {
        for leaf in aggregate_resources(&f.return_type, budget)? {
            let Type::Managed(payload) = leaf else {
                unreachable!()
            };
            budget.spend(module.resources.len())?;
            for _ in &module.resources {
                resources::charge_payload(payload, budget)?;
            }
            let resource = module
                .resources
                .iter()
                .find(|r| r.payload == **payload)
                .ok_or_else(|| {
                    reject(
                        "E-VERIFY-TASK",
                        span,
                        "task result leaf has no actual resource",
                    )
                })?;
            if let Some(drop) = resource.teardown {
                cleanup(module, drop, budget, span)?;
            }
        }
    }
    let mut children = vec![];
    for b in &f.blocks {
        for i in &b.instructions {
            budget.spend(1)?;
            match &i.kind {
                InstructionKind::PlaceTagLocal { local, path } if enum_frame => {
                    let ty = local_type(f, *local, Some(i.span))?;
                    resources::charge_payload(ty, budget)?;
                    if !path.is_empty() || !sums::owned_enum(ty) {
                        return Err(unsupported(
                            "Enum continuation tag view requires a complete owning Enum local and empty path",
                            Some(i.span),
                        ));
                    }
                    // Actual current-tag, initialization and loan permission
                    // remain obligations of the ordinary independent kernels.
                }
                InstructionKind::VariantTagLocal { .. }
                | InstructionKind::ReadVariantLocal { .. }
                | InstructionKind::UnpackVariant { .. }
                | InstructionKind::DropEnum { .. }
                    if enum_frame => {}
                InstructionKind::Call { function, .. } => {
                    driver_call(module, *function, budget, Some(i.span))?
                }
                InstructionKind::SuspendCall {
                    function,
                    arguments,
                } => {
                    budget.spend(arguments.len().saturating_add(1))?;
                    let target = module.functions.get(function.0 as usize).ok_or_else(|| {
                        reject("E-VERIFY-TASK", Some(i.span), "child target absent")
                    })?;
                    if arguments.len() != target.parameters.len() {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            Some(i.span),
                            "child actual arity differs",
                        ));
                    }
                    for (arg, param) in arguments.iter().zip(&target.parameters) {
                        let actual = value_type(f, *arg, Some(i.span))?;
                        let wanted = local_type(target, *param, Some(i.span))?;
                        resources::charge_payload(actual, budget)?;
                        resources::charge_payload(wanted, budget)?;
                        if actual != wanted {
                            return Err(reject(
                                "E-VERIFY-TASK",
                                Some(i.span),
                                "child actual input type differs",
                            ));
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
                | InstructionKind::MakeClosure { .. }
                | InstructionKind::DropClosure { .. }
                | InstructionKind::UnsafeEnter { .. }
                | InstructionKind::UnsafeExit { .. }
                | InstructionKind::Constant(_)
                | InstructionKind::Load(_)
                | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
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
                | InstructionKind::TaskYield
                | InstructionKind::MakeManaged { .. }
                | InstructionKind::TakeLocal { .. }
                | InstructionKind::DescribeLocal { .. }
                | InstructionKind::ReadDesc { .. }
                | InstructionKind::WriteDesc { .. }
                | InstructionKind::DropManaged { .. }
                | InstructionKind::MakeOwnedTuple { .. }
                | InstructionKind::DropTuple { .. }
                | InstructionKind::ExtractField { .. }
                | InstructionKind::BorrowFieldLocal { .. }
                | InstructionKind::BorrowPlaceLocal { .. }
                | InstructionKind::ReadAddress { .. }
                | InstructionKind::WriteAddress { .. }
                | InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::AddressField { .. }
                | InstructionKind::AddressIndex { .. }
                | InstructionKind::AddressSlice { .. }
                | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::MakePackage { .. }
                | InstructionKind::DropPackage { .. }
                | InstructionKind::TakePackageField { .. }
                | InstructionKind::StorePackageField { .. }
                | InstructionKind::DropPackageField { .. }
                | InstructionKind::MakeAggregate { .. }
                | InstructionKind::MakeVariant { .. }
                | InstructionKind::VariantTag { .. }
                | InstructionKind::ReadVariant { .. }
                | InstructionKind::MakeErrorUnion { .. }
                | InstructionKind::ErrorTag { .. }
                | InstructionKind::TakeErrorUnion { .. }
                | InstructionKind::TakeTupleField { .. }
                | InstructionKind::UnpackTuple { .. }
                | InstructionKind::DropErrorUnion { .. }
                | InstructionKind::ConvertNumber { .. }
                | InstructionKind::ReadErrorUnion { .. } => {}
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
