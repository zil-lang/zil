//! Independent scalar task ABI admission. No producer call-mode flag is trusted.
use super::*;
mod context_work;
mod drop_sync;
pub(super) mod transfer;
pub(super) use context_work::{
    context as charge_result_context, selection as charge_result_selection,
};
mod owned;
pub(super) use drop_sync::{
    needs_proof as needs_synchronous_proof, require as require_synchronous_drop,
    require_value as require_synchronous_value, require_worker as require_synchronous_worker,
};

fn empty_context() -> zil_ir::RetainedLifeShape {
    zil_ir::RetainedLifeShape {
        binder_count: 0,
        ports: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}
pub(super) fn require_closed_context(ty: &Type, span: Option<Span>) -> Check<()> {
    if let Type::Task {
        result,
        retained_life,
    } = ty
    {
        let address_result = matches!(result.as_ref(), Type::Address { mutable: false, .. });
        if address_result || !retained_life.retained.is_empty() {
            if !address_result {
                return Err(reject(
                    "E-VERIFY-TASK-RESULT",
                    span,
                    "shared Task result has no exact shared retained input context",
                ));
            }
            if retained_life.retained.is_empty() {
                return Err(unsupported(
                    "borrowed Task retained classes require actual implemented frame port mappings",
                    span,
                ));
            }
            let result_ok = matches!(result.as_ref(), Type::Address { mutable: false, inner } if owned::retained_pointee(inner));
            let map_ok = matches!(retained_life.retained.as_slice(), [row]
                if row.path == [zil_ir::LifeTypeProjection::Result] && row.class == 0
                && row.binder < retained_life.binder_count
                && retained_life.ports.iter().any(|port| port.binder == row.binder && port.leaf == **result)
                && retained_life.ports.iter().filter(|port| port.binder == row.binder).all(|port|
                    port.mode == zil_ir::StoredBorrowMode::SharedAddress));
            if !(result_ok && map_ok && retained_life.nominal_arguments.is_empty()) {
                return Err(reject(
                    "E-VERIFY-TASK-RESULT",
                    span,
                    "shared Task result has no exact shared retained input context",
                ));
            }
        }
    }
    let mut next = 0;
    if let Type::Task {
        result,
        retained_life,
    } = ty
        && *retained_life != empty_context()
        && !((owned::retained_result(result)
            || matches!(result.as_ref(), Type::Address { mutable: false, inner } if owned::retained_pointee(inner)))
            && retained_life.binder_count > 0
            && retained_life.nominal_arguments.is_empty()
            && !retained_life.ports.is_empty()
            && retained_life.ports.iter().all(|port| {
                let dense = if port.binder == next { next += 1; true } else { port.binder < next };
                dense
                    && matches!(&port.leaf, Type::Address { mutable, inner }
                        if owned::retained_pointee(inner)
                            && port.mode == if *mutable { zil_ir::StoredBorrowMode::ExclusiveAddress } else { zil_ir::StoredBorrowMode::SharedAddress })
            })
            && next == retained_life.binder_count)
    {
        return Err(unsupported(
            "borrowed Task retained classes require actual implemented frame port mappings",
            span,
        ));
    }
    Ok(())
}

pub(super) fn task_payload(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Managed(t) | Type::Desc(t) | Type::Address { inner: t, .. } => task_payload(t),
        Type::TaskI32 | Type::Task { .. } => Some(ty),
        _ => None,
    }
}
// The context is independently admitted above. This projection selects classes,
// never pointee similarity or a constructor parameter ordinal.
pub(super) fn result_binder(ty: &Type) -> Option<u32> {
    if let Type::ErrorUnion { result, .. } = ty {
        return result_binder(result);
    }
    let Type::Task {
        result,
        retained_life,
    } = task_payload(ty)?
    else {
        return None;
    };
    if !matches!(result.as_ref(), Type::Address { mutable: false, .. }) {
        return None;
    }
    match retained_life.retained.as_slice() {
        [row] if row.path == [zil_ir::LifeTypeProjection::Result] && row.class == 0 => {
            Some(row.binder)
        }
        _ => None,
    }
}
pub(super) fn result_port(ty: &Type, port: u32) -> bool {
    if let Type::ErrorUnion { result, .. } = ty {
        return result_port(result, port);
    }
    let Some(binder) = result_binder(ty) else {
        return false;
    };
    // Full leaf equality only excludes impossible whole-input sources after the
    // declared result class is selected; it never establishes a life relation.
    matches!(task_payload(ty), Some(Type::Task { result, retained_life })
        if retained_life.ports.get(port as usize).is_some_and(|p|
            p.binder == binder && p.mode == zil_ir::StoredBorrowMode::SharedAddress && p.leaf == **result))
}
fn borrowed_result(ty: &Type) -> bool {
    if let Some(owner) = transfer::outcome_owner(ty) {
        return borrowed_result(owner);
    }
    match ty {
        Type::Task { retained_life, .. } => !retained_life.ports.is_empty(),
        Type::Managed(inner) | Type::Desc(inner) => {
            matches!(inner.as_ref(), Type::Task { retained_life, .. } if !retained_life.ports.is_empty())
        }
        Type::ErrorUnion { result, .. } => {
            matches!(result.as_ref(), Type::Managed(inner) if matches!(inner.as_ref(), Type::Task { retained_life, .. } if !retained_life.ports.is_empty()))
        }
        _ => false,
    }
}
fn retained_parameter(ty: &Type, budget: &mut Budget) -> Check<bool> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        match ty {
            Type::Task {
                result,
                retained_life,
            } => {
                if !retained_life.ports.is_empty() {
                    return Ok(true);
                }
                pending.push(result);
            }
            Type::BorrowedStorage { storage: child, .. }
            | Type::OwnedStore { element: child }
            | Type::Registration { environment: child }
            | Type::AsyncRegistration { environment: child }
            | Type::Array { element: child, .. }
            | Type::Slice { element: child }
            | Type::Address { inner: child, .. }
            | Type::Managed(child)
            | Type::Desc(child)
            | Type::Opaque { hidden: child, .. } => pending.push(child),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, t)| t))
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(false)
}
fn direct_retained_carrier(ty: &Type) -> bool {
    if let Some(owner) = transfer::outcome_owner(ty) {
        return direct_retained_carrier(owner);
    }
    let task = |ty: &Type| matches!(ty, Type::Task { retained_life, .. } if !retained_life.ports.is_empty());
    match ty {
        ty if task(ty) => true,
        Type::Managed(inner) | Type::Desc(inner) => task(inner),
        Type::Address { inner, .. } => match inner.as_ref() {
            ty if task(ty) => true,
            Type::Managed(task_ty) | Type::Desc(task_ty) => task(task_ty),
            _ => false,
        },
        Type::ErrorUnion { result, .. } => {
            matches!(result.as_ref(), Type::Managed(inner) if task(inner))
        }
        _ => false,
    }
}
fn retained_input_carrier(ty: &Type) -> bool {
    if let Some(owner) = transfer::outcome_owner(ty) {
        return retained_input_carrier(owner);
    }
    let payload = match ty {
        Type::Managed(inner) | Type::Desc(inner) => inner.as_ref(),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Managed(payload) | Type::Desc(payload) => payload.as_ref(),
            _ => return false,
        },
        _ => return false,
    };
    matches!(payload, Type::Task { retained_life, .. } if !retained_life.ports.is_empty())
}
fn reject_nested_transport(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    for closure in &module.closures {
        budget.spend(
            closure
                .captures
                .len()
                .saturating_add(closure.parameters.len()),
        )?;
        for ty in closure.captures.iter().chain(&closure.parameters) {
            if retained_parameter(ty, budget)? {
                return Err(reject(
                    "E-VERIFY-TASK",
                    None,
                    "retained Task closure transport has no frame owner loan",
                ));
            }
        }
    }
    for f in &module.functions {
        budget.spend(f.locals.len().saturating_add(f.values.len()))?;
        for ty in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            if retained_parameter(ty, budget)? && !direct_retained_carrier(ty) {
                return Err(reject(
                    "E-VERIFY-TASK",
                    Some(f.span),
                    "nested retained Task transport has no complete frame owner loan",
                ));
            }
        }
        for block in &f.blocks {
            for i in &block.instructions {
                budget.spend(1)?;
                let inputs: &[ValueId] = match &i.kind {
                    InstructionKind::MakeClosure {
                        captures: values, ..
                    }
                    | InstructionKind::MakePackage { fields: values }
                    | InstructionKind::MakeAggregate { fields: values }
                    | InstructionKind::MakeOwnedTuple { fields: values }
                    | InstructionKind::MakeOwnedArray { elements: values }
                    | InstructionKind::MakeVariant { fields: values, .. }
                    | InstructionKind::CallIndirect {
                        arguments: values, ..
                    }
                    | InstructionKind::CallDynSlot {
                        arguments: values, ..
                    }
                    | InstructionKind::ForeignCall {
                        arguments: values, ..
                    }
                    | InstructionKind::CallClosure {
                        arguments: values, ..
                    }
                    | InstructionKind::CallClosureShared {
                        arguments: values, ..
                    }
                    | InstructionKind::CallClosureMut {
                        arguments: values, ..
                    }
                    | InstructionKind::CallClosureOnce {
                        arguments: values, ..
                    }
                    | InstructionKind::CallClosureBorrowed {
                        arguments: values, ..
                    } => values,
                    InstructionKind::StorePackageField { value, .. }
                    | InstructionKind::StoreOwnedArrayElement { value, .. }
                    | InstructionKind::StoreStoredBorrow { value, .. } => {
                        std::slice::from_ref(value)
                    }
                    _ => &[],
                };
                budget.spend(inputs.len())?;
                for value in inputs {
                    let ty = f.values.get(value.0 as usize).ok_or_else(|| {
                        reject(
                            "E-VERIFY-TASK",
                            Some(i.span),
                            "Task transport operand absent",
                        )
                    })?;
                    if retained_parameter(ty, budget)? {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            Some(i.span),
                            "retained Task aggregate or indirect transport has no complete frame owner loan",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
pub(super) fn borrowed_signature(
    module: &TypedModule,
    f: &TypedFunction,
    budget: &mut Budget,
) -> Check<bool> {
    if !borrowed_result(&f.return_type) {
        if !budget.borrowed_task_ports {
            return Ok(false);
        }
        budget.spend(f.parameters.len())?;
        let mut actual_task_input = false;
        for parameter in &f.parameters {
            actual_task_input |= retained_input_carrier(local_type(f, *parameter, Some(f.span))?);
        }
        if !actual_task_input {
            return Ok(false);
        }
    }
    let Some(contract) = &f.storage_life else {
        return Ok(false);
    };
    budget.spend(f.parameters.len().saturating_add(1))?;
    let mut parameters = Vec::new();
    for parameter in &f.parameters {
        budget.spend(1)?;
        let ty = &f
            .locals
            .get(parameter.0 as usize)
            .ok_or_else(|| {
                reject(
                    "E-VERIFY-TASK",
                    Some(f.span),
                    "borrowed Task signature input absent",
                )
            })?
            .ty;
        resources::charge_payload(ty, budget)?;
        parameters.push(
            f.locals
                .get(parameter.0 as usize)
                .ok_or_else(|| {
                    reject(
                        "E-VERIFY-TASK",
                        Some(f.span),
                        "borrowed Task signature input absent",
                    )
                })?
                .ty
                .clone(),
        );
    }
    charge_result_selection(&f.return_type, budget)?;
    super::signature_life::charge(contract, budget)?;
    super::signature_life::function_relation(
        module,
        &parameters,
        &f.return_type,
        contract,
        Some(f.span),
        false,
    )?;
    Ok(true)
}
fn task_create_type(result: &Type, retained_life: zil_ir::RetainedLifeShape) -> Type {
    Type::ErrorUnion {
        error: Box::new(Type::Enum {
            name: "@std.task:v1:TaskCreateError".into(),
            variants: vec![("Allocation".into(), vec![])],
        }),
        result: Box::new(Type::Managed(Box::new(Type::Task {
            result: Box::new(result.clone()),
            retained_life,
        }))),
    }
}
fn owned_create_type(
    result: &Type,
    retained: zil_ir::RetainedLifeShape,
    parameters: Vec<Type>,
    b: &mut Budget,
) -> Check<Type> {
    b.spend(
        parameters
            .len()
            .saturating_mul(48)
            .saturating_add(retained.ports.len().saturating_mul(12)),
    )?;
    let qualified = parameters.iter().all(|t| {
        matches!(t, Type::Address { .. })
            || t.is_copy_value()
            || matches!(t, Type::Managed(payload) if payload.is_copy_value())
    }) && !retained.ports.is_empty();
    let mut rows = Vec::new();
    let mut relations = Vec::new();
    if qualified {
        let selected: Vec<_> = parameters
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(t, Type::Address { .. }))
            .collect();
        if selected.len() != retained.ports.len() {
            return Err(reject(
                "E-VERIFY-TASK",
                None,
                "owned failure actual port count differs",
            ));
        }
        for ((index, input), port) in selected.into_iter().zip(&retained.ports) {
            if input != &port.leaf {
                return Err(reject(
                    "E-VERIFY-TASK",
                    None,
                    "owned failure actual port type differs",
                ));
            }
            rows.push(zil_ir::StorageLifeSlot {
                path: vec![
                    PlaceProjection::Field(1),
                    PlaceProjection::Field(index as u32),
                ],
                mode: port.mode,
                binder: port.binder,
            });
            relations.push(zil_ir::StorageRetainedBinding {
                path: vec![
                    zil_ir::LifeTypeProjection::TypeArgument(0),
                    zil_ir::LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),
                ],
                class: 0,
                binder: port.binder,
            });
        }
        for binder in 0..retained.binder_count {
            relations.push(zil_ir::StorageRetainedBinding {
                path: vec![zil_ir::LifeTypeProjection::TypeArgument(1)],
                class: binder,
                binder,
            });
        }
    }
    let tuple = Type::Tuple(parameters);
    resources::charge_payload(&tuple, b)?;
    let name = format!("@std.task:v1:TaskCreateFailure:{}", tuple.fingerprint());
    let plain = if let Type::Tuple(fields) = &tuple {
        !fields.iter().any(|t| matches!(t, Type::Managed(_)))
    } else {
        false
    };
    let fields = vec![
        (
            "reason".into(),
            Type::Enum {
                name: "@std.task:v1:TaskCreateError".into(),
                variants: vec![("Allocation".into(), vec![])],
            },
        ),
        ("inputs".into(), tuple),
    ];
    let storage = if plain {
        Type::Record { name, fields }
    } else {
        Type::Package { name, fields }
    };
    let error = if qualified {
        Type::BorrowedStorage {
            storage: Box::new(storage),
            life: zil_ir::StorageLifeShape {
                binder_count: retained.binder_count,
                slots: rows,
                retained: relations,
                nominal_arguments: vec![],
            },
        }
    } else {
        storage
    };
    let mut expected = task_create_type(result, retained);
    if let Type::ErrorUnion { error: slot, .. } = &mut expected {
        **slot = error;
    }
    Ok(expected)
}
fn task_poll_type(result: &Type) -> Type {
    let storage = Type::Enum {
        name: format!("@std.task:v1:Poll:{}", result.fingerprint()),
        variants: vec![
            ("Pending".into(), vec![]),
            ("Ready".into(), vec![result.clone()]),
        ],
    };
    if matches!(result, Type::Address { mutable: false, .. }) {
        Type::BorrowedStorage {
            storage: Box::new(storage),
            life: zil_ir::StorageLifeShape {
                binder_count: 1,
                slots: vec![zil_ir::StorageLifeSlot {
                    path: vec![zil_ir::PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: 1,
                            index: 0,
                        },
                    )],
                    binder: 0,
                    mode: zil_ir::StoredBorrowMode::SharedAddress,
                }],
                retained: vec![zil_ir::StorageRetainedBinding {
                    path: vec![zil_ir::LifeTypeProjection::TypeArgument(0)],
                    class: 0,
                    binder: 0,
                }],
                nominal_arguments: vec![],
            },
        }
    } else {
        storage
    }
}
fn task_poll_result(result: &Type) -> Type {
    Type::ErrorUnion {
        error: Box::new(Type::Enum {
            name: "@std.task:v1:TaskPollState".into(),
            variants: ["Complete", "Cancelled", "Invalid"]
                .into_iter()
                .map(|s| (s.into(), vec![]))
                .collect(),
        }),
        result: Box::new(task_poll_type(result)),
    }
}

pub(super) fn carrier(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    budget.tasks = true;
    let (Type::Managed(payload) | Type::Desc(payload)) = ty else {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "task carrier requires actual owning or descriptor payload",
        ));
    };
    charge_result_context(payload, budget)?;
    require_closed_context(payload, span)?;
    resources::charge_payload(payload, budget)?;
    if let Type::Task { result, .. } = payload.as_ref()
        && !owned::result_type(result, budget)?
    {
        return Err(unsupported(
            "general owning task result lowering is not yet admitted by this frame profile",
            span,
        ));
    }
    let name = match payload.as_ref() {
        Type::TaskI32 => TASK_I32_RESOURCE_NAME.into(),
        Type::Task { .. } => format!("{TASK_RESOURCE_NAME_PREFIX}{}", payload.fingerprint()),
        _ => return Err(reject("E-VERIFY-TASK", span, "task payload absent")),
    };
    if !module.target.is_supported_x64_llvm()
        || !canonical_resource(&module.resources, payload, &name, budget)?
    {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "task carrier requires its exact canonical sealed native definition",
        ));
    }
    Ok(())
}
pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    budget.tasks = true;
    budget.spend(1)?;
    let span = Some(i.span);
    let bad = || {
        reject(
            "E-VERIFY-TASK",
            span,
            "task operand differs from exact sealed ABI",
        )
    };
    Ok(Some(match i.kind {
        InstructionKind::TaskCreate {
            worker,
            ref arguments,
            ref port_bindings,
        }
        | InstructionKind::TaskCreateOwned {
            worker,
            ref arguments,
            ref port_bindings,
        } => {
            budget.spend(arguments.len().saturating_add(1))?;
            profile(module, worker, budget, span)?;
            let target = &module.functions[worker.0 as usize];
            if arguments.len() != target.parameters.len() {
                return Err(bad());
            }
            for (v, p) in arguments.iter().zip(&target.parameters) {
                let actual = value_type(f, *v, span)?;
                let wanted = local_type(target, *p, span)?;
                resources::charge_payload(actual, budget)?;
                resources::charge_payload(wanted, budget)?;
                if actual != wanted {
                    return Err(bad());
                }
            }
            let retained_life = owned::borrowed_shape(target, budget)?;
            let mut expected = Vec::new();
            for (parameter, local) in target.parameters.iter().enumerate() {
                budget.spend(1)?;
                if matches!(local_type(target, *local, span)?, Type::Address { inner, .. } if owned::retained_pointee(inner))
                {
                    expected.push(zil_ir::RetainedPortBinding {
                        port: expected.len() as u32,
                        source: zil_ir::StoragePosition {
                            base: zil_ir::StorageBase::Parameter(parameter as u32),
                            path: vec![],
                        },
                    });
                }
            }
            if port_bindings != &expected {
                return Err(reject(
                    "E-VERIFY-TASK",
                    span,
                    "task retained ports differ from actual worker inputs",
                ));
            }
            for _ in 0..4 {
                resources::charge_payload(&target.return_type, budget)?;
            }
            budget.spend(budget.task_workers.len().saturating_add(1))?;
            budget.task_workers.insert(worker);
            let ty = if matches!(i.kind, InstructionKind::TaskCreateOwned { .. }) {
                let mut inputs = vec![];
                for parameter in &target.parameters {
                    let t = local_type(target, *parameter, span)?;
                    resources::charge_payload(t, budget)?;
                    resources::charge_payload(t, budget)?;
                    inputs.push(t.clone());
                }
                owned_create_type(&target.return_type, retained_life, inputs, budget)?
            } else {
                task_create_type(&target.return_type, retained_life)
            };
            let Type::ErrorUnion { result, .. } = &ty else {
                unreachable!()
            };
            carrier(module, result, span, budget)?;
            ty
        }
        InstructionKind::TaskStart { worker, argument } => {
            let target = module.functions.get(worker.0 as usize).ok_or_else(bad)?;
            if target.parameters.len() != 1
                || local_type(target, target.parameters[0], span)? != &Type::I32
                || target.return_type != Type::I32
            {
                return Err(bad());
            }
            if value_type(f, argument, span)? != &Type::I32 {
                return Err(bad());
            }
            profile(module, worker, budget, span)?;
            require_synchronous_worker(module, worker, budget, span)?;
            budget.spend(budget.task_workers.len().saturating_add(1))?;
            budget.task_workers.insert(worker);
            let ty = Type::Managed(Box::new(Type::TaskI32));
            carrier(module, &ty, span, budget)?;
            ty
        }
        InstructionKind::SuspendCall {
            function,
            ref arguments,
        } => {
            budget.spend(arguments.len().saturating_add(1))?;
            let target = module.functions.get(function.0 as usize).ok_or_else(bad)?;
            if arguments.len() != target.parameters.len() {
                return Err(bad());
            }
            for (argument, parameter) in arguments.iter().zip(&target.parameters) {
                let actual = value_type(f, *argument, span)?;
                let wanted = local_type(target, *parameter, span)?;
                resources::charge_payload(actual, budget)?;
                resources::charge_payload(wanted, budget)?;
                if actual != wanted {
                    return Err(bad());
                }
            }
            profile(module, function, budget, span)?;
            budget.spend(budget.task_workers.len().saturating_add(1))?;
            budget.task_workers.insert(function);
            resources::charge_payload(&target.return_type, budget)?;
            target.return_type.clone()
        }
        InstructionKind::TaskYield => {
            profile(module, f.id, budget, span)?;
            budget.spend(budget.task_workers.len().saturating_add(1))?;
            budget.task_workers.insert(f.id);
            Type::Unit
        }
        InstructionKind::TaskValid { task } => {
            let ty = value_type(f, task, span)?;
            if !matches!(ty, Type::Desc(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..})) {
                return Err(bad());
            }
            carrier(module, ty, span, budget)?;
            Type::Bool
        }
        InstructionKind::TaskFinishDestroy { task } => {
            let ty = value_type(f, task, span)?;
            if !matches!(ty, Type::Managed(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..}))
            {
                return Err(bad());
            }
            carrier(module, ty, span, budget)?;
            Type::Unit
        }
        InstructionKind::TaskPoll { task }
        | InstructionKind::TaskRequestCancel { task }
        | InstructionKind::TaskPollClose { task } => {
            let ty = value_type(f, task, span)?;
            if !ty.is_exclusive_task_head_address() {
                return Err(bad());
            }
            let Type::Address { inner, .. } = ty else {
                unreachable!()
            };
            carrier(module, inner, span, budget)?;
            match i.kind {
                InstructionKind::TaskPoll { .. } => match task_payload(ty) {
                    Some(Type::Task { result, .. }) => {
                        for _ in 0..4 {
                            resources::charge_payload(result, budget)?;
                        }
                        task_poll_result(result)
                    }
                    _ => Type::Tuple(vec![Type::I32, Type::I32]),
                },
                InstructionKind::TaskPollClose { .. } => {
                    if matches!(task_payload(ty), Some(Type::Task { .. })) {
                        task_poll_type(&Type::Unit)
                    } else {
                        Type::Bool
                    }
                }
                _ => Type::Unit,
            }
        }
        _ => unreachable!(),
    }))
}
fn has_await(blocks: &[BasicBlock], budget: &mut Budget) -> Check<bool> {
    budget.spend(blocks.len().saturating_mul(2))?;
    for block in blocks {
        budget.spend(block.instructions.len())?;
        if block
            .instructions
            .iter()
            .any(|i| matches!(i.kind, InstructionKind::SuspendCall { .. }))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn canonical_resource(
    resources: &[ResourceDefinition],
    payload: &Type,
    name: &str,
    budget: &mut Budget,
) -> Check<bool> {
    budget.spend(resources.len().saturating_mul(2))?;
    let mut count = 0usize;
    let mut canonical = false;
    for resource in resources {
        if std::mem::discriminant(&resource.payload) != std::mem::discriminant(payload) {
            continue;
        }
        resources::charge_payload(&resource.payload, budget)?;
        resources::charge_payload(payload, budget)?;
        if resource.payload == *payload {
            count = count.saturating_add(1);
            budget.spend(resource.name.len().saturating_add(name.len()))?;
            canonical |= resource.name == name && resource.teardown.is_none();
        }
    }
    Ok(count == 1 && canonical)
}
fn profile_one(
    module: &TypedModule,
    id: FunctionId,
    budget: &mut Budget,
    span: Option<Span>,
) -> Check<Vec<FunctionId>> {
    let f = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| reject("E-VERIFY-TASK", span, "task worker is absent"))?;
    budget.spend(
        f.locals
            .len()
            .saturating_mul(2)
            .saturating_add(f.values.len().saturating_mul(2))
            .saturating_add(module.resources.len())
            .saturating_add(1),
    )?;
    if f.locals.iter().any(|l| matches!(l.ty, Type::Managed(_)))
        || f.values.iter().any(|t| matches!(t, Type::Managed(_)))
        || f.return_type != Type::I32
        || f.parameters.len() != 1
        || f.parameters
            .first()
            .and_then(|p| f.locals.get(p.0 as usize))
            .is_none_or(|l| l.ty != Type::I32)
        || f.locals.iter().any(|l| {
            matches!(
                l.ty,
                Type::Enum { .. } | Type::ErrorUnion { .. } | Type::Tuple(_) | Type::Record { .. }
            )
        })
        || f.values.iter().any(|t| {
            matches!(
                t,
                Type::Enum { .. } | Type::ErrorUnion { .. } | Type::Tuple(_) | Type::Record { .. }
            )
        })
        || has_await(&f.blocks, budget)?
    {
        return owned::profile(module, f, budget, span);
    }
    let scalar = |t: &Type| {
        matches!(
            t,
            Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64
        )
    };
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.parameters.len() != 1
        || local_type(f, f.parameters[0], span)? != &Type::I32
        || f.result_mode != PassingMode::Infer
        || f.return_type != Type::I32
        || f.locals.iter().any(|l| !scalar(&l.ty))
        || f.values.iter().any(|ty| !scalar(ty))
        || module.resources.iter().any(|r| r.teardown == Some(id))
    {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "task worker must prove the closed scalar profile independently",
        ));
    }
    let mut children = Vec::new();
    for b in &f.blocks {
        budget.spend(1)?;
        for i in &b.instructions {
            budget.spend(1)?;
            match i.kind {
                InstructionKind::TaskYield
                | InstructionKind::SuspendCall { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::Load(_)
                | InstructionKind::Store { .. }
                | InstructionKind::Unary {
                    op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                    ..
                }
                | InstructionKind::Binary { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::Constant(
                    Constant::Unit
                    | Constant::Bool(_)
                    | Constant::U8(_)
                    | Constant::U16(_)
                    | Constant::I32(_)
                    | Constant::I64(_),
                ) => {}
                _ => {
                    return Err(reject(
                        "E-VERIFY-TASK",
                        Some(i.span),
                        "task worker operation has no closed scalar suspension lowering",
                    ));
                }
            }
            if let InstructionKind::SuspendCall {
                function,
                arguments,
            } = &i.kind
            {
                budget.spend(arguments.len().saturating_add(1))?;
                if arguments.len() != 1 || value_type(f, arguments[0], Some(i.span))? != &Type::I32
                {
                    return Err(reject(
                        "E-VERIFY-TASK",
                        Some(i.span),
                        "child input is outside actual scalar lane",
                    ));
                }
                children.push(*function);
            }
        }
    }
    Ok(children)
}
fn profile(
    module: &TypedModule,
    root: FunctionId,
    budget: &mut Budget,
    span: Option<Span>,
) -> Check<()> {
    // Independently build the actual finite child graph, then remove its leaves.
    // A surviving edge is a recursive frame shape and needs explicit storage.
    let mut pending = vec![root];
    let mut edges = std::collections::BTreeMap::new();
    while let Some(id) = pending.pop() {
        budget.spend(edges.len().saturating_add(1))?;
        if edges.contains_key(&id) {
            continue;
        }
        let children = profile_one(module, id, budget, span)?;
        budget.spend(children.len())?;
        pending.extend(children.iter().copied());
        edges.insert(id, children);
    }
    let mut removed = BTreeSet::new();
    loop {
        let mut changed = false;
        for (&id, children) in &edges {
            budget.spend(
                children
                    .len()
                    .saturating_add(removed.len())
                    .saturating_add(1),
            )?;
            if !removed.contains(&id) && children.iter().all(|child| removed.contains(child)) {
                removed.insert(id);
                changed = true;
            }
        }
        if removed.len() == edges.len() {
            return Ok(());
        }
        if !changed {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "recursive static child frames require explicit storage",
            ));
        }
    }
}
pub(super) fn module(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    if !budget.tasks {
        return Ok(());
    }
    transfer::declared(module, budget)?;
    for f in &module.functions {
        budget.spend(1)?;
        let mut has_retained_input = false;
        if budget.borrowed_task_ports {
            budget.spend(f.parameters.len())?;
            for parameter in &f.parameters {
                let ty = &f
                    .locals
                    .get(parameter.0 as usize)
                    .ok_or_else(|| reject("E-VERIFY-TASK", Some(f.span), "Task parameter absent"))?
                    .ty;
                if retained_parameter(ty, budget)? {
                    if !retained_input_carrier(ty) {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            Some(f.span),
                            "nested retained Task parameter has no frame owner loan",
                        ));
                    }
                    has_retained_input = true;
                }
            }
        }
        if has_retained_input {
            let contract = f.storage_life.as_ref().ok_or_else(|| {
                reject(
                    "E-VERIFY-TASK",
                    Some(f.span),
                    "retained Task input lacks complete source life contract",
                )
            })?;
            let mut inputs = Vec::new();
            for parameter in &f.parameters {
                budget.spend(1)?;
                charge_result_selection(&f.locals[parameter.0 as usize].ty, budget)?;
                inputs.push(
                    f.locals
                        .get(parameter.0 as usize)
                        .ok_or_else(|| {
                            reject("E-VERIFY-TASK", Some(f.span), "Task input local absent")
                        })?
                        .ty
                        .clone(),
                );
            }
            for ty in &inputs {
                charge_result_selection(ty, budget)?;
            }
            charge_result_selection(&f.return_type, budget)?;
            super::signature_life::charge(contract, budget)?;
            super::signature_life::function_relation(
                module,
                &inputs,
                &f.return_type,
                contract,
                Some(f.span),
                false,
            )?;
        }
        if borrowed_result(&f.return_type) && f.storage_life.is_none() {
            return Err(reject(
                "E-VERIFY-TASK",
                Some(f.span),
                "borrowed Task return lacks an actual retained input relation",
            ));
        }
    }
    if budget.borrowed_task_ports {
        reject_nested_transport(module, budget)?;
    }
    budget.spend(module.functions.len())?;
    let mut modes = vec![false; module.functions.len()];
    let mut edges = Vec::new();
    for f in &module.functions {
        if f.call_kind == CallKind::Suspend {
            modes[f.id.0 as usize] = true;
            profile(module, f.id, budget, Some(f.span))?;
            budget.spend(budget.task_workers.len().saturating_add(1))?;
            budget.task_workers.insert(f.id);
        }
        for b in &f.blocks {
            for i in &b.instructions {
                budget.spend(1)?;
                match i.kind {
                    InstructionKind::TaskYield | InstructionKind::SuspendCall { .. } => {
                        modes[f.id.0 as usize] = true
                    }
                    InstructionKind::Call { function, .. } => {
                        budget.spend(1)?;
                        edges.push((f.id, function));
                    }
                    _ => {}
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for &(caller, callee) in &edges {
            budget.spend(1)?;
            let to = modes
                .get(callee.0 as usize)
                .ok_or_else(|| reject("E-VERIFY-TASK", None, "call target is absent"))?;
            if *to && !modes[caller.0 as usize] {
                modes[caller.0 as usize] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    budget.spend(modes.len())?;
    for (index, mode) in modes.iter().enumerate() {
        if *mode && module.functions[index].call_kind != CallKind::Suspend {
            return Err(reject(
                "E-VERIFY-TASK",
                Some(module.functions[index].span),
                "Direct selection excludes actual reachable suspension",
            ));
        }
        if *mode {
            budget.spend(budget.suspend_functions.len().saturating_add(1))?;
            budget.suspend_functions.insert(FunctionId(index as u32));
        }
    }
    for f in &module.functions {
        for b in &f.blocks {
            for i in &b.instructions {
                budget.spend(1)?;
                let target = match i.kind {
                    InstructionKind::Call { function, .. }
                    | InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function } => Some(function),
                    InstructionKind::WithScopedThread { worker, .. } => Some(worker),
                    _ => None,
                };
                if target.is_some_and(|id| modes.get(id.0 as usize).copied().unwrap_or(false)) {
                    return Err(reject(
                        "E-VERIFY-TASK",
                        Some(i.span),
                        "actual Suspend function has no ordinary Direct execution authority",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Independent semantic SABI binding. The fields remain raw data; successful
/// checking exposes only the matching immutable program snapshot to lowering.
pub(super) fn interfaces(program: &TypedProgram, budget: &mut Budget) -> Check<()> {
    if program.task_interfaces.is_empty() && !budget.tasks {
        return Ok(());
    }
    let module = &program.module;
    budget.spend(program.task_interfaces.len())?;
    let mut seen_modules = BTreeSet::new();
    let mut seen_workers = BTreeSet::new();
    for interface in &program.task_interfaces {
        budget.spend(
            seen_modules
                .len()
                .saturating_add(interface.exports.len())
                .saturating_add(1),
        )?;
        if !seen_modules.insert(interface.module) {
            return Err(reject(
                "E-VERIFY-TASK-INTERFACE",
                None,
                "duplicate semantic task interface module",
            ));
        }
        let mut names = BTreeSet::new();
        for export in &interface.exports {
            budget.spend(
                export
                    .name
                    .len()
                    .saturating_add(names.len())
                    .saturating_add(seen_workers.len())
                    .saturating_add(1),
            )?;
            if export.abi_version != 1
                || export.name.is_empty()
                || export.parameter != (PassingMode::Infer, Type::I32)
                || export.result != (PassingMode::Infer, Type::I32)
                || !names.insert(export.name.as_str())
                || !seen_workers.insert(export.worker)
                || !budget.suspend_functions.contains(&export.worker)
            {
                return Err(reject(
                    "E-VERIFY-TASK-INTERFACE",
                    None,
                    "semantic export must name a unique actual Suspend scalar SABI v1 worker",
                ));
            }
            profile(module, export.worker, budget, None)?;
            // Module checking inventoried TaskYield even in unused definitions.
            if !budget.task_workers.contains(&export.worker) {
                return Err(reject(
                    "E-VERIFY-TASK-INTERFACE",
                    None,
                    "task export has no independently checked worker",
                ));
            }
            budget.spend(budget.task_exports.len().saturating_add(1))?;
            budget.task_exports.insert(export.worker);
        }
    }
    for interface in &program.interfaces {
        for export in &interface.exports {
            budget.spend(budget.suspend_functions.len().saturating_add(1))?;
            if budget.suspend_functions.contains(&export.function) {
                return Err(reject(
                    "E-VERIFY-TASK-DIRECT-EXPORT",
                    None,
                    "actual Suspend worker cannot acquire a scalar Direct export",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn interface_fingerprint(
    program: &TypedProgram,
    module: Fingerprint,
    budget: &mut Budget,
) -> Check<Fingerprint> {
    if program.task_interfaces.is_empty() {
        return Ok(module);
    }
    let mut hash = zil_base::FingerprintBuilder::new();
    hash.update(b"ZIL-VERIFIED-TASK-SEMANTICS-v1\0");
    hash.update(&module.0);
    budget.spend(program.task_interfaces.len())?;
    hash.update(&(program.task_interfaces.len() as u64).to_le_bytes());
    for interface in &program.task_interfaces {
        hash.update(&interface.module.0.to_le_bytes());
        hash.update(&(interface.exports.len() as u64).to_le_bytes());
        for export in &interface.exports {
            budget.spend(export.name.len().saturating_add(16))?;
            hash.update(&(export.name.len() as u64).to_le_bytes());
            hash.update(export.name.as_bytes());
            hash.update(&export.worker.0.to_le_bytes());
            hash.update(&export.abi_version.to_le_bytes());
            // interfaces() already checked exact Infer i32 -> Infer i32.
            hash.update(b"infer:i32->infer:i32\0");
        }
    }
    Ok(hash.digest())
}

#[cfg(test)]
mod budget_tests;
