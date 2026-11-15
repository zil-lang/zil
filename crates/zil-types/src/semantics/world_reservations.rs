//! Replacement tickets carry a complete type equation in each reaching world.
//! These records grant no pin, management authority or resource phase; those
//! actual obligations remain independently checked from the real operations.
use super::*;
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Local(LocalId),
    Address(ValueId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Reservation {
    pub target: Target,
    pub path: Vec<PlaceProjection>,
    pub parent: SharedType,
    pub expected: SharedType,
    pub roots: BTreeSet<arguments::Root>,
}
pub(super) fn snapshot(world: &mut World<'_, '_>, value: &Reservation) -> R<Reservation> {
    world.pay(value.path.len().saturating_add(3))?;
    let roots = arguments::copy_roots(world, &value.roots)?;
    Ok(Reservation {
        target: value.target.clone(),
        path: value.path.clone(),
        parent: value.parent.clone(),
        expected: value.expected.clone(),
        roots,
    })
}
pub(super) fn charge(world: &mut World<'_, '_>, value: &Reservation) -> R<()> {
    world.pay(value.path.len().saturating_add(3))?;
    crate::dyn_metadata::semantic_operand_snapshot(
        value.parent.as_ref(),
        world.limits,
        world.left,
        1,
    )?;
    crate::dyn_metadata::semantic_operand_snapshot(
        value.expected.as_ref(),
        world.limits,
        world.left,
        1,
    )?;
    for root in &value.roots {
        world.pay(root.path.len().saturating_add(1))?;
    }
    Ok(())
}
fn roots_live(
    world: &mut World<'_, '_>,
    roots: &BTreeSet<arguments::Root>,
    locals: &[Option<SharedType>],
) -> R<()> {
    for root in roots {
        world.pay(
            world
                .vacant
                .len()
                .saturating_mul(root.path.len().saturating_add(1))
                .saturating_add(1),
        )?;
        if world.stale_locals.contains(&root.local)
            || locals
                .get(root.local.0 as usize)
                .and_then(Option::as_ref)
                .is_none()
            || world.vacant.iter().any(|(local, path)| {
                *local == root.local
                    && (path.starts_with(&root.path) || root.path.starts_with(path))
            })
        {
            return Err(bad(
                "replacement equation has an inactive, stale or vacant actual target root",
            ));
        }
    }
    Ok(())
}
fn target(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    target: &Target,
    path: &[PlaceProjection],
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
) -> R<(SharedType, SharedType, BTreeSet<arguments::Root>)> {
    match target {
        Target::Local(local) => {
            places::local_mutable(world, function, *local, true)?;
            let expected = world.projected_local(locals, *local, path)?;
            let parent = locals
                .get(local.0 as usize)
                .and_then(Option::as_ref)
                .ok_or_else(|| bad("replacement local has no complete current shell"))?;
            let parent = world.share(parent)?;
            world.pay(path.len().saturating_add(1))?;
            let roots = BTreeSet::from([arguments::Root {
                local: *local,
                path: path.to_vec(),
            }]);
            Ok((parent, expected, roots))
        }
        Target::Address(address) => {
            let parent = world.get(values, *address)?;
            places::address_mutable(world, &parent, true)?;
            let pointee = world.pointee(&parent)?;
            let expected = places::path(world, &pointee, path)?;
            let roots = arguments::roots(world, *address, &parent)?;
            let mut selected = BTreeSet::new();
            for mut root in roots {
                let depth = root.path.len().saturating_add(path.len());
                if depth > world.limits.max_call_depth {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "replacement root projection depth exceeded",
                        None,
                    ));
                }
                world.pay(depth.saturating_add(1))?;
                root.path.extend_from_slice(path);
                selected.insert(root);
            }
            Ok((parent, expected, selected))
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn reserve(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    id: ReservationId,
    root: Target,
    path: &[PlaceProjection],
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
) -> R<()> {
    world.pay(index::search(world.reservations.len()))?;
    if world.reservations.contains_key(&id) {
        return Err(bad("one actual replacement ticket is already active"));
    }
    let (parent, expected, roots) = target(world, function, &root, path, values, locals)?;
    roots_live(world, &roots, locals)?;
    world.pay(
        path.len()
            .saturating_add(index::search(world.reservations.len()))
            .saturating_add(1),
    )?;
    world.reservations.insert(
        id,
        Reservation {
            target: root,
            path: path.to_vec(),
            parent,
            expected,
            roots,
        },
    );
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn commit(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    function: &TypedFunction,
    id: ReservationId,
    value: ValueId,
    values: &[Option<SharedType>],
    locals: &[Option<SharedType>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<()> {
    world.pay(index::search(world.reservations.len()))?;
    let reservation = world
        .reservations
        .remove(&id)
        .ok_or_else(|| bad("actual replacement commit has no active ticket equation"))?;
    roots_live(world, &reservation.roots, locals)?;
    let (parent, expected, roots) = target(
        world,
        function,
        &reservation.target,
        &reservation.path,
        values,
        locals,
    )?;
    world.same(instance.id, &parent, &reservation.parent, implicit)?;
    world.same(instance.id, &expected, &reservation.expected, implicit)?;
    world.pay(roots.len().saturating_add(reservation.roots.len()))?;
    for root in roots.iter().chain(&reservation.roots) {
        world.pay(root.path.len())?;
    }
    if roots != reservation.roots {
        return Err(bad(
            "replacement commit changes its exact prepared target roots",
        ));
    }
    let actual = world.get(values, value)?;
    world.same(instance.id, &actual, &reservation.expected, implicit)
}
pub(super) fn abort(world: &mut World<'_, '_>, id: ReservationId) -> R<()> {
    world.pay(index::search(world.reservations.len()))?;
    if world.reservations.remove(&id).is_none() {
        return Err(bad(
            "actual replacement abort has no active ticket equation",
        ));
    }
    Ok(())
}
pub(super) fn exit(world: &mut World<'_, '_>) -> R<()> {
    world.pay(1)?;
    if !world.reservations.is_empty() {
        return Err(bad(
            "actual function return leaves an unsettled replacement equation",
        ));
    }
    Ok(())
}
