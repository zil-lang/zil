//! Independent scalar task ABI admission. No producer call-mode flag is trusted.
use super::*;
mod owned;

fn empty_context() -> zil_ir::RetainedLifeShape {
    zil_ir::RetainedLifeShape {
        binder_count: 0,
        ports: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}
pub(super) fn require_closed_context(ty: &Type, span: Option<Span>) -> Check<()> {
    if let Type::Task { retained_life, .. } = ty
        && (retained_life.binder_count != 0
            || !retained_life.ports.is_empty()
            || !retained_life.retained.is_empty()
            || !retained_life.nominal_arguments.is_empty())
    {
        return Err(unsupported(
            "borrowed Task retained classes require actual implemented frame port mappings",
            span,
        ));
    }
    Ok(())
}

fn task_payload(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Managed(t) | Type::Desc(t) | Type::Address { inner: t, .. } => task_payload(t),
        Type::TaskI32 | Type::Task { .. } => Some(ty),
        _ => None,
    }
}
fn task_create_type(result: &Type) -> Type {
    Type::ErrorUnion {
        error: Box::new(Type::Enum {
            name: "@std.task:v1:TaskCreateError".into(),
            variants: vec![("Allocation".into(), vec![])],
        }),
        result: Box::new(Type::Managed(Box::new(Type::Task {
            result: Box::new(result.clone()),
            retained_life: empty_context(),
        }))),
    }
}
fn task_poll_type(result: &Type) -> Type {
    Type::Enum {
        name: format!("@std.task:v1:Poll:{}", result.fingerprint()),
        variants: vec![
            ("Pending".into(), vec![]),
            ("Ready".into(), vec![result.clone()]),
        ],
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
            for _ in 0..4 {
                resources::charge_payload(&target.return_type, budget)?;
            }
            budget.spend(budget.task_workers.len().saturating_add(1))?;
            budget.task_workers.insert(worker);
            let ty = task_create_type(&target.return_type);
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
