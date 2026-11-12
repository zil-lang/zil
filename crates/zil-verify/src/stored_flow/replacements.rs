//! Replacement in the stored-source ledger. Inventory/schedule are immutable
//! executable recipes, not permissions. Every activation captures its actual
//! CFG permission. Responsibility, initialization, tags and weak-descriptor
//! retirement are additionally checked by the independent resource/Sum kernels.
use super::*;
use crate::place_replacements::{Inventory, Origin, Schedule};
use std::rc::Rc;
use zil_ir::ReservationId;

pub(crate) struct Context {
    inventory: Inventory,
    schedule: Schedule,
    points: std::collections::BTreeMap<usize, (usize, usize)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Prepared {
    pub(super) id: ReservationId,
    pub(super) permission: Permission,
}
pub(super) fn operation(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::ReservePlaceLocal { .. }
            | InstructionKind::ReservePlaceAddress { .. }
            | InstructionKind::CommitPlaceReplace { .. }
            | InstructionKind::AbortPlaceReservation { .. }
    )
}
fn selected(f: &TypedFunction, b: &mut Budget) -> Check<bool> {
    if !b.replacements {
        return Ok(false);
    }
    b.spend(
        b.stored_replacement_functions
            .len()
            .checked_ilog2()
            .map_or(1, |n| n as usize + 1),
    )?;
    Ok(b.stored_replacement_functions.contains(&f.id))
}
fn context(f: &TypedFunction, b: &mut Budget) -> Check<Rc<Context>> {
    b.spend(1)?;
    b.stored_replacements
        .get(f.id.0 as usize)
        .and_then(Option::as_ref)
        .map(Rc::clone)
        .ok_or_else(|| err("replacement has no actual immutable CFG inventory"))
}
pub(super) fn prepare(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    b: &mut Budget,
) -> Check<()> {
    if !selected(f, b)? {
        return Ok(());
    }
    if b.stored_replacements.is_empty() {
        b.spend(module.functions.len())?;
        b.stored_replacements = (0..module.functions.len()).map(|_| None).collect();
    }
    b.spend(1)?;
    if b.stored_replacements[f.id.0 as usize].is_some() {
        return Ok(());
    }
    let inventory = Inventory::reconstruct(module, f, b)?;
    // Including unreachable blocks, authenticate the complete RHS and ticket.
    for block in &f.blocks {
        b.spend(block.instructions.len().saturating_add(1))?;
        for i in &block.instructions {
            match i.kind {
                InstructionKind::CommitPlaceReplace { reservation, value } => {
                    let actual = inventory.get(reservation, b)?;
                    type_work(&actual.ty, &b.limits.clone(), b)?;
                    type_work(vt(f, value)?, &b.limits.clone(), b)?;
                    if i.result.is_some() || vt(f, value)? != &actual.ty {
                        return Err(err("replacement RHS differs from the exact reserved owner"));
                    }
                }
                InstructionKind::AbortPlaceReservation { reservation } => {
                    inventory.get(reservation, b)?;
                    if i.result.is_some() {
                        return Err(err("abort unexpectedly produces a value"));
                    }
                }
                _ => {}
            }
        }
    }
    let schedule = inventory.schedule(f, graph, b)?;
    let mut points = std::collections::BTreeMap::new();
    for (bi, block) in f.blocks.iter().enumerate() {
        b.spend(block.instructions.len())?;
        for (at, i) in block.instructions.iter().enumerate() {
            if !schedule.retained[bi][at].is_empty() {
                b.spend(points.len().checked_ilog2().map_or(1, |n| n as usize + 1))?;
                points.insert(i as *const Instruction as usize, (bi, at));
            }
        }
    }
    b.stored_replacements[f.id.0 as usize] = Some(Rc::new(Context {
        inventory,
        schedule,
        points,
    }));
    Ok(())
}
pub(super) fn frame_work(rows: &[Prepared], b: &mut Budget) -> Check<()> {
    if rows.is_empty() {
        return Ok(());
    }
    b.spend(rows.len())?;
    for row in rows {
        permission_work(&row.permission, b)?;
    }
    Ok(())
}
pub(super) fn join(to: &mut Vec<Prepared>, from: &[Prepared], b: &mut Budget) -> Check<()> {
    for row in from {
        b.spend(to.len().saturating_add(1))?;
        if let Some(prior) = to.iter_mut().find(|old| old.id == row.id) {
            join_permission(&mut prior.permission, &row.permission, b)?;
        } else {
            permission_work(&row.permission, b)?;
            b.spend(to.len().saturating_add(1))?;
            to.push(row.clone());
        }
    }
    Ok(())
}
fn project(permission: &Permission, path: &[PlaceProjection], b: &mut Budget) -> Check<Permission> {
    permission_work(permission, b)?;
    let mut out = permission.clone();
    for source in &mut out.sources {
        let depth = source
            .inline
            .len()
            .saturating_add(source.heap_element.as_ref().map_or(0, Vec::len))
            .saturating_add(path.len());
        if depth > b.limits.max_call_depth {
            return Err(limit("replacement source path exceeds depth limit"));
        }
        b.spend(path.len())?;
        if let Some(last) = &mut source.heap_element {
            last.extend_from_slice(path);
        } else {
            source.inline.extend_from_slice(path);
        }
    }
    Ok(out)
}
fn permission(
    f: &TypedFunction,
    reservation: ReservationId,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Permission> {
    let cx = context(f, b)?;
    let place = cx.inventory.get(reservation, b)?;
    let initial = match place.origin {
        Origin::Local(local) => Permission {
            sources: vec![Source {
                retained_port: None,
                base: HoldRoot::Local(local),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            }],
            loans: Vec::new(),
        },
        Origin::Address(value) => {
            let Type::Address { mutable: true, .. } = vt(f, value)? else {
                return Err(err("replacement lost its actual mutable destination"));
            };
            permission_work(&values[value.0 as usize].address, b)?;
            values[value.0 as usize].address.clone()
        }
    };
    project(&initial, &place.steps, b)
}
fn held<'a>(
    state: &'a Frame,
    reservation: ReservationId,
    b: &mut Budget,
) -> Check<Option<&'a Prepared>> {
    b.spend(state.prepared.len().saturating_add(1))?;
    Ok(state.prepared.iter().find(|p| p.id == reservation))
}
// Preserve every unrelated field. Only one concrete destination permits a kill;
// a may-target keeps old and new leaves, never strongly overwrites alternatives.
fn publish(
    state: &mut Frame,
    destination: &Permission,
    rhs: &Payload,
    b: &mut Budget,
) -> Check<()> {
    let strong = destination.sources.len() == 1;
    for source in &destination.sources {
        b.spend(width(source))?;
        if source.heap_element.is_some()
            || !source.leaf.is_empty()
            || source.retained_port.is_some()
        {
            if !rhs.leaves.is_empty()
                || !rhs.address.sources.is_empty()
                || !rhs.task_ports.is_empty()
            {
                return Err(unsupported(
                    "replacement of borrowed content through a heap/retained target needs its actual slot protocol",
                    None,
                ));
            }
            // Closed owning content has no stored leaves to publish. Its
            // complete responsibility and retirement still run in resources/Sums.
            continue;
        }
        let holder = match source.base {
            HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
            HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
            HoldRoot::Capture(index) => state.captures.get_mut(index as usize),
            HoldRoot::Static => None,
        }
        .ok_or_else(|| err("replacement destination has no actual mutable holder"))?;
        if source.inline.is_empty() {
            if strong {
                *holder = copy_payload(rhs, b)?;
            } else {
                join_payload(holder, rhs, b)?;
            }
            continue;
        }
        if !rhs.address.sources.is_empty() || !rhs.store_description.sources.is_empty() {
            return Err(unsupported(
                "owning field replacement contains an unclassified top-level pointer payload",
                None,
            ));
        }
        if strong {
            for (path, _) in &holder.leaves {
                b.spend(
                    path.len()
                        .saturating_add(source.inline.len())
                        .saturating_add(1),
                )?;
            }
            holder
                .leaves
                .retain(|(path, _)| !path.starts_with(&source.inline), b)?;
            b.spend(holder.task_ports.len())?;
            for (path, _) in &holder.task_ports {
                b.spend(
                    path.len()
                        .saturating_add(source.inline.len())
                        .saturating_add(1),
                )?;
            }
            let prefix: Vec<_> = source
                .inline
                .iter()
                .cloned()
                .map(ContractProjection::Place)
                .collect();
            holder
                .task_ports
                .retain(|(path, _)| !path.starts_with(&prefix));
        }
        for (path, permission) in &rhs.leaves {
            b.spend(source.inline.len().saturating_add(path.len()))?;
            let mut complete = source.inline.clone();
            complete.extend_from_slice(path);
            add_leaf(holder, &complete, permission, b)?;
        }
        let mut nested = Payload::default();
        for (path, permission) in &rhs.task_ports {
            b.spend(source.inline.len().saturating_add(path.len()))?;
            let mut complete: Vec<_> = source
                .inline
                .iter()
                .cloned()
                .map(ContractProjection::Place)
                .collect();
            complete.extend_from_slice(path);
            permission_work(permission, b)?;
            nested.task_ports.push((complete, permission.clone()));
        }
        task_address::join(holder, &nested, b)?;
    }
    if strong {
        for old in &state.vacant {
            b.spend(width(old).saturating_add(width(&destination.sources[0])))?;
        }
        let target = &destination.sources[0];
        state.vacant.retain(|old| {
            !(old.base == target.base
                && old.leaf == target.leaf
                && old.heap_parents == target.heap_parents
                && old.heap_element == target.heap_element
                && old.inline.starts_with(&target.inline))
        });
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn step(
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    sites: &WriteSites,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<()> {
    match i.kind {
        InstructionKind::ReservePlaceLocal { reservation, .. }
        | InstructionKind::ReservePlaceAddress { reservation, .. } => {
            let permission = permission(f, reservation, values, b)?;
            b.spend(state.prepared.len().saturating_add(1))?;
            if let Some(old) = state.prepared.iter_mut().find(|p| p.id == reservation) {
                old.permission = permission;
            } else {
                state.prepared.push(Prepared {
                    id: reservation,
                    permission,
                });
            }
        }
        InstructionKind::CommitPlaceReplace { reservation, value } => {
            let mut rhs = copy_payload(&values[value.0 as usize], b)?;
            if let Some((_, shape)) = qualified(vt(f, value)?) {
                for slot in &shape.slots {
                    if slot.mode == StoredBorrowMode::ExclusiveAddress
                        && let Some(row) = rhs.leaves.position(&slot.path, b)?
                    {
                        let prior = rhs.leaves.permission_mut(row);
                        *prior =
                            retain_written_slot(prior, i, u32::MAX, &slot.path, sites, loans, b)?;
                    }
                }
            }
            let target = if let Some(prepared) = held(state, reservation, b)? {
                permission_work(&prepared.permission, b)?;
                prepared.permission.clone()
            } else {
                permission(f, reservation, values, b)?
            }; // bottom during reverse batches only
            publish(state, &target, &rhs, b)?;
            b.spend(state.prepared.len())?;
            state.prepared.retain(|p| p.id != reservation);
        }
        InstructionKind::AbortPlaceReservation { reservation } => {
            b.spend(state.prepared.len())?;
            state.prepared.retain(|p| p.id != reservation);
        }
        _ => return Err(err("non-replacement instruction in replacement transfer")),
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    f: &TypedFunction,
    i: &Instruction,
    state: &Frame,
    values: &[Payload],
    active: &[usize],
    loans: &[Loan],
    requirements: &mut Vec<(Source, Source)>,
    writes: &mut Vec<(Source, bool)>,
    b: &mut Budget,
) -> Check<()> {
    let reservation = match i.kind {
        InstructionKind::ReservePlaceLocal { reservation, .. }
        | InstructionKind::ReservePlaceAddress { reservation, .. } => {
            let actual = permission(f, reservation, values, b)?;
            if actual.sources.is_empty() {
                return Err(err("replacement reserve has no actual destination"));
            }
            store_paths::writable(&actual, loans, b)?;
            return Ok(());
        }
        InstructionKind::CommitPlaceReplace { reservation, .. }
        | InstructionKind::AbortPlaceReservation { reservation } => reservation,
        _ => return Ok(()),
    };
    let prepared = held(state, reservation, b)?
        .ok_or_else(|| err("replacement consumes no actual prepared permission"))?;
    if let InstructionKind::CommitPlaceReplace { value, .. } = i.kind {
        let mut all = active.to_vec();
        b.spend(active.len())?;
        let rhs = &values[value.0 as usize];
        add_loans(&mut all, &rhs.address.loans, b)?;
        for (_, p) in &rhs.leaves {
            add_loans(&mut all, &p.loans, b)?;
        }
        for (_, p) in &rhs.task_ports {
            add_loans(&mut all, &p.loans, b)?;
        }
        let all = ancestry(&all, loans, b)?;
        store_paths::writable(&prepared.permission, loans, b)?;
        let mut access = access_candidates::Access::new(&all, loans, b)?;
        access.check(&prepared.permission, true, requirements, b)?;
        store_paths::record(writes, &prepared.permission, true, b)?;
    }
    Ok(())
}

fn pins<'a>(cx: &'a Context, i: &Instruction, b: &mut Budget) -> Check<&'a [ValueId]> {
    b.spend(
        cx.points
            .len()
            .checked_ilog2()
            .map_or(1, |n| n as usize + 1),
    )?;
    Ok(
        if let Some((block, at)) = cx.points.get(&(i as *const Instruction as usize)) {
            &cx.schedule.retained[*block][*at]
        } else {
            &[]
        },
    )
}
pub(super) fn pin(
    f: &TypedFunction,
    i: &Instruction,
    live: &mut WordRow,
    b: &mut Budget,
) -> Check<()> {
    if !selected(f, b)? {
        return Ok(());
    }
    let cx = context(f, b)?;
    for value in pins(&cx, i, b)? {
        b.spend(1)?;
        live.assign(value.0 as usize, true)?;
    }
    Ok(())
}
#[cfg(test)]
pub(super) fn pin_bool(
    f: &TypedFunction,
    i: &Instruction,
    live: &mut [bool],
    b: &mut Budget,
) -> Check<()> {
    if !selected(f, b)? {
        return Ok(());
    }
    let cx = context(f, b)?;
    for value in pins(&cx, i, b)? {
        b.spend(1)?;
        live[value.0 as usize] = true;
    }
    Ok(())
}
pub(super) fn entry(f: &TypedFunction, block: usize, state: &Frame, b: &mut Budget) -> Check<()> {
    if !selected(f, b)? {
        return Ok(());
    }
    let cx = context(f, b)?;
    let expected = cx.schedule.entries[block]
        .as_ref()
        .ok_or_else(|| err("replacement has no reachable actual CFG entry"))?;
    b.spend(
        expected
            .len()
            .saturating_add(state.prepared.len())
            .saturating_add(1),
    )?;
    if expected.len() != state.prepared.len()
        || state.prepared.iter().any(|p| !expected.contains(&p.id))
    {
        return Err(err(
            "prepared replacement permissions differ from actual CFG activation",
        ));
    }
    Ok(())
}
