//! Reconstruct logical port identity from the complete actual Context. The
//! physical paths are used only to validate Context, never as opaque Self GEPs.
use super::*;

pub(super) struct Port {
    pub leaf: Type,
    pub mode: StoredBorrowMode,
    pub binder: u32,
    pub inner: Vec<(u32, u32)>,
}
pub(super) fn check(
    module: &TypedModule,
    interface: &DynInterface,
    mut domain: impl FnMut(&Type, &mut Budget) -> Check<u32>,
    b: &mut Budget,
) -> Check<Vec<Port>> {
    let Some(context) = &interface.context else {
        let life = &interface.retained_life;
        if life.binder_count != 0
            || !life.ports.is_empty()
            || !life.retained.is_empty()
            || !life.nominal_arguments.is_empty()
        {
            return Err(bad(
                "opaque interface without Context invents an internal class or port",
            ));
        }
        return Ok(vec![]);
    };
    let limits = b.limits.clone();
    stored_flow::qualification(module, context, &limits, b)?;
    let Type::BorrowedStorage { storage, life } = context else {
        return Err(bad(
            "opaque Context requires its complete actual qualified type",
        ));
    };
    dynamic_metadata::ty(context, b)?;
    dynamic_metadata::retained_shape(&interface.retained_life, b)?;
    if life.binder_count != interface.retained_life.binder_count
        || life.retained != interface.retained_life.retained
        || life.nominal_arguments != interface.retained_life.nominal_arguments
    {
        return Err(bad("opaque Context class and nominal inventories disagree"));
    }
    b.spend(life.slots.len())?;
    let mut ports: Vec<Port> = vec![];
    for slot in &life.slots {
        let leaf = dynamic_ports::physical_path(storage, &slot.path, b)?;
        let inner = match (leaf, slot.mode) {
            (
                Type::Address {
                    mutable: false,
                    inner,
                },
                StoredBorrowMode::SharedAddress,
            )
            | (
                Type::Address {
                    mutable: true,
                    inner,
                },
                StoredBorrowMode::ExclusiveAddress,
            )
            | (
                Type::Desc(inner),
                StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription,
            ) => inner,
            _ => return Err(bad("opaque port mode differs from the actual Context leaf")),
        };
        let count = domain(inner, b)?;
        if count as usize > b.limits.max_nodes {
            return Err(limit(
                "opaque port inner class domain exceeds selected bound",
            ));
        }
        b.spend(count as usize + slot.path.len())?;
        let mut prefix = slot
            .path
            .iter()
            .copied()
            .map(LifeTypeProjection::Place)
            .collect::<Vec<_>>();
        prefix.push(LifeTypeProjection::Pointee);
        let mut mapping = Vec::with_capacity(count as usize);
        for class in 0..count {
            b.spend(life.retained.len())?;
            let mut rows = life
                .retained
                .iter()
                .filter(|row| row.path == prefix && row.class == class);
            let row = rows
                .next()
                .ok_or_else(|| bad("opaque port loses an inner class or phantom occurrence"))?;
            if rows.next().is_some() || row.binder >= life.binder_count {
                return Err(bad("opaque port repeats or misbinds an inner occurrence"));
            }
            mapping.push((class, row.binder));
        }
        let mut duplicate = false;
        for previous in &ports {
            dynamic_metadata::ty(&previous.leaf, b)?;
            dynamic_metadata::ty(leaf, b)?;
            b.spend(
                previous
                    .inner
                    .len()
                    .saturating_add(mapping.len())
                    .saturating_add(1),
            )?;
            if previous.leaf == *leaf
                && previous.mode == slot.mode
                && previous.binder == slot.binder
                && previous.inner == mapping
            {
                duplicate = true;
                break;
            }
        }
        if duplicate {
            continue;
        }
        let claimed = interface
            .retained_life
            .ports
            .get(ports.len())
            .ok_or_else(|| bad("opaque port directory coalesces distinct inner class mappings"))?;
        dynamic_metadata::ty(&claimed.leaf, b)?;
        dynamic_metadata::ty(leaf, b)?;
        if claimed.leaf != *leaf || claimed.mode != slot.mode || claimed.binder != slot.binder {
            return Err(bad(
                "opaque logical port order or whole leaf type is not its actual Context",
            ));
        }
        ports.push(Port {
            leaf: leaf.clone(),
            mode: slot.mode,
            binder: slot.binder,
            inner: mapping,
        });
    }
    if ports.len() != interface.retained_life.ports.len() {
        return Err(bad("opaque Context has extra or missing logical ports"));
    }
    Ok(ports)
}
