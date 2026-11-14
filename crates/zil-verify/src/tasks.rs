//! Independent scalar task ABI admission. No producer call-mode flag is trusted.
use super::*;
mod context_work;
mod drop_sync;
pub(super) mod transfer;
pub(super) use context_work::{
    context as charge_result_context, selection as charge_result_selection,
};
pub(super) mod input_scope;
mod owned;
pub(super) use drop_sync::{
    needs_proof as needs_synchronous_proof, once_cleanup_closed, once_cleanup_has_no_task,
    require as require_synchronous_drop, require_value as require_synchronous_value,
    require_worker as require_synchronous_worker,
};
pub(super) use owned::once_cleanup_resumable;

fn empty_context() -> zil_ir::RetainedLifeShape {
    zil_ir::RetainedLifeShape {
        binder_count: 0,
        ports: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}

mod failure;
pub(crate) mod frame_shape;
mod sabi;
pub(super) fn raw_frame_data(
    module: &TypedModule,
    worker: &TypedFunction,
    declaration: &Type,
    b: &mut Budget,
) -> Check<frame_shape::Frame> {
    frame_shape::frame(module, worker, declaration, b)
}
/// Structural admission of an explicitly declared opaque frame domain. This
/// grants no Closed state, synchronous destruction or actual port source.
pub(super) fn require_closed_context(ty: &Type, span: Option<Span>) -> Check<()> {
    let Type::Task { retained_life, .. } = ty else {
        return Ok(());
    };
    if retained_life
        .retained
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
        || retained_life
            .nominal_arguments
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "Task full class/phantom directory is not canonical",
        ));
    }
    let valid = |binder: u32| binder < retained_life.binder_count;
    for port in &retained_life.ports {
        if !valid(port.binder)
            || !matches!(
                (&port.leaf, port.mode),
                (
                    Type::Address { mutable: false, .. },
                    StoredBorrowMode::SharedAddress
                ) | (
                    Type::Address { mutable: true, .. },
                    StoredBorrowMode::ExclusiveAddress
                ) | (
                    Type::Desc(_),
                    StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription
                ) | (Type::Mutex { .. }, StoredBorrowMode::SharedAddress)
            )
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "Task port changes its complete declared leaf/mode/domain",
            ));
        }
    }
    for row in &retained_life.retained {
        if !valid(row.binder)
            || !matches!(
                row.path.as_slice(),
                [LifeTypeProjection::Parameter(_)] | [LifeTypeProjection::Result]
            )
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "Task complete child-domain row selects another frame component",
            ));
        }
    }
    for row in &retained_life.nominal_arguments {
        if !valid(row.binder) || !matches!(row.path.first(), Some(LifeTypeProjection::Parameter(_)))
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "Task phantom row loses its original whole input occurrence",
            ));
        }
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
    let mut rows = retained_life
        .retained
        .iter()
        .filter(|row| row.path == [LifeTypeProjection::Result] && row.class == 0);
    let row = rows.next()?;
    if rows.next().is_some() {
        None
    } else {
        Some(row.binder)
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
        for ty in &closure.captures {
            if retained_parameter(ty, budget)?
                && !(matches!(closure.kind, CallableKind::FnOnce | CallableKind::FnMut)
                    && closure.storage_life.is_some()
                    && matches!(ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..})))
            {
                return Err(reject(
                    "E-VERIFY-TASK",
                    None,
                    "retained Task closure transport has no frame owner loan",
                ));
            }
        }
        for ty in &closure.parameters {
            if retained_parameter(ty, budget)? {
                return Err(reject(
                    "E-VERIFY-TASK",
                    None,
                    "retained Task callable arguments need complete frame argument transport",
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
                    let captured = match &i.kind {
                        InstructionKind::MakeClosure { closure, .. } => {
                            module.closures.get(closure.0 as usize).is_some_and(|d| {
                                matches!(d.kind, CallableKind::FnOnce | CallableKind::FnMut)
                                    && d.storage_life.is_some()
                            }) && matches!(ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..}))
                        }
                        _ => false,
                    };
                    // The new route is still proved from executable sources,
                    // exact capture classes and retained owner/loan facts.
                    if retained_parameter(ty, budget)? && !captured {
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
    if let Some(environment) = f.environment {
        let definition = module.closures.get(environment.0 as usize).ok_or_else(|| {
            reject(
                "E-VERIFY-TASK",
                Some(f.span),
                "Task result environment has no actual definition",
            )
        })?;
        budget.spend(
            definition
                .parameters
                .len()
                .saturating_add(definition.captures.len())
                .saturating_add(1),
        )?;
        for ty in definition
            .parameters
            .iter()
            .chain(&definition.captures)
            .chain(std::iter::once(&definition.result))
        {
            crate::stored_flow::type_work(ty, &budget.limits.clone(), budget)?;
        }
        if definition.function != f.id
            || definition.parameters != parameters
            || definition.result != f.return_type
            || definition.storage_life.as_ref() != Some(contract)
        {
            return Err(reject(
                "E-VERIFY-TASK",
                Some(f.span),
                "Task result entry differs from the complete actual environment signature",
            ));
        }
        super::signature_life::closure_relation(
            module,
            &parameters,
            &definition.captures,
            definition.kind,
            &f.return_type,
            contract,
            Some(f.span),
        )?;
        return Ok(true);
    }
    super::signature_life::function_relation_resource_scopes(
        module,
        &parameters,
        &f.return_type,
        contract,
        Some(f.span),
        f.callable_return_contract
            .as_ref()
            .is_some_and(|c| f.return_type == Type::Closure(c.closure)),
        false,
        budget,
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
pub(super) fn is_close_poll(ty: &Type) -> bool {
    ty == &task_poll_type(&Type::Unit)
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
    if matches!(payload.as_ref(), Type::Task { .. }) {
        crate::semantic::task_domains::task_metadata_domain(module, payload, budget)?;
    }
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
            let proposed = value_type(f, i.result.ok_or_else(bad)?, span)?;
            let declaration =
                crate::semantic::task_domains::constructor_declaration(proposed, budget)?;
            let Type::Task {
                result: declared_result,
                retained_life: declared_shape,
            } = declaration
            else {
                unreachable!()
            };
            if **declared_result != target.return_type {
                return Err(bad());
            }
            let frames = crate::semantic::task_domains::metadata_candidates(worker, budget)?;
            let mut selected: Option<crate::semantic::task_domains::FrameMetadata> = None;
            for frame in frames {
                if frame.result_type != target.return_type
                    || frame.shape != *declared_shape
                    || frame.parameter_types.len() != target.parameters.len()
                {
                    continue;
                }
                let mut compatible = true;
                for (expected, parameter) in frame.parameter_types.iter().zip(&target.parameters) {
                    if expected != local_type(target, *parameter, span)? {
                        compatible = false;
                        break;
                    }
                }
                if !compatible {
                    continue;
                }
                if let Some(known) = &selected {
                    if known.shape != frame.shape
                        || known.port_sources != frame.port_sources
                        || known.parameter_lives != frame.parameter_lives
                        || known.result_life != frame.result_life
                    {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            span,
                            "Task constructor has multiple unselected full original frame worlds",
                        ));
                    }
                } else {
                    selected = Some(frame);
                }
            }
            let selected = selected.ok_or_else(bad)?;
            if port_bindings != &selected.port_sources {
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
                failure::check(module, proposed, target, &selected, budget)?
            } else {
                task_create_type(&target.return_type, selected.shape.clone())
            };
            let Type::ErrorUnion { result, .. } = &ty else {
                unreachable!()
            };
            carrier(module, result, span, budget)?;
            crate::semantic::task_domains::record_raw_constructor(f, i, &ty, budget)?;
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
                    | Constant::U32(_)
                    | Constant::I32(_)
                    | Constant::I8(_)
                    | Constant::I16(_)
                    | Constant::Isize(_)
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
/// Structural work selection only. The ordinary solver still proves the
/// actual body, retained roots, loans, writes, return sources and cleanups.
/// A positive answer supplies no Direct entry for the public Suspend ABI.
pub(super) fn ordinary_sabi_body(f: &TypedFunction, budget: &mut Budget) -> Check<bool> {
    budget.spend(budget.sabi_non_suspending.len().saturating_add(1))?;
    Ok(f.call_kind == CallKind::Suspend
        && f.foreign.is_none()
        && f.environment.is_none()
        && budget.sabi_non_suspending.contains(&f.id))
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
            super::signature_life::function_relation_resource_scopes(
                module,
                &inputs,
                &f.return_type,
                contract,
                Some(f.span),
                f.callable_return_contract
                    .as_ref()
                    .is_some_and(|c| f.return_type == Type::Closure(c.closure)),
                false,
                budget,
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
    let schedule = sabi::schedule(module, budget)?;
    budget.spend(module.functions.len())?;
    let mut modes = vec![false; module.functions.len()];
    let edges = schedule.direct_edges;
    for f in &module.functions {
        if f.call_kind == CallKind::Suspend {
            modes[f.id.0 as usize] = true;
            if schedule.frame[f.id.0 as usize] {
                profile(module, f.id, budget, Some(f.span))?;
            }
            // This inventory selects SABI lowering and real resource/PC0
            // bookkeeping only. Constructor and suspension frame admission
            // above remain separate, independently checked obligations.
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
    budget.sabi_non_suspending = schedule.non_suspending;
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
                let function = module
                    .functions
                    .get(export.function.0 as usize)
                    .filter(|row| row.id == export.function)
                    .ok_or_else(|| {
                        reject(
                            "E-VERIFY-TASK-INTERFACE",
                            None,
                            "ordinary SABI export has no exact actual definition",
                        )
                    })?;
                if function.call_kind != CallKind::Suspend
                    || function.environment.is_some()
                    || function.foreign.is_some()
                {
                    return Err(reject(
                        "E-VERIFY-TASK-INTERFACE",
                        Some(function.span),
                        "ordinary SABI export changes its fixed entry ABI",
                    ));
                }
                // The interface names this actual Suspend function. Publishing
                // its signature does not supply a Direct entry; execution
                // still requires the backend's explicit SABI root adapter.
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
