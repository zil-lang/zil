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
                // Pointee is semantic metadata. Logical Task ports stay on
                // their actual carrier; no Field dereference is invented.
                let domain = values::intrinsic(self.module, inner, b)?;
                if domain.count != 0 && !matches!(inner.as_ref(), Type::Task { .. }) {
                    return Err(bad(
                        "owning Self payload needs its complete logical retained source inventory",
                    ));
                }
                let mut prefix = semantic.clone();
                prefix.push(LifeTypeProjection::Pointee);
                let mut classes = inner_classes(self.module, parent, inner, &prefix, b)?;
                let mut task_terms = Vec::with_capacity(classes.len());
                for class in &classes {
                    b.spend(parent.life.retained.len())?;
                    let row = parent
                        .life
                        .retained
                        .iter()
                        .find(|row| row.path == prefix && row.class == class.class)
                        .ok_or_else(|| bad("actual Managed payload class occurrence is absent"))?;
                    task_terms.push(parent.term(row.binder, b)?);
                }
                for class in &mut classes {
                    class.path = vec![LifeTypeProjection::Pointee];
                }
                dynamic_metadata::ty(ty, b)?;
                let position = selected(source, physical, b)?;
                self.leaves.push(requests::Leaf {
                    source: position.clone(),
                    kind: requests::LeafKind::OwningHead,
                    ty: ty.clone(),
                    classes,
                });
                if matches!(inner.as_ref(), Type::Task { .. }) {
                    dynamic_metadata::ty(inner, b)?;
                    let task = terms::Value {
                        raw: DynTypeValue {
                            ty: inner.as_ref().clone(),
                            life: StorageLifeShape {
                                binder_count: domain.count,
                                ..zero()
                            },
                            classes: task_terms.iter().map(|term| term.image).collect(),
                        },
                        terms: task_terms,
                    };
                    self.ports(&task, &position, b)?;
                }
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
        // A naked Task is its sealed payload, not an owner. Its real head is
        // represented by Managed<Task>; only OwnedDyn owns at this root.
        if matches!(value.raw.ty, Type::OwnedDyn { .. }) {
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

#[cfg(test)]
mod tests {
    use super::*;
    fn check_inventory(module: &TypedModule, value: &terms::Value) -> Check<Vec<requests::Leaf>> {
        let limits = Limits::default();
        let scope = DynInstantiationScope {
            fixed_binder_count: 4,
            invocation_binder_count: 0,
        };
        let raw = self_leaves(module, value, scope, &mut Budget::new(&limits));
        let bytes = zil_ir::wire::encode_module_candidate(
            module,
            &CandidatePlan::empty_for(module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = self_leaves(&decoded, value, scope, &mut Budget::new(&limits));
        assert_eq!(
            raw.as_ref().map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn repeated_opaque_children_keep_actual_capture_selectors_and_all_intrinsic_origins() {
        let (module, closure) = super::super::callables::tests::fixture(2, 0);
        let life = StorageLifeShape {
            binder_count: 4,
            slots: vec![],
            nominal_arguments: vec![],
            retained: (0..2)
                .flat_map(|field| {
                    (0..2).map(move |class| StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Place(PlaceProjection::Field(field))],
                        class,
                        binder: field * 2 + class,
                    })
                })
                .collect(),
        };
        let limits = Limits::default();
        let value = terms::Value::anchored(
            &DynTypeValue {
                ty: Type::BorrowedStorage {
                    storage: Box::new(Type::Tuple(vec![closure.raw.ty.clone(); 2])),
                    life: life.clone(),
                },
                life,
                classes: vec![StorageBinder::Fixed(0); 4],
            },
            terms::Owner::Function(FunctionId(99)),
            terms::Root::Receiver,
            &mut Budget::new(&limits),
        )
        .unwrap();
        let leaves = check_inventory(&module, &value).unwrap();
        assert_eq!(leaves.len(), 6);
        let borrowed = leaves
            .iter()
            .filter_map(|leaf| match &leaf.kind {
                requests::LeafKind::Physical {
                    outer: requests::Borrow::Variable { origin, .. },
                    ..
                } => Some((leaf.source.clone(), origin.class)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(borrowed.len(), 4);
        for (index, (position, origin)) in borrowed.iter().enumerate() {
            assert_eq!(*origin, index as u32);
            assert_eq!(
                position.path,
                vec![
                    ContractProjection::Place(PlaceProjection::Field(index as u32 / 2)),
                    ContractProjection::Capture(index as u32 % 2)
                ]
            );
        }
        let mut omitted = value.clone();
        omitted.raw.life.retained.pop();
        if let Type::BorrowedStorage { life, .. } = &mut omitted.raw.ty {
            *life = omitted.raw.life.clone();
        }
        assert!(check_inventory(&module, &omitted).is_err());
    }
    #[test]
    fn static_capture_stays_pending_without_a_fake_fixed_class_and_owner_has_no_borrow_mode() {
        let (mut module, _) = super::super::callables::tests::fixture(1, 0);
        module.closures[0].result = Type::I32;
        module.functions[0].return_type = Type::I32;
        module.functions[0].values = vec![Type::I32];
        module.functions[0].blocks[0].instructions = vec![Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::Constant(Constant::I32(7)),
            span: module.functions[0].span,
        }];
        let relation = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            static_bindings: vec![StoragePosition {
                base: StorageBase::Capture(0),
                path: vec![],
            }],
            classes: vec![],
        };
        module.closures[0].storage_life = Some(relation.clone());
        module.functions[0].storage_life = Some(relation);
        let limits = Limits::default();
        let value = terms::Value::anchored(
            &DynTypeValue {
                ty: Type::Closure(ClosureId(0)),
                life: zero(),
                classes: vec![],
            },
            terms::Owner::Function(FunctionId(99)),
            terms::Root::Receiver,
            &mut Budget::new(&limits),
        )
        .unwrap();
        let leaves = check_inventory(&module, &value).unwrap();
        assert_eq!(leaves.len(), 2);
        assert!(matches!(leaves[0].kind, requests::LeafKind::OwningHead));
        assert!(matches!(
            leaves[1].kind,
            requests::LeafKind::Physical {
                outer: requests::Borrow::Static,
                mode: StoredBorrowMode::SharedAddress
            }
        ));
        assert!(leaves[1].classes.is_empty());
        assert_eq!(leaves[1].source.path, vec![ContractProjection::Capture(0)]);
        module.closures[0].storage_life = None;
        module.functions[0].storage_life = None;
        assert!(check_inventory(&module, &value).is_err());
        let value = terms::Value::anchored(
            &DynTypeValue {
                ty: Type::Managed(Box::new(Type::ByteBuffer)),
                life: zero(),
                classes: vec![],
            },
            terms::Owner::Function(FunctionId(99)),
            terms::Root::Receiver,
            &mut Budget::new(&limits),
        )
        .unwrap();
        let leaves = check_inventory(&module, &value).unwrap();
        assert_eq!(leaves.len(), 1);
        assert!(matches!(leaves[0].kind, requests::LeafKind::OwningHead));
        assert_eq!(leaves[0].ty, value.raw.ty);
    }
}
