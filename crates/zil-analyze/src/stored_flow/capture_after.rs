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
            if !matches!(ty, Type::Address { .. }) {
                for (id, _) in environment_roots(ty, b)? {
                    pending.push((&module.closures[id.0 as usize], depth + 1));
                }
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
                ty if !matches!(ty, Type::Address { .. }) => {
                    for (id, _) in environment_roots(ty, b)? {
                        pending.push((&module.closures[id.0 as usize], depth + 1));
                    }
                }
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
    effect_leaves(module, d, false, true, b)
}
/// Capture fields saved inline in the frame. Children beyond an actual
/// address belong to the current target cell, not to this frame's SSA value.
pub(super) fn frame_leaves(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<Vec<(Path, bool)>> {
    effect_leaves(module, d, false, false, b)
}
/// A retired inline frame publishes external cells, while an F stored behind
/// a captured address is a persistent external frame and publishes its heads.
pub(super) fn outputs(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> CompileResult<Vec<(Path, bool)>> {
    effect_leaves(module, d, d.kind == CallableKind::FnOnce, true, b)
}
fn effect_leaves(
    module: &TypedModule,
    d: &ClosureDefinition,
    retired: bool,
    referents: bool,
    b: &mut Budget,
) -> CompileResult<Vec<(Path, bool)>> {
    let mut out = Vec::new();
    let mut pending = vec![(d, Vec::new(), retired, 0usize)];
    while let Some((d, prefix, retired, depth)) = pending.pop() {
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "environment output inventory exceeds finite typed depth",
                None,
            )]);
        }
        for (index, ty) in d.captures.iter().enumerate() {
            fee(b, prefix.len().saturating_add(1))?;
            let mut head = prefix.clone();
            head.push(PlaceProjection::Field(index as u32));
            if let Type::Address { mutable, inner } = ty {
                if !retired {
                    out.push((head.clone(), *mutable));
                }
                if referents && pointee_memory::context(inner) && (!retired || *mutable) {
                    for (tail, mode) in pointee_memory::rows(inner, b)? {
                        fee(b, head.len().saturating_add(tail.len()))?;
                        let mut path = head.clone();
                        path.extend(tail);
                        out.push((path, mode == StoredBorrowMode::ExclusiveAddress));
                    }
                    for (child, relative) in environment_roots(inner, b)? {
                        fee(b, head.len().saturating_add(relative.len()))?;
                        let mut path = head.clone();
                        path.extend(relative);
                        pending.push((&module.closures[child.0 as usize], path, false, depth + 1));
                    }
                }
            } else {
                if !retired {
                    for (tail, mode) in pointee_memory::rows(ty, b)? {
                        fee(b, head.len().saturating_add(tail.len()))?;
                        let mut path = head.clone();
                        path.extend(tail);
                        out.push((path, mode == StoredBorrowMode::ExclusiveAddress));
                    }
                }
                for (child, relative) in environment_roots(ty, b)? {
                    fee(b, head.len().saturating_add(relative.len()))?;
                    let mut path = head.clone();
                    path.extend(relative);
                    pending.push((&module.closures[child.0 as usize], path, retired, depth + 1));
                }
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
        let mut auxiliary = None;
        let mut referent = None;
        let mut add = |tail: &Path, permission: &Permission, b: &mut Budget| -> CompileResult<()> {
            let path = environment_path(index as usize, tail, b)?;
            ordered(b, actual.capture_after.environment.len(), path.len())?;
            let row = &mut actual
                .capture_after
                .environment
                .entry(path)
                .or_default()
                .targets;
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
                    row.insert(target.clone());
                }
            }
            Ok(())
        };
        if let Some(value) = value
            && !value.pointer.targets.is_empty()
        {
            add(&Vec::new(), &value.pointer, b)?;
        }
        if super::super::stored_callables::mutable_context_capture(ty) {
            let slots = state
                .external
                .get(&HoldRoot::Capture(index))
                .ok_or_else(|| failure("terminal captured holder has no actual slot memory"))?;
            // The pointer field and its current referent have separate roots.
            // Only the real saved-head path keys this complete effect Value.
            referent = Some(pointee_memory::summary(slots, b)?);
            if let Some(value) = value {
                auxiliary = Some(pointee_memory::summary(value, b)?);
            }
        } else if let Some(value) = value {
            let value = state
                .external
                .get(&HoldRoot::Capture(index))
                .filter(|_| !value.saved_heads.is_empty())
                .unwrap_or(value);
            for (path, p) in value.slots.iter().chain(&value.environment) {
                add(path, p, b)?;
            }
            auxiliary = Some(pointee_memory::summary(value, b)?);
        } else if matches!(ty, Type::Closure(_))
            && definition.kind == CallableKind::FnOnce
            && let Some(rows) = state.external.get(&HoldRoot::Capture(index))
        {
            for (path, p) in &rows.environment {
                add(path, p, b)?;
            }
            auxiliary = Some(pointee_memory::summary(rows, b)?);
        }
        drop(add);
        if let Some(payload) = referent {
            for source in pointee_memory::source_targets(&payload) {
                fee(b, target_width(source).saturating_add(1))?;
                if !matches!(
                    source.base,
                    HoldRoot::Capture(_) | HoldRoot::Parameter(_) | HoldRoot::Static
                ) {
                    return Err(failure(
                        "captured referent retains activation-local storage",
                    ));
                }
            }
            let head = vec![PlaceProjection::Field(index)];
            ordered(b, actual.capture_after.pointees.len(), head.len())?;
            pointee_memory::add_pointee_effect(&mut actual.capture_after, head, payload, b)?;
        }
        if let Some(payload) = auxiliary {
            for source in pointee_memory::source_targets(&payload) {
                fee(b, target_width(source).saturating_add(1))?;
                if !matches!(
                    source.base,
                    HoldRoot::Capture(_) | HoldRoot::Parameter(_) | HoldRoot::Static
                ) {
                    return Err(failure(
                        "captured pointee summary retains an activation-local auxiliary source",
                    ));
                }
            }
            let prefix = [PlaceProjection::Field(index)];
            pointee_memory::graft_capture_aux(
                &mut actual.capture_after,
                index,
                &payload,
                false,
                b,
            )?;
            actual.capture_after.enum_variants.replace(
                &prefix,
                &payload.enum_variants,
                false,
                b,
            )?;
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
        return state
            .external
            .get(&HoldRoot::Capture(*index))
            .map(|value| effect_row(value, tail, b))
            .transpose()
            .map(Option::flatten);
    }
    if matches!(ty, Type::Closure(_))
        && let Some(current) = state.external.get(&HoldRoot::Capture(*index))
    {
        return effect_row(current, tail, b);
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
    if let Some(current) = state.external.get(&HoldRoot::Capture(*index)) {
        effect_row(current, tail, b)
    } else {
        Ok(None)
    }
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
    let (location, receiver) = match &i.kind {
        InstructionKind::CallClosureMut { callee, .. } => {
            let receiver = state
                .locals
                .get(callee)
                .ok_or_else(|| failure("mutable receiver actual environment absent"))?;
            (
                Some(Target {
                    base: local_capture_base(state, *callee, b)?,
                    stored: Vec::new(),
                    place: Vec::new(),
                    retained_port: None,
                    heap_element: None,
                    heap_parents: Vec::new(),
                }),
                snapshot(receiver, b)?,
            )
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
                || head.heap_element.is_some()
                || !head.heap_parents.is_empty()
                || head.retained_port.is_some()
            {
                return Err(failure(
                    "mutable borrowed receiver requires one complete actual environment head",
                ));
            }
            let receiver = current_receiver(actual, state, b)?;
            fee(b, target_width(head))?;
            (Some(head.clone()), receiver)
        }
        InstructionKind::CallClosureOnce { callee, .. } => {
            (None, snapshot(&values[callee.0 as usize], b)?)
        }
        _ => return Ok(()),
    };
    let summary = &summaries[d.function.0 as usize];
    if !summary.complete
        && summary.capture_after.environment.is_empty()
        && summary.capture_after.pointees.is_empty()
    {
        return Ok(());
    }
    let mut after = pointee_memory::transport(&summary.capture_after, b, |target, b| {
        source(target, &receiver, arguments, values, state, b)
    })?;
    pointee_memory::substitute_variants(
        &mut after,
        &summary.capture_after,
        arguments,
        values,
        state,
        Some(&receiver),
        b,
    )?;
    let mut rows = BTreeMap::new();
    for (path, mutable) in outputs(module, d, b)? {
        let mut permission = Permission::default();
        if !slot_active(&after, &path) {
            continue;
        }
        if let Some(sources) = effect_row(&summary.capture_after, &path, b)? {
            for source in &sources.targets {
                let actual = self::source(source, &receiver, arguments, values, state, b)?;
                union(&mut permission, &actual, b)?;
            }
        }
        // An unsolved summary is bottom during finite iteration. Final
        // completeness authenticates every actual leaf before windows run.
        if !permission.targets.is_empty() {
            ordered(b, summary.capture_stable.len(), path.len())?;
            if summary.capture_stable.contains(&path) {
                ordered(b, receiver.environment.len(), path.len())?;
                let previous = environment_memory::select_rows_current(
                    &receiver.environment,
                    state,
                    &path,
                    b,
                )?;
                permission_clone_work(&previous, b)?;
                permission_clone_work(&permission, b)?;
                if previous != permission {
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
    for (path, p) in rows {
        set_effect_row(&mut after, &path, p, b)?;
    }
    pointee_memory::recompute_held(&mut after, b)?;
    publish_contexts(module, d, state, &receiver.environment, &after, b)?;
    if let Some(location) = location {
        // Publish frame fields through the same complete Target resolver as
        // all other current memory. Pointer-value metadata is never the cell.
        after.pointees.clear();
        after.saved_heads = pointee_memory::frame_heads(module, d, b)?;
        let address = Permission {
            targets: Targets::from([location]),
            loans: Loans::new(),
        };
        pointee_memory::write(state, &address, &after, &Type::Closure(d.id), true, b)?;
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
    after: &Value,
    b: &mut Budget,
) -> CompileResult<()> {
    let updates = prepare_contexts(module, definition, state, before, after, b)?;
    pointee_memory::commit_updates(state, updates, b)
}
pub(super) fn prepare_contexts<'a>(
    module: &'a TypedModule,
    definition: &'a ClosureDefinition,
    state: &State,
    before: &BTreeMap<Path, Permission>,
    after: &Value,
    b: &mut Budget,
) -> CompileResult<Vec<(Permission, Value, &'a Type)>> {
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
            let children = if let Type::Address { inner, .. } = ty {
                environment_roots(inner, b)?
            } else {
                environment_roots(ty, b)?
            };
            for (child, relative) in children {
                fee(b, head.len().saturating_add(relative.len()))?;
                let mut path = head.clone();
                path.extend(relative);
                pending.push((&module.closures[child.0 as usize], path, depth + 1));
            }
            if !super::super::stored_callables::mutable_context_capture(ty) {
                continue;
            }
            let Type::Address { inner, .. } = ty else {
                unreachable!()
            };
            let address = environment_memory::select_rows_current(before, state, &head, b)?;
            if address.targets.is_empty() {
                continue;
            }
            let Some(value) = after.pointees.get(&head) else {
                continue;
            };
            // A missing tag tree is an unresolved effect, not proof
            // that all of its payload rows are inactive.
            if pointee_memory::complete(value, inner, b)? {
                updates.push((address, snapshot(value, b)?, inner.as_ref()));
            }
        }
    }
    Ok(updates)
}

type CheckedCells = BTreeSet<(Target, usize)>;
pub(super) fn check_current(
    module: &TypedModule,
    d: &ClosureDefinition,
    receiver: &Value,
    state: &State,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut seen = CheckedCells::new();
    if receiver.pointer.targets.is_empty() {
        check_frame(module, d, receiver, state, &mut seen, b)
    } else {
        for root in &receiver.pointer.targets {
            fee(b, target_width(root).saturating_add(1))?;
            let address = Permission {
                targets: Targets::from([root.clone()]),
                loans: Loans::new(),
            };
            let frame = pointee_memory::read(state, &address, None, true, b)?;
            check_frame(module, d, &frame, state, &mut seen, b)?;
        }
        Ok(())
    }
}
pub(super) fn check_value(
    module: &TypedModule,
    ty: &Type,
    value: &Value,
    state: &State,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut seen = CheckedCells::new();
    if let Type::Address { inner, .. } = ty {
        if pointee_memory::aggregate(inner) || !environment_roots(inner, b)?.is_empty() {
            check_address(module, inner, &value.pointer, state, &mut seen, b)?;
        }
    } else {
        check_inline(module, ty, value, state, &mut seen, b)?;
    }
    Ok(())
}
fn check_inline(
    module: &TypedModule,
    ty: &Type,
    value: &Value,
    state: &State,
    seen: &mut CheckedCells,
    b: &mut Budget,
) -> CompileResult<()> {
    for (id, path) in environment_roots(ty, b)? {
        if !value.enum_variants.slot_active(&path) {
            continue;
        }
        let frame = pointee_memory::project(value, &path, None, b)?;
        check_frame(
            module,
            &module.closures[id.0 as usize],
            &frame,
            state,
            seen,
            b,
        )?;
    }
    Ok(())
}
fn check_address(
    module: &TypedModule,
    ty: &Type,
    pointer: &Permission,
    state: &State,
    seen: &mut CheckedCells,
    b: &mut Budget,
) -> CompileResult<()> {
    if pointer.targets.is_empty() {
        return Err(failure(
            "callable aggregate query has no actual saved pointer",
        ));
    }
    for target in &pointer.targets {
        fee(b, target_width(target).saturating_add(1))?;
        let key = (target.clone(), ty as *const Type as usize);
        ordered(b, seen.len(), target_width(target))?;
        if !seen.insert(key) {
            continue;
        }
        let address = Permission {
            targets: Targets::from([target.clone()]),
            loans: Loans::new(),
        };
        let value = pointee_memory::read(state, &address, Some(ty), true, b)?;
        check_inline(module, ty, &value, state, seen, b)?;
    }
    Ok(())
}
fn check_frame(
    module: &TypedModule,
    d: &ClosureDefinition,
    value: &Value,
    state: &State,
    seen: &mut CheckedCells,
    b: &mut Budget,
) -> CompileResult<()> {
    let current = environment_memory::current(value, state, b)?;
    for (index, ty) in d.captures.iter().enumerate() {
        fee(b, 1)?;
        let head = [PlaceProjection::Field(index as u32)];
        if let Type::Address { inner, .. } = ty {
            if pointee_memory::aggregate(inner) || !environment_roots(inner, b)?.is_empty() {
                let pointer = current
                    .get(head.as_slice())
                    .ok_or_else(|| failure("captured aggregate omits its actual saved head"))?;
                check_address(module, inner, pointer, state, seen, b)?;
            }
        } else {
            let child = pointee_memory::project(value, &head, None, b)?;
            check_inline(module, ty, &child, state, seen, b)?;
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
            Type::OwnedDyn { interface } => {
                fee(b, module.dyn_interfaces.len())?;
                let d = module
                    .dyn_interfaces
                    .iter()
                    .find(|d| d.id == *interface)
                    .ok_or_else(|| failure("cleanup dynamic interface absent"))?;
                super::super::dyn_cleanup::capture_interface(module, *interface, b)?;
                let cleanup = d
                    .owning_cleanup
                    .as_ref()
                    .ok_or_else(|| failure("cleanup bound Unknown"))?;
                if !cleanup.access.reads.is_empty() || !cleanup.access.writes.is_empty() {
                    return Ok(true);
                }
            }
            Type::Address { .. } | Type::Desc(_) => {}
            ty if ty.is_copy_value() => {}
            _ => return Ok(true),
        }
    }
    Ok(false)
}

pub(super) fn current_receiver(
    actual: &Value,
    state: &State,
    b: &mut Budget,
) -> CompileResult<Value> {
    if actual.pointer.targets.is_empty() {
        snapshot(actual, b)
    } else {
        pointee_memory::read(state, &actual.pointer, None, false, b)
    }
}
fn source(
    source: &Target,
    receiver: &Value,
    arguments: &[ValueId],
    values: &[Value],
    state: &State,
    b: &mut Budget,
) -> CompileResult<Permission> {
    let HoldRoot::Capture(index) = source.base else {
        return substitute(source, arguments, values, state, b);
    };
    let key = environment_path(index as usize, &source.stored, b)?;
    let mut p = environment_memory::select_rows_current(&receiver.environment, state, &key, b)?;
    let mut targets = Targets::new();
    for target in &p.targets {
        fee(
            b,
            target_width(target)
                .saturating_add(source.place.len())
                .saturating_add(1),
        )?;
        let mut target = target.clone();
        if source.heap_element.is_some() || !source.heap_parents.is_empty() {
            return Err(failure(
                "captured heap source requires explicit complete element transport",
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
    Ok(p)
}
pub(super) fn complete_terminal(
    module: &TypedModule,
    d: &ClosureDefinition,
    state: &State,
    b: &mut Budget,
) -> CompileResult<bool> {
    let mut complete = true;
    for (index, ty) in d.captures.iter().enumerate() {
        fee(b, 1)?;
        if let Type::Address { inner, .. } = ty
            && pointee_memory::aggregate(inner)
        {
            if let Some(value) = state.external.get(&HoldRoot::Capture(index as u32)) {
                complete &= pointee_memory::complete(value, inner, b)?;
            } else {
                complete = false;
            }
        } else if let Type::Closure(id) = ty {
            if let Some(value) = state.external.get(&HoldRoot::Capture(index as u32)) {
                complete &= complete_frame(module, &module.closures[id.0 as usize], value, b)?;
            } else {
                complete = false;
            }
        }
    }
    Ok(complete)
}

/// One namespace transition: exact head identity followed by a physical leaf.
/// A head's own permission is always its frame environment row.
fn effect_head<'a>(
    value: &'a Value,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Option<&'a Path>> {
    let mut selected: Option<&Path> = None;
    for head in value.pointees.keys() {
        fee(b, head.len().saturating_add(path.len()).saturating_add(1))?;
        if head.len() < path.len()
            && path.starts_with(head)
            && selected.is_none_or(|old| old.len() < head.len())
        {
            selected = Some(head);
        }
    }
    Ok(selected)
}
pub(super) fn effect_row<'a>(
    value: &'a Value,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Option<&'a Permission>> {
    if let Some(head) = effect_head(value, path, b)? {
        let cell = &value.pointees[head];
        let tail = &path[head.len()..];
        ordered(
            b,
            cell.slots.len().saturating_add(cell.environment.len()),
            tail.len(),
        )?;
        return Ok(cell.slots.get(tail).or_else(|| cell.environment.get(tail)));
    }
    ordered(
        b,
        value.slots.len().saturating_add(value.environment.len()),
        path.len(),
    )?;
    Ok(value
        .slots
        .get(path)
        .or_else(|| value.environment.get(path)))
}
fn set_effect_row(
    value: &mut Value,
    path: &[PlaceProjection],
    p: Permission,
    b: &mut Budget,
) -> CompileResult<()> {
    if let Some(head) = effect_head(value, path, b)? {
        fee(b, head.len())?;
        let head = head.clone();
        let cell = value.pointees.get_mut(&head).unwrap();
        let tail = &path[head.len()..];
        fee(b, tail.len())?;
        if cell.environment.contains_key(tail) {
            cell.environment.insert(tail.to_vec(), p);
        } else {
            cell.slots.insert(tail.to_vec(), p);
        }
        return Ok(());
    }
    fee(b, path.len())?;
    if value.slots.contains_key(path) {
        value.slots.insert(path.to_vec(), p);
    } else {
        value.environment.insert(path.to_vec(), p);
    }
    Ok(())
}
pub(super) fn slot_active(value: &Value, path: &[PlaceProjection]) -> bool {
    let mut previous = 0usize;
    let mut active = value.enum_variants.slot_active(path);
    for (head, cell) in &value.pointees {
        if head.len() < path.len() && path.starts_with(head) && head.len() >= previous {
            active &= cell.enum_variants.slot_active(&path[head.len()..]);
            previous = head.len();
        }
    }
    active
}
pub(super) fn complete_frame(
    module: &TypedModule,
    d: &ClosureDefinition,
    value: &Value,
    b: &mut Budget,
) -> CompileResult<bool> {
    for (path, _) in leaves(module, d, b)? {
        if !slot_active(value, &path) {
            continue;
        }
        if effect_row(value, &path, b)?.is_none_or(|p| p.targets.is_empty()) {
            return Ok(false);
        }
    }
    let mut pending = vec![(d, Vec::new(), 0usize)];
    while let Some((d, prefix, depth)) = pending.pop() {
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "current frame coverage exceeds finite depth",
                None,
            )]);
        }
        for (index, ty) in d.captures.iter().enumerate() {
            fee(b, prefix.len().saturating_add(1))?;
            let mut head = prefix.clone();
            head.push(PlaceProjection::Field(index as u32));
            if let Type::Address { inner, .. } = ty
                && pointee_memory::aggregate(inner)
            {
                let Some(referent) = value.pointees.get(&head) else {
                    return Ok(false);
                };
                if !pointee_memory::complete(referent, inner, b)? {
                    return Ok(false);
                }
            } else if let Type::Closure(child) = ty {
                pending.push((&module.closures[child.0 as usize], head, depth + 1));
            }
        }
    }
    Ok(true)
}
