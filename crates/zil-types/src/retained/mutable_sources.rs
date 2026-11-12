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
    if let Some(head) = row.get(&Vec::new())
        && !head.places.is_empty()
    {
        let mut out = Row::new();
        spend(left, head.places.len(), span)?;
        for (local, prefix) in &head.places {
            spend(left, prefix.len().saturating_add(path.len()), span)?;
            let mut full = prefix.clone();
            full.extend_from_slice(path);
            let holder = state
                .get(local.0 as usize)
                .ok_or_else(|| unsupported(span, "mutable symbolic receiver is absent"))?;
            join(&mut out, &select(holder, &full, left, span)?, left, span)?;
        }
        return Ok(out);
    }
    let selected = select(row, path, left, span)?;
    if !selected.is_empty() {
        Ok(selected)
    } else {
        pointed(row, path, left, span)
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
    let mut paths = Vec::new();
    leaves(
        module,
        &Type::Closure(id),
        0,
        &mut Vec::new(),
        &mut paths,
        left,
        span,
        limit,
    )?;
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
