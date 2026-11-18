//! Addressed callable values refer to current frame memory. Their SSA copy is
//! only a head: it cannot freeze the environment before a mutable helper call.
use super::*;

fn captured_local(state: &State, capture: u32, b: &mut Budget) -> CompileResult<Option<LocalId>> {
    let mut selected = None;
    for (local, index) in &state.capture_views {
        fee(b, 1)?;
        if *index == capture && selected.replace(*local).is_some() {
            return Err(failure(
                "current environment has competing active capture views",
            ));
        }
    }
    Ok(selected)
}

pub(super) fn rows<'a>(
    state: &'a State,
    base: HoldRoot,
    b: &mut Budget,
) -> CompileResult<&'a BTreeMap<Path, Permission>> {
    ordered(
        b,
        state
            .locals
            .len()
            .saturating_add(state.captures.len())
            .saturating_add(state.external.len()),
        0,
    )?;
    let value = match base {
        HoldRoot::Local(local) => state.locals.get(&local),
        HoldRoot::Capture(index) => match captured_local(state, index, b)? {
            Some(local) => state.locals.get(&local),
            None => state.captures.get(&index),
        },
        _ => None,
    };
    value
        .map(|value| &value.environment)
        .or_else(|| state.external.get(&base).map(|value| &value.environment))
        .ok_or_else(|| failure("addressed callable has no current actual environment"))
}

/// Exact physical prefixes only. Capture/retained port paths stay in their
/// own metadata namespace, even when projected by the same field operation.
pub(super) fn project<P: Copy + Ord>(
    rows: &BTreeMap<Vec<P>, Permission>,
    prefix: &[P],
    b: &mut Budget,
) -> CompileResult<BTreeMap<Vec<P>, Permission>> {
    let mut out = BTreeMap::new();
    for (path, permission) in rows {
        fee(b, path.len().saturating_add(prefix.len()).saturating_add(1))?;
        if let Some(relative) = path.strip_prefix(prefix) {
            ordered(b, out.len(), relative.len())?;
            fee(b, relative.len())?;
            out.insert(relative.to_vec(), copy_permission(permission, b)?);
        }
    }
    Ok(out)
}

pub(super) fn replace<P: Copy + Ord>(
    rows: &mut BTreeMap<Vec<P>, Permission>,
    prefix: &[P],
    incoming: BTreeMap<Vec<P>, Permission>,
    b: &mut Budget,
) -> CompileResult<BTreeMap<Vec<P>, Permission>> {
    if prefix.is_empty() {
        return Ok(std::mem::replace(rows, incoming));
    }
    let mut selected = Vec::new();
    for path in rows.keys() {
        fee(b, path.len().saturating_add(prefix.len()).saturating_add(1))?;
        if path.starts_with(prefix) {
            selected.push(path.clone());
        }
    }
    let mut previous = BTreeMap::new();
    for path in selected {
        ordered(b, rows.len(), path.len())?;
        let permission = rows
            .remove(&path)
            .ok_or_else(|| failure("selected environment row disappeared during replacement"))?;
        let relative = &path[prefix.len()..];
        fee(b, relative.len())?;
        ordered(b, previous.len(), relative.len())?;
        previous.insert(relative.to_vec(), permission);
    }
    for (tail, permission) in incoming {
        fee(b, prefix.len().saturating_add(tail.len()))?;
        let mut path = prefix.to_vec();
        path.extend(tail);
        ordered(b, rows.len(), path.len())?;
        if rows.insert(path, permission).is_some() {
            return Err(failure("environment replacement overlaps an unretired row"));
        }
    }
    Ok(previous)
}

fn held_rows(rows: &BTreeMap<Path, Permission>, b: &mut Budget) -> CompileResult<Permission> {
    let mut held = Permission::default();
    for permission in rows.values() {
        union(&mut held, permission, b)?;
    }
    Ok(held)
}

pub(super) fn complete_held(value: &Value, b: &mut Budget) -> CompileResult<()> {
    let mut before = held_rows(&value.environment, b)?;
    for payload in value.projections.values() {
        union(&mut before, &payload.held, b)?;
    }

    permission_clone_work(&before, b)?;
    permission_clone_work(&value.held, b)?;
    if before != value.held {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "environment field update needs its complete nonenvironment held projection",
            None,
        )]);
    }
    Ok(())
}

pub(super) fn replace_value(
    value: &mut Value,
    prefix: &[PlaceProjection],
    incoming: BTreeMap<Path, Permission>,
    b: &mut Budget,
) -> CompileResult<BTreeMap<Path, Permission>> {
    // This existing held channel is the complete environment union for an F
    // or its aggregate shell. Do not erase an unprojected permission family.
    complete_held(value, b)?;
    let previous = replace(&mut value.environment, prefix, incoming, b)?;
    value.held = held_rows(&value.environment, b)?;
    for payload in value.projections.values() {
        union(&mut value.held, &payload.held, b)?;
    }
    Ok(previous)
}

pub(super) fn at(
    state: &State,
    base: HoldRoot,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<BTreeMap<Path, Permission>> {
    project(rows(state, base, b)?, path, b)
}

pub(super) fn current(
    value: &Value,
    state: &State,
    b: &mut Budget,
) -> CompileResult<BTreeMap<Path, Permission>> {
    if value.pointer.targets.is_empty() {
        return copy_permission_rows(&value.environment, b);
    }
    let mut out = BTreeMap::new();
    for head in &value.pointer.targets {
        fee(b, target_width(head).saturating_add(1))?;
        let address = Permission {
            targets: Targets::from([head.clone()]),
            loans: Loans::new(),
        };
        let current = pointee_memory::read(state, &address, None, false, b)?;
        for (path, p) in current.environment {
            ordered(b, out.len(), path.len())?;
            union(out.entry(path).or_default(), &p, b)?;
        }
    }
    Ok(out)
}

/// Resolve a saved pointer through its current pointee, not the constructor's
/// earlier nested-row image. A present pointer prefix forbids stale fallback.
pub(super) fn select_rows_current(
    rows: &BTreeMap<Path, Permission>,
    state: &State,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Permission> {
    for length in 1..path.len() {
        ordered(b, rows.len(), length)?;
        if let Some(head) = rows.get(&path[..length]) {
            let current = pointee_memory::read(state, head, None, false, b)?;
            let tail = &path[length..];
            if let Some(p) = current.slots.get(tail) {
                return copy_permission(p, b);
            }
            // Every boundary consumes at least one actual selector. No
            // missing current row can fall back to the old SSA directory.
            return select_rows_current(&current.environment, state, tail, b);
        }
    }
    ordered(b, rows.len(), path.len())?;
    rows.get(path)
        .map(|p| copy_permission(p, b))
        .transpose()
        .map(|p| p.unwrap_or_default())
}
pub(super) fn select_current(
    value: &Value,
    state: &State,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Permission> {
    select_rows_current(&current(value, state, b)?, state, path, b)
}

pub(super) fn held<'a>(
    value: &'a Value,
    state: &State,
    b: &mut Budget,
) -> CompileResult<std::borrow::Cow<'a, Permission>> {
    if value.saved_heads.is_empty()
        && (value.environment.is_empty() || value.pointer.targets.is_empty())
    {
        return Ok(std::borrow::Cow::Borrowed(&value.held));
    }
    let rows = current(value, state, b)?;
    let mut held = held_rows(&rows, b)?;
    let mut pending = Vec::new();
    for head in &value.saved_heads {
        fee(b, head.len().saturating_add(1))?;
        if let Some(pointer) = rows.get(head) {
            pending.push(copy_permission(pointer, b)?);
        }
    }
    let mut seen = Targets::new();
    while let Some(pointer) = pending.pop() {
        for target in &pointer.targets {
            ordered(b, seen.len(), target_width(target))?;
            fee(b, target_width(target).saturating_add(1))?;
            if !seen.insert(target.clone()) {
                continue;
            }
            let address = Permission {
                targets: Targets::from([target.clone()]),
                loans: Loans::new(),
            };
            let referent = pointee_memory::read(state, &address, None, false, b)?;
            for p in pointee_memory::permissions(&referent) {
                union(&mut held, p, b)?;
            }
            for head in &referent.saved_heads {
                fee(b, head.len().saturating_add(1))?;
                if let Some(pointer) = referent.environment.get(head) {
                    pending.push(copy_permission(pointer, b)?);
                }
            }
        }
    }
    for p in value.projections.values() {
        union(&mut held, &p.held, b)?;
    }
    Ok(std::borrow::Cow::Owned(held))
}

// Find the exact Fixed class of an actual environment leaf, then instantiate
// it through this helper's parameter-class row. Equal types grant no equality.
pub(super) fn fixed(
    module: &TypedModule,
    id: ClosureId,
    path: &[PlaceProjection],
    depth: usize,
    b: &mut Budget,
) -> CompileResult<u32> {
    fee(b, path.len().saturating_add(1))?;
    if depth > b.partial_depth {
        return Err(failure("callable leaf relation exceeds finite depth"));
    }
    let d = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| failure("callable leaf definition absent"))?;
    let Some((PlaceProjection::Field(index), tail)) = path.split_first() else {
        return Err(failure("callable leaf omits capture selector"));
    };
    let ty = d
        .captures
        .get(*index as usize)
        .ok_or_else(|| failure("callable leaf capture absent"))?;
    let c = d
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("callable leaf lacks actual class relation"))?;
    let base = StorageBase::Capture(*index);
    let binder = match ty {
        Type::Address { .. } | Type::Desc(_) if tail.is_empty() => {
            fee(b, c.bindings.len())?;
            super::super::signature_life::outer(c, base)
        }
        Type::Address { inner, .. } if carrier(inner).is_some() => {
            let (_, life) = carrier(inner).unwrap();
            let (class, duplicate) = b
                .physical_slot(life, tail)?
                .ok_or_else(|| failure("pointed callable leaf has no physical slot"))?;
            if duplicate {
                return Err(failure(
                    "pointed callable leaf has duplicate physical slots",
                ));
            }
            fee(b, c.classes.len())?;
            c.classes
                .iter()
                .find(|row| {
                    row.position.base == base
                        && row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                        && row.position.class == class
                })
                .map(|row| row.binder)
        }
        Type::BorrowedStorage { life, .. } => {
            let (class, duplicate) = b
                .physical_slot(life, tail)?
                .ok_or_else(|| failure("callable leaf has no physical slot"))?;
            if duplicate {
                return Err(failure("callable leaf has duplicate physical slots"));
            }
            fee(b, c.classes.len())?;
            c.classes
                .iter()
                .find(|row| {
                    row.position.base == base
                        && row.position.path.is_empty()
                        && row.position.class == class
                })
                .map(|row| row.binder)
        }
        Type::Closure(child) => {
            let class = fixed(module, *child, tail, depth + 1, b)?;
            fee(b, c.classes.len())?;
            c.classes
                .iter()
                .find(|row| {
                    row.position.base == base
                        && row.position.path.is_empty()
                        && row.position.class == class
                })
                .map(|row| row.binder)
        }
        _ => None,
    }
    .ok_or_else(|| failure("callable leaf lacks exact capture class binding"))?;
    match binder {
        StorageBinder::Fixed(class) => Ok(class),
        _ => Err(failure("external callable leaf requires a Fixed class")),
    }
}

pub(super) fn binding(
    module: &TypedModule,
    f: &TypedFunction,
    index: u32,
    path: &Path,
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    if path.is_empty() {
        return super::binding(f, StorageBase::Parameter(index), path, b);
    }
    fee(b, path.len())?;
    let key = (f.id, index, path.clone());
    ordered(b, b.semantic_environment_binders.len(), path.len())?;
    if let Some(binder) = b.semantic_environment_binders.get(&key) {
        return Ok(*binder);
    }
    let ty = &f.locals[f
        .parameters
        .get(index as usize)
        .ok_or_else(|| failure("external class parameter absent"))?
        .0 as usize]
        .ty;
    let (ty, pointed) = if let Type::Address { inner, .. } = ty {
        (inner.as_ref(), true)
    } else {
        (ty, false)
    };
    let Type::Closure(id) = ty else {
        return super::binding(f, StorageBase::Parameter(index), path, b);
    };
    let class = fixed(module, *id, path, 0, b)?;
    let c = f
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("external callable signature relation absent"))?;
    fee(b, c.classes.len())?;
    c.classes
        .iter()
        .find(|row| {
            row.position.base == StorageBase::Parameter(index)
                && row.position.class == class
                && if pointed {
                    row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                } else {
                    row.position.path.is_empty()
                }
        })
        .map(|row| row.binder)
        .ok_or_else(|| failure("signature omits actual external callable class"))
}
