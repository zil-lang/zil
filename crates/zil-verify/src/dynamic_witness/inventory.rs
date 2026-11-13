//! Actual Self leaf inventories. Logical captures/ports remain selectors in
//! their own namespaces; class metadata never becomes a physical GEP or loan.
use super::*;

fn zero() -> StorageLifeShape {
    StorageLifeShape {
        binder_count: 0,
        slots: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}
fn field<'a>(ty: &'a Type, path: &[PlaceProjection], b: &mut Budget) -> Check<&'a Type> {
    dynamic_ports::physical_path(ty, path, b)
}
fn selected(
    source: &StoragePosition,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<StoragePosition> {
    b.spend(
        source
            .path
            .len()
            .saturating_add(path.len())
            .saturating_add(1),
    )?;
    let mut result = source.clone();
    result
        .path
        .extend(path.iter().copied().map(ContractProjection::Place));
    Ok(result)
}
fn inner_classes(
    module: &TypedModule,
    parent: &terms::Value,
    inner: &Type,
    prefix: &[LifeTypeProjection],
    b: &mut Budget,
) -> Check<Vec<requests::LeafClass>> {
    let count = values::intrinsic(module, inner, b)?.count;
    b.spend(count as usize)?;
    let mut classes = Vec::with_capacity(count as usize);
    for class in 0..count {
        b.spend(parent.life.retained.len())?;
        let mut rows = parent
            .life
            .retained
            .iter()
            .filter(|r| r.path == prefix && r.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("Self leaf loses a complete inner class occurrence"))?;
        if rows.next().is_some() {
            return Err(bad("Self leaf repeats an inner class occurrence"));
        }
        let term = parent.term(row.binder, b)?;
        b.spend(prefix.len())?;
        classes.push(requests::LeafClass {
            path: prefix.to_vec(),
            class,
            binder: term.image,
            origin: headers::symbol(term),
        });
    }
    Ok(classes)
}
struct Walk<'a> {
    module: &'a TypedModule,
    scope: DynInstantiationScope,
    leaves: Vec<requests::Leaf>,
    active: Vec<Type>,
}
impl Walk<'_> {
    fn value(
        &mut self,
        value: &terms::Value,
        source: StoragePosition,
        b: &mut Budget,
    ) -> Check<()> {
        values::check(self.module, &value.raw, self.scope, b)?;
        b.spend(self.active.len().saturating_add(1))?;
        if self.active.len() >= b.limits.max_call_depth.min(128) {
            return Err(limit("Self inventory semantic depth exceeded"));
        }
        if self.active.contains(&value.raw.ty) {
            return Err(bad("Self inventory has a recursive opaque definition"));
        }
        self.active.push(value.raw.ty.clone());
        let result = match &value.raw.ty {
            Type::Closure(id) => self.closure(value, *id, &source, b),
            Type::Task { .. } | Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                self.ports(value, &source, b)
            }
            _ => self.ordinary(value, &source, b),
        };
        self.active.pop();
        result
    }
    fn ordinary(
        &mut self,
        value: &terms::Value,
        source: &StoragePosition,
        b: &mut Budget,
    ) -> Check<()> {
        self.boundaries(value, &value.raw.ty, source, &mut vec![], &mut vec![], b, 0)
    }
    fn boundaries(
        &mut self,
        parent: &terms::Value,
        ty: &Type,
        source: &StoragePosition,
        physical: &mut Vec<PlaceProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        b.spend(
            physical
                .len()
                .saturating_add(semantic.len())
                .saturating_add(1),
        )?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("Self inline inventory depth exceeded"));
        }
        match ty {
            Type::BorrowedStorage { storage, life } => {
                if physical.is_empty() && semantic.is_empty() {
                    self.boundaries(parent, storage, source, physical, semantic, b, depth + 1)?;
                } else {
                    let classes = inner_classes(self.module, parent, ty, semantic, b)?;
                    let terms = (0..life.binder_count)
                        .map(|class| {
                            let row = parent
                                .life
                                .retained
                                .iter()
                                .find(|row| row.path == *semantic && row.class == class)
                                .ok_or_else(|| {
                                    bad("Self qualified child class mapping is absent")
                                })?;
                            parent.term(row.binder, b)
                        })
                        .collect::<Check<Vec<_>>>()?;
                    dynamic_metadata::ty(ty, b)?;
                    let child = terms::Value {
                        raw: DynTypeValue {
                            ty: ty.clone(),
                            life: life.clone(),
                            classes: classes.iter().map(|class| class.binder).collect(),
                        },
                        terms,
                    };
                    self.value(&child, selected(source, physical, b)?, b)?;
                }
            }
            Type::Address { inner, .. } | Type::Desc(inner) => {
                b.spend(parent.life.slots.len())?;
                let mut rows = parent.life.slots.iter().filter(|row| row.path == *physical);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("Self actual borrowed position is absent"))?;
                if rows.next().is_some() {
                    return Err(bad("Self actual borrowed position is duplicated"));
                }
                if !matches!(
                    (ty, row.mode),
                    (
                        Type::Address { mutable: false, .. },
                        StoredBorrowMode::SharedAddress
                    ) | (
                        Type::Address { mutable: true, .. },
                        StoredBorrowMode::ExclusiveAddress
                    ) | (
                        Type::Desc(_),
                        StoredBorrowMode::ReadOnlyDescription
                            | StoredBorrowMode::WritableDescription
                    )
                ) {
                    return Err(bad(
                        "Self actual leaf mode differs from its full typed source",
                    ));
                }
                let term = parent.term(row.binder, b)?;
                let mut prefix = semantic.clone();
                prefix.push(LifeTypeProjection::Pointee);
                let mut classes = inner_classes(self.module, parent, inner, &prefix, b)?;
                for class in &mut classes {
                    class.path = vec![LifeTypeProjection::Pointee];
                }
                dynamic_metadata::ty(ty, b)?;
                self.leaves.push(requests::Leaf {
                    source: selected(source, physical, b)?,
                    kind: requests::LeafKind::Physical {
                        outer: requests::Borrow::Variable {
                            binder: term.image,
                            origin: headers::symbol(term),
                        },
                        mode: row.mode,
                    },
                    ty: ty.clone(),
                    classes,
                });
            }
            Type::Managed(inner) => {
                // An owning head is not a physical route through its payload.
                // Nonzero payload scopes need their separate logical source
                // inventory, never a guessed Field past Managed.
                let domain = values::intrinsic(self.module, inner, b)?;
                if domain.count != 0 {
                    return Err(bad(
                        "owning Self payload needs its complete logical retained source inventory",
                    ));
                }
                dynamic_metadata::ty(ty, b)?;
                self.leaves.push(requests::Leaf {
                    source: selected(source, physical, b)?,
                    kind: requests::LeafKind::OwningHead,
                    ty: ty.clone(),
                    classes: vec![],
                });
            }
            Type::Closure(_) | Type::Task { .. } | Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                let count = values::intrinsic(self.module, ty, b)?.count;
                let classes = inner_classes(self.module, parent, ty, semantic, b)?;
                if classes.len() != count as usize {
                    return Err(bad("Self opaque child class forest is incomplete"));
                }
                let life = zero();
                let value = terms::Value {
                    raw: DynTypeValue {
                        ty: ty.clone(),
                        life: StorageLifeShape {
                            binder_count: count,
                            ..life
                        },
                        classes: classes.iter().map(|c| c.binder).collect(),
                    },
                    terms: classes
                        .iter()
                        .map(|c| {
                            parent
                                .life
                                .retained
                                .iter()
                                .find(|r| r.path == *semantic && r.class == c.class)
                                .ok_or_else(|| bad("Self opaque child class mapping disappeared"))
                                .and_then(|r| parent.term(r.binder, b))
                        })
                        .collect::<Check<Vec<_>>>()?,
                };
                self.value(&value, selected(source, physical, b)?, b)?;
            }
            Type::Tuple(fields) => {
                for (index, child) in fields.iter().enumerate() {
                    self.child(
                        parent,
                        child,
                        source,
                        PlaceProjection::Field(index as u32),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, child)) in fields.iter().enumerate() {
                    self.child(
                        parent,
                        child,
                        source,
                        PlaceProjection::Field(index as u32),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, child) in fields.iter().enumerate() {
                        self.child(
                            parent,
                            child,
                            source,
                            PlaceProjection::Variant(VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            }),
                            physical,
                            semantic,
                            b,
                            depth,
                        )?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                self.child(
                    parent,
                    error,
                    source,
                    PlaceProjection::ErrorArm(true),
                    physical,
                    semantic,
                    b,
                    depth,
                )?;
                self.child(
                    parent,
                    result,
                    source,
                    PlaceProjection::ErrorArm(false),
                    physical,
                    semantic,
                    b,
                    depth,
                )?;
            }
            Type::Array { element, length } => {
                if *length as usize > b.limits.max_nodes {
                    return Err(limit("Self array inventory exceeds selected bound"));
                }
                b.spend(*length as usize)?;
                for index in 0..*length {
                    self.child(
                        parent,
                        element,
                        source,
                        PlaceProjection::Field(index),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            _ => {
                values::closed(self.module, ty, b)?;
            }
        }
        Ok(())
    }
    fn child(
        &mut self,
        parent: &terms::Value,
        ty: &Type,
        source: &StoragePosition,
        step: PlaceProjection,
        physical: &mut Vec<PlaceProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        physical.push(step);
        semantic.push(LifeTypeProjection::Place(step));
        self.boundaries(parent, ty, source, physical, semantic, b, depth + 1)?;
        physical.pop();
        semantic.pop();
        Ok(())
    }
    fn closure(
        &mut self,
        value: &terms::Value,
        id: ClosureId,
        source: &StoragePosition,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(self.module.closures.len())?;
        let definition = self
            .module
            .closures
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| bad("Self closure definition is absent"))?;
        let header = definition
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("Self closure capture scope is Unknown"))?;
        dynamic_metadata::ty(&value.raw.ty, b)?;
        self.leaves.push(requests::Leaf {
            source: source.clone(),
            kind: requests::LeafKind::OwningHead,
            ty: value.raw.ty.clone(),
            classes: vec![],
        });
        for (index, ty) in definition.captures.iter().enumerate() {
            let base = StorageBase::Capture(index as u32);
            b.spend(source.path.len().saturating_add(1))?;
            let mut position = source.clone();
            position
                .path
                .push(ContractProjection::Capture(index as u32));
            self.capture(
                value,
                ty,
                header,
                base,
                &mut vec![],
                &mut vec![],
                position,
                b,
                0,
            )?;
        }
        Ok(())
    }
    fn capture(
        &mut self,
        owner: &terms::Value,
        ty: &Type,
        header: &StorageLifeContract,
        base: StorageBase,
        physical: &mut Vec<PlaceProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        source: StoragePosition,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        b.spend(
            physical
                .len()
                .saturating_add(semantic.len())
                .saturating_add(1),
        )?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("Self capture inventory depth exceeded"));
        }
        match ty {
            Type::Address { inner, .. } | Type::Desc(inner) => {
                let mode = match ty {
                    Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                    Type::Address { .. } => StoredBorrowMode::SharedAddress,
                    _ => StoredBorrowMode::ReadOnlyDescription,
                };
                let position = StoragePosition {
                    base,
                    path: physical
                        .iter()
                        .copied()
                        .map(ContractProjection::Place)
                        .collect(),
                };
                let outer = if header.static_bindings.contains(&position) {
                    requests::Borrow::Static
                } else {
                    b.spend(header.bindings.len())?;
                    let mut rows = header.bindings.iter().filter(|r| r.position == position);
                    let row = rows.next().ok_or_else(|| {
                        bad("Self capture borrow has no actual header occurrence")
                    })?;
                    if rows.next().is_some() {
                        return Err(bad("Self capture borrow header occurrence is duplicated"));
                    }
                    let StorageBinder::Fixed(class) = row.binder else {
                        return Err(bad("Self capture cannot retain a fresh invocation binder"));
                    };
                    let term = owner.term(class, b)?;
                    requests::Borrow::Variable {
                        binder: term.image,
                        origin: headers::symbol(term),
                    }
                };
                let mut path = semantic.clone();
                path.push(LifeTypeProjection::Pointee);
                let classes = self.capture_classes(owner, inner, header, base, &path, b)?;
                dynamic_metadata::ty(ty, b)?;
                self.leaves.push(requests::Leaf {
                    source,
                    kind: requests::LeafKind::Physical { outer, mode },
                    ty: ty.clone(),
                    classes,
                });
            }
            _ => {
                let domain = values::intrinsic(self.module, ty, b)?;
                let classes = self.capture_classes(owner, ty, header, base, semantic, b)?;
                let life = match ty {
                    Type::BorrowedStorage { life, .. } => life.clone(),
                    Type::Closure(_)
                    | Type::Task { .. }
                    | Type::Dyn { .. }
                    | Type::OwnedDyn { .. } => StorageLifeShape {
                        binder_count: domain.count,
                        ..zero()
                    },
                    _ if domain.count == 0 => zero(),
                    _ => {
                        return Err(bad(
                            "Self capture ordinary wrapper needs its full actual compound shape",
                        ));
                    }
                };
                let terms = classes.iter().map(|c| {
                    let row = header.classes.iter().find(|r| r.position.base == base && r.position.path == *semantic && r.position.class == c.class)
                        .ok_or_else(|| bad("Self capture child loses its original complete class occurrence"))?;
                    let StorageBinder::Fixed(index) = row.binder else { return Err(bad("Self capture child contains an invocation class")); };
                    owner.term(index, b)
                }).collect::<Check<Vec<_>>>()?;
                let child = terms::Value {
                    raw: DynTypeValue {
                        ty: ty.clone(),
                        life,
                        classes: classes.iter().map(|c| c.binder).collect(),
                    },
                    terms,
                };
                self.value(&child, source, b)?;
            }
        }
        Ok(())
    }
    fn capture_classes(
        &self,
        owner: &terms::Value,
        ty: &Type,
        header: &StorageLifeContract,
        base: StorageBase,
        path: &[LifeTypeProjection],
        b: &mut Budget,
    ) -> Check<Vec<requests::LeafClass>> {
        let count = values::intrinsic(self.module, ty, b)?.count;
        b.spend(count as usize)?;
        let mut classes = vec![];
        for class in 0..count {
            b.spend(header.classes.len().saturating_add(path.len()))?;
            let mut rows = header.classes.iter().filter(|r| {
                r.position.base == base && r.position.path == path && r.position.class == class
            });
            let row = rows
                .next()
                .ok_or_else(|| bad("Self capture lacks a complete child class occurrence"))?;
            if rows.next().is_some() {
                return Err(bad("Self capture duplicates a child class occurrence"));
            }
            let StorageBinder::Fixed(index) = row.binder else {
                return Err(bad("Self capture class cannot be invocation-local"));
            };
            let term = owner.term(index, b)?;
            classes.push(requests::LeafClass {
                path: path.to_vec(),
                class,
                binder: term.image,
                origin: headers::symbol(term),
            });
        }
        Ok(classes)
    }
    fn ports(
        &mut self,
        value: &terms::Value,
        source: &StoragePosition,
        b: &mut Budget,
    ) -> Check<()> {
        let retained = match &value.raw.ty {
            Type::Task { retained_life, .. } => retained_life,
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                &dynamic_ports::interface(self.module, *interface, b)?.retained_life
            }
            _ => unreachable!(),
        };
        if matches!(value.raw.ty, Type::Task { .. } | Type::OwnedDyn { .. }) {
            dynamic_metadata::ty(&value.raw.ty, b)?;
            self.leaves.push(requests::Leaf {
                source: source.clone(),
                kind: requests::LeafKind::OwningHead,
                ty: value.raw.ty.clone(),
                classes: vec![],
            });
        }
        for (port, row) in retained.ports.iter().enumerate() {
            dynamic_metadata::ty(&row.leaf, b)?;
            let inner = match &row.leaf {
                Type::Address { inner, .. } | Type::Desc(inner) => inner,
                _ => return Err(bad("opaque Self port is not an actual typed borrow")),
            };
            if values::intrinsic(self.module, inner, b)?.count != 0 {
                return Err(bad(
                    "opaque Self port requires the complete actual inner occurrence map",
                ));
            }
            let term = value.term(row.binder, b)?;
            b.spend(source.path.len().saturating_add(1))?;
            let mut position = source.clone();
            position
                .path
                .push(ContractProjection::RetainedPort(port as u32));
            dynamic_metadata::recipe(&value.raw, b)?;
            self.leaves.push(requests::Leaf {
                source: position,
                kind: requests::LeafKind::RetainedPort {
                    outer: requests::Borrow::Variable {
                        binder: term.image,
                        origin: headers::symbol(term),
                    },
                    mode: row.mode,
                    carrier: value.raw.clone(),
                    port: port as u32,
                },
                ty: row.leaf.clone(),
                classes: vec![],
            });
        }
        Ok(())
    }
}
pub(super) fn self_leaves(
    module: &TypedModule,
    value: &terms::Value,
    scope: DynInstantiationScope,
    b: &mut Budget,
) -> Check<Vec<requests::Leaf>> {
    let mut walk = Walk {
        module,
        scope,
        leaves: vec![],
        active: vec![],
    };
    walk.value(
        value,
        StoragePosition {
            base: StorageBase::Result,
            path: vec![],
        },
        b,
    )?;
    Ok(walk.leaves)
}
