//! Retained owning capture updates are inferred from terminal entry states.
//! A lifetime class constrains an update; only actual receiver permissions
//! supply its target and parent loans at a call.
use super::*;

pub(super) fn shape(ty: &Type) -> Option<&StorageLifeShape> {
    match ty {
        Type::BorrowedStorage { storage, life }
            if matches!(storage.as_ref(), Type::Package { .. }) =>
        {
            Some(life)
        }
        _ => None,
    }
}

pub(super) fn mutable_call<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    kind: &'a InstructionKind,
) -> Option<(&'a ClosureDefinition, &'a [ValueId])> {
    if !matches!(
        kind,
        InstructionKind::CallClosureMut { .. }
            | InstructionKind::CallClosureBorrowed {
                kind: CallableKind::FnMut,
                ..
            }
    ) {
        return None;
    }
    stored_callables::called(module, f, kind)
}

pub(super) fn write_paths(
    module: &TypedModule,
    f: &TypedFunction,
    kind: &InstructionKind,
    b: &mut Budget,
) -> Check<Vec<(Path, bool)>> {
    if !b.nested_callable_sources {
        return Ok(Vec::new());
    }
    let Some((definition, _)) = mutable_call(module, f, kind) else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    b.spend(definition.captures.len())?;
    for (index, ty) in definition.captures.iter().enumerate() {
        if let Some(shape) = shape(ty) {
            for slot in &shape.slots {
                b.spend(slot.path.len().saturating_add(2))?;
                let mut path = vec![PlaceProjection::Field(index as u32)];
                path.extend(&slot.path);
                paths.push((path, slot.mode == StoredBorrowMode::ExclusiveAddress));
            }
        }
    }
    Ok(paths)
}

pub(super) fn class(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let HoldRoot::Capture(index) = source.base else {
        let HoldRoot::Parameter(index) = source.base else {
            return Err(err("retained capture update keeps activation storage"));
        };
        let ty = f
            .parameters
            .get(index as usize)
            .and_then(|id| f.locals.get(id.0 as usize))
            .map(|local| &local.ty)
            .ok_or_else(|| err("capture update parameter is absent"))?;
        let (ty, pointee) = match ty {
            Type::Address { inner, .. } => (inner.as_ref(), true),
            ty => (ty, false),
        };
        if let Type::Closure(id) = ty {
            let Some((PlaceProjection::Field(capture), tail)) = source.leaf.split_first() else {
                return relation(f, StorageBase::Parameter(index), &source.leaf, b);
            };
            let entry = module
                .closures
                .get(id.0 as usize)
                .and_then(|definition| module.functions.get(definition.function.0 as usize))
                .ok_or_else(|| err("closure argument has no actual entry"))?;
            b.spend(tail.len().saturating_add(1))?;
            let nested = Source {
                base: HoldRoot::Capture(*capture),
                leaf: tail.to_vec(),
                inline: Vec::new(),
                retained_port: None,
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let StorageBinder::Fixed(local) = class(module, entry, &nested, b)? else {
                return Err(err("closure external target has no persistent class"));
            };
            let contract = f
                .storage_life
                .as_ref()
                .ok_or_else(|| err("closure argument lacks a complete caller class relation"))?;
            b.spend(contract.classes.len())?;
            return contract
                .classes
                .iter()
                .find(|row| {
                    row.position.base == StorageBase::Parameter(index)
                        && row.position.class == local
                        && if pointee {
                            row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                        } else {
                            row.position.path.is_empty()
                        }
                })
                .map(|row| row.binder)
                .ok_or_else(|| err("closure argument omits its actual persistent class"));
        }
        return relation(f, StorageBase::Parameter(index), &source.leaf, b);
    };
    let definition = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .ok_or_else(|| err("retained update lacks an actual entry environment"))?;
    let ty = definition
        .captures
        .get(index as usize)
        .ok_or_else(|| err("retained update source capture is absent"))?;
    let contract = f
        .storage_life
        .as_ref()
        .ok_or_else(|| err("retained capture update lacks a complete lifetime relation"))?;
    if source.leaf.is_empty() {
        if !matches!(ty, Type::Address { .. } | Type::Desc(_)) {
            return Err(err("owned capture head is not an external stored target"));
        }
        b.spend(contract.bindings.len())?;
        return signature_life::outer(contract, StorageBase::Capture(index))
            .ok_or_else(|| err("external capture source lacks its actual lifetime binding"));
    }
    let (shape, pointee) = match ty {
        Type::BorrowedStorage { life, .. } => (life, false),
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::BorrowedStorage { life, .. } => (life, true),
            _ => return Err(err("capture source path has no qualified physical storage")),
        },
        _ => return Err(err("capture source path has no qualified physical storage")),
    };
    b.spend(
        shape
            .slots
            .len()
            .saturating_mul(source.leaf.len().saturating_add(1)),
    )?;
    let slot = shape
        .slots
        .iter()
        .find(|slot| slot.path == source.leaf)
        .ok_or_else(|| err("capture source selects no actual external slot"))?;
    b.spend(contract.classes.len())?;
    contract
        .classes
        .iter()
        .find(|row| {
            row.position.base == StorageBase::Capture(index)
                && row.position.class == slot.binder
                && if pointee {
                    row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                } else {
                    row.position.path.is_empty()
                }
        })
        .map(|row| row.binder)
        .ok_or_else(|| err("capture slot has no complete class binding"))
}

pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    state: &Frame,
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    if !b.nested_callable_sources {
        return Ok(());
    }
    let Some(definition) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
    else {
        return Ok(());
    };
    if definition.kind == CallableKind::FnOnce {
        return Ok(());
    }
    b.spend(definition.captures.len())?;
    for (index, ty) in definition.captures.iter().enumerate() {
        let Some(shape) = shape(ty) else {
            continue;
        };
        b.spend(shape.slots.len())?;
        let payload = state
            .captures
            .get(index)
            .ok_or_else(|| err("retained owning capture has no actual output state"))?;
        for slot in &shape.slots {
            let Some(permission) = leaf(payload, &slot.path, b)? else {
                summary.complete = false;
                continue;
            };
            if permission.sources.is_empty() {
                summary.complete = false;
            }
            b.spend(slot.path.len().saturating_add(1))?;
            let destination = Source {
                base: HoldRoot::Capture(index as u32),
                leaf: slot.path.clone(),
                inline: Vec::new(),
                retained_port: None,
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let bound = class(module, f, &destination, b)?;
            for source in &permission.sources {
                if source.retained_port.is_some() || source.heap_element.is_some() {
                    return Err(err(
                        "capture update has no authenticated opaque or heap target mapping",
                    ));
                }
                if source.base != HoldRoot::Static && class(module, f, source, b)? != bound {
                    return Err(err("capture update exceeds its actual lifetime relation"));
                }
            }
            b.spend(
                summary
                    .capture_after
                    .len()
                    .saturating_mul(slot.path.len().saturating_add(1)),
            )?;
            if let Some((_, _, row)) = summary
                .capture_after
                .iter_mut()
                .find(|(capture, path, _)| *capture == index as u32 && *path == slot.path)
            {
                add_sources(row, &permission.sources, b)?;
            } else {
                b.spend(slot.path.len().saturating_add(1))?;
                source_work(&permission.sources, b)?;
                summary.capture_after.push((
                    index as u32,
                    slot.path.clone(),
                    permission.sources.clone(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn receiver(
    f: &TypedFunction,
    i: &Instruction,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Permission> {
    match i.kind {
        InstructionKind::CallClosureMut { callee, .. }
        | InstructionKind::CallClosureShared { callee, .. } => {
            b.spend(1)?;
            Ok(Permission {
                sources: vec![Source {
                    base: HoldRoot::Local(callee),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    retained_port: None,
                    heap_parents: Vec::new(),
                    heap_element: None,
                }],
                loans: Vec::new(),
            })
        }
        InstructionKind::CallClosureBorrowed { callee, .. } => {
            let _ = vt(f, callee)?;
            let permission = &values[callee.0 as usize].address;
            permission_work(permission, b)?;
            Ok(permission.clone())
        }
        _ => Err(err("capture update has no actual mutable receiver")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn substitute(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    captures: &[Type],
    receiver: &Permission,
    arguments: &[ValueId],
    values: &[Payload],
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    let HoldRoot::Capture(index) = source.base else {
        return instantiate(module, f, source, arguments, values, state, b);
    };
    let ty = captures
        .get(index as usize)
        .ok_or_else(|| err("callee capture source is absent"))?;
    if source.retained_port.is_some() || source.heap_element.is_some() {
        return Err(err("capture source has no checked opaque or heap mapping"));
    }
    if source.leaf.is_empty() && !matches!(ty, Type::Address { .. } | Type::Desc(_)) {
        permission_work(receiver, b)?;
        let mut actual = receiver.clone();
        for root in &mut actual.sources {
            b.spend(source.inline.len().saturating_add(1))?;
            root.inline.push(PlaceProjection::Field(index));
            root.inline.extend(&source.inline);
        }
        return Ok(actual);
    }
    b.spend(source.leaf.len().saturating_add(1))?;
    let mut path = vec![PlaceProjection::Field(index)];
    path.extend(&source.leaf);
    let mut permission = select(state, receiver, &path, b)?;
    for target in &mut permission.sources {
        b.spend(source.inline.len())?;
        target.inline.extend(&source.inline);
    }
    Ok(permission)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &WriteSites,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<()> {
    let Some((definition, arguments)) = mutable_call(module, f, &i.kind) else {
        return Ok(());
    };
    let summary = &summaries[definition.function.0 as usize];
    if summary.capture_after.is_empty() {
        return Ok(());
    }
    let receiver = receiver(f, i, values, b)?;
    let mut updates = Vec::new();
    // Derive every replacement from one pre-call state (a swap must not read
    // its own preceding write). Old SSA pointers keep their original payload.
    for (index, tail, sources) in &summary.capture_after {
        b.spend(tail.len().saturating_add(1))?;
        let mut path = vec![PlaceProjection::Field(*index)];
        path.extend(tail);
        let mut permission = Permission::default();
        for source in sources {
            join_permission(
                &mut permission,
                &substitute(
                    module,
                    f,
                    source,
                    &definition.captures,
                    &receiver,
                    arguments,
                    values,
                    state,
                    b,
                )?,
                b,
            )?;
        }
        let retained = retain_written_slot(&permission, i, u32::MAX, &path, sites, loans, b)?;
        for root in &receiver.sources {
            if !root.leaf.is_empty() || root.retained_port.is_some() || root.heap_element.is_some()
            {
                return Err(err("capture receiver has no exact inline owning location"));
            }
            b.spend(
                root.inline
                    .len()
                    .saturating_add(path.len())
                    .saturating_add(1),
            )?;
            let mut complete = root.inline.clone();
            complete.extend(&path);
            permission_work(&retained, b)?;
            updates.push((root.base, complete, retained.clone()));
        }
    }
    let unique = receiver.sources.len() == 1;
    for (base, path, permission) in updates {
        let holder = match base {
            HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
            HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
            HoldRoot::Capture(index) => state.captures.get_mut(index as usize),
            HoldRoot::Static => None,
        }
        .ok_or_else(|| err("mutable closure receiver location is absent"))?;
        if unique && let Some(row) = holder.leaves.position(&path, b)? {
            *holder.leaves.permission_mut(row) = permission;
        } else {
            add_leaf(holder, &path, &permission, b)?;
        }
    }
    Ok(())
}
