//! SABI selects an execution ABI. A Task frame and a suspension boundary are
//! separate obligations, derived from actual operations and their callees.
use super::*;

pub(super) struct Schedule {
    pub(super) frame: Vec<bool>,
    pub(super) direct_edges: Vec<(FunctionId, FunctionId)>,
    pub(super) non_suspending: BTreeSet<FunctionId>,
}

pub(super) fn schedule(module: &TypedModule, b: &mut Budget) -> Check<Schedule> {
    b.spend(module.functions.len())?;
    let mut frame = vec![false; module.functions.len()];
    let mut edges = Vec::new();
    for f in &module.functions {
        let index = f.id.0 as usize;
        if index >= frame.len() {
            return Err(bad(
                "SABI function is outside the actual directory",
                Some(f.span),
            ));
        }
        frame[index] =
            (f.foreign.is_some() || f.environment.is_some()) && f.call_kind == CallKind::Suspend;
        for block in &f.blocks {
            for instruction in &block.instructions {
                b.spend(1)?;
                match &instruction.kind {
                    InstructionKind::TaskYield | InstructionKind::SuspendCall { .. } => {
                        frame[index] = true
                    }
                    InstructionKind::Call { function, .. }
                    | InstructionKind::ForeignCall { function, .. } => {
                        edge(module, f.id, *function, &mut edges, b)?
                    }
                    InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureOnce { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureBorrowed { .. }
                    | InstructionKind::CallClosureMut { .. } => {
                        let id = closure_entry(module, f, &instruction.kind, b)?;
                        let Some(target) = id else {
                            frame[index] = true;
                            continue;
                        };
                        edge(module, f.id, target, &mut edges, b)?;
                        if matches!(instruction.kind, InstructionKind::CallClosureOnce { .. }) {
                            let InstructionKind::CallClosureOnce { callee, .. } = instruction.kind
                            else {
                                unreachable!()
                            };
                            frame[index] |= cleanup(
                                module,
                                value_type(f, callee, Some(instruction.span))?,
                                f.id,
                                &mut edges,
                                &mut BTreeSet::new(),
                                0,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::CallIndirect { callee, .. } => {
                        let mut target = None;
                        b.spend(f.blocks.len())?;
                        for block in &f.blocks {
                            b.spend(block.instructions.len())?;
                            for definition in &block.instructions {
                                if definition.result != Some(*callee) {
                                    continue;
                                }
                                if let InstructionKind::FunctionRef { function }
                                | InstructionKind::ForeignFunctionRef { function } =
                                    definition.kind
                                {
                                    if target.replace(function).is_some() {
                                        return Err(bad(
                                            "SABI indirect call has duplicated actual definitions",
                                            Some(instruction.span),
                                        ));
                                    }
                                }
                            }
                        }
                        if let Some(target) = target {
                            edge(module, f.id, target, &mut edges, b)?;
                        } else {
                            frame[index] = true;
                        }
                    }
                    InstructionKind::CallDynSlot {
                        interface, slot, ..
                    } => {
                        b.spend(
                            module
                                .dyn_interfaces
                                .len()
                                .saturating_add(module.dyn_vtables.len()),
                        )?;
                        let mut interfaces = module
                            .dyn_interfaces
                            .iter()
                            .filter(|row| row.id == *interface);
                        let iface = interfaces.next().ok_or_else(|| {
                            bad(
                                "SABI dyn dependency has no exact interface",
                                Some(instruction.span),
                            )
                        })?;
                        if interfaces.next().is_some() {
                            return Err(bad(
                                "SABI dyn dependency has duplicate interface identity",
                                Some(instruction.span),
                            ));
                        }
                        let method = iface.slots.get(*slot as usize).ok_or_else(|| {
                            bad(
                                "SABI dyn dependency has no complete slot",
                                Some(instruction.span),
                            )
                        })?;
                        if method.call_kind != CallKind::Direct {
                            frame[index] = true;
                        }
                        let mut found = false;
                        for table in module
                            .dyn_vtables
                            .iter()
                            .filter(|table| table.interface == *interface)
                        {
                            b.spend(table.entries.len())?;
                            let mut entries =
                                table.entries.iter().filter(|entry| entry.slot == *slot);
                            let entry = entries.next().ok_or_else(|| {
                                bad(
                                    "SABI dyn dependency omits an actual slot body",
                                    Some(instruction.span),
                                )
                            })?;
                            if entries.next().is_some() {
                                return Err(bad(
                                    "SABI dyn dependency repeats an actual slot body",
                                    Some(instruction.span),
                                ));
                            }
                            edge(module, f.id, entry.function, &mut edges, b)?;
                            found = true;
                        }
                        if !found {
                            frame[index] = true;
                        }
                    }
                    _ => {}
                }
                if let Some(ty) = cleanup_type(f, &instruction.kind)? {
                    frame[index] |=
                        cleanup(module, ty, f.id, &mut edges, &mut BTreeSet::new(), 0, b)?;
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for &(caller, callee) in &edges {
            b.spend(1)?;
            if frame[callee.0 as usize] && !frame[caller.0 as usize] {
                frame[caller.0 as usize] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Capture the complete actual dependency result before constructor/frame
    // admission adds entry-local obligations. This schedules the ordinary
    // memory solver; it is neither a Task frame proof nor an entry authority.
    let mut non_suspending = BTreeSet::new();
    for function in &module.functions {
        b.spend(non_suspending.len().saturating_add(1))?;
        if function.call_kind == CallKind::Suspend && !frame[function.id.0 as usize] {
            non_suspending.insert(function.id);
        }
    }
    // A constructor target needs PC0 admission even if its actual body never
    // yields. That obligation belongs to this entry, not to ordinary Direct
    // callers of the same body.
    b.spend(module.functions.len().saturating_add(b.task_workers.len()))?;
    for function in &module.functions {
        frame[function.id.0 as usize] |= b.task_workers.contains(&function.id);
    }
    Ok(Schedule {
        frame,
        direct_edges: edges,
        non_suspending,
    })
}
fn bad(message: &str, span: Option<Span>) -> Diagnostic {
    reject("E-VERIFY-SABI", span, message)
}
fn edge(
    module: &TypedModule,
    caller: FunctionId,
    target: FunctionId,
    edges: &mut Vec<(FunctionId, FunctionId)>,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?;
    let function = module
        .functions
        .get(target.0 as usize)
        .filter(|row| row.id == target)
        .ok_or_else(|| bad("SABI dependency has no exact actual function", None))?;
    if function.call_kind != CallKind::Direct {
        return Err(bad(
            "ordinary call or teardown cannot execute a Suspend ABI target",
            Some(function.span),
        ));
    }
    if edges.len() >= b.limits.max_nodes {
        return Err(limit("SABI dependency inventory exceeded"));
    }
    edges.push((caller, target));
    Ok(())
}
fn closure_entry(
    module: &TypedModule,
    f: &TypedFunction,
    kind: &InstructionKind,
    b: &mut Budget,
) -> Check<Option<FunctionId>> {
    let ty = match *kind {
        InstructionKind::CallClosure { callee, .. }
        | InstructionKind::CallClosureOnce { callee, .. } => value_type(f, callee, None)?,
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => local_type(f, callee, None)?,
        InstructionKind::CallClosureBorrowed { callee, .. } => {
            let Type::Address { inner, .. } = value_type(f, callee, None)? else {
                return Ok(None);
            };
            inner
        }
        _ => return Ok(None),
    };
    let mut ty = ty;
    while let Type::BorrowedStorage { storage, .. } = ty {
        ty = storage;
    }
    let Type::Closure(id) = ty else {
        return Ok(None);
    };
    b.spend(module.closures.len())?;
    let mut rows = module.closures.iter().filter(|row| row.id == *id);
    let closure = rows
        .next()
        .ok_or_else(|| bad("SABI closure dependency has no exact definition", None))?;
    if rows.next().is_some() {
        return Err(bad("SABI closure dependency repeats its identity", None));
    }
    let entry = module
        .functions
        .get(closure.function.0 as usize)
        .ok_or_else(|| bad("SABI closure dependency entry is absent", None))?;
    if entry.id != closure.function || entry.environment != Some(*id) {
        return Err(bad(
            "SABI closure dependency changes its actual entry adapter",
            Some(entry.span),
        ));
    }
    Ok(Some(entry.id))
}
fn cleanup_type<'a>(f: &'a TypedFunction, kind: &InstructionKind) -> Check<Option<&'a Type>> {
    let local = match *kind {
        InstructionKind::DropManaged { local }
        | InstructionKind::DropClosure { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropTuple { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::DropDynOwned { local } => Some(local),
        _ => None,
    };
    if let Some(local) = local {
        return local_type(f, local, None).map(Some);
    }
    match *kind {
        InstructionKind::DropPackageField { local, index } => {
            let ty = local_type(f, local, None)?;
            let ty = if let Type::BorrowedStorage { storage, .. } = ty {
                storage.as_ref()
            } else {
                ty
            };
            let Type::Package { fields, .. } = ty else {
                return Err(bad(
                    "SABI field cleanup has another actual owner kind",
                    None,
                ));
            };
            Ok(Some(
                &fields
                    .get(index as usize)
                    .ok_or_else(|| bad("SABI field cleanup selects a missing field", None))?
                    .1,
            ))
        }
        InstructionKind::DropOwnedArrayElement { local, .. } => {
            let ty = local_type(f, local, None)?;
            let ty = if let Type::BorrowedStorage { storage, .. } = ty {
                storage.as_ref()
            } else {
                ty
            };
            let Type::Array { element, .. } = ty else {
                return Err(bad(
                    "SABI element cleanup has another actual owner kind",
                    None,
                ));
            };
            Ok(Some(element))
        }
        _ => Ok(None),
    }
}
fn cleanup(
    module: &TypedModule,
    ty: &Type,
    caller: FunctionId,
    edges: &mut Vec<(FunctionId, FunctionId)>,
    active: &mut BTreeSet<ClosureId>,
    depth: usize,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("SABI cleanup dependency depth exceeded"));
    }
    Ok(match ty {
        Type::BorrowedStorage { storage, .. } => {
            cleanup(module, storage, caller, edges, active, depth + 1, b)?
        }
        Type::Managed(payload) => match payload.as_ref() {
            Type::Task { .. } | Type::AsyncRegistration { .. } => true,
            _ => {
                b.spend(module.resources.len())?;
                for resource in &module.resources {
                    resources::charge_payload(&resource.payload, b)?;
                    resources::charge_payload(payload, b)?;
                    if resource.payload == **payload {
                        if let Some(drop) = resource.teardown {
                            edge(module, caller, drop, edges, b)?;
                        }
                    }
                }
                false
            }
        },
        Type::Closure(id) => {
            if !active.insert(*id) {
                return Ok(true);
            }
            b.spend(module.closures.len())?;
            let mut rows = module.closures.iter().filter(|row| row.id == *id);
            let closure = rows
                .next()
                .ok_or_else(|| bad("SABI cleanup has no complete closure definition", None))?;
            if rows.next().is_some() {
                return Err(bad("SABI cleanup repeats a closure definition", None));
            }
            let mut result = false;
            for capture in &closure.captures {
                result |= cleanup(module, capture, caller, edges, active, depth + 1, b)?;
            }
            active.remove(id);
            result
        }
        Type::Tuple(fields) => {
            let mut result = false;
            for field in fields {
                result |= cleanup(module, field, caller, edges, active, depth + 1, b)?;
            }
            result
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            let mut result = false;
            for (_, field) in fields {
                result |= cleanup(module, field, caller, edges, active, depth + 1, b)?;
            }
            result
        }
        Type::Enum { variants, .. } => {
            let mut result = false;
            for (_, fields) in variants {
                for field in fields {
                    result |= cleanup(module, field, caller, edges, active, depth + 1, b)?;
                }
            }
            result
        }
        Type::ErrorUnion { error, result } => {
            cleanup(module, error, caller, edges, active, depth + 1, b)?
                | cleanup(module, result, caller, edges, active, depth + 1, b)?
        }
        Type::Array { element, .. } => {
            cleanup(module, element, caller, edges, active, depth + 1, b)?
        }
        Type::OwnedDyn { interface } => {
            b.spend(module.dyn_vtables.len())?;
            let mut found = false;
            for table in module
                .dyn_vtables
                .iter()
                .filter(|table| table.interface == *interface)
            {
                if let Some(entry) = &table.cleanup {
                    edge(module, caller, entry.function, edges, b)?;
                } else {
                    return Ok(true);
                }
                found = true;
            }
            !found
        }
        _ => false,
    })
}
