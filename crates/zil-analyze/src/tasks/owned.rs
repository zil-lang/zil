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
fn complete_aggregate(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(t, false, "complete task aggregate")?;
    let mut work = vec![t];
    while let Some(t) = work.pop() {
        budget.spend(1, "complete task aggregate")?;
        match t {
            t if scalar(t) => {}
            Type::Managed(payload) => {
                if payload.as_ref() != &Type::ByteBuffer && !record(payload, budget)? {
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
                if !record(element, budget)? {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn aggregate_resources<'a>(t: &'a Type, budget: &mut Budget) -> CompileResult<Vec<&'a Type>> {
    let mut work = vec![t];
    let mut leaves = Vec::new();
    while let Some(t) = work.pop() {
        budget.spend(1, "complete task aggregate")?;
        match t {
            Type::Managed(_) => leaves.push(t),
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
            _ => {}
        }
    }
    Ok(leaves)
}
pub(super) fn result_type(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
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
        budget.type_work(payload, false, "Enum resource query identity")?;
        budget.type_work(&resource.payload, false, "Enum resource candidate identity")?;
        budget.spend(
            name.len().min(resource.name.len()).saturating_add(1),
            "Enum resource name identity",
        )?;
        let same_payload = resource.payload == *payload;
        let same_name = resource.name == *name;
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
            || (f.storage_life.as_ref().is_some_and(|life| !super::input_scope::empty_relation(life)))
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
        // An explicit complete zero header is valid for the actual closed
        // Record receiver. Its presence is not a new retained class; None
        // keeps the prior interface, and nonempty/unknown rows stay rejected.
        if f.storage_life.is_some() && !record(payload, budget)? {
            return Err(reject("E-VERIFY-TASK", span,
                "complete Enum teardown zero header has a nonzero or unknown actual receiver"));
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
    }
    Ok(resource)
}
fn storage(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(1, "actual frame storage constructor")?;
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
fn pollable_storage(module: &TypedModule, t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(t, false, "pollable frame actual type")?;
    budget.spend(1, "pollable frame actual storage")?;
    match t {
        // An owned child head is a real asynchronous cleanup obligation. Its
        // constructor is separately traced by Task origin/migration checks.
        Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(),Type::Task{retained_life,..} if retained_life.ports.is_empty()) => {
            super::require_closed_context(p)?;
            Ok(super::input_scope::domain(p,budget)? == super::input_scope::Domain::Known(0))
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
            if super::input_scope::domain(t,budget)? != super::input_scope::Domain::Known(0) { return Ok(false); }
            for (index, capture) in c.captures.iter().enumerate() {
                if matches!(capture,Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..}|Type::AsyncRegistration{..})) {
                    if !pollable_storage(module,capture,budget)? { return Ok(false); }
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
                        || super::input_scope::domain(t, budget)?
                            != super::input_scope::Domain::Known(0)
                    {
                        return Ok(false);
                    }
                    // Only frame layout eligibility. Analyze's independent
                    // all-constructor/write source proof has already run.
                    continue;
                }
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
                    let resource = enum_resource(module, payload, budget, None)?;
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
    Ok(found && scalar(&f.return_type))
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
                    budget.type_work(payload, false, "actual cleanup resource equality")?;
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
                    budget.spend(1, "complete helper cleanup leaves")?;
                    let Type::Managed(payload) = leaf else {
                        unreachable!()
                    };
                    let resource = enum_resource(module, payload, budget, span)?;
                    if let Some(drop) = resource.teardown {
                        pending.push((drop, false));
                    }
                }
            }
        }
        for b in &f.blocks {
            for i in &b.instructions {
                budget.spend(1, "actual owning task profile")?;
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
    span: Option<zil_base::Span>,
) -> CompileResult<Vec<FunctionId>> {
    if f.foreign.is_some()
        || f.environment.is_some()
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

    if f.resume_bound == zil_ir::TaskResumeBound::Transfer {
        for parameter in &f.parameters {
            let ty=local_type(f,*parameter,span)?;
            if matches!(ty,Type::Closure(_))
                && (super::needs_synchronous_proof(module,ty,budget)?
                    || super::drop_sync::contains_kind(module,ty,budget,true)?) {
                return Err(fail(span.unwrap_or(f.span),"hidden asynchronous callable inputs need actual per-instance frame handoff proof"));
            }
        }
    }
    for parameter in &f.parameters {
        if !matches!(local_type(f, *parameter, span)?, Type::Managed(_))
            && !record(local_type(f, *parameter, span)?, budget)?
            && !complete_aggregate(local_type(f, *parameter, span)?, budget)?
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
    for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
        let old_storage = pollable_storage(module, t, budget)?;
        let enum_storage = !old_storage && complete_aggregate(t, budget)?;
        if !old_storage && !enum_storage {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task owner needs a complete canonical inline resource frame slot",
            ));
        }
        let mut leaves = match t {
            Type::Package { fields, .. } => fields.iter().map(|(_, t)| t).collect::<Vec<_>>(),
            Type::Tuple(fields) => fields.iter().collect(),
            _ => vec![t],
        };
        if enum_storage {
            leaves = aggregate_resources(t, budget)?;
        }
        for leaf in leaves {
            if let Type::Managed(payload) = leaf {
                let resource = if enum_storage || payload.as_ref() == &Type::ByteBuffer {
                    enum_resource(module, payload, budget, span)?
                } else {
                    for _ in &module.resources {
                        budget.type_work(payload, false, "actual task resource equality")?;
                    }
                    module
                        .resources
                        .iter()
                        .find(|r| r.payload == **payload)
                        .ok_or_else(|| {
                            reject(
                                "E-VERIFY-TASK",
                                span,
                                "owned task slot has no actual resource",
                            )
                        })?
                };
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
            let resource = enum_resource(module, payload, budget, span)?;
            if let Some(drop) = resource.teardown {
                cleanup(module, drop, budget, span)?;
            }
        }
    }
    let mut children = vec![];
    for b in &f.blocks {
        for i in &b.instructions {
            budget.spend(1, "actual owning task profile")?;
            match &i.kind {
                InstructionKind::PlaceTagLocal { local, path } => {
                    budget.spend(1, "complete Enum frame tag root")?;
                    let root = local_type(f, *local, Some(i.span))?;
                    if !path.is_empty()
                        || !matches!(root, Type::Enum { .. })
                        || record(root, budget)?
                    {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "task PlaceTagLocal requires a complete owning Enum local root and empty path",
                            Some(i.span),
                        )]);
                    }
                }
                InstructionKind::Call { function, .. }
                | InstructionKind::ForeignCall { function, .. } => {
                    driver_call(module, *function, budget, Some(i.span))?
                }
                InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::CallClosureOnce { .. } => {
                    let (definition, _) = crate::stored_callables::called(module, f, &i.kind)
                        .ok_or_else(|| {
                            fail(i.span, "Task callback has no actual checked concrete entry")
                        })?;
                    let entry = module
                        .functions
                        .get(definition.function.0 as usize)
                        .ok_or_else(|| fail(i.span, "Task callback entry is absent"))?;
                    if entry.environment != Some(definition.id)
                        || entry.call_kind != CallKind::Direct
                        || !pollable_storage(module, &Type::Closure(definition.id), budget)?
                    {
                        return Err(fail(
                            i.span,
                            "Task callback needs complete actual frame storage",
                        ));
                    }
                    // Helpers prove traces/actual operands; resources prove
                    // ownership. NoExternal separately walks this entry and
                    // every captured child/cleanup before automatic close.
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
                | InstructionKind::VariantTagLocal { .. }
                | InstructionKind::ReadVariantLocal { .. }
                | InstructionKind::UnpackVariant { .. }
                | InstructionKind::DropEnum { .. }
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
