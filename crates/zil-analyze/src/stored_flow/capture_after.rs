//! Actual mutable environment state is an output of an invocation. A lifetime
//! class constrains that output; only the callee's terminal permissions select
//! the runtime target and preserve its complete parent loan chain.
use super::*;

pub(super) fn has_context(
    module: &TypedModule,
    definition: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.mutable_capture_contexts {
        return Ok(false);
    }
    let mut pending = vec![(definition, 0usize)];
    while let Some((d, depth)) = pending.pop() {
        fee(b, d.captures.len())?;
        if depth > b.partial_depth {
            return Err(failure("captured holder inventory exceeds finite depth"));
        }
        for ty in &d.captures {
            if super::super::stored_callables::mutable_context_capture(ty) {
                return Ok(true);
            }
            if let Type::Closure(id) = ty {
                pending.push((
                    module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| failure("captured holder inventory child absent"))?,
                    depth + 1,
                ));
            }
        }
    }
    Ok(false)
}

/// Schedule new mutable environment facts from the actual owning capture graph
/// or a direct pointer-capture write. A scalar mutation does not change any
/// stored target and keeps the established invocation path.
pub(super) fn required(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<bool> {
    if d.kind == CallableKind::FnOnce {
        return has_context(module, d, b);
    }
    if d.kind != CallableKind::FnMut {
        return Ok(false);
    }
    let mut pending = vec![(d, 0usize)];
    while let Some((definition, depth)) = pending.pop() {
        fee(b, definition.captures.len())?;
        if depth > b.partial_depth {
            return Err(failure("mutable environment schedule exceeds finite depth"));
        }
        for capture in &definition.captures {
            match capture {
                ty if super::super::stored_callables::mutable_context_capture(ty) => {
                    return Ok(true);
                }
                Type::BorrowedStorage { storage, .. }
                    if matches!(storage.as_ref(), Type::Package { .. }) =>
                {
                    return Ok(true);
                }
                Type::Closure(id) => pending.push((
                    module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| failure("mutable child schedule definition absent"))?,
                    depth + 1,
                )),
                _ => {}
            }
        }
        let body = module
            .functions
            .get(definition.function.0 as usize)
            .ok_or_else(|| failure("mutable environment actual body absent"))?;
        fee(b, body.blocks.len())?;
        for block in &body.blocks {
            fee(b, block.instructions.len())?;
            for instruction in &block.instructions {
                if let InstructionKind::WriteCapture { index, .. } = instruction.kind
                    && definition.captures.get(index as usize).is_some_and(|ty| {
                        matches!(
                            ty,
                            Type::Address { .. } | Type::BorrowedStorage { .. } | Type::Closure(_)
                        )
                    })
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

pub(super) fn leaves(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<Vec<(Path, bool)>> {
    let mut out = Vec::new();
    let mut pending = vec![(d, Vec::new(), 0usize)];
    while let Some((d, prefix, depth)) = pending.pop() {
        if depth > b.partial_depth {
            return Err(failure("mutable environment shape exceeds finite depth"));
        }
        fee(b, d.captures.len().saturating_add(prefix.len()))?;
        for (index, ty) in d.captures.iter().enumerate() {
            let mut path = prefix.clone();
            fee(b, path.len().saturating_add(1))?;
            path.push(PlaceProjection::Field(index as u32));
            match ty {
                Type::Address { mutable, .. } => {
                    out.push((path.clone(), *mutable));
                    if let Some((_, shape)) = pointed_carrier(ty) {
                        for slot in &shape.slots {
                            let mut nested = path.clone();
                            fee(b, nested.len().saturating_add(slot.path.len()))?;
                            nested.extend(&slot.path);
                            out.push((nested, slot.mode == StoredBorrowMode::ExclusiveAddress));
                        }
                    }
                }
                Type::BorrowedStorage { life, .. } => {
                    for slot in &life.slots {
                        let mut nested = path.clone();
                        fee(b, nested.len().saturating_add(slot.path.len()))?;
                        nested.extend(&slot.path);
                        out.push((nested, slot.mode == StoredBorrowMode::ExclusiveAddress));
                    }
                }
                Type::Closure(id) => pending.push((
                    module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| failure("mutable child definition absent"))?,
                    path,
                    depth + 1,
                )),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// A consumed frame publishes the original pointee cells, not a replacement
/// environment. Captures genuinely taken out need not be refilled.
pub(super) fn outputs(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<Vec<(Path, bool)>> {
    if d.kind != CallableKind::FnOnce {
        return leaves(module, d, b);
    }
    let mut out = Vec::new();
    let mut pending = vec![(d, Vec::new(), 0usize)];
    while let Some((d, prefix, depth)) = pending.pop() {
        fee(b, d.captures.len().saturating_add(prefix.len()))?;
        if depth > b.partial_depth {
            return Err(failure("consuming context output exceeds finite depth"));
        }
        for (index, ty) in d.captures.iter().enumerate() {
            let mut path = prefix.clone();
            path.push(PlaceProjection::Field(index as u32));
            if super::super::stored_callables::mutable_context_capture(ty) {
                let (_, shape) = pointed_carrier(ty).unwrap();
                for slot in &shape.slots {
                    fee(b, path.len().saturating_add(slot.path.len()))?;
                    let mut full = path.clone();
                    full.extend(&slot.path);
                    out.push((full, slot.mode == StoredBorrowMode::ExclusiveAddress));
                }
            } else if let Type::Closure(id) = ty {
                pending.push((
                    module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| failure("consuming context child absent"))?,
                    path,
                    depth + 1,
                ));
            }
        }
    }
    Ok(out)
}

pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    state: &State,
    actual: &mut Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    if f.environment.is_none() {
        return Ok(());
    }
    let definition = module
        .closures
        .get(f.environment.unwrap().0 as usize)
        .ok_or_else(|| failure("terminal mutable environment definition absent"))?;
    let shape = leaves(module, definition, b)?;
    for (index, ty) in definition.captures.iter().enumerate() {
        let index = index as u32;
        let value = state.captures.get(&index);
        let mut add = |tail: &Path, permission: &Permission| -> CompileResult<()> {
            let path = environment_path(index as usize, tail, b)?;
            ordered(b, actual.capture_after.len(), path.len())?;
            let row = actual.capture_after.entry(path).or_default();
            for target in &permission.targets {
                fee(
                    b,
                    target_width(target)
                        .saturating_mul(row.len().saturating_add(1))
                        .saturating_add(1),
                )?;
                if !matches!(
                    target.base,
                    HoldRoot::Capture(_) | HoldRoot::Parameter(_) | HoldRoot::Static
                ) {
                    return Err(failure(
                        "mutable environment retains an activation-local target",
                    ));
                }
                if target.retained_port.is_some() || target.heap_element.is_some() {
                    return Err(failure(
                        "mutable capture update requires explicit opaque or heap source transport",
                    ));
                }
                if let HoldRoot::Capture(source) = target.base {
                    let key = environment_path(source as usize, &target.stored, b)?;
                    fee(b, shape.len().saturating_mul(key.len().saturating_add(1)))?;
                    if !shape.iter().any(|(path, _)| *path == key) {
                        return Err(failure(
                            "mutable capture update source is not an actual external environment leaf",
                        ));
                    }
                }
                if !row.contains(target) {
                    row.push(target.clone());
                }
            }
            Ok(())
        };
        if let Some(value) = value
            && !value.pointer.targets.is_empty()
        {
            add(&Vec::new(), &value.pointer)?;
        }
        if super::super::stored_callables::mutable_context_capture(ty) {
            let slots = state
                .external
                .get(&HoldRoot::Capture(index))
                .ok_or_else(|| failure("terminal captured holder has no actual slot memory"))?;
            for (path, p) in slots {
                add(path, p)?;
            }
        } else if let Some(value) = value {
            for (path, p) in value.slots.iter().chain(&value.environment) {
                add(path, p)?;
            }
        } else if matches!(ty, Type::Closure(_))
            && definition.kind == CallableKind::FnOnce
            && let Some(rows) = state.external.get(&HoldRoot::Capture(index))
        {
            for (path, p) in rows {
                add(path, p)?;
            }
        }
    }
    Ok(())
}

// The physical row from the authenticated frame/actual external pointee.
// This is for exact entry/terminal identity proof, not target reconstruction
// from a nominal type or a declared equality class.
pub(super) fn row<'a>(
    d: &ClosureDefinition,
    state: &'a State,
    path: &Path,
    b: &mut Budget,
) -> CompileResult<Option<&'a Permission>> {
    let Some((PlaceProjection::Field(index), tail)) = path.split_first() else {
        return Err(failure(
            "capture identity row has no actual capture selector",
        ));
    };
    let ty = d
        .captures
        .get(*index as usize)
        .ok_or_else(|| failure("capture identity selects an absent field"))?;
    ordered(b, state.captures.len(), tail.len())?;
    let value = state.captures.get(index);
    if tail.is_empty()
        && let Some(value) = value
        && !value.pointer.targets.is_empty()
    {
        return Ok(Some(&value.pointer));
    }
    if super::super::stored_callables::mutable_context_capture(ty) {
        ordered(b, state.external.len(), tail.len())?;
        return Ok(state
            .external
            .get(&HoldRoot::Capture(*index))
            .and_then(|rows| rows.get(tail)));
    }
    if let Some(value) = value {
        ordered(
            b,
            value.slots.len().saturating_add(value.environment.len()),
            tail.len(),
        )?;
        return Ok(value
            .slots
            .get(tail)
            .or_else(|| value.environment.get(tail)));
    }
    ordered(b, state.external.len(), tail.len())?;
    Ok(state
        .external
        .get(&HoldRoot::Capture(*index))
        .and_then(|rows| rows.get(tail)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn invoke(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    summaries: &[Facts],
    ids: &SlotLoanIds,
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<()> {
    let Some((d, arguments)) = super::super::stored_callables::called(module, f, &i.kind) else {
        return Ok(());
    };
    if (d.storage_life.is_none() && !b.returned_borrowed_environments.contains(&d.id))
        || !required(module, d, b)?
    {
        return Ok(());
    }
    let (base, receiver) = match &i.kind {
        InstructionKind::CallClosureMut { callee, .. } => {
            let receiver = state
                .locals
                .get(callee)
                .ok_or_else(|| failure("mutable receiver actual environment absent"))?;
            (Some(HoldRoot::Local(*callee)), snapshot(receiver, b)?)
        }
        InstructionKind::CallClosureBorrowed {
            callee,
            kind: CallableKind::FnMut,
            ..
        } => {
            let actual = &values[callee.0 as usize];
            fee(b, actual.pointer.targets.len())?;
            let mut roots = actual.pointer.targets.iter();
            let head = roots
                .next()
                .ok_or_else(|| failure("mutable borrowed receiver has no actual head"))?;
            if roots.next().is_some()
                || !head.stored.is_empty()
                || !head.place.is_empty()
                || head.heap_element.is_some()
            {
                return Err(failure(
                    "mutable borrowed receiver requires one complete actual environment head",
                ));
            }
            let receiver = Value {
                environment: copy_permission_rows(
                    environment_memory::rows(state, head.base, b)?,
                    b,
                )?,
                ..Value::default()
            };
            (Some(head.base), receiver)
        }
        InstructionKind::CallClosureOnce { callee, .. } => {
            (None, snapshot(&values[callee.0 as usize], b)?)
        }
        _ => return Ok(()),
    };
    let summary = &summaries[d.function.0 as usize];
    let mut rows = BTreeMap::new();
    for (path, mutable) in outputs(module, d, b)? {
        let mut permission = Permission::default();
        if let Some(sources) = summary.capture_after.get(&path) {
            for source in sources {
                let actual = if let HoldRoot::Capture(index) = source.base {
                    let key = environment_path(index as usize, &source.stored, b)?;
                    ordered(b, receiver.environment.len(), key.len())?;
                    let mut p = Permission::default();
                    if let Some(leaf) = receiver.environment.get(&key) {
                        union(&mut p, leaf, b)?;
                    }
                    let mut targets = Targets::new();
                    for target in &p.targets {
                        fee(
                            b,
                            target_width(target)
                                .saturating_add(source.place.len())
                                .saturating_add(1),
                        )?;
                        let mut target = target.clone();
                        if source.heap_element.is_some() {
                            return Err(failure(
                                "nested mutable environment heap source requires explicit element transport",
                            ));
                        }
                        if let Some(heap) = &mut target.heap_element {
                            heap.extend(&source.place);
                        } else {
                            target.place.extend(&source.place);
                        }
                        targets.insert(target);
                    }
                    p.targets = targets;
                    p
                } else {
                    substitute(source, arguments, values, state, b)?
                };
                union(&mut permission, &actual, b)?;
            }
        }
        // An unsolved summary is bottom during finite iteration. Final
        // completeness authenticates every actual leaf before windows run.
        if !permission.targets.is_empty() {
            ordered(b, summary.capture_stable.len(), path.len())?;
            if summary.capture_stable.contains(&path) {
                ordered(b, receiver.environment.len(), path.len())?;
                let previous = receiver
                    .environment
                    .get(&path)
                    .ok_or_else(|| failure("stable capture effect omits actual previous row"))?;
                permission_clone_work(previous, b)?;
                permission_clone_work(&permission, b)?;
                if *previous != permission {
                    return Err(failure(
                        "stable capture effect changed actual targets or loans",
                    ));
                }
            } else {
                permission = retain_slot(&permission, i, u32::MAX - 1, &path, ids, loans, b)?;
            }
        }
        ordered(b, rows.len(), path.len())?;
        rows.insert(path, permission);
        b.stored_exclusive_loans |= mutable;
    }
    publish_contexts(module, d, state, &receiver.environment, &rows, b)?;
    if let Some(base) = base {
        environment_memory::publish(state, base, rows, b)?;
    }
    Ok(())
}

/// Publish through the captured address into the same authenticated storage
/// cell. Environment rows describe that cell; they do not replace its identity.
pub(super) fn publish_contexts(
    module: &TypedModule,
    definition: &ClosureDefinition,
    state: &mut State,
    before: &BTreeMap<Path, Permission>,
    after: &BTreeMap<Path, Permission>,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut pending = vec![(definition, Vec::new(), 0usize)];
    let mut updates = Vec::new();
    while let Some((d, prefix, depth)) = pending.pop() {
        fee(b, d.captures.len().saturating_add(prefix.len()))?;
        if depth > b.partial_depth {
            return Err(failure("captured holder publication exceeds finite depth"));
        }
        for (index, ty) in d.captures.iter().enumerate() {
            let mut head = prefix.clone();
            fee(b, head.len().saturating_add(1))?;
            head.push(PlaceProjection::Field(index as u32));
            if let Type::Closure(child) = ty {
                pending.push((
                    module
                        .closures
                        .get(child.0 as usize)
                        .ok_or_else(|| failure("captured holder child definition absent"))?,
                    head,
                    depth + 1,
                ));
                continue;
            }
            if !super::super::stored_callables::mutable_context_capture(ty) {
                continue;
            }
            let (_, shape) = pointed_carrier(ty)
                .ok_or_else(|| failure("captured holder has no actual qualified pointee"))?;
            // Empty predecessor/callee facts are bottom during iteration.
            // The complete actual-environment inventory below derive_cfg
            // requires every head and slot before any access window runs.
            let Some(address) = before.get(&head) else {
                continue;
            };
            if address.targets.is_empty() {
                continue;
            }
            let strong = address.targets.len() == 1;
            for slot in &shape.slots {
                let mut leaf = head.clone();
                fee(b, leaf.len().saturating_add(slot.path.len()))?;
                leaf.extend(&slot.path);
                let replacement = after
                    .get(&leaf)
                    .ok_or_else(|| failure("captured holder publication omits a physical slot"))?;
                for root in &address.targets {
                    fee(b, target_width(root).saturating_add(slot.path.len()))?;
                    if root.heap_element.is_some() || root.retained_port.is_some() {
                        return Err(failure(
                            "captured holder publication requires explicit opaque/heap context",
                        ));
                    }
                    let environment = !root.stored.is_empty();
                    let mut path = if environment {
                        root.stored.clone()
                    } else {
                        Vec::new()
                    };
                    path.extend(&root.place);
                    path.extend(&slot.path);
                    updates.push((
                        root.base,
                        path,
                        copy_permission(replacement, b)?,
                        strong,
                        environment,
                    ));
                }
            }
        }
    }
    // All sources are staged before changing a destination (including swaps).
    for (base, path, replacement, strong, environment) in updates {
        if environment {
            let mut rows = copy_permission_rows(environment_memory::rows(state, base, b)?, b)?;
            let previous = rows.get_mut(&path).ok_or_else(|| {
                failure("captured holder update has no authenticated context slot")
            })?;
            if strong {
                *previous = replacement;
            } else {
                union(previous, &replacement, b)?;
            }
            environment_memory::publish(state, base, rows, b)?;
        } else {
            let rows = actual_slot_rows_mut(state, base, b)?;
            let previous = rows.get_mut(&path).ok_or_else(|| {
                failure("captured holder update has no authenticated physical slot")
            })?;
            if strong {
                *previous = replacement;
            } else {
                union(previous, &replacement, b)?;
            }
        }
    }
    Ok(())
}

/// Generated environment cleanup visits owned responsibilities. A borrowed
/// slot in an owning Package does not by itself make teardown read its target.
/// Opaque resource cleanup is conservative unless the actual payload proves
/// that no external target is reachable.
pub(super) fn cleanup_reads_external(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    let mut todo = vec![(ty, 0usize)];
    while let Some((ty, depth)) = todo.pop() {
        fee(b, 1)?;
        if depth > b.partial_depth {
            return Err(failure("environment cleanup shape exceeds finite depth"));
        }
        match ty {
            Type::Closure(id) => {
                let d = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| failure("cleanup environment definition absent"))?;
                fee(b, d.captures.len())?;
                todo.extend(d.captures.iter().map(|ty| (ty, depth + 1)));
            }
            Type::BorrowedStorage { storage, .. } => todo.push((storage, depth + 1)),
            Type::Package { fields, .. } => {
                fee(b, fields.len())?;
                todo.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                todo.extend(fields.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    fee(b, fields.len())?;
                    todo.extend(fields.iter().map(|ty| (ty, depth + 1)));
                }
            }
            Type::ErrorUnion { error, result } => {
                todo.extend([(error.as_ref(), depth + 1), (result.as_ref(), depth + 1)])
            }
            Type::Array { element, .. } | Type::OwnedStore { element } => {
                todo.push((element, depth + 1))
            }
            Type::Managed(payload) => match payload.as_ref() {
                Type::ByteBuffer => {}
                Type::OwnedStore { element } => todo.push((element, depth + 1)),
                payload if super::super::stored_callables::resource_payload(payload, b)? => {}
                _ => return Ok(true),
            },
            Type::OwnedDyn{interface}=>{
                fee(b,module.dyn_interfaces.len())?;
                let d=module.dyn_interfaces.iter().find(|d|d.id==*interface).ok_or_else(||failure("cleanup dynamic interface absent"))?;
                super::super::dyn_cleanup::capture_interface(module,*interface,b)?;
                let cleanup=d.owning_cleanup.as_ref().ok_or_else(||failure("cleanup bound Unknown"))?;
                if !cleanup.access.reads.is_empty() || !cleanup.access.writes.is_empty(){return Ok(true);}
            }
            Type::Address { .. } | Type::Desc(_) => {}
            ty if ty.is_copy_value() => {}
            _ => return Ok(true),
        }
    }
    Ok(false)
}
