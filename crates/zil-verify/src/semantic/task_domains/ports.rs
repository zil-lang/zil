//! Canonical declaration-tree enumeration of actual stored borrow occurrences.
//! Full class maps are metadata; none of these selectors supplies a target.
use super::*;
type Port = (Type, StoredBorrowMode, u32, Vec<ContractProjection>);

pub(super) fn expand(
    module: &TypedModule,
    ty: &Type,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> Check<Vec<Port>> {
    charge_life(life, b)?;
    b.spend(life.binder_count as usize)?;
    let map = (0..life.binder_count).collect::<Vec<_>>();
    let mut walker = Walker {
        module,
        ports: Vec::new(),
        active: Vec::new(),
    };
    walker.node(ty, life, &map, &[], &[], &[], 0, b)?;
    Ok(walker.ports)
}
struct Walker<'a> {
    module: &'a TypedModule,
    ports: Vec<Port>,
    active: Vec<ClosureId>,
}
impl Walker<'_> {
    fn push(
        &mut self,
        leaf: &Type,
        mode: StoredBorrowMode,
        class: u32,
        path: &[ContractProjection],
        b: &mut Budget,
    ) -> Check<()> {
        dynamic_metadata::ty(leaf, b)?;
        b.spend(path.len().saturating_add(1))?;
        if self.ports.len() >= b.limits.max_nodes {
            return Err(limit("Task complete port occurrence inventory exceeded"));
        }
        self.ports.push((leaf.clone(), mode, class, path.to_vec()));
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        ty: &Type,
        life: &StorageLifeShape,
        map: &[u32],
        semantic: &[LifeTypeProjection],
        physical: &[PlaceProjection],
        source: &[ContractProjection],
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        self.depth(depth, b)?;
        match ty {
            Type::BorrowedStorage {
                storage,
                life: child,
            } => {
                if semantic.is_empty() {
                    if life != child {
                        return Err(bad(
                            "Task input qualifier changes its full local class directory",
                        ));
                    }
                    self.node(storage, life, map, &[], physical, source, depth + 1, b)
                } else {
                    let child_map = boundary(life, map, semantic, child.binder_count, b)?;
                    self.node(storage, child, &child_map, &[], &[], source, depth + 1, b)
                }
            }
            Type::Address { .. } | Type::Desc(_) => {
                let mode = match ty {
                    Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                    Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
                    _ => StoredBorrowMode::ReadOnlyDescription,
                };
                b.spend(life.slots.len().saturating_add(physical.len()))?;
                let mut rows = life.slots.iter().filter(|row| row.path == physical);
                if let Some(row) = rows.next() {
                    if rows.next().is_some()
                        || !matches!(
                            (mode, row.mode),
                            (
                                StoredBorrowMode::ReadOnlyDescription,
                                StoredBorrowMode::ReadOnlyDescription
                                    | StoredBorrowMode::WritableDescription
                            ) | (
                                StoredBorrowMode::SharedAddress,
                                StoredBorrowMode::SharedAddress
                            ) | (
                                StoredBorrowMode::ExclusiveAddress,
                                StoredBorrowMode::ExclusiveAddress
                            )
                        )
                    {
                        return Err(bad(
                            "Task input borrow occurrence has an ambiguous complete mode",
                        ));
                    }
                    let class = *map
                        .get(row.binder as usize)
                        .ok_or_else(|| bad("Task input borrow escapes its full domain map"))?;
                    self.push(ty, row.mode, class, source, b)?;
                }
                Ok(())
            }
            Type::Managed(inner) => {
                // An owning pointer does not turn metadata into a physical
                // field address. Only an existing opaque directory is exposed.
                if opaque(inner) {
                    let mut next = semantic.to_vec();
                    b.spend(next.len().saturating_add(1))?;
                    next.push(LifeTypeProjection::Pointee);
                    self.node(inner, life, map, &next, physical, source, depth + 1, b)
                } else {
                    Ok(())
                }
            }
            Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
                self.opaque(retained_life, life, map, semantic, source, b)
            }
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                b.spend(self.module.dyn_interfaces.len())?;
                let mut rows = self
                    .module
                    .dyn_interfaces
                    .iter()
                    .filter(|row| row.id == *interface);
                let interface = rows
                    .next()
                    .ok_or_else(|| bad("Task opaque port interface is absent"))?;
                if rows.next().is_some() {
                    return Err(bad("Task opaque port interface is duplicated"));
                }
                self.opaque(&interface.retained_life, life, map, semantic, source, b)
            }
            Type::Closure(id) => {
                if self.active.contains(id) {
                    return Err(bad("Task logical capture port graph is recursive"));
                }
                b.spend(self.module.closures.len().saturating_add(self.active.len()))?;
                let mut rows = self.module.closures.iter().filter(|row| row.id == *id);
                let closure = rows
                    .next()
                    .ok_or_else(|| bad("Task input closure has no complete capture inventory"))?;
                if rows.next().is_some() {
                    return Err(bad("Task input closure identity is duplicated"));
                }
                let header = closure
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| bad("Task input closure complete domain is Unknown"))?;
                signature_life::charge(header, b)?;
                let closure_map = boundary(life, map, semantic, header.fixed_binder_count, b)?;
                self.active.push(*id);
                let result = (|| {
                    for (index, capture) in closure.captures.iter().enumerate() {
                        b.spend(source.len().saturating_add(1))?;
                        let mut path = source.to_vec();
                        path.push(ContractProjection::Capture(index as u32));
                        self.header_node(
                            capture,
                            header,
                            StorageBase::Capture(index as u32),
                            &closure_map,
                            &[],
                            &[],
                            &path,
                            depth + 1,
                            b,
                        )?;
                    }
                    Ok(())
                })();
                self.active.pop();
                result
            }
            Type::Tuple(fields) => {
                for (index, field) in fields.iter().enumerate() {
                    self.child(
                        field,
                        life,
                        map,
                        semantic,
                        physical,
                        source,
                        LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),
                        PlaceProjection::Field(index as u32),
                        depth,
                        b,
                    )?;
                }
                Ok(())
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, field)) in fields.iter().enumerate() {
                    self.child(
                        field,
                        life,
                        map,
                        semantic,
                        physical,
                        source,
                        LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),
                        PlaceProjection::Field(index as u32),
                        depth,
                        b,
                    )?;
                }
                Ok(())
            }
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, field) in fields.iter().enumerate() {
                        let projection = PlaceProjection::Variant(VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        });
                        self.child(
                            field,
                            life,
                            map,
                            semantic,
                            physical,
                            source,
                            LifeTypeProjection::Place(projection),
                            projection,
                            depth,
                            b,
                        )?;
                    }
                }
                Ok(())
            }
            Type::ErrorUnion { error, result } => {
                for (arm, field) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let projection = PlaceProjection::ErrorArm(arm);
                    self.child(
                        field,
                        life,
                        map,
                        semantic,
                        physical,
                        source,
                        LifeTypeProjection::Place(projection),
                        projection,
                        depth,
                        b,
                    )?;
                }
                Ok(())
            }
            Type::Array { element, length } => {
                if *length as usize > b.limits.max_nodes {
                    return Err(limit("Task array port occurrence domain exceeded"));
                }
                b.spend(*length as usize)?;
                for index in 0..*length {
                    self.child(
                        element,
                        life,
                        map,
                        semantic,
                        physical,
                        source,
                        LifeTypeProjection::Element,
                        PlaceProjection::Field(index),
                        depth,
                        b,
                    )?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn opaque(
        &mut self,
        shape: &RetainedLifeShape,
        life: &StorageLifeShape,
        map: &[u32],
        semantic: &[LifeTypeProjection],
        source: &[ContractProjection],
        b: &mut Budget,
    ) -> Check<()> {
        dynamic_metadata::retained_shape(shape, b)?;
        let child = boundary(life, map, semantic, shape.binder_count, b)?;
        for (index, port) in shape.ports.iter().enumerate() {
            b.spend(source.len().saturating_add(1))?;
            let mut path = source.to_vec();
            path.push(ContractProjection::RetainedPort(index as u32));
            let class = *child
                .get(port.binder as usize)
                .ok_or_else(|| bad("Task opaque port escapes its complete child domain"))?;
            self.push(&port.leaf, port.mode, class, &path, b)?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn child(
        &mut self,
        ty: &Type,
        life: &StorageLifeShape,
        map: &[u32],
        semantic: &[LifeTypeProjection],
        physical: &[PlaceProjection],
        source: &[ContractProjection],
        semantic_edge: LifeTypeProjection,
        physical_edge: PlaceProjection,
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(
            semantic
                .len()
                .saturating_add(physical.len())
                .saturating_add(source.len())
                .saturating_add(3),
        )?;
        let mut sem = semantic.to_vec();
        sem.push(semantic_edge);
        let mut phy = physical.to_vec();
        phy.push(physical_edge);
        let mut src = source.to_vec();
        src.push(ContractProjection::Place(physical_edge));
        self.node(ty, life, map, &sem, &phy, &src, depth + 1, b)
    }
    #[allow(clippy::too_many_arguments)]
    fn header_node(
        &mut self,
        ty: &Type,
        header: &StorageLifeContract,
        base: StorageBase,
        map: &[u32],
        semantic: &[LifeTypeProjection],
        contract: &[ContractProjection],
        source: &[ContractProjection],
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        self.depth(depth, b)?;
        match ty {
            Type::Address { .. } | Type::Desc(_) => {
                b.spend(
                    header
                        .bindings
                        .len()
                        .saturating_add(header.static_bindings.len())
                        .saturating_add(contract.len()),
                )?;
                let position = StoragePosition {
                    base,
                    path: contract.to_vec(),
                };
                let mut rows = header
                    .bindings
                    .iter()
                    .filter(|row| row.position == position);
                if let Some(row) = rows.next() {
                    if rows.next().is_some() {
                        return Err(bad("Task capture borrow occurrence is duplicated"));
                    }
                    let StorageBinder::Fixed(class) = row.binder else {
                        return Err(bad(
                            "Task capture borrow escapes its persistent fixed domain",
                        ));
                    };
                    let class = *map.get(class as usize).ok_or_else(|| {
                        bad("Task capture borrow escapes its full closure domain")
                    })?;
                    let mode = match ty {
                        Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                        Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
                        _ => StoredBorrowMode::ReadOnlyDescription,
                    };
                    self.push(ty, mode, class, source, b)
                } else if header.static_bindings.contains(&position) {
                    Ok(())
                } else {
                    Err(bad(
                        "Task capture borrow has neither a variable occurrence nor a pending static declaration",
                    ))
                }
            }
            Type::BorrowedStorage { life, .. } => {
                let child = header_map(header, base, map, semantic, life.binder_count, b)?;
                self.node(ty, life, &child, &[], &[], source, depth + 1, b)
            }
            Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
                let child = header_map(header, base, map, semantic, retained_life.binder_count, b)?;
                let life = empty_domain(retained_life.binder_count);
                self.node(ty, &life, &child, &[], &[], source, depth + 1, b)
            }
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                b.spend(self.module.dyn_interfaces.len())?;
                let mut rows = self
                    .module
                    .dyn_interfaces
                    .iter()
                    .filter(|row| row.id == *interface);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("Task captured opaque interface is absent"))?;
                if rows.next().is_some() {
                    return Err(bad("Task captured opaque interface is duplicated"));
                }
                let count = row.retained_life.binder_count;
                let child = header_map(header, base, map, semantic, count, b)?;
                self.node(
                    ty,
                    &empty_domain(count),
                    &child,
                    &[],
                    &[],
                    source,
                    depth + 1,
                    b,
                )
            }
            Type::Closure(id) => {
                b.spend(self.module.closures.len())?;
                let mut rows = self.module.closures.iter().filter(|row| row.id == *id);
                let closure = rows
                    .next()
                    .ok_or_else(|| bad("Task nested captured closure is absent"))?;
                if rows.next().is_some() {
                    return Err(bad("Task nested captured closure identity is duplicated"));
                }
                let shape = closure
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| bad("Task nested captured closure full domain is Unknown"))?;
                signature_life::charge(shape, b)?;
                let child = header_map(header, base, map, semantic, shape.fixed_binder_count, b)?;
                self.node(
                    ty,
                    &empty_domain(shape.fixed_binder_count),
                    &child,
                    &[],
                    &[],
                    source,
                    depth + 1,
                    b,
                )
            }
            Type::Managed(inner) => {
                if opaque(inner) {
                    b.spend(semantic.len().saturating_add(1))?;
                    let mut sem = semantic.to_vec();
                    sem.push(LifeTypeProjection::Pointee);
                    self.header_node(
                        inner,
                        header,
                        base,
                        map,
                        &sem,
                        contract,
                        source,
                        depth + 1,
                        b,
                    )
                } else {
                    Ok(())
                }
            }
            Type::Tuple(fields) => {
                for (index, field) in fields.iter().enumerate() {
                    self.header_child(
                        field,
                        header,
                        base,
                        map,
                        semantic,
                        contract,
                        source,
                        LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),
                        PlaceProjection::Field(index as u32),
                        depth,
                        b,
                    )?;
                }
                Ok(())
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, field)) in fields.iter().enumerate() {
                    self.header_child(
                        field,
                        header,
                        base,
                        map,
                        semantic,
                        contract,
                        source,
                        LifeTypeProjection::Place(PlaceProjection::Field(index as u32)),
                        PlaceProjection::Field(index as u32),
                        depth,
                        b,
                    )?;
                }
                Ok(())
            }
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, field) in fields.iter().enumerate() {
                        let projection = PlaceProjection::Variant(VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        });
                        self.header_child(
                            field,
                            header,
                            base,
                            map,
                            semantic,
                            contract,
                            source,
                            LifeTypeProjection::Place(projection),
                            projection,
                            depth,
                            b,
                        )?;
                    }
                }
                Ok(())
            }
            Type::ErrorUnion { error, result } => {
                for (arm, field) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let projection = PlaceProjection::ErrorArm(arm);
                    self.header_child(
                        field,
                        header,
                        base,
                        map,
                        semantic,
                        contract,
                        source,
                        LifeTypeProjection::Place(projection),
                        projection,
                        depth,
                        b,
                    )?;
                }
                Ok(())
            }
            Type::Array { element, length } => {
                if *length as usize > b.limits.max_nodes {
                    return Err(limit("Task captured array port domain exceeded"));
                }
                b.spend(*length as usize)?;
                for index in 0..*length {
                    self.header_child(
                        element,
                        header,
                        base,
                        map,
                        semantic,
                        contract,
                        source,
                        LifeTypeProjection::Element,
                        PlaceProjection::Field(index),
                        depth,
                        b,
                    )?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn header_child(
        &mut self,
        ty: &Type,
        header: &StorageLifeContract,
        base: StorageBase,
        map: &[u32],
        semantic: &[LifeTypeProjection],
        contract: &[ContractProjection],
        source: &[ContractProjection],
        semantic_edge: LifeTypeProjection,
        physical_edge: PlaceProjection,
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(
            semantic
                .len()
                .saturating_add(contract.len())
                .saturating_add(source.len())
                .saturating_add(3),
        )?;
        let mut sem = semantic.to_vec();
        sem.push(semantic_edge);
        let mut con = contract.to_vec();
        con.push(ContractProjection::Place(physical_edge));
        let mut src = source.to_vec();
        src.push(ContractProjection::Place(physical_edge));
        self.header_node(ty, header, base, map, &sem, &con, &src, depth + 1, b)
    }
    fn depth(&self, depth: usize, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            Err(limit("Task port occurrence nesting exceeded"))
        } else {
            Ok(())
        }
    }
}
fn empty_domain(count: u32) -> StorageLifeShape {
    StorageLifeShape {
        binder_count: count,
        slots: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}
fn opaque(mut ty: &Type) -> bool {
    while let Type::BorrowedStorage { storage, .. } = ty {
        ty = storage;
    }
    matches!(
        ty,
        Type::Task { .. }
            | Type::MutexGuard { .. }
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
            | Type::Closure(_)
    )
}
fn boundary(
    life: &StorageLifeShape,
    map: &[u32],
    path: &[LifeTypeProjection],
    count: u32,
    b: &mut Budget,
) -> Check<Vec<u32>> {
    if count as usize > b.limits.max_nodes {
        return Err(limit("Task complete child port domain exceeded"));
    }
    b.spend(count as usize)?;
    if path.is_empty() {
        if life.binder_count != count || map.len() != count as usize {
            return Err(bad("Task port root changes its full intrinsic domain"));
        }
        return Ok(map.to_vec());
    }
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for class in 0..count {
        b.spend(life.retained.len().saturating_add(path.len()))?;
        let mut rows = life
            .retained
            .iter()
            .filter(|row| row.path == path && row.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("Task port child lacks a complete intrinsic occurrence map"))?;
        if rows.next().is_some() || !seen.insert(row.binder) {
            return Err(bad(
                "Task port child map duplicates or coalesces intrinsic classes",
            ));
        }
        result.push(
            *map.get(row.binder as usize)
                .ok_or_else(|| bad("Task port child maps outside its original complete domain"))?,
        );
    }
    Ok(result)
}
fn header_map(
    header: &StorageLifeContract,
    base: StorageBase,
    map: &[u32],
    path: &[LifeTypeProjection],
    count: u32,
    b: &mut Budget,
) -> Check<Vec<u32>> {
    if count as usize > b.limits.max_nodes {
        return Err(limit("Task captured child domain exceeded"));
    }
    b.spend(count as usize)?;
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for class in 0..count {
        b.spend(header.classes.len().saturating_add(path.len()))?;
        let mut rows = header.classes.iter().filter(|row| {
            row.position.base == base && row.position.path == path && row.position.class == class
        });
        let row = rows
            .next()
            .ok_or_else(|| bad("Task captured child lacks its full header class map"))?;
        let StorageBinder::Fixed(binder) = row.binder else {
            return Err(bad("Task captured child uses an invocation class"));
        };
        if rows.next().is_some() || !seen.insert(binder) {
            return Err(bad("Task captured child map coalesces intrinsic classes"));
        }
        result.push(
            *map.get(binder as usize)
                .ok_or_else(|| bad("Task captured child escapes its complete persistent scope"))?,
        );
    }
    Ok(result)
}
