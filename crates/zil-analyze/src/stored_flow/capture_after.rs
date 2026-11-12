//! Actual mutable environment state is an output of an invocation. A lifetime
//! class constrains that output; only the callee's terminal permissions select
//! the runtime target and preserve its complete parent loan chain.
use super::*;

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

pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    state: &State,
    actual: &mut Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    if f.environment.is_none() || state.captures.is_empty() {
        return Ok(());
    }
    let definition = module
        .closures
        .get(f.environment.unwrap().0 as usize)
        .ok_or_else(|| failure("terminal mutable environment definition absent"))?;
    let shape = leaves(module, definition, b)?;
    for (index, value) in &state.captures {
        let mut add = |tail: &Path, permission: &Permission| -> CompileResult<()> {
            let path = environment_path(*index as usize, tail, b)?;
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
        if !value.pointer.targets.is_empty() {
            add(&Vec::new(), &value.pointer)?;
        }
        for (path, p) in value.slots.iter().chain(&value.environment) {
            add(path, p)?;
        }
    }
    Ok(())
}

fn replace_environment(
    value: &mut Value,
    rows: BTreeMap<Path, Permission>,
    b: &mut Budget,
) -> CompileResult<()> {
    value.environment = rows;
    value.held = Permission::default();
    for permission in value.environment.values() {
        union(&mut value.held, permission, b)?;
    }
    Ok(())
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
    if d.kind != CallableKind::FnMut
        || (d.storage_life.is_none() && !b.returned_borrowed_environments.contains(&d.id))
    {
        return Ok(());
    }
    let (base, receiver) = match &i.kind {
        InstructionKind::CallClosureMut { callee, .. } => {
            let receiver = state
                .locals
                .get(callee)
                .ok_or_else(|| failure("mutable receiver actual environment absent"))?;
            (HoldRoot::Local(*callee), snapshot(receiver, b)?)
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
            let receiver = match head.base {
                HoldRoot::Local(local) => snapshot(
                    state
                        .locals
                        .get(&local)
                        .ok_or_else(|| failure("mutable local receiver unavailable"))?,
                    b,
                )?,
                HoldRoot::Capture(index) => snapshot(
                    state
                        .captures
                        .get(&index)
                        .ok_or_else(|| failure("mutable child receiver unavailable"))?,
                    b,
                )?,
                _ => {
                    return Err(failure(
                        "mutable external environment publication is not yet implemented",
                    ));
                }
            };
            (head.base, receiver)
        }
        _ => return Ok(()),
    };
    let summary = &summaries[d.function.0 as usize];
    let mut rows = BTreeMap::new();
    for (path, mutable) in leaves(module, d, b)? {
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
            permission = retain_slot(&permission, i, u32::MAX - 1, &path, ids, loans, b)?;
        }
        ordered(b, rows.len(), path.len())?;
        rows.insert(path, permission);
        b.stored_exclusive_loans |= mutable;
    }
    match base {
        HoldRoot::Local(local) => {
            replace_environment(state.locals.get_mut(&local).unwrap(), rows, b)?
        }
        HoldRoot::Capture(index) => {
            replace_environment(state.captures.get_mut(&index).unwrap(), rows, b)?
        }
        _ => unreachable!(),
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
            Type::Address { .. } | Type::Desc(_) => {}
            ty if ty.is_copy_value() => {}
            _ => return Ok(true),
        }
    }
    Ok(false)
}
