//! Raw frame domains retain each complete child domain before composing the
//! header's outside images. No row in this module is a root or a WF receipt.
use super::*;

pub(crate) struct Frame {
    pub(crate) shape: RetainedLifeShape,
    pub(crate) sources: Vec<RetainedPortBinding>,
    pub(crate) parameters: Vec<StorageLifeShape>,
    pub(crate) result: StorageLifeShape,
    pub(crate) maps: Vec<Vec<u32>>,
}
/// The raw joint directory is an untrusted declaration. Each child's own
/// domain remains intact even when the declared parent images coincide.
pub(crate) fn frame(
    module: &TypedModule,
    f: &TypedFunction,
    declaration: &Type,
    b: &mut Budget,
) -> Check<Frame> {
    let Type::Task {
        result,
        retained_life,
    } = declaration
    else {
        return Err(bad("raw Task frame selects another declared constructor"));
    };
    dynamic_metadata::ty(declaration, b)?;
    if **result != f.return_type {
        return Err(bad("raw Task frame changes its complete worker result"));
    }
    if retained_life.binder_count as usize > b.limits.max_nodes {
        return Err(limit("raw Task joint frame domain exceeded"));
    }
    b.spend(retained_life.binder_count as usize)?;
    let mut parent = vec![None; retained_life.binder_count as usize];
    let mut parameters = Vec::new();
    let mut maps = Vec::new();
    let mut rebuilt = empty_context();
    let mut sources = Vec::new();
    for (ordinal, parameter) in f.parameters.iter().enumerate() {
        let ty = local_type(f, *parameter, Some(f.span))?;
        let child = life(module, ty, f, StorageBase::Parameter(ordinal as u32), b, 0)?;
        b.spend(child.binder_count as usize)?;
        let mut map = Vec::new();
        let mut injective = BTreeSet::new();
        for class in 0..child.binder_count {
            b.spend(retained_life.retained.len())?;
            let path = vec![LifeTypeProjection::Parameter(ordinal as u32)];
            let mut rows = retained_life
                .retained
                .iter()
                .filter(|row| row.path == path && row.class == class);
            let row = rows
                .next()
                .ok_or_else(|| bad("raw Task joint declaration omits a whole input class"))?;
            if rows.next().is_some() || !injective.insert(row.binder) {
                return Err(bad(
                    "raw Task joint declaration duplicates or coalesces an intrinsic child class",
                ));
            }
            let image = image(f, StorageBase::Parameter(ordinal as u32), &child, class, b)?;
            let slot = parent
                .get_mut(row.binder as usize)
                .ok_or_else(|| bad("raw Task child map escapes its declared full context"))?;
            if slot.is_some_and(|known| known != image) {
                return Err(bad(
                    "raw Task joint embedding disagrees with its complete worker header",
                ));
            }
            *slot = Some(image);
            map.push(row.binder);
            rebuilt.retained.push(row.clone());
        }
        for row in &child.nominal_arguments {
            b.spend(row.path.len().saturating_add(1))?;
            let mut row = row.clone();
            row.path
                .insert(0, LifeTypeProjection::Parameter(ordinal as u32));
            row.binder = *map
                .get(row.binder as usize)
                .ok_or_else(|| bad("raw Task phantom occurrence loses its complete child map"))?;
            rebuilt.nominal_arguments.push(row);
        }
        for (leaf, mode, class, path) in
            crate::semantic::task_domains::raw_input_ports(module, ty, &child, b)?
        {
            let binder = *map
                .get(class as usize)
                .ok_or_else(|| bad("raw Task port loses its complete child class"))?;
            let port = rebuilt.ports.len() as u32;
            rebuilt
                .ports
                .push(RetainedBorrowPort { leaf, mode, binder });
            sources.push(RetainedPortBinding {
                port,
                source: StoragePosition {
                    base: StorageBase::Parameter(ordinal as u32),
                    path,
                },
            });
        }
        parameters.push(child);
        maps.push(map);
    }
    if parent.iter().any(Option::is_none) {
        return Err(bad(
            "raw Task joint domain contains an ungrounded or redundant parent class",
        ));
    }
    let result = life(module, &f.return_type, f, StorageBase::Result, b, 0)?;
    let mut injective = BTreeSet::new();
    for class in 0..result.binder_count {
        b.spend(retained_life.retained.len())?;
        let mut rows = retained_life
            .retained
            .iter()
            .filter(|row| row.path == [LifeTypeProjection::Result] && row.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("raw Task joint declaration omits a complete result class"))?;
        if rows.next().is_some() || !injective.insert(row.binder) {
            return Err(bad(
                "raw Task result map coalesces independent intrinsic classes",
            ));
        }
        let image = image(f, StorageBase::Result, &result, class, b)?;
        if parent.get(row.binder as usize) != Some(&Some(image)) {
            return Err(bad(
                "raw Task result embedding disagrees with its full worker header",
            ));
        }
        rebuilt.retained.push(row.clone());
    }
    rebuilt.binder_count = retained_life.binder_count;
    rebuilt.retained.sort();
    rebuilt.nominal_arguments.sort();
    dynamic_metadata::retained_shape(&rebuilt, b)?;
    if rebuilt != *retained_life {
        return Err(bad(
            "raw Task declaration changes its full input/result/port/phantom occurrence directory",
        ));
    }
    // Actual return sources, every worker PC and every constructor's operands
    // remain mandatory in the ordinary stored/resource/Task passes.
    Ok(Frame {
        shape: rebuilt,
        sources,
        parameters,
        result,
        maps,
    })
}
fn image(
    f: &TypedFunction,
    base: StorageBase,
    life: &StorageLifeShape,
    class: u32,
    b: &mut Budget,
) -> Check<StorageBinder> {
    let header = f
        .storage_life
        .as_ref()
        .ok_or_else(|| bad("raw Task nonzero child has no complete worker header"))?;
    signature_life::charge(header, b)?;
    let mut found = None;
    let mut bind = |image: StorageBinder| -> Check<()> {
        if found.is_some_and(|known| known != image) {
            return Err(bad(
                "raw Task child occurrences disagree within the worker header",
            ));
        }
        found = Some(image);
        Ok(())
    };
    for row in &header.classes {
        if row.position.base == base && row.position.path.is_empty() && row.position.class == class
        {
            bind(row.binder)?;
        }
    }
    for slot in &life.slots {
        if slot.binder != class {
            continue;
        }
        b.spend(slot.path.len().saturating_add(header.bindings.len()))?;
        let path = slot
            .path
            .iter()
            .copied()
            .map(ContractProjection::Place)
            .collect::<Vec<_>>();
        for row in &header.bindings {
            if row.position.base == base && row.position.path == path {
                bind(row.binder)?;
            }
        }
    }
    for child in &life.retained {
        if child.binder != class {
            continue;
        }
        b.spend(header.classes.len().saturating_add(child.path.len()))?;
        for row in &header.classes {
            if row.position.base == base
                && row.position.path == child.path
                && row.position.class == child.class
            {
                bind(row.binder)?;
            }
        }
    }
    found.ok_or_else(|| bad("raw Task whole class has no exact complete worker-header occurrence"))
}

pub(crate) fn life(
    module: &TypedModule,
    ty: &Type,
    f: &TypedFunction,
    base: StorageBase,
    b: &mut Budget,
    depth: usize,
) -> Check<StorageLifeShape> {
    dynamic_metadata::ty(ty, b)?;
    if depth > b.limits.max_call_depth.min(128) {
        return Err(limit("raw Task complete child domain depth exceeded"));
    }
    let mut shape = match ty {
        Type::BorrowedStorage { life, .. } => life.clone(),
        Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
            StorageLifeShape {
                binder_count: retained_life.binder_count,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            }
        }
        Type::Closure(id) => {
            b.spend(module.closures.len())?;
            let mut rows = module.closures.iter().filter(|row| row.id == *id);
            let closure = rows
                .next()
                .ok_or_else(|| bad("raw Task closure child is absent"))?;
            if rows.next().is_some() {
                return Err(bad("raw Task closure child identity is duplicated"));
            }
            let header = closure
                .storage_life
                .as_ref()
                .ok_or_else(|| bad("raw Task closure full domain is Unknown"))?;
            signature_life::charge(header, b)?;
            StorageLifeShape {
                binder_count: header.fixed_binder_count,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            }
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            b.spend(module.dyn_interfaces.len())?;
            let mut rows = module
                .dyn_interfaces
                .iter()
                .filter(|row| row.id == *interface);
            let interface = rows
                .next()
                .ok_or_else(|| bad("raw Task opaque child is absent"))?;
            if rows.next().is_some() {
                return Err(bad("raw Task opaque child identity is duplicated"));
            }
            StorageLifeShape {
                binder_count: interface.retained_life.binder_count,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            }
        }
        Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner) => {
            let child = life(module, inner, f, base, b, depth + 1)?;
            let outer = matches!(ty, Type::Address { .. } | Type::Desc(_));
            let static_outer = outer
                && depth == 0
                && f.storage_life.as_ref().is_some_and(|header| {
                    header
                        .static_bindings
                        .contains(&StoragePosition { base, path: vec![] })
                });
            let offset = u32::from(outer && !static_outer);
            let mut parent = StorageLifeShape {
                binder_count: child
                    .binder_count
                    .checked_add(offset)
                    .ok_or_else(|| limit("raw Task child domain overflow"))?,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            };
            if offset != 0 {
                parent.slots.push(StorageLifeSlot {
                    path: vec![],
                    binder: 0,
                    mode: match ty {
                        Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                        Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
                        _ => StoredBorrowMode::ReadOnlyDescription,
                    },
                });
            }
            b.spend(child.binder_count as usize)?;
            for class in 0..child.binder_count {
                parent.retained.push(StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Pointee],
                    class,
                    binder: class + offset,
                });
            }
            for row in &child.nominal_arguments {
                b.spend(row.path.len().saturating_add(1))?;
                let mut row = row.clone();
                row.path.insert(0, LifeTypeProjection::Pointee);
                row.binder += offset;
                parent.nominal_arguments.push(row);
            }
            parent
        }
        _ => match input_scope::domain(ty, b)? {
            input_scope::Domain::Known(0) => StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            _ => {
                return Err(unsupported(
                    "raw Task child requires its complete qualified domain",
                    Some(f.span),
                ));
            }
        },
    };
    if shape.binder_count as usize > b.limits.max_nodes {
        return Err(limit("raw Task child full domain exceeded"));
    }
    b.spend(shape.binder_count as usize)?;
    shape.retained.sort();
    shape.nominal_arguments.sort();
    Ok(shape)
}
