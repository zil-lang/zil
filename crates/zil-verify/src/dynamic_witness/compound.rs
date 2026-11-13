//! Anonymous compound external directories. This is private to complete DTV
//! interpretation; it does not relax ordinary stored physical shape admission.
use super::*;

struct Walk<'a, 'm> {
    inventory: &'a mut values::Inventory<'m>,
    life: &'a StorageLifeShape,
    slots: Vec<(Vec<PlaceProjection>, StoredBorrowMode, Option<u32>)>,
    retained: Vec<(Vec<LifeTypeProjection>, u32)>,
    nominal: Vec<NominalLifeArgument>,
    pending: bool,
}
impl Walk<'_, '_> {
    fn boundary(
        &mut self,
        path: &[LifeTypeProjection],
        count: u32,
        b: &mut Budget,
    ) -> Check<Vec<u32>> {
        if count as usize > b.limits.max_nodes {
            return Err(limit("compound opaque class domain exceeded"));
        }
        b.spend((count as usize).saturating_add(self.life.retained.len()))?;
        let mut map = Vec::with_capacity(count as usize);
        for class in 0..count {
            let mut rows = self
                .life
                .retained
                .iter()
                .filter(|row| row.path == path && row.class == class);
            let row = rows
                .next()
                .ok_or_else(|| bad("compound omits an actual child class occurrence"))?;
            if rows.next().is_some() || row.binder >= self.life.binder_count {
                return Err(bad(
                    "compound repeats or misbinds an actual child class occurrence",
                ));
            }
            b.spend(path.len().saturating_add(1))?;
            self.retained.push((path.to_vec(), class));
            map.push(row.binder);
        }
        Ok(map)
    }
    fn node(
        &mut self,
        ty: &Type,
        physical: &mut Vec<PlaceProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        inline: bool,
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("compound actual semantic path depth exceeded"));
        }
        match ty {
            Type::BorrowedStorage { life, .. } => {
                let domain = self.inventory.domain(ty, b)?;
                self.pending |= domain.pending_sources;
                let map = self.boundary(semantic, domain.count, b)?;
                for row in &life.nominal_arguments {
                    b.spend(
                        semantic
                            .len()
                            .saturating_add(row.path.len())
                            .saturating_add(1),
                    )?;
                    let binder = *map.get(row.binder as usize).ok_or_else(|| {
                        bad("child nominal row is outside its complete intrinsic domain")
                    })?;
                    let mut path = semantic.clone();
                    path.extend_from_slice(&row.path);
                    self.nominal.push(NominalLifeArgument {
                        path,
                        declaration: row.declaration,
                        argument: row.argument,
                        binder,
                    });
                }
                if inline {
                    for row in &life.slots {
                        b.spend(
                            physical
                                .len()
                                .saturating_add(row.path.len())
                                .saturating_add(1),
                        )?;
                        let binder = *map.get(row.binder as usize).ok_or_else(|| {
                            bad("child slot is outside its complete intrinsic domain")
                        })?;
                        let mut path = physical.clone();
                        path.extend_from_slice(&row.path);
                        self.slots.push((path, row.mode, Some(binder)));
                    }
                }
            }
            Type::Closure(_) | Type::Task { .. } | Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                let domain = self.inventory.domain(ty, b)?;
                self.pending |= domain.pending_sources;
                self.boundary(semantic, domain.count, b)?;
            }
            Type::Address { mutable, inner } => {
                if !inline {
                    return Err(bad(
                        "semantic pointee raw Address has no independent complete child domain",
                    ));
                }
                b.spend(physical.len().saturating_add(1))?;
                self.slots.push((
                    physical.clone(),
                    if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    None,
                ));
                semantic.push(LifeTypeProjection::Pointee);
                self.node(inner, physical, semantic, false, depth + 1, b)?;
                semantic.pop();
            }
            Type::Desc(inner) => {
                if !inline {
                    return Err(bad(
                        "semantic pointee raw Desc has no independent complete child domain",
                    ));
                }
                b.spend(
                    self.life
                        .slots
                        .len()
                        .saturating_add(physical.len())
                        .saturating_add(1),
                )?;
                let mut rows = self.life.slots.iter().filter(|row| row.path == *physical);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("compound Description occurrence is absent"))?;
                if rows.next().is_some()
                    || !matches!(
                        row.mode,
                        StoredBorrowMode::ReadOnlyDescription
                            | StoredBorrowMode::WritableDescription
                    )
                {
                    return Err(bad(
                        "compound Description mode is not an actual weak description",
                    ));
                }
                self.slots.push((physical.clone(), row.mode, None));
                semantic.push(LifeTypeProjection::Pointee);
                self.node(inner, physical, semantic, false, depth + 1, b)?;
                semantic.pop();
            }
            Type::Managed(inner) => {
                semantic.push(LifeTypeProjection::Pointee);
                self.node(inner, physical, semantic, false, depth + 1, b)?;
                semantic.pop();
            }
            Type::Tuple(fields) => {
                for (i, child) in fields.iter().enumerate() {
                    let field = PlaceProjection::Field(i as u32);
                    physical.push(field);
                    semantic.push(LifeTypeProjection::Place(field));
                    self.node(child, physical, semantic, inline, depth + 1, b)?;
                    physical.pop();
                    semantic.pop();
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (i, (_, child)) in fields.iter().enumerate() {
                    let field = PlaceProjection::Field(i as u32);
                    physical.push(field);
                    semantic.push(LifeTypeProjection::Place(field));
                    self.node(child, physical, semantic, inline, depth + 1, b)?;
                    physical.pop();
                    semantic.pop();
                }
            }
            Type::ErrorUnion { error, result } => {
                for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let step = PlaceProjection::ErrorArm(arm);
                    physical.push(step);
                    semantic.push(LifeTypeProjection::Place(step));
                    self.node(child, physical, semantic, inline, depth + 1, b)?;
                    physical.pop();
                    semantic.pop();
                }
            }
            // A container with no opaque/nonzero child remains on its actual
            // complete zero-domain inventory, including zero-length elements.
            _ => {
                let domain = self.inventory.domain(ty, b)?;
                if domain.count != 0 {
                    return Err(bad(
                        "compound actual child requires its complete semantic path consumer",
                    ));
                }
                self.pending |= domain.pending_sources;
            }
        }
        Ok(())
    }
}
pub(super) fn check(
    inventory: &mut values::Inventory<'_>,
    storage: &Type,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> Check<values::Domain> {
    if !matches!(storage, Type::Tuple(_)) {
        return Err(bad("anonymous compound has no actual Tuple root"));
    }
    external_inner(inventory, storage, life, true, b)
}
/// Ordinary wrapper paths belong to the actual constructor. Managed adds a
/// Pointee edge; it does not turn its whole payload into one opaque boundary.
pub(super) fn external(
    inventory: &mut values::Inventory<'_>,
    ty: &Type,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> Check<values::Domain> {
    if !matches!(ty, Type::Address { .. } | Type::Desc(_) | Type::Managed(_)) {
        return Err(bad(
            "external compound has no actual Address, Desc or Managed root",
        ));
    }
    external_inner(inventory, ty, life, false, b)
}
fn external_inner(
    inventory: &mut values::Inventory<'_>,
    storage: &Type,
    life: &StorageLifeShape,
    anonymous: bool,
    b: &mut Budget,
) -> Check<values::Domain> {
    if life.binder_count as usize > b.limits.max_nodes {
        return Err(limit("compound actual domain exceeded"));
    }
    b.spend(life.binder_count as usize)?;
    let mut walk = Walk {
        inventory,
        life,
        slots: vec![],
        retained: vec![],
        nominal: vec![],
        pending: false,
    };
    walk.node(storage, &mut vec![], &mut vec![], true, 0, b)?;
    if walk.slots.len() != life.slots.len()
        || walk.retained.len() != life.retained.len()
        || walk.nominal != life.nominal_arguments
    {
        return Err(bad(
            "compound complete directory differs from its actual child occurrence forest",
        ));
    }
    for ((path, mode, binder), row) in walk.slots.iter().zip(&life.slots) {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if path != &row.path || mode != &row.mode || binder.is_some_and(|b| b != row.binder) {
            return Err(bad(
                "compound actual physical slot or whole child class map differs",
            ));
        }
    }
    for ((path, class), row) in walk.retained.iter().zip(&life.retained) {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if path != &row.path || class != &row.class {
            return Err(bad(
                "compound class directory omits or reorders an actual child occurrence",
            ));
        }
    }
    let mut seen = vec![false; life.binder_count as usize];
    let mut next = 0;
    for class in life
        .nominal_arguments
        .iter()
        .map(|r| r.binder)
        .chain(life.slots.iter().map(|r| r.binder))
        .chain(life.retained.iter().map(|r| r.binder))
    {
        b.spend(1)?;
        let row = seen
            .get_mut(class as usize)
            .ok_or_else(|| bad("compound class lies outside its intrinsic domain"))?;
        if !*row {
            if anonymous && class != next {
                return Err(bad(
                    "compound intrinsic classes are not dense by complete actual occurrence",
                ));
            }
            *row = true;
            next += 1;
        }
    }
    if seen.contains(&false) || anonymous && life.binder_count == 0 {
        return Err(bad(
            "compound invents an ungrounded class or a qualified zero-domain anonymous root",
        ));
    }
    Ok(values::Domain {
        count: life.binder_count,
        pending_sources: walk.pending,
    })
}
