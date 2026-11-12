//! Symbolic effects of actual mutable callable bodies. These are producer
//! proposals only: each safety kernel rebuilds terminal memory independently.
use super::*;

pub(super) fn current(
    row: &Row,
    path: &[ContractProjection],
    state: &[Row],
    left: &mut usize,
    span: Span,
) -> R<Row> {
    let mut out = Row::new();
    spend(left, path.len().saturating_add(1), span)?;
    let mut pending = vec![(row, path.to_vec())];
    let mut seen = BTreeSet::new();
    while let Some((row, path)) = pending.pop() {
        let selected = select(row, &path, left, span)?;
        if selected.contains_key(&Vec::new()) || path.is_empty() {
            join(&mut out, &selected, left, span)?;
            continue;
        }
        let mut followed = false;
        for length in (0..path.len()).rev() {
            spend(left, length.saturating_add(1), span)?;
            let Some(head) = row.get(&path[..length]) else {
                continue;
            };
            if !head.places.is_empty() {
                spend(left, head.places.len(), span)?;
                for (local, prefix) in &head.places {
                    spend(
                        left,
                        prefix
                            .len()
                            .saturating_add(path.len() - length)
                            .saturating_add(seen.len()),
                        span,
                    )?;
                    let mut full = prefix.clone();
                    full.extend_from_slice(&path[length..]);
                    spend(
                        left,
                        seen.len()
                            .saturating_add(1)
                            .saturating_mul(full.len().saturating_add(1))
                            .saturating_add(full.len()),
                        span,
                    )?;
                    if seen.insert((*local, full.clone())) {
                        pending.push((
                            state.get(local.0 as usize).ok_or_else(|| {
                                unsupported(span, "mutable symbolic pointee absent")
                            })?,
                            full,
                        ));
                    }
                }
            } else {
                let projection = select(row, &path[..length], left, span)?;
                join(
                    &mut out,
                    &pointed(&projection, &path[length..], left, span)?,
                    left,
                    span,
                )?;
            }
            followed = true;
            break;
        }
        if !followed {
            join(&mut out, &pointed(row, &path, left, span)?, left, span)?;
        }
    }
}

pub(super) fn replace(
    holder: &mut Row,
    prefix: &[ContractProjection],
    value: &Row,
    weak: bool,
    left: &mut usize,
    span: Span,
) -> R<()> {
    cost(value, left, span)?;
    spend(
        left,
        holder.len().saturating_mul(prefix.len().saturating_add(1)),
        span,
    )?;
    if !weak {
        holder.retain(|path, _| !path.starts_with(prefix));
    }
    let mut replacement = Row::new();
    for (path, origin) in value {
        spend(left, prefix.len().saturating_add(path.len()), span)?;
        let mut full = prefix.to_vec();
        full.extend_from_slice(path);
        replacement.insert(full, origin.clone());
    }
    join(holder, &replacement, left, span)?;
    Ok(())
}

pub(super) fn substitute(
    origins: &Origins,
    receiver: Option<&Row>,
    arguments: &[ValueId],
    values: &[Row],
    state: &[Row],
    left: &mut usize,
    span: Span,
) -> R<Origins> {
    let mut out = Origins {
        local: origins.local,
        unknown: origins.unknown,
        ..Origins::default()
    };
    spend(left, origins.inputs.len(), span)?;
    for source in &origins.inputs {
        let mut selected = match source.base {
            StorageBase::Capture(index) => {
                let receiver = receiver.ok_or_else(|| {
                    unsupported(span, "capture effect has no actual symbolic receiver")
                })?;
                spend(left, source.path.len().saturating_add(1), span)?;
                let mut path = vec![ContractProjection::Capture(index)];
                path.extend_from_slice(&source.path);
                current(receiver, &path, state, left, span)?
            }
            StorageBase::Parameter(index) => {
                let argument = arguments
                    .get(index as usize)
                    .and_then(|value| values.get(value.0 as usize))
                    .ok_or_else(|| unsupported(span, "mutable source argument absent"))?;
                current(argument, &source.path, state, left, span)?
            }
            _ => {
                return Err(unsupported(
                    span,
                    "mutable effect is not rooted in actual inputs or captures",
                ));
            }
        };
        if let Some(actual) = selected.remove(&Vec::new()) {
            let row = Row::from([(Vec::new(), actual)]);
            let mut result = Row::from([(Vec::new(), out)]);
            join(&mut result, &row, left, span)?;
            out = result.remove(&Vec::new()).unwrap();
        }
    }
    Ok(out)
}

pub(super) fn effect_paths(
    module: &TypedModule,
    ty: &Type,
    left: &mut usize,
    span: Span,
    limit: usize,
) -> R<Vec<Path>> {
    let mut paths = Vec::new();
    leaves(
        module,
        ty,
        0,
        &mut Vec::new(),
        &mut paths,
        left,
        span,
        limit,
    )?;
    let mut pending = vec![(ty, Vec::new(), 0usize)];
    while let Some((ty, prefix, depth)) = pending.pop() {
        spend(left, prefix.len().saturating_add(1), span)?;
        if depth > limit {
            return Err(unsupported(
                span,
                "private pointee context exceeds finite depth",
            ));
        }
        match ty {
            Type::Closure(id) => {
                let d = &module.closures[id.0 as usize];
                spend(left, d.captures.len(), span)?;
                for (index, capture) in d.captures.iter().enumerate() {
                    let mut path = prefix.clone();
                    spend(left, path.len().saturating_add(1), span)?;
                    path.push(ContractProjection::Capture(index as u32));
                    pending.push((capture, path, depth + 1));
                }
            }
            Type::Address { inner, .. } => {
                if let Type::BorrowedStorage { life, .. } = inner.as_ref() {
                    for slot in &life.slots {
                        spend(
                            left,
                            prefix
                                .len()
                                .saturating_add(slot.path.len())
                                .saturating_add(1),
                            span,
                        )?;
                        let mut path = prefix.clone();
                        path.extend(slot.path.iter().copied().map(ContractProjection::Place));
                        paths.push(path);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(paths)
}

pub(super) fn context_updates(
    module: &TypedModule,
    id: ClosureId,
    receiver: &Row,
    updates: &Row,
    state: &[Row],
    left: &mut usize,
    span: Span,
    limit: usize,
) -> R<Vec<(Row, Row)>> {
    let mut out = Vec::new();
    let mut pending = vec![(id, Vec::new(), 0usize)];
    while let Some((id, prefix, depth)) = pending.pop() {
        if depth > limit {
            return Err(unsupported(
                span,
                "mutable borrowed context exceeds finite depth",
            ));
        }
        let d = &module.closures[id.0 as usize];
        spend(left, d.captures.len().saturating_add(prefix.len()), span)?;
        for (index, ty) in d.captures.iter().enumerate() {
            spend(left, prefix.len().saturating_add(1), span)?;
            let mut head = prefix.clone();
            head.push(ContractProjection::Capture(index as u32));
            if let Type::Closure(child) = ty {
                pending.push((*child, head, depth + 1));
                continue;
            }
            let Type::Address {
                mutable: true,
                inner,
            } = ty
            else {
                continue;
            };
            let Type::BorrowedStorage { life, .. } = inner.as_ref() else {
                continue;
            };
            let address = current(receiver, &head, state, left, span)?;
            if !address
                .get(&Vec::new())
                .is_some_and(|origin| !origin.places.is_empty())
            {
                continue;
            }
            let mut fields = Row::new();
            for slot in &life.slots {
                spend(left, head.len().saturating_add(slot.path.len()), span)?;
                let mut full = head.clone();
                let path = slot
                    .path
                    .iter()
                    .copied()
                    .map(ContractProjection::Place)
                    .collect::<Vec<_>>();
                full.extend_from_slice(&path);
                let mut value = select(updates, &full, left, span)?;
                if let Some(origin) = value.remove(&Vec::new()) {
                    spend(
                        left,
                        fields
                            .len()
                            .saturating_add(1)
                            .saturating_mul(path.len().saturating_add(1)),
                        span,
                    )?;
                    fields.insert(path, origin);
                }
            }
            out.push((address, fields));
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn invoke(
    module: &TypedModule,
    id: ClosureId,
    receiver: &Row,
    arguments: &[ValueId],
    values: &[Row],
    state: &[Row],
    summaries: &[Summary],
    left: &mut usize,
    span: Span,
    limit: usize,
) -> R<Row> {
    let definition = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| unsupported(span, "mutable callable definition absent"))?;
    let summary = &summaries[definition.function.0 as usize].captures;
    let paths = effect_paths(module, &Type::Closure(id), left, span, limit)?;
    let mut updates = Row::new();
    for path in paths {
        let origin = if definition.kind == CallableKind::Fn {
            current(receiver, &path, state, left, span)?
                .remove(&Vec::new())
                .unwrap_or_default()
        } else if let Some(actual) = summary.get(&path) {
            substitute(actual, Some(receiver), arguments, values, state, left, span)?
        } else {
            Origins::default()
        };
        spend(left, path.len().saturating_add(1), span)?;
        updates.insert(path, origin);
    }
    Ok(updates)
}

pub(super) fn publish(
    address: &Row,
    updates: &Row,
    state: &mut [Row],
    left: &mut usize,
    span: Span,
) -> R<()> {
    let head = address
        .get(&Vec::new())
        .ok_or_else(|| unsupported(span, "mutable receiver has no actual symbolic head"))?;
    spend(left, head.places.len(), span)?;
    if head.places.is_empty() {
        return Err(unsupported(
            span,
            "mutable receiver lacks an actual symbolic memory location",
        ));
    }
    for (local, prefix) in &head.places {
        for (path, origin) in updates {
            spend(left, prefix.len().saturating_add(path.len()), span)?;
            let mut full = prefix.clone();
            full.extend_from_slice(path);
            replace(
                state
                    .get_mut(local.0 as usize)
                    .ok_or_else(|| unsupported(span, "mutable receiver memory absent"))?,
                &full,
                &Row::from([(Vec::new(), origin.clone())]),
                head.places.len() != 1,
                left,
                span,
            )?;
        }
    }
    Ok(())
}
