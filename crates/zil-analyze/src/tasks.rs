//! Producer checks reconstruct the scalar task profile and actual suspension.
use super::*;
mod borrowed_loans;
mod context_work;
mod drop_sync;
pub(super) use context_work::{
    context as charge_result_context, selection as charge_result_selection,
};
mod owned;
mod port_origins;
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
pub(super) fn require_closed_context(ty: &Type) -> CompileResult<()> {
    if let Type::Task {
        result,
        retained_life,
    } = ty
        && (matches!(result.as_ref(), Type::Address { mutable: false, .. })
            || !retained_life.retained.is_empty())
    {
        if !matches!(result.as_ref(), Type::Address { mutable: false, .. }) {
            return Err(fail(
                zil_base::Span::default(),
                "shared Task result requires its complete actual shared input context",
            ));
        }
        if retained_life.retained.is_empty() {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "borrowed Task retained classes require actual implemented frame port mappings",
                None,
            )]);
        }
        let relation = matches!(retained_life.retained.as_slice(), [row]
            if row.path == [zil_ir::LifeTypeProjection::Result] && row.class == 0
            && row.binder < retained_life.binder_count
            && retained_life.ports.iter().any(|port| port.binder == row.binder && port.leaf == **result)
            && retained_life.ports.iter().filter(|port| port.binder == row.binder).all(|port|
                port.mode == zil_ir::StoredBorrowMode::SharedAddress));
        if !matches!(result.as_ref(), Type::Address { mutable: false, inner } if owned::retained_pointee(inner))
            || !relation
            || !retained_life.nominal_arguments.is_empty()
        {
            return Err(fail(
                zil_base::Span::default(),
                "shared Task result requires its complete actual shared input context",
            ));
        }
    }
    let mut next = 0;
    if let Type::Task { result, retained_life } = ty
        && *retained_life != empty_context()
        && !((owned::retained_result(result)
            || matches!(result.as_ref(), Type::Address { mutable: false, inner } if owned::retained_pointee(inner)))
            && retained_life.binder_count > 0
            && retained_life.nominal_arguments.is_empty()
            && !retained_life.ports.is_empty()
            && retained_life.ports.iter().all(|port| {
                let dense = if port.binder == next { next += 1; true } else { port.binder < next };
                dense && matches!(&port.leaf, Type::Address { mutable, inner }
                    if owned::retained_pointee(inner)
                    && port.mode == if *mutable { zil_ir::StoredBorrowMode::ExclusiveAddress } else { zil_ir::StoredBorrowMode::SharedAddress })
            }) && next == retained_life.binder_count)
    {
        return Err(vec![Diagnostic::unsupported(Phase::Analyze,
            "borrowed Task retained classes require actual implemented frame port mappings", None)]);
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
fn retained_parameter(ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(ty, false, "Task parameter retained shape inventory")?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1, "Task parameter retained shape walk")?;
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
pub(super) fn borrowed_signature(f: &TypedFunction, budget: &mut Budget) -> CompileResult<bool> {
    if !borrowed_result(&f.return_type) {
        return Ok(false);
    }
    let Some(contract) = &f.storage_life else {
        return Ok(false);
    };
    budget.spend(
        f.parameters.len().saturating_add(1),
        "Task signature input inventory",
    )?;
    let mut parameters = Vec::new();
    for parameter in &f.parameters {
        budget.spend(1, "borrowed Task signature input")?;
        let ty = &f
            .locals
            .get(parameter.0 as usize)
            .ok_or_else(|| fail(f.span, "borrowed Task signature input absent"))?
            .ty;
        budget.type_work(ty, false, "Task signature complete input clone")?;
        parameters.push(
            f.locals
                .get(parameter.0 as usize)
                .ok_or_else(|| fail(f.span, "borrowed Task signature input absent"))?
                .ty
                .clone(),
        );
    }
    charge_result_selection(&f.return_type, budget)?;
    super::signature_life::charge(contract, budget)?;
    super::signature_life::function_relation(&parameters, &f.return_type, contract, false, &[])?;
    Ok(true)
}
fn direct_retained_parameter(ty: &Type) -> bool {
    let task = match ty {
        Type::Managed(inner) | Type::Desc(inner) => inner.as_ref(),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Managed(payload) | Type::Desc(payload) => payload.as_ref(),
            _ => return false,
        },
        _ => return false,
    };
    matches!(task, Type::Task { retained_life, .. } if !retained_life.ports.is_empty())
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

fn fail(span: zil_base::Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-TASK-ABI",
        Some(span),
        message,
    )]
}
fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
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
}
fn has_await(blocks: &[BasicBlock], budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(
        blocks.len().saturating_mul(2),
        "task lane classification blocks",
    )?;
    for block in blocks {
        budget.spend(
            block.instructions.len(),
            "task lane classification instructions",
        )?;
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
fn matching_resources(
    resources: &[ResourceDefinition],
    payload: &Type,
    budget: &mut Budget,
) -> CompileResult<usize> {
    budget.spend(
        resources.len().saturating_mul(2),
        "task resource comparison rows",
    )?;
    let mut count = 0usize;
    for resource in resources {
        if std::mem::discriminant(&resource.payload) != std::mem::discriminant(payload) {
            continue;
        }
        budget.type_work(&resource.payload, false, "actual task resource equality")?;
        budget.type_work(payload, false, "query task resource equality")?;
        if resource.payload == *payload {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}
fn worker_one(
    module: &TypedModule,
    id: FunctionId,
    budget: &mut Budget,
    span: zil_base::Span,
) -> CompileResult<Vec<FunctionId>> {
    let f = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| fail(span, "task worker is absent"))?;
    budget.spend(
        f.locals
            .len()
            .saturating_mul(2)
            .saturating_add(f.values.len().saturating_mul(2))
            .saturating_add(module.resources.len())
            .saturating_add(1),
        "task worker real scalar profile",
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
        return owned::profile(module, f, budget, Some(span));
    }
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.result_mode != PassingMode::Infer
        || f.parameters.len() != 1
        || f.locals
            .get(f.parameters[0].0 as usize)
            .is_none_or(|l| l.ty != Type::I32)
        || f.return_type != Type::I32
        || f.locals.iter().any(|l| !scalar(&l.ty))
        || f.values.iter().any(|t| !scalar(t))
        || module.resources.iter().any(|r| r.teardown == Some(id))
    {
        return Err(fail(
            span,
            "task worker requires a closed scalar fn(i32)->i32 without external users or cleanup",
        ));
    }
    let mut children = Vec::new();
    for b in &f.blocks {
        budget.spend(1, "task worker block")?;
        for i in &b.instructions {
            budget.spend(1, "task worker instruction")?;
            if !matches!(
                i.kind,
                InstructionKind::Constant(
                    Constant::Unit
                        | Constant::Bool(_)
                        | Constant::U8(_)
                        | Constant::U16(_)
                        | Constant::U32(_)
                        | Constant::I32(_)
                        | Constant::I8(_)
                        | Constant::I16(_)
                        | Constant::Isize(_)
                        | Constant::I64(_)
                ) | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::Unary {
                        op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
                        ..
                    }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::TaskYield
                    | InstructionKind::SuspendCall { .. }
            ) {
                return Err(fail(
                    i.span,
                    "scalar task worker contains an external, owning or unsupported operation",
                ));
            }
            if let InstructionKind::SuspendCall {
                function,
                arguments,
            } = &i.kind
            {
                budget.spend(arguments.len().saturating_add(1), "actual child inputs")?;
                if arguments.len() != 1
                    || arguments
                        .iter()
                        .any(|v| f.values.get(v.0 as usize) != Some(&Type::I32))
                {
                    return Err(fail(
                        i.span,
                        "first static child lane requires actual scalar i32 input",
                    ));
                }
                children.push(*function);
            }
        }
    }
    Ok(children)
}
fn worker(
    module: &TypedModule,
    id: FunctionId,
    budget: &mut Budget,
    span: zil_base::Span,
) -> CompileResult<()> {
    let mut stack = vec![(id, false)];
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    while let Some((current, exiting)) = stack.pop() {
        budget.spend(
            active
                .len()
                .saturating_add(complete.len())
                .saturating_add(1),
            "actual static child graph",
        )?;
        if exiting {
            active.remove(&current);
            complete.insert(current);
            continue;
        }
        if complete.contains(&current) {
            continue;
        }
        if !active.insert(current) {
            return Err(fail(
                span,
                "recursive static children require explicit BoxTask or arena",
            ));
        }
        stack.push((current, true));
        for child in worker_one(module, current, budget, span)?.into_iter().rev() {
            budget.spend(1, "static child edge")?;
            stack.push((child, false));
        }
    }
    Ok(())
}
pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if !budget.tasks {
        return Ok(());
    }
    for f in &module.functions {
        budget.spend(
            f.parameters.len().saturating_add(1),
            "Task parameter inventory",
        )?;
        let mut has_retained_input = false;
        for parameter in &f.parameters {
            let ty = &f
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| fail(f.span, "Task parameter absent"))?
                .ty;
            if retained_parameter(ty, budget)? {
                if !direct_retained_parameter(ty) {
                    return Err(fail(
                        f.span,
                        "nested retained Task parameter has no frame loan consumer",
                    ));
                }
                has_retained_input = true;
            }
        }
        if has_retained_input {
            let contract = f.storage_life.as_ref().ok_or_else(|| {
                fail(
                    f.span,
                    "retained Task input lacks complete source life contract",
                )
            })?;
            let mut inputs = Vec::new();
            for parameter in &f.parameters {
                budget.spend(1, "Task input signature")?;
                charge_result_selection(&f.locals[parameter.0 as usize].ty, budget)?;
                inputs.push(f.locals[parameter.0 as usize].ty.clone());
            }
            for ty in &inputs {
                charge_result_selection(ty, budget)?;
            }
            charge_result_selection(&f.return_type, budget)?;
            super::signature_life::charge(contract, budget)?;
            super::signature_life::function_relation(
                &inputs,
                &f.return_type,
                contract,
                false,
                &[],
            )?;
        }
    }
    budget.spend(
        module.functions.len(),
        "task suspension inventory allocation",
    )?;
    budget.spend(
        module.resources.len(),
        "task canonical definition inventory",
    )?;
    let mut task_definitions = 0;
    for r in &module.resources {
        if matches!(r.payload, Type::Task { .. }) || r.name.starts_with(TASK_RESOURCE_NAME_PREFIX) {
            budget.type_work(
                &r.payload,
                false,
                "canonical general Task definition identity",
            )?;
            if let Type::Task { result, .. } = &r.payload
                && !owned::result_type(result, budget)?
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "general owning task result lowering is not yet admitted by this frame profile",
                    None,
                )]);
            }
            require_closed_context(&r.payload)?;
            let Type::Task { .. } = &r.payload else {
                return Err(bad("general Task canonical name has another payload"));
            };
            let expected_name = format!("{TASK_RESOURCE_NAME_PREFIX}{}", r.payload.fingerprint());
            budget.spend(
                r.name.len().saturating_add(expected_name.len()),
                "task canonical name comparison",
            )?;
            if r.teardown.is_some()
                || r.name != expected_name
                || matching_resources(&module.resources, &r.payload, budget)? != 1
            {
                return Err(bad(
                    "general Task definition must be actual canonical unique sealed payload",
                ));
            }
        }
        if r.name == TASK_I32_RESOURCE_NAME || r.payload == Type::TaskI32 {
            task_definitions += 1;
            if r.name != TASK_I32_RESOURCE_NAME
                || r.payload != Type::TaskI32
                || r.teardown.is_some()
                || task_definitions != 1
            {
                return Err(bad("task definition must be canonical, unique and sealed"));
            }
        }
    }
    let mut suspend = vec![false; module.functions.len()];
    for (index, f) in module.functions.iter().enumerate() {
        if borrowed_result(&f.return_type) && f.storage_life.is_none() {
            return Err(fail(
                f.span,
                "borrowed Task return requires an actual retained input relation",
            ));
        }
        if f.call_kind == CallKind::Suspend {
            suspend[index] = true;
            worker(module, f.id, budget, f.span)?;
        }
        for block in &f.blocks {
            for i in &block.instructions {
                budget.spend(1, "task suspension inventory")?;
                let ty = |v: ValueId| {
                    f.values
                        .get(v.0 as usize)
                        .ok_or_else(|| fail(i.span, "task operand is absent"))
                };
                match i.kind {
                    InstructionKind::ReadAddress { address }
                    | InstructionKind::WriteAddress { address, .. }
                        if ty(address)?.is_exclusive_task_head_address() =>
                    {
                        return Err(fail(
                            i.span,
                            "generic address access cannot expose task owner bytes",
                        ));
                    }
                    InstructionKind::ReadDesc { desc, .. }
                    | InstructionKind::WriteDesc { desc, .. }
                        if matches!(ty(desc)?, Type::Desc(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..})) =>
                    {
                        return Err(fail(
                            i.span,
                            "generic descriptor access cannot expose sealed task frame bytes",
                        ));
                    }
                    InstructionKind::MakeManaged { .. }
                        if i.result
                            .and_then(|v| f.values.get(v.0 as usize))
                            .is_some_and(
                                |ty| matches!(ty, Type::Managed(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..})),
                            ) =>
                    {
                        return Err(fail(
                            i.span,
                            "only task_start can construct a scalar task frame",
                        ));
                    }
                    _ => {}
                }
                if matches!(
                    i.kind,
                    InstructionKind::TaskStart { .. }
                        | InstructionKind::TaskValid { .. }
                        | InstructionKind::TaskPoll { .. }
                        | InstructionKind::TaskRequestCancel { .. }
                        | InstructionKind::TaskPollClose { .. }
                        | InstructionKind::TaskFinishDestroy { .. }
                ) && task_definitions != 1
                    && !crate::resources::uses(&i.kind).any(|v| {
                        f.values
                            .get(v.0 as usize)
                            .is_some_and(|ty| matches!(task_payload(ty), Some(Type::Task { .. })))
                    })
                {
                    return Err(fail(
                        i.span,
                        "task operation requires its actual canonical resource definition",
                    ));
                }
                let expected = match i.kind {
                    InstructionKind::TaskCreate {
                        worker: id,
                        ref arguments,
                        ref port_bindings,
                    } => {
                        worker(module, id, budget, i.span)?;
                        let target = &module.functions[id.0 as usize];
                        budget.spend(
                            arguments.len().saturating_add(1),
                            "actual general task inputs",
                        )?;
                        if arguments.len() != target.parameters.len() {
                            return Err(fail(i.span, "task_create input count differs"));
                        }
                        for (v, p) in arguments.iter().zip(&target.parameters) {
                            let actual = ty(*v)?;
                            let wanted = &target.locals[p.0 as usize].ty;
                            budget.type_work(actual, false, "task initial actual input")?;
                            budget.type_work(wanted, false, "task initial signature input")?;
                            if actual != wanted {
                                return Err(fail(i.span, "task_create actual input type differs"));
                            }
                        }
                        let retained_life = owned::borrowed_shape(target, budget)?;
                        let mut expected = Vec::new();
                        for (parameter, local) in target.parameters.iter().enumerate() {
                            budget.spend(1, "task actual retained binding")?;
                            if matches!(&target.locals[local.0 as usize].ty, Type::Address { inner, .. } if owned::retained_pointee(inner))
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
                            return Err(fail(
                                i.span,
                                "task retained ports differ from actual worker inputs",
                            ));
                        }
                        for _ in 0..4 {
                            budget.type_work(
                                &target.return_type,
                                false,
                                "general task result identity",
                            )?;
                        }
                        let output = task_create_type(&target.return_type, retained_life);
                        Some(output)
                    }
                    InstructionKind::TaskStart {
                        worker: id,
                        argument,
                    } => {
                        let target = module
                            .functions
                            .get(id.0 as usize)
                            .ok_or_else(|| fail(i.span, "task worker absent"))?;
                        if target.parameters.len() != 1
                            || target.locals[target.parameters[0].0 as usize].ty != Type::I32
                            || target.return_type != Type::I32
                        {
                            return Err(fail(
                                i.span,
                                "legacy task_start requires an actual i32 worker signature",
                            ));
                        }
                        if ty(argument)? != &Type::I32 {
                            return Err(fail(i.span, "task payload must be i32"));
                        }
                        worker(module, id, budget, i.span)?;
                        require_synchronous_worker(module, id, budget, Some(i.span))?;
                        Some(Type::Managed(Box::new(Type::TaskI32)))
                    }
                    InstructionKind::SuspendCall {
                        function,
                        ref arguments,
                    } => {
                        suspend[index] = true;
                        budget
                            .spend(arguments.len().saturating_add(1), "actual child arguments")?;
                        let target = module
                            .functions
                            .get(function.0 as usize)
                            .ok_or_else(|| fail(i.span, "child absent"))?;
                        if arguments.len() != target.parameters.len() {
                            return Err(fail(i.span, "child argument count differs"));
                        }
                        for (argument, parameter) in arguments.iter().zip(&target.parameters) {
                            let actual = ty(*argument)?;
                            let wanted = &target.locals[parameter.0 as usize].ty;
                            budget.type_work(actual, false, "actual child input")?;
                            budget.type_work(wanted, false, "child signature input")?;
                            if actual != wanted {
                                return Err(fail(i.span, "child input type differs"));
                            }
                        }
                        worker(module, function, budget, i.span)?;
                        budget.type_work(&target.return_type, false, "actual child result")?;
                        Some(target.return_type.clone())
                    }
                    InstructionKind::TaskYield => {
                        suspend[index] = true;
                        worker(module, f.id, budget, i.span)?;
                        Some(Type::Unit)
                    }
                    InstructionKind::TaskValid { task } => {
                        if !matches!(ty(task)?, Type::Desc(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..}))
                        {
                            return Err(fail(
                                i.span,
                                "task validity requires its actual descriptor",
                            ));
                        }
                        Some(Type::Bool)
                    }
                    InstructionKind::TaskPoll { task }
                    | InstructionKind::TaskRequestCancel { task }
                    | InstructionKind::TaskPollClose { task } => {
                        if !ty(task)?.is_exclusive_task_head_address() {
                            return Err(fail(
                                i.span,
                                "task activation requires exclusive actual owning head",
                            ));
                        }
                        Some(match i.kind {
                            InstructionKind::TaskPoll { .. } => match task_payload(ty(task)?) {
                                Some(Type::Task { result, .. }) => {
                                    for _ in 0..4 {
                                        budget.type_work(
                                            result,
                                            false,
                                            "general task Poll identity",
                                        )?;
                                    }
                                    task_poll_result(result)
                                }
                                _ => Type::Tuple(vec![Type::I32, Type::I32]),
                            },
                            InstructionKind::TaskPollClose { .. } => {
                                if matches!(task_payload(ty(task)?), Some(Type::Task { .. })) {
                                    task_poll_type(&Type::Unit)
                                } else {
                                    Type::Bool
                                }
                            }
                            _ => Type::Unit,
                        })
                    }
                    InstructionKind::TaskFinishDestroy { task } => {
                        if !matches!(ty(task)?, Type::Managed(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..}))
                        {
                            return Err(fail(i.span, "task destruction consumes its actual owner"));
                        }
                        Some(Type::Unit)
                    }
                    _ => None,
                };
                if let Some(expected) = expected
                    && i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&expected)
                {
                    return Err(fail(
                        i.span,
                        "task instruction result differs from sealed ABI",
                    ));
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for f in &module.functions {
            for block in &f.blocks {
                for i in &block.instructions {
                    budget.spend(1, "derive actual Suspend call closure")?;
                    if let InstructionKind::Call { function, .. } = i.kind {
                        let callee = suspend
                            .get(function.0 as usize)
                            .ok_or_else(|| fail(i.span, "call target is absent"))?;
                        if *callee && !suspend[f.id.0 as usize] {
                            suspend[f.id.0 as usize] = true;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for f in &module.functions {
        if suspend[f.id.0 as usize] && f.call_kind != CallKind::Suspend {
            return Err(fail(
                f.span,
                "Direct selection excludes actual reachable suspension",
            ));
        }
        for b in &f.blocks {
            for i in &b.instructions {
                budget.spend(1, "Suspend Direct boundary exclusion")?;
                let direct = match i.kind {
                    InstructionKind::Call { function, .. }
                    | InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function } => Some(function),
                    InstructionKind::WithScopedThread { worker, .. } => Some(worker),
                    _ => None,
                };
                if direct.is_some_and(|id| suspend.get(id.0 as usize).copied().unwrap_or(false)) {
                    return Err(fail(
                        i.span,
                        "Suspend function cannot be called, referenced or started through a Direct ABI",
                    ));
                }
            }
        }
    }
    Ok(())
}

// Task head windows use actual address parent identities. Kind tags, function
// names and copied pointer bytes are not exclusive-poll authority.
pub(super) fn loans(
    module: &TypedModule,
    plan: &CandidatePlan,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<()> {
    if budget.borrowed_task_ports {
        port_origins::check(module, plan, graphs, budget)?;
        borrowed_loans::check(module, plan, graphs, budget)?;
    }
    use std::collections::BTreeMap;
    type Set = BTreeSet<usize>;
    type State = BTreeMap<LocalId, Set>;
    fn add(to: &mut Set, from: &Set, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(
            to.len().saturating_add(from.len()).saturating_add(1),
            "task loan relation union",
        )?;
        let n = to.len();
        to.extend(from);
        Ok(n != to.len())
    }
    fn copy(s: &State, budget: &mut Budget) -> CompileResult<State> {
        budget.spend(s.len(), "task head state allocation")?;
        for row in s.values() {
            budget.spend(row.len(), "task head state relations")?;
        }
        Ok(s.clone())
    }
    if !budget.tasks {
        return Ok(());
    }
    for (fi, f) in module.functions.iter().enumerate() {
        budget.spend(
            f.values.len().saturating_add(f.locals.len()),
            "task head profile inventory",
        )?;
        if !f.values.iter().any(Type::is_exclusive_task_head_address)
            && !f
                .locals
                .iter()
                .any(|l| l.ty.is_exclusive_task_head_address())
        {
            continue;
        }
        let n = f.values.len();
        let count = n.saturating_add(f.locals.len());
        budget.spend(
            count.saturating_mul(3).saturating_add(f.blocks.len()),
            "task head loan tables",
        )?;
        let mut aliases = vec![Set::new(); n];
        let mut parents = vec![Set::new(); count];
        let mut roots: Vec<Vec<HoldRoot>> = vec![vec![]; count];
        for h in &plan.functions[fi].holds {
            budget.spend(h.roots.len(), "task loan actual root snapshot")?;
            roots[h.value.0 as usize] = h.roots.clone();
        }
        let mut entry = State::new();
        for (index, local) in f.parameters.iter().enumerate() {
            if f.locals[local.0 as usize]
                .ty
                .is_exclusive_task_head_address()
            {
                budget.spend(3, "task head input loan")?;
                entry.insert(*local, Set::from([n + local.0 as usize]));
                roots[n + local.0 as usize] = vec![HoldRoot::Parameter(index as u32)];
            }
        }
        let (pred, reachable) = &graphs[fi];
        let mut outputs = vec![State::new(); f.blocks.len()];
        loop {
            let mut changed = false;
            for (b, block) in f.blocks.iter().enumerate() {
                if !reachable[b] {
                    continue;
                }
                let mut state = if b == f.entry.0 as usize {
                    copy(&entry, budget)?
                } else {
                    State::new()
                };
                for &p in &pred[b] {
                    budget.spend(outputs[p].len(), "task head predecessor state")?;
                    for (local, row) in &outputs[p] {
                        add(state.entry(*local).or_default(), row, budget)?;
                    }
                }
                for i in &block.instructions {
                    budget.spend(1, "task head alias instruction")?;
                    if let Some(output) = i.result
                        && f.values[output.0 as usize].is_exclusive_task_head_address()
                    {
                        let id = output.0 as usize;
                        let row = match i.kind {
                            InstructionKind::BorrowLocalMut { local } if matches!(&f.locals[local.0 as usize].ty, Type::Managed(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..})) => {
                                Set::from([id])
                            }
                            InstructionKind::ReborrowAddress {
                                address,
                                mutable: true,
                            } => {
                                let source = &aliases[address.0 as usize];
                                let mut ancestry = Set::new();
                                add(&mut ancestry, source, budget)?;
                                for p in source {
                                    add(&mut ancestry, &parents[*p], budget)?;
                                }
                                changed |= add(&mut parents[id], &ancestry, budget)?;
                                Set::from([id])
                            }
                            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                                let source = state.get(&local);
                                budget.spend(
                                    source.map_or(0, Set::len),
                                    "task head local alias snapshot",
                                )?;
                                source.cloned().unwrap_or_default()
                            }
                            _ => {
                                return Err(fail(
                                    i.span,
                                    "exclusive task head has no actual checked producer",
                                ));
                            }
                        };
                        changed |= add(&mut aliases[id], &row, budget)?;
                    }
                    match i.kind {
                        InstructionKind::Store { local, value }
                            if f.locals[local.0 as usize]
                                .ty
                                .is_exclusive_task_head_address() =>
                        {
                            budget.spend(
                                aliases[value.0 as usize].len().saturating_add(1),
                                "task address local assignment",
                            )?;
                            state.insert(local, aliases[value.0 as usize].clone());
                        }
                        InstructionKind::TakeLocal { local }
                        | InstructionKind::StorageLive(local)
                        | InstructionKind::StorageDead(local) => {
                            state.remove(&local);
                        }
                        _ => {}
                    }
                }
                budget.spend(
                    state.len().saturating_add(outputs[b].len()),
                    "task head state convergence",
                )?;
                if state != outputs[b] {
                    outputs[b] = state;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let afters = resources::address_afters(f, reachable, budget)?;
        for (b, block) in f.blocks.iter().enumerate() {
            if !reachable[b] {
                continue;
            }
            let mut state = if b == f.entry.0 as usize {
                copy(&entry, budget)?
            } else {
                State::new()
            };
            for &p in &pred[b] {
                budget.spend(outputs[p].len(), "task checking predecessor state")?;
                for (local, row) in &outputs[p] {
                    add(state.entry(*local).or_default(), row, budget)?;
                }
            }
            for (position, i) in block.instructions.iter().enumerate() {
                budget.spend(1, "task exclusive activation check")?;
                let (accessed, operand) = match i.kind {
                    InstructionKind::TaskPoll { task }
                    | InstructionKind::TaskRequestCancel { task }
                    | InstructionKind::TaskPollClose { task } => {
                        (Some(roots[task.0 as usize].as_slice()), Some(task))
                    }
                    InstructionKind::TaskValid { task } => {
                        (Some(roots[task.0 as usize].as_slice()), None)
                    }
                    InstructionKind::ReborrowAddress { address, .. }
                        if f.values[address.0 as usize].is_exclusive_task_head_address() =>
                    {
                        (Some(roots[address.0 as usize].as_slice()), Some(address))
                    }
                    _ => (None, None),
                };
                let local = match i.kind {
                    InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::Store { local, .. }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                        if matches!(&f.locals[local.0 as usize].ty, Type::Managed(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..})) =>
                    {
                        Some(local)
                    }
                    _ => None,
                };
                let owner_roots = local.map(|l| [HoldRoot::Local(l)]);
                let accessed = accessed.or_else(|| owner_roots.as_ref().map(|r| r.as_slice()));
                if let Some(accessed) = accessed {
                    if accessed.is_empty() {
                        return Err(fail(i.span, "task activation has no actual head root"));
                    }
                    let mut allowed = Set::new();
                    if let Some(operand) = operand {
                        add(&mut allowed, &aliases[operand.0 as usize], budget)?;
                        for loan in &aliases[operand.0 as usize] {
                            add(&mut allowed, &parents[*loan], budget)?;
                        }
                    }
                    for active in &afters[b][position] {
                        budget.spend(1, "task live exclusive head")?;
                        let held = match active {
                            resources::Place::Value(v)
                                if i.result != Some(*v)
                                    && f.values[v.0 as usize].is_exclusive_task_head_address() =>
                            {
                                Some(&aliases[v.0 as usize])
                            }
                            resources::Place::Local(l)
                                if f.locals[l.0 as usize].ty.is_exclusive_task_head_address() =>
                            {
                                state.get(l)
                            }
                            _ => None,
                        };
                        for loan in held.into_iter().flatten() {
                            budget.spend(1, "task actual loan identity")?;
                            if allowed.contains(loan) {
                                continue;
                            }
                            budget.product(
                                accessed.len(),
                                roots[*loan].len(),
                                1,
                                "task head overlap",
                            )?;
                            if accessed.iter().any(|r| roots[*loan].contains(r)) {
                                return Err(fail(
                                    i.span,
                                    "task head access overlaps another live exclusive activation",
                                ));
                            }
                        }
                    }
                }
                match i.kind {
                    InstructionKind::Store { local, value }
                        if f.locals[local.0 as usize]
                            .ty
                            .is_exclusive_task_head_address() =>
                    {
                        budget.spend(
                            aliases[value.0 as usize].len().saturating_add(1),
                            "task checking local assignment",
                        )?;
                        state.insert(local, aliases[value.0 as usize].clone());
                    }
                    InstructionKind::TakeLocal { local }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local) => {
                        state.remove(&local);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Semantic export candidates consume the same request balance as the module.
/// Neither an export label nor its parameter modes establish a frame profile.
pub(super) fn interfaces(program: &TypedProgram, budget: &mut Budget) -> CompileResult<()> {
    if program.task_interfaces.is_empty() && !budget.tasks {
        return Ok(());
    }
    let module = &program.module;
    budget.spend(program.task_interfaces.len(), "task semantic interfaces")?;
    let mut modules = BTreeSet::new();
    let mut workers = BTreeSet::new();
    for interface in &program.task_interfaces {
        budget.spend(
            modules
                .len()
                .saturating_add(interface.exports.len())
                .saturating_add(1),
            "task interface identities",
        )?;
        if !modules.insert(interface.module) {
            return Err(bad("duplicate task interface module"));
        }
        let mut names = BTreeSet::new();
        for export in &interface.exports {
            budget.spend(
                export
                    .name
                    .len()
                    .saturating_add(names.len())
                    .saturating_add(workers.len())
                    .saturating_add(1),
                "task export identity",
            )?;
            if export.abi_version != 1
                || export.parameter != (PassingMode::Infer, Type::I32)
                || export.result != (PassingMode::Infer, Type::I32)
                || export.name.is_empty()
                || !names.insert(export.name.as_str())
                || !workers.insert(export.worker)
            {
                return Err(bad(
                    "task export differs from the unique scalar SABI v1 contract",
                ));
            }
            worker(module, export.worker, budget, zil_base::Span::default())?;
            let f = &module.functions[export.worker.0 as usize];
            if f.call_kind != CallKind::Suspend {
                return Err(bad("task export must select Suspend ABI"));
            }
        }
    }
    // Existing scalar metadata is not authority for a new execution root.
    for interface in &program.interfaces {
        for export in &interface.exports {
            budget.spend(1, "scalar Suspend export exclusion")?;
            let Some(f) = module.functions.get(export.function.0 as usize) else {
                continue;
            };
            if f.call_kind == CallKind::Suspend {
                return Err(bad(
                    "Suspend worker cannot be exported as ordinary Direct scalar ABI",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod budget_tests;
