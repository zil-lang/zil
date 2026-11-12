//! Actual function ownership for the finite stored/ordinary scalar seam.
use super::*;

fn contains_stored(root: &Type, b: &mut Budget) -> CompileResult<bool> {
    b.type_work(root, false, "composition type boundary")?;
    let mut pending = vec![root];
    while let Some(ty) = pending.pop() {
        b.spend(1, "composition type node")?;
        match ty {
            Type::BorrowedStorage { .. } => return Ok(true),
            Type::OwnedStore { element }
            | Type::Registration {
                environment: element,
            }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::Task {
                result: element, ..
            }
            | Type::Array { element, .. }
            | Type::Slice { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::Opaque {
                hidden: element, ..
            } => pending.push(element),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| ty));
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
            Type::ForeignCallback { parameters, result }
            | Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub(super) fn partition(module: &TypedModule, b: &mut Budget) -> CompileResult<Vec<bool>> {
    b.spend(module.functions.len(), "composition function ownership")?;
    let mut stored = vec![false; module.functions.len()];
    for (index, f) in module.functions.iter().enumerate() {
        b.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(1),
            "composition actual function types",
        )?;
        let has_stored_type = contains_stored(&f.return_type, b)?
            || f.locals.iter().try_fold(false, |found, local| {
                Ok::<_, Vec<Diagnostic>>(found | contains_stored(&local.ty, b)?)
            })?
            || f.values.iter().try_fold(false, |found, ty| {
                Ok::<_, Vec<Diagnostic>>(found | contains_stored(ty, b)?)
            })?;
        let mut has_stored_op = false;
        for block in &f.blocks {
            b.spend(
                block.instructions.len(),
                "composition actual instruction inventory",
            )?;
            has_stored_op |= block.instructions.iter().any(|i| {
                matches!(
                    i.kind,
                    InstructionKind::LoadStoredBorrow { .. }
                        | InstructionKind::StoreStoredBorrow { .. }
                )
            });
        }
        let rich_stored = f.storage_life.is_some()
            && f.call_kind != CallKind::Suspend
            && !super::tasks::borrowed_signature(f, b)?;
        stored[index] = has_stored_type || has_stored_op || rich_stored;
    }
    for closure in &module.closures {
        b.spend(
            closure.captures.len().saturating_add(1),
            "composition actual closure entry",
        )?;
        let member = closure.storage_life.is_some()
            || closure.captures.iter().try_fold(false, |found, ty| {
                Ok::<_, Vec<Diagnostic>>(found | contains_stored(ty, b)?)
            })?;
        let entry = *stored
            .get(closure.function.0 as usize)
            .ok_or_else(|| bad("composition closure entry absent"))?;
        if member || entry {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "stored/ordinary composition does not transfer stored callback environments",
                None,
            )]);
        }
    }
    for (index, f) in module.functions.iter().enumerate() {
        for block in &f.blocks {
            b.spend(block.instructions.len(), "composition direct call edge")?;
            for i in &block.instructions {
                if stored[index]
                    && matches!(
                        i.kind,
                        InstructionKind::CallIndirect { .. }
                            | InstructionKind::CallClosure { .. }
                            | InstructionKind::CallClosureShared { .. }
                            | InstructionKind::CallClosureBorrowed { .. }
                            | InstructionKind::CallClosureOnce { .. }
                            | InstructionKind::CallClosureMut { .. }
                            | InstructionKind::MakeDynBorrow { .. }
                            | InstructionKind::CallDynSlot { .. }
                    )
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "stored/ordinary composition requires direct stored-domain calls",
                        Some(i.span),
                    )]);
                }
                let indirect_targets = match &i.kind {
                    InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function }
                    | InstructionKind::SuspendCall { function, .. }
                    | InstructionKind::ForeignCall { function, .. }
                    | InstructionKind::TaskStart {
                        worker: function, ..
                    }
                    | InstructionKind::WithScopedThread {
                        worker: function, ..
                    }
                    | InstructionKind::TaskCreate {
                        worker: function, ..
                    } | InstructionKind::TaskCreateOwned {
                        worker: function, ..
                    } => [Some(*function), None, None],
                    InstructionKind::WithForeignCallback {
                        register, close, ..
                    }
                    | InstructionKind::RegistrationStart {
                        register, close, ..
                    } => [Some(*register), Some(*close), None],
                    InstructionKind::AsyncRegistrationStart {
                        register,
                        request_close,
                        poll_drain,
                        ..
                    } => [Some(*register), Some(*request_close), Some(*poll_drain)],
                    InstructionKind::MakeClosure { closure, .. } => [
                        Some(
                            module
                                .closures
                                .get(closure.0 as usize)
                                .ok_or_else(|| bad("composition closure definition absent"))?
                                .function,
                        ),
                        None,
                        None,
                    ],
                    _ => [None, None, None],
                };
                for target in indirect_targets.into_iter().flatten() {
                    let member = stored
                        .get(target.0 as usize)
                        .ok_or_else(|| bad("composition referenced function absent"))?;
                    if stored[index] || *member != stored[index] {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "stored/ordinary composition cannot put an indirect function edge in the stored domain or across domains",
                            Some(i.span),
                        )]);
                    }
                }
                let InstructionKind::Call {
                    function,
                    arguments,
                } = &i.kind
                else {
                    continue;
                };
                let target = module
                    .functions
                    .get(function.0 as usize)
                    .ok_or_else(|| bad("composition call target absent"))?;
                if stored[index] == stored[function.0 as usize] {
                    continue;
                }
                b.spend(
                    arguments
                        .len()
                        .saturating_add(target.parameters.len())
                        .saturating_add(1),
                    "composition scalar seam",
                )?;
                let scalar = target.call_kind == CallKind::Direct
                    && target.environment.is_none()
                    && target.foreign.is_none()
                    && target.storage_life.is_none()
                    && target.callable_contract.is_none()
                    && target.callable_return_contract.is_none()
                    && target.reference_return_bound.is_none()
                    && target.protocol_contract.is_none()
                    && target.parameters.len() == 1
                    && arguments.len() == 1
                    && target.return_type == Type::I32
                    && target
                        .locals
                        .get(target.parameters[0].0 as usize)
                        .is_some_and(|l| l.ty == Type::I32)
                    && f.values.get(arguments[0].0 as usize) == Some(&Type::I32)
                    && i.result.and_then(|v| f.values.get(v.0 as usize)) == Some(&Type::I32);
                if !scalar {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "stored/ordinary composition requires an actual direct i32 -> i32 boundary",
                        Some(i.span),
                    )]);
                }
            }
        }
    }
    b.spend(
        module.dyn_vtables.len(),
        "composition dynamic definition inventory",
    )?;
    for table in &module.dyn_vtables {
        b.spend(
            table.entries.len().saturating_add(1),
            "composition dynamic entry edges",
        )?;
        for entry in table.entries.iter().chain(table.cleanup.iter()) {
            if *stored
                .get(entry.function.0 as usize)
                .ok_or_else(|| bad("composition dynamic entry absent"))?
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "stored/ordinary composition cannot transfer a dynamic entry",
                    None,
                )]);
            }
        }
    }
    for resource in &module.resources {
        b.spend(1, "composition teardown edge")?;
        if contains_stored(&resource.payload, b)? {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "stored/ordinary composition cannot carry a stored resource payload",
                None,
            )]);
        }
        if let Some(id) = resource.teardown
            && *stored
                .get(id.0 as usize)
                .ok_or_else(|| bad("composition resource teardown absent"))?
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "stored/ordinary composition cannot transfer a resource teardown",
                None,
            )]);
        }
    }
    Ok(stored)
}
