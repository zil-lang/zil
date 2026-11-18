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
impl<'a,'m> Walk<'a,'m> {
    fn raw_child(&mut self, ty: &'m Type, semantic: &[LifeTypeProjection], b: &mut Budget) -> Check<()> {
        // A raw pointee's slots are local to that pointee. Reconstruct and
        // independently validate its complete domain before mapping any of
        // its intrinsic indices into this parent's logical directory.
        let (life, domain) = self.inventory.raw_scope(ty, b)?;
        self.pending |= domain.pending_sources;
        let map = self.boundary(semantic, domain.count, b)?;
        self.retained_descendants(&life, semantic, &map, b)?;
        for child in &life.nominal_arguments {
            b.spend(semantic.len().saturating_add(child.path.len()).saturating_add(1))?;
            let mut row = child.clone();
            let mut path = semantic.to_vec(); path.extend_from_slice(&row.path); row.path = path;
            row.binder = *map.get(row.binder as usize)
                .ok_or_else(|| bad("raw pointee nominal escapes its complete local domain"))?;
            self.nominal.push(row);
        }
        Ok(())
    }
    fn retained_descendants(
        &mut self,
        child: &StorageLifeShape,
        prefix: &[LifeTypeProjection],
        map: &[u32],
        b: &mut Budget,
    ) -> Check<()> {
        for descendant in &child.retained {
            b.spend(
                self.life
                    .retained
                    .len()
                    .saturating_add(prefix.len())
                    .saturating_add(descendant.path.len()),
            )?;
            let mut path = prefix.to_vec();
            path.extend_from_slice(&descendant.path);
            let binder = *map
                .get(descendant.binder as usize)
                .ok_or_else(|| bad("pointee descendant escapes its complete local domain"))?;
            let mut rows = self
                .life
                .retained
                .iter()
                .filter(|row| row.path == path && row.class == descendant.class);
            if rows.next().map(|row| row.binder) != Some(binder) || rows.next().is_some() {
                return Err(bad(
                    "pointee descendant changes its original intrinsic class map",
                ));
            }
            self.retained.push((path, descendant.class));
        }
        Ok(())
    }
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
            if map.contains(&row.binder){return Err(bad("compound child coalesces independent intrinsic class indices"));}
            b.spend(path.len().saturating_add(1))?;
            self.retained.push((path.to_vec(), class));
            map.push(row.binder);
        }
        Ok(map)
    }
    fn node(
        &mut self,
        ty: &'m Type,
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
                // A complete wrapper value carries the pointee's declared
                // TypeArgument and other retained selectors through its whole
                // map. Physical slots stay in the embedded pointee type.
                if semantic.last() == Some(&LifeTypeProjection::Pointee) {
                    self.retained_descendants(life, semantic, &map, b)?;
                }
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
            Type::Dyn{..}|Type::OwnedDyn{..}=>{
                let domain=self.inventory.domain(ty,b)?;self.pending|=domain.pending_sources;
                let map=self.boundary(semantic,domain.count,b)?;
                let context=self.inventory.dyn_context(ty,b)?.ok_or_else(||bad("compound erased child loses its exact Context directory"))?;
                // The child retains its complete internal logical directory.
                // A parent records its whole intrinsic boundary and prefixes
                // nominal identities, without adding any physical Context GEP.
                for row in &context.nominal_arguments{
                    b.spend(semantic.len().saturating_add(row.path.len()).saturating_add(1))?;
                    let binder=*map.get(row.binder as usize).ok_or_else(||bad("opaque nominal Context row escapes its child domain"))?;
                    let mut path=semantic.clone();path.extend_from_slice(&row.path);
                    self.nominal.push(NominalLifeArgument{path,declaration:row.declaration,argument:row.argument,binder});
                }
            },
            Type::Closure(_) | Type::Task { .. } | Type::MutexGuard { .. } => {
                let domain = self.inventory.domain(ty, b)?;
                self.pending |= domain.pending_sources;
                self.boundary(semantic, domain.count, b)?;
            }
            Type::Address { mutable, inner } => {
                if !inline {
                    return self.raw_child(ty, semantic, b);
                }
                b.spend(physical.len().saturating_add(self.life.slots.len()).saturating_add(1))?;
                if self.life.slots.iter().any(|row|row.path==*physical){
                    self.slots.push((physical.clone(),if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress},None));
                }else{
                    if *mutable{return Err(bad("static compound occurrence cannot invent an exclusive address"));}
                    // No variable κ is fabricated. This is a pending immutable
                    // source occurrence, discharged by actual constructors,
                    // writes and Return provenance before module release.
                    self.pending=true;
                }
                semantic.push(LifeTypeProjection::Pointee);
                self.node(inner, physical, semantic, false, depth + 1, b)?;
                semantic.pop();
            }
            Type::Desc(inner) => {
                if !inline {
                    return self.raw_child(ty, semantic, b);
                }
                b.spend(
                    self.life
                        .slots
                        .len()
                        .saturating_add(physical.len())
                        .saturating_add(1),
                )?;
                let mut rows = self.life.slots.iter().filter(|row| row.path == *physical);
                if let Some(row)=rows.next(){
                    if rows.next().is_some()||!matches!(row.mode,StoredBorrowMode::ReadOnlyDescription|StoredBorrowMode::WritableDescription){return Err(bad("compound Description mode is not an actual weak description"));}
                    self.slots.push((physical.clone(),row.mode,None));
                }else{
                    // A constant description is read-only and still has no
                    // source certificate at this metadata layer.
                    self.pending=true;
                }
                semantic.push(LifeTypeProjection::Pointee);
                self.node(inner, physical, semantic, false, depth + 1, b)?;
                semantic.pop();
            }
            Type::Managed(inner) => {
                let map = if semantic.is_empty() {
                    None
                } else {
                    let domain = self.inventory.domain(ty, b)?;
                    self.pending |= domain.pending_sources;
                    Some(self.boundary(semantic, domain.count, b)?)
                };
                semantic.push(LifeTypeProjection::Pointee);
                self.node(inner, physical, semantic, false, depth + 1, b)?;
                if let Some(map) = map {
                    // Managed contributes no lifetime of its own. Its whole
                    // occurrence must transport exactly the intrinsic indices
                    // of the real pointee, including phantom classes.
                    for (class, binder) in map.into_iter().enumerate() {
                        b.spend(self.life.retained.len().saturating_add(semantic.len()))?;
                        let mut rows = self.life.retained.iter().filter(|row| {
                            row.path == *semantic && row.class == class as u32
                        });
                        if rows.next().map(|row| row.binder) != Some(binder)
                            || rows.next().is_some()
                        {
                            return Err(bad("owning wrapper changes its complete pointee class map"));
                        }
                    }
                }
                semantic.pop();
            }
            Type::Array{element,length:1}if matches!(element.as_ref(),Type::Closure(_))=>{
                // The private callback cell has one complete semantic child,
                // not a flattened capture domain or an additional κ.
                physical.push(PlaceProjection::Field(0));
                semantic.push(LifeTypeProjection::Element);
                self.node(element,physical,semantic,inline,depth+1,b)?;
                physical.pop();semantic.pop();
            },
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
pub(super) fn check<'m>(
    inventory: &mut values::Inventory<'m>,
    storage: &'m Type,
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
pub(super) fn external<'m>(
    inventory: &mut values::Inventory<'m>,
    ty: &'m Type,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> Check<values::Domain> {
    if !matches!(ty, Type::Address { .. } | Type::Desc(_) | Type::Managed(_) | Type::Tuple(_) | Type::Record{..} | Type::Package{..} | Type::ErrorUnion{..})
        &&!matches!(ty,Type::Array{element,length:1}if matches!(element.as_ref(),Type::Closure(_))) {
        return Err(bad(
            "external compound has no actual complete wrapper or product/sum root",
        ));
    }
    external_inner(inventory, ty, life, false, b)
}
fn external_inner<'m>(
    inventory: &mut values::Inventory<'m>,
    storage: &'m Type,
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
