//! Prepared actual memory updates. Preparation reads the one call-before
//! occupant; publication only installs those facts, never resolves another
//! source against an already updated sibling. Unknown is explicit debt.
use super::*;
#[derive(Default)]
pub(super) struct Prepared {
    pub(super) rows: Vec<(HoldRoot, Path, Permission, bool)>,
    pub(super) pending: bool,
}
pub(super) fn publish(state: &mut Frame, prepared: Prepared, b: &mut Budget) -> Check<()> {
    state.pending_external_update |= prepared.pending;
    for (base, path, permission, unique) in prepared.rows {
        let holder = memory_holder_mut(state, base, b)?
            .ok_or_else(|| err("updated caller carrier location is absent"))?;
        if unique && let Some(row) = holder.leaves.position(&path, b)? {
            *holder.leaves.permission_mut(row) = permission;
        } else {
            add_leaf(holder, &path, &permission, b)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn function(
    module: &TypedModule,
    f: &TypedFunction,
    callee: &TypedFunction,
    i: &Instruction,
    summary: &Summary,
    arguments: &[ValueId],
    values: &[Payload],
    state: &Frame,
    sites: &WriteSites,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<Prepared> {
    let before = call_results::Before::function(module, f, i, values, state, b)?;
    let mut updates = Vec::new();
    let mut pending = false;
    for (parameter, path, sources) in &summary.external_after {
        let index = *parameter as usize;
        let local = *callee
            .parameters
            .get(index)
            .ok_or_else(|| err("external summary parameter is absent"))?;
        let ty = &callee.locals[local.0 as usize].ty;
        let captured = captured_owner::external_capture_tail(module, ty, path, b)?;
        let borrowed = matches!(ty,
                    Type::Address { mutable: true, inner } if qualified(inner).is_some() || returned_environment::closure(inner).is_some());
        if captured.is_none() && !borrowed {
            continue;
        }
        let actual = values
            .get(
                arguments
                    .get(index)
                    .ok_or_else(|| err("external summary actual argument is absent"))?
                    .0 as usize,
            )
            .ok_or_else(|| err("external summary input snapshot is absent"))?;
        let mut permission = Permission::default();
        let mut known = summary.complete;
        for source in sources {
            let actual = before.source(source, b)?;
            known &= !actual.sources.is_empty();
            join_permission(&mut permission, &actual, b)?;
        }
        if !known || permission.sources.is_empty() {
            // Bottom during the interprocedural fixed point is not an
            // empty after-state. Preserve an explicit unproved debt.
            pending = true;
            continue;
        }
        b.spend(path.len().saturating_add(permission.sources.len()))?;
        b.spend(
            summary
                .external_stable
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let stable = summary
            .external_stable
            .iter()
            .any(|(p, row)| p == parameter && row == path);
        let retained = if stable {
            let previous = if matches!(ty, Type::Address { .. }) {
                select(state, &actual.address, path, b)?
            } else {
                let row = leaf(actual, path, b)?
                    .ok_or_else(|| err("stable environment after-row is absent"))?;
                permission_work(row, b)?;
                row.clone()
            };
            permission_work(&previous, b)?;
            permission_work(&permission, b)?;
            if previous != permission {
                return Err(err(
                    "stable external row changed its actual targets or loan identity",
                ));
            }
            continue; // exact unchanged memory needs no caller update
        } else {
            retain_written_slot(&permission, i, *parameter, path, sites, loans, b)?
        };
        if let Some((capture, tail)) = captured {
            let head = if matches!(ty, Type::Address { .. }) {
                select(
                    state,
                    &actual.address,
                    &[PlaceProjection::Field(capture)],
                    b,
                )?
            } else {
                let head = leaf(actual, &[PlaceProjection::Field(capture)], b)?
                    .ok_or_else(|| err("by-value environment lacks its actual captured pointer"))?;
                permission_work(head, b)?;
                head.clone()
            };
            if head.sources.is_empty() {
                return Err(err("captured pointee update has no actual target"));
            }
            for root in &head.sources {
                let holder = memory_holder(state, root.base, b)?
                    .ok_or_else(|| err("captured pointee update target memory is absent"))?;
                let complete = symbolic_slot_path(holder, root, tail, b)?.ok_or_else(|| {
                    err("captured pointee update lacks a complete actual target directory")
                })?;
                permission_work(&retained, b)?;
                updates.push((
                    root.base,
                    complete,
                    retained.clone(),
                    head.sources.len() == 1,
                ));
            }
        }
        if borrowed {
            let unique = actual.address.sources.len() == 1;
            for root in &actual.address.sources {
                if !root.leaf.is_empty()
                    || root.retained_port.is_some()
                    || root.heap_element.is_some()
                    || !root.heap_parents.is_empty()
                {
                    return Err(err("external update crosses an unproved target boundary"));
                }
                b.spend(root.inline.len().saturating_add(path.len()))?;
                let mut actual_path = root.inline.clone();
                actual_path.extend(path);
                permission_work(&retained, b)?;
                updates.push((root.base, actual_path, retained.clone(), unique));
            }
        }
    }
    Ok(Prepared {
        rows: updates,
        pending,
    })
}
