//! Addressed callable values refer to current frame memory. Their SSA copy is
//! only a head: it cannot freeze the environment before a mutable helper call.
use super::*;

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
    match base {
        HoldRoot::Local(local) => state.locals.get(&local).map(|v| &v.environment),
        HoldRoot::Capture(index) => state
            .captures
            .get(&index)
            .map(|v| &v.environment)
            .or_else(|| state.external.get(&base)),
        HoldRoot::Parameter(_) => state.external.get(&base),
        HoldRoot::Static => None,
    }
    .ok_or_else(|| failure("addressed callable has no current actual environment"))
}

pub(super) fn current(
    value: &Value,
    state: &State,
    b: &mut Budget,
) -> CompileResult<BTreeMap<Path, Permission>> {
    if value.environment.is_empty() {
        return Ok(BTreeMap::new());
    }
    if value.pointer.targets.is_empty() {
        return copy_permission_rows(&value.environment, b);
    }
    let mut out = BTreeMap::new();
    fee(b, value.pointer.targets.len())?;
    for head in &value.pointer.targets {
        if !head.stored.is_empty()
            || !head.place.is_empty()
            || head.heap_element.is_some()
            || head.retained_port.is_some()
        {
            return Err(failure(
                "addressed callable requires an actual complete environment head",
            ));
        }
        for (path, permission) in rows(state, head.base, b)? {
            ordered(b, out.len(), path.len())?;
            fee(b, path.len())?;
            union(out.entry(path.clone()).or_default(), permission, b)?;
        }
    }
    Ok(out)
}

pub(super) fn held<'a>(
    value: &'a Value,
    state: &State,
    b: &mut Budget,
) -> CompileResult<std::borrow::Cow<'a, Permission>> {
    if value.environment.is_empty() || value.pointer.targets.is_empty() {
        return Ok(std::borrow::Cow::Borrowed(&value.held));
    }
    let mut held = Permission::default();
    for permission in current(value, state, b)?.values() {
        union(&mut held, permission, b)?;
    }
    Ok(std::borrow::Cow::Owned(held))
}

pub(super) fn publish(
    state: &mut State,
    base: HoldRoot,
    rows: BTreeMap<Path, Permission>,
    b: &mut Budget,
) -> CompileResult<()> {
    if matches!(base, HoldRoot::Parameter(_)) {
        ordered(b, state.external.len(), 0)?;
        let actual = state
            .external
            .get_mut(&base)
            .ok_or_else(|| failure("updated external callable unavailable"))?;
        *actual = rows;
        return Ok(());
    }
    if matches!(base, HoldRoot::Capture(_)) && state.external.contains_key(&base) {
        let mirrored = copy_permission_rows(&rows, b)?;
        state.external.insert(base, mirrored);
        if let HoldRoot::Capture(index) = base
            && !state.captures.contains_key(&index)
        {
            return Ok(());
        }
    }
    let value = match base {
        HoldRoot::Local(local) => state.locals.get_mut(&local),
        HoldRoot::Capture(index) => state.captures.get_mut(&index),
        _ => None,
    }
    .ok_or_else(|| failure("updated callable environment unavailable"))?;
    value.environment = rows;
    value.held = Permission::default();
    for leaf in value.environment.values() {
        union(&mut value.held, leaf, b)?;
    }
    Ok(())
}

// Find the exact Fixed class of an actual environment leaf, then instantiate
// it through this helper's parameter-class row. Equal types grant no equality.
fn fixed(
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
