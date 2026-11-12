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
    f: &TypedFunction,
    state: &State,
    actual: &mut Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    if f.environment.is_none() || state.captures.is_empty() {
        return Ok(());
    }
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
